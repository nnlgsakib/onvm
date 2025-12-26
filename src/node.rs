use crate::consensus::{BlobSyncMode, DagConfig, DagEngine};
use crate::crypto::keys::NodeKeys;
use crate::network::{
    ChunkDistributor, NetworkConfig, NetworkHandle, NetworkService, NetworkStreams,
};
use crate::storage::{ProgramCatalog, UnifiedStore};
use crate::syncer::SyncMan;
use crate::wasm_runtime::{ExecutionAdapter, ExecutionEngine, ExecutionPool, FuelEstimator};
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
    pub network: NetworkSecurityConfig,
    pub bootnodes: Vec<Multiaddr>,
}

#[derive(Clone, Debug)]
pub struct NetworkSecurityConfig {
    pub enable_mdns: bool,
    pub require_encryption: bool,
    pub max_inbound_connections: usize,
    pub max_inbound_streams: usize,
    pub max_gossip_bytes: usize,
}

pub struct Node {
    pub identity: Arc<NodeKeys>,
    pub unified_store: Arc<UnifiedStore>,
    pub state_store: Arc<crate::storage::StateStore>,
    pub execution_adapter: Arc<ExecutionAdapter>,
    pub chunk_distributor: Arc<ChunkDistributor>,
    pub execution: Arc<ExecutionEngine>,
    pub scheduler: Arc<ExecutionPool>,
    pub consensus: Arc<DagEngine>,
    pub fuel_estimator: Arc<FuelEstimator>,
    pub network: NetworkHandle,
    pub program_catalog: Arc<ProgramCatalog>,
    pub bls_public: crate::crypto::bls::BlsPublicKey,
    pub bls_secret: crate::crypto::bls::BlsSecretKey,
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
        let program_catalog = Arc::new(ProgramCatalog::new(db.clone())?);

        let state_store = Arc::new(crate::storage::StateStore::new(
            db.clone(),
            "contract_state",
        )?);

        let (bls_secret, bls_public) =
            crate::crypto::bls::load_identity(&config.data_dir, &identity)
                .await
                .with_context(|| {
                    format!(
                    "loading BLS identity (missing `bls_identity` under {}; run `onvm init` first)",
                    config.data_dir.display()
                )
                })?;

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
                    enable_mdns: config.network.enable_mdns,
                    require_encryption: config.network.require_encryption,
                    max_inbound_connections: config.network.max_inbound_connections,
                    max_inbound_streams: config.network.max_inbound_streams,
                    max_gossip_bytes: config.network.max_gossip_bytes,
                    bootnodes: config.bootnodes.clone(),
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
            program_catalog.clone(),
            bls_secret.clone(),
            bls_public.clone(),
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
            let consensus = consensus.clone();
            tokio::spawn(async move {
                let status_syncer = SyncMan::new(consensus.clone());
                tokio::spawn(async move {
                    status_syncer
                        .run_status_logger(std::time::Duration::from_secs(10))
                        .await;
                });

                let initial_syncer = SyncMan::new(consensus.clone());
                let _ = initial_syncer.await_initial_sync(None).await;
            })
        };

        Ok(Self {
            identity,
            unified_store,
            state_store,
            execution_adapter,
            chunk_distributor,
            execution: exec,
            scheduler,
            consensus,
            fuel_estimator,
            network,
            program_catalog,
            bls_public,
            bls_secret,
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
