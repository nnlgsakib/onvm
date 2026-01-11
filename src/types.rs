use crate::crypto::bls::{BlsPublicKey, BlsSignature};
use crate::crypto::hashing::hash_bytes;
use crate::crypto::threshold_ecdsa::{DkgTranscript, ThresholdPublicKey, ThresholdSignature};
use anyhow::{anyhow, Result};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// Subnet identifier: identifies a collection of nodes forming a execution shard
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SubnetId(pub [u8; 32]);

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

pub type StateRoot = [u8; 32];
pub type StateDeltaRoot = [u8; 32];
pub type WasmEnvHash = [u8; 32];
pub type ReceiptId = [u8; 32];

pub const BLOB_ID_PREFIX: &str = "blob";
pub const PROGRAM_ID_PREFIX: &str = "prog";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdPrefix {
    Blob,
    Program,
}

/// Strongly-typed manifest for programs with network-wide availability data
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramManifest {
    pub program_id: ProgramId,
    pub subnet_id: SubnetId,
    pub version: u64,
    pub deployer: NodeId,
    pub wasm_env_hash: WasmEnvHash,
    pub code_manifest: Option<ManifestId>,
    pub metadata_hash: [u8; 32],
    pub entrypoints: Vec<String>,
    pub initial_state_root: StateRoot,
    pub dag_parent: Option<StateRoot>,
    pub timestamp_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committee: Option<CommitteeCertificate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subnet_membership: Option<SubnetId>,
    pub signature: Vec<u8>,
}

/// Announcement broadcast so peers can sync manifests and initial state
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramAnnouncement {
    pub manifest: ProgramManifest,
    pub chunk_roots: Vec<ChunkId>,
    pub initial_state_chunks: Vec<ChunkId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateCommitment {
    pub program_id: ProgramId,
    pub height: u64,
    pub root: StateRoot,
    pub parent: Option<StateRoot>,
    pub state_delta_root: Option<StateDeltaRoot>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionCall {
    pub entrypoint: String,
    pub calldata: Vec<u8>,
    pub request_id: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionReceipt {
    pub program_id: ProgramId,
    #[serde(default)]
    pub height: u64,
    pub state_root_in: StateRoot,
    pub call: ExecutionCall,
    pub inputs_hash: [u8; 32],
    pub gas_used: u64,
    pub state_root_out: StateRoot,
    pub write_digest: StateDeltaRoot,
    pub events_hash: [u8; 32],
    pub wasm_code_hash: Option<[u8; 32]>,
    pub wasm_env_hash: WasmEnvHash,
    pub nonce: u64,
    pub executor: NodeId,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AggregatedReceipt {
    pub receipt: ExecutionReceipt,
    pub committee_epoch: u64,
    pub signer_bitmap: Vec<u8>,
    pub aggregate_signature: BlsSignature,
    pub aggregate_public_key: BlsPublicKey,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitteeMember {
    pub node: NodeId,
    pub weight: u64,
    pub bls_public_key: BlsPublicKey,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitteeCertificate {
    pub program_id: ProgramId,
    pub epoch: u64,
    pub members: Vec<CommitteeMember>,
    pub threshold: u32,
    pub aggregate_public_key: BlsPublicKey,
    pub signature: Option<BlsSignature>,
}

impl SubnetId {
    pub fn new(data: &[u8]) -> Self {
        Self(hash_bytes(data))
    }

    pub fn to_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Display for SubnetId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "subnet_{}", hex::encode(&self.0[..8]))
    }
}

/// Subnet membership certificate - local BFT for subnet consensus
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubnetCertificate {
    pub subnet_id: SubnetId,
    pub epoch: u64,
    pub members: Vec<SubnetMember>,
    pub threshold: u32,
    pub aggregate_public_key: BlsPublicKey,
    pub signature: Option<BlsSignature>,
}

/// Member of a subnet with BLS key for local consensus
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubnetMember {
    pub node: NodeId,
    pub weight: u64,
    pub bls_public_key: BlsPublicKey,
}

/// Subnet using threshold ECDSA (Chain Key) for signatures
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThresholdSubnetCertificate {
    pub subnet_id: SubnetId,
    pub epoch: u64,
    pub threshold: u32,
    pub num_members: u32,
    pub dkg_transcript: DkgTranscript,
    pub threshold_public_key: ThresholdPublicKey,
    pub members: Vec<ThresholdSubnetMember>,
    pub signature: Option<BlsSignature>,
}

/// Member of a threshold subnet with index for threshold signing
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThresholdSubnetMember {
    pub node: NodeId,
    pub index: u32,
    pub weight: u64,
}

/// Signed commitment for threshold key resharing
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThresholdKeyCommitment {
    pub subnet_id: SubnetId,
    pub epoch: u64,
    pub from_index: u32,
    pub to_index: u32,
    pub commitment: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Response containing a key share during DKG
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThresholdKeyShareResponse {
    pub subnet_id: SubnetId,
    pub epoch: u64,
    pub receiver_index: u32,
    pub share_index: u32,
    pub encrypted_share: Vec<u8>,
    pub commitment: Vec<u8>,
    pub signature: Vec<u8>,
}

impl ThresholdSubnetCertificate {
    pub fn get_threshold_public_key(&self) -> &ThresholdPublicKey {
        &self.threshold_public_key
    }

    pub fn verify_threshold_signature(
        &self,
        signature: &ThresholdSignature,
        message_hash: &[u8; 32],
    ) -> Result<()> {
        if signature.threshold != self.threshold {
            return Err(anyhow!("signature threshold mismatch"));
        }
        if signature.signers.len() < self.threshold as usize {
            return Err(anyhow!("not enough signers"));
        }
        crate::crypto::threshold_ecdsa::ThresholdSigner::verify_threshold_signature(
            signature,
            &self.threshold_public_key,
            message_hash,
        )
    }
}

impl Display for ThresholdSubnetCertificate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ThresholdSubnet {} epoch {} (t={}/n={}, key={})",
            self.subnet_id,
            self.epoch,
            self.threshold,
            self.num_members,
            hex::encode(&self.threshold_public_key.key[..8])
        )
    }
}

/// Cross-program async message for inter-canister calls
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrossProgramMessage {
    pub id: [u8; 32],
    pub source_program: ProgramId,
    pub source_subnet: SubnetId,
    pub target_program: ProgramId,
    pub target_subnet: SubnetId,
    pub method: String,
    pub payload: Vec<u8>,
    pub nonce: u64,
    pub timestamp_ms: u64,
    pub expires_at: Option<u64>,
    pub response_for: Option<[u8; 32]>,
}

/// Queue entry for pending async messages
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageQueueEntry {
    pub message: CrossProgramMessage,
    pub retry_count: u32,
    pub next_retry_at: u64,
    pub status: MessageStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageStatus {
    Pending,
    InFlight,
    Delivered,
    Failed,
    Expired,
    ResponseReceived,
}

/// Response to a cross-program message
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageResponse {
    pub message_id: [u8; 32],
    pub result: MessageResult,
    pub payload: Vec<u8>,
    pub error_code: Option<u32>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageResult {
    Success,
    MethodNotFound,
    InvalidPayload,
    Trap,
    Timeout,
    DestinationUnavailable,
    Forbidden,
}

/// Program now includes subnet_id for ICP-style isolation
impl ProgramManifest {
    pub fn get_state_namespace(&self) -> Vec<u8> {
        let mut ns = Vec::with_capacity(64);
        ns.extend_from_slice(&self.program_id.0);
        ns
    }
}

impl Display for SubnetCertificate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Subnet {} epoch {} ({} members, threshold {})",
            self.subnet_id,
            self.epoch,
            self.members.len(),
            self.threshold
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateProof {
    pub state_root: StateRoot,
    pub keys: Vec<Vec<u8>>,
    pub values: Vec<Vec<u8>>,
    pub proof_hashes: Vec<[u8; 32]>,
}

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
    pub value: Option<Vec<u8>>,
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

    pub fn to_prefixed_string(&self) -> String {
        format!("{}{}", PROGRAM_ID_PREFIX, hex::encode(self.0))
    }
}

impl BlobId {
    pub fn new(data: &[u8]) -> Self {
        Self(hash_bytes(data))
    }

    pub fn to_object_id(&self) -> ObjectId {
        ObjectId(self.0)
    }

    pub fn to_prefixed_string(&self) -> String {
        format!("{}{}", BLOB_ID_PREFIX, hex::encode(self.0))
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

impl ProgramManifest {
    pub fn digest(&self) -> [u8; 32] {
        let mut tmp = self.clone();
        tmp.signature.clear();
        let encoded = bincode::serde::encode_to_vec(&tmp, bincode::config::standard())
            .expect("program manifest encoding should never fail");
        hash_bytes(&encoded)
    }
}

impl StateCommitment {
    pub fn digest(&self) -> [u8; 32] {
        let encoded = bincode::serde::encode_to_vec(self, bincode::config::standard())
            .expect("state commitment encoding should never fail");
        hash_bytes(&encoded)
    }
}

impl ExecutionReceipt {
    pub fn id(&self) -> ReceiptId {
        let encoded = bincode::serde::encode_to_vec(self, bincode::config::standard())
            .expect("execution receipt encoding should never fail");
        hash_bytes(&encoded)
    }
}

impl AggregatedReceipt {
    pub fn digest(&self) -> [u8; 32] {
        let encoded = bincode::serde::encode_to_vec(self, bincode::config::standard())
            .expect("aggregated receipt encoding should never fail");
        hash_bytes(&encoded)
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
display_hex!(BlockId);
display_hex!(NodeId);

impl Display for ProgramId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", PROGRAM_ID_PREFIX, hex::encode(self.0))
    }
}

impl Display for BlobId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", BLOB_ID_PREFIX, hex::encode(self.0))
    }
}

pub fn parse_blob_id_str(input: &str) -> Result<BlobId, String> {
    let (prefix, bytes) = parse_prefixed_hex_id(input)?;
    if matches!(prefix, Some(IdPrefix::Program)) {
        return Err("expected blob id, got program id prefix".into());
    }
    Ok(BlobId(bytes))
}

pub fn parse_program_id_str(input: &str) -> Result<ProgramId, String> {
    let (prefix, bytes) = parse_prefixed_hex_id(input)?;
    if matches!(prefix, Some(IdPrefix::Blob)) {
        return Err("expected program id, got blob id prefix".into());
    }
    Ok(ProgramId(bytes))
}

pub fn parse_object_id_str(input: &str) -> Result<ObjectId, String> {
    let (_, bytes) = parse_prefixed_hex_id(input)?;
    Ok(ObjectId(bytes))
}

fn parse_prefixed_hex_id(input: &str) -> Result<(Option<IdPrefix>, [u8; 32]), String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("id cannot be empty".into());
    }

    let (prefix, rest) = if let Some(candidate) = trimmed.get(0..4) {
        if candidate.eq_ignore_ascii_case(BLOB_ID_PREFIX) {
            (Some(IdPrefix::Blob), trimmed.get(4..).unwrap_or(""))
        } else if candidate.eq_ignore_ascii_case(PROGRAM_ID_PREFIX) {
            (Some(IdPrefix::Program), trimmed.get(4..).unwrap_or(""))
        } else {
            (None, trimmed)
        }
    } else {
        (None, trimmed)
    };

    let hex_part = rest.trim();
    let bytes = hex::decode(hex_part).map_err(|e| format!("invalid hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!(
            "invalid id length: expected 32 bytes, got {}",
            bytes.len()
        ));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok((prefix, arr))
}
