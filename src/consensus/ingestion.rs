use super::DagEngine;
use crate::network::NetworkMessage;
use crate::types::{BlobId, Object};
use anyhow::{anyhow, Context, Result};

impl DagEngine {
    pub async fn ingest_local_object(&self, object: Object, _data: Vec<u8>) -> Result<()> {
        self.network.provide(&object.id.0);

        let object_meta_msg =
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectMetadata(
                object.clone(),
            );
        let meta_network_msg = NetworkMessage::UnifiedProtocol(object_meta_msg);

        let _ = self.network.publisher.send(meta_network_msg);

        let announcement = crate::network::unified_protocol::ObjectAnnouncement::from_object(
            &object,
            &self.network.peer_id,
        );

        let unified_msg =
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectAnnouncement(
                announcement,
            );
        let msg = NetworkMessage::UnifiedProtocol(unified_msg);

        self.network
            .publisher
            .send(msg)
            .map_err(|_| anyhow!("failed to announce object"))?;

        tracing::info!("announced object {} to DHT and gossipsub", object.id);

        Ok(())
    }

    pub async fn ingest_local_blob(&self, object: Object, data: Vec<u8>) -> Result<()> {
        self.ingest_local_object(object, data).await
    }

    pub async fn ingest_local_program(&self, object: Object, data: Vec<u8>) -> Result<()> {
        self.ingest_local_object(object, data).await
    }

    pub async fn fetch_blob(&self, blob_id: &BlobId) -> Result<Vec<u8>> {
        let object_id = blob_id.to_object_id();
        if self.unified_store.is_complete(&object_id)? {
            return self.unified_store.get_object(&object_id);
        }

        if let Some(ref distributor) = self.chunk_distributor {
            tracing::info!(
                "blob {} not available locally, attempting to fetch from network",
                blob_id
            );

            distributor
                .fetch_object(&object_id, crate::network::ProviderKind::Blob)
                .await
                .context("network blob fetch failed")?;

            return self
                .unified_store
                .get_object(&object_id)
                .context("blob fetch completed but still unavailable");
        }

        Err(anyhow!(
            "blob not available locally and no chunk distributor configured"
        ))
    }
}
