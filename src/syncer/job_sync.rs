use crate::network::{NetworkHandle, NetworkMessage};
use crate::types::ProgramId;
use crate::wasm_runtime::{FailureReason, Job, JobStore};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobBroadcast {
    pub job: JobDescriptor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobDescriptor {
    pub id: [u8; 32],
    pub request_id: String,
    pub program_id: ProgramId,
    pub input_blob_id: Option<crate::types::BlobId>,
    pub output_blob_id: Option<crate::types::BlobId>,
    pub status: JobStatusDescriptor,
    pub fuel_consumed: u64,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub error_message: Option<String>,
    pub failure_reason: Option<FailureReasonDescriptor>,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobStatusDescriptor {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailureReasonDescriptor {
    ExecutionError,
    Timeout,
    OutOfMemory,
    OutOfFuel,
    InvalidProgram,
    MissingDependency,
    NetworkError,
    InsufficientResources,
    SchedulerError,
    Unknown,
}

impl From<&Job> for JobDescriptor {
    fn from(job: &Job) -> Self {
        Self {
            id: job.id.0,
            request_id: job.request_id.clone(),
            program_id: job.program_id.clone(),
            input_blob_id: job.input_blob_id.clone(),
            output_blob_id: job.output_blob_id.clone(),
            status: match job.status {
                crate::wasm_runtime::JobStatus::Pending => JobStatusDescriptor::Pending,
                crate::wasm_runtime::JobStatus::Running => JobStatusDescriptor::Running,
                crate::wasm_runtime::JobStatus::Completed => JobStatusDescriptor::Completed,
                crate::wasm_runtime::JobStatus::Failed => JobStatusDescriptor::Failed,
                crate::wasm_runtime::JobStatus::Cancelled => JobStatusDescriptor::Cancelled,
                crate::wasm_runtime::JobStatus::TimedOut => JobStatusDescriptor::TimedOut,
                crate::wasm_runtime::JobStatus::Expired => JobStatusDescriptor::Expired,
            },
            fuel_consumed: job.fuel_consumed,
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
            error_message: job.error_message.clone(),
            failure_reason: job.failure_reason.as_ref().map(|r| match r {
                FailureReason::ExecutionError => FailureReasonDescriptor::ExecutionError,
                FailureReason::Timeout => FailureReasonDescriptor::Timeout,
                FailureReason::OutOfMemory => FailureReasonDescriptor::OutOfMemory,
                FailureReason::OutOfFuel => FailureReasonDescriptor::OutOfFuel,
                FailureReason::InvalidProgram => FailureReasonDescriptor::InvalidProgram,
                FailureReason::MissingDependency => FailureReasonDescriptor::MissingDependency,
                FailureReason::NetworkError => FailureReasonDescriptor::NetworkError,
                FailureReason::InsufficientResources => {
                    FailureReasonDescriptor::InsufficientResources
                }
                FailureReason::SchedulerError => FailureReasonDescriptor::SchedulerError,
                FailureReason::Unknown => FailureReasonDescriptor::Unknown,
            }),
            metadata: job.metadata.clone(),
        }
    }
}

pub struct JobSyncManager {
    job_store: Arc<JobStore>,
    network: NetworkHandle,
    last_broadcast_time: Arc<RwLock<std::collections::HashMap<[u8; 32], u64>>>,
}

impl JobSyncManager {
    pub fn new(job_store: Arc<JobStore>, network: NetworkHandle) -> Self {
        Self {
            job_store,
            network,
            last_broadcast_time: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    pub async fn start(self: Arc<Self>) {
        let mut tick = interval(Duration::from_secs(10));
        loop {
            tick.tick().await;
            if let Err(e) = self.broadcast_completed_jobs().await {
                tracing::warn!("job sync broadcast error: {e:?}");
            }
        }
    }

    async fn broadcast_completed_jobs(&self) -> Result<()> {
        let jobs = self.job_store.list_all()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for job in jobs {
            if !job.is_terminal() {
                continue;
            }

            let should_broadcast = {
                let last_times = self.last_broadcast_time.read().await;
                match last_times.get(&job.id.0) {
                    None => true,
                    Some(&last_time) => now - last_time > 60_000,
                }
            };

            if should_broadcast {
                self.broadcast_job(&job).await?;
                let mut last_times = self.last_broadcast_time.write().await;
                last_times.insert(job.id.0, now);
            }
        }

        Ok(())
    }

    pub async fn broadcast_job(&self, job: &Job) -> Result<()> {
        let descriptor = JobDescriptor::from(job);
        let broadcast = JobBroadcast { job: descriptor };

        let _ = self.network.publisher.send(NetworkMessage::Job(broadcast));

        Ok(())
    }

    pub async fn handle_job_broadcast(&self, broadcast: JobBroadcast) -> Result<()> {
        let job_id = crate::wasm_runtime::JobId::from_bytes(broadcast.job.id);

        if self.job_store.get(&job_id)?.is_some() {
            return Ok(());
        }

        let status = match broadcast.job.status {
            JobStatusDescriptor::Pending => crate::wasm_runtime::JobStatus::Pending,
            JobStatusDescriptor::Running => crate::wasm_runtime::JobStatus::Running,
            JobStatusDescriptor::Completed => crate::wasm_runtime::JobStatus::Completed,
            JobStatusDescriptor::Failed => crate::wasm_runtime::JobStatus::Failed,
            JobStatusDescriptor::Cancelled => crate::wasm_runtime::JobStatus::Cancelled,
            JobStatusDescriptor::TimedOut => crate::wasm_runtime::JobStatus::TimedOut,
            JobStatusDescriptor::Expired => crate::wasm_runtime::JobStatus::Expired,
        };

        let failure_reason = broadcast.job.failure_reason.map(|r| match r {
            FailureReasonDescriptor::ExecutionError => FailureReason::ExecutionError,
            FailureReasonDescriptor::Timeout => FailureReason::Timeout,
            FailureReasonDescriptor::OutOfMemory => FailureReason::OutOfMemory,
            FailureReasonDescriptor::OutOfFuel => FailureReason::OutOfFuel,
            FailureReasonDescriptor::InvalidProgram => FailureReason::InvalidProgram,
            FailureReasonDescriptor::MissingDependency => FailureReason::MissingDependency,
            FailureReasonDescriptor::NetworkError => FailureReason::NetworkError,
            FailureReasonDescriptor::InsufficientResources => FailureReason::InsufficientResources,
            FailureReasonDescriptor::SchedulerError => FailureReason::SchedulerError,
            FailureReasonDescriptor::Unknown => FailureReason::Unknown,
        });

        let job = Job {
            id: job_id,
            request_id: broadcast.job.request_id,
            program_id: broadcast.job.program_id,
            status: status.clone(),
            input_blob_id: broadcast.job.input_blob_id,
            output_blob_id: broadcast.job.output_blob_id,
            error_message: broadcast.job.error_message,
            fuel_consumed: broadcast.job.fuel_consumed,
            created_at: broadcast.job.created_at,
            started_at: broadcast.job.started_at,
            completed_at: broadcast.job.completed_at,
            retry_count: 0,
            max_retries: 3,
            ttl_ms: None,
            failure_reason,
            metadata: broadcast.job.metadata,
            logs: Vec::new(),
        };

        self.job_store.insert(&job)?;

        tracing::info!(
            "replicated job {} from network (status: {:?})",
            job_id,
            status
        );

        Ok(())
    }
}
