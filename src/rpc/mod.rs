mod health;
mod jobs;

pub use health::{health_routes, HealthRpcContext};
pub use jobs::{job_routes, JobRpcContext};

use crate::node::Node;
use crate::types::{BlobId, ProgramId};
use anyhow::Result;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum::http::Method;
use tower_http::cors::{CorsLayer, Any};
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::info;

// Permit large blob uploads; adjust if hosting constraints change.
const MAX_UPLOAD_SIZE_BYTES: usize = 1024 * 1024 * 1024; // 1 GiB

pub struct RpcServer {
    #[allow(dead_code)]
    handle: JoinHandle<()>,
    pub bound: SocketAddr,
}

#[derive(Clone)]
pub struct RpcContext {
    pub node: Arc<Node>,
}

#[derive(Serialize)]
struct UploadBlobResponse {
    id: String,
    size: u64,
}

#[derive(Deserialize)]
struct DeployProgramRequest {
    wasm_base64: String,
    entrypoint: String,
    blob_refs: Vec<String>,
}

#[derive(Serialize)]
struct DeployProgramResponse {
    id: String,
}

#[derive(Deserialize)]
struct ExecuteRequest {
    program_id: String,
    input_base64: String,
}

#[derive(Serialize)]
struct ExecuteResponse {
    return_base64: String,
    fuel: u64,
}

#[derive(Deserialize)]
struct EstimateFuelRequest {
    program_id: String,
    input_base64: String,
}

#[derive(Serialize)]
struct EstimateFuelResponse {
    estimated_fuel: u64,
    confidence: f64,
    based_on_samples: usize,
    input_size_bytes: usize,
    estimated_execution_time_ms: u64,
}

#[derive(Serialize)]
struct FuelProfileResponse {
    program_id: String,
    samples: Vec<FuelSampleResponse>,
    average_fuel_per_byte: f64,
    base_fuel_cost: u64,
}

#[derive(Serialize)]
struct FuelSampleResponse {
    input_size: usize,
    fuel_consumed: u64,
    execution_time_ms: u64,
    timestamp: u64,
}

pub async fn start_rpc(node: Arc<Node>, addr: SocketAddr) -> Result<RpcServer> {
    let ctx = RpcContext { node: node.clone() };

    let db = node.db.clone();

    let job_store = Arc::new(crate::wasm_runtime::JobStore::new(db.clone())?);

    let health_reporter = Arc::new(crate::wasm_runtime::HealthReporter::new_with_db(Some(db.clone())));

    let mut job_executor = crate::wasm_runtime::JobExecutor::new(
        node.execution.clone(),
        job_store.clone(),
        node.unified_store.clone(),
        db.clone(),
    )?;
    job_executor.set_consensus(node.consensus.clone());

    let mut job_scheduler = crate::wasm_runtime::JobScheduler::new(
        Arc::new(job_executor),
        job_store.clone(),
        10,
    );
    job_scheduler.set_health_reporter(health_reporter.clone());

    let job_ctx = Arc::new(JobRpcContext {
        scheduler: Arc::new(job_scheduler),
        job_store: job_store.clone(),
        unified_store: node.unified_store.clone(),
        consensus: node.consensus.clone(),
    });

    let health_ctx = Arc::new(HealthRpcContext {
        reporter: health_reporter,
        scheduler: Arc::downgrade(&job_ctx.scheduler),
        max_concurrent: 10,
    });

    let scheduler_handle = job_ctx.scheduler.clone();
    tokio::spawn(async move {
        scheduler_handle.start().await;
    });

    let job_sync_manager = Arc::new(crate::syncer::JobSyncManager::new(
        job_store,
        node.network.clone(),
    ));

    node.consensus.set_job_sync(job_sync_manager.clone()).await;

    let job_sync_handle = job_sync_manager.clone();
    tokio::spawn(async move {
        job_sync_handle.start().await;
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any)
        .allow_credentials(false);

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/blobs", post(upload_blob))
        .route("/blobs/:id", get(fetch_blob))
        .route("/programs", post(deploy_program))
        .route("/programs/:id", get(program_info))
        .route("/execute", post(execute_program))
        .route("/estimate-fuel", post(estimate_fuel))
        .route("/fuel-profile/:program_id", get(get_fuel_profile))
        .merge(job_routes().with_state(job_ctx))
        .merge(health_routes().with_state(health_ctx))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE_BYTES))
        .layer(cors)
        .with_state(ctx);

    let mut port = addr.port();
    let ip = addr.ip();
    let listener = loop {
        match tokio::net::TcpListener::bind(SocketAddr::new(ip, port)).await {
            Ok(l) => break l,
            Err(e) => {
                if port == u16::MAX {
                    return Err(e.into());
                }
                port += 1;
            }
        }
    };
    let bound = listener
        .local_addr()
        .unwrap_or_else(|_| SocketAddr::new(ip, port));
    info!("RPC listening on {}", bound);
    let handle = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, app).await {
            eprintln!("rpc server error: {err:?}");
        }
    });
    Ok(RpcServer { handle, bound })
}

async fn upload_blob(
    State(ctx): State<RpcContext>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<UploadBlobResponse>, (StatusCode, String)> {
    let mime = headers
        .get("x-mime")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        });
    let data = body.to_vec();
    let object = ctx
        .node
        .unified_store
        .put_object(
            &data,
            crate::types::ObjectType::Blob { mime_type: mime },
            ctx.node.identity.node_id.clone(),
        )
        .map_err(internal_err)?;
    ctx.node
        .consensus
        .ingest_local_blob(object.clone(), data)
        .await
        .map_err(internal_err)?;
    Ok(Json(UploadBlobResponse {
        id: hex::encode(object.id.0),
        size: object.total_size,
    }))
}

async fn fetch_blob(
    State(ctx): State<RpcContext>,
    Path(id_hex): Path<String>,
) -> Result<Vec<u8>, (axum::http::StatusCode, String)> {
    let id = parse_blob_id(&id_hex).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    let object_id = id.to_object_id();
    match ctx.node.unified_store.get_object(&object_id) {
        Ok(data) => Ok(data),
        Err(_) => ctx
            .node
            .consensus
            .fetch_blob(&id)
            .await
            .map_err(|e| (axum::http::StatusCode::NOT_FOUND, e.to_string())),
    }
}

async fn deploy_program(
    State(ctx): State<RpcContext>,
    Json(req): Json<DeployProgramRequest>,
) -> Result<Json<DeployProgramResponse>, (axum::http::StatusCode, String)> {
    let wasm = match general_purpose::STANDARD.decode(req.wasm_base64) {
        Ok(d) => d,
        Err(e) => return Err((axum::http::StatusCode::BAD_REQUEST, e.to_string())),
    };
    let blob_refs = req
        .blob_refs
        .into_iter()
        .map(|h| {
            let id = parse_blob_id(&h)?;
            Ok(id.to_object_id())
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: String| (axum::http::StatusCode::BAD_REQUEST, e))?;

    let deploy_salt = {
        use std::time::SystemTime;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut hasher = blake3::Hasher::new();
        hasher.update(&timestamp.to_le_bytes());
        hasher.update(&ctx.node.identity.node_id.0);
        hasher.update(&wasm);
        hasher.finalize().as_bytes().to_vec()
    };

    let object = ctx
        .node
        .unified_store
        .put_object(
            &wasm,
            crate::types::ObjectType::WasmProgram {
                entrypoint: req.entrypoint,
                wasm_version: Some("1.0".to_string()),
                source_language: None,
                compiler: None,
                blob_refs,
                deploy_salt,
            },
            ctx.node.identity.node_id.clone(),
        )
        .map_err(internal_err)?;
    ctx.node
        .consensus
        .ingest_local_program(object.clone(), wasm)
        .await
        .map_err(internal_err)?;
    Ok(Json(DeployProgramResponse {
        id: hex::encode(object.id.0),
    }))
}

async fn program_info(
    State(ctx): State<RpcContext>,
    Path(id_hex): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let id = parse_program_id(&id_hex).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    let object_id = id.to_object_id();
    let object = ctx.node.unified_store.get_object_metadata(&object_id).map_err(internal_err)?;
    match object {
        Some(obj) => {
            match &obj.object_type {
                crate::types::ObjectType::WasmProgram { entrypoint, blob_refs, .. } => {
                    Ok(Json(serde_json::json!({
                        "id": hex::encode(obj.id.0),
                        "publisher": hex::encode(obj.publisher.0),
                        "size": obj.total_size,
                        "entrypoint": entrypoint,
                        "blob_refs": blob_refs.iter().map(|b| hex::encode(b.0)).collect::<Vec<_>>(),
                    })))
                }
                _ => Err((axum::http::StatusCode::BAD_REQUEST, "not a WASM program".to_string())),
            }
        }
        None => Err((axum::http::StatusCode::NOT_FOUND, "not found".to_string())),
    }
}

async fn execute_program(
    State(ctx): State<RpcContext>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, (axum::http::StatusCode, String)> {
    let program_id =
        parse_program_id(&req.program_id).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    let input = general_purpose::STANDARD
        .decode(req.input_base64)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    
    let start_time = std::time::Instant::now();
    let result = ctx
        .node
        .consensus
        .submit_execution(&program_id, &input)
        .await
        .map_err(internal_err)?;
    let execution_time_ms = start_time.elapsed().as_millis() as u64;
    
    let _ = ctx.node.fuel_estimator.record_execution(
        &program_id,
        input.len(),
        result.fuel_consumed,
        execution_time_ms,
    ).await;
    
    Ok(Json(ExecuteResponse {
        return_base64: general_purpose::STANDARD.encode(result.return_data),
        fuel: result.fuel_consumed,
    }))
}

async fn estimate_fuel(
    State(ctx): State<RpcContext>,
    Json(req): Json<EstimateFuelRequest>,
) -> Result<Json<EstimateFuelResponse>, (axum::http::StatusCode, String)> {
    let program_id =
        parse_program_id(&req.program_id).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    let input = general_purpose::STANDARD
        .decode(req.input_base64)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    
    let estimate = ctx
        .node
        .fuel_estimator
        .estimate_fuel(&program_id, &input)
        .await
        .map_err(internal_err)?;
    
    Ok(Json(EstimateFuelResponse {
        estimated_fuel: estimate.estimated_fuel,
        confidence: estimate.confidence,
        based_on_samples: estimate.based_on_samples,
        input_size_bytes: estimate.input_size_bytes,
        estimated_execution_time_ms: estimate.estimated_execution_time_ms,
    }))
}

async fn get_fuel_profile(
    State(ctx): State<RpcContext>,
    Path(program_id_hex): Path<String>,
) -> Result<Json<FuelProfileResponse>, (axum::http::StatusCode, String)> {
    let program_id =
        parse_program_id(&program_id_hex).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    
    let profile = ctx
        .node
        .fuel_estimator
        .get_profile(&program_id)
        .await
        .ok_or_else(|| (axum::http::StatusCode::NOT_FOUND, "no profile found".to_string()))?;
    
    Ok(Json(FuelProfileResponse {
        program_id: program_id_hex,
        samples: profile.samples.iter().map(|s| FuelSampleResponse {
            input_size: s.input_size,
            fuel_consumed: s.fuel_consumed,
            execution_time_ms: s.execution_time_ms,
            timestamp: s.timestamp,
        }).collect(),
        average_fuel_per_byte: profile.average_fuel_per_byte,
        base_fuel_cost: profile.base_fuel_cost,
    }))
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

fn parse_program_id(hex_str: &str) -> Result<ProgramId, String> {
    let bytes = hex::decode(hex_str).map_err(|e| e.to_string())?;
    if bytes.len() != 32 {
        return Err("invalid program id length".into());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(ProgramId(arr))
}

fn internal_err<E: std::fmt::Display>(err: E) -> (axum::http::StatusCode, String) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        err.to_string(),
    )
}