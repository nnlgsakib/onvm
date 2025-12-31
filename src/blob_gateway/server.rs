use crate::node::Node;
use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

use super::content_detector::ContentDetector;
use super::fetcher::BlobFetcher;
use super::ui_embed::UiAssets;

const SIZE_THRESHOLD_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Clone)]
pub struct GatewayConfig {
    pub listen_addr: String,
}

#[derive(Clone)]
struct GatewayContext {
    node: Arc<Node>,
    detector: ContentDetector,
}

#[derive(Deserialize)]
struct BlobQuery {
    #[serde(default)]
    download: bool,
}

pub async fn start_gateway(node: Arc<Node>, config: GatewayConfig) -> Result<()> {
    let ctx = GatewayContext {
        node,
        detector: ContentDetector::new(),
    };

    let app = Router::new()
        .route("/api/blob/:id", get(get_blob_handler))
        .route("/api/blob/:id/info", get(get_blob_info))
        .route("/api/health", get(health_check))
        .route("/cdn/:id", get(cdn_serve_handler))
        .route("/cdn/:id/*path", get(cdn_serve_handler))
        .fallback(serve_ui)
        .layer(CorsLayer::permissive())
        .with_state(ctx);

    let listener = TcpListener::bind(&config.listen_addr).await?;
    info!("Blob Gateway listening on {}", config.listen_addr);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn serve_ui(uri: Uri) -> Result<Response, (StatusCode, String)> {
    let path = uri.path().trim_start_matches('/');

    let path = if path.is_empty() || path.starts_with('#') {
        "index.html"
    } else if UiAssets::get(path).is_some() {
        path
    } else {
        "index.html"
    };

    match UiAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Ok(([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response())
        }
        None => Err((StatusCode::NOT_FOUND, "Not found".to_string())),
    }
}



async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "Gateway healthy")
}

async fn get_blob_info(
    State(ctx): State<GatewayContext>,
    Path(id_hex): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let blob_id = parse_id(&id_hex)?;

    let fetcher = BlobFetcher::new(ctx.node.clone());
    let info = fetcher
        .get_blob_info(&blob_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(info))
}

async fn get_blob_handler(
    State(ctx): State<GatewayContext>,
    Path(id_hex): Path<String>,
    Query(query): Query<BlobQuery>,
) -> Result<Response, (StatusCode, String)> {
    let blob_id = parse_id(&id_hex)?;

    if is_program_id(&blob_id, &ctx).await {
        return Err((
            StatusCode::BAD_REQUEST,
            "ID appears to be a program, not a blob. Gateway only serves blob data.".to_string(),
        ));
    }

    let fetcher = BlobFetcher::new(ctx.node.clone());

    match fetcher.fetch_blob(&blob_id).await {
        Ok(data) => {
            let size = data.len() as u64;
            let content_type = ctx.detector.detect(&data);
            let extension = ctx.detector.get_extension(&content_type);

            if query.download || size > SIZE_THRESHOLD_BYTES || !is_renderable(&content_type) {
                let filename = format!("{}.{}", &id_hex[..12], extension);
                Ok((
                    [
                        (header::CONTENT_TYPE, content_type.as_str()),
                        (
                            header::CONTENT_DISPOSITION,
                            &format!("attachment; filename=\"{}\"", filename),
                        ),
                    ],
                    data,
                )
                    .into_response())
            } else {
                Ok(([(header::CONTENT_TYPE, content_type.as_str())], data).into_response())
            }
        }
        Err(e) if e.to_string().contains("not found") => {
            match fetcher.check_network(&blob_id).await {
                Ok(true) => Err((
                    StatusCode::ACCEPTED,
                    "Data found in network. Syncing to local node. Please try again later."
                        .to_string(),
                )),
                Ok(false) => Err((StatusCode::NOT_FOUND, "Blob not found".to_string())),
                Err(_) => Err((StatusCode::NOT_FOUND, "Blob not found".to_string())),
            }
        }
        Err(e) => {
            error!("Blob fetch error: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

async fn cdn_serve_handler(
    State(ctx): State<GatewayContext>,
    Path(id_hex): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    let blob_id = parse_id(&id_hex)?;

    if is_program_id(&blob_id, &ctx).await {
        return Err((
            StatusCode::BAD_REQUEST,
            "ID appears to be a program, not a blob. Gateway only serves blob data.".to_string(),
        ));
    }

    let fetcher = BlobFetcher::new(ctx.node.clone());

    match fetcher.fetch_blob(&blob_id).await {
        Ok(data) => {
            let content_type = ctx.detector.detect(&data);

            Ok((
                [
                    (header::CONTENT_TYPE, content_type.as_str()),
                    (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
                    (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
                ],
                data,
            )
                .into_response())
        }
        Err(e) if e.to_string().contains("not found") => {
            match fetcher.check_network(&blob_id).await {
                Ok(true) => Err((
                    StatusCode::ACCEPTED,
                    "Content syncing from network. Please try again in a few moments.".to_string(),
                )),
                Ok(false) => Err((StatusCode::NOT_FOUND, "Content not found".to_string())),
                Err(_) => Err((StatusCode::NOT_FOUND, "Content not found".to_string())),
            }
        }
        Err(e) => {
            error!("CDN fetch error: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}



fn parse_id(id_hex: &str) -> Result<crate::types::ObjectId, (StatusCode, String)> {
    let decoded = hex::decode(id_hex).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid ID: must be hex-encoded".to_string(),
        )
    })?;

    if decoded.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid ID: must be 32 bytes".to_string(),
        ));
    }

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&decoded);
    Ok(crate::types::ObjectId(arr))
}

async fn is_program_id(id: &crate::types::ObjectId, ctx: &GatewayContext) -> bool {
    if let Ok(Some(obj)) = ctx.node.unified_store.get_object_metadata(id) {
        matches!(
            obj.object_type,
            crate::types::ObjectType::WasmProgram { .. }
        )
    } else {
        false
    }
}

fn is_renderable(content_type: &str) -> bool {
    content_type.starts_with("image/")
        || content_type.starts_with("video/")
        || content_type.starts_with("audio/")
        || content_type == "application/pdf"
        || content_type.starts_with("text/")
}

