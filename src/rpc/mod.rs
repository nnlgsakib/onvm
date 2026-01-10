pub mod auth;
mod auth_store;
mod health;
mod jobs;

pub use health::{health_routes, HealthRpcContext};
pub use jobs::{job_routes, JobRpcContext};

use crate::config::RpcAuthConfig;
use crate::node::Node;
use crate::types::{BlobId, ProgramId};
use anyhow::Result;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::Method;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tower_http::cors::{Any, CorsLayer};
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
    pub data_dir: PathBuf,
    pub rpc_auth: RpcAuthConfig,
    pub auth_state: Option<Arc<crate::rpc::auth::AuthState>>,
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

#[derive(Serialize)]
struct CatalogEntryResponse {
    program_id: String,
    version: u64,
    initial_state_root: String,
    dag_parent: Option<String>,
    code_manifest: Option<String>,
    timestamp_ms: u64,
}

#[derive(Serialize)]
struct AggregatedReceiptResponse {
    program_id: String,
    state_root_in: String,
    state_root_out: String,
    write_digest: String,
    gas_used: u64,
    signer_bitmap: String,
    committee_epoch: u64,
    receipt_id: String,
}

#[derive(Serialize)]
struct CommitteeResponse {
    program_id: String,
    epoch: u64,
    aggregate_public_key: String,
    members: Vec<CommitteeMemberResponse>,
    threshold: u32,
}

#[derive(Serialize)]
struct CommitteeMemberResponse {
    node: String,
    weight: u64,
    bls_public_key: String,
}

#[derive(Serialize)]
struct StateRootResponse {
    program_id: String,
    root: String,
    proof: Option<Vec<String>>,
    value_hex: Option<String>,
}

#[derive(Deserialize)]
struct StateProofQuery {
    key: Option<String>,
}

#[derive(Deserialize)]
struct CreateProjectRequest {
    identity_passphrase: String,
    project_id: Option<String>,
    project_secret: Option<String>,
}

#[derive(Serialize)]
struct CreateProjectResponse {
    project_id: String,
    project_secret: String,
}

pub async fn start_rpc(
    node: Arc<Node>,
    addr: SocketAddr,
    rpc_auth: RpcAuthConfig,
    data_dir: PathBuf,
    identity_passphrase: Option<String>,
    enable_gateway: bool,
) -> Result<RpcServer> {
    let auth_state =
        auth::AuthState::from_config(&rpc_auth, &data_dir, identity_passphrase.as_deref())?
            .map(Arc::new);
    let ctx = RpcContext {
        node: node.clone(),
        data_dir: data_dir.clone(),
        rpc_auth: rpc_auth.clone(),
        auth_state: auth_state.clone(),
    };

    let db = node.db.clone();

    let job_store = Arc::new(crate::wasm_runtime::JobStore::new(db.clone())?);

    let health_reporter = Arc::new(crate::wasm_runtime::HealthReporter::new_with_db(Some(
        db.clone(),
    )));

    let mut job_executor = crate::wasm_runtime::JobExecutor::new(
        node.execution.clone(),
        job_store.clone(),
        node.unified_store.clone(),
        db.clone(),
    )?;
    job_executor.set_consensus(node.consensus.clone());

    let mut job_scheduler =
        crate::wasm_runtime::JobScheduler::new(Arc::new(job_executor), job_store.clone(), 10);
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
        dev_mode: auth_state.is_none(),
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
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .allow_credentials(false);

    let public = Router::new()
        .route("/rpc-auth/projects", post(create_project))
        .merge(health_routes().with_state(health_ctx.clone()));

    let protected = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/blobs", post(upload_blob))
        .route("/blobs/:id", get(fetch_blob))
        .route("/programs", post(deploy_program))
        .route("/programs/:id", get(program_info))
        .route("/programs/:id/receipts", get(program_receipts))
        .route("/programs/:id/committee", get(program_committee))
        .route("/programs/:id/state-root", get(program_state_root))
        .route("/program-catalog", get(list_program_catalog))
        .route("/execute", post(execute_program))
        .route("/estimate-fuel", post(estimate_fuel))
        .route("/fuel-profile/:program_id", get(get_fuel_profile))
        .merge(job_routes().with_state(job_ctx.clone()))
        .with_state(ctx.clone());

    let protected = if let Some(state) = auth_state.clone() {
        info!("RPC auth enabled for project {}", state.config.project_id);
        protected.layer(from_fn_with_state(state, auth::verify_signed_request))
    } else {
        info!("RPC auth disabled; requests are unauthenticated");
        protected
    };

    let app = Router::new()
        .merge(public)
        .merge(protected)
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE_BYTES))
        .layer(cors)
        .with_state(ctx);

    let app = if enable_gateway {
        app.nest(
            "/explorer",
            crate::blob_gateway::gateway_router(node.clone()),
        )
    } else {
        app
    };

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
            crate::types::ObjectType::blob_with_random_salt(mime),
            ctx.node.identity.node_id.clone(),
        )
        .map_err(internal_err)?;
    ctx.node
        .consensus
        .ingest_local_blob(object.clone(), data)
        .await
        .map_err(internal_err)?;
    Ok(Json(UploadBlobResponse {
        id: BlobId(object.id.0).to_prefixed_string(),
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
        id: ProgramId(object.id.0).to_prefixed_string(),
    }))
}

async fn program_info(
    State(ctx): State<RpcContext>,
    Path(id_hex): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let id = parse_program_id(&id_hex).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    let object_id = id.to_object_id();
    let object = ctx
        .node
        .unified_store
        .get_object_metadata(&object_id)
        .map_err(internal_err)?;
    match object {
        Some(obj) => match &obj.object_type {
            crate::types::ObjectType::WasmProgram {
                entrypoint,
                blob_refs,
                ..
            } => Ok(Json(serde_json::json!({
                "id": ProgramId(obj.id.0).to_prefixed_string(),
                "publisher": hex::encode(obj.publisher.0),
                "size": obj.total_size,
                "entrypoint": entrypoint,
                "blob_refs": blob_refs
                    .iter()
                    .map(|b| BlobId(b.0).to_prefixed_string())
                    .collect::<Vec<_>>(),
            }))),
            _ => Err((
                axum::http::StatusCode::BAD_REQUEST,
                "not a WASM program".to_string(),
            )),
        },
        None => Err((axum::http::StatusCode::NOT_FOUND, "not found".to_string())),
    }
}

async fn list_program_catalog(
    State(ctx): State<RpcContext>,
) -> Result<Json<Vec<CatalogEntryResponse>>, (axum::http::StatusCode, String)> {
    let manifests = ctx
        .node
        .program_catalog
        .list_manifests()
        .map_err(internal_err)?;
    let entries = manifests
        .into_iter()
        .map(|m| CatalogEntryResponse {
            program_id: m.program_id.to_prefixed_string(),
            version: m.version,
            initial_state_root: hex::encode(m.initial_state_root),
            dag_parent: m.dag_parent.map(hex::encode),
            code_manifest: m.code_manifest.map(|id| hex::encode(id.0)),
            timestamp_ms: m.timestamp_ms,
        })
        .collect();
    Ok(Json(entries))
}

async fn program_receipts(
    State(ctx): State<RpcContext>,
    Path(id_hex): Path<String>,
) -> Result<Json<Vec<AggregatedReceiptResponse>>, (axum::http::StatusCode, String)> {
    let program_id =
        parse_program_id(&id_hex).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    let receipts = ctx
        .node
        .program_catalog
        .receipts_for_program(&program_id)
        .map_err(internal_err)?;
    let mapped = receipts
        .into_iter()
        .map(|r| AggregatedReceiptResponse {
            program_id: r.receipt.program_id.to_prefixed_string(),
            state_root_in: hex::encode(r.receipt.state_root_in),
            state_root_out: hex::encode(r.receipt.state_root_out),
            write_digest: hex::encode(r.receipt.write_digest),
            gas_used: r.receipt.gas_used,
            signer_bitmap: hex::encode(r.signer_bitmap),
            committee_epoch: r.committee_epoch,
            receipt_id: hex::encode(r.receipt.id()),
        })
        .collect();
    Ok(Json(mapped))
}

async fn program_committee(
    State(ctx): State<RpcContext>,
    Path(id_hex): Path<String>,
) -> Result<Json<CommitteeResponse>, (axum::http::StatusCode, String)> {
    let program_id =
        parse_program_id(&id_hex).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;

    let manifest = ctx
        .node
        .program_catalog
        .get_manifest(&program_id)
        .map_err(internal_err)?
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                "program not found".to_string(),
            )
        })?;

    let committee = manifest.committee.ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            "committee not set for program".to_string(),
        )
    })?;

    let resp = CommitteeResponse {
        program_id: committee.program_id.to_prefixed_string(),
        epoch: committee.epoch,
        aggregate_public_key: hex::encode(committee.aggregate_public_key.0),
        threshold: committee.threshold,
        members: committee
            .members
            .into_iter()
            .map(|m| CommitteeMemberResponse {
                node: hex::encode(m.node.0),
                weight: m.weight,
                bls_public_key: hex::encode(m.bls_public_key.0),
            })
            .collect(),
    };
    Ok(Json(resp))
}

async fn program_state_root(
    State(ctx): State<RpcContext>,
    Path(id_hex): Path<String>,
    Query(query): Query<StateProofQuery>,
) -> Result<Json<StateRootResponse>, (axum::http::StatusCode, String)> {
    let program_id =
        parse_program_id(&id_hex).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    let root = ctx
        .node
        .state_store
        .sparse_root_scoped(&program_id.0)
        .map_err(internal_err)?;

    let (proof, value_hex) = if let Some(key_hex) = query.key {
        let key_bytes = hex::decode(&key_hex)
            .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
        let (computed_root, proof) = ctx
            .node
            .state_store
            .prove_scoped(&program_id.0, &key_bytes)
            .map_err(internal_err)?;
        let value = ctx
            .node
            .state_store
            .get_scoped(&program_id.0, &key_bytes)
            .map_err(internal_err)?;
        let proof_hex = proof.iter().map(|p| hex::encode(p)).collect();
        let value_hex = value.map(hex::encode);
        // ensure roots align
        if computed_root != root {
            return Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "state root mismatch during proof generation".to_string(),
            ));
        }
        (Some(proof_hex), value_hex)
    } else {
        (None, None)
    };

    Ok(Json(StateRootResponse {
        program_id: program_id.to_prefixed_string(),
        root: hex::encode(root),
        proof,
        value_hex,
    }))
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

    let _ = ctx
        .node
        .fuel_estimator
        .record_execution(
            &program_id,
            input.len(),
            result.fuel_consumed,
            execution_time_ms,
        )
        .await;

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

async fn create_project(
    State(ctx): State<RpcContext>,
    Json(req): Json<CreateProjectRequest>,
) -> Result<Json<CreateProjectResponse>, (StatusCode, String)> {
    if !ctx.rpc_auth.enable {
        return Err((StatusCode::BAD_REQUEST, "rpc auth disabled".into()));
    }
    let store = crate::rpc::auth::open_store_from_config(
        &ctx.rpc_auth,
        &ctx.data_dir,
        &req.identity_passphrase,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let project_id = match req.project_id {
        Some(id) => id,
        None => crate::rpc::auth::generate_project_secret()
            .map(|(id, _, _)| id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
    };
    let secret = if let Some(secret_hex) = req.project_secret {
        let bytes =
            hex::decode(secret_hex).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        if bytes.len() != 32 {
            return Err((
                StatusCode::BAD_REQUEST,
                "project_secret must be 32 bytes hex".into(),
            ));
        }
        let mut secret = [0u8; 32];
        secret.copy_from_slice(&bytes);
        store
            .upsert_project_secret(&project_id, secret)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        secret
    } else {
        store
            .ensure_project(&project_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };
    if let Some(auth_state) = &ctx.auth_state {
        auth_state
            .upsert_project(&project_id, secret)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    Ok(Json(CreateProjectResponse {
        project_id,
        project_secret: hex::encode(secret),
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
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                "no profile found".to_string(),
            )
        })?;

    Ok(Json(FuelProfileResponse {
        program_id: program_id.to_prefixed_string(),
        samples: profile
            .samples
            .iter()
            .map(|s| FuelSampleResponse {
                input_size: s.input_size,
                fuel_consumed: s.fuel_consumed,
                execution_time_ms: s.execution_time_ms,
                timestamp: s.timestamp,
            })
            .collect(),
        average_fuel_per_byte: profile.average_fuel_per_byte,
        base_fuel_cost: profile.base_fuel_cost,
    }))
}

fn parse_blob_id(hex_str: &str) -> Result<BlobId, String> {
    crate::types::parse_blob_id_str(hex_str)
}

fn parse_program_id(hex_str: &str) -> Result<ProgramId, String> {
    crate::types::parse_program_id_str(hex_str)
}

fn internal_err<E: std::fmt::Display>(err: E) -> (axum::http::StatusCode, String) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        err.to_string(),
    )
}
