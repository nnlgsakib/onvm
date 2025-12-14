use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealth {
    pub status: HealthStatus,
    pub timestamp: u64,
    pub capacity: NodeCapacity,
    pub resources: ResourceUsage,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Ready,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapacity {
    pub max_concurrent_jobs: usize,
    pub current_jobs: usize,
    pub queue_depth: usize,
    pub cpu_cores: usize,
    pub total_memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_percent: f64,
    pub active_jobs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetrics {
    pub uptime_seconds: u64,
    pub total_jobs_completed: u64,
    pub total_jobs_failed: u64,
    pub total_jobs_cancelled: u64,
    pub total_fuel_consumed: u64,
    pub average_execution_time_ms: f64,
}

pub struct HealthReporter {
    start_time: SystemTime,
    total_jobs_completed: std::sync::atomic::AtomicU64,
    total_jobs_failed: std::sync::atomic::AtomicU64,
    total_jobs_cancelled: std::sync::atomic::AtomicU64,
    total_fuel_consumed: std::sync::atomic::AtomicU64,
    execution_times: std::sync::Arc<std::sync::Mutex<Vec<u64>>>,
}

impl HealthReporter {
    pub fn new() -> Self {
        Self {
            start_time: SystemTime::now(),
            total_jobs_completed: std::sync::atomic::AtomicU64::new(0),
            total_jobs_failed: std::sync::atomic::AtomicU64::new(0),
            total_jobs_cancelled: std::sync::atomic::AtomicU64::new(0),
            total_fuel_consumed: std::sync::atomic::AtomicU64::new(0),
            execution_times: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn record_job_completed(&self, fuel: u64, duration_ms: u64) {
        self.total_jobs_completed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.total_fuel_consumed
            .fetch_add(fuel, std::sync::atomic::Ordering::Relaxed);

        let mut times = self.execution_times.lock().unwrap();
        times.push(duration_ms);
        if times.len() > 1000 {
            times.remove(0);
        }
    }

    pub fn record_job_failed(&self) {
        self.total_jobs_failed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_job_cancelled(&self) {
        self.total_jobs_cancelled
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_health(
        &self,
        current_jobs: usize,
        queue_depth: usize,
        max_concurrent: usize,
    ) -> NodeHealth {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let cpu_usage = self.get_cpu_usage();
        let memory_info = self.get_memory_info();

        let status = if current_jobs >= max_concurrent {
            HealthStatus::Degraded
        } else {
            HealthStatus::Ready
        };

        NodeHealth {
            status,
            timestamp,
            capacity: NodeCapacity {
                max_concurrent_jobs: max_concurrent,
                current_jobs,
                queue_depth,
                cpu_cores: num_cpus::get(),
                total_memory_bytes: memory_info.1,
            },
            resources: ResourceUsage {
                cpu_percent: cpu_usage,
                memory_used_bytes: memory_info.0,
                memory_percent: (memory_info.0 as f64 / memory_info.1 as f64) * 100.0,
                active_jobs: current_jobs,
            },
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn get_metrics(&self) -> NodeMetrics {
        let uptime = self
            .start_time
            .elapsed()
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        let times = self.execution_times.lock().unwrap();
        let avg_time = if times.is_empty() {
            0.0
        } else {
            times.iter().sum::<u64>() as f64 / times.len() as f64
        };

        NodeMetrics {
            uptime_seconds: uptime,
            total_jobs_completed: self
                .total_jobs_completed
                .load(std::sync::atomic::Ordering::Relaxed),
            total_jobs_failed: self
                .total_jobs_failed
                .load(std::sync::atomic::Ordering::Relaxed),
            total_jobs_cancelled: self
                .total_jobs_cancelled
                .load(std::sync::atomic::Ordering::Relaxed),
            total_fuel_consumed: self
                .total_fuel_consumed
                .load(std::sync::atomic::Ordering::Relaxed),
            average_execution_time_ms: avg_time,
        }
    }

    fn get_cpu_usage(&self) -> f64 {
        0.0
    }

    fn get_memory_info(&self) -> (u64, u64) {
        (0, 8 * 1024 * 1024 * 1024)
    }
}

impl Default for HealthReporter {
    fn default() -> Self {
        Self::new()
    }
}
