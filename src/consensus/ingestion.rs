use super::DagEngine;
use crate::crypto::hashing::hash_bytes;
use crate::network::NetworkMessage;
use crate::types::{BlobId, Object, ObjectType, ProgramAnnouncement, ProgramId, ProgramManifest};
use anyhow::{anyhow, Context, Result};
use std::time::{SystemTime, UNIX_EPOCH};

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
        let manifest = self.build_program_manifest(&object)?;
        self.program_catalog.store_manifest(&manifest)?;
        let announcement = self.build_program_announcement(&object, manifest)?;

        self.ingest_local_object(object, data).await?;
        self.announce_program_manifest(announcement).await?;
        Ok(())
    }

    pub async fn announce_program_manifest(&self, announcement: ProgramAnnouncement) -> Result<()> {
        let msg = crate::network::UnifiedProtocolMessage::ProgramAnnouncement(announcement);
        let net_msg = NetworkMessage::UnifiedProtocol(msg);
        self.network
            .publisher
            .send(net_msg)
            .map_err(|_| anyhow!("failed to publish program announcement"))?;
        Ok(())
    }

    pub async fn broadcast_aggregated_receipt(
        &self,
        bundle: crate::network::AggregatedReceiptBundle,
    ) -> Result<()> {
        let msg = crate::network::UnifiedProtocolMessage::AggregatedReceipt(bundle);
        let net_msg = NetworkMessage::UnifiedProtocol(msg);
        self.network
            .publisher
            .send(net_msg)
            .map_err(|_| anyhow!("failed to publish aggregated receipt"))?;
        Ok(())
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

    fn build_program_manifest(&self, object: &Object) -> Result<ProgramManifest> {
        let ObjectType::WasmProgram {
            entrypoint,
            wasm_version: _,
            source_language: _,
            compiler: _,
            blob_refs: _,
            deploy_salt: _,
        } = &object.object_type
        else {
            return Err(anyhow!("object is not a WASM program"));
        };

        let program_id = ProgramId(object.id.0);
        let initial_state_root = self
            .state_store
            .root_scoped(&program_id.0)
            .context("computing initial state root")?;

        let metadata_hash = {
            let encoded =
                bincode::serde::encode_to_vec(&object.object_type, bincode::config::standard())?;
            hash_bytes(&encoded)
        };

        let wasm_env_hash = hash_bytes(b"wasm_env_v1");

        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut manifest = ProgramManifest {
            program_id,
            version: 1,
            deployer: self.identity.node_id.clone(),
            wasm_env_hash,
            code_manifest: Some(object.manifest_id),
            metadata_hash,
            entrypoints: vec![entrypoint.clone()],
            initial_state_root,
            dag_parent: None,
            timestamp_ms,
            signature: Vec::new(),
        };

        let digest = manifest.digest();
        let sig = self.identity.sign(&digest);
        manifest.signature = sig.to_bytes().to_vec();
        Ok(manifest)
    }

    fn build_program_announcement(
        &self,
        object: &Object,
        manifest: ProgramManifest,
    ) -> Result<ProgramAnnouncement> {
        let Some(obj_manifest) = self
            .unified_store
            .get_manifest_by_object(&object.id)
            .context("loading object manifest")?
        else {
            return Err(anyhow!("missing manifest for object {}", object.id));
        };
        let chunk_roots = obj_manifest.chunks.iter().map(|c| c.chunk_id).collect();
        Ok(ProgramAnnouncement {
            manifest,
            chunk_roots,
            initial_state_chunks: Vec::new(),
        })
    }
}
