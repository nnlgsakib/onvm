use crate::merkle::merkle_root;
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

    pub fn delete_scoped(&self, ns: &[u8], key: &[u8]) -> Result<()> {
        let tree = self.db.open_tree(self.tree_name)?;
        tree.remove(prefixed(ns, key))?;
        tree.flush()?;
        Ok(())
    }
}

fn prefixed(ns: &[u8], key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ns.len() + key.len());
    out.extend_from_slice(ns);
    out.extend_from_slice(key);
    out
}
