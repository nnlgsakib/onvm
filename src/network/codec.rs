use crate::crypto::keys::NodeKeys;
use crate::types::{
    BlobId, BlobMetadata, ChunkDescriptor, ChunkId, ComputeOp, Manifest, ManifestId, NodeId,
    Object, ObjectId, ObjectType, ProgramId, ProgramMetadata, StateWrite,
};
use anyhow::{anyhow, Context, Result};
use blake3;
use ed25519_dalek::{PublicKey as EdPublicKey, Signature as EdSignature};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub const TOPIC_BLOBS: &str = "onvm-blobs";
pub const TOPIC_PROGRAMS: &str = "onvm-programs";
pub const TOPIC_BLOCKS: &str = "onvm-dag";

pub const TRANSFER_PROTOCOL: &str = "/onvm/transfer/3.0.0";
pub const HANDSHAKE_PROTOCOL: &str = "/onvm/handshake/1.0.0";

pub fn transfer_codec_fingerprint() -> [u8; 32] {
    static FINGERPRINT: OnceLock<[u8; 32]> = OnceLock::new();
    *FINGERPRINT.get_or_init(|| {
        let mut hasher = blake3::Hasher::new();

        fn hash_transfer_wire<T: Serialize>(hasher: &mut blake3::Hasher, label: &[u8], value: &T) {
            hasher.update(label);
            // Fingerprint must match the *actual transfer wire codec*.
            match cbor4ii::serde::to_vec(Vec::new(), value) {
                Ok(bytes) => {
                    hasher.update(&bytes);
                }
                Err(_) => {
                    hasher.update(b"encode-error");
                }
            }
        }

        // Include the protocol name itself so any intentional protocol bump
        // necessarily yields a different fingerprint.
        hasher.update(TRANSFER_PROTOCOL.as_bytes());

        let program_id = ProgramId([0u8; 32]);
        let blob_id = BlobId([1u8; 32]);
        let publisher = NodeId([2u8; 32]);
        let object_id = ObjectId([3u8; 32]);
        let manifest_id = ManifestId([4u8; 32]);
        let chunk_id = ChunkId([5u8; 32]);
        let exec_id = [6u8; 32];

        let blob_meta = BlobMetadata {
            id: blob_id.clone(),
            publisher: publisher.clone(),
            size: 1,
            mime: Some("application/octet-stream".to_string()),
            chunk_sizes: vec![1],
            chunk_hashes: vec![[7u8; 32]],
            merkle_root: [8u8; 32],
            data_shards: 1,
            parity_shards: 1,
        };
        let blob_broadcast = BlobBroadcast { meta: blob_meta };

        let program_meta = ProgramMetadata {
            id: program_id.clone(),
            publisher: publisher.clone(),
            size: 1,
            entrypoint: "main".to_string(),
            blob_refs: vec![blob_id.clone()],
            deploy_salt: vec![9u8],
        };
        let program_broadcast = ProgramBroadcast {
            meta: program_meta,
            wasm: vec![0x00],
        };

        let compute_op = ComputeOp {
            program_id: program_id.clone(),
            input: blob_id.clone(),
            output: BlobMetadata {
                id: blob_id.clone(),
                publisher: publisher.clone(),
                size: 1,
                mime: None,
                chunk_sizes: vec![1],
                chunk_hashes: vec![[10u8; 32]],
                merkle_root: [11u8; 32],
                data_shards: 1,
                parity_shards: 1,
            },
            fuel_used: 1,
            state_root: [12u8; 32],
            state_writes: vec![StateWrite {
                key: vec![0x01],
                value: Some(vec![0x02]),
            }],
        };
        let exec_broadcast = ExecutionBroadcast {
            dag_id: exec_id,
            op: compute_op,
        };

        let sync_snapshot = SyncSnapshot {
            programs: vec![program_id.clone()],
            executions: vec![exec_id],
        };
        let sync_delta = SyncDelta {
            missing_programs: vec![program_id.clone()],
            missing_executions: vec![exec_id],
        };

        let state_resp = StateResponse {
            program_id: program_id.clone(),
            state_root: [13u8; 32],
            state_entries: vec![(vec![0xAA], vec![0xBB])],
        };

        let inventory = DagInventory {
            programs: vec![[14u8; 32]],
            program_bloom: Some(BloomFilter {
                bits: vec![0u8, 1u8, 2u8],
                k: 3,
                salt: [0u8; 32],
            }),
            blobs: vec![BlobInventoryEntry {
                id: [15u8; 32],
                has_data: true,
                locations: vec!["local".to_string()],
            }],
            executions: vec![[16u8; 32]],
        };

        let object = Object {
            id: object_id,
            object_type: ObjectType::WasmProgram {
                entrypoint: "main".to_string(),
                wasm_version: Some("v1".to_string()),
                source_language: Some("rust".to_string()),
                compiler: Some("rustc".to_string()),
                blob_refs: vec![ObjectId([17u8; 32])],
                deploy_salt: vec![0x01, 0x02],
            },
            total_size: 1,
            chunk_count: 1,
            manifest_id,
            publisher,
            created_at: 1,
        };
        let manifest = Manifest {
            object_id,
            content_hash: [18u8; 32],
            chunks: vec![ChunkDescriptor {
                chunk_id,
                size: 1,
                offset: 0,
            }],
            version: 1,
        };

        let unified_requests = [
            crate::network::unified_protocol::UnifiedRequest::GetObjectMetadata(
                crate::network::unified_protocol::ObjectMetadataRequest { object_id },
            ),
            crate::network::unified_protocol::UnifiedRequest::GetManifest(
                crate::network::unified_protocol::ManifestRequest { object_id },
            ),
            crate::network::unified_protocol::UnifiedRequest::GetChunk(
                crate::network::unified_protocol::ChunkRequest { chunk_id },
            ),
            crate::network::unified_protocol::UnifiedRequest::GetChunkPart(
                crate::network::unified_protocol::ChunkPartRequest {
                    chunk_id,
                    offset: 0,
                    length: 1,
                },
            ),
            crate::network::unified_protocol::UnifiedRequest::GetChunks(
                crate::network::unified_protocol::BatchChunkRequest {
                    chunk_ids: vec![chunk_id],
                },
            ),
            crate::network::unified_protocol::UnifiedRequest::GetObjectAvailability(
                crate::network::unified_protocol::ObjectAvailabilityRequest { object_id },
            ),
            crate::network::unified_protocol::UnifiedRequest::GetFinalizedTransition(
                crate::network::unified_protocol::FinalizedTransitionRequest {
                    program_id: program_id.clone(),
                    height: 1,
                },
            ),
            crate::network::unified_protocol::UnifiedRequest::ExecuteViaLeader(
                crate::network::unified_protocol::LeaderExecutionRequest {
                    request_id: [19u8; 32],
                    program_id: program_id.clone(),
                    calldata: vec![0x01],
                },
            ),
            crate::network::unified_protocol::UnifiedRequest::GetProgramManifest(
                crate::network::unified_protocol::ProgramManifestRequest {
                    program_id: program_id.clone(),
                },
            ),
        ];

        let unified_responses = [
            crate::network::unified_protocol::UnifiedResponse::ObjectMetadata(
                crate::network::unified_protocol::ObjectMetadataResponse {
                    object_id,
                    metadata: Some(object.clone()),
                },
            ),
            crate::network::unified_protocol::UnifiedResponse::Manifest(
                crate::network::unified_protocol::ManifestResponse {
                    manifest: Some(manifest.clone()),
                },
            ),
            crate::network::unified_protocol::UnifiedResponse::Chunk(
                crate::network::unified_protocol::ChunkResponse {
                    chunk_id,
                    data: Some(vec![0x01, 0x02]),
                },
            ),
            crate::network::unified_protocol::UnifiedResponse::ChunkPart(
                crate::network::unified_protocol::ChunkPartResponse {
                    chunk_id,
                    offset: 0,
                    data: Some(vec![0x04]),
                },
            ),
            crate::network::unified_protocol::UnifiedResponse::Chunks(
                crate::network::unified_protocol::BatchChunkResponse {
                    chunks: vec![(chunk_id, Some(vec![0x03]))],
                },
            ),
            crate::network::unified_protocol::UnifiedResponse::ObjectAvailability(
                crate::network::unified_protocol::ObjectAvailabilityResponse {
                    object_id,
                    has_object: true,
                    has_manifest: true,
                    available_chunks: vec![chunk_id],
                    missing_chunks: vec![ChunkId([20u8; 32])],
                    metadata: Some(object.clone()),
                },
            ),
            crate::network::unified_protocol::UnifiedResponse::FinalizedTransition(
                crate::network::unified_protocol::FinalizedTransitionResponse {
                    program_id: program_id.clone(),
                    height: 1,
                    bundle: None,
                },
            ),
            crate::network::unified_protocol::UnifiedResponse::ExecuteViaLeader(
                crate::network::unified_protocol::LeaderExecutionResponse {
                    request_id: [19u8; 32],
                    outcome: None,
                    redirect: None,
                    error: Some("not available".to_string()),
                },
            ),
            crate::network::unified_protocol::UnifiedResponse::ProgramManifest(
                crate::network::unified_protocol::ProgramManifestResponse {
                    program_id: program_id.clone(),
                    manifest: None,
                },
            ),
        ];

        // Requests
        hash_transfer_wire(
            &mut hasher,
            b"req:program",
            &TransferRequest::Program(program_id.clone()),
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:program_chunk",
            &TransferRequest::ProgramChunk {
                id: program_id.clone(),
                chunk_idx: 1,
            },
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:blob",
            &TransferRequest::Blob(blob_id.clone()),
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:blob_chunk",
            &TransferRequest::BlobChunk {
                id: blob_id.clone(),
                chunk_idx: 1,
            },
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:execution",
            &TransferRequest::Execution(exec_id),
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:push_program",
            &TransferRequest::PushProgram(program_broadcast.clone()),
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:push_program_chunk",
            &TransferRequest::PushProgramChunk {
                id: program_id.clone(),
                chunk_idx: 1,
                chunk_data: vec![0x01, 0x02],
            },
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:push_blob",
            &TransferRequest::PushBlob(blob_broadcast.clone()),
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:push_execution",
            &TransferRequest::PushExecution(exec_broadcast.clone()),
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:sync",
            &TransferRequest::Sync(sync_snapshot),
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:state_request",
            &TransferRequest::StateRequest(StateRequest {
                program_id: program_id.clone(),
            }),
        );
        hash_transfer_wire(
            &mut hasher,
            b"req:inventory_request",
            &TransferRequest::InventoryRequest,
        );
        for (idx, req) in unified_requests.iter().enumerate() {
            hash_transfer_wire(
                &mut hasher,
                format!("req:unified:{idx}").as_bytes(),
                &TransferRequest::Unified(req.clone()),
            );
        }

        // Responses
        hash_transfer_wire(
            &mut hasher,
            b"resp:program",
            &TransferResponse::Program(Some(program_broadcast.clone())),
        );
        hash_transfer_wire(
            &mut hasher,
            b"resp:program_chunk",
            &TransferResponse::ProgramChunk {
                id: program_id.clone(),
                chunk_idx: 1,
                chunk_data: Some(vec![0x01]),
            },
        );
        hash_transfer_wire(
            &mut hasher,
            b"resp:blob",
            &TransferResponse::Blob(Some(blob_broadcast.clone())),
        );
        hash_transfer_wire(
            &mut hasher,
            b"resp:blob_chunk",
            &TransferResponse::BlobChunk {
                id: blob_id.clone(),
                chunk_idx: 1,
                shards: Some(vec![(0u8, vec![0x01]), (1u8, vec![0x02])]),
            },
        );
        hash_transfer_wire(
            &mut hasher,
            b"resp:execution",
            &TransferResponse::Execution(Some(exec_broadcast)),
        );
        hash_transfer_wire(
            &mut hasher,
            b"resp:sync",
            &TransferResponse::Sync(sync_delta),
        );
        hash_transfer_wire(&mut hasher, b"resp:ack", &TransferResponse::Ack);
        hash_transfer_wire(
            &mut hasher,
            b"resp:state_response",
            &TransferResponse::StateResponse(state_resp),
        );
        hash_transfer_wire(
            &mut hasher,
            b"resp:inventory",
            &TransferResponse::Inventory(inventory),
        );
        for (idx, resp) in unified_responses.iter().enumerate() {
            hash_transfer_wire(
                &mut hasher,
                format!("resp:unified:{idx}").as_bytes(),
                &TransferResponse::Unified(resp.clone()),
            );
        }

        let hash = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(hash.as_bytes());
        out
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeRequest {
    pub node_version: String,
    pub transfer_protocol: String,
    pub transfer_codec_fingerprint: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub ok: bool,
    pub node_version: String,
    pub transfer_protocol: String,
    pub transfer_codec_fingerprint: [u8; 32],
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum ProviderKind {
    Program,
    Blob,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedNetworkMessage {
    pub payload: Vec<u8>,
    pub publisher: crate::types::NodeId,
    pub public_key: [u8; 32],
    pub signature: Vec<u8>,
}

pub fn sign_network_message(keys: &NodeKeys, msg: &NetworkMessage) -> Result<SignedNetworkMessage> {
    let payload =
        bincode::serde::encode_to_vec(msg, bincode::config::standard()).context("encode msg")?;
    let signature = keys.sign(&payload).to_bytes();
    Ok(SignedNetworkMessage {
        payload,
        publisher: keys.node_id.clone(),
        public_key: keys.keypair.public.to_bytes(),
        signature: signature.to_vec(),
    })
}

pub fn verify_signed_message(env: &SignedNetworkMessage) -> Result<NetworkMessage> {
    let pubkey =
        EdPublicKey::from_bytes(&env.public_key).map_err(|e| anyhow!("pubkey parse: {e}"))?;
    let expected_node = crate::types::NodeId::from_public_key(pubkey.as_bytes());
    if expected_node != env.publisher {
        return Err(anyhow!("publisher mismatch"));
    }
    let sig = EdSignature::from_bytes(&env.signature).map_err(|e| anyhow!("sig parse: {e}"))?;
    pubkey
        .verify_strict(&env.payload, &sig)
        .map_err(|_| anyhow!("signature invalid"))?;
    let msg: NetworkMessage =
        bincode::serde::decode_from_slice(&env.payload, bincode::config::standard())
            .map_err(|e| anyhow!("decode payload: {e}"))?
            .0;
    Ok(msg)
}

/// Direct transfer request/response messages used by request-response protocols.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TransferRequest {
    Program(ProgramId),
    ProgramChunk {
        id: ProgramId,
        chunk_idx: usize,
    },
    Blob(BlobId),
    BlobChunk {
        id: BlobId,
        chunk_idx: u32,
    },
    Execution([u8; 32]),
    PushProgram(ProgramBroadcast),
    PushProgramChunk {
        id: ProgramId,
        chunk_idx: usize,
        chunk_data: Vec<u8>,
    },
    PushBlob(BlobBroadcast),
    PushExecution(ExecutionBroadcast),
    Sync(SyncSnapshot),
    Unified(crate::network::unified_protocol::UnifiedRequest),
    StateRequest(StateRequest),
    InventoryRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
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
    Inventory(DagInventory),
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BlobInventoryEntry {
    pub id: [u8; 32],
    pub has_data: bool,
    pub locations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
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
    #[serde(default = "default_bloom_salt")]
    pub salt: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramSyncRequest {
    pub bloom: BloomFilter,
}

impl BloomFilter {
    pub fn new(size_bytes: usize, k: u8) -> Self {
        let mut salt = [0u8; 32];
        OsRng.fill_bytes(&mut salt);
        Self {
            bits: vec![0u8; size_bytes.max(1)],
            k,
            salt,
        }
    }

    pub fn insert(&mut self, data: &[u8]) {
        for i in 0..self.k {
            let mut key = self.salt;
            key[0] ^= i;
            let hash = blake3::keyed_hash(&key, data);
            self.set_bit(hash.as_bytes());
        }
    }

    pub fn contains(&self, data: &[u8]) -> bool {
        for i in 0..self.k {
            let mut key = self.salt;
            key[0] ^= i;
            let hash = blake3::keyed_hash(&key, data);
            if !self.get_bit(hash.as_bytes()) {
                return false;
            }
        }
        true
    }

    fn set_bit(&mut self, hash: &[u8]) {
        let Some(prefix) = hash.get(..8) else {
            return;
        };
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(prefix);
        let idx = (u64::from_le_bytes(bytes) as usize) % (self.bits.len() * 8);
        let byte = idx / 8;
        let bit = idx % 8;
        self.bits[byte] |= 1 << bit;
    }

    fn get_bit(&self, hash: &[u8]) -> bool {
        let Some(prefix) = hash.get(..8) else {
            return false;
        };
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(prefix);
        let idx = (u64::from_le_bytes(bytes) as usize) % (self.bits.len() * 8);
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

fn default_bloom_salt() -> [u8; 32] {
    [0u8; 32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_roundtrip() -> Result<()> {
        let mut rng = OsRng;
        let kp = ed25519_dalek::Keypair::generate(&mut rng);
        let nk = NodeKeys::from_keypair(
            ed25519_dalek::Keypair::from_bytes(&kp.to_bytes()).unwrap(),
            std::path::PathBuf::new(),
        );
        let msg = NetworkMessage::Inventory(DagInventory {
            programs: vec![[1u8; 32]],
            program_bloom: None,
            blobs: Vec::new(),
            executions: Vec::new(),
        });

        let signed = sign_network_message(&nk, &msg)?;
        let verified = verify_signed_message(&signed)?;
        assert!(matches!(verified, NetworkMessage::Inventory(_)));
        Ok(())
    }

    #[test]
    fn detect_tampered_signature() {
        let mut rng = OsRng;
        let kp = ed25519_dalek::Keypair::generate(&mut rng);
        let nk = NodeKeys::from_keypair(
            ed25519_dalek::Keypair::from_bytes(&kp.to_bytes()).unwrap(),
            std::path::PathBuf::new(),
        );
        let msg = NetworkMessage::Inventory(DagInventory {
            programs: vec![[2u8; 32]],
            program_bloom: None,
            blobs: Vec::new(),
            executions: Vec::new(),
        });
        let mut signed = sign_network_message(&nk, &msg).unwrap();
        signed.payload[0] ^= 0xFF;
        assert!(verify_signed_message(&signed).is_err());
    }
}
