use crate::storage::UnifiedStore;
use crate::types::{ObjectId, ObjectType, ProgramId};
use anyhow::{anyhow, Result};
use std::sync::Arc;

pub struct ExecutionAdapter {
    unified_store: Arc<UnifiedStore>,
}

impl ExecutionAdapter {
    pub fn new(unified_store: Arc<UnifiedStore>) -> Self {
        Self { unified_store }
    }

    pub fn load_wasm_by_object_id(&self, object_id: &ObjectId) -> Result<Vec<u8>> {
        let object = self
            .unified_store
            .get_object_metadata(object_id)?
            .ok_or_else(|| anyhow!("object not found"))?;

        match &object.object_type {
            ObjectType::WasmProgram { .. } => {}
            ObjectType::Blob { .. } => {
                return Err(anyhow!("object is not a WASM program"));
            }
        }

        if !self.unified_store.is_complete(object_id)? {
            return Err(anyhow!("WASM program not fully available"));
        }

        let wasm_bytes = self.unified_store.get_object(object_id)?;

        self.validate_wasm(&wasm_bytes)?;

        Ok(wasm_bytes)
    }

    pub fn load_wasm_by_program_id(&self, program_id: &ProgramId) -> Result<Vec<u8>> {
        let object_id = program_id.to_object_id();
        self.load_wasm_by_object_id(&object_id)
    }

    pub fn get_entrypoint(&self, object_id: &ObjectId) -> Result<String> {
        let object = self
            .unified_store
            .get_object_metadata(object_id)?
            .ok_or_else(|| anyhow!("object not found"))?;

        match &object.object_type {
            ObjectType::WasmProgram { entrypoint, .. } => Ok(entrypoint.clone()),
            ObjectType::Blob { .. } => Err(anyhow!("object is not a WASM program")),
        }
    }

    pub fn get_blob_refs(&self, object_id: &ObjectId) -> Result<Vec<ObjectId>> {
        let object = self
            .unified_store
            .get_object_metadata(object_id)?
            .ok_or_else(|| anyhow!("object not found"))?;

        match &object.object_type {
            ObjectType::WasmProgram { blob_refs, .. } => Ok(blob_refs.clone()),
            ObjectType::Blob { .. } => Ok(Vec::new()),
        }
    }

    pub fn is_wasm_available(&self, object_id: &ObjectId) -> Result<bool> {
        let object = self.unified_store.get_object_metadata(object_id)?;
        if object.is_none() {
            return Ok(false);
        }

        self.unified_store.is_complete(object_id)
    }

    fn validate_wasm(&self, wasm_bytes: &[u8]) -> Result<()> {
        if wasm_bytes.len() < 8 {
            return Err(anyhow!("WASM binary too short"));
        }

        if &wasm_bytes[0..4] != b"\0asm" {
            return Err(anyhow!("invalid WASM magic number"));
        }

        Ok(())
    }

    pub fn object_id_from_program_id(program_id: &ProgramId) -> ObjectId {
        ObjectId(program_id.0)
    }

    pub fn program_id_from_object_id(object_id: &ObjectId) -> ProgramId {
        ProgramId(object_id.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NodeId;
    use sled::Config;

    #[test]
    fn test_wasm_validation() -> Result<()> {
        let db = Config::new().temporary(true).open()?;
        let store = Arc::new(UnifiedStore::new(db)?);
        let adapter = ExecutionAdapter::new(store.clone());

        let valid_wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

        adapter.validate_wasm(&valid_wasm)?;

        let invalid_wasm = vec![0x00, 0x00, 0x00, 0x00];
        assert!(adapter.validate_wasm(&invalid_wasm).is_err());

        Ok(())
    }

    #[test]
    fn test_wasm_storage_and_retrieval() -> Result<()> {
        let db = Config::new().temporary(true).open()?;
        let store = Arc::new(UnifiedStore::new(db)?);
        let adapter = ExecutionAdapter::new(store.clone());

        let wasm_bytes = vec![
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x07, 0x01, 0x60, 0x02, 0x7f,
            0x7f, 0x01, 0x7f,
        ];

        let publisher = NodeId::new(&[1u8; 32]);

        let object = store.put_object(
            &wasm_bytes,
            ObjectType::WasmProgram {
                entrypoint: "main".to_string(),
                wasm_version: Some("1.0".to_string()),
                source_language: Some("rust".to_string()),
                compiler: Some("rustc".to_string()),
                blob_refs: Vec::new(),
                deploy_salt: vec![0u8; 32],
            },
            publisher,
        )?;

        assert!(adapter.is_wasm_available(&object.id)?);

        let loaded = adapter.load_wasm_by_object_id(&object.id)?;
        assert_eq!(wasm_bytes, loaded);

        let entrypoint = adapter.get_entrypoint(&object.id)?;
        assert_eq!("main", entrypoint);

        Ok(())
    }
}
