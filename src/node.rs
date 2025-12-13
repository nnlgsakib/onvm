use crate::consensus::{BlobSyncMode, DagConfig, DagEngine};
use crate::crypto::keys::NodeKeys;
use crate::execution::{ ExecutionEngine, ExecutionScheduler, ProgramStore};
use crate::network::{NetworkConfig, NetworkHandle, NetworkService, NetworkStreams};
use crate::storage::BlobStore;
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
    pub blob_store: Arc<BlobStore>,
    pub program_store: Arc<ProgramStore>,
    pub execution: Arc<ExecutionEngine>,
    pub scheduler: Arc<ExecutionScheduler>,
    pub consensus: Arc<DagEngine>,
    pub network: NetworkHandle,
    #[allow(dead_code)]
    background: JoinHandle<()>,
    #[allow(dead_code)]
    sync_task: JoinHandle<()>,
}

impl Node {
    pub async fn start(config: NodeConfig) -> Result<Self> {
        let db_path = config.data_dir.join("db");
        let blob_path = config.data_dir.join("blobs");
        let program_path = config.data_dir.join("programs");

        tokio::fs::create_dir_all(&config.data_dir).await?;

        let db = sled::open(db_path).context("opening sled db")?;
        let identity = Arc::new(config.identity);
        let blob_store = Arc::new(BlobStore::new(db.clone(), blob_path)?);
        let program_store = Arc::new(ProgramStore::new(db.clone(), program_path)?);
        let state_store = Arc::new(crate::storage::StateStore::new(db.clone(), "contract_state")?);
        let exec = Arc::new(ExecutionEngine::new(
            blob_store.clone(),
            state_store.clone(),
            program_store.clone(),
            crate::execution::ExecutionConfig::default(),
        )?);
        let scheduler = Arc::new(ExecutionScheduler::new(exec.clone(), None));

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

        let consensus = Arc::new(DagEngine::new(
            db.clone(),
            blob_store.clone(),
            state_store.clone(),
            program_store.clone(),
            scheduler.clone(),
            identity.clone(),
            network.clone(),
            DagConfig {
                min_peers: config.min_peers,
                blob_sync_mode: config.blob_sync_mode.clone(),
            },
        )?);

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
            blob_store,
            program_store,
            execution: exec,
            scheduler,
            consensus,
            network,
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
