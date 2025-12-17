use crate::crypto::keys::NodeKeys;
use crate::types::{BlobId, BlobMetadata, ComputeOp, ProgramId, ProgramMetadata};
use anyhow::{anyhow, Context, Result};
use blake3;
use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic as Topic};
use libp2p::identity as libp2p_identity;
use libp2p::kad::{
    store::MemoryStore, Behaviour as Kademlia, Event as KademliaEvent, QueryId, QueryResult,
    RecordKey,
};
use libp2p::mdns;
use libp2p::noise;
use libp2p::request_response::{cbor, ProtocolSupport, ResponseChannel};
use libp2p::swarm::{NetworkBehaviour, StreamProtocol, SwarmEvent};
use libp2p::yamux;
use libp2p::{Multiaddr, PeerId, SwarmBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

pub const TOPIC_BLOBS: &str = "onvm-blobs";
pub const TOPIC_PROGRAMS: &str = "onvm-programs";
pub const TOPIC_BLOCKS: &str = "onvm-dag";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    Blob(BlobBroadcast),
    BlobMeta(BlobAdvertisement),
    Program(ProgramBroadcast),
    ProgramMeta(Vec<ProgramMetadata>),
    Execution(ExecutionBroadcast),
    Inventory(DagInventory),
    InventoryRequest,
    ProgramSyncRequest(ProgramSyncRequest),
    ProgramRequest(ProgramId),
    ProgramResponse(ProgramBroadcast),
    BlobRequest(BlobRequest),
    ExecutionRequest(Vec<[u8; 32]>),
    Job(crate::syncer::JobBroadcast),
    Capability(crate::network::coordination::NodeCapabilities),
    UnifiedProtocol(crate::network::unified_protocol::UnifiedProtocolMessage),
    StateSync(StateSyncMessage),
}

/// Direct transfer request/response messages used by request-response protocols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransferRequest {
    Program(ProgramId),
    ProgramChunk { id: ProgramId, chunk_idx: usize },
    Blob(BlobId),
    BlobChunk { id: BlobId, chunk_idx: u32 },
    Execution([u8; 32]),
    PushProgram(ProgramBroadcast),
    PushProgramChunk { id: ProgramId, chunk_idx: usize, chunk_data: Vec<u8> },
    PushBlob(BlobBroadcast),
    PushExecution(ExecutionBroadcast),
    Sync(SyncSnapshot),
    Unified(crate::network::unified_protocol::UnifiedRequest),
    StateRequest(StateRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransferResponse {
    Program(Option<ProgramBroadcast>),
    ProgramChunk {
        id: ProgramId,
        chunk_idx: usize,
        chunk_data: Option<Vec<u8>>,
    },
    Blob(Option<BlobBroadcast>),
    BlobChunk {
        id: BlobId,
        chunk_idx: u32,
        shards: Option<Vec<(u8, Vec<u8>)>>,
    },
    Execution(Option<ExecutionBroadcast>),
    Sync(SyncDelta),
    Ack,
    Unified(crate::network::unified_protocol::UnifiedResponse),
    StateResponse(StateResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSnapshot {
    pub programs: Vec<ProgramId>,
    pub executions: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDelta {
    pub missing_programs: Vec<ProgramId>,
    pub missing_executions: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobBroadcast {
    pub meta: BlobMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobAdvertisement {
    pub meta: BlobMetadata,
    pub has_data: bool,
    pub locations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramBroadcast {
    pub meta: ProgramMetadata,
    pub wasm: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionBroadcast {
    pub dag_id: [u8; 32],
    pub op: ComputeOp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSyncMessage {
    pub program_id: ProgramId,
    pub state_writes: Vec<crate::types::StateWrite>,
    pub state_root: [u8; 32],
    pub executor_node: crate::types::NodeId,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateRequest {
    pub program_id: ProgramId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateResponse {
    pub program_id: ProgramId,
    pub state_root: [u8; 32],
    pub state_entries: Vec<(Vec<u8>, Vec<u8>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobInventoryEntry {
    pub id: [u8; 32],
    pub has_data: bool,
    pub locations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagInventory {
    pub programs: Vec<[u8; 32]>,
    pub program_bloom: Option<BloomFilter>,
    pub blobs: Vec<BlobInventoryEntry>,
    pub executions: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobRequest {
    pub ids: Vec<[u8; 32]>,
    pub want_data: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloomFilter {
    pub bits: Vec<u8>,
    pub k: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramSyncRequest {
    pub bloom: BloomFilter,
}

impl BloomFilter {
    pub fn new(size_bytes: usize, k: u8) -> Self {
        Self {
            bits: vec![0u8; size_bytes],
            k,
        }
    }

    pub fn insert(&mut self, data: &[u8]) {
        for i in 0..self.k {
            let mut key = [0u8; 32];
            key[0] = i;
            let hash = blake3::keyed_hash(&key, data);
            self.set_bit(hash.as_bytes());
        }
    }

    pub fn contains(&self, data: &[u8]) -> bool {
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

    pub fn from_programs(ids: &[[u8; 32]]) -> Self {
        let mut bloom = Self::new(256, 3);
        for id in ids {
            bloom.insert(id);
        }
        bloom
    }
}

#[derive(Debug)]
pub enum NetworkEvent {
    Inbound(PeerId, NetworkMessage),
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    Listening(Multiaddr),
    TransferRequest(PeerId, TransferRequest, ResponseChannel<TransferResponse>),
    TransferResponse(PeerId, TransferResponse),
    ProvidersFound {
        key: Vec<u8>,
        peers: Vec<PeerId>,
        kind: ProviderKind,
    },
}

#[derive(Clone)]
pub struct NetworkHandle {
    pub peer_id: PeerId,
    pub publisher: mpsc::UnboundedSender<NetworkMessage>,
    cmd: mpsc::UnboundedSender<KadCommand>,
    transfer_req: mpsc::UnboundedSender<TransferJob>,
    transfer_resp: mpsc::UnboundedSender<TransferResponseJob>,
    peers: Arc<RwLock<HashSet<PeerId>>>,
}

impl NetworkHandle {
    pub fn provide(&self, key: &[u8]) {
        let _ = self.cmd.send(KadCommand::Provide(key.to_vec()));
    }

    pub fn find_providers(&self, key: &[u8], kind: ProviderKind) {
        let _ = self.cmd.send(KadCommand::FindProviders(key.to_vec(), kind));
    }

    pub fn request_transfer(&self, peer: PeerId, req: TransferRequest) {
        let _ = self.transfer_req.send(TransferJob { peer, req });
    }

    pub fn respond_transfer(
        &self,
        channel: ResponseChannel<TransferResponse>,
        response: TransferResponse,
    ) {
        let _ = self
            .transfer_resp
            .send(TransferResponseJob { channel, response });
    }

    pub async fn get_connected_peers(&self) -> Vec<PeerId> {
        let peers = self.peers.read().await;
        peers.iter().copied().collect()
    }
}

pub struct NetworkStreams {
    pub handle: NetworkHandle,
    pub events: mpsc::UnboundedReceiver<NetworkEvent>,
}

pub struct NetworkConfig {
    pub listen_addr: Multiaddr,
    pub heartbeat: Duration,
}

#[derive(Debug, Clone)]
pub enum ProviderKind {
    Program,
    Blob,
}

enum KadCommand {
    Provide(Vec<u8>),
    FindProviders(Vec<u8>, ProviderKind),
}

#[derive(Debug, Clone)]
struct TransferJob {
    peer: PeerId,
    req: TransferRequest,
}

#[derive(Debug)]
struct TransferResponseJob {
    channel: ResponseChannel<TransferResponse>,
    response: TransferResponse,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    kademlia: Kademlia<MemoryStore>,
    transfer: cbor::Behaviour<TransferRequest, TransferResponse>,
}

pub struct NetworkService;

impl NetworkService {
    pub async fn start(identity: &NodeKeys, config: NetworkConfig) -> Result<NetworkStreams> {
        let secret =
            libp2p_identity::ed25519::SecretKey::try_from_bytes(identity.keypair.secret.to_bytes())
                .context("libp2p secret")?;
        let ed_kp = libp2p_identity::ed25519::Keypair::from(secret);
        let local_key = libp2p_identity::Keypair::from(ed_kp);
        let peer_id = PeerId::from(local_key.public());

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .validation_mode(gossipsub::ValidationMode::Strict)
            .message_id_fn(|m: &gossipsub::Message| {
                let hash = blake3::hash(&m.data);
                gossipsub::MessageId::from(hash.as_bytes().to_vec())
            })
            .heartbeat_interval(config.heartbeat)
            .build()
            .context("building gossipsub config")?;
        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
        .map_err(|e| anyhow!("{e}"))?;

        gossipsub.subscribe(&Topic::new(TOPIC_BLOBS))?;
        gossipsub.subscribe(&Topic::new(TOPIC_PROGRAMS))?;
        gossipsub.subscribe(&Topic::new(TOPIC_BLOCKS))?;

        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;
        let store = MemoryStore::new(peer_id);
        
        let mut kad_config = libp2p::kad::Config::default();
        kad_config
            .set_provider_record_ttl(Some(Duration::from_secs(3600)))
            .set_provider_publication_interval(Some(Duration::from_secs(600)));
        
        let mut kademlia = Kademlia::with_config(peer_id, store, kad_config);
        kademlia.set_mode(Some(libp2p::kad::Mode::Server));
        
        let transfer_protocol = StreamProtocol::new("/onvm/transfer/1.0.0");
        let transfer_config = libp2p::request_response::Config::default()
            .with_request_timeout(Duration::from_secs(60))
            .with_max_concurrent_streams(256);
        let transfer = cbor::Behaviour::new(
            std::iter::once((transfer_protocol, ProtocolSupport::Full)),
            transfer_config,
        );
        let behaviour = Behaviour {
            gossipsub,
            mdns,
            kademlia,
            transfer,
        };

        let mut swarm = SwarmBuilder::with_existing_identity(local_key)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default().nodelay(true),
                (libp2p::tls::Config::new, noise::Config::new),
                yamux::Config::default,
            )?
            .with_behaviour(|_| behaviour)?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(30)))
            .build();
        let peer_id = *swarm.local_peer_id();

        tracing::info!("libp2p node initialized as {}", peer_id);

        swarm
            .listen_on(config.listen_addr.clone())
            .context("listen_on")?;

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (publish_tx, mut publish_rx) = mpsc::unbounded_channel();
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let (transfer_tx, mut transfer_rx) = mpsc::unbounded_channel::<TransferJob>();
        let (transfer_resp_tx, mut transfer_resp_rx) =
            mpsc::unbounded_channel::<TransferResponseJob>();
        let mut pending_queries: HashMap<QueryId, ProviderKind> = HashMap::new();
        let mut recent_ids: std::collections::VecDeque<gossipsub::MessageId> =
            std::collections::VecDeque::new();
        let dedupe_window: usize = 256;

        let peers_shared = Arc::new(RwLock::new(HashSet::new()));
        let peers_clone = Arc::clone(&peers_shared);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    swarm_event = swarm.select_next_some() => {
                        match swarm_event {
                            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(ev)) => {
                                if let gossipsub::Event::Message {
                                    propagation_source,
                                    message,
                                    ..
                                } = ev
                                {
                                    if let Ok(msg) = serde_json::from_slice::<NetworkMessage>(&message.data) {
                                        tracing::info!("inbound {:?} from {}", describe_msg(&msg), propagation_source);
                                        let _ = event_tx.send(NetworkEvent::Inbound(propagation_source, msg));
                                    }
                                }
                            }
                            SwarmEvent::Behaviour(BehaviourEvent::Mdns(ev)) => match ev {
                                mdns::Event::Discovered(list) => {
                                    for (peer, _addr) in list {
                                        tracing::info!("mDNS discovered peer {}, attempting dial", peer);
                                        let _ = swarm.dial(peer);
                                    }
                                }
                                mdns::Event::Expired(list) => {
                                    for (peer, _addr) in list {
                                        tracing::debug!("mDNS expired peer {}", peer);
                                    }
                                }
                            },
                            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(ev)) => if let KademliaEvent::OutboundQueryProgressed { id, result, .. } = ev {
                                if let Some(kind) = pending_queries.get(&id).cloned() {
                                    match result {
                                        QueryResult::GetProviders(Ok(libp2p::kad::GetProvidersOk::FoundProviders { key, providers })) => {
                                            let peers: Vec<_> = providers.into_iter().collect();
                                            let _ = event_tx.send(NetworkEvent::ProvidersFound {
                                                key: key.to_vec(),
                                                peers,
                                                kind,
                                            });
                                        }
                                        QueryResult::GetProviders(Ok(libp2p::kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. }))
                                        | QueryResult::GetProviders(Err(_)) => {
                                            pending_queries.remove(&id);
                                        }
                                        _ => {}
                                    }
                                }
                            },
                            SwarmEvent::NewListenAddr { address, .. } => {
                                tracing::info!("listening on {}", address);
                                let _ = event_tx.send(NetworkEvent::Listening(address));
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                                tracing::info!("connected to peer {} at {:?}", peer_id, endpoint.get_remote_address());
                                peers_clone.write().await.insert(peer_id);
                                
                                swarm.behaviour_mut().kademlia.add_address(&peer_id, endpoint.get_remote_address().clone());
                                let _ = swarm.behaviour_mut().kademlia.bootstrap();
                                
                                let _ = event_tx.send(NetworkEvent::PeerConnected(peer_id));
                            }
                            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                                tracing::warn!("disconnected from peer {}", peer_id);
                                peers_clone.write().await.remove(&peer_id);
                                let _ = event_tx.send(NetworkEvent::PeerDisconnected(peer_id));
                            }
                            SwarmEvent::Behaviour(BehaviourEvent::Transfer(event)) => match event {
                                libp2p::request_response::Event::Message { peer, message } => {
                                    match message {
                                        libp2p::request_response::Message::Request { request, channel, .. } => {
                                            let _ = event_tx.send(NetworkEvent::TransferRequest(peer, request, channel));
                                        }
                                        libp2p::request_response::Message::Response { response, .. } => {
                                            let _ = event_tx.send(NetworkEvent::TransferResponse(peer, response));
                                        }
                                    }
                                }
                                libp2p::request_response::Event::OutboundFailure { peer, error, request_id } => {
                                    tracing::warn!("transfer outbound failure to {}: {error:?} ({:?})", peer, request_id);
                                }
                                libp2p::request_response::Event::InboundFailure { peer, error, request_id } => {
                                    tracing::warn!("transfer inbound failure from {}: {error:?} ({:?})", peer, request_id);
                                }
                                libp2p::request_response::Event::ResponseSent { peer, request_id } => {
                                    tracing::trace!("transfer response sent to {} ({:?})", peer, request_id);
                                }
                            },
                            _ => {}
                        }
                    }
                    Some(cmd) = cmd_rx.recv() => {
                        match cmd {
                            KadCommand::Provide(key) => {
                                let _ = swarm.behaviour_mut().kademlia.start_providing(RecordKey::new(&key));
                            }
                            KadCommand::FindProviders(key, kind) => {
                                let qid = swarm.behaviour_mut().kademlia.get_providers(RecordKey::new(&key));
                                pending_queries.insert(qid, kind);
                            }
                        }
                    }
                    Some(job) = transfer_rx.recv() => {
                        let req_id = swarm.behaviour_mut().transfer.send_request(&job.peer, job.req);
                        tracing::debug!("sent transfer request {:?} to {}", req_id, job.peer);
                    }
                    Some(resp) = transfer_resp_rx.recv() => {
                        let _ = swarm
                            .behaviour_mut()
                            .transfer
                            .send_response(resp.channel, resp.response);
                    }
                    Some(msg) = publish_rx.recv() => {
                        let topic = match msg {
                            NetworkMessage::Blob(_) => Topic::new(TOPIC_BLOBS),
                            NetworkMessage::Program(_) => Topic::new(TOPIC_PROGRAMS),
                            NetworkMessage::Execution(_)
                            | NetworkMessage::Inventory(_)
                            | NetworkMessage::ProgramMeta(_)
                            | NetworkMessage::InventoryRequest
                            | NetworkMessage::ProgramSyncRequest(_)
                            | NetworkMessage::ProgramRequest(_)
                            | NetworkMessage::ProgramResponse(_)
                            | NetworkMessage::BlobRequest(_)
                            | NetworkMessage::BlobMeta(_)
                            | NetworkMessage::ExecutionRequest(_)
                            | NetworkMessage::Job(_)
                            | NetworkMessage::Capability(_)
                            | NetworkMessage::UnifiedProtocol(_)
                            | NetworkMessage::StateSync(_) => Topic::new(TOPIC_BLOCKS),
                        };
                        let data = match serde_json::to_vec(&msg) {
                            Ok(d) => d,
                            Err(_) => continue,
                        };
                        let msg_id = gossipsub::MessageId::from(blake3::hash(&data).as_bytes().to_vec());
                        if recent_ids.contains(&msg_id) {
                            continue;
                        }
                        if recent_ids.len() >= dedupe_window {
                            recent_ids.pop_front();
                        }
                        recent_ids.push_back(msg_id);

                        tracing::debug!("publishing {:?}", describe_msg(&msg));
                        let _ = swarm.behaviour_mut().gossipsub.publish(topic, data);
                    }
                }
            }
        });

        Ok(NetworkStreams {
            handle: NetworkHandle {
                peer_id,
                publisher: publish_tx,
                cmd: cmd_tx,
                transfer_req: transfer_tx,
                transfer_resp: transfer_resp_tx,
                peers: peers_shared,
            },
            events: event_rx,
        })
    }
}

fn describe_msg(msg: &NetworkMessage) -> &'static str {
    match msg {
        NetworkMessage::Blob(_) => "blob",
        NetworkMessage::BlobMeta(_) => "blob_meta",
        NetworkMessage::Program(_) => "program",
        NetworkMessage::ProgramMeta(_) => "program_meta",
        NetworkMessage::Execution(_) => "execution",
        NetworkMessage::Inventory(_) => "inventory",
        NetworkMessage::InventoryRequest => "inventory_request",
        NetworkMessage::ProgramSyncRequest(_) => "program_sync_request",
        NetworkMessage::ProgramRequest(_) => "program_request",
        NetworkMessage::ProgramResponse(_) => "program_response",
        NetworkMessage::BlobRequest(_) => "blob_request",
        NetworkMessage::ExecutionRequest(_) => "execution_request",
        NetworkMessage::Job(_) => "job",
        NetworkMessage::Capability(_) => "capability",
        NetworkMessage::UnifiedProtocol(_) => "unified_protocol",
        NetworkMessage::StateSync(_) => "state_sync",
    }
}