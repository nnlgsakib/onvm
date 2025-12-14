use crate::consensus::DagEngine;
pub mod sync;
use anyhow::Result;
use std::sync::Arc;
use tokio::time::{sleep, Duration, Instant};
pub struct SyncMan {
    dag: Arc<DagEngine>,
}

impl SyncMan {
    pub fn new(dag: Arc<DagEngine>) -> Self {
        Self { dag }
    }

    pub async fn await_initial_sync(&self, timeout: Duration) -> Result<()> {
        let mut timeout_start: Option<Instant> = None;
        let mut last_gaps: Option<(usize, usize, usize)> = None;
        loop {
            if self.dag.peer_count().await == 0 {
                timeout_start = None;
                last_gaps = None;
                sleep(Duration::from_millis(500)).await;
                continue;
            }

            self.dag.request_inventory().await?;
            self.dag.refresh_sync_state().await?;
            if self.dag.is_fully_synced().await {
                tracing::info!("sync complete: programs/blobs/executions fully present");
                return Ok(());
            }

            let gaps = self.dag.sync_gaps().await;
            if let Some(g) = gaps {
                if last_gaps.map(|prev| prev != g).unwrap_or(true) {
                    timeout_start = Some(Instant::now());
                    Self::log_progress(g);
                } else if timeout_start.is_none() {
                    timeout_start = Some(Instant::now());
                }
                last_gaps = Some(g);
            }

            if let Some(start) = timeout_start {
                if start.elapsed() >= timeout {
                    tracing::warn!(
                        "initial sync timed out before fetching network programs/blobs; continuing startup with partial view"
                    );
                    return Ok(());
                }
            }

            sleep(Duration::from_millis(500)).await;
        }
    }

    fn log_progress(gaps: (usize, usize, usize)) {
        let total_missing = gaps.0 + gaps.1 + gaps.2;
        let pct = if total_missing == 0 { 100 } else { 0 };
        tracing::info!(
            "sync progress: missing programs={}, blobs={}, executions={}, progress={}%",
            gaps.0,
            gaps.1,
            gaps.2,
            pct
        );
    }
}
