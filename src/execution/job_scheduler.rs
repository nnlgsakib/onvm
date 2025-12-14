use super::job::{Job, JobId, JobStatus, JobStore, LogLevel};
use super::job_executor::JobExecutor;
use anyhow::Result;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tokio::time::interval;

pub struct JobScheduler {
    executor: Arc<JobExecutor>,
    job_store: Arc<JobStore>,
    pending_queue: Arc<RwLock<VecDeque<JobId>>>,
    max_concurrent: usize,
    running_count: Arc<RwLock<usize>>,
    retry_backoff_ms: u64,
}

impl JobScheduler {
    pub fn new(
        executor: Arc<JobExecutor>,
        job_store: Arc<JobStore>,
        max_concurrent: usize,
    ) -> Self {
        Self {
            executor,
            job_store,
            pending_queue: Arc::new(RwLock::new(VecDeque::new())),
            max_concurrent,
            running_count: Arc::new(RwLock::new(0)),
            retry_backoff_ms: 5000,
        }
    }

    pub async fn start(self: Arc<Self>) {
        let mut tick = interval(Duration::from_millis(1000));
        loop {
            tick.tick().await;
            if let Err(e) = self.schedule_cycle().await {
                tracing::warn!("scheduler cycle error: {e:?}");
            }
        }
    }

    async fn schedule_cycle(&self) -> Result<()> {
        self.enqueue_pending_jobs().await?;

        self.check_timeouts().await?;

        let running = *self.running_count.read().await;
        let available_slots = self.max_concurrent.saturating_sub(running);

        for _ in 0..available_slots {
            let job_id = {
                let mut queue = self.pending_queue.write().await;
                queue.pop_front()
            };

            if let Some(job_id) = job_id {
                let executor = Arc::clone(&self.executor);
                let running_count = Arc::clone(&self.running_count);
                let job_store = Arc::clone(&self.job_store);

                {
                    let mut count = running_count.write().await;
                    *count += 1;
                }

                tokio::spawn(async move {
                    let result = executor.execute_job(&job_id).await;

                    if let Err(e) = result {
                        tracing::error!("job {job_id} execution error: {e:?}");
                        if let Ok(Some(mut job)) = job_store.get(&job_id) {
                            job.status = JobStatus::Failed;
                            job.error_message = Some(e.to_string());
                            job.add_log(LogLevel::Error, format!("Scheduler error: {e}"));
                            let _ = job_store.update(&job);
                        }
                    }

                    let mut count = running_count.write().await;
                    *count = count.saturating_sub(1);
                });
            } else {
                break;
            }
        }

        Ok(())
    }

    async fn enqueue_pending_jobs(&self) -> Result<()> {
        let pending = self.job_store.list_pending()?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for job in pending {
            if job.retry_count > 0 {
                let backoff = self.retry_backoff_ms * (1u64 << (job.retry_count.min(5) - 1));
                let retry_after = job.created_at + backoff;
                if now < retry_after {
                    continue;
                }
            }

            let mut queue = self.pending_queue.write().await;
            if !queue.contains(&job.id) {
                queue.push_back(job.id);
            }
        }

        Ok(())
    }

    async fn check_timeouts(&self) -> Result<()> {
        let running = self.job_store.list_running()?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for mut job in running {
            if let Some(started_at) = job.started_at {
                let elapsed = now.saturating_sub(started_at);
                if elapsed > 300_000 {
                    job.status = JobStatus::TimedOut;
                    job.completed_at = Some(now);
                    job.error_message = Some("Job exceeded maximum runtime".to_string());
                    job.add_log(
                        LogLevel::Error,
                        "Job force-timed out by scheduler".to_string(),
                    );
                    self.job_store.update(&job)?;
                }
            }
        }

        Ok(())
    }

    pub async fn submit_job(&self, job: Job) -> Result<JobId> {
        let job_id = job.id;
        self.job_store.insert(&job)?;

        let mut queue = self.pending_queue.write().await;
        queue.push_back(job_id);

        Ok(job_id)
    }

    pub async fn get_job_status(&self, job_id: &JobId) -> Result<Option<Job>> {
        self.job_store.get(job_id)
    }

    pub async fn cancel_job(&self, job_id: &JobId) -> Result<()> {
        self.executor.cancel_job(job_id).await?;

        let mut queue = self.pending_queue.write().await;
        queue.retain(|id| id != job_id);

        Ok(())
    }

    pub async fn queue_depth(&self) -> usize {
        self.pending_queue.read().await.len()
    }

    pub async fn running_count(&self) -> usize {
        *self.running_count.read().await
    }
}
