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
    config.log_startup_diagnostics();
    config.validate_startup_paths()?;
    tracing::info!("startup path validation passed");

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

    let bind_host = config.host.clone();
    let bind_port = config.port;
    let bind_target = (bind_host.as_str(), bind_port);
    let listener = tokio::net::TcpListener::bind(bind_target)
        .await
        .map_err(|error| anyhow::anyhow!("failed to bind {bind_host}:{bind_port}: {error}"))?;
    let addr = listener.local_addr()?;
    tracing::info!("starting server on {addr}");

    axum::serve(listener, app).await?;

    Ok(())
}
