use crate::types::{
    AggregatedReceipt, ChunkId, CommitteeCertificate, ExecutionReceipt, Manifest, ManifestId,
    NodeId, Object, ObjectId, ProgramAnnouncement, ProgramId, ProgramManifest, ReceiptId,
    StateWrite,
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

/// Chunk part request (range within a chunk), used for fixed-size subchunk transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkPartRequest {
    pub chunk_id: ChunkId,
    /// Byte offset within the chunk.
    pub offset: u32,
    /// Requested length in bytes (peers should cap this to a safe maximum).
    pub length: u32,
}

/// Chunk part response (range within a chunk).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkPartResponse {
    pub chunk_id: ChunkId,
    /// Byte offset within the chunk for this response.
    pub offset: u32,
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
pub struct ProgramManifestRequest {
    pub program_id: ProgramId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramManifestResponse {
    pub program_id: ProgramId,
    pub manifest: Option<ProgramManifest>,
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
    GetChunkPart(ChunkPartRequest),
    GetChunks(BatchChunkRequest),
    GetObjectAvailability(ObjectAvailabilityRequest),
    GetObjectMetadata(ObjectMetadataRequest),
    GetFinalizedTransition(FinalizedTransitionRequest),
    ExecuteViaLeader(LeaderExecutionRequest),
    GetProgramManifest(ProgramManifestRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnifiedResponse {
    Manifest(ManifestResponse),
    Chunk(ChunkResponse),
    ChunkPart(ChunkPartResponse),
    Chunks(BatchChunkResponse),
    ObjectAvailability(ObjectAvailabilityResponse),
    ObjectMetadata(ObjectMetadataResponse),
    FinalizedTransition(FinalizedTransitionResponse),
    ExecuteViaLeader(LeaderExecutionResponse),
    ProgramManifest(ProgramManifestResponse),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_manifest_request_bincode_roundtrip() {
        let program_id = ProgramId([7u8; 32]);
        let req = UnifiedRequest::GetProgramManifest(ProgramManifestRequest { program_id });

        let bytes = bincode::serde::encode_to_vec(&req, bincode::config::standard()).unwrap();
        let (decoded, _): (UnifiedRequest, _) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();

        match decoded {
            UnifiedRequest::GetProgramManifest(decoded_req) => {
                assert_eq!(decoded_req.program_id.0, [7u8; 32]);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn program_manifest_response_bincode_roundtrip() {
        let program_id = ProgramId([9u8; 32]);
        let resp = UnifiedResponse::ProgramManifest(ProgramManifestResponse {
            program_id,
            manifest: None,
        });

        let bytes = bincode::serde::encode_to_vec(&resp, bincode::config::standard()).unwrap();
        let (decoded, _): (UnifiedResponse, _) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard()).unwrap();

        match decoded {
            UnifiedResponse::ProgramManifest(decoded_resp) => {
                assert_eq!(decoded_resp.program_id.0, [9u8; 32]);
                assert!(decoded_resp.manifest.is_none());
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }
}
