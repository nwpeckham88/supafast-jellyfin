use std::sync::Arc;

use crate::{
    config::AppConfig,
    repositories::execute_repo::InMemoryExecuteRepository,
    services::{
        execute::{ExecuteService, InMemoryExecuteService},
        scan::{InMemoryScanService, ScanService},
    },
};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub scan_service: Arc<dyn ScanService>,
    pub execute_service: Arc<dyn ExecuteService>,
}

impl AppState {
    pub fn new(config: AppConfig, scan_service: Arc<dyn ScanService>) -> Self {
        let execute_service: Arc<dyn ExecuteService> = Arc::new(InMemoryExecuteService::new(
            scan_service.clone(),
            Arc::new(InMemoryExecuteRepository::new()),
        ));

        Self {
            config,
            scan_service,
            execute_service,
        }
    }

    pub fn new_for_tests(config: AppConfig) -> Self {
        let scan_service: Arc<dyn ScanService> = Arc::new(InMemoryScanService::new());
        Self::new(config, scan_service)
    }
}
