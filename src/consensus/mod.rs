use crate::crypto::keys::NodeKeys;
use crate::execution::{ExecutionAdapter, ExecutionOutcome, ExecutionPool};
use crate::network::{NetworkEvent, NetworkHandle, NetworkMessage};
use crate::qeue_manager::AsyncQueue;
use crate::storage::{StateStore, UnifiedStore, BlobIndex, DagStore, ProgramIndex};
use crate::syncer::sync::SyncState;
use crate::types::{BlobId, ComputeOp, NodeId, Object, ObjectId, ProgramId};
use anyhow::{anyhow, Context, Result};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Operation {
    PublishObject(Object),
    Compute(ComputeOp),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DagRef {
    Object(ObjectId),
    Execution(DagId),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DagId(pub [u8; 32]);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DagNode {
    pub id: DagId,
    pub parents: Vec<DagRef>,
    pub op: Operation,
    pub timestamp_ms: u64,
    pub publisher: NodeId,
}

#[derive(Clone, Debug)]
pub enum BlobSyncMode {
    FullData,
    MetadataOnly,
}

#[derive(Clone, Debug)]
pub struct DagConfig {
    pub min_peers: usize,
    pub blob_sync_mode: BlobSyncMode,
}

pub struct DagEngine {
    unified_store: Arc<UnifiedStore>,
    #[allow(dead_code)]
    state_store: Arc<StateStore>,
    #[allow(dead_code)]
    execution_adapter: Arc<ExecutionAdapter>,
    #[allow(dead_code)]
    blob_index: BlobIndex,
    #[allow(dead_code)]
    program_index: ProgramIndex,
    #[allow(dead_code)]
    dag_store: DagStore,
    #[allow(dead_code)]
    scheduler: Arc<ExecutionPool>,
    #[allow(dead_code)]
    identity: Arc<NodeKeys>,
    network: NetworkHandle,
    peers: Arc<RwLock<HashSet<PeerId>>>,
    #[allow(dead_code)]
    config: DagConfig,
    #[allow(dead_code)]
    sync_state: Arc<RwLock<SyncState>>,
    #[allow(dead_code)]
    pending_object_fetches: Arc<RwLock<HashSet<ObjectId>>>,
    #[allow(dead_code)]
    fetch_queue: Arc<RwLock<Option<AsyncQueue<ObjectId>>>>,
    job_sync: Arc<RwLock<Option<Arc<crate::syncer::JobSyncManager>>>>,
    chunk_distributor: Option<Arc<crate::network::ChunkDistributor>>,
    program_state_versions: Arc<RwLock<HashMap<ProgramId, u64>>>,
    state_sync_in_progress: Arc<RwLock<HashSet<ProgramId>>>,
}

impl DagEngine {
    pub fn new(
        db: Db,
        unified_store: Arc<UnifiedStore>,
        state_store: Arc<StateStore>,
        execution_adapter: Arc<ExecutionAdapter>,
        scheduler: Arc<ExecutionPool>,
        identity: Arc<NodeKeys>,
        network: NetworkHandle,
        config: DagConfig,
    ) -> Result<Self> {
        Ok(Self {
            blob_index: BlobIndex::new(&db)?,
            program_index: ProgramIndex::new(&db)?,
            dag_store: DagStore::new(&db)?,
            unified_store,
            state_store,
            execution_adapter,
            scheduler,
            identity,
            network,
            peers: Arc::new(RwLock::new(HashSet::new())),
            config,
            sync_state: Arc::new(RwLock::new(SyncState::default())),
            pending_object_fetches: Arc::new(RwLock::new(HashSet::new())),
            fetch_queue: Arc::new(RwLock::new(None)),
            job_sync: Arc::new(RwLock::new(None)),
            chunk_distributor: None,
            program_state_versions: Arc::new(RwLock::new(HashMap::new())),
            state_sync_in_progress: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    pub fn set_chunk_distributor(&mut self, distributor: Arc<crate::network::ChunkDistributor>) {
        self.chunk_distributor = Some(distributor);
    }

    pub async fn ingest_local_object(&self, object: Object, _data: Vec<u8>) -> Result<()> {
        self.network.provide(&object.id.0);
        
        let object_meta_msg = crate::network::unified_protocol::UnifiedProtocolMessage::ObjectMetadata(object.clone());
        let meta_network_msg = NetworkMessage::UnifiedProtocol(object_meta_msg);
        
        let _ = self.network.publisher.send(meta_network_msg);
        
        let announcement = crate::network::unified_protocol::ObjectAnnouncement::from_object(
            &object,
            &self.network.peer_id
        );
        
        let unified_msg = crate::network::unified_protocol::UnifiedProtocolMessage::ObjectAnnouncement(announcement);
        let msg = NetworkMessage::UnifiedProtocol(unified_msg);
        
        self.network.publisher.send(msg)
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

    pub async fn submit_execution(&self, program_id: &ProgramId, input: &[u8]) -> Result<ExecutionOutcome> {
        let object_id = program_id.to_object_id();
        
        if !self.unified_store.is_complete(&object_id)? {
            tracing::info!("program {} not available locally, attempting to fetch from network", program_id);
            
            if let Some(ref distributor) = self.chunk_distributor {
                match distributor.fetch_object(&object_id).await {
                    Ok(data) => {
                        tracing::info!(
                            "successfully fetched program {} from network ({} bytes)",
                            program_id,
                            data.len()
                        );
                    }
                    Err(e) => {
                        return Err(anyhow!(
                            "program not available locally and fetch from network failed: {}",
                            e
                        ));
                    }
                }
            } else {
                return Err(anyhow!("program not available and chunk distributor not initialized"));
            }
            
            if !self.unified_store.is_complete(&object_id)? {
                return Err(anyhow!("program fetch completed but object is still incomplete"));
            }
        }
        
        let has_state = {
            let versions = self.program_state_versions.read().await;
            versions.contains_key(program_id)
        };
        
        if !has_state {
            let in_progress = self.state_sync_in_progress.read().await.contains(program_id);
            
            if !in_progress {
                self.state_sync_in_progress.write().await.insert(program_id.clone());
                
                let state_store = Arc::clone(&self.state_store);
                let network = self.network.clone();
                let peers = Arc::clone(&self.peers);
                let versions = Arc::clone(&self.program_state_versions);
                let in_progress = Arc::clone(&self.state_sync_in_progress);
                let pid = program_id.clone();
                
                tokio::spawn(async move {
                    if let Err(e) = Self::background_state_sync_static(
                        &state_store,
                        &network,
                        &peers,
                        &versions,
                        &pid
                    ).await {
                        tracing::debug!("background state sync failed for {}: {}", pid, e);
                    }
                    in_progress.write().await.remove(&pid);
                });
            }
        }
        
        let outcome = self.scheduler.execute(program_id, input).await?;
        
        if !outcome.state_writes.is_empty() {
            let new_version = {
                let mut versions = self.program_state_versions.write().await;
                let version = versions.entry(program_id.clone()).or_insert(0);
                *version += 1;
                *version
            };
            
            let sync_msg = crate::network::StateSyncMessage {
                program_id: program_id.clone(),
                state_writes: outcome.state_writes.clone(),
                state_root: outcome.state_root,
                executor_node: self.identity.node_id.clone(),
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };
            
            let msg = NetworkMessage::StateSync(sync_msg);
            let _ = self.network.publisher.send(msg);
            
            tracing::debug!(
                "broadcasted {} state writes for program {} (version {})",
                outcome.state_writes.len(),
                program_id,
                new_version
            );
        }
        
        Ok(outcome)
    }

    async fn background_state_sync_static(
        state_store: &Arc<StateStore>,
        network: &NetworkHandle,
        peers: &Arc<RwLock<HashSet<PeerId>>>,
        versions: &Arc<RwLock<HashMap<ProgramId, u64>>>,
        program_id: &ProgramId,
    ) -> Result<()> {
        let peer_list: Vec<_> = peers.read().await.iter().copied().collect();
        
        if peer_list.is_empty() {
            return Err(anyhow!("no peers available"));
        }
        
        for peer in peer_list.iter().take(3) {
            let req = crate::network::StateRequest {
                program_id: program_id.clone(),
            };
            
            network.request_transfer(
                *peer,
                crate::network::TransferRequest::StateRequest(req),
            );
            
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            let state_count = state_store.get_all_scoped(&program_id.0)?.len();
            if state_count > 0 {
                versions.write().await.insert(program_id.clone(), 1);
                tracing::info!("background sync: fetched {} state entries for program {}", state_count, program_id);
                return Ok(());
            }
        }
        
        versions.write().await.insert(program_id.clone(), 0);
        tracing::debug!("background sync: no state found for program {}, marked as initialized", program_id);
        Ok(())
    }

    pub async fn fetch_blob(&self, blob_id: &BlobId) -> Result<Vec<u8>> {
        let object_id = blob_id.to_object_id();
        self.unified_store.get_object(&object_id)
    }

    pub async fn run(self: Arc<Self>, mut events: mpsc::UnboundedReceiver<NetworkEvent>) {
        let mut tick = interval(Duration::from_secs(5));
        let mut dht_announce_tick = interval(Duration::from_secs(300));

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Err(e) = self.periodic_sync().await {
                        tracing::warn!("periodic sync error: {e:?}");
                    }
                }
                _ = dht_announce_tick.tick() => {
                    if let Err(e) = self.periodic_dht_announce().await {
                        tracing::warn!("DHT announce error: {e:?}");
                    }
                }
                Some(event) = events.recv() => {
                    if let Err(e) = self.handle_event(event).await {
                        tracing::warn!("event handling error: {e:?}");
                    }
                }
            }
        }
    }

    async fn periodic_dht_announce(&self) -> Result<()> {
        let objects = self.unified_store.list_objects()?;
        
        tracing::info!("announcing {} objects to DHT", objects.len());
        
        for object in objects {
            if self.unified_store.is_complete(&object.id).unwrap_or(false) {
                self.network.provide(&object.id.0);
                tracing::debug!("announced object {} to DHT", object.id);
            }
        }
        
        Ok(())
    }

    async fn handle_event(&self, event: NetworkEvent) -> Result<()> {
        match event {
            NetworkEvent::PeerConnected(peer_id) => {
                self.peers.write().await.insert(peer_id);
                tracing::info!("peer connected: {}", peer_id);
            }
            NetworkEvent::PeerDisconnected(peer_id) => {
                self.peers.write().await.remove(&peer_id);
                tracing::info!("peer disconnected: {}", peer_id);
            }
            NetworkEvent::Inbound(_peer_id, msg) => {
                self.handle_network_message(msg).await?;
            }
            NetworkEvent::TransferRequest(peer_id, req, channel) => {
                self.handle_transfer_request(peer_id, req, channel).await?;
            }
            NetworkEvent::TransferResponse(_peer_id, resp) => {
                self.handle_transfer_response(resp).await?;
            }
            NetworkEvent::ProvidersFound { key, peers, kind } => {
                self.handle_providers_found(key, peers, kind).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_providers_found(
        &self,
        key: Vec<u8>,
        peers: Vec<PeerId>,
        _kind: crate::network::ProviderKind,
    ) -> Result<()> {
        if key.len() == 32 {
            let mut object_id_bytes = [0u8; 32];
            object_id_bytes.copy_from_slice(&key);
            let object_id = ObjectId(object_id_bytes);
            
            if peers.is_empty() {
                tracing::warn!("DHT query for {} returned 0 providers", object_id);
                return Ok(());
            }
            
            tracing::info!(
                "DHT found {} providers for object {}: {:?}",
                peers.len(),
                object_id,
                peers.iter().map(|p| p.to_string()).collect::<Vec<_>>()
            );
            
            if let Some(ref distributor) = self.chunk_distributor {
                let mut provider_map = distributor.provider_map.write().await;
                let entry = provider_map
                    .entry(object_id)
                    .or_insert_with(HashSet::new);
                    
                for peer in peers {
                    entry.insert(peer);
                }
                
                tracing::info!("added {} providers for object {}", entry.len(), object_id);
            }
        }
        Ok(())
    }

    async fn handle_transfer_response(&self, response: crate::network::TransferResponse) -> Result<()> {
        match response {
            crate::network::TransferResponse::Unified(unified_resp) => {
                use crate::network::unified_protocol::*;
                match unified_resp {
                    UnifiedResponse::Manifest(manifest_resp) => {
                        if let Some(manifest) = manifest_resp.manifest {
                            tracing::info!("received manifest for object {}", manifest.object_id);
                            let _ = self.unified_store.store_manifest(&manifest);
                        }
                    }
                    UnifiedResponse::Chunk(chunk_resp) => {
                        if let Some(data) = chunk_resp.data {
                            let chunk = crate::types::Chunk {
                                id: chunk_resp.chunk_id,
                                data,
                            };
                            tracing::debug!("received chunk {}", chunk_resp.chunk_id);
                            let _ = self.unified_store.store_chunk(&chunk);
                        }
                    }
                    UnifiedResponse::ObjectMetadata(metadata_resp) => {
                        if let Some(object) = metadata_resp.metadata {
                            tracing::info!("received object metadata for {}", object.id);
                            let _ = self.unified_store.store_object(&object);
                        }
                    }
                    _ => {}
                }
            }
            crate::network::TransferResponse::StateResponse(state_resp) => {
                tracing::info!(
                    "received state response for program {} ({} entries)",
                    state_resp.program_id,
                    state_resp.state_entries.len()
                );
                
                for (key, value) in &state_resp.state_entries {
                    let _ = self.state_store.set_scoped(&state_resp.program_id.0, key, value);
                }
                
                let local_root = self.state_store.root_scoped(&state_resp.program_id.0)?;
                if local_root == state_resp.state_root {
                    tracing::info!("state sync successful, root matches: {}", hex::encode(local_root));
                } else {
                    tracing::warn!(
                        "state root mismatch: expected {}, got {}",
                        hex::encode(state_resp.state_root),
                        hex::encode(local_root)
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_network_message(&self, msg: NetworkMessage) -> Result<()> {
        match msg {
            NetworkMessage::UnifiedProtocol(unified_msg) => {
                self.handle_unified_protocol(unified_msg).await?;
            }
            NetworkMessage::StateSync(state_sync) => {
                self.handle_state_sync(state_sync).await?;
            }
            _ => {
                tracing::debug!("received non-unified message");
            }
        }
        Ok(())
    }

    async fn handle_state_sync(&self, sync_msg: crate::network::StateSyncMessage) -> Result<()> {
        if sync_msg.executor_node == self.identity.node_id {
            return Ok(());
        }
        
        tracing::debug!(
            "received state sync for program {} with {} writes from node {}",
            sync_msg.program_id,
            sync_msg.state_writes.len(),
            sync_msg.executor_node
        );
        
        for write in &sync_msg.state_writes {
            self.state_store
                .set_scoped(&sync_msg.program_id.0, &write.key, &write.value)
                .context("applying remote state write")?;
        }
        
        let mut versions = self.program_state_versions.write().await;
        let version = versions.entry(sync_msg.program_id.clone()).or_insert(0);
        *version += 1;
        
        let local_root = self.state_store.root_scoped(&sync_msg.program_id.0)?;
        if local_root == sync_msg.state_root {
            tracing::debug!("state sync successful for program {} (version {})", sync_msg.program_id, version);
        } else {
            tracing::warn!(
                "state root mismatch for program {}: expected {}, got {}",
                sync_msg.program_id,
                hex::encode(sync_msg.state_root),
                hex::encode(local_root)
            );
        }
        
        Ok(())
    }

    async fn handle_unified_protocol(&self, msg: crate::network::unified_protocol::UnifiedProtocolMessage) -> Result<()> {
        match msg {
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectAnnouncement(announcement) => {
                tracing::info!(
                    "received object announcement: {} ({} bytes, {} chunks)",
                    announcement.object_id,
                    announcement.total_size,
                    announcement.chunk_count
                );
                
                if let Some(ref distributor) = self.chunk_distributor {
                    let _ = distributor.handle_announcement(announcement.clone()).await;
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectMetadata(object) => {
                let already_have = self.unified_store.get_object_metadata(&object.id).ok().flatten().is_some();
                let is_complete = self.unified_store.is_complete(&object.id).unwrap_or(false);
                
                if !already_have {
                    tracing::info!("received new object metadata: {}", object.id);
                    if let Err(e) = self.unified_store.store_object(&object) {
                        tracing::warn!("failed to store object metadata: {}", e);
                        return Ok(());
                    }
                    tracing::info!("object {} metadata stored", object.id);
                } else {
                    tracing::debug!("already have metadata for object {}", object.id);
                }
                
                if !is_complete && !already_have {
                    if let Some(ref distributor) = self.chunk_distributor {
                        tracing::info!("object {} not complete, triggering fetch", object.id);
                        
                        let distributor = distributor.clone();
                        let object_id = object.id;
                        tokio::spawn(async move {
                            match distributor.fetch_object(&object_id).await {
                                Ok(data) => {
                                    tracing::info!("successfully fetched object {} ({} bytes)", object_id, data.len());
                                }
                                Err(e) => {
                                    tracing::warn!("failed to fetch object {}: {}", object_id, e);
                                }
                            }
                        });
                    }
                } else if is_complete {
                    tracing::debug!("object {} already complete locally", object.id);
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectMetadataRequest(req) => {
                tracing::debug!("received metadata request for object {}", req.object_id);
                
                if let Ok(Some(object)) = self.unified_store.get_object_metadata(&req.object_id) {
                    if self.unified_store.is_complete(&req.object_id).unwrap_or(false) {
                        tracing::info!("responding with metadata for {}", req.object_id);
                        
                        let response_msg = crate::network::unified_protocol::UnifiedProtocolMessage::ObjectMetadata(object);
                        let network_msg = NetworkMessage::UnifiedProtocol(response_msg);
                        let _ = self.network.publisher.send(network_msg);
                    } else {
                        tracing::debug!("have metadata for {} but object incomplete", req.object_id);
                    }
                } else {
                    tracing::debug!("don't have metadata for {}", req.object_id);
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::ManifestRequest(req) => {
                tracing::debug!("received manifest request for object {}", req.object_id);
                
                if let Ok(Some(manifest)) = self.unified_store.get_manifest_by_object(&req.object_id) {
                    tracing::info!("responding with manifest for {}", req.object_id);
                    
                    let response_msg = crate::network::unified_protocol::UnifiedProtocolMessage::ManifestResponse(manifest);
                    let network_msg = NetworkMessage::UnifiedProtocol(response_msg);
                    let _ = self.network.publisher.send(network_msg);
                } else {
                    tracing::debug!("don't have manifest for {}", req.object_id);
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::ManifestResponse(manifest) => {
                tracing::info!("received manifest response for object {}", manifest.object_id);
                
                if self.unified_store.get_manifest_by_object(&manifest.object_id).ok().flatten().is_none() {
                    let _ = self.unified_store.store_manifest(&manifest);
                    tracing::info!("stored manifest for {}", manifest.object_id);
                }
            }
        }
        Ok(())
    }

    async fn handle_transfer_request(
        &self,
        peer_id: PeerId,
        req: crate::network::TransferRequest,
        channel: libp2p::request_response::ResponseChannel<crate::network::TransferResponse>,
    ) -> Result<()> {
        match req {
            crate::network::TransferRequest::Unified(unified_req) => {
                let response = self.handle_unified_request(unified_req).await?;
                self.network.respond_transfer(
                    channel,
                    crate::network::TransferResponse::Unified(response),
                );
            }
            crate::network::TransferRequest::StateRequest(state_req) => {
                let response = self.handle_state_request(state_req).await?;
                self.network.respond_transfer(
                    channel,
                    crate::network::TransferResponse::StateResponse(response),
                );
            }
            _ => {
                tracing::debug!("received non-unified transfer request from {}", peer_id);
                self.network.respond_transfer(
                    channel,
                    crate::network::TransferResponse::Ack,
                );
            }
        }
        Ok(())
    }

    async fn handle_state_request(&self, req: crate::network::StateRequest) -> Result<crate::network::StateResponse> {
        let state_entries = self.state_store.get_all_scoped(&req.program_id.0)?;
        let state_root = self.state_store.root_scoped(&req.program_id.0)?;
        
        tracing::info!(
            "responding to state request for program {} ({} entries, root: {})",
            req.program_id,
            state_entries.len(),
            hex::encode(state_root)
        );
        
        Ok(crate::network::StateResponse {
            program_id: req.program_id,
            state_root,
            state_entries,
        })
    }

    async fn handle_unified_request(
        &self,
        req: crate::network::unified_protocol::UnifiedRequest,
    ) -> Result<crate::network::unified_protocol::UnifiedResponse> {
        use crate::network::unified_protocol::*;
        
        match req {
            UnifiedRequest::GetChunk(chunk_req) => {
                let chunk_data = self.unified_store.get_chunk(&chunk_req.chunk_id)?;
                Ok(UnifiedResponse::Chunk(ChunkResponse {
                    chunk_id: chunk_req.chunk_id,
                    data: chunk_data.map(|c| c.data),
                }))
            }
            UnifiedRequest::GetManifest(manifest_req) => {
                let manifest = self.unified_store.get_manifest_by_object(&manifest_req.object_id)?;
                Ok(UnifiedResponse::Manifest(ManifestResponse { manifest }))
            }
            UnifiedRequest::GetChunks(batch_req) => {
                let chunks: Vec<_> = batch_req.chunk_ids
                    .iter()
                    .map(|cid| {
                        let data = self.unified_store.get_chunk(cid).ok().flatten().map(|c| c.data);
                        (*cid, data)
                    })
                    .collect();
                Ok(UnifiedResponse::Chunks(BatchChunkResponse { chunks }))
            }
            UnifiedRequest::GetObjectAvailability(avail_req) => {
                let has_object = self.unified_store.is_complete(&avail_req.object_id).unwrap_or(false);
                let manifest = self.unified_store.get_manifest_by_object(&avail_req.object_id).ok().flatten();
                let has_manifest = manifest.is_some();
                let metadata = self.unified_store.get_object_metadata(&avail_req.object_id).ok().flatten();
                
                let (available_chunks, missing_chunks) = if let Some(ref m) = manifest {
                    let missing = self.unified_store.get_missing_chunks(m).unwrap_or_default();
                    let available: Vec<_> = m.chunks.iter()
                        .filter(|c| !missing.contains(&c.chunk_id))
                        .map(|c| c.chunk_id)
                        .collect();
                    (available, missing)
                } else {
                    (Vec::new(), Vec::new())
                };
                
                Ok(UnifiedResponse::ObjectAvailability(ObjectAvailabilityResponse {
                    object_id: avail_req.object_id,
                    has_object,
                    has_manifest,
                    available_chunks,
                    missing_chunks,
                    metadata,
                }))
            }
            UnifiedRequest::GetObjectMetadata(metadata_req) => {
                let metadata = self.unified_store.get_object_metadata(&metadata_req.object_id).ok().flatten();
                Ok(UnifiedResponse::ObjectMetadata(ObjectMetadataResponse {
                    object_id: metadata_req.object_id,
                    metadata,
                }))
            }
        }
    }

    async fn periodic_sync(&self) -> Result<()> {
        Ok(())
    }

    pub async fn start_fetch_workers(&self, _worker_count: usize) {
    }

    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    pub async fn request_inventory(&self) -> Result<()> {
        Ok(())
    }

    pub async fn refresh_sync_state(&self) -> Result<()> {
        Ok(())
    }

    pub async fn is_fully_synced(&self) -> bool {
        true
    }

    pub async fn sync_gaps(&self) -> Option<(usize, usize, usize)> {
        None
    }

    pub async fn set_job_sync(&self, job_sync: Arc<crate::syncer::JobSyncManager>) {
        *self.job_sync.write().await = Some(job_sync);
    }
}