use super::health::HealthReporter;
use super::job::{FailureReason, Job, JobId, JobStatus, JobStore, LogLevel};
use super::job_executor::JobExecutor;
use super::job_resources::{JobResourceMetrics, ResourceTracker};
use anyhow::Result;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tokio::time::interval;

#[derive(Debug, Clone, Copy)]
pub enum RetryStrategy {
    ExponentialBackoff,
    LinearBackoff,
    FixedDelay,
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self::ExponentialBackoff
    }
}

pub struct JobScheduler {
    executor: Arc<JobExecutor>,
    job_store: Arc<JobStore>,
    pending_queue: Arc<RwLock<VecDeque<JobId>>>,
    max_concurrent: usize,
    running_count: Arc<RwLock<usize>>,
    retry_strategy: RetryStrategy,
    base_retry_backoff_ms: u64,
    health_reporter: Option<Arc<HealthReporter>>,
    resource_tracker: Option<Arc<ResourceTracker>>,
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
            retry_strategy: RetryStrategy::default(),
            base_retry_backoff_ms: 5000,
            health_reporter: None,
            resource_tracker: None,
        }
    }

    pub fn with_retry_strategy(mut self, strategy: RetryStrategy) -> Self {
        self.retry_strategy = strategy;
        self
    }

    pub fn with_base_backoff(mut self, backoff_ms: u64) -> Self {
        self.base_retry_backoff_ms = backoff_ms;
        self
    }

    pub fn set_health_reporter(&mut self, reporter: Arc<HealthReporter>) {
        self.health_reporter = Some(reporter);
    }

    pub fn set_resource_tracker(&mut self, tracker: Arc<ResourceTracker>) {
        self.resource_tracker = Some(tracker);
    }

    pub async fn start(self: Arc<Self>) {
        tracing::info!("job scheduler starting");

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

        self.check_ttl_expiration().await?;

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
                let health_reporter = self.health_reporter.clone();
                let resource_tracker = self.resource_tracker.clone();

                {
                    let mut count = running_count.write().await;
                    *count += 1;
                }

                tokio::spawn(async move {
                    let start_time = SystemTime::now();

                    let result = executor.execute_job(&job_id).await;

                    let duration_ms = start_time
                        .elapsed()
                        .unwrap_or(Duration::from_secs(0))
                        .as_millis() as u64;

                    if let Err(e) = result {
                        tracing::error!("job {job_id} execution error: {e:?}");
                        if let Ok(Some(mut job)) = job_store.get(&job_id) {
                            job.set_failure(
                                FailureReason::SchedulerError,
                                format!("Scheduler error: {e}"),
                            );
                            let _ = job_store.update(&job);

                            if let Some(ref reporter) = health_reporter {
                                reporter.record_job_failed();
                            }
                        }
                    } else if let Ok(Some(job)) = job_store.get(&job_id) {
                        if let Some(ref tracker) = resource_tracker {
                            let mut metrics = JobResourceMetrics::new(job_id);
                            metrics.fuel_consumed = job.fuel_consumed;
                            metrics.execution_duration_ms = duration_ms;
                            let _ = tracker.record_metrics(&metrics);
                        }

                        if let Some(ref reporter) = health_reporter {
                            match job.status {
                                JobStatus::Completed => {
                                    reporter.record_job_completed(job.fuel_consumed, duration_ms);
                                }
                                JobStatus::Cancelled => {
                                    reporter.record_job_cancelled();
                                }
                                JobStatus::Failed => {
                                    reporter.record_job_failed();
                                }
                                _ => {}
                            }
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
                let backoff = self.calculate_backoff(job.retry_count);
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

    fn calculate_backoff(&self, retry_count: u32) -> u64 {
        match self.retry_strategy {
            RetryStrategy::ExponentialBackoff => {
                self.base_retry_backoff_ms * (1u64 << (retry_count.min(10) - 1))
            }
            RetryStrategy::LinearBackoff => self.base_retry_backoff_ms * retry_count as u64,
            RetryStrategy::FixedDelay => self.base_retry_backoff_ms,
        }
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
                    job.failure_reason = Some(FailureReason::Timeout);
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

    async fn check_ttl_expiration(&self) -> Result<()> {
        let pending = self.job_store.list_pending()?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for mut job in pending {
            if job.is_expired(now) {
                job.status = JobStatus::Expired;
                job.completed_at = Some(now);
                job.failure_reason = Some(FailureReason::Timeout);
                job.error_message = Some(format!(
                    "Job expired (TTL: {}ms)",
                    job.ttl_ms.unwrap_or(0)
                ));
                job.add_log(LogLevel::Warn, "Job expired before execution".to_string());
                self.job_store.update(&job)?;
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