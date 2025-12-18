use crate::types::ProgramId;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct JobId(pub [u8; 32]);

impl JobId {
    pub fn new(request_id: &str, program_id: &ProgramId, nonce: u64) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(request_id.as_bytes());
        hasher.update(&program_id.0);
        hasher.update(&nonce.to_le_bytes());
        let hash = hasher.finalize();
        Self(*hash.as_bytes())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailureReason {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub request_id: String,
    pub program_id: ProgramId,
    pub status: JobStatus,
    pub input_blob_id: Option<crate::types::BlobId>,
    pub output_blob_id: Option<crate::types::BlobId>,
    pub error_message: Option<String>,
    pub fuel_consumed: u64,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub retry_count: u32,
    pub max_retries: u32,
    #[serde(default)]
    pub ttl_ms: Option<u64>,
    #[serde(default)]
    pub failure_reason: Option<FailureReason>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(default)]
    pub logs: Vec<LogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: u64,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl Job {
    pub fn new(
        request_id: String,
        program_id: ProgramId,
        input_blob_id: Option<crate::types::BlobId>,
        max_retries: u32,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let nonce = now;
        let id = JobId::new(&request_id, &program_id, nonce);

        Self {
            id,
            request_id,
            program_id,
            status: JobStatus::Pending,
            input_blob_id,
            output_blob_id: None,
            error_message: None,
            fuel_consumed: 0,
            created_at: now,
            started_at: None,
            completed_at: None,
            retry_count: 0,
            max_retries,
            ttl_ms: None,
            failure_reason: None,
            metadata: HashMap::new(),
            logs: Vec::new(),
        }
    }

    pub fn with_ttl(mut self, ttl_ms: u64) -> Self {
        self.ttl_ms = Some(ttl_ms);
        self
    }

    pub fn is_expired(&self, now_ms: u64) -> bool {
        if let Some(ttl) = self.ttl_ms {
            let age_ms = now_ms.saturating_sub(self.created_at);
            age_ms > ttl
        } else {
            false
        }
    }

    pub fn can_retry(&self) -> bool {
        matches!(self.status, JobStatus::Failed | JobStatus::TimedOut)
            && self.retry_count < self.max_retries
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Completed
                | JobStatus::Cancelled
                | JobStatus::Failed
                | JobStatus::TimedOut
                | JobStatus::Expired
        ) && !self.can_retry()
    }

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        self.logs.push(LogEntry {
            timestamp,
            level,
            message,
        });
    }

    pub fn duration_ms(&self) -> Option<u64> {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => Some(end.saturating_sub(start)),
            _ => None,
        }
    }

    pub fn set_failure(&mut self, reason: FailureReason, message: String) {
        self.status = JobStatus::Failed;
        self.failure_reason = Some(reason);
        self.error_message = Some(message.clone());
        self.add_log(LogLevel::Error, message);
    }
}

pub struct JobStore {
    db: sled::Db,
}

impl JobStore {
    pub fn new(db: sled::Db) -> Result<Self> {
        Ok(Self { db })
    }

    pub fn insert(&self, job: &Job) -> Result<()> {
        let tree = self.db.open_tree("jobs")?;
        let encoded = bincode::serde::encode_to_vec(job, bincode::config::standard())?;
        tree.insert(job.id.0, encoded)?;
        tree.flush()?;

        let idx_tree = self.db.open_tree("jobs_by_request_id")?;
        idx_tree.insert(job.request_id.as_bytes(), &job.id.0[..])?;
        idx_tree.flush()?;

        Ok(())
    }

    pub fn get(&self, id: &JobId) -> Result<Option<Job>> {
        let tree = self.db.open_tree("jobs")?;
        let Some(raw) = tree.get(id.0)? else {
            return Ok(None);
        };
        let (job, _): (Job, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(job))
    }

    pub fn get_by_request_id(&self, request_id: &str) -> Result<Option<Job>> {
        let idx_tree = self.db.open_tree("jobs_by_request_id")?;
        let Some(job_id_bytes) = idx_tree.get(request_id.as_bytes())? else {
            return Ok(None);
        };
        let mut job_id = [0u8; 32];
        job_id.copy_from_slice(&job_id_bytes);
        self.get(&JobId(job_id))
    }

    pub fn update(&self, job: &Job) -> Result<()> {
        self.insert(job)
    }

    pub fn list_pending(&self) -> Result<Vec<Job>> {
        let tree = self.db.open_tree("jobs")?;
        let mut jobs = Vec::new();
        for entry in tree.iter() {
            let (_, v) = entry?;
            match bincode::serde::decode_from_slice::<Job, _>(&v, bincode::config::standard()) {
                Ok((job, _)) => {
                    if matches!(job.status, JobStatus::Pending) {
                        jobs.push(job);
                    }
                }
                Err(e) => {
                    tracing::debug!("skipping incompatible job entry: {e:?}");
                    continue;
                }
            }
        }
        jobs.sort_by_key(|j| j.created_at);
        Ok(jobs)
    }

    pub fn list_running(&self) -> Result<Vec<Job>> {
        let tree = self.db.open_tree("jobs")?;
        let mut jobs = Vec::new();
        for entry in tree.iter() {
            let (_, v) = entry?;
            match bincode::serde::decode_from_slice::<Job, _>(&v, bincode::config::standard()) {
                Ok((job, _)) => {
                    if matches!(job.status, JobStatus::Running) {
                        jobs.push(job);
                    }
                }
                Err(e) => {
                    tracing::debug!("skipping incompatible job entry: {e:?}");
                    continue;
                }
            }
        }
        Ok(jobs)
    }

    pub fn list_all(&self) -> Result<Vec<Job>> {
        let tree = self.db.open_tree("jobs")?;
        let mut jobs = Vec::new();
        for entry in tree.iter() {
            let (_, v) = entry?;
            match bincode::serde::decode_from_slice::<Job, _>(&v, bincode::config::standard()) {
                Ok((job, _)) => {
                    jobs.push(job);
                }
                Err(e) => {
                    tracing::debug!("skipping incompatible job entry: {e:?}");
                    continue;
                }
            }
        }
        jobs.sort_by_key(|j| j.created_at);
        Ok(jobs)
    }
}
