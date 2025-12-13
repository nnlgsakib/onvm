use crate::types::{NodeId, ProgramId, ProgramMetadata};
use anyhow::Result;
use sled::Db;
use std::path::Path;

pub struct ProgramStore {
    db: Db,
}

impl ProgramStore {
    pub fn new(db: Db, root: impl AsRef<Path>) -> Result<Self> {
        let _ = root.as_ref();
        Ok(Self { db })
    }

    pub fn deploy(
        &self,
        wasm: &[u8],
        entrypoint: String,
        publisher: NodeId,
        blob_refs: Vec<crate::types::BlobId>,
        deploy_salt: Vec<u8>,
    ) -> Result<ProgramMetadata> {
        let id = ProgramId::new_with_salt(wasm, &deploy_salt);
        let meta = ProgramMetadata {
            id: id.clone(),
            publisher,
            size: wasm.len() as u64,
            entrypoint,
            blob_refs,
            deploy_salt,
        };
        let tree = self.db.open_tree("programs")?;
        let bytes = self.db.open_tree("program_bytes")?;
        let encoded = bincode::serde::encode_to_vec(&meta, bincode::config::standard())?;
        tree.insert(id.0, encoded)?;
        bytes.insert(id.0, wasm)?;
        tree.flush()?;
        bytes.flush()?;
        Ok(meta)
    }

    pub fn replicate(&self, meta: &ProgramMetadata, wasm: &[u8]) -> Result<()> {
        let expect = ProgramId::new_with_salt(wasm, &meta.deploy_salt);
        if expect != meta.id {
            return Err(anyhow::anyhow!("program id mismatch on replicate"));
        }
        let tree = self.db.open_tree("programs")?;
        let bytes = self.db.open_tree("program_bytes")?;
        let encoded = bincode::serde::encode_to_vec(meta, bincode::config::standard())?;
        tree.insert(meta.id.0, encoded)?;
        bytes.insert(meta.id.0, wasm)?;
        tree.flush()?;
        bytes.flush()?;
        Ok(())
    }

    pub fn store_metadata(&self, meta: &ProgramMetadata) -> Result<()> {
        let tree = self.db.open_tree("programs")?;
        let encoded = bincode::serde::encode_to_vec(meta, bincode::config::standard())?;
        tree.insert(meta.id.0, encoded)?;
        tree.flush()?;
        Ok(())
    }

    pub fn load(&self, id: &ProgramId) -> Result<Vec<u8>> {
        let bytes = self.db.open_tree("program_bytes")?;
        let Some(raw) = bytes.get(id.0)? else {
            return Err(anyhow::anyhow!("program bytes missing"));
        };
        Ok(raw.to_vec())
    }

    pub fn metadata(&self, id: &ProgramId) -> Result<Option<ProgramMetadata>> {
        let tree = self.db.open_tree("programs")?;
        let Some(raw) = tree.get(id.0)? else {
            return Ok(None);
        };
        let (meta, _): (ProgramMetadata, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(meta))
    }

    pub fn list(&self) -> Result<Vec<ProgramMetadata>> {
        let tree = self.db.open_tree("programs")?;
        let mut out = Vec::new();
        for entry in tree.iter() {
            let (_, v) = entry?;
            let (meta, _): (ProgramMetadata, _) =
                bincode::serde::decode_from_slice(&v, bincode::config::standard())?;
            out.push(meta);
        }
        Ok(out)
    }
}
