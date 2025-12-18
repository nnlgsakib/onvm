/// Network Coordination Module
///
/// This module provides coordinated network services on top of the base libp2p network:
/// - Peer discovery: Enhanced discovery with scoring and caching
/// - Capability broadcasting: Advertise node capabilities (CPU, GPU, memory)
/// - Load balancing: Intelligent peer selection based on capabilities and load
pub mod capability;
pub mod discovery;
pub mod load_balancer;
pub mod task;

pub use capability::{CapabilityAdvertiser, JobRequirements, NodeCapabilities, Specialization};
pub use discovery::{PeerDiscovery, PeerInfo, PeerScore};
pub use load_balancer::{BalancingStrategy, LoadBalancer, PeerSelection, SelectionReason};
pub use task::CoordinationTask;

use anyhow::Result;
use libp2p::PeerId;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Coordination manager that orchestrates all coordination services
pub struct CoordinationManager {
    peer_discovery: Arc<RwLock<PeerDiscovery>>,
    capability_advertiser: Arc<RwLock<CapabilityAdvertiser>>,
    load_balancer: Arc<RwLock<LoadBalancer>>,
}

impl CoordinationManager {
    pub fn new(local_peer_id: PeerId, capabilities: NodeCapabilities) -> Self {
        let peer_discovery = Arc::new(RwLock::new(PeerDiscovery::new(local_peer_id)));
        let capability_advertiser = Arc::new(RwLock::new(CapabilityAdvertiser::new(capabilities)));
        let load_balancer = Arc::new(RwLock::new(LoadBalancer::new()));

        Self {
            peer_discovery,
            capability_advertiser,
            load_balancer,
        }
    }

    pub fn with_cache(mut self, cache_dir: std::path::PathBuf) -> Self {
        let peer_cache_path = cache_dir.join("peers.bin");
        let discovery = Arc::new(RwLock::new(
            PeerDiscovery::new(
                *futures::executor::block_on(async { self.peer_discovery.read().await })
                    .local_peer_id(),
            )
            .with_cache(peer_cache_path),
        ));
        self.peer_discovery = discovery;
        self
    }

    pub async fn on_peer_connected(&self, peer_id: PeerId) -> Result<()> {
        self.peer_discovery.write().await.on_peer_connected(peer_id);
        Ok(())
    }

    pub async fn on_peer_disconnected(&self, peer_id: PeerId) -> Result<()> {
        self.peer_discovery
            .write()
            .await
            .on_peer_disconnected(peer_id);
        self.load_balancer
            .write()
            .await
            .on_peer_disconnected(peer_id);
        Ok(())
    }

    pub async fn on_capability_received(
        &self,
        peer_id: PeerId,
        capabilities: NodeCapabilities,
    ) -> Result<()> {
        self.peer_discovery
            .write()
            .await
            .update_peer_capabilities(peer_id, capabilities.clone())?;
        self.load_balancer
            .write()
            .await
            .update_peer_capabilities(peer_id, capabilities)?;
        Ok(())
    }

    pub fn peer_discovery(&self) -> Arc<RwLock<PeerDiscovery>> {
        Arc::clone(&self.peer_discovery)
    }

    pub fn capability_advertiser(&self) -> Arc<RwLock<CapabilityAdvertiser>> {
        Arc::clone(&self.capability_advertiser)
    }

    pub fn load_balancer(&self) -> Arc<RwLock<LoadBalancer>> {
        Arc::clone(&self.load_balancer)
    }
}

impl PeerDiscovery {
    pub fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }
}
