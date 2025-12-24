use crate::types::{
    AggregatedReceipt, ChunkId, CommitteeCertificate, ExecutionReceipt, Manifest, ManifestId,
    NodeId, Object, ObjectId, ProgramAnnouncement, ProgramId, ReceiptId, StateWrite,
};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};

/// Object announcement message (gossip-safe, metadata only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectAnnouncement {
    pub object_id: ObjectId,
    pub manifest_id: ManifestId,
    pub total_size: u64,
    pub chunk_count: u32,
    pub providers: Vec<String>,
}

/// Manifest request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestRequest {
    pub object_id: ObjectId,
}

/// Manifest response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestResponse {
    pub manifest: Option<Manifest>,
}

/// Chunk request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRequest {
    pub chunk_id: ChunkId,
}

/// Chunk response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkResponse {
    pub chunk_id: ChunkId,
    pub data: Option<Vec<u8>>,
}

/// Batch chunk request (fetch multiple chunks in one round-trip)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchChunkRequest {
    pub chunk_ids: Vec<ChunkId>,
}

/// Batch chunk response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchChunkResponse {
    pub chunks: Vec<(ChunkId, Option<Vec<u8>>)>,
}

/// Object availability query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectAvailabilityRequest {
    pub object_id: ObjectId,
}

/// Object availability response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectAvailabilityResponse {
    pub object_id: ObjectId,
    pub has_object: bool,
    pub has_manifest: bool,
    pub available_chunks: Vec<ChunkId>,
    pub missing_chunks: Vec<ChunkId>,
    pub metadata: Option<Object>,
}

/// Object metadata request (for discovering unknown programs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMetadataRequest {
    pub object_id: ObjectId,
}

/// Object metadata response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMetadataResponse {
    pub object_id: ObjectId,
    pub metadata: Option<Object>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedReceiptBundle {
    pub receipt: AggregatedReceipt,
    pub committee: CommitteeCertificate,
    #[serde(default)]
    pub state_writes: Vec<StateWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransitionProposal {
    pub receipt: ExecutionReceipt,
    pub committee_epoch: u64,
    pub state_writes: Vec<StateWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransitionVote {
    pub program_id: ProgramId,
    pub committee_epoch: u64,
    pub receipt_id: ReceiptId,
    pub signer: NodeId,
    pub signature: crate::crypto::bls::BlsSignature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramHead {
    pub program_id: ProgramId,
    pub height: u64,
    pub state_root: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizedTransitionRequest {
    pub program_id: ProgramId,
    pub height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizedTransitionResponse {
    pub program_id: ProgramId,
    pub height: u64,
    pub bundle: Option<AggregatedReceiptBundle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderExecutionRequest {
    pub request_id: [u8; 32],
    pub program_id: ProgramId,
    pub calldata: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderExecutionResponse {
    pub request_id: [u8; 32],
    pub outcome: Option<crate::wasm_runtime::ExecutionOutcome>,
    pub redirect: Option<NodeId>,
    pub error: Option<String>,
}

/// Unified protocol message for gossipsub
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnifiedProtocolMessage {
    ObjectAnnouncement(ObjectAnnouncement),
    ObjectMetadata(Object),
    ObjectMetadataRequest(ObjectMetadataRequest),
    ManifestRequest(ManifestRequest),
    ManifestResponse(Manifest),
    ProgramAnnouncement(ProgramAnnouncement),
    AggregatedReceipt(AggregatedReceiptBundle),
    StateTransitionProposal(StateTransitionProposal),
    StateTransitionVote(StateTransitionVote),
    ProgramHead(ProgramHead),
}

/// Unified request-response protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnifiedRequest {
    GetManifest(ManifestRequest),
    GetChunk(ChunkRequest),
    GetChunks(BatchChunkRequest),
    GetObjectAvailability(ObjectAvailabilityRequest),
    GetObjectMetadata(ObjectMetadataRequest),
    GetFinalizedTransition(FinalizedTransitionRequest),
    ExecuteViaLeader(LeaderExecutionRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnifiedResponse {
    Manifest(ManifestResponse),
    Chunk(ChunkResponse),
    Chunks(BatchChunkResponse),
    ObjectAvailability(ObjectAvailabilityResponse),
    ObjectMetadata(ObjectMetadataResponse),
    FinalizedTransition(FinalizedTransitionResponse),
    ExecuteViaLeader(LeaderExecutionResponse),
}

impl ObjectAnnouncement {
    pub fn from_object(object: &Object, local_peer: &PeerId) -> Self {
        Self {
            object_id: object.id,
            manifest_id: object.manifest_id,
            total_size: object.total_size,
            chunk_count: object.chunk_count,
            providers: vec![local_peer.to_string()],
        }
    }
}
