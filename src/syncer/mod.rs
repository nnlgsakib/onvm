use crate::consensus::DagEngine;
pub mod job_sync;
pub mod sync;
use anyhow::Result;
use std::sync::Arc;
use tokio::time::{sleep, Duration, Instant};

pub use job_sync::{JobBroadcast, JobDescriptor, JobStatusDescriptor, JobSyncManager};

const INITIAL_SYNC_LOG_INTERVAL: Duration = Duration::from_secs(5);

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
        let mut last_log_at: Option<Instant> = None;
        loop {
            if self.dag.peer_count().await == 0 {
                timeout_start = None;
                last_gaps = None;
                last_log_at = None;
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
                let should_log = last_gaps.map(|prev| prev != g).unwrap_or(true)
                    || last_log_at
                        .map(|at| at.elapsed() >= INITIAL_SYNC_LOG_INTERVAL)
                        .unwrap_or(true);
                if should_log {
                    timeout_start = Some(Instant::now());
                    Self::log_progress(g);
                    last_log_at = Some(Instant::now());
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

    pub async fn run_status_logger(&self, interval: Duration) {
        let mut tick = tokio::time::interval(interval);
        loop {
            tick.tick().await;

            let peers = self.dag.peer_count().await;
            if peers == 0 {
                tracing::info!("sync status: no peers connected");
                continue;
            }

            if let Err(err) = self.dag.refresh_sync_state().await {
                tracing::debug!("sync status refresh failed: {err}");
            }

            if let Some(gaps) = self.dag.sync_gaps().await {
                Self::log_progress(gaps);
            } else {
                tracing::info!("sync status: waiting for inventory (peers={})", peers);
            }
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
        if total_missing == 0 {
            tracing::info!("sync status: synced");
        }
    }
}
