//! Core consensus engine state and cross-module plumbing.

use super::types::{BlobSyncMode, DagConfig};
use crate::crypto::{bls::BlsSecretKey, keys::NodeKeys};
use crate::network::NetworkHandle;
use crate::qeue_manager::AsyncQueue;
use crate::storage::{BlobIndex, DagStore, ProgramCatalog, ProgramIndex, StateStore, UnifiedStore};
use crate::syncer::sync::SyncState;
use crate::types::{
    AggregatedReceipt, BlobId, CommitteeCertificate, NodeId, ObjectId, ProgramId, ReceiptId,
    StateWrite,
};
use crate::wasm_runtime::{ExecutionAdapter, ExecutionPool};
use anyhow::{anyhow, Context, Result};
use sled::Db;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct DagEngine {
    pub(super) unified_store: Arc<UnifiedStore>,
    #[allow(dead_code)]
    pub(super) state_store: Arc<StateStore>,
    #[allow(dead_code)]
    pub(super) execution_adapter: Arc<ExecutionAdapter>,
    #[allow(dead_code)]
    pub(super) blob_index: BlobIndex,
    #[allow(dead_code)]
    pub(super) program_index: ProgramIndex,
    #[allow(dead_code)]
    pub(super) dag_store: DagStore,
    #[allow(dead_code)]
    pub(super) scheduler: Arc<ExecutionPool>,
    #[allow(dead_code)]
    pub(super) identity: Arc<NodeKeys>,
    pub(super) network: NetworkHandle,
    pub(super) peers: Arc<RwLock<HashSet<libp2p::PeerId>>>,
    #[allow(dead_code)]
    pub(super) config: DagConfig,
    #[allow(dead_code)]
    pub(super) sync_state: Arc<RwLock<SyncState>>,
    #[allow(dead_code)]
    pub(super) pending_object_fetches: Arc<RwLock<HashSet<ObjectId>>>,
    #[allow(dead_code)]
    pub(super) fetch_queue: Arc<RwLock<Option<AsyncQueue<ObjectId>>>>,
    pub(super) job_sync: Arc<RwLock<Option<Arc<crate::syncer::JobSyncManager>>>>,
    pub(super) chunk_distributor: Option<Arc<crate::network::ChunkDistributor>>,
    pub(super) program_state_versions: Arc<RwLock<HashMap<ProgramId, u64>>>,
    pub(super) state_sync_in_progress: Arc<RwLock<HashSet<ProgramId>>>,
    pub(super) program_catalog: Arc<ProgramCatalog>,
    pub(super) bls_secret: BlsSecretKey,
    pub(super) bls_public: crate::crypto::bls::BlsPublicKey,
    pub(super) pending_transitions: Arc<RwLock<HashMap<ReceiptId, PendingTransition>>>,
    pub(super) last_voted: Arc<RwLock<HashMap<ProgramId, (u64, ReceiptId)>>>,
    pub(super) last_voted_store: sled::Tree,
    pub(super) finalized_transition_store: sled::Tree,
    pub(super) pending_leader_execs:
        Arc<RwLock<HashMap<[u8; 32], tokio::sync::oneshot::Sender<LeaderForwardResponse>>>>,
    pub(super) program_locks: Arc<RwLock<HashMap<ProgramId, Arc<tokio::sync::Mutex<()>>>>>,
    pub(super) last_broadcast_heads: Arc<RwLock<HashMap<ProgramId, (u64, [u8; 32])>>>,
}

pub(super) struct PendingTransition {
    pub committee: CommitteeCertificate,
    pub receipt: crate::types::ExecutionReceipt,
    pub state_writes: Vec<StateWrite>,
    pub signatures: HashMap<NodeId, crate::crypto::bls::BlsSignature>,
    pub completion: Option<tokio::sync::oneshot::Sender<Result<()>>>,
}

#[derive(Debug)]
pub(super) enum LeaderForwardResponse {
    Outcome(crate::wasm_runtime::ExecutionOutcome),
    Redirect(NodeId),
    Error(String),
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
        program_catalog: Arc<ProgramCatalog>,
        bls_secret: BlsSecretKey,
        bls_public: crate::crypto::bls::BlsPublicKey,
    ) -> Result<Self> {
        let last_voted_store = db.open_tree("consensus_last_voted")?;
        let finalized_transition_store = db.open_tree("consensus_finalized_transitions")?;
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
            program_catalog,
            bls_secret,
            bls_public,
            pending_transitions: Arc::new(RwLock::new(HashMap::new())),
            last_voted: Arc::new(RwLock::new(HashMap::new())),
            last_voted_store,
            finalized_transition_store,
            pending_leader_execs: Arc::new(RwLock::new(HashMap::new())),
            program_locks: Arc::new(RwLock::new(HashMap::new())),
            last_broadcast_heads: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn set_chunk_distributor(&mut self, distributor: Arc<crate::network::ChunkDistributor>) {
        self.chunk_distributor = Some(distributor);
    }

    pub async fn start_fetch_workers(&self, worker_count: usize) {
        if worker_count == 0 {
            return;
        }

        if self.fetch_queue.read().await.is_some() {
            return;
        }

        let Some(distributor) = self.chunk_distributor.clone() else {
            tracing::warn!("chunk distributor not configured; fetch workers not started");
            return;
        };

        let store = Arc::clone(&self.unified_store);
        let pending = Arc::clone(&self.pending_object_fetches);
        let queue = AsyncQueue::new(worker_count, move |object_id: ObjectId| {
            let distributor = distributor.clone();
            let store = store.clone();
            let pending = pending.clone();
            async move {
                if store.is_complete(&object_id).unwrap_or(false) {
                    pending.write().await.remove(&object_id);
                    return;
                }

                if let Err(err) = distributor
                    .fetch_object(&object_id, crate::network::ProviderKind::Program)
                    .await
                {
                    tracing::debug!("background fetch failed for {}: {}", object_id, err);
                }

                pending.write().await.remove(&object_id);
            }
        });

        *self.fetch_queue.write().await = Some(queue);
    }

    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    pub async fn request_inventory(&self) -> Result<()> {
        let mut peers = self.network.get_connected_peers().await;
        if peers.is_empty() {
            return Ok(());
        }
        peers.sort();

        // Request inventories from a bounded set of peers to avoid O(N^2) chatter on large networks.
        let max_peers = 8usize;
        for peer in peers.into_iter().take(max_peers) {
            self.network
                .request_transfer(peer, crate::network::TransferRequest::InventoryRequest);
        }
        Ok(())
    }

    pub async fn refresh_sync_state(&self) -> Result<()> {
        let last_inventory = { self.sync_state.read().await.last_inventory.clone() };
        let Some(inv) = last_inventory else {
            return Ok(());
        };

        let mut pending_programs: HashSet<ProgramId> = HashSet::new();
        let mut manifest_requests = 0usize;
        for pid_bytes in inv.programs {
            let pid = ProgramId(pid_bytes);
            let object_id = pid.to_object_id();
            let object_complete = self.unified_store.is_complete(&object_id).unwrap_or(false);
            if !object_complete {
                pending_programs.insert(pid.clone());
                let _ = self.queue_object_fetch(object_id).await;
            }

            let has_committee = self
                .program_catalog
                .get_manifest(&pid)?
                .and_then(|m| m.committee)
                .is_some();
            if !has_committee {
                pending_programs.insert(pid.clone());
                if manifest_requests < 8 {
                    let _ = self.request_program_manifest(&pid).await;
                    manifest_requests = manifest_requests.saturating_add(1);
                }
            }
        }

        let mut missing_blobs = 0usize;
        if matches!(self.config.blob_sync_mode, BlobSyncMode::FullData) {
            for blob_entry in inv.blobs {
                let bid = BlobId(blob_entry.id);
                let object_id = bid.to_object_id();
                if !self.unified_store.is_complete(&object_id).unwrap_or(false) {
                    missing_blobs += 1;
                    let _ = self.queue_object_fetch(object_id).await;
                }
            }
        }

        let mut state = self.sync_state.write().await;
        state.missing_programs = pending_programs.len();
        state.missing_blobs = missing_blobs;
        state.missing_execs = 0;
        state.pending_program_ids = pending_programs;
        state.pending_exec_ids.clear();
        state.last_seen = true;
        Ok(())
    }

    pub async fn request_program_manifest(&self, program_id: &ProgramId) -> Result<()> {
        let mut peers = self.network.get_connected_peers().await;
        if peers.is_empty() {
            return Ok(());
        }

        peers.sort();

        let req = crate::network::unified_protocol::ProgramManifestRequest {
            program_id: program_id.clone(),
        };
        let unified_req = crate::network::UnifiedRequest::GetProgramManifest(req);

        for peer in peers.into_iter().take(8) {
            self.network.request_transfer(
                peer,
                crate::network::TransferRequest::Unified(unified_req.clone()),
            );
        }

        Ok(())
    }

    pub async fn is_fully_synced(&self) -> bool {
        let state = self.sync_state.read().await;
        state.last_inventory.is_some()
            && state.missing_programs == 0
            && state.missing_blobs == 0
            && state.missing_execs == 0
    }

    pub async fn sync_gaps(&self) -> Option<(usize, usize, usize)> {
        let state = self.sync_state.read().await;
        if state.last_inventory.is_none() {
            return None;
        }
        Some((
            state.missing_programs,
            state.missing_blobs,
            state.missing_execs,
        ))
    }

    pub async fn set_job_sync(&self, job_sync: Arc<crate::syncer::JobSyncManager>) {
        *self.job_sync.write().await = Some(job_sync);
    }

    pub async fn ingest_aggregated_receipt(
        &self,
        receipt: AggregatedReceipt,
        committee: CommitteeCertificate,
    ) -> Result<()> {
        super::ReceiptVerifier::verify_aggregated_receipt(&receipt, &committee)?;
        self.program_catalog.store_aggregated_receipt(&receipt)?;
        Ok(())
    }

    pub(super) async fn load_last_voted(
        &self,
        program_id: &ProgramId,
    ) -> Result<Option<(u64, ReceiptId)>> {
        if let Some(found) = self.last_voted.read().await.get(program_id).copied() {
            return Ok(Some(found));
        }

        let Some(raw) = self.last_voted_store.get(program_id.0)? else {
            return Ok(None);
        };
        if raw.len() != 40 {
            return Err(anyhow!("invalid last_voted entry length"));
        }
        let mut height_bytes = [0u8; 8];
        height_bytes.copy_from_slice(&raw[..8]);
        let height = u64::from_be_bytes(height_bytes);
        let mut receipt_id = [0u8; 32];
        receipt_id.copy_from_slice(&raw[8..40]);

        let mut cache = self.last_voted.write().await;
        cache.insert(program_id.clone(), (height, receipt_id));
        Ok(Some((height, receipt_id)))
    }

    pub(super) async fn store_last_voted(
        &self,
        program_id: &ProgramId,
        height: u64,
        receipt_id: ReceiptId,
    ) -> Result<()> {
        let mut buf = [0u8; 40];
        buf[..8].copy_from_slice(&height.to_be_bytes());
        buf[8..40].copy_from_slice(&receipt_id);
        self.last_voted_store.insert(program_id.0, buf.as_slice())?;
        self.last_voted_store
            .flush()
            .context("flushing last_voted")?;
        self.last_voted
            .write()
            .await
            .insert(program_id.clone(), (height, receipt_id));
        Ok(())
    }

    pub(super) fn store_finalized_transition(
        &self,
        bundle: &crate::network::AggregatedReceiptBundle,
    ) -> Result<()> {
        let program_id = &bundle.receipt.receipt.program_id;
        let height = bundle.receipt.receipt.height;
        let mut key = Vec::with_capacity(32 + 8);
        key.extend_from_slice(&program_id.0);
        key.extend_from_slice(&height.to_be_bytes());
        let encoded = bincode::serde::encode_to_vec(bundle, bincode::config::standard())?;
        self.finalized_transition_store.insert(key, encoded)?;
        self.finalized_transition_store.flush()?;
        Ok(())
    }

    pub(super) fn load_finalized_transition(
        &self,
        program_id: &ProgramId,
        height: u64,
    ) -> Result<Option<crate::network::AggregatedReceiptBundle>> {
        let mut key = Vec::with_capacity(32 + 8);
        key.extend_from_slice(&program_id.0);
        key.extend_from_slice(&height.to_be_bytes());
        let Some(raw) = self.finalized_transition_store.get(key)? else {
            return Ok(None);
        };
        let (bundle, _): (crate::network::AggregatedReceiptBundle, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(bundle))
    }

    pub(super) async fn request_finalized_transition(
        &self,
        program_id: &ProgramId,
        height: u64,
    ) -> Result<()> {
        let peers: Vec<_> = self.peers.read().await.iter().copied().collect();
        if peers.is_empty() {
            return Ok(());
        }

        let req = crate::network::unified_protocol::FinalizedTransitionRequest {
            program_id: program_id.clone(),
            height,
        };
        let unified_req = crate::network::UnifiedRequest::GetFinalizedTransition(req);

        for peer in peers.iter().take(3) {
            self.network.request_transfer(
                *peer,
                crate::network::TransferRequest::Unified(unified_req.clone()),
            );
        }

        Ok(())
    }

    pub(super) async fn program_lock(&self, program_id: &ProgramId) -> Arc<tokio::sync::Mutex<()>> {
        {
            let locks = self.program_locks.read().await;
            if let Some(lock) = locks.get(program_id) {
                return Arc::clone(lock);
            }
        }

        let mut locks = self.program_locks.write().await;
        Arc::clone(
            locks
                .entry(program_id.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }

    async fn queue_object_fetch(&self, object_id: ObjectId) -> Result<()> {
        if self.unified_store.is_complete(&object_id).unwrap_or(false) {
            return Ok(());
        }

        {
            let mut pending = self.pending_object_fetches.write().await;
            if !pending.insert(object_id) {
                return Ok(());
            }
        }

        if let Some(queue) = self.fetch_queue.read().await.as_ref() {
            queue.enqueue(object_id);
            return Ok(());
        }

        let Some(distributor) = self.chunk_distributor.clone() else {
            self.pending_object_fetches.write().await.remove(&object_id);
            return Ok(());
        };

        let store = Arc::clone(&self.unified_store);
        let pending = Arc::clone(&self.pending_object_fetches);
        tokio::spawn(async move {
            if store.is_complete(&object_id).unwrap_or(false) {
                pending.write().await.remove(&object_id);
                return;
            }

            if let Err(err) = distributor
                .fetch_object(&object_id, crate::network::ProviderKind::Program)
                .await
            {
                tracing::debug!("background fetch failed for {}: {}", object_id, err);
            }

            pending.write().await.remove(&object_id);
        });

        Ok(())
    }
}
