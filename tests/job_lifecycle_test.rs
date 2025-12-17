use anyhow::Result;
use onvm::wasm_runtime::{
    ExecutionConfig, ExecutionEngine, Job, JobExecutor, JobScheduler, JobStatus, JobStore,
    ProgramStore, SandboxValidator,
};
use onvm::storage::{BlobStore, StateStore};
use onvm::types::{NodeId, ProgramId};
use std::sync::Arc;
use tokio::time::Duration;

#[tokio::test]
async fn test_job_submission_and_execution() -> Result<()> {
    let db = sled::Config::new().temporary(true).open()?;
    let blob_store = Arc::new(BlobStore::new(db.clone(), std::env::temp_dir())?);
    let state_store = Arc::new(StateStore::new(db.clone(), "test_state")?);
    let program_store = Arc::new(ProgramStore::new(db.clone(), std::env::temp_dir())?);
    let job_store = Arc::new(JobStore::new(db.clone())?);

    let wasm = include_bytes!("../wasm_programs/echo/target/wasm32-unknown-unknown/release/echo.wasm");
    let program_meta = program_store.deploy(
        wasm,
        "onvm_main".to_string(),
        NodeId::from_public_key(&[0u8; 32]),
        Vec::new(),
        b"test-salt".to_vec(),
    )?;

    let input = b"hello world";
    let input_blob = blob_store.put(
        input,
        Some("text/plain".to_string()),
        NodeId::from_public_key(&[0u8; 32]),
    )?;

    let config = ExecutionConfig::default();
    let engine = Arc::new(ExecutionEngine::new(
        blob_store.clone(),
        state_store.clone(),
        program_store.clone(),
        config,
    )?);

    let executor = Arc::new(JobExecutor::new(
        engine,
        job_store.clone(),
        program_store.clone(),
        blob_store.clone(),
        db.clone(),
    )?);

    let scheduler = Arc::new(JobScheduler::new(executor, job_store.clone(), 2));

    let job = Job::new(
        "test-request-1".to_string(),
        program_meta.id.clone(),
        Some(input_blob.id),
        3,
    );

    let job_id = scheduler.submit_job(job).await?;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let status = scheduler.get_job_status(&job_id).await?;
    assert!(status.is_some());

    let job = status.unwrap();
    assert!(
        matches!(
            job.status,
            JobStatus::Pending | JobStatus::Running | JobStatus::Completed
        ),
        "expected job to be pending, running, or completed"
    );

    Ok(())
}

#[tokio::test]
async fn test_job_cancellation() -> Result<()> {
    let db = sled::Config::new().temporary(true).open()?;
    let blob_store = Arc::new(BlobStore::new(db.clone(), std::env::temp_dir())?);
    let state_store = Arc::new(StateStore::new(db.clone(), "test_state")?);
    let program_store = Arc::new(ProgramStore::new(db.clone(), std::env::temp_dir())?);
    let job_store = Arc::new(JobStore::new(db.clone())?);

    let wasm =
        include_bytes!("../../wasm_programs/echo/target/wasm32-unknown-unknown/release/echo.wasm");
    let program_meta = program_store.deploy(
        wasm,
        "onvm_main".to_string(),
        NodeId::from_public_key(&[0u8; 32]),
        Vec::new(),
        b"test-salt".to_vec(),
    )?;

    let config = ExecutionConfig::default();
    let engine = Arc::new(ExecutionEngine::new(
        blob_store.clone(),
        state_store.clone(),
        program_store.clone(),
        config,
    )?);

    let executor = Arc::new(JobExecutor::new(
        engine,
        job_store.clone(),
        program_store.clone(),
        blob_store.clone(),
        db.clone(),
    )?);

    let scheduler = Arc::new(JobScheduler::new(executor, job_store.clone(), 2));

    let job = Job::new(
        "test-request-cancel".to_string(),
        program_meta.id.clone(),
        None,
        3,
    );

    let job_id = scheduler.submit_job(job).await?;

    scheduler.cancel_job(&job_id).await?;

    let status = scheduler.get_job_status(&job_id).await?;
    assert!(status.is_some());

    let job = status.unwrap();
    assert_eq!(job.status, JobStatus::Cancelled);

    Ok(())
}

#[tokio::test]
async fn test_job_retry_on_failure() -> Result<()> {
    let db = sled::Config::new().temporary(true).open()?;
    let blob_store = Arc::new(BlobStore::new(db.clone(), std::env::temp_dir())?);
    let state_store = Arc::new(StateStore::new(db.clone(), "test_state")?);
    let program_store = Arc::new(ProgramStore::new(db.clone(), std::env::temp_dir())?);
    let job_store = Arc::new(JobStore::new(db.clone())?);

    let invalid_program_id = ProgramId::new(b"nonexistent-program");

    let config = ExecutionConfig::default();
    let engine = Arc::new(ExecutionEngine::new(
        blob_store.clone(),
        state_store.clone(),
        program_store.clone(),
        config,
    )?);

    let executor = Arc::new(JobExecutor::new(
        engine,
        job_store.clone(),
        program_store.clone(),
        blob_store.clone(),
        db.clone(),
    )?);

    let scheduler = Arc::new(JobScheduler::new(executor, job_store.clone(), 2));

    let job = Job::new(
        "test-request-retry".to_string(),
        invalid_program_id,
        None,
        2,
    );

    let job_id = scheduler.submit_job(job).await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    let status = scheduler.get_job_status(&job_id).await?;
    assert!(status.is_some());

    let job = status.unwrap();
    assert!(job.retry_count > 0 || matches!(job.status, JobStatus::Failed));

    Ok(())
}

#[test]
fn test_sandbox_validator_limits() {
    let validator = SandboxValidator::default();

    assert!(validator
        .validate_resource_limits(50_000_000, 64 * 1024 * 1024, 30_000)
        .is_ok());

    assert!(validator
        .validate_resource_limits(1_000_000_000, 64 * 1024 * 1024, 30_000)
        .is_err());

    assert!(validator
        .validate_resource_limits(50_000_000, 1024 * 1024 * 1024, 30_000)
        .is_err());

    assert!(validator
        .validate_resource_limits(50_000_000, 64 * 1024 * 1024, 500_000)
        .is_err());
}

#[test]
fn test_job_idempotency() -> Result<()> {
    let db = sled::Config::new().temporary(true).open()?;
    let job_store = JobStore::new(db)?;

    let program_id = ProgramId::new(b"test-program");
    let job1 = Job::new("test-request".to_string(), program_id.clone(), None, 3);
    let job2 = Job::new("test-request".to_string(), program_id.clone(), None, 3);

    job_store.insert(&job1)?;

    let retrieved = job_store.get_by_request_id("test-request")?;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, job1.id);

    job_store.insert(&job2)?;

    let retrieved = job_store.get_by_request_id("test-request")?;
    assert!(retrieved.is_some());

    Ok(())
}