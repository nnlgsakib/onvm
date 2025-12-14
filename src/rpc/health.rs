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
    pub scheduler: std::sync::Weak<crate::execution::JobScheduler>,
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
    let scheduler = ctx
        .scheduler
        .upgrade()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let queue_depth = scheduler.queue_depth().await;
    let running_count = scheduler.running_count().await;

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