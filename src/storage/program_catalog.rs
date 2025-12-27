use crate::types::{AggregatedReceipt, ProgramId, ProgramManifest, ReceiptId, StateCommitment};
use anyhow::{bail, Result};
use sled::Db;

const TREE_MANIFESTS: &str = "program_manifests";
const TREE_STATES: &str = "program_states";
const TREE_RECEIPTS: &str = "program_receipts";

pub struct ProgramCatalog {
    db: Db,
}

impl ProgramCatalog {
    pub fn new(db: Db) -> Result<Self> {
        db.open_tree(TREE_MANIFESTS)?;
        db.open_tree(TREE_STATES)?;
        db.open_tree(TREE_RECEIPTS)?;
        Ok(Self { db })
    }

    pub fn store_manifest(&self, manifest: &ProgramManifest) -> Result<()> {
        let tree = self.db.open_tree(TREE_MANIFESTS)?;
        let key = manifest.program_id.0;
        let encoded = bincode::serde::encode_to_vec(manifest, bincode::config::standard())?;
        tree.insert(key, encoded)?;
        tree.flush()?;
        Ok(())
    }

    pub fn get_manifest(&self, program_id: &ProgramId) -> Result<Option<ProgramManifest>> {
        let tree = self.db.open_tree(TREE_MANIFESTS)?;
        let Some(raw) = tree.get(program_id.0)? else {
            return Ok(None);
        };
        let (manifest, _): (ProgramManifest, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(manifest))
    }

    pub fn list_manifests(&self) -> Result<Vec<ProgramManifest>> {
        let tree = self.db.open_tree(TREE_MANIFESTS)?;
        let mut out = Vec::new();
        for entry in tree.iter() {
            let (_, val) = entry?;
            let (manifest, _): (ProgramManifest, _) =
                bincode::serde::decode_from_slice(&val, bincode::config::standard())?;
            out.push(manifest);
        }
        Ok(out)
    }

    pub fn store_state_commitment(&self, commitment: &StateCommitment) -> Result<()> {
        let tree = self.db.open_tree(TREE_STATES)?;
        let mut key = Vec::with_capacity(32 + 8);
        key.extend_from_slice(&commitment.program_id.0);
        key.extend_from_slice(&commitment.height.to_be_bytes());
        let encoded = bincode::serde::encode_to_vec(commitment, bincode::config::standard())?;
        tree.insert(key, encoded)?;
        tree.flush()?;
        Ok(())
    }

    pub fn latest_state_commitment(
        &self,
        program_id: &ProgramId,
    ) -> Result<Option<StateCommitment>> {
        let tree = self.db.open_tree(TREE_STATES)?;
        let prefix = program_id.0;
        let mut iter = tree.scan_prefix(prefix);
        let mut last: Option<(Vec<u8>, sled::IVec)> = None;
        while let Some(entry) = iter.next() {
            let (k, v) = entry?;
            last = Some((k.to_vec(), v));
        }
        let Some((_, raw)) = last else {
            return Ok(None);
        };
        let (commitment, _): (StateCommitment, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(commitment))
    }

    pub fn get_state_commitment(
        &self,
        program_id: &ProgramId,
        height: u64,
    ) -> Result<Option<StateCommitment>> {
        let tree = self.db.open_tree(TREE_STATES)?;
        let mut key = Vec::with_capacity(32 + 8);
        key.extend_from_slice(&program_id.0);
        key.extend_from_slice(&height.to_be_bytes());
        let Some(raw) = tree.get(key)? else {
            return Ok(None);
        };
        let (commitment, _): (StateCommitment, _) =
            bincode::serde::decode_from_slice(&raw, bincode::config::standard())?;
        Ok(Some(commitment))
    }

    pub fn store_aggregated_receipt(&self, receipt: &AggregatedReceipt) -> Result<()> {
        let tree = self.db.open_tree(TREE_RECEIPTS)?;
        let receipt_id: ReceiptId = receipt.receipt.id();
        let mut key = Vec::with_capacity(32 + 8);
        key.extend_from_slice(&receipt.receipt.program_id.0);
        key.extend_from_slice(&receipt_id);
        let encoded = bincode::serde::encode_to_vec(receipt, bincode::config::standard())?;
        tree.insert(key, encoded)?;
        tree.flush()?;
        Ok(())
    }

    pub fn receipts_for_program(&self, program_id: &ProgramId) -> Result<Vec<AggregatedReceipt>> {
        let tree = self.db.open_tree(TREE_RECEIPTS)?;
        let prefix = program_id.0;
        let mut out = Vec::new();
        for entry in tree.scan_prefix(prefix) {
            let (_, v) = entry?;
            let (receipt, _): (AggregatedReceipt, _) =
                bincode::serde::decode_from_slice(&v, bincode::config::standard())?;
            out.push(receipt);
        }
        Ok(out)
    }

    pub fn verify_initial_sync(&self, manifest: &ProgramManifest) -> Result<()> {
        let stored = self.get_manifest(&manifest.program_id)?;
        if let Some(existing) = stored {
            if existing.initial_state_root != manifest.initial_state_root {
                bail!("manifest conflict for program {}", manifest.program_id);
            }
            if existing.committee.is_some()
                && manifest.committee.is_some()
                && existing.committee != manifest.committee
            {
                bail!("committee conflict for program {}", manifest.program_id);
            }
            if existing.committee.is_some() && manifest.committee.is_none() {
                bail!("committee conflict for program {}", manifest.program_id);
            }
        }
        Ok(())
    }
}
