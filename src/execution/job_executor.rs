use super::job::{JobId, JobStatus, JobStore, LogLevel};
use super::manifest::{JobManifest, ResourceRequirements};
use super::runtime::ExecutionEngine;
use super::ProgramStore;
use crate::storage::BlobStore;
use crate::types::ProgramId;
use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tokio::time::timeout;

pub struct JobExecutor {
    engine: Arc<ExecutionEngine>,
    job_store: Arc<JobStore>,
    blob_store: Arc<BlobStore>,
    manifest_store: Arc<ManifestStore>,
    active_jobs: Arc<RwLock<std::collections::HashSet<JobId>>>,
    consensus: Option<Arc<crate::consensus::DagEngine>>,
}

impl JobExecutor {
    pub fn new(
        engine: Arc<ExecutionEngine>,
        job_store: Arc<JobStore>,
        _program_store: Arc<ProgramStore>,
        blob_store: Arc<BlobStore>,
        db: sled::Db,
    ) -> Result<Self> {
        Ok(Self {
            engine,
            job_store,
            blob_store,
            manifest_store: Arc::new(ManifestStore::new(db)?),
            active_jobs: Arc::new(RwLock::new(std::collections::HashSet::new())),
            consensus: None,
        })
    }

    pub fn set_consensus(&mut self, consensus: Arc<crate::consensus::DagEngine>) {
        self.consensus = Some(consensus);
    }

    pub async fn execute_job(&self, job_id: &JobId) -> Result<()> {
        let span = tracing::info_span!("execute_job", job_id = %job_id);
        let _guard = span.enter();

        {
            let mut active = self.active_jobs.write().await;
            if active.contains(job_id) {
                return Err(anyhow!("job already running"));
            }
            active.insert(*job_id);
        }

        let result = self.execute_job_inner(job_id).await;

        {
            let mut active = self.active_jobs.write().await;
            active.remove(job_id);
        }

        result
    }

    async fn execute_job_inner(&self, job_id: &JobId) -> Result<()> {
        tracing::debug!("starting job execution");

        let mut job = self
            .job_store
            .get(job_id)?
            .ok_or_else(|| anyhow!("job not found"))?;

        if !matches!(job.status, JobStatus::Pending) {
            return Err(anyhow!("job not in pending state"));
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        job.status = JobStatus::Running;
        job.started_at = Some(now);
        job.add_log(LogLevel::Info, "Job execution started".to_string());
        self.job_store.update(&job)?;

        let manifest = self.get_or_default_manifest(&job.program_id)?;

        manifest.validate()?;

        let input_data = match &job.input_blob_id {
            Some(blob_id) => self.blob_store.get(blob_id).context("loading input blob")?,
            None => Vec::new(),
        };

        let timeout_duration = Duration::from_millis(manifest.resources.timeout_ms);

        let execution_result = timeout(
            timeout_duration,
            self.execute_with_limits(&job.program_id, &input_data, &manifest.resources),
        )
        .await;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        job.completed_at = Some(now);

        match execution_result {
            Ok(Ok(outcome)) => {
                let output_blob = self
                    .blob_store
                    .put(
                        &outcome.return_data,
                        Some("application/octet-stream".to_string()),
                        crate::types::NodeId::from_public_key(&[0u8; 32]),
                    )
                    .context("storing output blob")?;

                if let Some(ref consensus) = self.consensus {
                    let blob_data = outcome.return_data.clone();
                    let _ = consensus
                        .ingest_local_blob(output_blob.clone(), blob_data)
                        .await;
                }

                job.output_blob_id = Some(output_blob.id);
                job.fuel_consumed = outcome.fuel_consumed;
                job.status = JobStatus::Completed;
                job.add_log(
                    LogLevel::Info,
                    format!(
                        "Job completed successfully. Fuel: {}",
                        outcome.fuel_consumed
                    ),
                );
            }
            Ok(Err(e)) => {
                job.status = JobStatus::Failed;
                job.error_message = Some(e.to_string());
                job.add_log(LogLevel::Error, format!("Execution failed: {}", e));

                if job.can_retry() {
                    job.retry_count += 1;
                    job.status = JobStatus::Pending;
                    job.add_log(
                        LogLevel::Info,
                        format!(
                            "Retrying job (attempt {}/{})",
                            job.retry_count, job.max_retries
                        ),
                    );
                }
            }
            Err(_) => {
                job.status = JobStatus::TimedOut;
                job.error_message = Some(format!(
                    "Execution timed out after {}ms",
                    manifest.resources.timeout_ms
                ));
                job.add_log(LogLevel::Error, "Execution timed out".to_string());

                if job.can_retry() {
                    job.retry_count += 1;
                    job.status = JobStatus::Pending;
                    job.add_log(
                        LogLevel::Info,
                        format!(
                            "Retrying job (attempt {}/{})",
                            job.retry_count, job.max_retries
                        ),
                    );
                }
            }
        }

        self.job_store.update(&job)?;
        Ok(())
    }

    async fn execute_with_limits(
        &self,
        program_id: &ProgramId,
        input: &[u8],
        resources: &ResourceRequirements,
    ) -> Result<super::runtime::ExecutionOutcome> {
        let engine = Arc::clone(&self.engine);
        let program_id = program_id.clone();
        let input = input.to_vec();
        let max_fuel = resources.max_fuel;
        let _max_memory = resources.max_memory_bytes;

        tokio::task::spawn_blocking(move || {
            let mut config = engine.config();
            config.max_fuel = max_fuel;

            let temp_engine = ExecutionEngine::new(
                engine.blob_store(),
                engine.state_store(),
                engine.program_store(),
                config,
            )?;

            temp_engine.execute(&program_id, &input)
        })
        .await
        .map_err(|e| anyhow!("execution task join error: {e}"))?
    }

    fn get_or_default_manifest(&self, program_id: &ProgramId) -> Result<JobManifest> {
        match self.manifest_store.get(program_id)? {
            Some(manifest) => Ok(manifest),
            None => Ok(JobManifest::default()),
        }
    }

    pub async fn cancel_job(&self, job_id: &JobId) -> Result<()> {
        let mut job = self
            .job_store
            .get(job_id)?
            .ok_or_else(|| anyhow!("job not found"))?;

        if job.is_terminal() {
            return Err(anyhow!("job already in terminal state"));
        }

        job.status = JobStatus::Cancelled;
        job.add_log(LogLevel::Info, "Job cancelled by user".to_string());

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        if job.completed_at.is_none() {
            job.completed_at = Some(now);
        }

        self.job_store.update(&job)?;
        Ok(())
    }
}

pub struct ManifestStore {
    db: sled::Db,
}

impl ManifestStore {
    pub fn new(db: sled::Db) -> Result<Self> {
        Ok(Self { db })
    }

    pub fn store(&self, program_id: &ProgramId, manifest: &JobManifest) -> Result<()> {
        let tree = self.db.open_tree("job_manifests")?;
        let encoded = serde_json::to_vec(manifest)?;
        tree.insert(program_id.0, encoded)?;
        tree.flush()?;
        Ok(())
    }

    pub fn get(&self, program_id: &ProgramId) -> Result<Option<JobManifest>> {
        let tree = self.db.open_tree("job_manifests")?;
        let Some(raw) = tree.get(program_id.0)? else {
            return Ok(None);
        };
        let manifest: JobManifest = serde_json::from_slice(&raw)?;
        Ok(Some(manifest))
    }

    pub fn delete(&self, program_id: &ProgramId) -> Result<()> {
        let tree = self.db.open_tree("job_manifests")?;
        tree.remove(program_id.0)?;
        tree.flush()?;
        Ok(())
    }
}

impl JobManifest {
    fn default() -> Self {
        Self {
            version: super::manifest::ManifestVersion::V1,
            metadata: super::manifest::JobMetadata {
                name: "default".to_string(),
                description: None,
                author: None,
                version: "1.0.0".to_string(),
                tags: Vec::new(),
            },
            resources: ResourceRequirements::default(),
            runtime: super::manifest::RuntimeConfig::default(),
            io: super::manifest::IoSchema::default(),
        }
    }
}