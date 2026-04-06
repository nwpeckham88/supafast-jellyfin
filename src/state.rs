use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    config::AppConfig,
    repositories::execute_repo::{
        ExecuteRepository, InMemoryExecuteRepository, PgExecuteRepository,
    },
    safety::base_path_lock::BasePathLockManager,
    services::{
        execute::{ExecuteService, InMemoryExecuteService},
        rollback::{InMemoryRollbackService, RollbackService},
        scan::{InMemoryScanService, ScanService},
    },
};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub scan_service: Arc<dyn ScanService>,
    pub execute_service: Arc<dyn ExecuteService>,
    pub rollback_service: Arc<dyn RollbackService>,
}

impl AppState {
    pub fn new(
        config: AppConfig,
        scan_service: Arc<dyn ScanService>,
        pg_pool: Option<PgPool>,
    ) -> Self {
        let execute_repository: Arc<dyn ExecuteRepository> = match pg_pool {
            Some(pool) => Arc::new(PgExecuteRepository::new(pool)),
            None => Arc::new(InMemoryExecuteRepository::new()),
        };
        let base_path_lock = Arc::new(BasePathLockManager::new());

        let execute_service: Arc<dyn ExecuteService> = Arc::new(InMemoryExecuteService::new(
            scan_service.clone(),
            execute_repository.clone(),
            base_path_lock.clone(),
            config.movie_rename_mode,
        ));

        let rollback_service: Arc<dyn RollbackService> = Arc::new(InMemoryRollbackService::new(
            execute_repository,
            base_path_lock,
        ));

        Self {
            config,
            scan_service,
            execute_service,
            rollback_service,
        }
    }

    pub fn new_for_tests(config: AppConfig) -> Self {
        let scan_service: Arc<dyn ScanService> = Arc::new(InMemoryScanService::new());
        Self::new(config, scan_service, None)
    }
}
