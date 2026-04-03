use std::sync::Arc;

use crate::{config::AppConfig, services::scan::ScanService};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub scan_service: Arc<dyn ScanService>,
}

impl AppState {
    pub fn new(config: AppConfig, scan_service: Arc<dyn ScanService>) -> Self {
        Self {
            config,
            scan_service,
        }
    }
}
