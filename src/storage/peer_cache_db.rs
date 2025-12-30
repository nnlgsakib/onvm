use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCacheRecord {
    pub addrs: Vec<String>,
    pub last_seen_ms: u64,
}

#[derive(Clone)]
pub struct PeerCacheDb {
    db: sled::Db,
}

impl PeerCacheDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let db = sled::open(path)
            .with_context(|| format!("opening peer cache db {}", path.display()))?;
        Ok(Self { db })
    }

    pub fn insert(&self, peer_id: &str, record: &PeerCacheRecord) -> Result<()> {
        let value = serde_json::to_vec(record).context("encoding peer cache record")?;
        self.db
            .insert(peer_id.as_bytes(), value)
            .context("inserting peer cache record")?;
        Ok(())
    }

    pub fn load_all(&self) -> Vec<(String, PeerCacheRecord)> {
        let mut out = Vec::new();
        for item in self.db.iter() {
            let Ok((key, value)) = item else {
                continue;
            };
            let Ok(key_str) = std::str::from_utf8(&key) else {
                continue;
            };
            let Ok(record) = serde_json::from_slice::<PeerCacheRecord>(&value) else {
                continue;
            };
            out.push((key_str.to_string(), record));
        }
        out
    }

    pub fn prune_older_than_ms(&self, cutoff_ms: u64) -> usize {
        let mut removed = 0usize;
        let mut remove_keys: Vec<sled::IVec> = Vec::new();
        for item in self.db.iter() {
            let Ok((key, value)) = item else {
                continue;
            };
            let record = match serde_json::from_slice::<PeerCacheRecord>(&value) {
                Ok(r) => r,
                Err(_) => {
                    remove_keys.push(key);
                    continue;
                }
            };
            if record.last_seen_ms < cutoff_ms {
                remove_keys.push(key);
            }
        }

        for key in remove_keys {
            if self.db.remove(key).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    pub async fn flush_async(&self) -> Result<()> {
        self.db
            .flush_async()
            .await
            .context("flushing peer cache db")?;
        Ok(())
    }
}
