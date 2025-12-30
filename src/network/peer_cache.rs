use anyhow::{Context, Result};
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::storage::{PeerCacheDb, PeerCacheRecord};

pub const PEER_CACHE_MAX_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000;
pub const PEER_CACHE_MAX_PEERS: usize = 2048;
const PEER_CACHE_VERSION: u8 = 1;

#[derive(Debug, Clone)]
pub struct PeerCacheLoadStats {
    pub peer_cache_peers: usize,
    pub public_db_peers: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PeerCacheFile {
    version: u8,
    saved_at_ms: u64,
    peers: Vec<CachedPeer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPeer {
    peer_id: String,
    addrs: Vec<String>,
    last_seen_ms: u64,
}

pub struct PeerCache {
    local_peer_id: PeerId,
    pex_policy: crate::network::pex::PexPolicy,
    public_policy: crate::network::pex::PexPolicy,
    peer_cache_path: Option<PathBuf>,
    public_peer_db: Option<PeerCacheDb>,
    peer_last_seen_ms: HashMap<PeerId, u64>,
    peer_cache_dirty: bool,
    public_peer_db_dirty: bool,
}

impl PeerCache {
    pub fn new(
        local_peer_id: PeerId,
        bootnodes: &[Multiaddr],
        peer_cache_path: Option<PathBuf>,
        public_peer_db_path: Option<PathBuf>,
        pex_policy: crate::network::pex::PexPolicy,
    ) -> Self {
        let public_policy = crate::network::pex::PexPolicy {
            max_peers_per_response: pex_policy.max_peers_per_response,
            max_addrs_per_peer: pex_policy.max_addrs_per_peer,
            allow_private_addrs: false,
            allow_loopback_addrs: false,
        };

        let public_peer_db = match public_peer_db_path.as_ref() {
            Some(path) => match PeerCacheDb::open(path) {
                Ok(db) => Some(db),
                Err(err) => {
                    tracing::warn!("failed to open public peer db {}: {}", path.display(), err);
                    None
                }
            },
            None => None,
        };

        let now_ms = crate::network::ping_png::now_ms();
        let mut peer_last_seen_ms = HashMap::new();
        peer_last_seen_ms.insert(local_peer_id, now_ms);
        for addr in bootnodes {
            if let Some(peer) = peer_id_from_multiaddr(addr) {
                peer_last_seen_ms.entry(peer).or_insert(now_ms);
            }
        }

        Self {
            local_peer_id,
            pex_policy,
            public_policy,
            peer_cache_path,
            public_peer_db,
            peer_last_seen_ms,
            peer_cache_dirty: false,
            public_peer_db_dirty: false,
        }
    }

    pub fn last_seen_ms(&self, peer: &PeerId) -> Option<u64> {
        self.peer_last_seen_ms.get(peer).copied()
    }

    pub fn mark_seen(&mut self, peer: PeerId, at_ms: u64) {
        if peer == self.local_peer_id {
            return;
        }
        let seen = self.peer_last_seen_ms.entry(peer).or_insert(0);
        if *seen < at_ms {
            *seen = at_ms;
            self.peer_cache_dirty = true;
        }
    }

    pub fn note_peer_addresses_updated(
        &mut self,
        peer: PeerId,
        at_ms: u64,
        known_addrs: &HashMap<PeerId, HashSet<Multiaddr>>,
    ) {
        self.mark_seen(peer, at_ms);
        self.upsert_public_peer(peer, at_ms, known_addrs);
    }

    pub fn note_pex_discovered_peer(
        &mut self,
        peer: PeerId,
        at_ms: u64,
        known_addrs: &HashMap<PeerId, HashSet<Multiaddr>>,
    ) {
        self.mark_seen(peer, at_ms);
        self.peer_cache_dirty = true;
        self.upsert_public_peer(peer, at_ms, known_addrs);
    }

    pub async fn load(
        &mut self,
        known_addrs: &mut HashMap<PeerId, HashSet<Multiaddr>>,
    ) -> Result<PeerCacheLoadStats> {
        let public_db_peers = self.load_public_peer_db(known_addrs);
        let peer_cache_peers = self.load_peer_cache_file(known_addrs).await?;

        self.seed_public_peer_db_from_known_addrs(known_addrs);

        Ok(PeerCacheLoadStats {
            peer_cache_peers,
            public_db_peers,
        })
    }

    fn load_public_peer_db(
        &mut self,
        known_addrs: &mut HashMap<PeerId, HashSet<Multiaddr>>,
    ) -> usize {
        let Some(db) = self.public_peer_db.as_ref() else {
            return 0;
        };

        let now_ms = crate::network::ping_png::now_ms();
        let mut loaded = 0usize;

        for (peer_str, record) in db.load_all() {
            if now_ms.saturating_sub(record.last_seen_ms) > PEER_CACHE_MAX_AGE_MS {
                continue;
            }
            let Ok(pid) = PeerId::from_str(&peer_str) else {
                continue;
            };
            if pid == self.local_peer_id {
                continue;
            }

            let mut addrs = Vec::new();
            for raw in record.addrs {
                if let Ok(addr) = Multiaddr::from_str(&raw) {
                    addrs.push(addr);
                }
            }

            let validated =
                match crate::network::pex::validate_peer_addrs(&pid, &addrs, &self.public_policy) {
                    Ok(v) => v,
                    Err(_) => Vec::new(),
                };
            if validated.is_empty() {
                continue;
            }

            let entry = known_addrs.entry(pid).or_default();
            for addr in validated {
                entry.insert(strip_p2p_component(&addr));
            }

            let seen = self.peer_last_seen_ms.entry(pid).or_insert(0);
            *seen = (*seen).max(record.last_seen_ms);
            loaded += 1;
        }

        loaded
    }

    async fn load_peer_cache_file(
        &mut self,
        known_addrs: &mut HashMap<PeerId, HashSet<Multiaddr>>,
    ) -> Result<usize> {
        let Some(path) = self.peer_cache_path.as_ref() else {
            return Ok(0);
        };

        let bytes = match tokio::fs::read(path).await {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(err) => {
                tracing::warn!("failed to read peer cache {}: {}", path.display(), err);
                return Ok(0);
            }
        };

        let cache: PeerCacheFile = match serde_json::from_slice(&bytes) {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!("failed to parse peer cache {}: {}", path.display(), err);
                return Ok(0);
            }
        };

        if cache.version != PEER_CACHE_VERSION {
            tracing::warn!(
                "peer cache version mismatch (expected {}, got {}), ignoring {}",
                PEER_CACHE_VERSION,
                cache.version,
                path.display()
            );
            return Ok(0);
        }

        let now_ms = crate::network::ping_png::now_ms();
        let mut loaded = 0usize;

        for peer in cache.peers.into_iter().take(PEER_CACHE_MAX_PEERS) {
            if now_ms.saturating_sub(peer.last_seen_ms) > PEER_CACHE_MAX_AGE_MS {
                continue;
            }
            let Ok(pid) = PeerId::from_str(&peer.peer_id) else {
                continue;
            };
            if pid == self.local_peer_id {
                continue;
            }

            let mut addrs = Vec::new();
            for raw in peer.addrs {
                if let Ok(addr) = Multiaddr::from_str(&raw) {
                    addrs.push(addr);
                }
            }

            let validated =
                match crate::network::pex::validate_peer_addrs(&pid, &addrs, &self.pex_policy) {
                    Ok(v) => v,
                    Err(_) => Vec::new(),
                };
            if validated.is_empty() {
                continue;
            }

            let entry = known_addrs.entry(pid).or_default();
            for addr in validated {
                entry.insert(strip_p2p_component(&addr));
            }
            let seen = self.peer_last_seen_ms.entry(pid).or_insert(0);
            *seen = (*seen).max(peer.last_seen_ms);
            loaded += 1;
        }

        Ok(loaded)
    }

    fn seed_public_peer_db_from_known_addrs(
        &mut self,
        known_addrs: &HashMap<PeerId, HashSet<Multiaddr>>,
    ) {
        let Some(db) = self.public_peer_db.as_ref() else {
            return;
        };

        let mut updated = 0usize;
        for (peer, addrs) in known_addrs.iter() {
            if *peer == self.local_peer_id {
                continue;
            }
            let last_seen_ms = self.peer_last_seen_ms.get(peer).copied().unwrap_or(0);
            if last_seen_ms == 0 {
                continue;
            }

            let Some(record) =
                peer_cache_record_for_addrs(peer, addrs, last_seen_ms, &self.public_policy)
            else {
                continue;
            };

            if db.insert(&peer.to_string(), &record).is_ok() {
                updated += 1;
            }
        }

        if updated > 0 {
            self.public_peer_db_dirty = true;
            tracing::info!("updated public peer db with {} peers", updated);
        }
    }

    fn upsert_public_peer(
        &mut self,
        peer: PeerId,
        at_ms: u64,
        known_addrs: &HashMap<PeerId, HashSet<Multiaddr>>,
    ) {
        let Some(db) = self.public_peer_db.as_ref() else {
            return;
        };
        let Some(addrs) = known_addrs.get(&peer) else {
            return;
        };

        let Some(record) = peer_cache_record_for_addrs(&peer, addrs, at_ms, &self.public_policy)
        else {
            return;
        };

        if db.insert(&peer.to_string(), &record).is_ok() {
            self.public_peer_db_dirty = true;
        }
    }

    pub async fn persist_tick(
        &mut self,
        known_addrs: &HashMap<PeerId, HashSet<Multiaddr>>,
    ) -> Result<()> {
        if self.peer_cache_dirty {
            if let Some(path) = self.peer_cache_path.clone() {
                self.persist_peer_cache_file(&path, known_addrs).await?;
            } else {
                self.peer_cache_dirty = false;
            }
        }

        if self.public_peer_db_dirty {
            if let Some(db) = self.public_peer_db.as_ref() {
                if let Err(err) = db.flush_async().await {
                    tracing::warn!("failed to flush public peer db: {}", err);
                } else {
                    self.public_peer_db_dirty = false;
                }
            } else {
                self.public_peer_db_dirty = false;
            }
        }

        Ok(())
    }

    async fn persist_peer_cache_file(
        &mut self,
        path: &Path,
        known_addrs: &HashMap<PeerId, HashSet<Multiaddr>>,
    ) -> Result<()> {
        let now_ms = crate::network::ping_png::now_ms();
        let mut peers: Vec<CachedPeer> = Vec::new();

        for (pid, addrs) in known_addrs.iter() {
            if *pid == self.local_peer_id {
                continue;
            }
            if addrs.is_empty() {
                continue;
            }

            let last_seen_ms = self.peer_last_seen_ms.get(pid).copied().unwrap_or(0);
            if last_seen_ms == 0 {
                continue;
            }
            if now_ms.saturating_sub(last_seen_ms) > PEER_CACHE_MAX_AGE_MS {
                continue;
            }

            let addr_vec: Vec<Multiaddr> = addrs.iter().cloned().collect();
            let validated =
                match crate::network::pex::validate_peer_addrs(pid, &addr_vec, &self.pex_policy) {
                    Ok(v) => v,
                    Err(_) => Vec::new(),
                };
            if validated.is_empty() {
                continue;
            }

            let addrs: Vec<String> = validated
                .into_iter()
                .map(|a| strip_p2p_component(&a).to_string())
                .collect();
            if addrs.is_empty() {
                continue;
            }

            peers.push(CachedPeer {
                peer_id: pid.to_string(),
                addrs,
                last_seen_ms,
            });
        }

        peers.sort_by_key(|p| Reverse(p.last_seen_ms));
        peers.truncate(PEER_CACHE_MAX_PEERS);

        let cache = PeerCacheFile {
            version: PEER_CACHE_VERSION,
            saved_at_ms: now_ms,
            peers,
        };

        let bytes = serde_json::to_vec_pretty(&cache).context("encoding peer cache file")?;

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("creating peer cache dir {}", parent.display()))?;
        }

        let tmp = path.with_extension("tmp");
        tokio::fs::write(&tmp, bytes)
            .await
            .with_context(|| format!("writing peer cache tmp {}", tmp.display()))?;

        if let Err(err) = tokio::fs::rename(&tmp, path).await {
            if tokio::fs::remove_file(path).await.is_ok()
                && tokio::fs::rename(&tmp, path).await.is_ok()
            {
                self.peer_cache_dirty = false;
                return Ok(());
            }
            return Err(err).with_context(|| format!("persisting peer cache {}", path.display()));
        }

        self.peer_cache_dirty = false;
        Ok(())
    }

    pub fn prune_public_peer_db(&mut self) -> usize {
        let Some(db) = self.public_peer_db.as_ref() else {
            return 0;
        };
        let now_ms = crate::network::ping_png::now_ms();
        let cutoff_ms = now_ms.saturating_sub(PEER_CACHE_MAX_AGE_MS);
        let removed = db.prune_older_than_ms(cutoff_ms);
        if removed > 0 {
            self.public_peer_db_dirty = true;
        }
        removed
    }
}

fn peer_cache_record_for_addrs(
    peer: &PeerId,
    addrs: &HashSet<Multiaddr>,
    last_seen_ms: u64,
    policy: &crate::network::pex::PexPolicy,
) -> Option<PeerCacheRecord> {
    let addr_vec: Vec<Multiaddr> = addrs.iter().cloned().collect();
    let validated = match crate::network::pex::validate_peer_addrs(peer, &addr_vec, policy) {
        Ok(v) => v,
        Err(err) => {
            tracing::debug!("peer addr validation failed for {}: {}", peer, err);
            Vec::new()
        }
    };
    if validated.is_empty() {
        return None;
    }

    let addrs: Vec<String> = validated
        .into_iter()
        .map(|a| strip_p2p_component(&a).to_string())
        .collect();
    if addrs.is_empty() {
        return None;
    }

    Some(PeerCacheRecord {
        addrs,
        last_seen_ms,
    })
}

fn peer_id_from_multiaddr(addr: &Multiaddr) -> Option<PeerId> {
    for proto in addr.iter() {
        if let libp2p::multiaddr::Protocol::P2p(peer) = proto {
            return Some(peer);
        }
    }
    None
}

fn strip_p2p_component(addr: &Multiaddr) -> Multiaddr {
    let mut out = addr.clone();
    if let Some(last) = out.pop() {
        if matches!(last, libp2p::multiaddr::Protocol::P2p(_)) {
            return out;
        }
        out.push(last);
    }
    out
}
