use crate::network::dht;
use crate::network::unified_protocol::{
    ChunkPartRequest, ChunkPartResponse, ChunkRequest, ChunkResponse, ManifestRequest,
    ManifestResponse, ObjectAnnouncement, ObjectMetadataRequest, ObjectMetadataResponse,
    UnifiedRequest, UnifiedResponse,
};
use crate::network::{NetworkHandle, ProviderKind, TransferRequest, TransferResponse};
use crate::storage::{UnifiedStore, CHUNK_SIZE_SMALL};
use crate::types::{Chunk, ChunkDescriptor, ChunkId, Manifest, Object, ObjectId};
use anyhow::{anyhow, Context, Result};
use futures::{stream, TryStreamExt};
use libp2p::PeerId;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

const SUBCHUNK_SIZE_BYTES: usize = CHUNK_SIZE_SMALL;
const MAX_CONCURRENT_CHUNK_FETCHES: usize = 8;
const MAX_PROVIDER_TRIES_PER_CHUNK: usize = 3;
const MAX_PROVIDER_TRIES_PER_PART: usize = 3;
const MAX_CHUNK_FETCH_ATTEMPTS: usize = 4;

const METADATA_RR_TIMEOUT: Duration = Duration::from_secs(5);
const MANIFEST_RR_TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK_RR_TIMEOUT: Duration = Duration::from_secs(15);

pub struct ChunkDistributor {
    store: Arc<UnifiedStore>,
    network: NetworkHandle,
    pub provider_map: Arc<RwLock<HashMap<ObjectId, HashSet<PeerId>>>>,
}

impl ChunkDistributor {
    pub fn new(store: Arc<UnifiedStore>, network: NetworkHandle) -> Self {
        Self {
            store,
            network,
            provider_map: Arc::new(RwLock::new(HashMap::new())),
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

        let _object = match self.store.get_object_metadata(object_id)? {
            Some(obj) => obj,
            None => {
                tracing::info!("object metadata not found locally, requesting from peers");
                self.fetch_object_metadata(object_id, provider_kind).await?
            }
        };

        let manifest = match self.store.get_manifest_by_object(object_id)? {
            Some(m) => m,
            None => {
                tracing::info!("manifest not found locally, requesting from peers");
                self.fetch_manifest(object_id, provider_kind).await?
            }
        };

        manifest.validate()?;

        let mut missing = Vec::new();
        for desc in &manifest.chunks {
            if !self.store.has_chunk(&desc.chunk_id)? {
                missing.push(desc.clone());
            }
        }

        if missing.is_empty() {
            let data = self.store.get_object(object_id)?;
            self.network.provide(&object_id.0);
            return Ok(data);
        }

        self.ensure_providers_for_object(object_id, provider_kind)
            .await;

        let distributor = self.clone_arc();
        let oid = *object_id;

        stream::iter(missing.into_iter().map(|d| Ok::<_, anyhow::Error>(d)))
            .try_for_each_concurrent(MAX_CONCURRENT_CHUNK_FETCHES, move |desc| {
                let distributor = distributor.clone();
                async move {
                    distributor
                        .fetch_single_chunk(&oid, desc, provider_kind)
                        .await
                }
            })
            .await?;

        let data = self
            .store
            .get_object(object_id)
            .context("reassembling object after chunk fetch")?;

        self.network.provide(&object_id.0);

        Ok(data)
    }

    async fn fetch_single_chunk(
        self: Arc<Self>,
        object_id: &ObjectId,
        chunk_desc: ChunkDescriptor,
        provider_kind: ProviderKind,
    ) -> Result<()> {
        let chunk_id = chunk_desc.chunk_id;

        if self.store.has_chunk(&chunk_id)? {
            return Ok(());
        }

        let chunk_size = chunk_desc.size as usize;

        for attempt in 0..MAX_CHUNK_FETCH_ATTEMPTS {
            let mut providers = self.get_providers_for_object(object_id).await;
            if providers.is_empty() {
                self.ensure_providers_for_object(object_id, provider_kind)
                    .await;
                providers = self.get_providers_for_object(object_id).await;
            }
            if providers.is_empty() {
                providers = self.get_all_connected_peers().await;
            }

            if providers.is_empty() {
                return Err(anyhow!("no providers available for {}", object_id));
            }

            match self
                .fetch_chunk_data_from_providers(&providers, chunk_id, chunk_size)
                .await
            {
                Ok(data) => {
                    let chunk = Chunk { id: chunk_id, data };
                    chunk.verify().context("chunk verification failed")?;
                    self.store.store_chunk(&chunk)?;
                    return Ok(());
                }
                Err(e) => {
                    tracing::debug!(
                        "chunk {} fetch attempt {}/{} failed: {e:?}",
                        chunk_id,
                        attempt + 1,
                        MAX_CHUNK_FETCH_ATTEMPTS
                    );
                    if attempt + 1 < MAX_CHUNK_FETCH_ATTEMPTS {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(anyhow!("chunk {} exceeded max retries", chunk_id))
    }

    async fn fetch_chunk_data_from_providers(
        &self,
        providers: &[PeerId],
        chunk_id: ChunkId,
        chunk_size: usize,
    ) -> Result<Vec<u8>> {
        if chunk_size <= SUBCHUNK_SIZE_BYTES {
            for peer in providers.iter().take(MAX_PROVIDER_TRIES_PER_CHUNK) {
                match self.request_chunk_from_peer(*peer, chunk_id).await {
                    Ok(data) => return Ok(data),
                    Err(e) => {
                        tracing::debug!("chunk request to {} failed: {e:?}", peer);
                        continue;
                    }
                }
            }
            return Err(anyhow!("all providers failed for chunk {}", chunk_id));
        }

        let mut out = vec![0u8; chunk_size];
        let mut offset = 0usize;
        while offset < chunk_size {
            let want_len = std::cmp::min(SUBCHUNK_SIZE_BYTES, chunk_size - offset);
            let mut got = None;

            for peer in providers.iter().take(MAX_PROVIDER_TRIES_PER_PART) {
                match self
                    .request_chunk_part_from_peer(*peer, chunk_id, offset, want_len)
                    .await
                {
                    Ok(part) => {
                        if part.len() != want_len {
                            tracing::debug!(
                                "provider {} returned wrong part length for {} off={}: expected {}, got {}",
                                peer,
                                chunk_id,
                                offset,
                                want_len,
                                part.len()
                            );
                            continue;
                        }
                        got = Some(part);
                        break;
                    }
                    Err(e) => {
                        tracing::debug!(
                            "chunk part request to {} failed (chunk {} off={}): {e:?}",
                            peer,
                            chunk_id,
                            offset
                        );
                        continue;
                    }
                }
            }

            let Some(part) = got else {
                return Err(anyhow!(
                    "all providers failed for chunk {} part off={}",
                    chunk_id,
                    offset
                ));
            };

            out[offset..offset + want_len].copy_from_slice(&part);
            offset += want_len;
        }

        Ok(out)
    }

    async fn request_chunk_from_peer(&self, peer: PeerId, chunk_id: ChunkId) -> Result<Vec<u8>> {
        let resp = self
            .request_unified(
                peer,
                UnifiedRequest::GetChunk(ChunkRequest { chunk_id }),
                CHUNK_RR_TIMEOUT,
            )
            .await?;

        let UnifiedResponse::Chunk(ChunkResponse {
            chunk_id: got_id,
            data,
        }) = resp
        else {
            return Err(anyhow!("unexpected response for GetChunk"));
        };

        if got_id != chunk_id {
            return Err(anyhow!("chunk id mismatch in response"));
        }

        data.ok_or_else(|| anyhow!("chunk not found on peer"))
    }

    async fn request_chunk_part_from_peer(
        &self,
        peer: PeerId,
        chunk_id: ChunkId,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>> {
        let resp = self
            .request_unified(
                peer,
                UnifiedRequest::GetChunkPart(ChunkPartRequest {
                    chunk_id,
                    offset: offset as u32,
                    length: length as u32,
                }),
                CHUNK_RR_TIMEOUT,
            )
            .await?;

        let UnifiedResponse::ChunkPart(ChunkPartResponse {
            chunk_id: got_id,
            offset: got_offset,
            data,
        }) = resp
        else {
            return Err(anyhow!("unexpected response for GetChunkPart"));
        };

        if got_id != chunk_id {
            return Err(anyhow!("chunk id mismatch in part response"));
        }
        if got_offset as usize != offset {
            return Err(anyhow!(
                "chunk part offset mismatch: expected {}, got {}",
                offset,
                got_offset
            ));
        }

        data.ok_or_else(|| anyhow!("chunk part not found on peer"))
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

            let deadline = Instant::now() + dht::FIND_PROVIDERS_MAX_WAIT;
            loop {
                let provider_map = self.provider_map.read().await;
                if provider_map
                    .get(object_id)
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                {
                    tracing::info!("DHT returned providers for {}", object_id);
                    return;
                }

                if Instant::now() >= deadline {
                    break;
                }

                drop(provider_map);
                tokio::time::sleep(dht::FIND_PROVIDERS_POLL_INTERVAL).await;
            }

            tracing::warn!(
                "no DHT providers found for {} after {:?}",
                object_id,
                dht::FIND_PROVIDERS_MAX_WAIT
            );
        }
    }

    async fn fetch_object_metadata(
        &self,
        object_id: &ObjectId,
        kind: ProviderKind,
    ) -> Result<Object> {
        self.network.find_providers(&object_id.0, kind);

        let deadline = Instant::now() + dht::FIND_PROVIDERS_MAX_WAIT;
        loop {
            if !self.get_providers_for_object(object_id).await.is_empty() {
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(dht::FIND_PROVIDERS_POLL_INTERVAL).await;
        }

        let mut peers = self.get_providers_for_object(object_id).await;
        if peers.is_empty() {
            peers = self.get_all_connected_peers().await;
        }
        if peers.is_empty() {
            return Err(anyhow!("no connected peers"));
        }

        for peer in peers.iter().take(5) {
            match self
                .request_object_metadata_from_peer(*peer, object_id)
                .await
            {
                Ok(object) => {
                    self.store.store_object(&object)?;
                    return Ok(object);
                }
                Err(e) => {
                    tracing::debug!("metadata request to {} failed: {e:?}", peer);
                    continue;
                }
            }
        }

        tracing::info!("request-response metadata failed; falling back to gossipsub broadcast");

        let metadata_req_msg =
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectMetadataRequest(
                ObjectMetadataRequest {
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

        Err(anyhow!("no peer provided metadata"))
    }

    async fn request_object_metadata_from_peer(
        &self,
        peer: PeerId,
        object_id: &ObjectId,
    ) -> Result<Object> {
        let resp = self
            .request_unified(
                peer,
                UnifiedRequest::GetObjectMetadata(ObjectMetadataRequest {
                    object_id: *object_id,
                }),
                METADATA_RR_TIMEOUT,
            )
            .await?;

        let UnifiedResponse::ObjectMetadata(ObjectMetadataResponse { metadata, .. }) = resp else {
            return Err(anyhow!("unexpected response for GetObjectMetadata"));
        };

        metadata.ok_or_else(|| anyhow!("peer returned no metadata"))
    }

    async fn fetch_manifest(&self, object_id: &ObjectId, kind: ProviderKind) -> Result<Manifest> {
        self.network.find_providers(&object_id.0, kind);

        let deadline = Instant::now() + dht::FIND_PROVIDERS_MAX_WAIT;
        loop {
            if !self.get_providers_for_object(object_id).await.is_empty() {
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(dht::FIND_PROVIDERS_POLL_INTERVAL).await;
        }

        let mut peers = self.get_providers_for_object(object_id).await;
        if peers.is_empty() {
            peers = self.get_all_connected_peers().await;
        }

        for peer in peers.iter().take(5) {
            match self.request_manifest_from_peer(*peer, object_id).await {
                Ok(manifest) => {
                    self.store.store_manifest(&manifest)?;
                    return Ok(manifest);
                }
                Err(e) => {
                    tracing::debug!("manifest request to {} failed: {e:?}", peer);
                    continue;
                }
            }
        }

        tracing::info!("broadcasting manifest request via gossipsub");

        let manifest_req_msg =
            crate::network::unified_protocol::UnifiedProtocolMessage::ManifestRequest(
                ManifestRequest {
                    object_id: *object_id,
                },
            );
        let network_msg = crate::network::NetworkMessage::UnifiedProtocol(manifest_req_msg);
        let _ = self.network.publisher.send(network_msg);

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if let Some(manifest) = self.store.get_manifest_by_object(object_id)? {
                return Ok(manifest);
            }
        }

        Err(anyhow!("no peer provided manifest"))
    }

    async fn request_manifest_from_peer(
        &self,
        peer: PeerId,
        object_id: &ObjectId,
    ) -> Result<Manifest> {
        let resp = self
            .request_unified(
                peer,
                UnifiedRequest::GetManifest(ManifestRequest {
                    object_id: *object_id,
                }),
                MANIFEST_RR_TIMEOUT,
            )
            .await?;

        let UnifiedResponse::Manifest(ManifestResponse { manifest }) = resp else {
            return Err(anyhow!("unexpected response for GetManifest"));
        };

        manifest.ok_or_else(|| anyhow!("peer returned no manifest"))
    }

    async fn request_unified(
        &self,
        peer: PeerId,
        req: UnifiedRequest,
        timeout: Duration,
    ) -> Result<UnifiedResponse> {
        let rx = self
            .network
            .request_transfer_oneshot(peer, TransferRequest::Unified(req))?;
        let resp = tokio::time::timeout(timeout, rx)
            .await
            .with_context(|| format!("transfer request timed out (peer={peer})"))?
            .with_context(|| format!("transfer response channel closed (peer={peer})"))??;

        let TransferResponse::Unified(unified) = resp else {
            return Err(anyhow!("unexpected transfer response type"));
        };

        Ok(unified)
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
            provider_map: Arc::clone(&self.provider_map),
        })
    }

    async fn get_all_connected_peers(&self) -> Vec<PeerId> {
        self.network.get_connected_peers().await
    }
}
