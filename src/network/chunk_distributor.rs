use crate::network::unified_protocol::{ChunkRequest, ObjectAnnouncement};
use crate::network::{NetworkHandle, ProviderKind};
use crate::storage::UnifiedStore;
use crate::types::{Chunk, ChunkId, Object, ObjectId};
use anyhow::{anyhow, Result};
use libp2p::PeerId;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

const MAX_CONCURRENT_CHUNK_FETCHES: usize = 16;

pub struct ChunkDistributor {
    store: Arc<UnifiedStore>,
    network: NetworkHandle,
    pending_objects: Arc<RwLock<HashMap<ObjectId, PendingObject>>>,
    pub provider_map: Arc<RwLock<HashMap<ObjectId, HashSet<PeerId>>>>,
    chunk_requests: Arc<RwLock<HashMap<ChunkId, Vec<PeerId>>>>,
}

struct PendingObject {
    missing_chunks: VecDeque<ChunkId>,
    in_flight: HashSet<ChunkId>,
    retries: HashMap<ChunkId, u32>,
}

impl ChunkDistributor {
    pub fn new(store: Arc<UnifiedStore>, network: NetworkHandle) -> Self {
        Self {
            store,
            network,
            pending_objects: Arc::new(RwLock::new(HashMap::new())),
            provider_map: Arc::new(RwLock::new(HashMap::new())),
            chunk_requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn announce_object(&self, object: &Object) -> Result<()> {
        let announcement = ObjectAnnouncement::from_object(object, &self.network.peer_id);

        let unified_msg =
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectAnnouncement(
                announcement,
            );
        let network_msg = crate::network::NetworkMessage::UnifiedProtocol(unified_msg);

        self.network
            .publisher
            .send(network_msg)
            .map_err(|_| anyhow!("failed to send announcement"))?;

        Ok(())
    }

    pub async fn fetch_object(
        &self,
        object_id: &ObjectId,
        provider_kind: ProviderKind,
    ) -> Result<Vec<u8>> {
        if self.store.is_complete(object_id)? {
            return self.store.get_object(object_id);
        }

        let object = match self.store.get_object_metadata(object_id)? {
            Some(obj) => obj,
            None => {
                tracing::info!("object metadata not found locally, requesting from peers");
                self.fetch_object_metadata(object_id, provider_kind).await?
            }
        };

        // Try manifest and missing chunks before hitting the network for providers.
        if let Some(manifest) = self.store.get_manifest_by_object(object_id)? {
            let missing_chunks = self.store.get_missing_chunks(&manifest)?;
            if missing_chunks.is_empty() {
                return self.store.get_object(object_id);
            }
        }

        self.ensure_providers_for_object(object_id, provider_kind)
            .await;

        let manifest = match self.store.get_manifest_by_object(object_id)? {
            Some(m) => m,
            None => {
                tracing::info!("manifest not found locally, requesting from peers");
                self.fetch_manifest(object_id, &object.manifest_id, provider_kind)
                    .await?
            }
        };

        let missing_chunks = self.store.get_missing_chunks(&manifest)?;

        if missing_chunks.is_empty() {
            return self.store.get_object(object_id);
        }

        {
            let mut pending = self.pending_objects.write().await;
            pending.insert(
                *object_id,
                PendingObject {
                    missing_chunks: missing_chunks.into_iter().collect(),
                    in_flight: HashSet::new(),
                    retries: HashMap::new(),
                },
            );
        }

        self.fetch_chunks_loop(object_id).await?;

        self.store.get_object(object_id)
    }

    async fn ensure_providers_for_object(&self, object_id: &ObjectId, kind: ProviderKind) {
        let has_providers = {
            let provider_map = self.provider_map.read().await;
            provider_map
                .get(object_id)
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        };

        if !has_providers {
            tracing::info!("no providers in map, querying DHT for object {}", object_id);
            self.network.find_providers(&object_id.0, kind);

            for i in 0..5 {
                tokio::time::sleep(Duration::from_secs(1)).await;

                let provider_map = self.provider_map.read().await;
                if provider_map
                    .get(object_id)
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                {
                    tracing::info!("DHT returned providers after {} seconds", i + 1);
                    return;
                }
            }

            tracing::warn!("no DHT providers found after 5 seconds");
        }
    }

    async fn fetch_object_metadata(
        &self,
        object_id: &ObjectId,
        kind: ProviderKind,
    ) -> Result<crate::types::Object> {
        self.network.find_providers(&object_id.0, kind);

        tokio::time::sleep(Duration::from_secs(2)).await;

        let providers = self.get_providers_for_object(object_id).await;

        if providers.is_empty() {
            tracing::warn!("no DHT providers found, trying all connected peers");
            return self.fetch_metadata_from_any_peer(object_id).await;
        }

        for provider in providers.iter().take(3) {
            match self.request_metadata_from_peer(provider, object_id).await {
                Ok(object) => {
                    self.store.store_object(&object)?;
                    return Ok(object);
                }
                Err(e) => {
                    tracing::debug!("metadata request to {} failed: {e:?}", provider);
                    continue;
                }
            }
        }

        Err(anyhow!("all metadata providers failed"))
    }

    async fn fetch_metadata_from_any_peer(
        &self,
        object_id: &ObjectId,
    ) -> Result<crate::types::Object> {
        if let Some(object) = self.store.get_object_metadata(object_id)? {
            tracing::debug!("metadata already available locally");
            return Ok(object);
        }

        let all_peers = self.get_all_connected_peers().await;
        if all_peers.is_empty() {
            return Err(anyhow!("no connected peers"));
        }

        tracing::info!(
            "requesting metadata from {} connected peers via request-response",
            all_peers.len()
        );

        for peer in all_peers.iter().take(5) {
            match self.request_metadata_from_peer(peer, object_id).await {
                Ok(object) => {
                    tracing::info!("received metadata from peer {}", peer);
                    return Ok(object);
                }
                Err(e) => {
                    tracing::debug!("metadata request to {} failed: {e:?}", peer);
                    continue;
                }
            }
        }

        tracing::info!("request-response failed, falling back to gossipsub broadcast");

        let metadata_req_msg =
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectMetadataRequest(
                crate::network::unified_protocol::ObjectMetadataRequest {
                    object_id: *object_id,
                },
            );

        let network_msg = crate::network::NetworkMessage::UnifiedProtocol(metadata_req_msg);
        let _ = self.network.publisher.send(network_msg);

        for i in 0..6 {
            tokio::time::sleep(Duration::from_secs(1)).await;

            if let Some(object) = self.store.get_object_metadata(object_id)? {
                tracing::info!("received metadata via gossipsub after {} seconds", i + 1);
                return Ok(object);
            }
        }

        tracing::error!("no peer provided metadata after request-response and gossipsub attempts");
        Err(anyhow!("no peer provided metadata"))
    }

    async fn request_metadata_from_peer(
        &self,
        peer: &PeerId,
        object_id: &ObjectId,
    ) -> Result<crate::types::Object> {
        let req = crate::network::unified_protocol::ObjectMetadataRequest {
            object_id: *object_id,
        };

        let tx_req = crate::network::TransferRequest::Unified(
            crate::network::unified_protocol::UnifiedRequest::GetObjectMetadata(req),
        );

        self.network.request_transfer(*peer, tx_req);

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(300)).await;
            if let Some(object) = self.store.get_object_metadata(object_id)? {
                return Ok(object);
            }
        }

        Err(anyhow!("metadata not received from peer"))
    }

    async fn fetch_manifest(
        &self,
        object_id: &ObjectId,
        _manifest_id: &crate::types::ManifestId,
        kind: ProviderKind,
    ) -> Result<crate::types::Manifest> {
        self.network.find_providers(&object_id.0, kind);

        tokio::time::sleep(Duration::from_secs(2)).await;

        let providers = self.get_providers_for_object(object_id).await;

        if providers.is_empty() {
            tracing::warn!("no DHT providers for manifest, trying gossipsub broadcast");
            return self.fetch_manifest_from_any_peer(object_id).await;
        }

        for provider in providers.iter().take(3) {
            match self.request_manifest_from_peer(provider, object_id).await {
                Ok(manifest) => {
                    self.store.store_manifest(&manifest)?;
                    return Ok(manifest);
                }
                Err(e) => {
                    tracing::debug!("manifest request to {} failed: {e:?}", provider);
                    continue;
                }
            }
        }

        Err(anyhow!("all manifest providers failed"))
    }

    async fn fetch_manifest_from_any_peer(
        &self,
        object_id: &ObjectId,
    ) -> Result<crate::types::Manifest> {
        tracing::info!("broadcasting manifest request via gossipsub");

        let manifest_req_msg =
            crate::network::unified_protocol::UnifiedProtocolMessage::ManifestRequest(
                crate::network::unified_protocol::ManifestRequest {
                    object_id: *object_id,
                },
            );

        let network_msg = crate::network::NetworkMessage::UnifiedProtocol(manifest_req_msg);
        let _ = self.network.publisher.send(network_msg);

        for i in 0..10 {
            tokio::time::sleep(Duration::from_millis(500)).await;

            if let Some(manifest) = self.store.get_manifest_by_object(object_id)? {
                tracing::info!("received manifest after {} attempts", i + 1);
                return Ok(manifest);
            }
        }

        Err(anyhow!("no peer provided manifest"))
    }

    async fn request_manifest_from_peer(
        &self,
        peer: &PeerId,
        object_id: &ObjectId,
    ) -> Result<crate::types::Manifest> {
        let req = crate::network::unified_protocol::ManifestRequest {
            object_id: *object_id,
        };

        let tx_req = crate::network::TransferRequest::Unified(
            crate::network::unified_protocol::UnifiedRequest::GetManifest(req),
        );

        self.network.request_transfer(*peer, tx_req);

        let wait_duration = Duration::from_secs(5);
        tokio::time::sleep(wait_duration).await;

        if let Some(manifest) = self.store.get_manifest_by_object(object_id)? {
            return Ok(manifest);
        }

        Err(anyhow!("manifest not received"))
    }

    async fn fetch_chunks_loop(&self, object_id: &ObjectId) -> Result<()> {
        let mut tick = interval(Duration::from_millis(500));
        let max_iterations = 120;
        let mut iterations = 0;

        loop {
            tick.tick().await;
            iterations += 1;

            if iterations > max_iterations {
                return Err(anyhow!(
                    "chunk fetch timeout after {} iterations",
                    iterations
                ));
            }

            let (missing_count, in_flight_count) = {
                let pending = self.pending_objects.read().await;
                if let Some(obj) = pending.get(object_id) {
                    (obj.missing_chunks.len(), obj.in_flight.len())
                } else {
                    return Err(anyhow!("pending object disappeared"));
                }
            };

            if missing_count == 0 && in_flight_count == 0 {
                break;
            }

            let available_slots = MAX_CONCURRENT_CHUNK_FETCHES.saturating_sub(in_flight_count);

            for _ in 0..available_slots {
                let chunk_id = {
                    let mut pending = self.pending_objects.write().await;
                    if let Some(obj) = pending.get_mut(object_id) {
                        if let Some(chunk_id) = obj.missing_chunks.pop_front() {
                            obj.in_flight.insert(chunk_id);
                            chunk_id
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                };

                let distributor = self.clone_arc();
                let oid = *object_id;
                tokio::spawn(async move {
                    if let Err(e) = distributor.fetch_single_chunk(&oid, &chunk_id).await {
                        tracing::warn!("chunk fetch failed: {e:?}");
                        distributor.mark_chunk_failed(&oid, &chunk_id).await;
                    }
                });
            }
        }

        let mut pending = self.pending_objects.write().await;
        pending.remove(object_id);

        Ok(())
    }

    async fn fetch_single_chunk(&self, object_id: &ObjectId, chunk_id: &ChunkId) -> Result<()> {
        let mut providers = self.get_providers_for_object(object_id).await;

        if providers.is_empty() {
            tracing::debug!(
                "no providers in map for object {}, requesting from any peer",
                object_id
            );
            let all_peers: Vec<_> = self
                .provider_map
                .read()
                .await
                .values()
                .flat_map(|peers| peers.iter().copied())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            if !all_peers.is_empty() {
                providers = all_peers;
                tracing::debug!("using {} connected peers as fallback", providers.len());
            } else {
                return Err(anyhow!("no providers available"));
            }
        }

        for provider in providers.iter().take(3) {
            match self.request_chunk_from_peer(provider, chunk_id).await {
                Ok(chunk) => {
                    self.store.store_chunk(&chunk)?;
                    self.mark_chunk_completed(object_id, chunk_id).await;
                    return Ok(());
                }
                Err(e) => {
                    tracing::debug!("chunk request to {} failed: {e:?}", provider);
                    continue;
                }
            }
        }

        Err(anyhow!("all providers failed"))
    }

    async fn request_chunk_from_peer(&self, peer: &PeerId, chunk_id: &ChunkId) -> Result<Chunk> {
        let req = ChunkRequest {
            chunk_id: *chunk_id,
        };

        let tx_req = crate::network::TransferRequest::Unified(
            crate::network::unified_protocol::UnifiedRequest::GetChunk(req),
        );

        self.network.request_transfer(*peer, tx_req);

        let wait_duration = Duration::from_secs(5);
        tokio::time::sleep(wait_duration).await;

        if let Some(chunk) = self.store.get_chunk(chunk_id)? {
            return Ok(chunk);
        }

        Err(anyhow!("chunk not received"))
    }

    async fn mark_chunk_completed(&self, object_id: &ObjectId, chunk_id: &ChunkId) {
        let mut pending = self.pending_objects.write().await;
        if let Some(obj) = pending.get_mut(object_id) {
            obj.in_flight.remove(chunk_id);
        }
    }

    async fn mark_chunk_failed(&self, object_id: &ObjectId, chunk_id: &ChunkId) {
        let mut pending = self.pending_objects.write().await;
        if let Some(obj) = pending.get_mut(object_id) {
            obj.in_flight.remove(chunk_id);

            let retries = obj.retries.entry(*chunk_id).or_insert(0);
            *retries += 1;

            if *retries < 5 {
                obj.missing_chunks.push_back(*chunk_id);
            } else {
                tracing::error!("chunk {} exceeded max retries", chunk_id);
            }
        }
    }

    async fn get_providers_for_object(&self, object_id: &ObjectId) -> Vec<PeerId> {
        let provider_map = self.provider_map.read().await;
        provider_map
            .get(object_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    pub async fn handle_announcement(&self, announcement: ObjectAnnouncement) -> Result<()> {
        let mut providers = self.provider_map.write().await;
        let entry = providers
            .entry(announcement.object_id)
            .or_insert_with(HashSet::new);

        for provider_str in &announcement.providers {
            if let Ok(peer_id) = provider_str.parse::<PeerId>() {
                entry.insert(peer_id);
            }
        }

        Ok(())
    }

    fn clone_arc(&self) -> Arc<Self> {
        Arc::new(Self {
            store: Arc::clone(&self.store),
            network: self.network.clone(),
            pending_objects: Arc::clone(&self.pending_objects),
            provider_map: Arc::clone(&self.provider_map),
            chunk_requests: Arc::clone(&self.chunk_requests),
        })
    }

    async fn get_all_connected_peers(&self) -> Vec<PeerId> {
        self.network.get_connected_peers().await
    }
}
