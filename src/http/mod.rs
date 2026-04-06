use axum::{http::StatusCode, middleware::from_fn, response::IntoResponse, routing::get, Json, Router};

use crate::{
    http::{
        middleware::correlation_id::{correlation_id_middleware, CorrelationId},
        response::ApiResponse,
    },
    state::AppState,
};

pub mod api;
pub mod middleware;
pub mod response;

pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .merge(api::execute::router())
        .merge(api::rollback::router())
        .merge(api::scan::router())
        .with_state(state)
        .layer(from_fn(correlation_id_middleware))
}

async fn health(correlation_id: CorrelationId) -> impl IntoResponse {
    tracing::info!(correlation_id = %correlation_id, "health endpoint called");
    (StatusCode::OK, Json(ApiResponse::success("ok")))
}
