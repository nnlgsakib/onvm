use super::health::{HealthStatus, NodeHealth};
use crate::network::coordination::{NodeCapabilities, PeerInfo};
use anyhow::{anyhow, Result};
use libp2p::PeerId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingStrategy {
    CapacityBased,
    HealthBased,
    RoundRobin,
}

impl Default for SchedulingStrategy {
    fn default() -> Self {
        Self::CapacityBased
    }
}

#[derive(Debug, Clone)]
pub struct NodeSelection {
    pub peer_id: PeerId,
    pub score: f64,
    pub reason: String,
}

pub struct NodeSelector;

impl NodeSelector {
    pub fn select_node(
        strategy: SchedulingStrategy,
        peers: &[PeerInfo],
        local_health: Option<&NodeHealth>,
    ) -> Result<NodeSelection> {
        if peers.is_empty() && local_health.is_none() {
            return Err(anyhow!("no available nodes"));
        }

        match strategy {
            SchedulingStrategy::CapacityBased => Self::select_by_capacity(peers, local_health),
            SchedulingStrategy::HealthBased => Self::select_by_health(peers, local_health),
            SchedulingStrategy::RoundRobin => Self::select_round_robin(peers, local_health),
        }
    }

    fn select_by_capacity(
        peers: &[PeerInfo],
        local_health: Option<&NodeHealth>,
    ) -> Result<NodeSelection> {
        let mut candidates = Vec::new();

        for peer in peers {
            if !peer.is_connected {
                continue;
            }

            if let Some(caps) = &peer.capabilities {
                let capacity_score = Self::calculate_capacity_score(caps);
                let reliability_score = peer.score.reliability;
                let combined_score = (capacity_score * 0.7) + (reliability_score * 0.3);

                candidates.push((peer.peer_id, combined_score, "remote peer"));
            }
        }

        if let Some(health) = local_health {
            if health.status != HealthStatus::Unavailable {
                let local_score = Self::calculate_local_capacity_score(health);
                candidates.push((PeerId::random(), local_score, "local node"));
            }
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        candidates
            .first()
            .map(|(peer_id, score, reason)| NodeSelection {
                peer_id: *peer_id,
                score: *score,
                reason: format!("{} (capacity-based)", reason),
            })
            .ok_or_else(|| anyhow!("no suitable nodes found"))
    }

    fn select_by_health(
        peers: &[PeerInfo],
        local_health: Option<&NodeHealth>,
    ) -> Result<NodeSelection> {
        let mut candidates = Vec::new();

        for peer in peers {
            if !peer.is_connected {
                continue;
            }

            if let Some(caps) = &peer.capabilities {
                let health_score = Self::calculate_health_score(caps);
                let reliability_score = peer.score.reliability;
                let combined_score = (health_score * 0.6) + (reliability_score * 0.4);

                candidates.push((peer.peer_id, combined_score, "remote peer"));
            }
        }

        if let Some(health) = local_health {
            let local_score = Self::calculate_local_health_score(health);
            if local_score > 0.0 {
                candidates.push((PeerId::random(), local_score, "local node"));
            }
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        candidates
            .first()
            .map(|(peer_id, score, reason)| NodeSelection {
                peer_id: *peer_id,
                score: *score,
                reason: format!("{} (health-based)", reason),
            })
            .ok_or_else(|| anyhow!("no suitable nodes found"))
    }

    fn select_round_robin(
        peers: &[PeerInfo],
        _local_health: Option<&NodeHealth>,
    ) -> Result<NodeSelection> {
        let connected: Vec<_> = peers.iter().filter(|p| p.is_connected).collect();

        if connected.is_empty() {
            return Err(anyhow!("no connected peers"));
        }

        let idx = 0;
        let peer = connected[idx];

        Ok(NodeSelection {
            peer_id: peer.peer_id,
            score: 1.0,
            reason: "round-robin".to_string(),
        })
    }

    fn calculate_capacity_score(caps: &NodeCapabilities) -> f64 {
        let cpu_available = 1.0 - (caps.cpu_usage_percent / 100.0) as f64;
        let mem_available = (caps.memory_available_gb as f64) / (caps.memory_gb.max(1) as f64);
        let job_capacity =
            1.0 - ((caps.current_job_count as f64) / (caps.max_concurrent_jobs.max(1) as f64));

        (cpu_available * 0.3) + (mem_available * 0.3) + (job_capacity * 0.4)
    }

    fn calculate_health_score(caps: &NodeCapabilities) -> f64 {
        let cpu_health = if caps.cpu_usage_percent < 70.0 {
            1.0
        } else if caps.cpu_usage_percent < 90.0 {
            0.5
        } else {
            0.1
        };

        let mem_ratio = (caps.memory_available_gb as f64) / (caps.memory_gb.max(1) as f64);
        let mem_health = if mem_ratio > 0.3 {
            1.0
        } else if mem_ratio > 0.1 {
            0.5
        } else {
            0.1
        };

        let job_ratio = (caps.current_job_count as f64) / (caps.max_concurrent_jobs.max(1) as f64);
        let job_health = if job_ratio < 0.7 {
            1.0
        } else if job_ratio < 0.9 {
            0.5
        } else {
            0.1
        };

        (cpu_health * 0.35) + (mem_health * 0.35) + (job_health * 0.3)
    }

    fn calculate_local_capacity_score(health: &NodeHealth) -> f64 {
        let cpu_available = 1.0 - (health.resources.cpu_percent / 100.0);
        let mem_available = 1.0 - (health.resources.memory_percent / 100.0);
        let job_capacity = 1.0
            - ((health.capacity.current_jobs as f64)
                / (health.capacity.max_concurrent_jobs.max(1) as f64));

        (cpu_available * 0.3) + (mem_available * 0.3) + (job_capacity * 0.4)
    }

    fn calculate_local_health_score(health: &NodeHealth) -> f64 {
        if health.status == HealthStatus::Unavailable {
            return 0.0;
        }

        let base_score = if health.status == HealthStatus::Ready {
            1.0
        } else {
            0.5
        };

        let cpu_health = if health.resources.cpu_percent < 70.0 {
            1.0
        } else if health.resources.cpu_percent < 90.0 {
            0.5
        } else {
            0.1
        };

        let mem_health = if health.resources.memory_percent < 70.0 {
            1.0
        } else if health.resources.memory_percent < 90.0 {
            0.5
        } else {
            0.1
        };

        base_score * ((cpu_health * 0.5) + (mem_health * 0.5))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_runtime::health::{NodeCapacity, ResourceUsage};

    #[test]
    fn test_capacity_based_selection() {
        let caps1 = NodeCapabilities {
            cpu_cores: 4,
            cpu_usage_percent: 50.0,
            memory_gb: 16,
            memory_available_gb: 8,
            has_gpu: false,
            gpu_model: None,
            gpu_vram_gb: None,
            storage_gb: 100,
            bandwidth_mbps: Some(1000),
            specializations: vec![],
            max_concurrent_jobs: 10,
            current_job_count: 2,
            advertised_at: 0,
        };

        let caps2 = NodeCapabilities {
            cpu_cores: 8,
            cpu_usage_percent: 20.0,
            memory_gb: 32,
            memory_available_gb: 24,
            has_gpu: false,
            gpu_model: None,
            gpu_vram_gb: None,
            storage_gb: 200,
            bandwidth_mbps: Some(1000),
            specializations: vec![],
            max_concurrent_jobs: 20,
            current_job_count: 5,
            advertised_at: 0,
        };

        let peer1 = PeerInfo {
            peer_id: PeerId::random(),
            capabilities: Some(caps1),
            first_seen: std::time::SystemTime::now(),
            last_seen: std::time::SystemTime::now(),
            connection_count: 1,
            disconnection_count: 0,
            is_connected: true,
            score: crate::network::coordination::PeerScore {
                reliability: 0.9,
                latency_score: 0.8,
                capability_score: 0.7,
                overall: 0.8,
            },
        };

        let peer2 = PeerInfo {
            peer_id: PeerId::random(),
            capabilities: Some(caps2),
            first_seen: std::time::SystemTime::now(),
            last_seen: std::time::SystemTime::now(),
            connection_count: 1,
            disconnection_count: 0,
            is_connected: true,
            score: crate::network::coordination::PeerScore {
                reliability: 0.95,
                latency_score: 0.85,
                capability_score: 0.9,
                overall: 0.9,
            },
        };

        let peers = vec![peer1, peer2.clone()];
        let result = NodeSelector::select_node(SchedulingStrategy::CapacityBased, &peers, None);

        assert!(result.is_ok());
        let selection = result.unwrap();
        assert_eq!(selection.peer_id, peer2.peer_id);
    }

    #[test]
    fn test_health_based_selection() {
        let health = NodeHealth {
            status: HealthStatus::Ready,
            timestamp: 0,
            capacity: NodeCapacity {
                max_concurrent_jobs: 10,
                current_jobs: 2,
                queue_depth: 0,
                cpu_cores: 4,
                total_memory_bytes: 16 * 1024 * 1024 * 1024,
            },
            resources: ResourceUsage {
                cpu_percent: 30.0,
                memory_used_bytes: 4 * 1024 * 1024 * 1024,
                memory_percent: 25.0,
                active_jobs: 2,
            },
            version: "0.1.0".to_string(),
        };

        let result = NodeSelector::select_node(SchedulingStrategy::HealthBased, &[], Some(&health));

        assert!(result.is_ok());
        let selection = result.unwrap();
        assert!(selection.score > 0.5);
    }
}
