//! Periodic background tasks driven by the consensus event loop.

use super::DagEngine;
use anyhow::Result;

impl DagEngine {
    pub async fn periodic_sync(&self) -> Result<()> {
        if self.peer_count().await == 0 {
            return Ok(());
        }

        let _ = self.request_inventory().await;
        let _ = self.refresh_sync_state().await;
        let _ = self.broadcast_program_heads().await;
        Ok(())
    }

    async fn broadcast_program_heads(&self) -> Result<()> {
        let manifests = self.program_catalog.list_manifests()?;
        for manifest in manifests {
            let (height, state_root) = if let Some(commitment) = self
                .program_catalog
                .latest_state_commitment(&manifest.program_id)?
            {
                (commitment.height, commitment.root)
            } else {
                (0, manifest.initial_state_root)
            };

            let head = crate::network::unified_protocol::ProgramHead {
                program_id: manifest.program_id,
                height,
                state_root,
            };
            let msg = crate::network::UnifiedProtocolMessage::ProgramHead(head);
            let net_msg = crate::network::NetworkMessage::UnifiedProtocol(msg);
            let _ = self.network.publisher.send(net_msg);
        }
        Ok(())
    }
}
