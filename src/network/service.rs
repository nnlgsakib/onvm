use crate::crypto::keys::NodeKeys;
use crate::network::codec::{
    sign_network_message, transfer_codec_fingerprint, verify_signed_message, HandshakeRequest,
    HandshakeResponse, NetworkMessage, ProviderKind, SignedNetworkMessage, TransferRequest,
    TransferResponse, HANDSHAKE_PROTOCOL, TOPIC_BLOBS, TOPIC_BLOCKS, TOPIC_PROGRAMS,
    TRANSFER_PROTOCOL,
};
use crate::network::peer_cache::{PeerCache, PEER_CACHE_MAX_AGE_MS};
use anyhow::{anyhow, Context, Result};
use blake3;
use futures::StreamExt;
use libp2p::core::ConnectedPoint;
use libp2p::gossipsub::{self, IdentTopic as Topic};
use libp2p::identify;
use libp2p::identity as libp2p_identity;
use libp2p::kad::{
    store::MemoryStore, Behaviour as Kademlia, Event as KademliaEvent, QueryId, QueryResult,
    RecordKey,
};
use libp2p::mdns;
use libp2p::noise;
use libp2p::request_response::{cbor, OutboundRequestId, ProtocolSupport, ResponseChannel};
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::{NetworkBehaviour, StreamProtocol, SwarmEvent};
use libp2p::yamux;
use libp2p::{Multiaddr, PeerId, SwarmBuilder};
use rand::seq::SliceRandom;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::{interval, Instant};

const MAX_OUTBOUND_TRANSFER_STREAMS_PER_PEER_MIN: usize = 2;
const MAX_OUTBOUND_TRANSFER_STREAMS_PER_PEER_CAP: usize = 32;
const MAX_OUTBOUND_TRANSFER_STREAMS_TOTAL_CAP: usize = 256;
const MAX_OUTBOUND_TRANSFER_QUEUE_PER_PEER: usize = 256;

fn describe_transfer_request(req: &TransferRequest) -> String {
    match req {
        TransferRequest::Program(id) => format!("program({id})"),
        TransferRequest::ProgramChunk { id, chunk_idx } => {
            format!("program_chunk(id={id}, idx={chunk_idx})")
        }
        TransferRequest::Blob(id) => format!("blob({id})"),
        TransferRequest::BlobChunk { id, chunk_idx } => {
            format!("blob_chunk(id={id}, idx={chunk_idx})")
        }
        TransferRequest::Execution(id) => format!("execution({})", hex::encode(id)),
        TransferRequest::PushProgram(p) => format!("push_program({})", p.meta.id),
        TransferRequest::PushProgramChunk { id, chunk_idx, .. } => {
            format!("push_program_chunk(id={id}, idx={chunk_idx})")
        }
        TransferRequest::PushBlob(b) => format!("push_blob({})", b.meta.id),
        TransferRequest::PushExecution(_) => "push_execution".to_string(),
        TransferRequest::Sync(s) => format!(
            "sync(programs={}, executions={})",
            s.programs.len(),
            s.executions.len()
        ),
        TransferRequest::Unified(u) => match u {
            crate::network::unified_protocol::UnifiedRequest::GetManifest(r) => {
                format!("unified:get_manifest({})", r.object_id)
            }
            crate::network::unified_protocol::UnifiedRequest::GetChunk(r) => {
                format!("unified:get_chunk({})", r.chunk_id)
            }
            crate::network::unified_protocol::UnifiedRequest::GetChunkPart(r) => format!(
                "unified:get_chunk_part({}, off={}, len={})",
                r.chunk_id, r.offset, r.length
            ),
            crate::network::unified_protocol::UnifiedRequest::GetChunks(r) => {
                format!("unified:get_chunks(n={})", r.chunk_ids.len())
            }
            crate::network::unified_protocol::UnifiedRequest::GetObjectAvailability(r) => {
                format!("unified:get_object_availability({})", r.object_id)
            }
            crate::network::unified_protocol::UnifiedRequest::GetObjectMetadata(r) => {
                format!("unified:get_object_metadata({})", r.object_id)
            }
            crate::network::unified_protocol::UnifiedRequest::GetFinalizedTransition(r) => format!(
                "unified:get_finalized_transition(program={}, height={})",
                r.program_id, r.height
            ),
            crate::network::unified_protocol::UnifiedRequest::ExecuteViaLeader(r) => {
                format!("unified:execute_via_leader(program={})", r.program_id)
            }
            crate::network::unified_protocol::UnifiedRequest::GetProgramManifest(r) => {
                format!("unified:get_program_manifest({})", r.program_id)
            }
        },
        TransferRequest::StateRequest(r) => format!("state_request({})", r.program_id),
        TransferRequest::InventoryRequest => "inventory_request".to_string(),
    }
}

#[derive(Debug, Clone)]
struct RedialState {
    next_attempt_at: Instant,
    attempts: u32,
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
        let _ = self.transfer_req.send(TransferJob {
            peer,
            req,
            reply: None,
        });
    }

    pub fn request_transfer_oneshot(
        &self,
        peer: PeerId,
        req: TransferRequest,
    ) -> Result<oneshot::Receiver<Result<TransferResponse>>> {
        let (tx, rx) = oneshot::channel();
        self.transfer_req
            .send(TransferJob {
                peer,
                req,
                reply: Some(tx),
            })
            .map_err(|_| anyhow!("failed to submit transfer request"))?;
        Ok(rx)
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
    pub enable_mdns: bool,
    pub require_encryption: bool,
    pub max_inbound_connections: usize,
    pub max_total_connections: usize,
    pub max_connections_per_peer: usize,
    pub max_inbound_streams: usize,
    pub max_gossip_bytes: usize,
    pub bootnodes: Vec<Multiaddr>,
    pub pex: crate::config::PexConfig,
    pub keep_alive: crate::config::KeepAliveConfig,
    pub memory_throttle: crate::config::MemoryThrottleConfig,
    pub peer_cache_path: Option<PathBuf>,
    pub public_peer_db_path: Option<PathBuf>,
}

enum KadCommand {
    Provide(Vec<u8>),
    FindProviders(Vec<u8>, ProviderKind),
}

#[derive(Debug)]
struct TransferJob {
    peer: PeerId,
    req: TransferRequest,
    reply: Option<oneshot::Sender<Result<TransferResponse>>>,
}

#[derive(Debug)]
struct TransferResponseJob {
    channel: ResponseChannel<TransferResponse>,
    response: TransferResponse,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: Toggle<mdns::tokio::Behaviour>,
    identify: identify::Behaviour,
    kademlia: Kademlia<MemoryStore>,
    handshake: cbor::Behaviour<HandshakeRequest, HandshakeResponse>,
    pex: cbor::Behaviour<crate::network::pex::PexRequest, crate::network::pex::PexResponse>,
    ping_png: cbor::Behaviour<
        crate::network::ping_png::PingRequest,
        crate::network::ping_png::PongResponse,
    >,
    transfer: cbor::Behaviour<TransferRequest, TransferResponse>,
}

pub struct NetworkService;

fn peer_id_from_multiaddr(addr: &Multiaddr) -> Option<PeerId> {
    for proto in addr.iter() {
        if let libp2p::multiaddr::Protocol::P2p(peer) = proto {
            return Some(peer);
        }
    }
    None
}

fn strip_p2p_component(addr: &Multiaddr) -> Multiaddr {
    let mut out = addr.clone();
    if let Some(last) = out.pop() {
        if matches!(last, libp2p::multiaddr::Protocol::P2p(_)) {
            return out;
        }
        out.push(last);
    }
    out
}

fn ensure_p2p_component(mut addr: Multiaddr, peer: &PeerId) -> Multiaddr {
    if let Some(last) = addr.pop() {
        if !matches!(last, libp2p::multiaddr::Protocol::P2p(_)) {
            addr.push(last);
        }
    }
    addr.push(libp2p::multiaddr::Protocol::P2p((*peer).into()));
    addr
}

fn dial_peer_best_effort(
    swarm: &mut libp2p::Swarm<Behaviour>,
    peer: PeerId,
    known_addrs: &HashMap<PeerId, HashSet<Multiaddr>>,
) -> bool {
    if let Some(addrs) = known_addrs.get(&peer) {
        for addr in addrs.iter().take(4) {
            let dial_addr = ensure_p2p_component(addr.clone(), &peer);
            match swarm.dial(dial_addr) {
                Ok(_) => return true,
                Err(err) => {
                    tracing::debug!("dial {} via addr failed: {}", peer, err);
                }
            }
        }
    }
    match swarm.dial(peer) {
        Ok(_) => true,
        Err(err) => {
            tracing::debug!("dial {} by peer id failed: {}", peer, err);
            false
        }
    }
}

fn is_expected_eof(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::UnexpectedEof {
        return true;
    }
    let msg = err.to_string().to_lowercase();
    if msg.contains("unexpected end of file") || msg.contains("unexpected eof") {
        return true;
    }
    false
}

fn is_expected_disconnect_io(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::NotConnected
    ) || is_expected_eof(err)
}

impl NetworkService {
    pub async fn start(identity: &NodeKeys, config: NetworkConfig) -> Result<NetworkStreams> {
        let secret =
            libp2p_identity::ed25519::SecretKey::try_from_bytes(identity.keypair.secret.to_bytes())
                .context("libp2p secret")?;
        let ed_kp = libp2p_identity::ed25519::Keypair::from(secret);
        let local_key = libp2p_identity::Keypair::from(ed_kp);
        let peer_id = PeerId::from(local_key.public());

        if !config.require_encryption {
            tracing::warn!(
                "require_encryption=false is insecure; ONVM will still enforce noise/tls transports"
            );
        }

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .validation_mode(gossipsub::ValidationMode::Strict)
            .message_id_fn(|m: &gossipsub::Message| {
                let hash = blake3::hash(&m.data);
                gossipsub::MessageId::from(hash.as_bytes().to_vec())
            })
            .heartbeat_interval(config.heartbeat)
            .max_transmit_size(config.max_gossip_bytes)
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

        let mdns = if config.enable_mdns {
            Toggle::from(Some(mdns::tokio::Behaviour::new(
                mdns::Config::default(),
                peer_id,
            )?))
        } else {
            Toggle::from(None)
        };

        let identify = identify::Behaviour::new(
            identify::Config::new("onvm/1.0.0".to_string(), local_key.public())
                .with_agent_version(format!("onvm/{}", env!("CARGO_PKG_VERSION")))
                .with_push_listen_addr_updates(true)
                .with_cache_size(1024),
        );

        let kademlia = crate::network::dht::build_kademlia(peer_id);

        let handshake_protocol = StreamProtocol::new(HANDSHAKE_PROTOCOL);
        let handshake_config = libp2p::request_response::Config::default()
            .with_request_timeout(Duration::from_secs(10))
            .with_max_concurrent_streams(config.max_inbound_streams);
        let handshake = cbor::Behaviour::new(
            std::iter::once((handshake_protocol, ProtocolSupport::Full)),
            handshake_config,
        );

        let pex_protocol = StreamProtocol::new(crate::network::pex::PEX_PROTOCOL);
        let pex_config = libp2p::request_response::Config::default()
            .with_request_timeout(Duration::from_secs(10))
            .with_max_concurrent_streams(config.max_inbound_streams);
        let pex = cbor::Behaviour::new(
            std::iter::once((pex_protocol, ProtocolSupport::Full)),
            pex_config,
        );

        let ping_protocol = StreamProtocol::new(crate::network::ping_png::PING_PROTOCOL);
        let ping_config = libp2p::request_response::Config::default()
            .with_request_timeout(Duration::from_secs(
                config.keep_alive.ping_timeout_secs.max(1),
            ))
            .with_max_concurrent_streams(config.max_inbound_streams);
        let ping_png = cbor::Behaviour::new(
            std::iter::once((ping_protocol, ProtocolSupport::Full)),
            ping_config,
        );

        let transfer_protocol = StreamProtocol::new(TRANSFER_PROTOCOL);
        let transfer_config = libp2p::request_response::Config::default()
            .with_request_timeout(Duration::from_secs(60))
            .with_max_concurrent_streams(config.max_inbound_streams);
        let transfer = cbor::Behaviour::new(
            std::iter::once((transfer_protocol, ProtocolSupport::Full)),
            transfer_config,
        );
        let behaviour = Behaviour {
            gossipsub,
            mdns,
            identify,
            kademlia,
            handshake,
            pex,
            ping_png,
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
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(10 * 60)))
            .build();
        let peer_id = *swarm.local_peer_id();

        tracing::info!("libp2p node initialized as {}", peer_id);

        swarm
            .listen_on(config.listen_addr.clone())
            .context("listen_on")?;

        for addr in &config.bootnodes {
            match swarm.dial(addr.clone()) {
                Ok(_) => {
                    tracing::info!("dialing bootnode {}", addr);
                }
                Err(e) => {
                    tracing::warn!("failed to dial bootnode {}: {}", addr, e);
                }
            }
        }

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (publish_tx, mut publish_rx) = mpsc::unbounded_channel();
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let (transfer_tx, mut transfer_rx) = mpsc::unbounded_channel::<TransferJob>();
        let (transfer_resp_tx, mut transfer_resp_rx) =
            mpsc::unbounded_channel::<TransferResponseJob>();
        let mut pending_queries: HashMap<QueryId, ProviderKind> = HashMap::new();

        let peers_shared = Arc::new(RwLock::new(HashSet::new()));
        let peers_clone = Arc::clone(&peers_shared);
        let signing_keys = identity.clone();
        let pex_cfg = config.pex.clone();
        let keep_alive_cfg = config.keep_alive.clone();
        let mem_cfg = config.memory_throttle.clone();
        let peer_cache_path = config.peer_cache_path.clone();
        let public_peer_db_path = config.public_peer_db_path.clone();
        let max_total_connections = config.max_total_connections.max(1);
        let max_connections_per_peer = config.max_connections_per_peer.max(1);
        let max_inbound_connections = config.max_inbound_connections.max(1);
        let max_outbound_transfer_streams_per_peer = {
            let mut v = config.max_inbound_streams / 4;
            if v < MAX_OUTBOUND_TRANSFER_STREAMS_PER_PEER_MIN {
                v = MAX_OUTBOUND_TRANSFER_STREAMS_PER_PEER_MIN;
            }
            if v > MAX_OUTBOUND_TRANSFER_STREAMS_PER_PEER_CAP {
                v = MAX_OUTBOUND_TRANSFER_STREAMS_PER_PEER_CAP;
            }
            v
        };
        let max_outbound_transfer_streams_total = std::cmp::max(
            max_outbound_transfer_streams_per_peer,
            std::cmp::min(
                MAX_OUTBOUND_TRANSFER_STREAMS_TOTAL_CAP,
                max_outbound_transfer_streams_per_peer.saturating_mul(8),
            ),
        );

        let bootnodes = config.bootnodes.clone();

        tokio::spawn(async move {
            let mut handshake_pending: HashMap<
                libp2p::request_response::OutboundRequestId,
                PeerId,
            > = HashMap::new();
            let mut handshake_inflight: HashSet<PeerId> = HashSet::new();
            let mut transfer_ready: HashSet<PeerId> = HashSet::new();
            let mut transfer_blocked: HashSet<PeerId> = HashSet::new();
            let mut transfer_buffer: HashMap<PeerId, VecDeque<TransferJob>> = HashMap::new();
            let mut transfer_pending: HashMap<
                OutboundRequestId,
                (
                    PeerId,
                    String,
                    Option<oneshot::Sender<Result<TransferResponse>>>,
                ),
            > = HashMap::new();
            let mut transfer_queue: HashMap<PeerId, VecDeque<TransferJob>> = HashMap::new();
            let mut transfer_outbound_inflight_total: usize = 0;
            let mut transfer_outbound_inflight_per_peer: HashMap<PeerId, usize> = HashMap::new();
            let mut redial: HashMap<PeerId, RedialState> = HashMap::new();
            let mut redial_tick = interval(Duration::from_secs(2));
            let mut total_connections: usize = 0;
            let mut inbound_connections: usize = 0;
            let mut counted_connections: HashSet<libp2p::swarm::ConnectionId> = HashSet::new();
            let mut counted_inbound: HashSet<libp2p::swarm::ConnectionId> = HashSet::new();

            tracing::info!(
                "transfer outbound limits: per_peer={} total={}",
                max_outbound_transfer_streams_per_peer,
                max_outbound_transfer_streams_total
            );

            fn drain_transfer_queues(
                swarm: &mut libp2p::Swarm<Behaviour>,
                transfer_ready: &HashSet<PeerId>,
                transfer_blocked: &HashSet<PeerId>,
                transfer_queue: &mut HashMap<PeerId, VecDeque<TransferJob>>,
                transfer_pending: &mut HashMap<
                    OutboundRequestId,
                    (
                        PeerId,
                        String,
                        Option<oneshot::Sender<Result<TransferResponse>>>,
                    ),
                >,
                transfer_outbound_inflight_total: &mut usize,
                transfer_outbound_inflight_per_peer: &mut HashMap<PeerId, usize>,
                max_outbound_transfer_streams_per_peer: usize,
                max_outbound_transfer_streams_total: usize,
            ) {
                if transfer_queue.is_empty() {
                    return;
                }

                let peers: Vec<PeerId> = transfer_queue.keys().copied().collect();
                for peer in peers {
                    if *transfer_outbound_inflight_total >= max_outbound_transfer_streams_total {
                        return;
                    }
                    if transfer_blocked.contains(&peer) || !transfer_ready.contains(&peer) {
                        continue;
                    }

                    loop {
                        if *transfer_outbound_inflight_total >= max_outbound_transfer_streams_total
                        {
                            return;
                        }
                        let inflight_peer = transfer_outbound_inflight_per_peer
                            .get(&peer)
                            .copied()
                            .unwrap_or(0);
                        if inflight_peer >= max_outbound_transfer_streams_per_peer {
                            break;
                        }

                        let Some(job) = transfer_queue.get_mut(&peer).and_then(|q| q.pop_front())
                        else {
                            transfer_queue.remove(&peer);
                            break;
                        };

                        let desc = describe_transfer_request(&job.req);
                        let req_id = swarm.behaviour_mut().transfer.send_request(&peer, job.req);
                        transfer_pending.insert(req_id, (peer, desc, job.reply));
                        *transfer_outbound_inflight_total =
                            transfer_outbound_inflight_total.saturating_add(1);
                        *transfer_outbound_inflight_per_peer.entry(peer).or_insert(0) += 1;
                    }
                }
            }

            let mut sys = sysinfo::System::new();
            let mut memory_pressure = false;
            let mut mem_tick = interval(Duration::from_secs(5));

            let pex_policy = crate::network::pex::PexPolicy {
                max_peers_per_response: pex_cfg.max_peers_shared,
                max_addrs_per_peer: pex_cfg.max_addrs_per_peer,
                allow_private_addrs: pex_cfg.allow_private_addrs,
                allow_loopback_addrs: pex_cfg.allow_loopback_addrs,
            };
            let mut known_addrs: HashMap<PeerId, HashSet<Multiaddr>> = HashMap::new();
            known_addrs.insert(peer_id, HashSet::new());
            for addr in &bootnodes {
                if let Some(peer) = peer_id_from_multiaddr(addr) {
                    let base = strip_p2p_component(addr);
                    known_addrs.entry(peer).or_default().insert(base.clone());
                    swarm.behaviour_mut().kademlia.add_address(&peer, base);
                }
            }

            let mut peer_cache = PeerCache::new(
                peer_id,
                &bootnodes,
                peer_cache_path.clone(),
                public_peer_db_path.clone(),
                pex_policy.clone(),
            );
            match peer_cache.load(&mut known_addrs).await {
                Ok(stats) => {
                    if let Some(path) = peer_cache_path.as_ref() {
                        if stats.peer_cache_peers > 0 {
                            tracing::info!(
                                "loaded {} peers from peer cache {}",
                                stats.peer_cache_peers,
                                path.display()
                            );
                        }
                    }
                    if let Some(path) = public_peer_db_path.as_ref() {
                        if stats.public_db_peers > 0 {
                            tracing::info!(
                                "loaded {} public peers from public peer db {}",
                                stats.public_db_peers,
                                path.display()
                            );
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!("failed to load peer cache: {}", err);
                }
            }

            for (pid, addrs) in known_addrs.iter() {
                if *pid == peer_id {
                    continue;
                }
                for addr in addrs {
                    swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(pid, addr.clone());
                }
            }

            let mut peer_cache_tick = interval(Duration::from_secs(60));
            let mut discovery_dial_tick = interval(Duration::from_secs(3));
            let mut public_peer_prune_tick = interval(Duration::from_secs(300));

            let mut pex_tick = interval(Duration::from_secs(pex_cfg.request_interval_secs.max(1)));
            let mut pex_pending: HashMap<libp2p::request_response::OutboundRequestId, PeerId> =
                HashMap::new();
            let mut pex_inflight: HashSet<PeerId> = HashSet::new();

            let mut ping_tick = interval(Duration::from_secs(
                keep_alive_cfg.ping_interval_secs.max(1),
            ));
            let mut ping_pending: HashMap<libp2p::request_response::OutboundRequestId, PeerId> =
                HashMap::new();
            let mut ping_inflight: HashSet<PeerId> = HashSet::new();
            let mut ping_failures: HashMap<PeerId, u32> = HashMap::new();
            let mut connected_peers_local: HashSet<PeerId> = HashSet::new();
            let mut dialing_peers: HashSet<PeerId> = bootnodes
                .iter()
                .filter_map(peer_id_from_multiaddr)
                .collect();

            if !memory_pressure && !known_addrs.is_empty() {
                let effective_max_total = max_total_connections.max(1);
                if total_connections < effective_max_total {
                    let now_ms = crate::network::ping_png::now_ms();
                    let mut candidates: Vec<(PeerId, u64)> = known_addrs
                        .keys()
                        .cloned()
                        .filter_map(|pid| {
                            if pid == peer_id {
                                return None;
                            }
                            if dialing_peers.contains(&pid) {
                                return None;
                            }
                            let Some(seen_ms) = peer_cache.last_seen_ms(&pid) else {
                                return None;
                            };
                            if now_ms.saturating_sub(seen_ms) > PEER_CACHE_MAX_AGE_MS {
                                return None;
                            }
                            Some((pid, seen_ms))
                        })
                        .collect();
                    candidates.sort_by_key(|(_, seen)| Reverse(*seen));

                    for (pid, _) in candidates.into_iter().take(3) {
                        if dial_peer_best_effort(&mut swarm, pid, &known_addrs) {
                            dialing_peers.insert(pid);
                        } else {
                            redial.entry(pid).or_insert(RedialState {
                                next_attempt_at: Instant::now() + Duration::from_millis(250),
                                attempts: 0,
                            });
                        }
                    }
                }
            }

            loop {
                tokio::select! {
                    _ = mem_tick.tick() => {
                        if mem_cfg.enable {
                            sys.refresh_memory();
                            let avail_mb = sys.available_memory() / 1024 / 1024;
                            let new_pressure = avail_mb < mem_cfg.min_available_mb;
                            if new_pressure != memory_pressure {
                                memory_pressure = new_pressure;
                                if memory_pressure {
                                    tracing::warn!(
                                        "memory pressure: available={}MB below threshold {}MB; throttling connections",
                                        avail_mb,
                                        mem_cfg.min_available_mb
                                    );
                                } else {
                                    tracing::info!("memory pressure cleared: available={}MB", avail_mb);
                                }
                            }
                        }
                    }
                    _ = peer_cache_tick.tick() => {
                        if let Err(err) = peer_cache.persist_tick(&known_addrs).await {
                            tracing::warn!("peer cache persist failed: {}", err);
                        }
                    }
                    _ = public_peer_prune_tick.tick() => {
                        let removed = peer_cache.prune_public_peer_db();
                        if removed > 0 {
                            tracing::debug!(
                                "pruned {} stale peers from public peer db",
                                removed
                            );
                        }
                    }
                    _ = discovery_dial_tick.tick() => {
                        if memory_pressure {
                            continue;
                        }

                        let effective_max_total = if mem_cfg.enable && memory_pressure {
                            mem_cfg.max_total_connections_under_pressure
                        } else {
                            max_total_connections
                        }
                        .max(1);

                        if total_connections >= effective_max_total {
                            continue;
                        }

                        let needed = pex_cfg.target_peers.saturating_sub(connected_peers_local.len());
                        if needed == 0 {
                            continue;
                        }

                        let now_ms = crate::network::ping_png::now_ms();
                        let mut candidates: Vec<(PeerId, u64)> = known_addrs
                            .iter()
                            .filter_map(|(pid, addrs)| {
                                if *pid == peer_id {
                                    return None;
                                }
                                if addrs.is_empty() {
                                    return None;
                                }
                                if connected_peers_local.contains(pid) {
                                    return None;
                                }
                                if dialing_peers.contains(pid) {
                                    return None;
                                }
                                if transfer_blocked.contains(pid) {
                                    return None;
                                }
                                if redial.contains_key(pid) {
                                    return None;
                                }

                                let last_seen_ms = peer_cache.last_seen_ms(pid).unwrap_or(0);
                                if last_seen_ms == 0 {
                                    return None;
                                }
                                if now_ms.saturating_sub(last_seen_ms) > PEER_CACHE_MAX_AGE_MS {
                                    return None;
                                }

                                Some((*pid, last_seen_ms))
                            })
                            .collect();

                        candidates.sort_by_key(|(_, seen)| Reverse(*seen));
                        candidates.truncate(32);
                        candidates.shuffle(&mut rand::thread_rng());

                        let dial_attempts = needed.min(2).min(candidates.len());
                        for (pid, _) in candidates.into_iter().take(dial_attempts) {
                            if dial_peer_best_effort(&mut swarm, pid, &known_addrs) {
                                dialing_peers.insert(pid);
                            } else {
                                redial.entry(pid).or_insert(RedialState {
                                    next_attempt_at: Instant::now() + Duration::from_millis(250),
                                    attempts: 0,
                                });
                            }
                        }
                    }
                    _ = ping_tick.tick() => {
                        if keep_alive_cfg.enable {
                            let peers: Vec<PeerId> = connected_peers_local.iter().copied().collect();
                            for peer in peers {
                                if ping_inflight.contains(&peer) {
                                    continue;
                                }
                                let ping = crate::network::ping_png::new_ping();
                                let req_id = swarm.behaviour_mut().ping_png.send_request(&peer, ping);
                                ping_pending.insert(req_id, peer);
                                ping_inflight.insert(peer);
                            }
                        }
                    }
                    _ = pex_tick.tick() => {
                        if pex_cfg.enable {
                            if memory_pressure || connected_peers_local.is_empty() {
                                continue;
                            }

                            let needed = pex_cfg
                                .target_peers
                                .saturating_sub(connected_peers_local.len());
                            if needed == 0 {
                                continue;
                            }

                            let mut peers: Vec<PeerId> =
                                connected_peers_local.iter().copied().collect();
                            peers.shuffle(&mut rand::thread_rng());

                            let request_budget = needed.min(3);
                            let mut requested = 0usize;

                            for peer in peers {
                                if requested >= request_budget {
                                    break;
                                }
                                if pex_inflight.contains(&peer) {
                                    continue;
                                }
                                let req = crate::network::pex::PexRequest {
                                    want: pex_cfg.want_peers as u32,
                                };
                                let req_id = swarm.behaviour_mut().pex.send_request(&peer, req);
                                pex_pending.insert(req_id, peer);
                                pex_inflight.insert(peer);
                                requested += 1;
                            }
                        }
                    }
                    _ = redial_tick.tick() => {
                        if memory_pressure || connected_peers_local.len() >= pex_cfg.target_peers {
                            continue;
                        }

                        let effective_max_total = if mem_cfg.enable && memory_pressure {
                            mem_cfg.max_total_connections_under_pressure
                        } else {
                            max_total_connections
                        }
                        .max(1);

                        if total_connections >= effective_max_total {
                            continue;
                        }

                        let now = Instant::now();
                        let mut remove: Vec<PeerId> = Vec::new();
                        for (peer, state) in redial.iter_mut() {
                            if transfer_blocked.contains(peer) {
                                remove.push(*peer);
                                continue;
                            }
                            if connected_peers_local.contains(peer) {
                                remove.push(*peer);
                                continue;
                            }
                            if dialing_peers.contains(peer) {
                                continue;
                            }
                            if now < state.next_attempt_at {
                                continue;
                            }

                            if state.attempts >= 20 {
                                remove.push(*peer);
                                continue;
                            }

                            state.attempts = state.attempts.saturating_add(1);
                            let backoff_secs = 1u64 << state.attempts.min(5);
                            let backoff = Duration::from_secs(backoff_secs.min(30));
                            state.next_attempt_at = now + backoff;

                            if dial_peer_best_effort(&mut swarm, *peer, &known_addrs) {
                                dialing_peers.insert(*peer);
                                tracing::debug!("redial attempt {} to {}", state.attempts, peer);
                            }
                        }
                        for peer in remove {
                            redial.remove(&peer);
                        }
                    }
                    swarm_event = swarm.select_next_some() => {
                        match swarm_event {
                            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(ev)) => {
                                if let gossipsub::Event::Message {
                                    propagation_source,
                                    message,
                                    ..
                                } = ev
                                {
                                    match bincode::serde::decode_from_slice::<SignedNetworkMessage, _>(
                                        &message.data,
                                        bincode::config::standard(),
                                    ) {
                                        Ok((env, _)) => {
                                            match verify_signed_message(&env) {
                                                Ok(msg) => {
                                                    tracing::debug!("inbound {:?} from {}", describe_msg(&msg), propagation_source);
                                                    let _ = event_tx.send(NetworkEvent::Inbound(propagation_source, msg));
                                                }
                                                Err(e) => {
                                                    tracing::warn!("dropping unsigned/invalid gossip from {}: {}", propagation_source, e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!("failed to decode signed gossip: {}", e);
                                        }
                                    }
                                }
                            }
                            SwarmEvent::Behaviour(BehaviourEvent::Mdns(ev)) => match ev {
                                mdns::Event::Discovered(list) => {
                                    let mut discovered_peers: HashSet<PeerId> = HashSet::new();
                                    for (peer, addr) in list {
                                        let base = strip_p2p_component(&addr);
                                        known_addrs.entry(peer).or_default().insert(base.clone());
                                        swarm.behaviour_mut().kademlia.add_address(&peer, base);
                                        discovered_peers.insert(peer);
                                    }
                                    for peer in discovered_peers {
                                        if peer == peer_id {
                                            continue;
                                        }
                                        if connected_peers_local.contains(&peer)
                                            || dialing_peers.contains(&peer)
                                        {
                                            continue;
                                        }
                                        tracing::info!("mDNS discovered peer {}, attempting dial", peer);
                                        if dial_peer_best_effort(&mut swarm, peer, &known_addrs) {
                                            dialing_peers.insert(peer);
                                        }
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
                            SwarmEvent::Behaviour(BehaviourEvent::Identify(ev)) => match ev {
                                identify::Event::Received { peer_id: remote, info, .. }
                                | identify::Event::Pushed { peer_id: remote, info, .. } => {
                                    let addrs = crate::network::pex::rewrite_unspecified_listen_addrs(
                                        info.listen_addrs,
                                        &info.observed_addr,
                                    );
                                    let validated = match crate::network::pex::validate_peer_addrs(
                                        &remote,
                                        &addrs,
                                        &pex_policy,
                                    ) {
                                        Ok(v) => v,
                                        Err(err) => {
                                            tracing::debug!(
                                                "identify produced invalid addrs for {}: {}",
                                                remote,
                                                err
                                            );
                                            Vec::new()
                                        }
                                    };
                                    if !validated.is_empty() {
                                        {
                                            let entry = known_addrs.entry(remote).or_default();
                                            for addr in validated {
                                                let base = strip_p2p_component(&addr);
                                                entry.insert(base.clone());
                                                swarm.behaviour_mut().kademlia.add_address(&remote, base);
                                            }
                                        }

                                        let now_ms = crate::network::ping_png::now_ms();
                                        peer_cache.note_peer_addresses_updated(
                                            remote,
                                            now_ms,
                                            &known_addrs,
                                        );
                                    }
                                }
                                identify::Event::Error { peer_id: remote, error, .. } => {
                                    tracing::debug!("identify error from {}: {error:?}", remote);
                                }
                                identify::Event::Sent { .. } => {}
                            },
                            SwarmEvent::NewListenAddr { address, .. } => {
                                let mut addr_with_peer = address.clone();
                                addr_with_peer.push(libp2p::multiaddr::Protocol::P2p(peer_id.into()));
                                tracing::info!("listening on {}", addr_with_peer);
                                known_addrs
                                    .entry(peer_id)
                                    .or_default()
                                    .insert(address.clone());
                                let _ = event_tx.send(NetworkEvent::Listening(addr_with_peer));
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, connection_id, endpoint, num_established, .. } => {
                                let inbound = matches!(endpoint, ConnectedPoint::Listener { .. });
                                let effective_max_total = if mem_cfg.enable && memory_pressure {
                                    mem_cfg.max_total_connections_under_pressure
                                } else {
                                    max_total_connections
                                }
                                .max(1);

                                if inbound && inbound_connections >= max_inbound_connections {
                                    tracing::warn!(
                                        "dropping inbound connection from {}: inbound limit reached ({}/{})",
                                        peer_id,
                                        inbound_connections,
                                        max_inbound_connections
                                    );
                                    let _ = swarm.close_connection(connection_id);
                                    continue;
                                }
                                if total_connections >= effective_max_total {
                                    tracing::warn!(
                                        "dropping connection from {}: global connection limit reached ({}/{})",
                                        peer_id,
                                        total_connections,
                                        effective_max_total
                                    );
                                    let _ = swarm.close_connection(connection_id);
                                    continue;
                                }
                                let established_to_peer = num_established.get();
                                if established_to_peer as usize > max_connections_per_peer {
                                    if connected_peers_local.insert(peer_id) {
                                        swarm
                                            .behaviour_mut()
                                            .gossipsub
                                            .add_explicit_peer(&peer_id);
                                    }
                                    dialing_peers.remove(&peer_id);
                                    tracing::warn!(
                                        "dropping extra connection to {}: per-peer limit reached ({}/{})",
                                        peer_id,
                                        established_to_peer,
                                        max_connections_per_peer
                                    );
                                    let _ = swarm.close_connection(connection_id);
                                    continue;
                                }

                                counted_connections.insert(connection_id);
                                total_connections = total_connections.saturating_add(1);
                                if inbound {
                                    counted_inbound.insert(connection_id);
                                    inbound_connections = inbound_connections.saturating_add(1);
                                }

                                tracing::info!(
                                    "connected to peer {} at {:?} (peer_conns={} total_conns={})",
                                    peer_id,
                                    endpoint.get_remote_address(),
                                    established_to_peer,
                                    total_connections
                                );

                                let now_ms = crate::network::ping_png::now_ms();

                                // Only treat `Dialer` endpoints as dialable addresses. For inbound connections the
                                // "remote address" is a send-back address (often an ephemeral port) and is usually
                                // not dialable. We rely on Identify to learn listen addrs.
                                if endpoint.is_dialer() {
                                    let base = strip_p2p_component(endpoint.get_remote_address());
                                    known_addrs.entry(peer_id).or_default().insert(base.clone());
                                }

                                peer_cache.note_peer_addresses_updated(peer_id, now_ms, &known_addrs);

                                redial.remove(&peer_id);

                                if connected_peers_local.insert(peer_id) {
                                    swarm
                                        .behaviour_mut()
                                        .gossipsub
                                        .add_explicit_peer(&peer_id);
                                }
                                dialing_peers.remove(&peer_id);

                                if endpoint.is_dialer() {
                                    let base = strip_p2p_component(endpoint.get_remote_address());
                                    swarm
                                        .behaviour_mut()
                                        .kademlia
                                        .add_address(&peer_id, base);
                                    let _ = swarm.behaviour_mut().kademlia.bootstrap();
                                }

                                if !transfer_ready.contains(&peer_id)
                                    && !handshake_inflight.contains(&peer_id)
                                    && !transfer_blocked.contains(&peer_id)
                                {
                                    let req = HandshakeRequest {
                                        node_version: env!("CARGO_PKG_VERSION").to_string(),
                                        transfer_protocol: TRANSFER_PROTOCOL.to_string(),
                                        transfer_codec_fingerprint: transfer_codec_fingerprint(),
                                    };
                                    let req_id = swarm
                                        .behaviour_mut()
                                        .handshake
                                        .send_request(&peer_id, req);
                                    handshake_pending.insert(req_id, peer_id);
                                    handshake_inflight.insert(peer_id);
                                    tracing::debug!(
                                        "sent handshake request {:?} to {}",
                                        req_id,
                                        peer_id
                                    );
                                }
                            }
                            SwarmEvent::Dialing { peer_id: Some(peer), .. } => {
                                dialing_peers.insert(peer);
                            }
                            SwarmEvent::OutgoingConnectionError {
                                peer_id: Some(peer),
                                error,
                                connection_id,
                                ..
                            } => {
                                dialing_peers.remove(&peer);
                                tracing::debug!(
                                    "outgoing connection error to {}: {error:?} ({:?})",
                                    peer,
                                    connection_id
                                );

                                if !memory_pressure
                                    && connected_peers_local.len() < pex_cfg.target_peers
                                    && !transfer_blocked.contains(&peer)
                                {
                                    redial.entry(peer).or_insert(RedialState {
                                        next_attempt_at: Instant::now() + Duration::from_millis(250),
                                        attempts: 0,
                                    });
                                }
                            }
                            SwarmEvent::ConnectionClosed { peer_id, connection_id, endpoint: _endpoint, num_established, cause, .. } => {
                                if counted_connections.remove(&connection_id) {
                                    total_connections = total_connections.saturating_sub(1);
                                }
                                if counted_inbound.remove(&connection_id) {
                                    inbound_connections = inbound_connections.saturating_sub(1);
                                }

                                if let Some(cause) = &cause {
                                    match cause {
                                        libp2p::swarm::ConnectionError::IO(err)
                                            if is_expected_disconnect_io(err) =>
                                        {
                                            tracing::debug!(
                                                "connection closed with peer {} (remaining_connections={} total_conns={}): {}",
                                                peer_id,
                                                num_established,
                                                total_connections,
                                                cause
                                            );
                                        }
                                        libp2p::swarm::ConnectionError::KeepAliveTimeout => {
                                            tracing::info!(
                                                "connection closed with peer {} (remaining_connections={} total_conns={}): {}",
                                                peer_id,
                                                num_established,
                                                total_connections,
                                                cause
                                            );
                                        }
                                        _ => {
                                            tracing::warn!(
                                                "connection closed with peer {} (remaining_connections={} total_conns={}): {}",
                                                peer_id,
                                                num_established,
                                                total_connections,
                                                cause
                                            );
                                        }
                                    }
                                } else {
                                    tracing::info!(
                                        "connection closed with peer {} (remaining_connections={} total_conns={})",
                                        peer_id,
                                        num_established,
                                        total_connections
                                    );
                                }

                                if num_established == 0 {
                                    dialing_peers.remove(&peer_id);
                                    swarm
                                        .behaviour_mut()
                                        .gossipsub
                                        .remove_explicit_peer(&peer_id);
                                    let was_ready = peers_clone.write().await.remove(&peer_id);
                                    connected_peers_local.remove(&peer_id);
                                    handshake_inflight.remove(&peer_id);
                                    handshake_pending.retain(|_, peer| *peer != peer_id);
                                    let mut removed = 0usize;
                                    transfer_pending.retain(|_, (peer, _desc, reply)| {
                                        if *peer == peer_id {
                                            removed = removed.saturating_add(1);
                                            if let Some(reply) = reply.take() {
                                                let _ = reply.send(Err(anyhow!(
                                                    "peer {} disconnected",
                                                    peer_id
                                                )));
                                            }
                                            false
                                        } else {
                                            true
                                        }
                                    });
                                    if removed > 0 {
                                        transfer_outbound_inflight_total =
                                            transfer_outbound_inflight_total.saturating_sub(removed);
                                        transfer_outbound_inflight_per_peer.remove(&peer_id);
                                    }
                                    transfer_ready.remove(&peer_id);
                                    transfer_buffer.remove(&peer_id);
                                    transfer_queue.remove(&peer_id);
                                    ping_inflight.remove(&peer_id);
                                    ping_failures.remove(&peer_id);
                                    ping_pending.retain(|_, peer| *peer != peer_id);

                                    if !memory_pressure
                                        && connected_peers_local.len() < pex_cfg.target_peers
                                        && !transfer_blocked.contains(&peer_id)
                                    {
                                        redial.entry(peer_id).or_insert(RedialState {
                                            next_attempt_at: Instant::now()
                                                + Duration::from_millis(250),
                                            attempts: 0,
                                        });
                                    }

                                    if was_ready {
                                        let _ = event_tx.send(NetworkEvent::PeerDisconnected(peer_id));
                                    }
                                } else if !transfer_ready.contains(&peer_id)
                                    && !transfer_blocked.contains(&peer_id)
                                {
                                    // If we still have a connection but handshake was on the closed one, allow resending.
                                    handshake_inflight.remove(&peer_id);
                                    handshake_pending.retain(|_, peer| *peer != peer_id);
                                    let mut removed = 0usize;
                                    transfer_pending.retain(|_, (peer, _desc, reply)| {
                                        if *peer == peer_id {
                                            removed = removed.saturating_add(1);
                                            if let Some(reply) = reply.take() {
                                                let _ = reply.send(Err(anyhow!(
                                                    "peer {} disconnected",
                                                    peer_id
                                                )));
                                            }
                                            false
                                        } else {
                                            true
                                        }
                                    });
                                    if removed > 0 {
                                        transfer_outbound_inflight_total =
                                            transfer_outbound_inflight_total.saturating_sub(removed);
                                        transfer_outbound_inflight_per_peer.remove(&peer_id);
                                    }
                                }
                            }
                            SwarmEvent::Behaviour(BehaviourEvent::Pex(event)) => match event {
                                libp2p::request_response::Event::Message { peer, message } => match message {
                                    libp2p::request_response::Message::Request { request, channel, .. } => {
                                        let response = crate::network::pex::build_response(
                                            &known_addrs,
                                            &pex_policy,
                                            request.want as usize,
                                            peer_id,
                                            peer,
                                        );
                                        let _ = swarm.behaviour_mut().pex.send_response(channel, response);
                                    }
                                    libp2p::request_response::Message::Response { response, request_id } => {
                                        let from_peer = pex_pending.remove(&request_id);
                                        if let Some(peer) = from_peer.as_ref() {
                                            pex_inflight.remove(peer);
                                        }
                                        let discovered = crate::network::pex::parse_response(
                                            response,
                                            &pex_policy,
                                            peer_id,
                                        );
                                        if discovered.is_empty() {
                                            continue;
                                        }

                                        let now_ms = crate::network::ping_png::now_ms();
                                        let discovered_total = discovered.len();
                                        let pex_seen_refresh_ms = 5 * 60 * 1000;
                                        let mut new_peers = 0usize;
                                        let mut updated_peers = 0usize;
                                        let mut refreshed_peers = 0usize;

                                        let effective_max_total = if mem_cfg.enable && memory_pressure {
                                            mem_cfg.max_total_connections_under_pressure
                                        } else {
                                            max_total_connections
                                        }
                                        .max(1);

                                        let mut dial_budget = if memory_pressure
                                            || total_connections >= effective_max_total
                                        {
                                            0usize
                                        } else {
                                            pex_cfg
                                                .target_peers
                                                .saturating_sub(connected_peers_local.len())
                                                .min(4)
                                        };

                                        for (pid, addrs) in discovered {
                                            let (is_new_peer, new_base_addrs) = {
                                                let existing = known_addrs.get(&pid);
                                                let is_new_peer = match existing {
                                                    None => true,
                                                    Some(existing) => existing.is_empty(),
                                                };

                                                let mut new_base_addrs = Vec::new();
                                                for addr in addrs {
                                                    let base = strip_p2p_component(&addr);
                                                    let is_new_addr = match existing {
                                                        None => true,
                                                        Some(existing) => !existing.contains(&base),
                                                    };
                                                    if is_new_addr {
                                                        new_base_addrs.push(base);
                                                    }
                                                }
                                                (is_new_peer, new_base_addrs)
                                            };

                                            if new_base_addrs.is_empty() && !is_new_peer {
                                                let seen_ms =
                                                    peer_cache.last_seen_ms(&pid).unwrap_or(0);
                                                if seen_ms == 0
                                                    || now_ms.saturating_sub(seen_ms)
                                                        > pex_seen_refresh_ms
                                                {
                                                    peer_cache.note_peer_addresses_updated(
                                                        pid,
                                                        now_ms,
                                                        &known_addrs,
                                                    );
                                                    refreshed_peers += 1;
                                                }
                                                continue;
                                            }

                                            {
                                                let entry = known_addrs.entry(pid).or_default();
                                                for base in new_base_addrs {
                                                    entry.insert(base.clone());
                                                    swarm
                                                        .behaviour_mut()
                                                        .kademlia
                                                        .add_address(&pid, base);
                                                }
                                            }

                                            peer_cache.note_pex_discovered_peer(
                                                pid,
                                                now_ms,
                                                &known_addrs,
                                            );

                                            if is_new_peer {
                                                new_peers += 1;
                                                if let Some(from) = from_peer.as_ref() {
                                                    tracing::info!(
                                                        "discovered new peer {} from {} via pex",
                                                        pid,
                                                        from
                                                    );
                                                } else {
                                                    tracing::info!(
                                                        "discovered new peer {} via pex",
                                                        pid
                                                    );
                                                }
                                            } else {
                                                updated_peers += 1;
                                            }

                                            if dial_budget == 0 {
                                                continue;
                                            }

                                            if transfer_blocked.contains(&pid) {
                                                continue;
                                            }

                                            if connected_peers_local.contains(&pid) {
                                                continue;
                                            }
                                            if dialing_peers.contains(&pid) {
                                                continue;
                                            }

                                            // Prefer dialing by PeerId after adding addresses to the swarm's address book.
                                            if redial.contains_key(&pid) {
                                                continue;
                                            }
                                            if dial_peer_best_effort(&mut swarm, pid, &known_addrs) {
                                                dialing_peers.insert(pid);
                                                dial_budget = dial_budget.saturating_sub(1);
                                                tracing::debug!("PEX dialing discovered peer {}", pid);
                                            } else {
                                                redial.entry(pid).or_insert(RedialState {
                                                    next_attempt_at: Instant::now()
                                                        + Duration::from_millis(250),
                                                    attempts: 0,
                                                });
                                            }
                                        }

                                        if let Some(from) = from_peer.as_ref() {
                                            if new_peers > 0 || updated_peers > 0 {
                                                tracing::info!(
                                                    "PEX accepted {} peers (new={}, updated={}, refreshed={}, total={}) via {}",
                                                    new_peers + updated_peers,
                                                    new_peers,
                                                    updated_peers,
                                                    refreshed_peers,
                                                    discovered_total,
                                                    from
                                                );
                                            } else {
                                                tracing::debug!(
                                                    "PEX received {} peers via {} (all duplicates)",
                                                    discovered_total,
                                                    from
                                                );
                                            }
                                        }
                                    }
                                },
                                libp2p::request_response::Event::OutboundFailure { peer, error, request_id } => {
                                    pex_pending.remove(&request_id);
                                    pex_inflight.remove(&peer);
                                    tracing::debug!("PEX outbound failure to {}: {error:?} ({:?})", peer, request_id);
                                }
                                libp2p::request_response::Event::InboundFailure { peer, error, request_id } => {
                                    tracing::debug!("PEX inbound failure from {}: {error:?} ({:?})", peer, request_id);
                                }
                                libp2p::request_response::Event::ResponseSent { .. } => {}
                            },
                            SwarmEvent::Behaviour(BehaviourEvent::PingPng(event)) => match event {
                                libp2p::request_response::Event::Message { peer: _, message } => match message {
                                    libp2p::request_response::Message::Request { request, channel, .. } => {
                                        let pong = crate::network::ping_png::PongResponse {
                                            nonce: request.nonce,
                                            received_at_ms: crate::network::ping_png::now_ms(),
                                        };
                                        let _ = swarm.behaviour_mut().ping_png.send_response(channel, pong);
                                    }
                                    libp2p::request_response::Message::Response { response: _response, request_id } => {
                                        let Some(peer) = ping_pending.remove(&request_id) else {
                                            continue;
                                        };
                                        ping_inflight.remove(&peer);
                                        ping_failures.remove(&peer);
                                    }
                                },
                                libp2p::request_response::Event::OutboundFailure { peer, error, request_id } => {
                                    ping_pending.remove(&request_id);
                                    ping_inflight.remove(&peer);
                                    let failures = ping_failures.entry(peer).or_insert(0);
                                    *failures = failures.saturating_add(1);
                                    tracing::debug!("ping outbound failure to {}: {error:?} ({:?})", peer, request_id);
                                    if *failures >= keep_alive_cfg.max_failures {
                                        tracing::warn!("ping failure threshold reached for {}; disconnecting", peer);
                                        let _ = swarm.disconnect_peer_id(peer);
                                    }
                                }
                                libp2p::request_response::Event::InboundFailure { peer, error, request_id } => {
                                    tracing::debug!("ping inbound failure from {}: {error:?} ({:?})", peer, request_id);
                                }
                                libp2p::request_response::Event::ResponseSent { .. } => {}
                            },
                            SwarmEvent::Behaviour(BehaviourEvent::Handshake(event)) => match event {
                                libp2p::request_response::Event::Message { peer, message } => match message {
                                    libp2p::request_response::Message::Request { request, channel, .. } => {
                                        let local_fp = transfer_codec_fingerprint();
                                        let ok = request.transfer_protocol == TRANSFER_PROTOCOL
                                            && request.transfer_codec_fingerprint == local_fp;
                                        let message = if ok {
                                            None
                                        } else {
                                            Some(format!(
                                                "transfer mismatch: expected proto={} fp={}",
                                                TRANSFER_PROTOCOL,
                                                hex::encode(local_fp)
                                            ))
                                        };

                                        let response = HandshakeResponse {
                                            ok,
                                            node_version: env!("CARGO_PKG_VERSION").to_string(),
                                            transfer_protocol: TRANSFER_PROTOCOL.to_string(),
                                            transfer_codec_fingerprint: local_fp,
                                            message,
                                        };
                                        let _ = swarm
                                            .behaviour_mut()
                                            .handshake
                                            .send_response(channel, response);

                                        if ok {
                                            let is_new = peers_clone.write().await.insert(peer);
                                            if is_new {
                                                let _ = event_tx.send(NetworkEvent::PeerConnected(peer));
                                            }
                                            transfer_blocked.remove(&peer);
                                            transfer_ready.insert(peer);

                                            if pex_cfg.enable
                                                && !memory_pressure
                                                && connected_peers_local.len() < pex_cfg.target_peers
                                                && !pex_inflight.contains(&peer)
                                            {
                                                let req = crate::network::pex::PexRequest {
                                                    want: pex_cfg.want_peers as u32,
                                                };
                                                let req_id = swarm
                                                    .behaviour_mut()
                                                    .pex
                                                    .send_request(&peer, req);
                                                pex_pending.insert(req_id, peer);
                                                pex_inflight.insert(peer);
                                            }

                                            if let Some(mut queued) = transfer_buffer.remove(&peer) {
                                                let queue = transfer_queue.entry(peer).or_default();
                                                while let Some(job) = queued.pop_front() {
                                                    if queue.len() >= MAX_OUTBOUND_TRANSFER_QUEUE_PER_PEER {
                                                        if let Some(reply) = job.reply {
                                                            let _ = reply.send(Err(anyhow!(
                                                                "transfer queue full"
                                                            )));
                                                        }
                                                        continue;
                                                    }
                                                    queue.push_back(job);
                                                }
                                            }
                                            drain_transfer_queues(
                                                &mut swarm,
                                                &transfer_ready,
                                                &transfer_blocked,
                                                &mut transfer_queue,
                                                &mut transfer_pending,
                                                &mut transfer_outbound_inflight_total,
                                                &mut transfer_outbound_inflight_per_peer,
                                                max_outbound_transfer_streams_per_peer,
                                                max_outbound_transfer_streams_total,
                                            );
                                        } else {
                                            let was_ready = peers_clone.write().await.remove(&peer);
                                            if was_ready {
                                                let _ = event_tx.send(NetworkEvent::PeerDisconnected(peer));
                                            }
                                            transfer_ready.remove(&peer);
                                            transfer_blocked.insert(peer);
                                            transfer_buffer.remove(&peer);
                                            transfer_queue.remove(&peer);
                                        }
                                    }
                                    libp2p::request_response::Message::Response { response, request_id } => {
                                        let Some(peer) = handshake_pending.remove(&request_id) else {
                                            continue;
                                        };
                                        handshake_inflight.remove(&peer);

                                        if !response.ok {
                                            tracing::warn!(
                                                "handshake rejected by {}: {}",
                                                peer,
                                                response.message.as_deref().unwrap_or("no details")
                                            );
                                            let was_ready = peers_clone.write().await.remove(&peer);
                                            if was_ready {
                                                let _ = event_tx.send(NetworkEvent::PeerDisconnected(peer));
                                            }
                                            transfer_ready.remove(&peer);
                                            transfer_blocked.insert(peer);
                                            transfer_buffer.remove(&peer);
                                            transfer_queue.remove(&peer);
                                            continue;
                                        }

                                        let local_fp = transfer_codec_fingerprint();
                                        if response.transfer_protocol != TRANSFER_PROTOCOL
                                            || response.transfer_codec_fingerprint != local_fp
                                        {
                                            tracing::warn!(
                                                "handshake mismatch with {}: remote proto={} fp={} (local proto={} fp={})",
                                                peer,
                                                response.transfer_protocol,
                                                hex::encode(response.transfer_codec_fingerprint),
                                                TRANSFER_PROTOCOL,
                                                hex::encode(local_fp)
                                            );
                                            let was_ready = peers_clone.write().await.remove(&peer);
                                            if was_ready {
                                                let _ = event_tx.send(NetworkEvent::PeerDisconnected(peer));
                                            }
                                            transfer_ready.remove(&peer);
                                            transfer_blocked.insert(peer);
                                            transfer_buffer.remove(&peer);
                                            transfer_queue.remove(&peer);
                                            continue;
                                        }

                                        let is_new = peers_clone.write().await.insert(peer);
                                        if is_new {
                                            let _ = event_tx.send(NetworkEvent::PeerConnected(peer));
                                        }
                                        transfer_blocked.remove(&peer);
                                        transfer_ready.insert(peer);

                                        if pex_cfg.enable
                                            && !memory_pressure
                                            && connected_peers_local.len() < pex_cfg.target_peers
                                            && !pex_inflight.contains(&peer)
                                        {
                                            let req = crate::network::pex::PexRequest {
                                                want: pex_cfg.want_peers as u32,
                                            };
                                            let req_id = swarm.behaviour_mut().pex.send_request(&peer, req);
                                            pex_pending.insert(req_id, peer);
                                            pex_inflight.insert(peer);
                                        }

                                        if let Some(mut queued) = transfer_buffer.remove(&peer) {
                                            let queue = transfer_queue.entry(peer).or_default();
                                            while let Some(job) = queued.pop_front() {
                                                if queue.len() >= MAX_OUTBOUND_TRANSFER_QUEUE_PER_PEER {
                                                    if let Some(reply) = job.reply {
                                                        let _ =
                                                            reply.send(Err(anyhow!("transfer queue full")));
                                                    }
                                                    continue;
                                                }
                                                queue.push_back(job);
                                            }
                                        }
                                        drain_transfer_queues(
                                            &mut swarm,
                                            &transfer_ready,
                                            &transfer_blocked,
                                            &mut transfer_queue,
                                            &mut transfer_pending,
                                            &mut transfer_outbound_inflight_total,
                                            &mut transfer_outbound_inflight_per_peer,
                                            max_outbound_transfer_streams_per_peer,
                                            max_outbound_transfer_streams_total,
                                        );
                                    }
                                },
                                libp2p::request_response::Event::OutboundFailure { peer, error, request_id } => {
                                    handshake_pending.remove(&request_id);
                                    handshake_inflight.remove(&peer);
                                    if matches!(error, libp2p::request_response::OutboundFailure::UnsupportedProtocols) {
                                        tracing::debug!(
                                            "handshake unsupported by {} ({}): {:?}",
                                            peer,
                                            HANDSHAKE_PROTOCOL,
                                            request_id
                                        );
                                    } else {
                                        tracing::debug!(
                                            "handshake outbound failure to {}: {error:?} ({:?})",
                                            peer,
                                            request_id
                                        );
                                    }
                                    let was_ready = peers_clone.write().await.remove(&peer);
                                    if was_ready {
                                        let _ = event_tx.send(NetworkEvent::PeerDisconnected(peer));
                                    }
                                    transfer_ready.remove(&peer);
                                    transfer_blocked.insert(peer);
                                    transfer_buffer.remove(&peer);
                                    transfer_queue.remove(&peer);
                                }
                                libp2p::request_response::Event::InboundFailure { peer, error, request_id } => {
                                    tracing::debug!(
                                        "handshake inbound failure from {}: {error:?} ({:?})",
                                        peer,
                                        request_id
                                    );
                                }
                                libp2p::request_response::Event::ResponseSent { peer, request_id } => {
                                    tracing::trace!("handshake response sent to {} ({:?})", peer, request_id);
                                }
                            },
                            SwarmEvent::Behaviour(BehaviourEvent::Transfer(event)) => match event {
                                libp2p::request_response::Event::Message { peer, message } => {
                                    match message {
                                        libp2p::request_response::Message::Request { request, channel, .. } => {
                                            let _ = event_tx.send(NetworkEvent::TransferRequest(peer, request, channel));
                                        }
                                        libp2p::request_response::Message::Response { response, request_id } => {
                                            let pending = transfer_pending.remove(&request_id);
                                            if let Some((pending_peer, _desc, reply)) = pending {
                                                transfer_outbound_inflight_total =
                                                    transfer_outbound_inflight_total.saturating_sub(1);
                                                let should_remove = match transfer_outbound_inflight_per_peer
                                                    .get_mut(&pending_peer)
                                                {
                                                    Some(inflight) => {
                                                        *inflight = inflight.saturating_sub(1);
                                                        *inflight == 0
                                                    }
                                                    None => false,
                                                };
                                                if should_remove {
                                                    transfer_outbound_inflight_per_peer
                                                        .remove(&pending_peer);
                                                }

                                                if let Some(reply) = reply {
                                                    let _ = reply.send(Ok(response));
                                                } else {
                                                    let _ = event_tx.send(NetworkEvent::TransferResponse(peer, response));
                                                }
                                            } else {
                                                let _ = event_tx.send(NetworkEvent::TransferResponse(peer, response));
                                            }
                                            drain_transfer_queues(
                                                &mut swarm,
                                                &transfer_ready,
                                                &transfer_blocked,
                                                &mut transfer_queue,
                                                &mut transfer_pending,
                                                &mut transfer_outbound_inflight_total,
                                                &mut transfer_outbound_inflight_per_peer,
                                                max_outbound_transfer_streams_per_peer,
                                                max_outbound_transfer_streams_total,
                                            );
                                        }
                                    }
                                }
                                libp2p::request_response::Event::OutboundFailure { peer, error, request_id } => {
                                    let pending = transfer_pending.remove(&request_id);
                                    let req_desc = pending
                                        .as_ref()
                                        .map(|(_peer, d, _reply)| d.clone())
                                        .unwrap_or_else(|| "<unknown>".to_string());

                                    if let Some((pending_peer, _desc, reply)) = pending {
                                        transfer_outbound_inflight_total =
                                            transfer_outbound_inflight_total.saturating_sub(1);
                                        let should_remove = match transfer_outbound_inflight_per_peer
                                            .get_mut(&pending_peer)
                                        {
                                            Some(inflight) => {
                                                *inflight = inflight.saturating_sub(1);
                                                *inflight == 0
                                            }
                                            None => false,
                                        };
                                        if should_remove {
                                            transfer_outbound_inflight_per_peer
                                                .remove(&pending_peer);
                                        }

                                        if let Some(reply) = reply {
                                            let _ = reply.send(Err(anyhow!(
                                                "transfer outbound failure to {}: {error:?}",
                                                peer
                                            )));
                                        }
                                    }
                                    match &error {
                                        libp2p::request_response::OutboundFailure::UnsupportedProtocols => {
                                            tracing::debug!(
                                                "transfer unsupported by {} ({}): {:?}",
                                                peer,
                                                TRANSFER_PROTOCOL,
                                                request_id
                                            );
                                        }
                                        libp2p::request_response::OutboundFailure::Io(err)
                                            if err.kind() == std::io::ErrorKind::InvalidData =>
                                        {
                                            tracing::warn!(
                                                "transfer decode failure from {} ({}): {:?} req={} ({}) (peer may be running an incompatible ONVM version)",
                                                peer,
                                                TRANSFER_PROTOCOL,
                                                request_id,
                                                req_desc,
                                                err
                                            );
                                            let was_ready = peers_clone.write().await.remove(&peer);
                                            if was_ready {
                                                let _ = event_tx.send(NetworkEvent::PeerDisconnected(peer));
                                            }
                                            transfer_ready.remove(&peer);
                                            transfer_blocked.insert(peer);
                                            transfer_buffer.remove(&peer);
                                            transfer_queue.remove(&peer);
                                        }
                                        libp2p::request_response::OutboundFailure::Io(err)
                                            if err.to_string().contains("max sub-streams reached") =>
                                        {
                                            tracing::debug!(
                                                "transfer outbound backpressure to {}: {} ({:?}) req={}",
                                                peer,
                                                err,
                                                request_id,
                                                req_desc
                                            );
                                        }
                                        _ => {
                                            tracing::warn!(
                                                "transfer outbound failure to {}: {error:?} ({:?})",
                                                peer,
                                                request_id
                                            );
                                        }
                                    }
                                    drain_transfer_queues(
                                        &mut swarm,
                                        &transfer_ready,
                                        &transfer_blocked,
                                        &mut transfer_queue,
                                        &mut transfer_pending,
                                        &mut transfer_outbound_inflight_total,
                                        &mut transfer_outbound_inflight_per_peer,
                                        max_outbound_transfer_streams_per_peer,
                                        max_outbound_transfer_streams_total,
                                    );
                                }
                                libp2p::request_response::Event::InboundFailure { peer, error, request_id } => {
                                    match &error {
                                        libp2p::request_response::InboundFailure::UnsupportedProtocols => {
                                            tracing::debug!(
                                                "transfer unsupported by {} ({}): {:?}",
                                                peer,
                                                TRANSFER_PROTOCOL,
                                                request_id
                                            );
                                        }
                                        libp2p::request_response::InboundFailure::Io(err)
                                            if err.kind() == std::io::ErrorKind::InvalidData =>
                                        {
                                            tracing::warn!(
                                                "transfer decode failure from {} ({}): {:?} ({}) (peer may be running an incompatible ONVM version)",
                                                peer,
                                                TRANSFER_PROTOCOL,
                                                request_id,
                                                err
                                            );
                                            let was_ready = peers_clone.write().await.remove(&peer);
                                            if was_ready {
                                                let _ = event_tx.send(NetworkEvent::PeerDisconnected(peer));
                                            }
                                            transfer_ready.remove(&peer);
                                            transfer_blocked.insert(peer);
                                            transfer_buffer.remove(&peer);
                                            transfer_queue.remove(&peer);
                                        }
                                        _ => {
                                            tracing::warn!(
                                                "transfer inbound failure from {}: {error:?} ({:?})",
                                                peer,
                                                request_id
                                            );
                                        }
                                    }
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
                        if transfer_blocked.contains(&job.peer) {
                            tracing::debug!("dropping transfer request to blocked peer {}", job.peer);
                            continue;
                        }
                        if !transfer_ready.contains(&job.peer) {
                            let queue = transfer_buffer.entry(job.peer).or_default();
                            if queue.len() >= 64 {
                                if let Some(dropped) = queue.pop_front() {
                                    if let Some(reply) = dropped.reply {
                                        let _ = reply.send(Err(anyhow!(
                                            "transfer buffer full (peer not ready)"
                                        )));
                                    }
                                }
                            }
                            queue.push_back(job);
                            continue;
                        }
                        let peer = job.peer;
                        let queue = transfer_queue.entry(peer).or_default();
                        if queue.len() >= MAX_OUTBOUND_TRANSFER_QUEUE_PER_PEER {
                            tracing::debug!(
                                "dropping transfer request to {} due to outbound queue full",
                                peer
                            );
                            if let Some(reply) = job.reply {
                                let _ = reply.send(Err(anyhow!("transfer queue full")));
                            }
                            continue;
                        }
                        queue.push_back(job);
                        drain_transfer_queues(
                            &mut swarm,
                            &transfer_ready,
                            &transfer_blocked,
                            &mut transfer_queue,
                            &mut transfer_pending,
                            &mut transfer_outbound_inflight_total,
                            &mut transfer_outbound_inflight_per_peer,
                            max_outbound_transfer_streams_per_peer,
                            max_outbound_transfer_streams_total,
                        );
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
                        let envelope = match sign_network_message(&signing_keys, &msg) {
                            Ok(env) => env,
                            Err(e) => {
                                tracing::warn!("failed to sign message: {}", e);
                                continue;
                            }
                        };
                        let data = match bincode::serde::encode_to_vec(
                            &envelope,
                            bincode::config::standard(),
                        ) {
                            Ok(d) => d,
                            Err(e) => {
                                tracing::warn!("failed to encode signed message: {}", e);
                                continue;
                            }
                        };

                        tracing::debug!("publishing {:?}", describe_msg(&msg));
                        if let Err(err) = swarm.behaviour_mut().gossipsub.publish(topic, data) {
                            tracing::debug!("publish failed: {err:?}");
                        }
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
