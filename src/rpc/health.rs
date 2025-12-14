use crate::execution::{HealthReporter, NodeHealth, NodeMetrics};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;

pub fn health_routes() -> Router<Arc<HealthRpcContext>> {
    Router::new()
        .route("/health/liveness", get(liveness))
        .route("/health/readiness", get(readiness))
        .route("/health/metrics", get(metrics))
}

#[derive(Clone)]
pub struct HealthRpcContext {
    pub reporter: Arc<HealthReporter>,
    pub get_queue_depth: Arc<dyn Fn() -> usize + Send + Sync>,
    pub get_running_count: Arc<dyn Fn() -> usize + Send + Sync>,
    pub max_concurrent: usize,
}

#[derive(Serialize)]
struct LivenessResponse {
    alive: bool,
}

async fn liveness() -> Json<LivenessResponse> {
    Json(LivenessResponse { alive: true })
}

async fn readiness(
    State(ctx): State<Arc<HealthRpcContext>>,
) -> Result<Json<NodeHealth>, StatusCode> {
    let queue_depth = (ctx.get_queue_depth)();
    let running_count = (ctx.get_running_count)();

    let health = ctx
        .reporter
        .get_health(running_count, queue_depth, ctx.max_concurrent);

    if matches!(health.status, crate::execution::HealthStatus::Unavailable) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    Ok(Json(health))
}

async fn metrics(State(ctx): State<Arc<HealthRpcContext>>) -> Json<NodeMetrics> {
    let metrics = ctx.reporter.get_metrics();
    Json(metrics)
}
