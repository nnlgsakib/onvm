use crate::crypto::hashing::hash_bytes;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// Object identifier: BLAKE3 hash of complete assembled content
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ObjectId(pub [u8; 32]);

/// Chunk identifier: BLAKE3 hash of chunk content
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ChunkId(pub [u8; 32]);

/// Manifest identifier: BLAKE3 hash of canonical manifest encoding
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ManifestId(pub [u8; 32]);

/// Legacy types kept for backward compatibility during migration
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ProgramId(pub [u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlobId(pub [u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlockId(pub [u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeId(pub [u8; 32]);

/// Unified object representing both WASM programs and blobs
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Object {
    pub id: ObjectId,
    pub object_type: ObjectType,
    pub total_size: u64,
    pub chunk_count: u32,
    pub manifest_id: ManifestId,
    pub publisher: NodeId,
    pub created_at: u64,
}

/// Object type discriminator
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ObjectType {
    Blob {
        mime_type: Option<String>,
        #[serde(default = "default_blob_salt")]
        upload_salt: [u8; 32],
    },
    WasmProgram {
        entrypoint: String,
        wasm_version: Option<String>,
        source_language: Option<String>,
        compiler: Option<String>,
        blob_refs: Vec<ObjectId>,
        deploy_salt: Vec<u8>,
    },
}

impl ObjectType {
    pub fn blob_with_random_salt(mime_type: Option<String>) -> Self {
        Self::Blob {
            mime_type,
            upload_salt: generate_blob_salt(),
        }
    }
}

fn generate_blob_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    salt
}

fn default_blob_salt() -> [u8; 32] {
    [0u8; 32]
}

/// Manifest defining ordered chunk list
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub object_id: ObjectId,
    pub content_hash: [u8; 32],
    pub chunks: Vec<ChunkDescriptor>,
    pub version: u32,
}

/// Chunk descriptor in manifest
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkDescriptor {
    pub chunk_id: ChunkId,
    pub size: u32,
    pub offset: u64,
}

/// Chunk: atomic unit of transfer
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Chunk {
    pub id: ChunkId,
    pub data: Vec<u8>,
}

/// Legacy state write type
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateWrite {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Legacy compute operation type
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComputeOp {
    pub program_id: ProgramId,
    pub input: BlobId,
    pub output: BlobMetadata,
    pub fuel_used: u64,
    pub state_root: [u8; 32],
    pub state_writes: Vec<StateWrite>,
}

/// Legacy program metadata (will be migrated to Object)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramMetadata {
    pub id: ProgramId,
    pub publisher: NodeId,
    pub size: u64,
    pub entrypoint: String,
    pub blob_refs: Vec<BlobId>,
    pub deploy_salt: Vec<u8>,
}

/// Legacy blob metadata (will be migrated to Object + Manifest)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlobMetadata {
    pub id: BlobId,
    pub publisher: NodeId,
    pub size: u64,
    pub mime: Option<String>,
    pub chunk_sizes: Vec<u32>,
    pub chunk_hashes: Vec<[u8; 32]>,
    pub merkle_root: [u8; 32],
    pub data_shards: u8,
    pub parity_shards: u8,
}

impl ObjectId {
    pub fn new(data: &[u8]) -> Self {
        Self(hash_bytes(data))
    }

    pub fn from_program_id(pid: &ProgramId) -> Self {
        Self(pid.0)
    }

    pub fn from_blob_id(bid: &BlobId) -> Self {
        Self(bid.0)
    }
}

impl ChunkId {
    pub fn new(data: &[u8]) -> Self {
        Self(hash_bytes(data))
    }
}

impl ManifestId {
    pub fn new(data: &[u8]) -> Self {
        Self(hash_bytes(data))
    }
}

impl Manifest {
    pub fn id(&self) -> ManifestId {
        let encoded = bincode::serde::encode_to_vec(self, bincode::config::standard())
            .expect("manifest encoding should never fail");
        ManifestId::new(&encoded)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let mut expected_offset = 0u64;
        for (idx, chunk_desc) in self.chunks.iter().enumerate() {
            if chunk_desc.offset != expected_offset {
                return Err(anyhow::anyhow!(
                    "chunk {} offset mismatch: expected {}, got {}",
                    idx,
                    expected_offset,
                    chunk_desc.offset
                ));
            }
            expected_offset += chunk_desc.size as u64;
        }
        Ok(())
    }

    pub fn total_size(&self) -> u64 {
        self.chunks.iter().map(|c| c.size as u64).sum()
    }
}

impl Chunk {
    pub fn new(data: Vec<u8>) -> Self {
        let id = ChunkId::new(&data);
        Self { id, data }
    }

    pub fn verify(&self) -> anyhow::Result<()> {
        let computed = ChunkId::new(&self.data);
        if computed != self.id {
            return Err(anyhow::anyhow!("chunk hash mismatch"));
        }
        Ok(())
    }
}

impl ProgramId {
    pub fn new(data: &[u8]) -> Self {
        Self(hash_bytes(data))
    }

    pub fn new_with_salt(data: &[u8], salt: &[u8]) -> Self {
        let mut combined = Vec::with_capacity(data.len() + salt.len());
        combined.extend_from_slice(data);
        combined.extend_from_slice(salt);
        Self(hash_bytes(&combined))
    }

    pub fn to_object_id(&self) -> ObjectId {
        ObjectId(self.0)
    }
}

impl BlobId {
    pub fn new(data: &[u8]) -> Self {
        Self(hash_bytes(data))
    }

    pub fn to_object_id(&self) -> ObjectId {
        ObjectId(self.0)
    }
}

impl BlockId {
    pub fn new(data: &[u8]) -> Self {
        Self(hash_bytes(data))
    }
}

impl NodeId {
    pub fn new(data: &[u8]) -> Self {
        let mut out = [0u8; 32];
        let copy_len = std::cmp::min(data.len(), 32);
        out[..copy_len].copy_from_slice(&data[..copy_len]);
        Self(out)
    }

    pub fn from_public_key(pk: &[u8]) -> Self {
        let mut out = [0u8; 32];
        out.copy_from_slice(&pk[..32]);
        Self(out)
    }
}

macro_rules! display_hex {
    ($ty:ty) => {
        impl Display for $ty {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", hex::encode(self.0))
            }
        }
    };
}

display_hex!(ObjectId);
display_hex!(ChunkId);
display_hex!(ManifestId);
display_hex!(ProgramId);
display_hex!(BlobId);
display_hex!(BlockId);
display_hex!(NodeId);
