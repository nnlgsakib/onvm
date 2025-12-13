use crate::crypto::hashing::hash_bytes;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ProgramId(pub [u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlobId(pub [u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlockId(pub [u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeId(pub [u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateWrite {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComputeOp {
    pub program_id: ProgramId,
    pub input: BlobId,
    pub output: BlobMetadata,
    pub fuel_used: u64,
    pub state_root: [u8; 32],
    pub state_writes: Vec<StateWrite>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramMetadata {
    pub id: ProgramId,
    pub publisher: NodeId,
    pub size: u64,
    pub entrypoint: String,
    pub blob_refs: Vec<BlobId>,
    pub deploy_salt: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlobMetadata {
    pub id: BlobId,
    pub publisher: NodeId,
    pub size: u64,
    pub mime: Option<String>,
    pub chunk_sizes: Vec<u32>,       // variable chunk sizes (FastCDC)
    pub chunk_hashes: Vec<[u8; 32]>, // hash per chunk
    pub merkle_root: [u8; 32],       // merkle over chunk hashes
    pub data_shards: u8,             // data shards per chunk
    pub parity_shards: u8,           // parity shards per chunk
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
}

impl BlobId {
    pub fn new(data: &[u8]) -> Self {
        Self(hash_bytes(data))
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

display_hex!(ProgramId);
display_hex!(BlobId);
display_hex!(BlockId);
display_hex!(NodeId);
