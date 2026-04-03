use std::net::SocketAddr;
use std::sync::Arc;

use supafast_jellyfin::config::AppConfig;
use supafast_jellyfin::db::{assert_scan_schema_ready, connect_pg_pool};
use supafast_jellyfin::http::build_app;
use supafast_jellyfin::repositories::scan_repo::{
    InMemoryScanRepository, PgScanRepository, ScanRepository,
};
use supafast_jellyfin::services::scan::{InMemoryScanService, ScanService};
use supafast_jellyfin::state::AppState;
use supafast_jellyfin::telemetry::logging;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    let config = AppConfig::from_env()?;
    let scan_repository: Arc<dyn ScanRepository> = match config.database_url.as_deref() {
        Some(database_url) => {
            let pool = connect_pg_pool(database_url).await?;
            assert_scan_schema_ready(&pool).await?;
            Arc::new(PgScanRepository::new(pool))
        }
        None => Arc::new(InMemoryScanRepository::new()),
    };
    let scan_service: Arc<dyn ScanService> =
        Arc::new(InMemoryScanService::with_repository(scan_repository));
    let state = AppState::new(config.clone(), scan_service);

    let app = build_app(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
    tracing::info!("starting server on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
