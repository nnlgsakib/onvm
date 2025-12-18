use crate::crypto::hashing::hash_bytes;
use anyhow::Result;
use sled::Db;

#[derive(Clone)]
pub struct StateStore {
    db: Db,
    tree_name: &'static str,
}

impl StateStore {
    pub fn new(db: Db, tree_name: &'static str) -> Result<Self> {
        db.open_tree(tree_name)?; // ensure tree exists
        Ok(Self { db, tree_name })
    }

    pub fn get_scoped(&self, ns: &[u8], key: &[u8]) -> Result<Option<Vec<u8>>> {
        let tree = self.db.open_tree(self.tree_name)?;
        Ok(tree.get(prefixed(ns, key))?.map(|ivec| ivec.to_vec()))
    }

    pub fn set_scoped(&self, ns: &[u8], key: &[u8], value: &[u8]) -> Result<()> {
        let tree = self.db.open_tree(self.tree_name)?;
        tree.insert(prefixed(ns, key), value)?;
        tree.flush()?;
        Ok(())
    }

    pub fn root_scoped(&self, ns: &[u8]) -> Result<[u8; 32]> {
        let tree = self.db.open_tree(self.tree_name)?;
        let mut pairs = Vec::new();
        let ns_prefix = ns.to_vec();
        for entry in tree.scan_prefix(&ns_prefix) {
            let (k, v) = entry?;
            // strip namespace prefix for stable hashing
            let key = k[ns.len()..].to_vec();
            pairs.push((key, v.to_vec()));
        }
        Ok(merkle_root(&pairs))
    }

    pub fn get_all_scoped(&self, ns: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let tree = self.db.open_tree(self.tree_name)?;
        let mut pairs = Vec::new();
        let ns_prefix = ns.to_vec();
        for entry in tree.scan_prefix(&ns_prefix) {
            let (k, v) = entry?;
            // strip namespace prefix
            let key = k[ns.len()..].to_vec();
            pairs.push((key, v.to_vec()));
        }
        Ok(pairs)
    }
}

fn prefixed(ns: &[u8], key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ns.len() + key.len());
    out.extend_from_slice(ns);
    out.extend_from_slice(key);
    out
}

fn hash_pair(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    hash_bytes([a.as_slice(), b.as_slice()].concat().as_slice())
}

fn merkle_root(pairs: &[(Vec<u8>, Vec<u8>)]) -> [u8; 32] {
    if pairs.is_empty() {
        return hash_bytes(b"onvm-empty-root");
    }
    let mut leaves: Vec<[u8; 32]> = pairs
        .iter()
        .map(|(k, v)| hash_pair(hash_bytes(k), hash_bytes(v)))
        .collect();
    leaves.sort(); // deterministic ordering
    reduce(leaves)
}

fn reduce(mut nodes: Vec<[u8; 32]>) -> [u8; 32] {
    while nodes.len() > 1 {
        if nodes.len() % 2 == 1 {
            let last = *nodes.last().unwrap();
            nodes.push(last);
        }
        let mut next = Vec::with_capacity(nodes.len() / 2);
        for chunk in nodes.chunks(2) {
            next.push(hash_pair(chunk[0], chunk[1]));
        }
        nodes = next;
    }
    nodes[0]
}
