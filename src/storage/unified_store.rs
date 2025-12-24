use crate::types::{
    Chunk, ChunkDescriptor, ChunkId, Manifest, ManifestId, Object, ObjectId, ObjectType,
};
use anyhow::{anyhow, Result};
use fastcdc::v2020::FastCDC;
use sled::Db;

pub const CHUNK_SIZE_MIN: usize = 256 * 1024;
pub const CHUNK_SIZE_AVG: usize = 1024 * 1024;
pub const CHUNK_SIZE_MAX: usize = 4 * 1024 * 1024;

pub struct UnifiedStore {
    db: Db,
}

impl UnifiedStore {
    pub fn new(db: Db) -> Result<Self> {
        db.open_tree("objects")?;
        db.open_tree("manifests")?;
        db.open_tree("chunks")?;
        db.open_tree("manifest_index")?;
        Ok(Self { db })
    }

    pub fn put_object(
        &self,
        data: &[u8],
        object_type: ObjectType,
        publisher: crate::types::NodeId,
    ) -> Result<Object> {
        if data.is_empty() {
            return Err(anyhow!("cannot store empty object"));
        }

        let chunks = chunk_data(data);

        let content_hash = ObjectId::new(data);

        let type_hash = {
            let type_bytes =
                bincode::serde::encode_to_vec(&object_type, bincode::config::standard())?;
            blake3::hash(&type_bytes)
        };

        let mut combined = Vec::new();
        combined.extend_from_slice(&content_hash.0);
        combined.extend_from_slice(type_hash.as_bytes());
        let object_id = ObjectId(blake3::hash(&combined).into());

        let mut chunk_descriptors = Vec::new();
        let mut offset = 0u64;

        for chunk in &chunks {
            chunk_descriptors.push(ChunkDescriptor {
                chunk_id: chunk.id,
                size: chunk.data.len() as u32,
                offset,
            });
            offset += chunk.data.len() as u64;
        }

        let manifest = Manifest {
            object_id,
            content_hash: content_hash.0,
            chunks: chunk_descriptors,
            version: 1,
        };

        manifest.validate()?;
        let manifest_id = manifest.id();

        let object = Object {
            id: object_id,
            object_type,
            total_size: data.len() as u64,
            chunk_count: chunks.len() as u32,
            manifest_id,
            publisher,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        self.store_object(&object)?;
        self.store_manifest(&manifest)?;
        for chunk in chunks {
            self.store_chunk(&chunk)?;
        }

        Ok(object)
    }

    pub fn get_object(&self, object_id: &ObjectId) -> Result<Vec<u8>> {
        let manifest = self
            .get_manifest_by_object(object_id)?
            .ok_or_else(|| anyhow!("manifest not found for object"))?;

        manifest.validate()?;

        let mut data = Vec::with_capacity(manifest.total_size() as usize);

        for chunk_desc in &manifest.chunks {
            let chunk = self
                .get_chunk(&chunk_desc.chunk_id)?
                .ok_or_else(|| anyhow!("chunk {} not found", chunk_desc.chunk_id))?;

            chunk.verify()?;

            if chunk.data.len() != chunk_desc.size as usize {
                return Err(anyhow!(
                    "chunk size mismatch: expected {}, got {}",
                    chunk_desc.size,
                    chunk.data.len()
                ));
            }

            data.extend_from_slice(&chunk.data);
        }

        let computed_content_hash = blake3::hash(&data);
        if computed_content_hash.as_bytes() != &manifest.content_hash {
            return Err(anyhow!(
                "content hash mismatch after reassembly: expected {}, got {}",
                hex::encode(manifest.content_hash),
                hex::encode(computed_content_hash.as_bytes())
            ));
        }

        Ok(data)
    }

    pub fn store_object(&self, object: &Object) -> Result<()> {
        let tree = self.db.open_tree("objects")?;
        let encoded = bincode::serde::encode_to_vec(object, bincode::config::standard())?;
        tree.insert(object.id.0, encoded)?;
        tree.flush()?;
        Ok(())
    }

    pub fn get_object_metadata(&self, object_id: &ObjectId) -> Result<Option<Object>> {
        let tree = self.db.open_tree("objects")?;
        let Some(raw) = tree.get(object_id.0)? else {
            return Ok(None);
        };
        let (object, _): (Object, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(object))
    }

    pub fn store_manifest(&self, manifest: &Manifest) -> Result<()> {
        let tree = self.db.open_tree("manifests")?;
        let encoded = bincode::serde::encode_to_vec(manifest, bincode::config::standard())?;
        tree.insert(manifest.object_id.0, encoded)?;
        tree.flush()?;

        let index = self.db.open_tree("manifest_index")?;
        let manifest_id = manifest.id();
        index.insert(manifest_id.0, &manifest.object_id.0)?;
        index.flush()?;
        Ok(())
    }

    pub fn get_manifest(&self, manifest_id: &ManifestId) -> Result<Option<Manifest>> {
        let index = self.db.open_tree("manifest_index")?;
        if let Some(raw) = index.get(manifest_id.0)? {
            if raw.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&raw);
                let object_id = ObjectId(arr);
                if let Some(manifest) = self.get_manifest_by_object(&object_id)? {
                    return Ok(Some(manifest));
                }
            }
        }

        // Fallback for older databases without index.
        let tree = self.db.open_tree("manifests")?;
        for entry in tree.iter() {
            let (_, v) = entry?;
            let (manifest, _): (Manifest, _) =
                bincode::serde::decode_from_slice(&v, bincode::config::standard())?;
            if manifest.id() == *manifest_id {
                index.insert(manifest_id.0, &manifest.object_id.0)?;
                let _ = index.flush();
                return Ok(Some(manifest));
            }
        }

        Ok(None)
    }

    pub fn get_manifest_by_object(&self, object_id: &ObjectId) -> Result<Option<Manifest>> {
        let tree = self.db.open_tree("manifests")?;
        let Some(raw) = tree.get(object_id.0)? else {
            return Ok(None);
        };
        let (manifest, _): (Manifest, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(manifest))
    }

    pub fn store_chunk(&self, chunk: &Chunk) -> Result<()> {
        chunk.verify()?;
        let tree = self.db.open_tree("chunks")?;
        tree.insert(chunk.id.0, &chunk.data[..])?;
        tree.flush()?;
        Ok(())
    }

    pub fn get_chunk(&self, chunk_id: &ChunkId) -> Result<Option<Chunk>> {
        let tree = self.db.open_tree("chunks")?;
        let Some(raw) = tree.get(chunk_id.0)? else {
            return Ok(None);
        };
        let chunk = Chunk {
            id: *chunk_id,
            data: raw.to_vec(),
        };
        chunk.verify()?;
        Ok(Some(chunk))
    }

    pub fn has_chunk(&self, chunk_id: &ChunkId) -> Result<bool> {
        let tree = self.db.open_tree("chunks")?;
        Ok(tree.contains_key(chunk_id.0)?)
    }

    pub fn list_objects(&self) -> Result<Vec<Object>> {
        let tree = self.db.open_tree("objects")?;
        let mut objects = Vec::new();
        for entry in tree.iter() {
            let (_, v) = entry?;
            match bincode::serde::decode_from_slice::<Object, _>(&v, bincode::config::standard()) {
                Ok((object, _)) => objects.push(object),
                Err(e) => {
                    tracing::warn!("skipping malformed object record: {}", e);
                    continue;
                }
            }
        }
        Ok(objects)
    }

    pub fn get_missing_chunks(&self, manifest: &Manifest) -> Result<Vec<ChunkId>> {
        let mut missing = Vec::new();
        for chunk_desc in &manifest.chunks {
            if !self.has_chunk(&chunk_desc.chunk_id)? {
                missing.push(chunk_desc.chunk_id);
            }
        }
        Ok(missing)
    }

    pub fn is_complete(&self, object_id: &ObjectId) -> Result<bool> {
        let manifest = match self.get_manifest_by_object(object_id)? {
            Some(m) => m,
            None => return Ok(false),
        };

        let missing = self.get_missing_chunks(&manifest)?;
        Ok(missing.is_empty())
    }
}

pub fn chunk_data(data: &[u8]) -> Vec<Chunk> {
    if data.is_empty() {
        return Vec::new();
    }

    let chunker = FastCDC::new(
        data,
        CHUNK_SIZE_MIN as u32,
        CHUNK_SIZE_AVG as u32,
        CHUNK_SIZE_MAX as u32,
    );

    chunker
        .map(|entry| {
            let chunk_data = data[entry.offset..entry.offset + entry.length].to_vec();
            Chunk::new(chunk_data)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NodeId;
    use sled::Config;

    #[test]
    fn test_unified_store_roundtrip() -> Result<()> {
        let config = Config::new().temporary(true);
        let db = config.open()?;
        let store = UnifiedStore::new(db)?;

        let test_data = vec![0u8; 5 * 1024 * 1024];
        let publisher = NodeId::new(&[1u8; 32]);

        let object = store.put_object(
            &test_data,
            ObjectType::blob_with_random_salt(Some("application/octet-stream".to_string())),
            publisher,
        )?;

        assert_eq!(object.total_size, test_data.len() as u64);
        assert!(object.chunk_count > 1);

        let retrieved = store.get_object(&object.id)?;
        assert_eq!(test_data, retrieved);

        Ok(())
    }

    #[test]
    fn test_chunk_verification() -> Result<()> {
        let config = Config::new().temporary(true);
        let db = config.open()?;
        let store = UnifiedStore::new(db)?;

        let test_data = b"hello world";
        let chunk = Chunk::new(test_data.to_vec());

        store.store_chunk(&chunk)?;

        let retrieved = store.get_chunk(&chunk.id)?.unwrap();
        assert_eq!(chunk.data, retrieved.data);

        Ok(())
    }

    #[test]
    fn test_manifest_validation() -> Result<()> {
        let data = vec![0u8; 3 * 1024 * 1024];
        let chunks = chunk_data(&data);

        let mut chunk_descriptors = Vec::new();
        let mut offset = 0u64;

        for chunk in &chunks {
            chunk_descriptors.push(ChunkDescriptor {
                chunk_id: chunk.id,
                size: chunk.data.len() as u32,
                offset,
            });
            offset += chunk.data.len() as u64;
        }

        let manifest = Manifest {
            object_id: ObjectId::new(&data),
            content_hash: *blake3::hash(&data).as_bytes(),
            chunks: chunk_descriptors,
            version: 1,
        };

        manifest.validate()?;

        Ok(())
    }
}
