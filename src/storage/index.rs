use crate::types::{BlobId, BlobMetadata, NodeId, ProgramId};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::collections::HashSet;

pub struct BlobIndex {
    tree: sled::Tree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobRecord {
    pub meta: BlobMetadata,
    pub locations: HashSet<NodeId>,
    pub has_data: bool,
}

impl BlobIndex {
    pub fn new(db: &Db) -> Result<Self> {
        Ok(Self {
            tree: db.open_tree("blob_index")?,
        })
    }

    pub fn record(&self, meta: &BlobMetadata, location: NodeId, has_data: bool) -> Result<()> {
        let mut rec = self.get(meta.id.clone())?.unwrap_or_else(|| BlobRecord {
            meta: meta.clone(),
            locations: HashSet::new(),
            has_data: false,
        });
        rec.meta = meta.clone();
        rec.locations.insert(location);
        rec.has_data = rec.has_data || has_data;
        let enc = bincode::serde::encode_to_vec(&rec, bincode::config::standard())?;
        self.tree.insert(meta.id.0, enc)?;
        self.tree.flush()?;
        Ok(())
    }

    pub fn get(&self, id: BlobId) -> Result<Option<BlobRecord>> {
        let Some(raw) = self.tree.get(id.0)? else {
            return Ok(None);
        };
        let (rec, _): (BlobRecord, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(rec))
    }

    pub fn inventory_records(&self) -> Result<Vec<BlobRecord>> {
        let mut out = Vec::new();
        for entry in self.tree.iter() {
            let (_, v) = entry?;
            let (rec, _): (BlobRecord, _) =
                bincode::serde::decode_from_slice(&v, bincode::config::standard())?;
            out.push(rec);
        }
        Ok(out)
    }
}

pub struct ProgramIndex {
    tree: sled::Tree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramRecord {
    pub id: ProgramId,
    pub locations: HashSet<NodeId>,
}

impl ProgramIndex {
    pub fn new(db: &Db) -> Result<Self> {
        Ok(Self {
            tree: db.open_tree("program_index")?,
        })
    }

    pub fn record(&self, id: ProgramId, location: NodeId) -> Result<()> {
        let mut rec = self.get(&id)?.unwrap_or_else(|| ProgramRecord {
            id: id.clone(),
            locations: HashSet::new(),
        });
        rec.locations.insert(location);
        let enc = bincode::serde::encode_to_vec(&rec, bincode::config::standard())?;
        self.tree.insert(id.0, enc)?;
        self.tree.flush()?;
        Ok(())
    }

    pub fn get(&self, id: &ProgramId) -> Result<Option<ProgramRecord>> {
        let Some(raw) = self.tree.get(id.0)? else {
            return Ok(None);
        };
        let (rec, _): (ProgramRecord, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(rec))
    }
}
