use crate::node::Node;
use crate::types::{BlobId, ProgramId};
use anyhow::Result;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::info;

pub struct RpcServer {
    #[allow(dead_code)]
    handle: JoinHandle<()>,
    pub bound: SocketAddr,
}

#[derive(Clone)]
pub struct RpcContext {
    pub node: Arc<Node>,
}

#[derive(Deserialize)]
struct UploadBlobRequest {
    data_base64: String,
    mime: Option<String>,
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
    salt_base64: Option<String>,
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

pub async fn start_rpc(node: Arc<Node>, addr: SocketAddr) -> Result<RpcServer> {
    let ctx = RpcContext { node };
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/blobs", post(upload_blob))
        .route("/blobs/:id", get(fetch_blob))
        .route("/programs", post(deploy_program))
        .route("/programs/:id", get(program_info))
        .route("/execute", post(execute_program))
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
    Json(req): Json<UploadBlobRequest>,
) -> Result<Json<UploadBlobResponse>, (axum::http::StatusCode, String)> {
    let data = match general_purpose::STANDARD.decode(req.data_base64) {
        Ok(d) => d,
        Err(e) => return Err((axum::http::StatusCode::BAD_REQUEST, e.to_string())),
    };
    let meta = ctx
        .node
        .blob_store
        .put(&data, req.mime, ctx.node.identity.node_id.clone())
        .map_err(internal_err)?;
    ctx.node
        .consensus
        .ingest_local_blob(meta.clone(), data)
        .await
        .map_err(internal_err)?;
    Ok(Json(UploadBlobResponse {
        id: hex::encode(meta.id.0),
        size: meta.size,
    }))
}

async fn fetch_blob(
    State(ctx): State<RpcContext>,
    Path(id_hex): Path<String>,
) -> Result<Vec<u8>, (axum::http::StatusCode, String)> {
    let id = parse_blob_id(&id_hex).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    ctx.node
        .blob_store
        .get(&id)
        .map_err(|e| (axum::http::StatusCode::NOT_FOUND, e.to_string()))
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
        .map(|h| parse_blob_id(&h))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    let deploy_salt = if let Some(s) = req.salt_base64 {
        general_purpose::STANDARD
            .decode(s)
            .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?
    } else {
        uuid::Uuid::new_v4().as_bytes().to_vec()
    };

    let meta = ctx
        .node
        .program_store
        .deploy(
            &wasm,
            req.entrypoint,
            ctx.node.identity.node_id.clone(),
            blob_refs,
            deploy_salt,
        )
        .map_err(internal_err)?;
    ctx.node
        .consensus
        .ingest_local_program(meta.clone(), wasm)
        .await
        .map_err(internal_err)?;
    Ok(Json(DeployProgramResponse {
        id: hex::encode(meta.id.0),
    }))
}

async fn program_info(
    State(ctx): State<RpcContext>,
    Path(id_hex): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let id = parse_program_id(&id_hex).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    let meta = ctx.node.program_store.metadata(&id).map_err(internal_err)?;
    match meta {
        Some(m) => Ok(Json(serde_json::json!({
            "id": hex::encode(m.id.0),
            "publisher": hex::encode(m.publisher.0),
            "size": m.size,
            "entrypoint": m.entrypoint,
            "blob_refs": m.blob_refs.iter().map(|b| hex::encode(b.0)).collect::<Vec<_>>(),
        }))),
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
    let result = ctx
        .node
        .consensus
        .submit_execution(&program_id, &input)
        .await
        .map_err(internal_err)?;
    Ok(Json(ExecuteResponse {
        return_base64: general_purpose::STANDARD.encode(result.return_data),
        fuel: result.fuel_consumed,
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
