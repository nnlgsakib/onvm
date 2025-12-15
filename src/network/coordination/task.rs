/// Background coordination task
///
/// Runs periodic coordination operations:
/// - Broadcasting node capabilities
/// - Cleaning up stale peers
/// - Updating peer scores

use crate::network::coordination::CoordinationManager;
use crate::network::NetworkHandle;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

pub struct CoordinationTask {
    coordination: Arc<CoordinationManager>,
    network: NetworkHandle,
    current_job_count: Arc<tokio::sync::RwLock<u32>>,
}

impl CoordinationTask {
    pub fn new(coordination: Arc<CoordinationManager>, network: NetworkHandle) -> Self {
        Self {
            coordination,
            network,
            current_job_count: Arc::new(tokio::sync::RwLock::new(0)),
        }
    }

    pub async fn set_job_count(&self, count: u32) {
        let mut job_count = self.current_job_count.write().await;
        *job_count = count;
    }

    pub async fn run(self: Arc<Self>) {
        let mut capability_tick = interval(Duration::from_secs(30));
        let mut cleanup_tick = interval(Duration::from_secs(300));

        loop {
            tokio::select! {
                _ = capability_tick.tick() => {
                    if let Err(e) = self.broadcast_capabilities().await {
                        tracing::warn!("failed to broadcast capabilities: {e:?}");
                    }
                }
                _ = cleanup_tick.tick() => {
                    self.cleanup_stale_data().await;
                }
            }
        }
    }

    async fn broadcast_capabilities(&self) -> anyhow::Result<()> {
        let job_count = *self.current_job_count.read().await;
        let advertiser_lock = self.coordination.capability_advertiser();
        let mut advertiser = advertiser_lock.write().await;

        if let Some(capabilities) = advertiser.get_broadcast_message(job_count) {
            let msg = crate::network::NetworkMessage::Capability(capabilities);
            let _ = self.network.publisher.send(msg);
            tracing::debug!("broadcasted node capabilities to network");
        }

        Ok(())
    }

    async fn cleanup_stale_data(&self) {
        let discovery_lock = self.coordination.peer_discovery();
        let mut discovery = discovery_lock.write().await;
        discovery.cleanup_stale_peers(Duration::from_secs(600));

        if let Err(e) = discovery.save_to_cache() {
            tracing::warn!("failed to save peer cache: {e:?}");
        }
        drop(discovery);

        let lb_lock = self.coordination.load_balancer();
        let mut load_balancer = lb_lock.write().await;
        load_balancer.cleanup_stale_peers();

        tracing::debug!("cleaned up stale coordination data");
    }
}