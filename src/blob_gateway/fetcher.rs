use crate::node::Node;
use crate::types::ObjectId;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct BlobFetcher {
    node: Arc<Node>,
}

#[derive(Serialize, Deserialize)]
pub struct BlobInfo {
    pub id: String,
    pub size: u64,
    pub chunk_count: u32,
    pub subchunk_count: u64,
    pub available_locally: bool,
    pub mime_type: Option<String>,
    pub detected_mime_type: Option<String>,
}

impl BlobFetcher {
    pub fn new(node: Arc<Node>) -> Self {
        Self { node }
    }

    pub async fn fetch_blob(&self, id: &ObjectId) -> Result<Vec<u8>> {
        match self.node.unified_store.get_object(id) {
            Ok(data) => Ok(data),
            Err(_) => {
                let blob_id = crate::types::BlobId(id.0);
                self.node
                    .consensus
                    .fetch_blob(&blob_id)
                    .await
                    .map_err(|e| anyhow!("Failed to fetch blob from network: {}", e))
            }
        }
    }

    pub async fn get_blob_info(&self, id: &ObjectId) -> Result<BlobInfo> {
        let metadata = self.node.unified_store.get_object_metadata(id)?;

        if let Some(obj) = metadata {
            let mime_type = match &obj.object_type {
                crate::types::ObjectType::Blob { mime_type, .. } => mime_type.clone(),
                _ => None,
            };

            let detected_mime_type = if self.node.unified_store.is_complete(id).unwrap_or(false) {
                match self.node.unified_store.get_object(id) {
                    Ok(data) => {
                        let detector =
                            crate::blob_gateway::content_detector::ContentDetector::new();
                        Some(detector.detect(&data))
                    }
                    Err(_) => None,
                }
            } else {
                None
            };

            let subchunk_count = crate::storage::subchunk_count_for_total_size(obj.total_size);

            Ok(BlobInfo {
                id: crate::types::BlobId(id.0).to_prefixed_string(),
                size: obj.total_size,
                chunk_count: obj.chunk_count,
                subchunk_count,
                available_locally: self.node.unified_store.is_complete(id).unwrap_or(false),
                mime_type,
                detected_mime_type,
            })
        } else {
            Err(anyhow!("Blob metadata not found"))
        }
    }

    pub async fn check_network(&self, id: &ObjectId) -> Result<bool> {
        let blob_id = crate::types::BlobId(id.0);

        match self.node.consensus.fetch_blob(&blob_id).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}
