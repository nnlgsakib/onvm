use crate::network::DagInventory;
use crate::types::ProgramId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct SyncState {
    pub last_inventory: Option<DagInventory>,
    pub missing_programs: usize,
    pub missing_blobs: usize,
    pub missing_execs: usize,
    pub pending_program_ids: HashSet<ProgramId>,
    pub pending_exec_ids: HashSet<[u8; 32]>,
    pub last_seen: bool,
}
