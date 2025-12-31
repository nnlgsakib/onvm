use crate::node::Node;
use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
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
use super::ui;

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
        .route("/", get(explorer_ui))
        .route("/explorer", get(explorer_ui))
        .route("/cdn/:id", get(cdn_serve_handler))
        .route("/cdn/:id/*path", get(cdn_serve_handler))
        .route("/share/:id", get(share_page_handler))
        .route("/api/blob/:id", get(get_blob_handler))
        .route("/api/blob/:id/info", get(get_blob_info))
        .route("/api/health", get(health_check))
        .layer(CorsLayer::permissive())
        .with_state(ctx);

    let listener = TcpListener::bind(&config.listen_addr).await?;
    info!("Blob Gateway listening on {}", config.listen_addr);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn explorer_ui() -> Html<String> {
    Html(ui::EXPLORER_HTML.to_string())
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

async fn share_page_handler(
    State(ctx): State<GatewayContext>,
    Path(id_hex): Path<String>,
) -> Result<Html<String>, (StatusCode, String)> {
    let blob_id = parse_id(&id_hex)?;

    if is_program_id(&blob_id, &ctx).await {
        return Err((
            StatusCode::BAD_REQUEST,
            "Cannot share program IDs. Gateway only serves blob data.".to_string(),
        ));
    }

    let fetcher = BlobFetcher::new(ctx.node.clone());
    let info = fetcher
        .get_blob_info(&blob_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let cdn_url = format!("/cdn/{}", id_hex);
    let mime_type = info
        .detected_mime_type
        .or(info.mime_type)
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let preview_html = if mime_type.starts_with("image/") {
        format!(
            r#"<img src="{}" alt="Shared content" style="max-width: 100%; height: auto; border-radius: 10px; box-shadow: 0 4px 12px rgba(0,0,0,0.1);">"#,
            cdn_url
        )
    } else if mime_type.starts_with("video/") {
        format!(
            r#"<video controls style="max-width: 100%; border-radius: 10px; box-shadow: 0 4px 12px rgba(0,0,0,0.1);"><source src="{}" type="{}">Your browser does not support video playback.</video>"#,
            cdn_url, mime_type
        )
    } else if mime_type.starts_with("audio/") {
        format!(
            r#"<audio controls style="width: 100%;"><source src="{}" type="{}">Your browser does not support audio playback.</audio>"#,
            cdn_url, mime_type
        )
    } else if mime_type == "application/pdf" {
        format!(
            r#"<embed src="{}" type="application/pdf" width="100%" height="600px" style="border-radius: 10px; box-shadow: 0 4px 12px rgba(0,0,0,0.1);">"#,
            cdn_url
        )
    } else if mime_type.starts_with("text/") {
        format!(
            r#"<iframe src="{}" style="width: 100%; height: 600px; border: 1px solid #ddd; border-radius: 10px;"></iframe>"#,
            cdn_url
        )
    } else {
        format!(
            r#"<div style="text-align: center; padding: 40px; background: #f8f9fa; border-radius: 10px;"><p style="font-size: 18px; margin-bottom: 20px;">Preview not available for this file type.</p><a href="{}" download style="display: inline-block; padding: 12px 24px; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; text-decoration: none; border-radius: 8px; font-weight: 600;">Download File</a></div>"#,
            cdn_url
        )
    };

    let size_str = format_size(info.size);
    let html = ui::generate_share_page(&id_hex, &mime_type, &size_str, &preview_html, &cdn_url);

    Ok(Html(html))
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

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
