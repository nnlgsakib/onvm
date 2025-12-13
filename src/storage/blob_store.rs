use crate::crypto::hashing::hash_bytes;
use crate::types::{BlobId, BlobMetadata, NodeId};
use anyhow::{anyhow, Result};
use sled::Db;

const DEFAULT_CHUNK_SIZE: usize = 1 << 20; // 1 MiB

pub struct BlobStore {
    db: Db,
}

impl BlobStore {
    pub fn new(db: Db, _root: impl AsRef<std::path::Path>) -> Result<Self> {
        // All blob data lives in sled; root is ignored for backward compatibility.
        Ok(Self { db })
    }

    pub fn put(
        &self,
        data: &[u8],
        mime: Option<String>,
        publisher: NodeId,
    ) -> Result<BlobMetadata> {
        if data.is_empty() {
            return Err(anyhow!("cannot store empty blob"));
        }
        let chunk_size = std::cmp::max(1, std::cmp::min(DEFAULT_CHUNK_SIZE, data.len()));
        let chunks = chunk_bytes(data, chunk_size);
        let chunk_hashes: Vec<[u8; 32]> = chunks.iter().map(|c| hash_bytes(c)).collect();
        let merkle_root = merkle_root(&chunk_hashes);

        let id = BlobId::new(data);
        let meta = BlobMetadata {
            id: id.clone(),
            publisher,
            size: data.len() as u64,
            mime,
            chunk_size: chunk_size as u32,
            chunk_count: chunk_hashes.len() as u32,
            chunk_hashes: chunk_hashes.clone(),
            merkle_root,
        };

        self.persist(&meta, &chunks)?;
        Ok(meta)
    }

    pub fn replicate(&self, meta: &BlobMetadata, data: &[u8]) -> Result<()> {
        if BlobId::new(data) != meta.id {
            return Err(anyhow!("blob id mismatch"));
        }
        if data.is_empty() {
            return Err(anyhow!("cannot replicate empty blob"));
        }
        let chunk_size = meta.chunk_size as usize;
        if chunk_size == 0 {
            return Err(anyhow!("invalid chunk size"));
        }
        let chunks = chunk_bytes(data, chunk_size);
        if chunks.len() as u32 != meta.chunk_count {
            return Err(anyhow!(
                "chunk count mismatch (meta {}, computed {})",
                meta.chunk_count,
                chunks.len()
            ));
        }
        let chunk_hashes: Vec<[u8; 32]> = chunks.iter().map(|c| hash_bytes(c)).collect();
        if chunk_hashes != meta.chunk_hashes {
            return Err(anyhow!("chunk hash mismatch"));
        }
        let root = merkle_root(&chunk_hashes);
        if root != meta.merkle_root {
            return Err(anyhow!("merkle root mismatch"));
        }
        self.persist(meta, &chunks)
    }

    pub fn get(&self, id: &BlobId) -> Result<Vec<u8>> {
        let meta = self
            .metadata(id)?
            .ok_or_else(|| anyhow!("blob metadata missing"))?;
        let chunk_tree = self.db.open_tree("blob_chunks")?;
        let mut out = Vec::with_capacity(meta.size as usize);
        let mut computed_hashes = Vec::with_capacity(meta.chunk_count as usize);
        for idx in 0..meta.chunk_count {
            let key = chunk_key(&meta.id, idx);
            let Some(chunk) = chunk_tree.get(key.clone())? else {
                return Err(anyhow!("missing chunk {idx} for blob {id}"));
            };
            computed_hashes.push(hash_bytes(&chunk));
            out.extend_from_slice(&chunk);
        }
        if computed_hashes != meta.chunk_hashes {
            return Err(anyhow!("chunk hash mismatch on read"));
        }
        let root = merkle_root(&computed_hashes);
        if root != meta.merkle_root {
            return Err(anyhow!("merkle root mismatch on read"));
        }
        if out.len() as u64 != meta.size {
            return Err(anyhow!("size mismatch on read"));
        }
        Ok(out)
    }

    pub fn metadata(&self, id: &BlobId) -> Result<Option<BlobMetadata>> {
        let tree = self.db.open_tree("blob_meta")?;
        let Some(raw) = tree.get(id.0)? else {
            return Ok(None);
        };
        let (meta, _): (BlobMetadata, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(meta))
    }

    pub fn list(&self) -> Result<Vec<BlobMetadata>> {
        let tree = self.db.open_tree("blob_meta")?;
        let mut out = Vec::new();
        for entry in tree.iter() {
            let (_, v) = entry?;
            let (meta, _): (BlobMetadata, _) =
                bincode::serde::decode_from_slice(&v, bincode::config::standard())?;
            out.push(meta);
        }
        Ok(out)
    }

    fn persist(&self, meta: &BlobMetadata, chunks: &[Vec<u8>]) -> Result<()> {
        let meta_tree = self.db.open_tree("blob_meta")?;
        let chunk_tree = self.db.open_tree("blob_chunks")?;

        let encoded = bincode::serde::encode_to_vec(meta, bincode::config::standard())?;
        meta_tree.insert(meta.id.0, encoded)?;

        for (idx, chunk) in chunks.iter().enumerate() {
            let key = chunk_key(&meta.id, idx as u32);
            chunk_tree.insert(key, chunk.as_slice())?;
        }
        meta_tree.flush()?;
        chunk_tree.flush()?;
        Ok(())
    }
}

fn chunk_bytes(data: &[u8], chunk_size: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < data.len() {
        let end = std::cmp::min(start + chunk_size, data.len());
        out.push(data[start..end].to_vec());
        start = end;
    }
    out
}

fn chunk_key(id: &BlobId, idx: u32) -> Vec<u8> {
    let mut key = Vec::with_capacity(36);
    key.extend_from_slice(&id.0);
    key.extend_from_slice(&idx.to_be_bytes());
    key
}

fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return hash_bytes(&[]);
    }
    let mut current = leaves.to_vec();
    while current.len() > 1 {
        let mut next = Vec::with_capacity((current.len() + 1) / 2);
        let mut iter = current.chunks(2);
        while let Some(pair) = iter.next() {
            let mut buf = Vec::with_capacity(64);
            buf.extend_from_slice(&pair[0]);
            if pair.len() == 2 {
                buf.extend_from_slice(&pair[1]);
            } else {
                buf.extend_from_slice(&pair[0]);
            }
            next.push(hash_bytes(&buf));
        }
        current = next;
    }
    current[0]
}
