use crate::types::{BlobId, ProgramId};
use crate::wasm_runtime::{Job, JobId, JobScheduler, JobStatus, JobStore};
use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub fn job_routes() -> Router<Arc<JobRpcContext>> {
    Router::new()
        .route("/jobs", post(submit_job))
        .route("/jobs/:id", get(get_job_status))
        .route("/jobs/:id/cancel", post(cancel_job))
        .route("/jobs/:id/logs", get(get_job_logs))
        .route("/jobs/:id/output", get(get_job_output))
        .route("/jobs", get(list_jobs))
}

#[derive(Clone)]
pub struct JobRpcContext {
    pub scheduler: Arc<JobScheduler>,
    pub job_store: Arc<JobStore>,
    pub unified_store: Arc<crate::storage::UnifiedStore>,
    pub consensus: Arc<crate::consensus::DagEngine>,
}

#[derive(Debug, Deserialize)]
struct SubmitJobRequest {
    request_id: String,
    program_id: String,
    #[serde(default)]
    input_base64: Option<String>,
    input_blob_id: Option<String>,
    #[serde(default)]
    max_retries: Option<u32>,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct SubmitJobResponse {
    job_id: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct JobStatusResponse {
    job_id: String,
    request_id: String,
    program_id: String,
    status: String,
    fuel_consumed: u64,
    created_at: u64,
    started_at: Option<u64>,
    completed_at: Option<u64>,
    duration_ms: Option<u64>,
    retry_count: u32,
    error_message: Option<String>,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct LogsResponse {
    logs: Vec<LogEntryResponse>,
}

#[derive(Debug, Serialize)]
struct LogEntryResponse {
    timestamp: u64,
    level: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct OutputResponse {
    job_id: String,
    output_base64: String,
    size: usize,
}

#[derive(Debug, Serialize)]
struct JobListResponse {
    jobs: Vec<JobSummary>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct JobSummary {
    job_id: String,
    request_id: String,
    program_id: String,
    status: String,
    created_at: u64,
    completed_at: Option<u64>,
}

async fn submit_job(
    State(ctx): State<Arc<JobRpcContext>>,
    Json(req): Json<SubmitJobRequest>,
) -> Result<Json<SubmitJobResponse>, (StatusCode, String)> {
    let span = tracing::info_span!("submit_job", request_id = %req.request_id);
    let _guard = span.enter();

    let program_id = parse_program_id(&req.program_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid program_id: {e}")))?;

    let input_blob_id = if let Some(blob_hex) = req.input_blob_id {
        Some(parse_blob_id(&blob_hex).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid input_blob_id: {e}"),
            )
        })?)
    } else if let Some(base64_input) = req.input_base64 {
        let data = general_purpose::STANDARD
            .decode(base64_input)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid base64: {e}")))?;

        let object = ctx
            .unified_store
            .put_object(
                &data,
                crate::types::ObjectType::blob_with_random_salt(Some(
                    "application/octet-stream".to_string(),
                )),
                crate::types::NodeId::from_public_key(&[0u8; 32]),
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        ctx.consensus
            .ingest_local_blob(object.clone(), data)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Some(BlobId(object.id.0))
    } else {
        None
    };

    let max_retries = req.max_retries.unwrap_or(3);

    let mut job = Job::new(req.request_id, program_id, input_blob_id, max_retries);
    job.metadata = req.metadata;

    let job_id = ctx
        .scheduler
        .submit_job(job)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(SubmitJobResponse {
        job_id: job_id.to_string(),
        status: "pending".to_string(),
    }))
}

async fn get_job_status(
    State(ctx): State<Arc<JobRpcContext>>,
    Path(job_id_hex): Path<String>,
) -> Result<Json<JobStatusResponse>, (StatusCode, String)> {
    let job_id = parse_job_id(&job_id_hex)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid job_id: {e}")))?;

    let job = ctx
        .scheduler
        .get_job_status(&job_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "job not found".to_string()))?;

    Ok(Json(JobStatusResponse {
        job_id: job.id.to_string(),
        request_id: job.request_id.clone(),
        program_id: job.program_id.to_string(),
        status: job_status_to_string(&job.status),
        fuel_consumed: job.fuel_consumed,
        created_at: job.created_at,
        started_at: job.started_at,
        completed_at: job.completed_at,
        duration_ms: job.duration_ms(),
        retry_count: job.retry_count,
        error_message: job.error_message.clone(),
        metadata: job.metadata.clone(),
    }))
}

async fn cancel_job(
    State(ctx): State<Arc<JobRpcContext>>,
    Path(job_id_hex): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let job_id = parse_job_id(&job_id_hex)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid job_id: {e}")))?;

    ctx.scheduler
        .cancel_job(&job_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::OK)
}

async fn get_job_logs(
    State(ctx): State<Arc<JobRpcContext>>,
    Path(job_id_hex): Path<String>,
) -> Result<Json<LogsResponse>, (StatusCode, String)> {
    let job_id = parse_job_id(&job_id_hex)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid job_id: {e}")))?;

    let job = ctx
        .job_store
        .get(&job_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "job not found".to_string()))?;

    let logs = job
        .logs
        .into_iter()
        .map(|log| LogEntryResponse {
            timestamp: log.timestamp,
            level: log_level_to_string(&log.level),
            message: log.message,
        })
        .collect();

    Ok(Json(LogsResponse { logs }))
}

async fn get_job_output(
    State(ctx): State<Arc<JobRpcContext>>,
    Path(job_id_hex): Path<String>,
) -> Result<Json<OutputResponse>, (StatusCode, String)> {
    let span = tracing::debug_span!("get_job_output", job_id = %job_id_hex);
    let _guard = span.enter();

    let job_id = parse_job_id(&job_id_hex)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid job_id: {e}")))?;

    let job = ctx
        .job_store
        .get(&job_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "job not found".to_string()))?;

    let output_blob_id = job.output_blob_id.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "job output not available".to_string(),
        )
    })?;

    let object_id = output_blob_id.to_object_id();
    let data = match ctx.unified_store.get_object(&object_id) {
        Ok(data) => data,
        Err(_) => ctx
            .consensus
            .fetch_blob(&output_blob_id)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?,
    };

    Ok(Json(OutputResponse {
        job_id: job.id.to_string(),
        output_base64: general_purpose::STANDARD.encode(&data),
        size: data.len(),
    }))
}

async fn list_jobs(
    State(ctx): State<Arc<JobRpcContext>>,
) -> Result<Json<JobListResponse>, (StatusCode, String)> {
    let jobs = ctx
        .job_store
        .list_all()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total = jobs.len();
    let summaries = jobs
        .into_iter()
        .map(|job| JobSummary {
            job_id: job.id.to_string(),
            request_id: job.request_id,
            program_id: job.program_id.to_string(),
            status: job_status_to_string(&job.status),
            created_at: job.created_at,
            completed_at: job.completed_at,
        })
        .collect();

    Ok(Json(JobListResponse {
        jobs: summaries,
        total,
    }))
}

fn parse_job_id(hex_str: &str) -> Result<JobId, String> {
    let bytes = hex::decode(hex_str).map_err(|e| e.to_string())?;
    if bytes.len() != 32 {
        return Err("invalid job id length".into());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(JobId::from_bytes(arr))
}

fn parse_program_id(hex_str: &str) -> Result<ProgramId, String> {
    let bytes = hex::decode(hex_str).map_err(|e| e.to_string())?;
    if bytes.len() != 32 {
        return Err("invalid program id length".into());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(ProgramId(arr))
}

fn parse_blob_id(hex_str: &str) -> Result<BlobId, String> {
    let bytes = hex::decode(hex_str).map_err(|e| e.to_string())?;
    if bytes.len() != 32 {
        return Err("invalid blob id length".into());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(BlobId(arr))
}

fn job_status_to_string(status: &JobStatus) -> String {
    match status {
        JobStatus::Pending => "pending".to_string(),
        JobStatus::Running => "running".to_string(),
        JobStatus::Completed => "completed".to_string(),
        JobStatus::Failed => "failed".to_string(),
        JobStatus::Cancelled => "cancelled".to_string(),
        JobStatus::TimedOut => "timed_out".to_string(),
        JobStatus::Expired => "expired".to_string(),
    }
}

fn log_level_to_string(level: &crate::wasm_runtime::LogLevel) -> String {
    match level {
        crate::wasm_runtime::LogLevel::Debug => "debug".to_string(),
        crate::wasm_runtime::LogLevel::Info => "info".to_string(),
        crate::wasm_runtime::LogLevel::Warn => "warn".to_string(),
        crate::wasm_runtime::LogLevel::Error => "error".to_string(),
    }
}
