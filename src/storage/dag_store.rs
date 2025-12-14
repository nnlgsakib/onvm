use crate::consensus::{DagId, DagNode, Operation};
use anyhow::Result;
use sled::Db;

pub struct DagStore {
    tree: sled::Tree,
}

impl DagStore {
    pub fn new(db: &Db) -> Result<Self> {
        Ok(Self {
            tree: db.open_tree("dag_nodes")?,
        })
    }

    pub fn insert(&self, node: &DagNode) -> Result<bool> {
        let enc = bincode::serde::encode_to_vec(node, bincode::config::standard())?;
        let inserted = self.tree.insert(node.id.0, enc)?.is_none();
        self.tree.flush()?;
        Ok(inserted)
    }

    pub fn contains(&self, id: &DagId) -> Result<bool> {
        Ok(self.tree.contains_key(id.0)?)
    }

    pub fn get(&self, id: &DagId) -> Result<Option<DagNode>> {
        let Some(raw) = self.tree.get(id.0)? else {
            return Ok(None);
        };
        let (node, _): (DagNode, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(node))
    }

    pub fn execution_ids(&self) -> Result<Vec<[u8; 32]>> {
        let mut out = Vec::new();
        for entry in self.tree.iter() {
            let (_, v) = entry?;
            let (node, _): (DagNode, _) =
                bincode::serde::decode_from_slice(&v, bincode::config::standard())?;
            if matches!(node.op, Operation::Compute(_)) {
                out.push(node.id.0);
            }
        }
        Ok(out)
    }

    pub fn all_compute_ops(&self) -> Result<Vec<DagNode>> {
        let mut out = Vec::new();
        for entry in self.tree.iter() {
            let (_, v) = entry?;
            let (node, _): (DagNode, _) =
                bincode::serde::decode_from_slice(&v, bincode::config::standard())?;
            if matches!(node.op, Operation::Compute(_)) {
                out.push(node);
            }
        }
        Ok(out)
    }
}
