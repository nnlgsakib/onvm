use crate::crypto::keys::NodeKeys;
use crate::network::codec::{
    sign_network_message, transfer_codec_fingerprint, verify_signed_message, HandshakeRequest,
    HandshakeResponse, NetworkMessage, ProviderKind, SignedNetworkMessage, TransferRequest,
    TransferResponse, HANDSHAKE_PROTOCOL, TOPIC_BLOBS, TOPIC_BLOCKS, TOPIC_PROGRAMS,
    TRANSFER_PROTOCOL,
};
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
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::{NetworkBehaviour, StreamProtocol, SwarmEvent};
use libp2p::yamux;
use libp2p::{Multiaddr, PeerId, SwarmBuilder};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
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
    pub enable_mdns: bool,
    pub require_encryption: bool,
    pub max_inbound_connections: usize,
    pub max_inbound_streams: usize,
    pub max_gossip_bytes: usize,
    pub bootnodes: Vec<Multiaddr>,
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
    mdns: Toggle<mdns::tokio::Behaviour>,
    kademlia: Kademlia<MemoryStore>,
    handshake: cbor::Behaviour<HandshakeRequest, HandshakeResponse>,
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

        let kademlia = crate::network::dht::build_kademlia(peer_id);

        let handshake_protocol = StreamProtocol::new(HANDSHAKE_PROTOCOL);
        let handshake_config = libp2p::request_response::Config::default()
            .with_request_timeout(Duration::from_secs(10))
            .with_max_concurrent_streams(config.max_inbound_streams);
        let handshake = cbor::Behaviour::new(
            std::iter::once((handshake_protocol, ProtocolSupport::Full)),
            handshake_config,
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
            kademlia,
            handshake,
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
        let mut recent_ids: std::collections::VecDeque<gossipsub::MessageId> =
            std::collections::VecDeque::new();
        let dedupe_window: usize = 256;

        let peers_shared = Arc::new(RwLock::new(HashSet::new()));
        let peers_clone = Arc::clone(&peers_shared);
        let signing_keys = identity.clone();

        tokio::spawn(async move {
            let mut handshake_pending: HashMap<
                libp2p::request_response::OutboundRequestId,
                PeerId,
            > = HashMap::new();
            let mut handshake_inflight: HashSet<PeerId> = HashSet::new();
            let mut transfer_ready: HashSet<PeerId> = HashSet::new();
            let mut transfer_blocked: HashSet<PeerId> = HashSet::new();
            let mut transfer_buffer: HashMap<PeerId, VecDeque<TransferJob>> = HashMap::new();

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
                                    match bincode::serde::decode_from_slice::<SignedNetworkMessage, _>(
                                        &message.data,
                                        bincode::config::standard(),
                                    ) {
                                        Ok((env, _)) => {
                                            match verify_signed_message(&env) {
                                                Ok(msg) => {
                                                    tracing::info!("inbound {:?} from {}", describe_msg(&msg), propagation_source);
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
                                let mut addr_with_peer = address.clone();
                                addr_with_peer.push(libp2p::multiaddr::Protocol::P2p(peer_id.into()));
                                tracing::info!("listening on {}", addr_with_peer);
                                let _ = event_tx.send(NetworkEvent::Listening(addr_with_peer));
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                                tracing::info!("connected to peer {} at {:?}", peer_id, endpoint.get_remote_address());
                                peers_clone.write().await.insert(peer_id);

                                swarm.behaviour_mut().kademlia.add_address(&peer_id, endpoint.get_remote_address().clone());
                                let _ = swarm.behaviour_mut().kademlia.bootstrap();

                                if !handshake_inflight.contains(&peer_id) && !transfer_blocked.contains(&peer_id) {
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
                                    tracing::debug!("sent handshake request {:?} to {}", req_id, peer_id);
                                }

                                let _ = event_tx.send(NetworkEvent::PeerConnected(peer_id));
                            }
                            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                                tracing::warn!("disconnected from peer {}", peer_id);
                                peers_clone.write().await.remove(&peer_id);
                                handshake_inflight.remove(&peer_id);
                                handshake_pending.retain(|_, peer| *peer != peer_id);
                                transfer_ready.remove(&peer_id);
                                transfer_blocked.remove(&peer_id);
                                transfer_buffer.remove(&peer_id);
                                let _ = event_tx.send(NetworkEvent::PeerDisconnected(peer_id));
                            }
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
                                            transfer_blocked.remove(&peer);
                                            transfer_ready.insert(peer);
                                            if let Some(mut queued) = transfer_buffer.remove(&peer) {
                                                while let Some(job) = queued.pop_front() {
                                                    let req_id = swarm
                                                        .behaviour_mut()
                                                        .transfer
                                                        .send_request(&job.peer, job.req);
                                                    tracing::debug!(
                                                        "sent buffered transfer request {:?} to {}",
                                                        req_id,
                                                        job.peer
                                                    );
                                                }
                                            }
                                        } else {
                                            transfer_ready.remove(&peer);
                                            transfer_blocked.insert(peer);
                                            transfer_buffer.remove(&peer);
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
                                            transfer_ready.remove(&peer);
                                            transfer_blocked.insert(peer);
                                            transfer_buffer.remove(&peer);
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
                                            transfer_ready.remove(&peer);
                                            transfer_blocked.insert(peer);
                                            transfer_buffer.remove(&peer);
                                            continue;
                                        }

                                        transfer_blocked.remove(&peer);
                                        transfer_ready.insert(peer);

                                        if let Some(mut queued) = transfer_buffer.remove(&peer) {
                                            while let Some(job) = queued.pop_front() {
                                                let req_id = swarm
                                                    .behaviour_mut()
                                                    .transfer
                                                    .send_request(&job.peer, job.req);
                                                tracing::debug!(
                                                    "sent buffered transfer request {:?} to {}",
                                                    req_id,
                                                    job.peer
                                                );
                                            }
                                        }
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
                                    transfer_ready.remove(&peer);
                                    transfer_blocked.insert(peer);
                                    transfer_buffer.remove(&peer);
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
                                        libp2p::request_response::Message::Response { response, .. } => {
                                            let _ = event_tx.send(NetworkEvent::TransferResponse(peer, response));
                                        }
                                    }
                                }
                                libp2p::request_response::Event::OutboundFailure { peer, error, request_id } => {
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
                                                "transfer decode failure from {} ({}): {:?} (peer may be running an incompatible ONVM version)",
                                                peer,
                                                TRANSFER_PROTOCOL,
                                                request_id
                                            );
                                            transfer_ready.remove(&peer);
                                            transfer_blocked.insert(peer);
                                            transfer_buffer.remove(&peer);
                                        }
                                        _ => {
                                            tracing::warn!(
                                                "transfer outbound failure to {}: {error:?} ({:?})",
                                                peer,
                                                request_id
                                            );
                                        }
                                    }
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
                                                "transfer decode failure from {} ({}): {:?} (peer may be running an incompatible ONVM version)",
                                                peer,
                                                TRANSFER_PROTOCOL,
                                                request_id
                                            );
                                            transfer_ready.remove(&peer);
                                            transfer_blocked.insert(peer);
                                            transfer_buffer.remove(&peer);
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
                                queue.pop_front();
                            }
                            queue.push_back(job);
                            continue;
                        }
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
                        let msg_id = gossipsub::MessageId::from(
                            blake3::hash(&data).as_bytes().to_vec(),
                        );
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
