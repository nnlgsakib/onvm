/// Load Balancer Module
///
/// Provides intelligent peer selection for job routing based on:
/// - Current load and capacity
/// - Capabilities matching job requirements
/// - Peer reputation scores
/// - Geographic/latency optimization
/// - Round-robin fairness
use crate::network::coordination::capability::{JobRequirements, NodeCapabilities};
use anyhow::{anyhow, Result};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Peer selection result
#[derive(Debug, Clone)]
pub struct PeerSelection {
    pub peer_id: PeerId,
    pub score: f64,
    pub reason: SelectionReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelectionReason {
    BestScore,
    CapabilityMatch,
    LowLoad,
    RoundRobin,
    OnlyAvailable,
}

/// Load tracking per peer
#[derive(Debug, Clone)]
struct PeerLoad {
    peer_id: PeerId,
    capabilities: Option<NodeCapabilities>,
    last_updated: SystemTime,
    jobs_assigned: u32,
    selection_score: f64,
}

impl PeerLoad {
    fn new(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            capabilities: None,
            last_updated: SystemTime::now(),
            jobs_assigned: 0,
            selection_score: 0.0,
        }
    }

    fn update_capabilities(&mut self, capabilities: NodeCapabilities) {
        self.capabilities = Some(capabilities);
        self.last_updated = SystemTime::now();
        self.recalculate_score();
    }

    fn recalculate_score(&mut self) {
        if let Some(ref caps) = self.capabilities {
            let capacity_score = caps.available_capacity_percent() as f64;

            let memory_score = caps.memory_available_percent() as f64;

            let load_penalty = (self.jobs_assigned as f64) * 5.0;

            let capability_bonus = if caps.has_gpu { 10.0 } else { 0.0 };

            self.selection_score = (capacity_score * 0.5 + memory_score * 0.3 + capability_bonus
                - load_penalty)
                .max(0.0);
        } else {
            self.selection_score = 0.0;
        }
    }

    fn on_job_assigned(&mut self) {
        self.jobs_assigned += 1;
        self.recalculate_score();
    }

    fn on_job_completed(&mut self) {
        self.jobs_assigned = self.jobs_assigned.saturating_sub(1);
        self.recalculate_score();
    }

    fn is_stale(&self, max_age_secs: u64) -> bool {
        if let Ok(elapsed) = SystemTime::now().duration_since(self.last_updated) {
            elapsed.as_secs() > max_age_secs
        } else {
            true
        }
    }
}

/// Load balancer strategy
#[derive(Debug, Clone, Copy)]
pub enum BalancingStrategy {
    BestScore,
    RoundRobin,
    LeastLoad,
    CapabilityMatch,
}

/// Load balancer for intelligent peer selection
pub struct LoadBalancer {
    peers: HashMap<PeerId, PeerLoad>,
    strategy: BalancingStrategy,
    round_robin_index: usize,
    peer_order: Vec<PeerId>,
}

impl LoadBalancer {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
            strategy: BalancingStrategy::BestScore,
            round_robin_index: 0,
            peer_order: Vec::new(),
        }
    }

    pub fn with_strategy(mut self, strategy: BalancingStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn update_peer_capabilities(
        &mut self,
        peer_id: PeerId,
        capabilities: NodeCapabilities,
    ) -> Result<()> {
        let peer_load = self
            .peers
            .entry(peer_id)
            .or_insert_with(|| PeerLoad::new(peer_id));
        peer_load.update_capabilities(capabilities);

        if !self.peer_order.contains(&peer_id) {
            self.peer_order.push(peer_id);
        }

        Ok(())
    }

    pub fn on_peer_disconnected(&mut self, peer_id: PeerId) {
        self.peers.remove(&peer_id);
        self.peer_order.retain(|id| *id != peer_id);
    }

    pub fn on_job_assigned(&mut self, peer_id: PeerId) {
        if let Some(peer_load) = self.peers.get_mut(&peer_id) {
            peer_load.on_job_assigned();
        }
    }

    pub fn on_job_completed(&mut self, peer_id: PeerId) {
        if let Some(peer_load) = self.peers.get_mut(&peer_id) {
            peer_load.on_job_completed();
        }
    }

    pub fn select_peer(&mut self, requirements: Option<&JobRequirements>) -> Result<PeerSelection> {
        let candidates = self.get_eligible_peers(requirements);

        if candidates.is_empty() {
            return Err(anyhow!("no eligible peers available"));
        }

        let candidate_data: Vec<(PeerId, f64)> = candidates
            .iter()
            .map(|p| (p.peer_id, p.selection_score))
            .collect();

        let selection = match self.strategy {
            BalancingStrategy::BestScore => self.select_best_score(&candidates),
            BalancingStrategy::RoundRobin => self.select_round_robin_from_data(&candidate_data),
            BalancingStrategy::LeastLoad => self.select_least_load(&candidates),
            BalancingStrategy::CapabilityMatch => {
                if let Some(reqs) = requirements {
                    self.select_capability_match(&candidates, reqs)
                } else {
                    self.select_best_score(&candidates)
                }
            }
        };

        if let Some(selection) = selection {
            self.on_job_assigned(selection.peer_id);
            Ok(selection)
        } else {
            Err(anyhow!("failed to select peer"))
        }
    }

    fn get_eligible_peers(&self, requirements: Option<&JobRequirements>) -> Vec<&PeerLoad> {
        self.peers
            .values()
            .filter(|peer| {
                if peer.is_stale(300) {
                    return false;
                }

                if let Some(reqs) = requirements {
                    if let Some(ref caps) = peer.capabilities {
                        return caps.meets_requirements(reqs);
                    }
                    return false;
                }

                true
            })
            .collect()
    }

    fn select_best_score(&self, candidates: &[&PeerLoad]) -> Option<PeerSelection> {
        candidates
            .iter()
            .max_by(|a, b| {
                a.selection_score
                    .partial_cmp(&b.selection_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|peer| PeerSelection {
                peer_id: peer.peer_id,
                score: peer.selection_score,
                reason: SelectionReason::BestScore,
            })
    }

    fn select_round_robin_from_data(
        &mut self,
        candidate_data: &[(PeerId, f64)],
    ) -> Option<PeerSelection> {
        if candidate_data.is_empty() {
            return None;
        }

        if self.round_robin_index >= candidate_data.len() {
            self.round_robin_index = 0;
        }

        let (selected_id, score) = candidate_data[self.round_robin_index];
        self.round_robin_index = (self.round_robin_index + 1) % candidate_data.len();

        Some(PeerSelection {
            peer_id: selected_id,
            score,
            reason: SelectionReason::RoundRobin,
        })
    }

    fn select_least_load(&self, candidates: &[&PeerLoad]) -> Option<PeerSelection> {
        candidates
            .iter()
            .min_by_key(|peer| peer.jobs_assigned)
            .map(|peer| PeerSelection {
                peer_id: peer.peer_id,
                score: peer.selection_score,
                reason: SelectionReason::LowLoad,
            })
    }

    fn select_capability_match(
        &self,
        candidates: &[&PeerLoad],
        requirements: &JobRequirements,
    ) -> Option<PeerSelection> {
        candidates
            .iter()
            .filter(|peer| {
                if let Some(ref caps) = peer.capabilities {
                    caps.meets_requirements(requirements)
                } else {
                    false
                }
            })
            .max_by(|a, b| {
                a.selection_score
                    .partial_cmp(&b.selection_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|peer| PeerSelection {
                peer_id: peer.peer_id,
                score: peer.selection_score,
                reason: SelectionReason::CapabilityMatch,
            })
    }

    pub fn cleanup_stale_peers(&mut self) {
        let initial_count = self.peers.len();
        self.peers.retain(|_, peer| !peer.is_stale(300));
        let removed = initial_count - self.peers.len();
        if removed > 0 {
            tracing::debug!("cleaned up {} stale peer load entries", removed);
        }
    }

    pub fn get_peer_stats(&self, peer_id: &PeerId) -> Option<(u32, f64)> {
        self.peers
            .get(peer_id)
            .map(|peer| (peer.jobs_assigned, peer.selection_score))
    }

    pub fn total_available_capacity(&self) -> u32 {
        self.peers
            .values()
            .filter_map(|peer| {
                peer.capabilities.as_ref().map(|caps| {
                    caps.max_concurrent_jobs
                        .saturating_sub(caps.current_job_count)
                })
            })
            .sum()
    }
}

impl Default for LoadBalancer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::coordination::capability::NodeCapabilities;

    #[test]
    fn test_load_balancer_basic() {
        let mut lb = LoadBalancer::new();
        let peer1 = PeerId::random();

        let mut caps = NodeCapabilities::detect_system();
        caps.max_concurrent_jobs = 10;
        caps.current_job_count = 2;

        lb.update_peer_capabilities(peer1, caps).unwrap();

        let selection = lb.select_peer(None).unwrap();
        assert_eq!(selection.peer_id, peer1);
    }

    #[test]
    fn test_best_score_selection() {
        let mut lb = LoadBalancer::new().with_strategy(BalancingStrategy::BestScore);

        let peer1 = PeerId::random();
        let mut caps1 = NodeCapabilities::detect_system();
        caps1.max_concurrent_jobs = 10;
        caps1.current_job_count = 8;
        lb.update_peer_capabilities(peer1, caps1).unwrap();

        let peer2 = PeerId::random();
        let mut caps2 = NodeCapabilities::detect_system();
        caps2.max_concurrent_jobs = 10;
        caps2.current_job_count = 2;
        lb.update_peer_capabilities(peer2, caps2).unwrap();

        let selection = lb.select_peer(None).unwrap();
        assert_eq!(selection.peer_id, peer2);
    }

    #[test]
    fn test_round_robin_selection() {
        let mut lb = LoadBalancer::new().with_strategy(BalancingStrategy::RoundRobin);

        let peer1 = PeerId::random();
        let caps1 = NodeCapabilities::detect_system();
        lb.update_peer_capabilities(peer1, caps1).unwrap();

        let peer2 = PeerId::random();
        let caps2 = NodeCapabilities::detect_system();
        lb.update_peer_capabilities(peer2, caps2).unwrap();

        let sel1 = lb.select_peer(None).unwrap();
        let sel2 = lb.select_peer(None).unwrap();

        assert_ne!(sel1.peer_id, sel2.peer_id);
    }

    #[test]
    fn test_job_tracking() {
        let mut lb = LoadBalancer::new();
        let peer1 = PeerId::random();
        let caps = NodeCapabilities::detect_system();
        lb.update_peer_capabilities(peer1, caps).unwrap();

        lb.on_job_assigned(peer1);
        let (jobs, _) = lb.get_peer_stats(&peer1).unwrap();
        assert_eq!(jobs, 1);

        lb.on_job_completed(peer1);
        let (jobs, _) = lb.get_peer_stats(&peer1).unwrap();
        assert_eq!(jobs, 0);
    }
}
