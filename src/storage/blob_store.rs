use crate::crypto::hashing::hash_bytes;
use crate::types::{BlobId, BlobMetadata, NodeId};
use anyhow::{anyhow, Result};
use fastcdc::v2020::FastCDC;
use reed_solomon_erasure::galois_8::ReedSolomon;
use sled::Db;

const FASTCDC_MIN: usize = 64 * 1024; // 64 KiB
const FASTCDC_AVG: usize = 256 * 1024; // 256 KiB target
const FASTCDC_MAX: usize = 512 * 1024; // 512 KiB cap to fit transfer limits
const STATIC_CHUNK_SIZE: usize = 1 * 1024 * 1024; // 1 MiB
const STATIC_THRESHOLD: usize = 100 * 1024 * 1024; // 100 MiB
const DATA_SHARDS: usize = 8;
const PARITY_SHARDS: usize = 4;

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
        let (chunks, chunk_sizes) = chunk_bytes(data);
        let chunk_hashes: Vec<[u8; 32]> = chunks.iter().map(|c| hash_bytes(c)).collect();
        let merkle_root = merkle_root(&chunk_hashes);

        let shards_per_chunk = encode_chunks(&chunks, DATA_SHARDS, PARITY_SHARDS)?;

        let id = BlobId::new(data);
        let meta = BlobMetadata {
            id: id.clone(),
            publisher,
            size: data.len() as u64,
            mime,
            chunk_sizes: chunk_sizes.iter().map(|s| *s as u32).collect(),
            chunk_hashes: chunk_hashes.clone(),
            merkle_root,
            data_shards: DATA_SHARDS as u8,
            parity_shards: PARITY_SHARDS as u8,
        };

        self.persist(&meta, &shards_per_chunk)?;
        Ok(meta)
    }

    pub fn replicate(&self, meta: &BlobMetadata, data: &[u8]) -> Result<()> {
        if BlobId::new(data) != meta.id {
            return Err(anyhow!("blob id mismatch"));
        }
        if data.is_empty() {
            return Err(anyhow!("cannot replicate empty blob"));
        }
        if meta.data_shards == 0 || meta.parity_shards == 0 {
            return Err(anyhow!("invalid erasure coding parameters"));
        }
        let chunk_sizes: Vec<usize> = meta.chunk_sizes.iter().map(|c| *c as usize).collect();
        if chunk_sizes.is_empty() {
            return Err(anyhow!("invalid chunk sizes in metadata"));
        }
        let chunks = chunk_bytes_with_sizes(data, &chunk_sizes)?;
        if chunks.len() != meta.chunk_sizes.len() {
            return Err(anyhow!(
                "chunk count mismatch (meta {}, computed {})",
                meta.chunk_sizes.len(),
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
        let shards_per_chunk = encode_chunks(
            &chunks,
            meta.data_shards as usize,
            meta.parity_shards as usize,
        )?;
        self.persist(meta, &shards_per_chunk)
    }

    pub fn get(&self, id: &BlobId) -> Result<Vec<u8>> {
        let meta = self
            .metadata(id)?
            .ok_or_else(|| anyhow!("blob metadata missing"))?;
        let mut out = Vec::with_capacity(meta.size as usize);
        let mut computed_hashes = Vec::with_capacity(meta.chunk_sizes.len());
        for (idx, expected_len) in meta.chunk_sizes.iter().enumerate() {
            let shards = self.load_shards(&meta.id, idx as u32)?;
            let chunk = reconstruct_chunk(
                shards,
                meta.data_shards as usize,
                meta.parity_shards as usize,
                *expected_len as usize,
            )?;
            let h = hash_bytes(&chunk);
            if h != meta.chunk_hashes[idx] {
                return Err(anyhow!("chunk hash mismatch on read"));
            }
            computed_hashes.push(h);
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

    /// Persist metadata without storing chunk data (used for metadata-only sync paths).
    pub fn store_metadata(&self, meta: &BlobMetadata) -> Result<()> {
        let tree = self.db.open_tree("blob_meta")?;
        let encoded = bincode::serde::encode_to_vec(meta, bincode::config::standard())?;
        tree.insert(meta.id.0, encoded)?;
        tree.flush()?;
        Ok(())
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

    pub fn store_chunk(
        &self,
        meta: &BlobMetadata,
        chunk_idx: usize,
        shards: &[(u8, Vec<u8>)],
    ) -> Result<()> {
        if chunk_idx >= meta.chunk_sizes.len() {
            return Err(anyhow!("chunk idx out of bounds"));
        }
        let expected_len = meta.chunk_sizes[chunk_idx] as usize;
        let chunk = reconstruct_chunk(
            shards.to_vec(),
            meta.data_shards as usize,
            meta.parity_shards as usize,
            expected_len,
        )?;
        if hash_bytes(&chunk) != meta.chunk_hashes[chunk_idx] {
            return Err(anyhow!("chunk hash mismatch for idx {}", chunk_idx));
        }
        // re-encode to normalize shard sizing and store
        let encoded = encode_chunks(
            &[chunk],
            meta.data_shards as usize,
            meta.parity_shards as usize,
        )?;
        let chunk_tree = self.db.open_tree("blob_chunk_shards")?;
        for (shard_idx, data) in encoded
            .get(0)
            .ok_or_else(|| anyhow!("no encoded shards"))?
            .iter()
        {
            let key = shard_key(&meta.id, chunk_idx as u32, *shard_idx);
            chunk_tree.insert(key, data.as_slice())?;
        }
        chunk_tree.flush()?;
        Ok(())
    }

    pub fn try_load_chunk(&self, meta: &BlobMetadata, chunk_idx: usize) -> Option<Vec<u8>> {
        let shards = self.load_shards(&meta.id, chunk_idx as u32).ok()?;
        reconstruct_chunk(
            shards,
            meta.data_shards as usize,
            meta.parity_shards as usize,
            meta.chunk_sizes.get(chunk_idx).copied().unwrap_or(0) as usize,
        )
        .ok()
    }

    fn persist(&self, meta: &BlobMetadata, shards_per_chunk: &[Vec<(u8, Vec<u8>)>]) -> Result<()> {
        let meta_tree = self.db.open_tree("blob_meta")?;
        let chunk_tree = self.db.open_tree("blob_chunk_shards")?;

        let encoded = bincode::serde::encode_to_vec(meta, bincode::config::standard())?;
        meta_tree.insert(meta.id.0, encoded)?;

        for (idx, shards) in shards_per_chunk.iter().enumerate() {
            for (shard_idx, data) in shards {
                let key = shard_key(&meta.id, idx as u32, *shard_idx);
                chunk_tree.insert(key, data.as_slice())?;
            }
        }
        meta_tree.flush()?;
        chunk_tree.flush()?;
        Ok(())
    }

    pub fn load_shards(&self, id: &BlobId, chunk_idx: u32) -> Result<Vec<(u8, Vec<u8>)>> {
        let chunk_tree = self.db.open_tree("blob_chunk_shards")?;
        let mut out = Vec::new();
        // Iterate over possible shard indices; since we don't store shard count explicitly, scan all with prefix.
        let prefix = shard_prefix(id, chunk_idx);
        for entry in chunk_tree.scan_prefix(prefix) {
            let (k, v) = entry?;
            let shard_idx = *k.last().unwrap_or(&0);
            out.push((shard_idx, v.to_vec()));
        }
        if out.is_empty() {
            return Err(anyhow!(
                "no shards found for chunk {chunk_idx} of blob {id}"
            ));
        }
        Ok(out)
    }
}

fn chunk_bytes(data: &[u8]) -> (Vec<Vec<u8>>, Vec<usize>) {
    if data.len() <= STATIC_THRESHOLD {
        let mut chunks = Vec::new();
        let mut sizes = Vec::new();
        let mut offset = 0;
        while offset < data.len() {
            let end = std::cmp::min(offset + STATIC_CHUNK_SIZE, data.len());
            let slice = data[offset..end].to_vec();
            sizes.push(slice.len());
            chunks.push(slice);
            offset = end;
        }
        return (chunks, sizes);
    }

    let mut chunks = Vec::new();
    let mut sizes = Vec::new();
    let detector = FastCDC::new(
        data,
        FASTCDC_MIN as u32,
        FASTCDC_AVG as u32,
        FASTCDC_MAX as u32,
    );
    for r in detector {
        let start = r.offset;
        let end = r.offset + r.length;
        chunks.push(data[start..end].to_vec());
        sizes.push(r.length);
    }
    (chunks, sizes)
}

fn chunk_bytes_with_sizes(data: &[u8], sizes: &[usize]) -> Result<Vec<Vec<u8>>> {
    let mut chunks = Vec::new();
    let mut offset = 0usize;
    for sz in sizes {
        let end = offset
            .checked_add(*sz)
            .ok_or_else(|| anyhow!("chunk size overflow"))?;
        if end > data.len() {
            return Err(anyhow!("chunk size out of bounds"));
        }
        chunks.push(data[offset..end].to_vec());
        offset = end;
    }
    if offset != data.len() {
        return Err(anyhow!("chunk sizes do not cover blob"));
    }
    Ok(chunks)
}

fn encode_chunks(
    chunks: &[Vec<u8>],
    data_shards: usize,
    parity_shards: usize,
) -> Result<Vec<Vec<(u8, Vec<u8>)>>> {
    let mut shards_per_chunk = Vec::new();
    for chunk in chunks {
        let shard_len = (chunk.len() + data_shards - 1) / data_shards;
        let total_shards = data_shards + parity_shards;
        let mut shards: Vec<Vec<u8>> = Vec::with_capacity(total_shards);
        for i in 0..data_shards {
            let start = i * shard_len;
            let end = std::cmp::min(start + shard_len, chunk.len());
            let mut shard = vec![0u8; shard_len];
            if start < chunk.len() {
                shard[..end - start].copy_from_slice(&chunk[start..end]);
            }
            shards.push(shard);
        }
        for _ in 0..parity_shards {
            shards.push(vec![0u8; shard_len]);
        }
        let r = ReedSolomon::new(data_shards, parity_shards)?;
        let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
        r.encode(&mut shard_refs)?;
        let mut encoded = Vec::new();
        for (idx, shard) in shard_refs.into_iter().enumerate() {
            encoded.push((idx as u8, shard.to_vec()));
        }
        shards_per_chunk.push(encoded);
    }
    Ok(shards_per_chunk)
}

pub fn reconstruct_chunk(
    shards: Vec<(u8, Vec<u8>)>,
    data_shards: usize,
    parity_shards: usize,
    expected_len: usize,
) -> Result<Vec<u8>> {
    let r = ReedSolomon::new(data_shards, parity_shards)?;
    let shard_len = shards
        .get(0)
        .ok_or_else(|| anyhow!("missing shards"))?
        .1
        .len();
    let total = data_shards + parity_shards;
    let mut all: Vec<Option<Box<[u8]>>> = vec![None; total];
    for (idx, shard) in shards {
        if idx as usize >= total {
            continue;
        }
        all[idx as usize] = Some(shard.into_boxed_slice());
    }
    r.reconstruct_data(&mut all)?;
    let mut out = Vec::with_capacity(data_shards * shard_len);
    for i in 0..data_shards {
        if let Some(ref shard) = all[i] {
            out.extend_from_slice(shard);
        }
    }
    out.truncate(expected_len);
    Ok(out)
}

fn shard_key(id: &BlobId, chunk_idx: u32, shard_idx: u8) -> Vec<u8> {
    let mut key = Vec::with_capacity(37);
    key.extend_from_slice(&id.0);
    key.extend_from_slice(&chunk_idx.to_be_bytes());
    key.push(shard_idx);
    key
}

fn shard_prefix(id: &BlobId, chunk_idx: u32) -> Vec<u8> {
    let mut key = Vec::with_capacity(36);
    key.extend_from_slice(&id.0);
    key.extend_from_slice(&chunk_idx.to_be_bytes());
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
