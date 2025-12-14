use crate::execution::{ExecutionEngine, ExecutionOutcome};
use crate::types::ProgramId;
use anyhow::Result;
use std::sync::Arc;

#[derive(Clone)]
pub struct ExecutionPool {
    engine: Arc<ExecutionEngine>,
    parallelism: usize,
}

impl ExecutionPool {
    pub fn new(engine: Arc<ExecutionEngine>, workers: Option<usize>) -> Self {
        let parallelism = workers.unwrap_or_else(|| {
            usize::from(
                std::thread::available_parallelism()
                    .unwrap_or(std::num::NonZeroUsize::new(1).unwrap()),
            )
            .max(2)
        });
        Self {
            engine,
            parallelism,
        }
    }

    pub async fn execute(&self, program_id: &ProgramId, input: &[u8]) -> Result<ExecutionOutcome> {
        let engine = self.engine.clone();
        let pid = program_id.clone();
        let data = input.to_vec();
        tokio::task::spawn_blocking(move || engine.execute(&pid, &data))
            .await
            .map_err(|e| anyhow::anyhow!("execution task join error: {e}"))?
    }

    pub fn max_parallel(&self) -> usize {
        self.parallelism
    }
}
