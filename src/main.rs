use std::net::SocketAddr;

use axum::{
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::get,
    Router,
};
use supafast_jellyfin::http::middleware::correlation_id::{
    correlation_id_middleware,
    CorrelationId,
};
use supafast_jellyfin::telemetry::logging;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();

    let app = Router::new()
        .route("/api/health", get(health))
        .layer(middleware::from_fn(correlation_id_middleware));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    tracing::info!("starting server on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health(correlation_id: CorrelationId) -> impl IntoResponse {
    tracing::info!(correlation_id = %correlation_id, "health endpoint called");
    (StatusCode::OK, "ok")
}
