//! Consensus core types (legacy DAG node model).

use crate::types::{ComputeOp, NodeId, Object, ObjectId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Operation {
    PublishObject(Object),
    Compute(ComputeOp),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DagRef {
    Object(ObjectId),
    Execution(DagId),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DagId(pub [u8; 32]);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DagNode {
    pub id: DagId,
    pub parents: Vec<DagRef>,
    pub op: Operation,
    pub timestamp_ms: u64,
    pub publisher: NodeId,
}

#[derive(Clone, Debug)]
pub enum BlobSyncMode {
    FullData,
    MetadataOnly,
}

#[derive(Clone, Debug)]
pub struct DagConfig {
    pub min_peers: usize,
    pub blob_sync_mode: BlobSyncMode,
}
