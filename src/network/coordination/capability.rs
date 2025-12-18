/// Capability Broadcasting Module
///
/// Manages advertising and tracking of node capabilities across the network.
/// Capabilities include:
/// - CPU cores and utilization
/// - Memory (total and available)
/// - GPU presence and specs
/// - Storage capacity
/// - Network bandwidth
/// - Special features (AI/ML optimized, high-memory, etc.)
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

/// Node capabilities that can be advertised to the network
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCapabilities {
    pub cpu_cores: u32,
    pub cpu_usage_percent: f32,
    pub memory_gb: u32,
    pub memory_available_gb: u32,
    pub has_gpu: bool,
    pub gpu_model: Option<String>,
    pub gpu_vram_gb: Option<u32>,
    pub storage_gb: u64,
    pub bandwidth_mbps: Option<u32>,
    pub specializations: Vec<Specialization>,
    pub max_concurrent_jobs: u32,
    pub current_job_count: u32,
    pub advertised_at: u64,
}

/// Node specialization tags
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Specialization {
    AiMl,
    HighMemory,
    GpuCompute,
    LowLatency,
    HighBandwidth,
    EdgeCompute,
    Custom(String),
}

impl NodeCapabilities {
    pub fn detect_system() -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::new().with_memory(MemoryRefreshKind::everything()),
        );
        sys.refresh_memory();

        let cpu_cores = num_cpus::get() as u32;
        let total_memory = sys.total_memory() / (1024 * 1024 * 1024);
        let available_memory = sys.available_memory() / (1024 * 1024 * 1024);

        let cpu_usage = Self::get_cpu_usage();

        let mut specializations = Vec::new();
        if total_memory > 64 {
            specializations.push(Specialization::HighMemory);
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            cpu_cores,
            cpu_usage_percent: cpu_usage,
            memory_gb: total_memory as u32,
            memory_available_gb: available_memory as u32,
            has_gpu: false,
            gpu_model: None,
            gpu_vram_gb: None,
            storage_gb: 100,
            bandwidth_mbps: None,
            specializations,
            max_concurrent_jobs: cpu_cores * 2,
            current_job_count: 0,
            advertised_at: now,
        }
    }

    fn get_cpu_usage() -> f32 {
        0.0
    }

    pub fn update_runtime_metrics(&mut self, current_jobs: u32) {
        let mut sys = System::new_with_specifics(
            RefreshKind::new().with_memory(MemoryRefreshKind::everything()),
        );
        sys.refresh_memory();

        self.memory_available_gb = (sys.available_memory() / (1024 * 1024 * 1024)) as u32;
        self.cpu_usage_percent = Self::get_cpu_usage();
        self.current_job_count = current_jobs;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.advertised_at = now;
    }

    pub fn is_available_for_jobs(&self) -> bool {
        self.current_job_count < self.max_concurrent_jobs
    }

    pub fn available_capacity_percent(&self) -> f32 {
        if self.max_concurrent_jobs == 0 {
            return 0.0;
        }
        let available = self
            .max_concurrent_jobs
            .saturating_sub(self.current_job_count);
        (available as f32 / self.max_concurrent_jobs as f32) * 100.0
    }

    pub fn memory_available_percent(&self) -> f32 {
        if self.memory_gb == 0 {
            return 0.0;
        }
        (self.memory_available_gb as f32 / self.memory_gb as f32) * 100.0
    }

    pub fn meets_requirements(&self, requirements: &JobRequirements) -> bool {
        if self.cpu_cores < requirements.min_cpu_cores {
            return false;
        }
        if self.memory_available_gb < requirements.min_memory_gb {
            return false;
        }
        if requirements.requires_gpu && !self.has_gpu {
            return false;
        }
        if let Some(min_vram) = requirements.min_gpu_vram_gb {
            if self.gpu_vram_gb.unwrap_or(0) < min_vram {
                return false;
            }
        }
        if !self.is_available_for_jobs() {
            return false;
        }
        true
    }
}

/// Requirements for a job to run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequirements {
    pub min_cpu_cores: u32,
    pub min_memory_gb: u32,
    pub requires_gpu: bool,
    pub min_gpu_vram_gb: Option<u32>,
    pub required_specializations: Vec<Specialization>,
}

impl Default for JobRequirements {
    fn default() -> Self {
        Self {
            min_cpu_cores: 1,
            min_memory_gb: 1,
            requires_gpu: false,
            min_gpu_vram_gb: None,
            required_specializations: Vec::new(),
        }
    }
}

/// Manages capability advertising for this node
pub struct CapabilityAdvertiser {
    capabilities: NodeCapabilities,
    broadcast_interval: Duration,
    last_broadcast: SystemTime,
}

impl CapabilityAdvertiser {
    pub fn new(capabilities: NodeCapabilities) -> Self {
        Self {
            capabilities,
            broadcast_interval: Duration::from_secs(30),
            last_broadcast: SystemTime::now() - Duration::from_secs(60),
        }
    }

    pub fn capabilities(&self) -> &NodeCapabilities {
        &self.capabilities
    }

    pub fn update_capabilities(&mut self, current_jobs: u32) {
        self.capabilities.update_runtime_metrics(current_jobs);
    }

    pub fn should_broadcast(&self) -> bool {
        if let Ok(elapsed) = SystemTime::now().duration_since(self.last_broadcast) {
            elapsed >= self.broadcast_interval
        } else {
            true
        }
    }

    pub fn mark_broadcasted(&mut self) {
        self.last_broadcast = SystemTime::now();
    }

    pub fn get_broadcast_message(&mut self, current_jobs: u32) -> Option<NodeCapabilities> {
        if self.should_broadcast() {
            self.update_capabilities(current_jobs);
            self.mark_broadcasted();
            Some(self.capabilities.clone())
        } else {
            None
        }
    }

    pub fn set_broadcast_interval(&mut self, interval: Duration) {
        self.broadcast_interval = interval;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_detection() {
        let caps = NodeCapabilities::detect_system();
        assert!(caps.cpu_cores > 0);
        assert!(caps.memory_gb > 0);
        assert!(caps.max_concurrent_jobs > 0);
    }

    #[test]
    fn test_available_capacity() {
        let mut caps = NodeCapabilities::detect_system();
        caps.max_concurrent_jobs = 10;
        caps.current_job_count = 3;

        assert_eq!(caps.available_capacity_percent(), 70.0);
        assert!(caps.is_available_for_jobs());

        caps.current_job_count = 10;
        assert_eq!(caps.available_capacity_percent(), 0.0);
        assert!(!caps.is_available_for_jobs());
    }

    #[test]
    fn test_job_requirements_matching() {
        let mut caps = NodeCapabilities::detect_system();
        caps.cpu_cores = 8;
        caps.memory_available_gb = 16;
        caps.has_gpu = true;
        caps.max_concurrent_jobs = 10;
        caps.current_job_count = 5;

        let req = JobRequirements {
            min_cpu_cores: 4,
            min_memory_gb: 8,
            requires_gpu: true,
            min_gpu_vram_gb: None,
            required_specializations: Vec::new(),
        };

        assert!(caps.meets_requirements(&req));

        let high_req = JobRequirements {
            min_cpu_cores: 16,
            ..req
        };
        assert!(!caps.meets_requirements(&high_req));
    }

    #[test]
    fn test_advertiser_intervals() {
        let caps = NodeCapabilities::detect_system();
        let mut advertiser = CapabilityAdvertiser::new(caps);

        assert!(advertiser.should_broadcast());
        advertiser.mark_broadcasted();

        advertiser.set_broadcast_interval(Duration::from_secs(30));
        assert_eq!(advertiser.broadcast_interval, Duration::from_secs(30));
    }
}
