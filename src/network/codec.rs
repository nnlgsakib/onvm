use crate::crypto::keys::NodeKeys;
use crate::types::{BlobId, BlobMetadata, ComputeOp, ProgramId, ProgramMetadata};
use anyhow::{anyhow, Context, Result};
use blake3;
use ed25519_dalek::{PublicKey as EdPublicKey, Signature as EdSignature};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

pub const TOPIC_BLOBS: &str = "onvm-blobs";
pub const TOPIC_PROGRAMS: &str = "onvm-programs";
pub const TOPIC_BLOCKS: &str = "onvm-dag";

pub const TRANSFER_PROTOCOL: &str = "/onvm/transfer/3.0.0";

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
            bits: vec![0u8; size_bytes],
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
