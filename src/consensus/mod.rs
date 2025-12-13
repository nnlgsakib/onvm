use crate::crypto::hashing::hash_bytes;
use crate::crypto::keys::NodeKeys;
use crate::execution::{ExecutionOutcome, ExecutionScheduler, ProgramStore};
use crate::network::{
    BlobAdvertisement, BlobBroadcast, BlobInventoryEntry, BlobRequest, BloomFilter,
    ExecutionBroadcast, NetworkEvent, NetworkHandle, NetworkMessage, ProgramBroadcast,
    ProgramSyncRequest, ProviderKind, TransferRequest, TransferResponse, SyncSnapshot, SyncDelta,
};
use crate::storage::BlobStore;
use crate::storage::StateStore;
use crate::types::{BlobId, BlobMetadata, ComputeOp, NodeId, ProgramId, ProgramMetadata};
use anyhow::{anyhow, Context, Result};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use libp2p::request_response::ResponseChannel;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Operation {
    PublishBlob(BlobMetadata),
    DeployProgram(ProgramMetadata),
    Compute(ComputeOp),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DagRef {
    Program(ProgramId),
    Blob(BlobId),
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

const PROGRAM_META_BATCH: usize = 256;

pub struct DagEngine {
    blob_store: Arc<BlobStore>,
    state_store: Arc<StateStore>,
    blob_index: BlobIndex,
    program_store: Arc<ProgramStore>,
    program_index: ProgramIndex,
    dag_store: DagStore,
    scheduler: Arc<ExecutionScheduler>,
    identity: Arc<NodeKeys>,
    network: NetworkHandle,
    peers: Arc<RwLock<HashSet<PeerId>>>,
    config: DagConfig,
    sync_state: Arc<RwLock<SyncState>>,
}

impl DagEngine {
    pub fn new(
        db: Db,
        blob_store: Arc<BlobStore>,
        state_store: Arc<StateStore>,
        program_store: Arc<ProgramStore>,
        scheduler: Arc<ExecutionScheduler>,
        identity: Arc<NodeKeys>,
        network: NetworkHandle,
        config: DagConfig,
    ) -> Result<Self> {
        Ok(Self {
            blob_index: BlobIndex::new(&db)?,
            program_index: ProgramIndex::new(&db)?,
            dag_store: DagStore::new(&db)?,
            blob_store,
            state_store,
            program_store,
            scheduler,
            identity,
            network,
            peers: Arc::new(RwLock::new(HashSet::new())),
            config,
            sync_state: Arc::new(RwLock::new(SyncState::default())),
        })
    }

    pub async fn run(self: Arc<Self>, mut events: mpsc::UnboundedReceiver<NetworkEvent>) {
        let mut sync_timer = interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                Some(event) = events.recv() => {
                    if let Err(err) = self.handle_event(event).await {
                        tracing::warn!("dag event error: {err:?}");
                    }
                }
                _ = sync_timer.tick() => {
                    let _ = self.broadcast_inventory(false).await;
                    let _ = self.retry_missing().await;
                }
            }
        }
    }

    async fn handle_event(&self, event: NetworkEvent) -> Result<()> {
        match event {
            NetworkEvent::Inbound(peer, msg) => match msg {
                NetworkMessage::Program(bcast) => {
                    self.handle_program_broadcast(&peer, bcast).await?;
                }
                NetworkMessage::ProgramMeta(list) => {
                    self.handle_program_metadata(&peer, list).await?;
                }
                NetworkMessage::InventoryRequest => {
                    let _ = self.broadcast_inventory(true).await;
                }
                NetworkMessage::ProgramSyncRequest(req) => {
                    self.handle_program_sync_request(req).await?;
                }
                NetworkMessage::ProgramRequest(pid) => {
                    if let Some(meta) = self
                        .program_store
                        .metadata(&pid)
                        .context("program lookup")?
                    {
                        if let Ok(wasm) = self.program_store.load(&pid) {
                            let _ = self
                                .network
                                .publisher
                                .send(NetworkMessage::Program(ProgramBroadcast { meta, wasm }));
                        }
                    }
                }
                NetworkMessage::ProgramResponse(bcast) => {
                    self.handle_program_broadcast(&peer, bcast).await?;
                }
                NetworkMessage::Blob(bcast) => {
                    self.handle_blob_broadcast(&peer, bcast).await?;
                }
                NetworkMessage::BlobMeta(ad) => {
                    self.handle_blob_advertisement(&peer, ad).await?;
                }
                NetworkMessage::BlobRequest(req) => {
                    self.handle_blob_request(req).await?;
                }
                NetworkMessage::Execution(bcast) => {
                    self.handle_execution_broadcast(&peer, bcast).await?;
                }
                NetworkMessage::ExecutionRequest(ids) => {
                    self.handle_execution_request(ids).await?;
                }
                NetworkMessage::Inventory(inv) => {
                    self.handle_inventory(&peer, inv).await?;
                }
            },
            NetworkEvent::TransferRequest(peer, req, channel) => {
                self.handle_transfer_request(&peer, req, channel).await?;
            }
            NetworkEvent::TransferResponse(peer, resp) => {
                self.handle_transfer_response(&peer, resp).await?;
            }
            NetworkEvent::PeerConnected(peer) => {
                self.peers.write().await.insert(peer);
                let _ = self.broadcast_inventory(true).await;
                let _ = self.request_inventory().await;
                let _ = self.request_program_sync().await;
                // proactively push known programs to the newly connected peer
                let _ = self.push_all_programs_to_peer(peer).await;
                let _ = self.push_all_executions_to_peer(peer).await;
                let _ = self.push_program_meta_to_peer(peer).await;
                let _ = self.request_missing_executions(peer).await;
                let _ = self.send_sync_snapshot(peer).await;
            }
            NetworkEvent::PeerDisconnected(peer) => {
                self.peers.write().await.remove(&peer);
            }
            NetworkEvent::Listening(_) => {}
            NetworkEvent::ProvidersFound { key, kind, .. } => {
                self.handle_provider_hint(&key, kind).await?;
            }
        }
        Ok(())
    }

    pub async fn submit_execution(
        &self,
        program: &ProgramId,
        input: &[u8],
    ) -> Result<ExecutionOutcome> {
        if self.program_store.metadata(program)?.is_none() {
            return Err(anyhow!("program metadata missing locally"));
        }

        let input_meta = self
            .blob_store
            .put(input, None, self.identity.node_id.clone())?;
        self.blob_index
            .record(&input_meta, self.identity.node_id.clone(), true)?;
        self.network.provide(&input_meta.id.0);

        let outcome = self.scheduler.execute(program, input).await?;

        let output_meta =
            self.blob_store
                .put(&outcome.return_data, None, self.identity.node_id.clone())?;
        self.blob_index
            .record(&output_meta, self.identity.node_id.clone(), true)?;
        self.network.provide(&output_meta.id.0);

        let _ = self
            .network
            .publisher
            .send(NetworkMessage::Blob(crate::network::BlobBroadcast {
                meta: output_meta.clone(),
                data: outcome.return_data.clone(),
            }));
        self.push_blob_to_peers(None, crate::network::BlobBroadcast {
            meta: output_meta.clone(),
            data: outcome.return_data.clone(),
        }).await;

        let compute = ComputeOp {
            program_id: program.clone(),
            input: input_meta.id.clone(),
            output: output_meta.clone(),
            fuel_used: outcome.fuel_consumed,
            state_root: outcome.state_root,
            state_writes: outcome.state_writes.clone(),
        };
        let parents = vec![
            DagRef::Program(compute.program_id.clone()),
            DagRef::Blob(compute.input.clone()),
            DagRef::Blob(compute.output.id.clone()),
        ];
        let node = self.record_operation(Operation::Compute(compute.clone()), parents)?;
        let _ = self
            .network
            .publisher
            .send(NetworkMessage::Execution(ExecutionBroadcast {
                dag_id: node.id.0,
                op: compute.clone(),
            }));
        self.push_execution_to_peers(None, ExecutionBroadcast {
            dag_id: node.id.0,
            op: compute,
        })
        .await;
        Ok(outcome)
    }

    /// Used by local RPC paths to ingest and disseminate freshly created blobs.
    pub async fn ingest_local_blob(&self, meta: BlobMetadata, data: Vec<u8>) -> Result<()> {
        let bcast = crate::network::BlobBroadcast {
            meta: meta.clone(),
            data: data.clone(),
        };
        let _ = self.network.publisher.send(NetworkMessage::Blob(bcast.clone()));
        self.handle_blob_broadcast(&self.network.peer_id, bcast.clone())
            .await?;
        self.push_blob_to_peers(Some(self.network.peer_id), bcast).await;
        Ok(())
    }

    /// Used by local RPC paths to ingest and disseminate freshly deployed programs.
    pub async fn ingest_local_program(
        &self,
        meta: ProgramMetadata,
        wasm: Vec<u8>,
    ) -> Result<()> {
        let bcast = ProgramBroadcast {
            meta: meta.clone(),
            wasm: wasm.clone(),
        };
        let _ = self
            .network
            .publisher
            .send(NetworkMessage::Program(bcast.clone()));
        self.handle_program_broadcast(&self.network.peer_id, bcast.clone())
            .await?;
        self.push_program_to_peers(Some(self.network.peer_id), bcast)
            .await;
        Ok(())
    }

    fn record_operation(&self, op: Operation, parents: Vec<DagRef>) -> Result<DagNode> {
        let id = dag_id(&op, &parents);
        let node = DagNode {
            id: id.clone(),
            parents,
            op,
            timestamp_ms: now_ms(),
            publisher: self.identity.node_id.clone(),
        };
        let _ = self.dag_store.insert(&node)?;
        Ok(node)
    }

    async fn handle_program_broadcast(
        &self,
        peer: &PeerId,
        bcast: ProgramBroadcast,
    ) -> Result<()> {
        let already_present = self.program_store.metadata(&bcast.meta.id)?.is_some();
        if let Some(existing) = self.program_store.metadata(&bcast.meta.id)? {
            if existing.deploy_salt != bcast.meta.deploy_salt {
                return Err(anyhow!("program id collision with different salt"));
            }
        }
        self.program_store
            .replicate(&bcast.meta, &bcast.wasm)
            .context("replicate program")?;
        self.program_index
            .record(bcast.meta.id.clone(), bcast.meta.publisher.clone())?;
        self.network.provide(&bcast.meta.id.0);
        let parents = bcast
            .meta
            .blob_refs
            .iter()
            .cloned()
            .map(DagRef::Blob)
            .collect::<Vec<_>>();
        let op = Operation::DeployProgram(bcast.meta.clone());
        let _ = self.record_operation(op, parents)?;
        self.refresh_sync_state().await?;
        if !already_present {
            self.push_program_to_peers(Some(peer.clone()), bcast.clone()).await;
        }
        {
            let mut st = self.sync_state.write().await;
            st.pending_program_ids.remove(&bcast.meta.id);
        }
        Ok(())
    }

    async fn handle_program_metadata(&self, from: &PeerId, metas: Vec<ProgramMetadata>) -> Result<()> {
        for meta in metas {
            if self.program_store.metadata(&meta.id)?.is_some() {
                continue;
            }
            self.program_store.store_metadata(&meta)?;
            self.program_index
                .record(meta.id.clone(), meta.publisher.clone())?;
            self.network
                .request_transfer(*from, TransferRequest::Program(meta.id.clone()));
            let _ = self
                .network
                .publisher
                .send(NetworkMessage::ProgramRequest(meta.id.clone()));
            self.network
                .find_providers(&meta.id.0, crate::network::ProviderKind::Program);
        }
        self.refresh_sync_state().await?;
        Ok(())
    }

    async fn handle_program_sync_request(&self, req: ProgramSyncRequest) -> Result<()> {
        let mut missing = Vec::new();
        for meta in self.program_store.list()? {
            if !req.bloom.contains(&meta.id.0) {
                missing.push(meta);
            }
            if missing.len() >= PROGRAM_META_BATCH {
                break;
            }
        }
        if !missing.is_empty() {
            let _ = self
                .network
                .publisher
                .send(NetworkMessage::ProgramMeta(missing));
        }
        Ok(())
    }

    async fn handle_blob_broadcast(
        &self,
        peer: &PeerId,
        bcast: crate::network::BlobBroadcast,
    ) -> Result<()> {
        let had_blob = self.blob_store.metadata(&bcast.meta.id)?.is_some();
        match self.config.blob_sync_mode {
            BlobSyncMode::FullData => {
                self.blob_store
                    .replicate(&bcast.meta, &bcast.data)
                    .context("blob replicate")?;
                self.blob_index
                    .record(&bcast.meta, bcast.meta.publisher.clone(), true)?;
                self.network.provide(&bcast.meta.id.0);
            }
            BlobSyncMode::MetadataOnly => {
                self.blob_index
                    .record(&bcast.meta, bcast.meta.publisher.clone(), false)?;
            }
        }
        let op = Operation::PublishBlob(bcast.meta.clone());
        let _ = self.record_operation(op, Vec::new())?;
        self.refresh_sync_state().await?;
        if !had_blob {
            self.push_blob_to_peers(Some(peer.clone()), bcast).await;
        }
        Ok(())
    }

    async fn handle_blob_advertisement(&self, _peer: &PeerId, ad: BlobAdvertisement) -> Result<()> {
        self.blob_index
            .record(&ad.meta, ad.meta.publisher.clone(), ad.has_data)?;
        self.refresh_sync_state().await?;
        Ok(())
    }

    async fn handle_blob_request(&self, req: BlobRequest) -> Result<()> {
        for raw in req.ids {
            let mut id_bytes = [0u8; 32];
            id_bytes.copy_from_slice(&raw);
            let id = BlobId(id_bytes);
            if !req.want_data {
                if let Some(meta) = self.blob_store.metadata(&id)? {
                    let _ =
                        self.network
                            .publisher
                            .send(NetworkMessage::BlobMeta(BlobAdvertisement {
                                meta,
                                has_data: self.blob_store.get(&id).is_ok(),
                                locations: vec![self.identity.node_id.to_string()],
                            }));
                }
                continue;
            }
            if let Ok(data) = self.blob_store.get(&id) {
                if let Some(meta) = self.blob_store.metadata(&id)? {
                    let _ = self.network.publisher.send(NetworkMessage::Blob(
                        crate::network::BlobBroadcast { meta, data },
                    ));
                }
            }
        }
        Ok(())
    }

    async fn handle_execution_broadcast(
        &self,
        peer: &PeerId,
        bcast: ExecutionBroadcast,
    ) -> Result<()> {
        let op = Operation::Compute(bcast.op.clone());
        let parents = vec![
            DagRef::Program(bcast.op.program_id.clone()),
            DagRef::Blob(bcast.op.input.clone()),
            DagRef::Blob(bcast.op.output.id.clone()),
        ];
        let id = dag_id(&op, &parents);
        if id.0 != bcast.dag_id {
            return Err(anyhow!("execution dag id mismatch"));
        }
        if self.dag_store.contains(&id)? {
            return Ok(());
        }
        // apply state writes for this program in order
        for sw in &bcast.op.state_writes {
            self.state_store
                .set_scoped(&bcast.op.program_id.0, &sw.key, &sw.value)?;
        }
        // ensure output blob present or at least indexed
        if self.blob_store.metadata(&bcast.op.output.id)?.is_none() {
            self.blob_index
                .record(&bcast.op.output, bcast.op.output.publisher.clone(), false)?;
        }
        self.dag_store.insert(&DagNode {
            id,
            parents,
            op,
            timestamp_ms: now_ms(),
            publisher: self.identity.node_id.clone(),
        })?;
        self.refresh_sync_state().await?;
        self.push_execution_to_peers(Some(peer.clone()), bcast.clone()).await;
        {
            let mut st = self.sync_state.write().await;
            st.pending_exec_ids.remove(&bcast.dag_id);
        }
        Ok(())
    }

    async fn handle_execution_request(&self, ids: Vec<[u8; 32]>) -> Result<()> {
        for raw in ids {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&raw);
            let did = DagId(arr);
            if let Some(node) = self.dag_store.get(&did)? {
                if let Operation::Compute(op) = node.op {
                    let _ = self.network.publisher.send(NetworkMessage::Execution(
                        ExecutionBroadcast { dag_id: did.0, op },
                    ));
                }
            }
        }
        Ok(())
    }

    async fn handle_inventory(&self, from: &PeerId, inv: crate::network::DagInventory) -> Result<()> {
        let mut missing_programs = Vec::new();
        for raw in &inv.programs {
            let mut id_bytes = [0u8; 32];
            id_bytes.copy_from_slice(raw);
            let pid = ProgramId(id_bytes);
            if self.program_store.metadata(&pid)?.is_none() {
                missing_programs.push(pid);
            }
        }

        let mut missing_blobs = Vec::new();
        for entry in &inv.blobs {
            let mut id_bytes = [0u8; 32];
            id_bytes.copy_from_slice(&entry.id);
            let bid = BlobId(id_bytes);
            if self.blob_store.metadata(&bid)?.is_none() {
                missing_blobs.push(entry.clone());
            }
        }

        let mut missing_execs = Vec::new();
        for raw in &inv.executions {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(raw);
            let did = DagId(arr);
            if !self.dag_store.contains(&did)? {
                missing_execs.push(*raw);
            }
        }

        if !missing_programs.is_empty() {
            {
                let mut state = self.sync_state.write().await;
                for pid in &missing_programs {
                    state.pending_program_ids.insert(pid.clone());
                }
            }
            for pid in missing_programs {
                self.network
                    .request_transfer(*from, TransferRequest::Program(pid.clone()));
                let _ = self
                    .network
                    .publisher
                    .send(NetworkMessage::ProgramRequest(pid.clone()));
                self.network.find_providers(&pid.0, ProviderKind::Program);
            }
        } else if let Some(bloom) = inv.program_bloom.clone() {
            let local = self.program_store.list()?;
            for meta in local {
                if !bloom.contains(&meta.id.0) {
                    let _ = self
                        .network
                        .publisher
                        .send(NetworkMessage::ProgramRequest(meta.id.clone()));
                    self.network
                        .find_providers(&meta.id.0, ProviderKind::Program);
                }
            }
        }

        if !missing_blobs.is_empty() {
            let ids = missing_blobs.iter().map(|b| b.id).collect::<Vec<_>>();
            let want_data = matches!(self.config.blob_sync_mode, BlobSyncMode::FullData);
            let _ = self
                .network
                .publisher
                .send(NetworkMessage::BlobRequest(BlobRequest { ids, want_data }));
            for ad in &missing_blobs {
                let mut id_bytes = [0u8; 32];
                id_bytes.copy_from_slice(&ad.id);
                let bid = BlobId(id_bytes);
                self.network
                    .request_transfer(*from, TransferRequest::Blob(bid.clone()));
            }
            for ad in missing_blobs {
                let mut id_bytes = [0u8; 32];
                id_bytes.copy_from_slice(&ad.id);
                let bid = BlobId(id_bytes);
                let meta = BlobMetadata {
                    id: bid.clone(),
                    publisher: NodeId::new(&[]),
                    size: 0,
                    mime: None,
                    chunk_size: 0,
                    chunk_count: 0,
                    chunk_hashes: Vec::new(),
                    merkle_root: [0u8; 32],
                };
                let node_id = ad
                    .locations
                    .first()
                    .and_then(|s| hex::decode(s).ok())
                    .and_then(|bytes| {
                        let mut arr = [0u8; 32];
                        if bytes.len() == 32 {
                            arr.copy_from_slice(&bytes);
                            Some(NodeId(arr))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| self.identity.node_id.clone());
                self.blob_index.record(&meta, node_id, false)?;
                self.network
                    .find_providers(&id_bytes, crate::network::ProviderKind::Blob);
            }
        }

        if !missing_execs.is_empty() {
            {
                let mut state = self.sync_state.write().await;
                for did in &missing_execs {
                    state.pending_exec_ids.insert(*did);
                }
            }
            let _ = self
                .network
                .publisher
                .send(NetworkMessage::ExecutionRequest(missing_execs.clone()));
            for did in &missing_execs {
                self.network
                    .request_transfer(*from, TransferRequest::Execution(*did));
            }
        }
        self.update_sync_state(inv).await?;
        Ok(())
    }

    async fn handle_transfer_request(
        &self,
        peer: &PeerId,
        req: TransferRequest,
        channel: ResponseChannel<TransferResponse>,
    ) -> Result<()> {
        let response = match req {
            TransferRequest::Program(pid) => {
                let resp = match self.program_store.metadata(&pid) {
                    Ok(Some(meta)) => self
                        .program_store
                        .load(&pid)
                        .ok()
                        .map(|wasm| ProgramBroadcast { meta, wasm }),
                    _ => None,
                };
                TransferResponse::Program(resp)
            }
            TransferRequest::Blob(bid) => {
                let resp = match self.blob_store.metadata(&bid) {
                    Ok(Some(meta)) => self
                        .blob_store
                        .get(&bid)
                        .ok()
                        .map(|data| BlobBroadcast { meta, data }),
                    _ => None,
                };
                TransferResponse::Blob(resp)
            }
            TransferRequest::Execution(did) => {
                let mut id = [0u8; 32];
                id.copy_from_slice(&did);
                let resp = match self.dag_store.get(&DagId(id)) {
                    Ok(Some(node)) => {
                        if let Operation::Compute(op) = node.op {
                            Some(ExecutionBroadcast { dag_id: did, op })
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                TransferResponse::Execution(resp)
            }
            TransferRequest::PushProgram(bcast) => {
                self.handle_program_broadcast(peer, bcast.clone()).await?;
                TransferResponse::Program(None)
            }
            TransferRequest::PushBlob(bcast) => {
                self.handle_blob_broadcast(peer, bcast.clone()).await?;
                TransferResponse::Blob(None)
            }
            TransferRequest::PushExecution(bcast) => {
                self.handle_execution_broadcast(peer, bcast.clone()).await?;
                TransferResponse::Execution(None)
            }
            TransferRequest::Sync(snapshot) => {
                let (missing_programs, missing_execs) = self.diff_snapshot(&snapshot)?;
                // also proactively request missing items
                for pid in &missing_programs {
                    self.network
                        .request_transfer(*peer, TransferRequest::Program(pid.clone()));
                    self.network.find_providers(&pid.0, ProviderKind::Program);
                }
                for did in &missing_execs {
                    self.network
                        .request_transfer(*peer, TransferRequest::Execution(*did));
                }
                TransferResponse::Sync(SyncDelta {
                    missing_programs,
                    missing_executions: missing_execs,
                })
            }
        };
        self.network.respond_transfer(channel, response);
        Ok(())
    }

    async fn handle_transfer_response(
        &self,
        peer: &PeerId,
        resp: TransferResponse,
    ) -> Result<()> {
        match resp {
            TransferResponse::Program(Some(bcast)) => {
                self.handle_program_broadcast(peer, bcast.clone()).await?;
                let mut state = self.sync_state.write().await;
                state.pending_program_ids.remove(&bcast.meta.id);
                self.refresh_sync_state().await?;
            }
            TransferResponse::Blob(Some(bcast)) => {
                self.handle_blob_broadcast(peer, bcast).await?;
            }
            TransferResponse::Execution(Some(bcast)) => {
                self.handle_execution_broadcast(peer, bcast.clone()).await?;
                let mut state = self.sync_state.write().await;
                state.pending_exec_ids.remove(&bcast.dag_id);
                self.refresh_sync_state().await?;
            }
            TransferResponse::Sync(delta) => {
                {
                    let mut state = self.sync_state.write().await;
                    for pid in &delta.missing_programs {
                        state.pending_program_ids.insert(pid.clone());
                    }
                    for did in &delta.missing_executions {
                        state.pending_exec_ids.insert(*did);
                    }
                }
                // If delta is empty, we might already be in sync; refresh state.
                if delta.missing_programs.is_empty() && delta.missing_executions.is_empty() {
                    self.refresh_sync_state().await?;
                }
                for pid in delta.missing_programs {
                    self.network
                        .request_transfer(*peer, TransferRequest::Program(pid.clone()));
                    self.network.find_providers(&pid.0, ProviderKind::Program);
                }
                for did in delta.missing_executions {
                    self.network
                        .request_transfer(*peer, TransferRequest::Execution(did));
                }
            }
            _ => {}
        }
        if self.is_fully_synced().await {
            tracing::info!("sync complete: programs/blobs/executions fully present");
        }
        Ok(())
    }

    async fn push_program_to_peers(&self, exclude: Option<PeerId>, bcast: ProgramBroadcast) {
        let peers = self.peers.read().await.clone();
        for peer in peers {
            if exclude.as_ref().map(|p| p == &peer).unwrap_or(false) {
                continue;
            }
            self.network
                .request_transfer(peer, TransferRequest::PushProgram(bcast.clone()));
        }
    }

    async fn push_blob_to_peers(&self, exclude: Option<PeerId>, bcast: BlobBroadcast) {
        let peers = self.peers.read().await.clone();
        for peer in peers {
            if exclude.as_ref().map(|p| p == &peer).unwrap_or(false) {
                continue;
            }
            self.network
                .request_transfer(peer, TransferRequest::PushBlob(bcast.clone()));
        }
    }

    async fn push_execution_to_peers(&self, exclude: Option<PeerId>, bcast: ExecutionBroadcast) {
        let peers = self.peers.read().await.clone();
        for peer in peers {
            if exclude.as_ref().map(|p| p == &peer).unwrap_or(false) {
                continue;
            }
            self.network
                .request_transfer(peer, TransferRequest::PushExecution(bcast.clone()));
        }
    }

    async fn push_all_programs_to_peer(&self, peer: PeerId) -> Result<()> {
        for meta in self.program_store.list()? {
            if let Ok(wasm) = self.program_store.load(&meta.id) {
                self.network
                    .request_transfer(peer, TransferRequest::PushProgram(ProgramBroadcast { meta: meta.clone(), wasm }));
            }
        }
        Ok(())
    }

    async fn push_all_executions_to_peer(&self, peer: PeerId) -> Result<()> {
        for entry in self.dag_store.all_compute_ops()? {
            if let Operation::Compute(op) = entry.op.clone() {
                let bcast = ExecutionBroadcast {
                    dag_id: entry.id.0,
                    op: op.clone(),
                };
                let _ = self.network.publisher.send(NetworkMessage::Execution(bcast.clone()));
                // also direct push to the peer via transfer to reduce reliance on gossip
                self.network
                    .request_transfer(peer, TransferRequest::PushExecution(bcast));
            }
        }
        Ok(())
    }

    async fn push_program_meta_to_peer(&self, _peer: PeerId) -> Result<()> {
        let metas = self.program_store.list()?;
        if metas.is_empty() {
            return Ok(());
        }
        // send via gossip path so all connected peers get it
        let _ = self
            .network
            .publisher
            .send(NetworkMessage::ProgramMeta(metas));
        Ok(())
    }

    async fn request_missing_executions(&self, _peer: PeerId) -> Result<()> {
        let ids = self.dag_store.execution_ids()?;
        if ids.is_empty() {
            return Ok(());
        }
        let _ = self
            .network
            .publisher
            .send(NetworkMessage::ExecutionRequest(ids));
        Ok(())
    }

    async fn send_sync_snapshot(&self, peer: PeerId) -> Result<()> {
        let programs = self
            .program_store
            .list()?
            .into_iter()
            .map(|m| m.id)
            .collect::<Vec<_>>();
        let executions = self.dag_store.execution_ids()?;
        self.network
            .request_transfer(peer, TransferRequest::Sync(SyncSnapshot { programs, executions }));
        Ok(())
    }

    fn diff_snapshot(
        &self,
        snapshot: &SyncSnapshot,
    ) -> Result<(Vec<ProgramId>, Vec<[u8; 32]>)> {
        let mut missing_programs = Vec::new();
        for pid in &snapshot.programs {
            if self.program_store.metadata(pid)?.is_none() {
                missing_programs.push(pid.clone());
            }
        }
        let mut missing_execs = Vec::new();
        for did in &snapshot.executions {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(did);
            let did = DagId(arr);
            if !self.dag_store.contains(&did)? {
                missing_execs.push(did.0);
            }
        }
        Ok((missing_programs, missing_execs))
    }

    async fn retry_missing(&self) -> Result<()> {
        let peers = self.peers.read().await.clone();
        if peers.is_empty() {
            return Ok(());
        }
        let peer = *peers.iter().next().unwrap();
        let (programs, execs) = {
            let state = self.sync_state.read().await;
            (
                state.pending_program_ids.clone(),
                state.pending_exec_ids.clone(),
            )
        };
        for pid in programs {
            self.network
                .request_transfer(peer, TransferRequest::Program(pid));
        }
        for did in execs {
            self.network
                .request_transfer(peer, TransferRequest::Execution(did));
        }
        Ok(())
    }

    async fn broadcast_inventory(&self, force: bool) -> Result<()> {
        if !force && self.peers.read().await.len() < self.config.min_peers {
            return Ok(());
        }
        let programs: Vec<[u8; 32]> = self
            .program_store
            .list()?
            .into_iter()
            .map(|p| p.id.0)
            .collect();
        let program_bloom = Some(BloomFilter::from_programs(&programs));
        let blobs = self.blob_index.inventory_entries()?;
        let executions = self.dag_store.execution_ids()?;
        let inv = crate::network::DagInventory {
            programs,
            program_bloom,
            blobs,
            executions,
        };
        let _ = self.network.publisher.send(NetworkMessage::Inventory(inv));
        Ok(())
    }

    pub async fn request_inventory(&self) -> Result<()> {
        let _ = self
            .network
            .publisher
            .send(NetworkMessage::InventoryRequest);
        Ok(())
    }

    pub async fn request_program_sync(&self) -> Result<()> {
        let program_ids: Vec<[u8; 32]> = self
            .program_store
            .list()?
            .into_iter()
            .map(|p| p.id.0)
            .collect();
        let bloom = BloomFilter::from_programs(&program_ids);
        let _ = self
            .network
            .publisher
            .send(NetworkMessage::ProgramSyncRequest(ProgramSyncRequest {
                bloom,
            }));
        Ok(())
    }

    pub async fn is_fully_synced(&self) -> bool {
        let state = self.sync_state.read().await;
        state.last_seen
            && state.missing_programs == 0
            && state.missing_blobs == 0
            && state.missing_execs == 0
            && state.pending_program_ids.is_empty()
            && state.pending_exec_ids.is_empty()
    }

    pub async fn refresh_sync_state(&self) -> Result<()> {
        let inv = {
            let state = self.sync_state.read().await;
            state.last_inventory.clone()
        };
        if let Some(inv) = inv {
            self.update_sync_state(inv).await?;
        }
        Ok(())
    }

    async fn update_sync_state(&self, inv: crate::network::DagInventory) -> Result<()> {
        let missing_programs = inv
            .programs
            .iter()
            .filter(|raw| {
                let pid = ProgramId(**raw);
                self.program_store
                    .metadata(&pid)
                    .map(|m| m.is_none())
                    .unwrap_or(true)
            })
            .count();
        let missing_blobs = inv
            .blobs
            .iter()
            .filter(|entry| {
                let bid = BlobId(entry.id);
                self.blob_store
                    .metadata(&bid)
                    .map(|m| m.is_none())
                    .unwrap_or(true)
            })
            .count();
        let missing_execs = inv
            .executions
            .iter()
            .filter(|raw| {
                let did = DagId(**raw);
                !self.dag_store.contains(&did).unwrap_or(false)
            })
            .count();
        let mut state = self.sync_state.write().await;
        state.last_inventory = Some(inv);
        state.missing_programs = missing_programs;
        state.missing_blobs = missing_blobs;
        state.missing_execs = missing_execs;
        state.last_seen = true;
        Ok(())
    }

    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    pub async fn sync_gaps(&self) -> Option<(usize, usize, usize)> {
        let state = self.sync_state.read().await;
        state.last_inventory.as_ref().map(|_| {
            (
                state.missing_programs,
                state.missing_blobs,
                state.missing_execs,
            )
        })
    }

    async fn handle_provider_hint(&self, key: &[u8], kind: ProviderKind) -> Result<()> {
        if key.len() != 32 {
            return Ok(());
        }
        let mut id_bytes = [0u8; 32];
        id_bytes.copy_from_slice(key);
        match kind {
            ProviderKind::Program => {
                let pid = ProgramId(id_bytes);
                let _ = self
                    .network
                    .publisher
                    .send(NetworkMessage::ProgramRequest(pid));
            }
            ProviderKind::Blob => {
                let want_data = matches!(self.config.blob_sync_mode, BlobSyncMode::FullData);
                let _ = self
                    .network
                    .publisher
                    .send(NetworkMessage::BlobRequest(BlobRequest {
                        ids: vec![id_bytes],
                        want_data,
                    }));
                self.network.find_providers(&id_bytes, ProviderKind::Blob);
            }
        }
        Ok(())
    }
}

struct DagStore {
    tree: sled::Tree,
}

impl DagStore {
    fn new(db: &Db) -> Result<Self> {
        Ok(Self {
            tree: db.open_tree("dag_nodes")?,
        })
    }

    fn insert(&self, node: &DagNode) -> Result<bool> {
        let enc = bincode::serde::encode_to_vec(node, bincode::config::standard())?;
        let inserted = self.tree.insert(node.id.0, enc)?.is_none();
        self.tree.flush()?;
        Ok(inserted)
    }

    fn contains(&self, id: &DagId) -> Result<bool> {
        Ok(self.tree.contains_key(id.0)?)
    }

    fn get(&self, id: &DagId) -> Result<Option<DagNode>> {
        let Some(raw) = self.tree.get(id.0)? else {
            return Ok(None);
        };
        let (node, _): (DagNode, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(node))
    }

    fn execution_ids(&self) -> Result<Vec<[u8; 32]>> {
        let mut out = Vec::new();
        for entry in self.tree.iter() {
            let (_, v) = entry?;
            let (node, _): (DagNode, _) =
                bincode::serde::decode_from_slice(&v, bincode::config::standard())?;
            if matches!(node.op, Operation::Compute(_)) {
                out.push(node.id.0);
            }
        }
        Ok(out)
    }

    fn all_compute_ops(&self) -> Result<Vec<DagNode>> {
        let mut out = Vec::new();
        for entry in self.tree.iter() {
            let (_, v) = entry?;
            let (node, _): (DagNode, _) =
                bincode::serde::decode_from_slice(&v, bincode::config::standard())?;
            if matches!(node.op, Operation::Compute(_)) {
                out.push(node);
            }
        }
        Ok(out)
    }
}

struct BlobIndex {
    tree: sled::Tree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlobRecord {
    meta: BlobMetadata,
    locations: HashSet<NodeId>,
    has_data: bool,
}

impl BlobIndex {
    fn new(db: &Db) -> Result<Self> {
        Ok(Self {
            tree: db.open_tree("blob_index")?,
        })
    }

    fn record(&self, meta: &BlobMetadata, location: NodeId, has_data: bool) -> Result<()> {
        let mut rec = self.get(&meta.id)?.unwrap_or_else(|| BlobRecord {
            meta: meta.clone(),
            locations: HashSet::new(),
            has_data: false,
        });
        rec.meta = meta.clone();
        rec.locations.insert(location);
        rec.has_data = rec.has_data || has_data;
        let enc = bincode::serde::encode_to_vec(&rec, bincode::config::standard())?;
        self.tree.insert(meta.id.0, enc)?;
        self.tree.flush()?;
        Ok(())
    }

    fn get(&self, id: &BlobId) -> Result<Option<BlobRecord>> {
        let Some(raw) = self.tree.get(id.0)? else {
            return Ok(None);
        };
        let (rec, _): (BlobRecord, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(rec))
    }

    fn inventory_entries(&self) -> Result<Vec<BlobInventoryEntry>> {
        let mut out = Vec::new();
        for entry in self.tree.iter() {
            let (_, v) = entry?;
            let (rec, _): (BlobRecord, _) =
                bincode::serde::decode_from_slice(&v, bincode::config::standard())?;
            out.push(BlobInventoryEntry {
                id: rec.meta.id.0,
                has_data: rec.has_data,
                locations: rec.locations.iter().map(|n| n.to_string()).collect(),
            });
        }
        Ok(out)
    }
}

struct ProgramIndex {
    tree: sled::Tree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProgramRecord {
    id: ProgramId,
    locations: HashSet<NodeId>,
}

impl ProgramIndex {
    fn new(db: &Db) -> Result<Self> {
        Ok(Self {
            tree: db.open_tree("program_index")?,
        })
    }

    fn record(&self, id: ProgramId, location: NodeId) -> Result<()> {
        let mut rec = self.get(&id)?.unwrap_or_else(|| ProgramRecord {
            id: id.clone(),
            locations: HashSet::new(),
        });
        rec.locations.insert(location);
        let enc = bincode::serde::encode_to_vec(&rec, bincode::config::standard())?;
        self.tree.insert(id.0, enc)?;
        self.tree.flush()?;
        Ok(())
    }

    fn get(&self, id: &ProgramId) -> Result<Option<ProgramRecord>> {
        let Some(raw) = self.tree.get(id.0)? else {
            return Ok(None);
        };
        let (rec, _): (ProgramRecord, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(rec))
    }
}

#[derive(Default)]
struct SyncState {
    last_inventory: Option<crate::network::DagInventory>,
    missing_programs: usize,
    missing_blobs: usize,
    missing_execs: usize,
    pending_program_ids: HashSet<ProgramId>,
    pending_exec_ids: HashSet<[u8; 32]>,
    last_seen: bool,
}

impl BloomFilter {
    fn new(size_bytes: usize, k: u8) -> Self {
        Self {
            bits: vec![0u8; size_bytes],
            k,
        }
    }

    fn insert(&mut self, data: &[u8]) {
        for i in 0..self.k {
            let mut key = [0u8; 32];
            key[0] = i;
            let hash = blake3::keyed_hash(&key, data);
            self.set_bit(hash.as_bytes());
        }
    }

    fn contains(&self, data: &[u8]) -> bool {
        for i in 0..self.k {
            let mut key = [0u8; 32];
            key[0] = i;
            let hash = blake3::keyed_hash(&key, data);
            if !self.get_bit(hash.as_bytes()) {
                return false;
            }
        }
        true
    }

    fn set_bit(&mut self, hash: &[u8]) {
        let idx =
            (u64::from_le_bytes(hash[0..8].try_into().unwrap()) as usize) % (self.bits.len() * 8);
        let byte = idx / 8;
        let bit = idx % 8;
        self.bits[byte] |= 1 << bit;
    }

    fn get_bit(&self, hash: &[u8]) -> bool {
        let idx =
            (u64::from_le_bytes(hash[0..8].try_into().unwrap()) as usize) % (self.bits.len() * 8);
        let byte = idx / 8;
        let bit = idx % 8;
        (self.bits[byte] & (1 << bit)) != 0
    }

    fn from_programs(ids: &[[u8; 32]]) -> Self {
        let mut bloom = Self::new(256, 3);
        for id in ids {
            bloom.insert(id);
        }
        bloom
    }
}

fn dag_id(op: &Operation, parents: &[DagRef]) -> DagId {
    let mut buf = Vec::new();
    for p in parents {
        match p {
            DagRef::Program(id) => buf.extend_from_slice(&id.0),
            DagRef::Blob(id) => buf.extend_from_slice(&id.0),
            DagRef::Execution(id) => buf.extend_from_slice(&id.0),
        }
    }
    buf.extend(
        serde_json::to_vec(op)
            .expect("operation serializes")
            .as_slice(),
    );
    DagId(hash_bytes(&buf))
}

fn now_ms() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    now.as_millis() as u64
}
