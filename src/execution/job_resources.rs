use super::job::JobId;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResourceMetrics {
    pub job_id: JobId,
    pub cpu_usage_percent: f64,
    pub memory_used_bytes: u64,
    pub fuel_consumed: u64,
    pub storage_read_bytes: u64,
    pub storage_write_bytes: u64,
    pub execution_duration_ms: u64,
    pub peak_memory_bytes: u64,
    pub recorded_at: u64,
}

impl JobResourceMetrics {
    pub fn new(job_id: JobId) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            job_id,
            cpu_usage_percent: 0.0,
            memory_used_bytes: 0,
            fuel_consumed: 0,
            storage_read_bytes: 0,
            storage_write_bytes: 0,
            execution_duration_ms: 0,
            peak_memory_bytes: 0,
            recorded_at: now,
        }
    }
}

pub struct ResourceTracker {
    db: sled::Db,
}

impl ResourceTracker {
    pub fn new(db: sled::Db) -> Result<Self> {
        Ok(Self { db })
    }

    pub fn record_metrics(&self, metrics: &JobResourceMetrics) -> Result<()> {
        let tree = self.db.open_tree("job_resources")?;
        let key = metrics.job_id.0;
        let encoded = serde_json::to_vec(metrics)?;
        tree.insert(key, encoded)?;
        tree.flush()?;
        Ok(())
    }

    pub fn get_metrics(&self, job_id: &JobId) -> Result<Option<JobResourceMetrics>> {
        let tree = self.db.open_tree("job_resources")?;
        let Some(raw) = tree.get(job_id.0)? else {
            return Ok(None);
        };
        let metrics: JobResourceMetrics = serde_json::from_slice(&raw)?;
        Ok(Some(metrics))
    }

    pub fn list_all_metrics(&self) -> Result<Vec<JobResourceMetrics>> {
        let tree = self.db.open_tree("job_resources")?;
        let mut metrics = Vec::new();
        for entry in tree.iter() {
            let (_, v) = entry?;
            let m: JobResourceMetrics = serde_json::from_slice(&v)?;
            metrics.push(m);
        }
        Ok(metrics)
    }

    pub fn compute_aggregate_stats(&self) -> Result<AggregateResourceStats> {
        let all_metrics = self.list_all_metrics()?;

        if all_metrics.is_empty() {
            return Ok(AggregateResourceStats::default());
        }

        let total_jobs = all_metrics.len() as u64;
        let total_fuel: u64 = all_metrics.iter().map(|m| m.fuel_consumed).sum();
        let total_duration: u64 = all_metrics.iter().map(|m| m.execution_duration_ms).sum();
        let total_memory: u64 = all_metrics.iter().map(|m| m.peak_memory_bytes).sum();
        let total_storage_read: u64 = all_metrics.iter().map(|m| m.storage_read_bytes).sum();
        let total_storage_write: u64 = all_metrics.iter().map(|m| m.storage_write_bytes).sum();

        let avg_fuel = total_fuel / total_jobs;
        let avg_duration = total_duration / total_jobs;
        let avg_memory = total_memory / total_jobs;

        Ok(AggregateResourceStats {
            total_jobs,
            total_fuel_consumed: total_fuel,
            total_execution_time_ms: total_duration,
            total_storage_read_bytes: total_storage_read,
            total_storage_write_bytes: total_storage_write,
            average_fuel_per_job: avg_fuel,
            average_duration_per_job_ms: avg_duration,
            average_memory_per_job_bytes: avg_memory,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregateResourceStats {
    pub total_jobs: u64,
    pub total_fuel_consumed: u64,
    pub total_execution_time_ms: u64,
    pub total_storage_read_bytes: u64,
    pub total_storage_write_bytes: u64,
    pub average_fuel_per_job: u64,
    pub average_duration_per_job_ms: u64,
    pub average_memory_per_job_bytes: u64,
}
