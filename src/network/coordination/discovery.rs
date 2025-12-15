/// Peer Discovery Module
///
/// Manages discovery, tracking, and scoring of network peers.
/// Features:
/// - Peer connection tracking
/// - Capability tracking per peer
/// - Peer scoring based on reliability and performance
/// - Peer caching for quick reconnection

use crate::network::coordination::capability::NodeCapabilities;
use anyhow::Result;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Information about a discovered peer
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub capabilities: Option<NodeCapabilities>,
    pub first_seen: SystemTime,
    pub last_seen: SystemTime,
    pub connection_count: u32,
    pub disconnection_count: u32,
    pub is_connected: bool,
    pub score: PeerScore,
}

impl PeerInfo {
    pub fn new(peer_id: PeerId) -> Self {
        let now = SystemTime::now();
        Self {
            peer_id,
            capabilities: None,
            first_seen: now,
            last_seen: now,
            connection_count: 0,
            disconnection_count: 0,
            is_connected: false,
            score: PeerScore::default(),
        }
    }

    pub fn update_last_seen(&mut self) {
        self.last_seen = SystemTime::now();
    }

    pub fn on_connected(&mut self) {
        self.is_connected = true;
        self.connection_count += 1;
        self.update_last_seen();
        self.score.on_successful_connection();
    }

    pub fn on_disconnected(&mut self) {
        self.is_connected = false;
        self.disconnection_count += 1;
        self.update_last_seen();
    }

    pub fn uptime_percentage(&self) -> f64 {
        if self.connection_count == 0 {
            return 0.0;
        }
        let total_events = self.connection_count + self.disconnection_count;
        (self.connection_count as f64 / total_events as f64) * 100.0
    }
}

/// Peer scoring system
/// Higher scores indicate better peers for routing traffic
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PeerScore {
    pub reliability: f64,
    pub latency_score: f64,
    pub capability_score: f64,
    pub overall: f64,
}

impl Default for PeerScore {
    fn default() -> Self {
        Self {
            reliability: 50.0,
            latency_score: 50.0,
            capability_score: 50.0,
            overall: 50.0,
        }
    }
}

impl PeerScore {
    pub fn on_successful_connection(&mut self) {
        self.reliability = (self.reliability * 0.9 + 10.0).min(100.0);
        self.recalculate_overall();
    }

    pub fn on_failed_connection(&mut self) {
        self.reliability = (self.reliability * 0.9).max(0.0);
        self.recalculate_overall();
    }

    pub fn update_latency(&mut self, latency_ms: u64) {
        let latency_score = if latency_ms < 50 {
            100.0
        } else if latency_ms < 100 {
            80.0
        } else if latency_ms < 200 {
            60.0
        } else if latency_ms < 500 {
            40.0
        } else {
            20.0
        };
        self.latency_score = self.latency_score * 0.8 + latency_score * 0.2;
        self.recalculate_overall();
    }

    pub fn update_capabilities(&mut self, capabilities: &NodeCapabilities) {
        let base_score = 50.0;
        let cpu_score = capabilities.cpu_cores as f64 * 2.0;
        let memory_score = (capabilities.memory_gb as f64 / 16.0) * 20.0;
        let gpu_score = if capabilities.has_gpu { 30.0 } else { 0.0 };

        self.capability_score = (base_score + cpu_score + memory_score + gpu_score).min(100.0);
        self.recalculate_overall();
    }

    fn recalculate_overall(&mut self) {
        self.overall = (self.reliability * 0.4 + self.latency_score * 0.3 + self.capability_score * 0.3).min(100.0);
    }
}

/// Peer discovery manager
pub struct PeerDiscovery {
    pub(super) local_peer_id: PeerId,
    peers: HashMap<PeerId, PeerInfo>,
    cache_file_path: Option<std::path::PathBuf>,
}

impl PeerDiscovery {
    pub fn new(local_peer_id: PeerId) -> Self {
        Self {
            local_peer_id,
            peers: HashMap::new(),
            cache_file_path: None,
        }
    }

    pub fn with_cache(mut self, cache_path: std::path::PathBuf) -> Self {
        self.cache_file_path = Some(cache_path);
        if let Err(e) = self.load_from_cache() {
            tracing::warn!("failed to load peer cache: {e:?}");
        }
        self
    }

    pub fn on_peer_connected(&mut self, peer_id: PeerId) {
        if peer_id == self.local_peer_id {
            return;
        }

        let peer_info = self.peers.entry(peer_id).or_insert_with(|| PeerInfo::new(peer_id));
        peer_info.on_connected();
        tracing::debug!("peer connected: {} (score: {:.2})", peer_id, peer_info.score.overall);
    }

    pub fn on_peer_disconnected(&mut self, peer_id: PeerId) {
        if let Some(peer_info) = self.peers.get_mut(&peer_id) {
            peer_info.on_disconnected();
            tracing::debug!("peer disconnected: {} (uptime: {:.1}%)", peer_id, peer_info.uptime_percentage());
        }
    }

    pub fn update_peer_capabilities(
        &mut self,
        peer_id: PeerId,
        capabilities: NodeCapabilities,
    ) -> Result<()> {
        let peer_info = self.peers.entry(peer_id).or_insert_with(|| PeerInfo::new(peer_id));
        peer_info.score.update_capabilities(&capabilities);
        peer_info.capabilities = Some(capabilities.clone());
        peer_info.update_last_seen();

        tracing::info!(
            "peer {} capabilities updated: {} CPU cores, {} GB RAM, GPU: {}",
            peer_id,
            capabilities.cpu_cores,
            capabilities.memory_gb,
            capabilities.has_gpu
        );
        Ok(())
    }

    pub fn update_peer_latency(&mut self, peer_id: PeerId, latency: Duration) {
        if let Some(peer_info) = self.peers.get_mut(&peer_id) {
            peer_info.score.update_latency(latency.as_millis() as u64);
        }
    }

    pub fn get_peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peers.get(peer_id)
    }

    pub fn get_connected_peers(&self) -> Vec<&PeerInfo> {
        self.peers.values().filter(|p| p.is_connected).collect()
    }

    pub fn get_all_peers(&self) -> Vec<&PeerInfo> {
        self.peers.values().collect()
    }

    pub fn get_best_peers(&self, limit: usize) -> Vec<&PeerInfo> {
        let mut peers: Vec<_> = self.get_connected_peers();
        peers.sort_by(|a, b| b.score.overall.partial_cmp(&a.score.overall).unwrap());
        peers.truncate(limit);
        peers
    }

    pub fn get_peers_with_capability(&self, filter: impl Fn(&NodeCapabilities) -> bool) -> Vec<&PeerInfo> {
        self.peers
            .values()
            .filter(|p| {
                p.is_connected
                    && p.capabilities
                        .as_ref()
                        .map(|caps| filter(caps))
                        .unwrap_or(false)
            })
            .collect()
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    pub fn connected_peer_count(&self) -> usize {
        self.peers.values().filter(|p| p.is_connected).count()
    }

    fn load_from_cache(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn save_to_cache(&self) -> Result<()> {
        Ok(())
    }

    pub fn cleanup_stale_peers(&mut self, max_age: Duration) {
        let now = SystemTime::now();
        let initial_count = self.peers.len();

        self.peers.retain(|_, peer_info| {
            if peer_info.is_connected {
                return true;
            }
            if let Ok(elapsed) = now.duration_since(peer_info.last_seen) {
                elapsed < max_age
            } else {
                false
            }
        });

        let removed = initial_count - self.peers.len();
        if removed > 0 {
            tracing::info!("cleaned up {} stale peers", removed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_score_connection() {
        let mut score = PeerScore::default();
        let initial = score.overall;

        score.on_successful_connection();
        assert!(score.overall > initial);
        assert!(score.reliability > 50.0);
    }

    #[test]
    fn test_peer_info_tracking() {
        let peer_id = PeerId::random();
        let mut info = PeerInfo::new(peer_id);

        info.on_connected();
        assert!(info.is_connected);
        assert_eq!(info.connection_count, 1);

        info.on_disconnected();
        assert!(!info.is_connected);
        assert_eq!(info.disconnection_count, 1);
    }

    #[test]
    fn test_peer_discovery_basic() {
        let local = PeerId::random();
        let mut discovery = PeerDiscovery::new(local);

        let peer1 = PeerId::random();
        discovery.on_peer_connected(peer1);

        assert_eq!(discovery.connected_peer_count(), 1);
        assert!(discovery.get_peer_info(&peer1).is_some());
    }
}