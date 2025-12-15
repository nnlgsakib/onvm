use crate::types::{NodeId, ProgramId, ProgramMetadata};
use anyhow::Result;
use sled::Db;
use std::path::Path;

pub struct ProgramStore {
    db: Db,
}

// Constants for program chunking
const PROGRAM_CHUNK_SIZE: usize = 1024 * 1024; // 1 MiB chunks
const MAX_UNCHUNKED_PROGRAM_SIZE: usize = 10 * 1024 * 1024; // 10 MiB threshold for chunking

// Metadata for chunked programs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkedProgramMetadata {
    pub program_id: ProgramId,
    pub chunk_count: usize,
    pub chunk_sizes: Vec<usize>,
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
        let encoded = bincode::serde::encode_to_vec(&meta, bincode::config::standard())?;
        tree.insert(id.0, encoded)?;
        
        // Handle chunked vs unchunked storage
        if wasm.len() > MAX_UNCHUNKED_PROGRAM_SIZE {
            self.store_chunked_program(&id, wasm)?;
        } else {
            let bytes = self.db.open_tree("program_bytes")?;
            bytes.insert(id.0, &wasm[..])?;
            bytes.flush()?;
        }
        
        tree.flush()?;
        Ok(meta)
    }
    
    fn store_chunked_program(&self, id: &ProgramId, wasm: &[u8]) -> Result<()> {
        let chunks_tree = self.db.open_tree("program_chunks")?;
        let meta_tree = self.db.open_tree("program_chunk_meta")?;
        
        // Split WASM into chunks
        let mut offset = 0;
        let mut chunk_index = 0;
        let mut chunk_sizes = Vec::new();
        
        while offset < wasm.len() {
            let end = std::cmp::min(offset + PROGRAM_CHUNK_SIZE, wasm.len());
            let chunk = &wasm[offset..end];
            chunk_sizes.push(chunk.len());
            
            // Store chunk with key: program_id + chunk_index
            let mut key = Vec::with_capacity(36); // 32 bytes for ProgramId + 4 bytes for chunk index
            key.extend_from_slice(&id.0);
            key.extend_from_slice(&(chunk_index as u32).to_be_bytes());
            
            chunks_tree.insert(key, &chunk[..])?;
            offset = end;
            chunk_index += 1;
        }
        
        // Store chunk metadata
        let chunk_meta = ChunkedProgramMetadata {
            program_id: id.clone(),
            chunk_count: chunk_index,
            chunk_sizes,
        };
        
        let meta_key = id.0;
        let encoded = bincode::serde::encode_to_vec(&chunk_meta, bincode::config::standard())?;
        meta_tree.insert(meta_key, encoded)?;
        
        chunks_tree.flush()?;
        meta_tree.flush()?;
        Ok(())
    }

    pub fn replicate(&self, meta: &ProgramMetadata, wasm: &[u8]) -> Result<()> {
        let expect = ProgramId::new_with_salt(wasm, &meta.deploy_salt);
        if expect != meta.id {
            return Err(anyhow::anyhow!("program id mismatch on replicate"));
        }
        let tree = self.db.open_tree("programs")?;
        let encoded = bincode::serde::encode_to_vec(meta, bincode::config::standard())?;
        tree.insert(meta.id.0, encoded)?;
        
        // Handle chunked vs unchunked storage
        if wasm.len() > MAX_UNCHUNKED_PROGRAM_SIZE {
            self.store_chunked_program(&meta.id, wasm)?;
        } else {
            let bytes = self.db.open_tree("program_bytes")?;
            bytes.insert(meta.id.0, &wasm[..])?;
            bytes.flush()?;
        }
        
        tree.flush()?;
        Ok(())
    }
    
    pub fn replicate_chunked(&self, chunk_meta: &ChunkedProgramMetadata, chunks: &[(usize, Vec<u8>)]) -> Result<()> {
        let chunks_tree = self.db.open_tree("program_chunks")?;
        let _meta_tree = self.db.open_tree("program_chunk_meta")?;
        
        // Verify chunk data integrity
        for (index, chunk) in chunks {
            // Verify chunk index is within bounds
            if *index >= chunk_meta.chunk_count {
                return Err(anyhow::anyhow!("chunk index {} out of bounds for program with {} chunks", index, chunk_meta.chunk_count));
            }
            
            // Verify chunk size matches metadata
            if let Some(expected_size) = chunk_meta.chunk_sizes.get(*index) {
                if chunk.len() != *expected_size {
                    return Err(anyhow::anyhow!("chunk {} size mismatch: expected {}, got {}", index, expected_size, chunk.len()));
                }
            } else {
                return Err(anyhow::anyhow!("no size information for chunk {}", index));
            }
            
            // Reconstruct key: program_id + chunk_index
            let mut key = Vec::with_capacity(36); // 32 bytes for ProgramId + 4 bytes for chunk index
            key.extend_from_slice(&chunk_meta.program_id.0);
            key.extend_from_slice(&(*index as u32).to_be_bytes());
            
            chunks_tree.insert(key, &chunk[..])?;
        }
        
        chunks_tree.flush()?;
        Ok(())
    }

    pub fn store_chunk(&self, id: &ProgramId, chunk_idx: usize, chunk_data: &[u8]) -> Result<()> {
        let chunks_tree = self.db.open_tree("program_chunks")?;
        let meta_tree = self.db.open_tree("program_chunk_meta")?;
        
        // Retrieve metadata to verify chunk
        let meta_key = id.0;
        let meta_bytes = meta_tree.get(meta_key)?.ok_or_else(|| anyhow::anyhow!("metadata not found for program"))?;
        let (chunk_meta, _): (ChunkedProgramMetadata, usize) = bincode::serde::decode_from_slice(&meta_bytes, bincode::config::standard())?;
        
        if chunk_idx >= chunk_meta.chunk_count {
             return Err(anyhow::anyhow!("chunk index out of bounds"));
        }
        
        if let Some(expected_size) = chunk_meta.chunk_sizes.get(chunk_idx) {
            if chunk_data.len() != *expected_size {
                 return Err(anyhow::anyhow!("chunk size mismatch"));
            }
        }
        
        let mut key = Vec::with_capacity(36);
        key.extend_from_slice(&id.0);
        key.extend_from_slice(&(chunk_idx as u32).to_be_bytes());
        
        chunks_tree.insert(key, chunk_data)?;
        chunks_tree.flush()?;
        Ok(())
    }

    pub fn store_metadata(&self, meta: &ProgramMetadata) -> Result<()> {
        let tree = self.db.open_tree("programs")?;
        let encoded = bincode::serde::encode_to_vec(meta, bincode::config::standard())?;
        tree.insert(meta.id.0, encoded)?;
        tree.flush()?;
        Ok(())
    }

    #[allow(unused_variables)]
    pub fn load(&self, id: &ProgramId) -> Result<Vec<u8>> {
        // First check if this is a chunked program
        if let Some(chunk_meta) = self.get_chunked_metadata(id)? {
            // For chunked programs, use reconstruction logic
            self.reconstruct_program(id)
        } else {
            // Load as regular unchunked program
            let bytes = self.db.open_tree("program_bytes")?;
            let Some(raw) = bytes.get(id.0)? else {
                return Err(anyhow::anyhow!("program bytes missing"));
            };
            Ok(raw.to_vec())
        }
    }
    
    fn load_chunked_program(&self, chunk_meta: &ChunkedProgramMetadata) -> Result<Vec<u8>> {
        let chunks_tree = self.db.open_tree("program_chunks")?;
        let mut wasm_data = Vec::with_capacity(chunk_meta.chunk_sizes.iter().sum());
        
        for chunk_index in 0..chunk_meta.chunk_count {
            // Reconstruct key: program_id + chunk_index
            let mut key = Vec::with_capacity(36); // 32 bytes for ProgramId + 4 bytes for chunk index
            key.extend_from_slice(&chunk_meta.program_id.0);
            key.extend_from_slice(&(chunk_index as u32).to_be_bytes());
            
            let Some(chunk_data) = chunks_tree.get(key)? else {
                return Err(anyhow::anyhow!("missing chunk {} for program {:?}", chunk_index, chunk_meta.program_id));
            };
            
            wasm_data.extend_from_slice(&chunk_data);
        }
        
        Ok(wasm_data)
    }
    
    /// Reconstruct a program from its chunks, verifying integrity
    pub fn reconstruct_program(&self, id: &ProgramId) -> Result<Vec<u8>> {
        // Check if this is a chunked program
        if let Some(chunk_meta) = self.get_chunked_metadata(id)? {
            // Verify all chunks are present before reconstruction
            let chunks_tree = self.db.open_tree("program_chunks")?;
            
            for chunk_index in 0..chunk_meta.chunk_count {
                // Reconstruct key: program_id + chunk_index
                let mut key = Vec::with_capacity(36); // 32 bytes for ProgramId + 4 bytes for chunk index
                key.extend_from_slice(&id.0);
                key.extend_from_slice(&(chunk_index as u32).to_be_bytes());
                
                if !chunks_tree.contains_key(&key)? {
                    return Err(anyhow::anyhow!("missing chunk {} for program {:?}", chunk_index, id));
                }
            }
            
            // All chunks present, reconstruct the program
            self.load_chunked_program(&chunk_meta)
        } else {
            // For unchunked programs, load normally
            self.load(id)
        }
    }
    
    /// Check if all chunks are available for a chunked program
    pub fn are_all_chunks_available(&self, id: &ProgramId) -> Result<bool> {
        // Check if this is a chunked program
        if let Some(chunk_meta) = self.get_chunked_metadata(id)? {
            let chunks_tree = self.db.open_tree("program_chunks")?;
            
            for chunk_index in 0..chunk_meta.chunk_count {
                // Reconstruct key: program_id + chunk_index
                let mut key = Vec::with_capacity(36); // 32 bytes for ProgramId + 4 bytes for chunk index
                key.extend_from_slice(&id.0);
                key.extend_from_slice(&(chunk_index as u32).to_be_bytes());
                
                if !chunks_tree.contains_key(&key)? {
                    return Ok(false);
                }
            }
            
            Ok(true)
        } else {
            // For unchunked programs, they're always "available"
            Ok(true)
        }
    }
    
    pub fn get_chunked_metadata(&self, id: &ProgramId) -> Result<Option<ChunkedProgramMetadata>> {
        let meta_tree = self.db.open_tree("program_chunk_meta")?;
        let Some(raw) = meta_tree.get(id.0)? else {
            return Ok(None);
        };
        let (meta, _): (ChunkedProgramMetadata, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(meta))
    }
    
    pub fn get_program_chunk(&self, id: &ProgramId, chunk_index: usize) -> Result<Option<Vec<u8>>> {
        // Check if this is a chunked program
        if let Some(chunk_meta) = self.get_chunked_metadata(id)? {
            if chunk_index >= chunk_meta.chunk_count {
                return Err(anyhow::anyhow!("chunk index out of bounds"));
            }
            
            let chunks_tree = self.db.open_tree("program_chunks")?;
            
            // Reconstruct key: program_id + chunk_index
            let mut key = Vec::with_capacity(36); // 32 bytes for ProgramId + 4 bytes for chunk index
            key.extend_from_slice(&id.0);
            key.extend_from_slice(&(chunk_index as u32).to_be_bytes());
            
            let chunk_data = chunks_tree.get(key)?;
            Ok(chunk_data.map(|data| data.to_vec()))
        } else {
            // For unchunked programs, return the whole program as chunk 0
            if chunk_index == 0 {
                match self.load(id) {
                    Ok(data) => Ok(Some(data)),
                    Err(_) => Ok(None),
                }
            } else {
                Err(anyhow::anyhow!("program is not chunked"))
            }
        }
    }
    
    pub fn get_chunk_count(&self, id: &ProgramId) -> Result<usize> {
        // Check if this is a chunked program
        if let Some(chunk_meta) = self.get_chunked_metadata(id)? {
            Ok(chunk_meta.chunk_count)
        } else {
            // For unchunked programs, there's only one chunk
            Ok(1)
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use sled::Config;
    use std::path::Path;

    #[test]
    fn test_chunked_program_storage() -> Result<()> {
        // Create a temporary database for testing
        let config = Config::new().temporary(true);
        let db = config.open()?;
        
        // Create a program store
        let program_store = ProgramStore::new(db, Path::new("."))?;
        
        // Generate a large fake WASM program (15MB) to trigger chunking
        let large_wasm = vec![0u8; 15 * 1024 * 1024]; // 15MB of zeros
        
        // Deploy the program
        let publisher = NodeId::new(&[1u8; 32]);
        let program_id = ProgramId::new(&large_wasm);
        
        let meta = program_store.deploy(
            &large_wasm,
            "main".to_string(),
            publisher.clone(),
            vec![],
            vec![],
        )?;
        
        assert_eq!(meta.size, large_wasm.len() as u64);
        
        // Verify the program can be loaded
        let loaded_wasm = program_store.load(&program_id)?;
        assert_eq!(large_wasm.len(), loaded_wasm.len());
        
        // Check if it's chunked
        if let Some(chunk_meta) = program_store.get_chunked_metadata(&program_id)? {
            // Verify all chunks are available
            let all_available = program_store.are_all_chunks_available(&program_id)?;
            assert!(all_available);
            
            // Test chunk count
            let chunk_count = program_store.get_chunk_count(&program_id)?;
            assert_eq!(chunk_meta.chunk_count, chunk_count);
        }
        
        Ok(())
    }
}