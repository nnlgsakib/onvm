use crate::consensus::{BlobSyncMode, DagConfig, DagEngine};
use crate::crypto::keys::NodeKeys;
use crate::wasm_runtime::{ExecutionAdapter, ExecutionEngine, ExecutionPool, FuelEstimator};
use crate::network::{ChunkDistributor, NetworkConfig, NetworkHandle, NetworkService, NetworkStreams};
use crate::storage::UnifiedStore;
use crate::syncer::SyncMan;
use anyhow::{Context, Result};
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct NodeConfig {
    pub data_dir: PathBuf,
    pub listen_addr: Multiaddr,
    pub rpc_bind: std::net::SocketAddr,
    pub min_peers: usize,
    pub blob_sync_mode: BlobSyncMode,
    pub identity: NodeKeys,
}

pub struct Node {
    pub identity: Arc<NodeKeys>,
    pub unified_store: Arc<UnifiedStore>,
    pub execution_adapter: Arc<ExecutionAdapter>,
    pub chunk_distributor: Arc<ChunkDistributor>,
    pub execution: Arc<ExecutionEngine>,
    pub scheduler: Arc<ExecutionPool>,
    pub consensus: Arc<DagEngine>,
    pub fuel_estimator: Arc<FuelEstimator>,
    pub network: NetworkHandle,
    pub db: sled::Db,
    #[allow(dead_code)]
    background: JoinHandle<()>,
    #[allow(dead_code)]
    sync_task: JoinHandle<()>,
}

impl Node {
    pub async fn start(config: NodeConfig) -> Result<Self> {
        let db_path = config.data_dir.join("db");

        tokio::fs::create_dir_all(&config.data_dir).await?;

        let db = sled::open(db_path).context("opening sled db")?;
        let identity = Arc::new(config.identity);
        
        let unified_store = Arc::new(UnifiedStore::new(db.clone())?);
        
        let state_store = Arc::new(crate::storage::StateStore::new(
            db.clone(),
            "contract_state",
        )?);
        
        let execution_adapter = Arc::new(ExecutionAdapter::new(unified_store.clone()));
        
        let exec = Arc::new(ExecutionEngine::new(
            unified_store.clone(),
            state_store.clone(),
            execution_adapter.clone(),
            crate::wasm_runtime::ExecutionConfig::default(),
        )?);
        let scheduler = Arc::new(ExecutionPool::new(exec.clone(), None));
        
        let fuel_estimator = Arc::new(FuelEstimator::new(
            unified_store.clone(),
            state_store.clone(),
        ));

        let mut listen_addr = config.listen_addr.clone();
        let streams = loop {
            match NetworkService::start(
                &identity,
                NetworkConfig {
                    listen_addr: listen_addr.clone(),
                    heartbeat: std::time::Duration::from_secs(1),
                },
            )
            .await
            {
                Ok(s) => break s,
                Err(err) => {
                    if let Some(next) = bump_tcp_port(&listen_addr) {
                        tracing::warn!(
                            "listen {} failed ({err}); trying next port {}",
                            listen_addr,
                            port_from_multiaddr(&next).unwrap_or(0)
                        );
                        listen_addr = next;
                    } else {
                        return Err(err);
                    }
                }
            }
        };

        let NetworkStreams {
            handle: network,
            events,
        } = streams;
        
        let chunk_distributor = Arc::new(ChunkDistributor::new(
            unified_store.clone(),
            network.clone(),
        ));

        let mut consensus = DagEngine::new(
            db.clone(),
            unified_store.clone(),
            state_store.clone(),
            execution_adapter.clone(),
            scheduler.clone(),
            identity.clone(),
            network.clone(),
            DagConfig {
                min_peers: config.min_peers,
                blob_sync_mode: config.blob_sync_mode.clone(),
            },
        )?;
        
        consensus.set_chunk_distributor(chunk_distributor.clone());
        let consensus = Arc::new(consensus);
        
        if matches!(config.blob_sync_mode, BlobSyncMode::FullData) {
            consensus.start_fetch_workers(5).await;
        }

        let consensus_task = {
            let c = consensus.clone();
            let ev = events;
            tokio::spawn(async move {
                c.run(ev).await;
            })
        };

        let sync_task = {
            let syncer = SyncMan::new(consensus.clone());
            tokio::spawn(async move {
                let _ = syncer
                    .await_initial_sync(std::time::Duration::from_secs(30))
                    .await;
            })
        };

        Ok(Self {
            identity,
            unified_store,
            execution_adapter,
            chunk_distributor,
            execution: exec,
            scheduler,
            consensus,
            fuel_estimator,
            network,
            db,
            background: consensus_task,
            sync_task,
        })
    }
}

fn bump_tcp_port(addr: &Multiaddr) -> Option<Multiaddr> {
    let mut new = Multiaddr::empty();
    let mut bumped = false;
    for proto in addr.iter() {
        match proto {
            Protocol::Tcp(port) => {
                if port == u16::MAX {
                    return None;
                }
                new.push(Protocol::Tcp(port + 1));
                bumped = true;
            }
            other => new.push(other),
        }
    }
    if bumped {
        Some(new)
    } else {
        None
    }
}

fn port_from_multiaddr(addr: &Multiaddr) -> Option<u16> {
    addr.iter().find_map(|p| match p {
        Protocol::Tcp(p) => Some(p),
        _ => None,
    })
}