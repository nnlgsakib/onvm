use super::types::DagConfig;
use crate::crypto::keys::NodeKeys;
use crate::network::NetworkHandle;
use crate::qeue_manager::AsyncQueue;
use crate::storage::{BlobIndex, DagStore, ProgramIndex, StateStore, UnifiedStore};
use crate::syncer::sync::SyncState;
use crate::types::{ObjectId, ProgramId};
use crate::wasm_runtime::{ExecutionAdapter, ExecutionPool};
use anyhow::Result;
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

    pub async fn start_fetch_workers(&self, _worker_count: usize) {}

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
