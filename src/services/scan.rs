use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    models::scan::{ScanGroup, ScanResult, ScanSummary},
    parsers::directory::normalize_directory_name,
    repositories::scan_repo::{InMemoryScanRepository, ScanRepository, ScanSessionRecord},
};

#[derive(Debug, Clone)]
pub struct NewScanRequest {
    pub media_type: String,
    pub base_path: String,
    pub ttl_minutes: u64,
}

#[derive(Debug, Error)]
pub enum ScanServiceError {
    #[error("scan session not found")]
    NotFound,
    #[error("internal scan service error: {0}")]
    Internal(String),
}

#[async_trait]
pub trait ScanService: Send + Sync {
    async fn create_scan(&self, request: NewScanRequest) -> Result<ScanResult, ScanServiceError>;
    async fn get_scan(&self, scan_id: Uuid) -> Result<Option<ScanResult>, ScanServiceError>;
    async fn expire_scan(&self, scan_id: Uuid) -> Result<bool, ScanServiceError>;
}

pub struct InMemoryScanService {
    repository: Arc<dyn ScanRepository>,
}

impl InMemoryScanService {
    pub fn new() -> Self {
        Self {
            repository: Arc::new(InMemoryScanRepository::new()),
        }
    }

    pub fn with_repository(repository: Arc<dyn ScanRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl ScanService for InMemoryScanService {
    async fn create_scan(&self, request: NewScanRequest) -> Result<ScanResult, ScanServiceError> {
        let scan_id = Uuid::new_v4();
        let groups = build_groups_from_base_path(Path::new(&request.base_path))?;
        let ttl_secs = request
            .ttl_minutes
            .checked_mul(60)
            .ok_or_else(|| ScanServiceError::Internal("scan ttl overflow".to_string()))?;
        let ttl = Duration::from_secs(ttl_secs);
        let expires_at = SystemTime::now()
            .checked_add(ttl)
            .ok_or_else(|| ScanServiceError::Internal("scan ttl overflow".to_string()))?;
        let total_source_directories = groups
            .iter()
            .map(|group| group.source_directories.len() as u64)
            .sum::<u64>();
        let total_shows = groups.len() as u64;
        let scan_result = ScanResult {
            scan_id,
            media_type: request.media_type,
            base_path: request.base_path.clone(),
            groups,
            summary: ScanSummary {
                total_shows,
                total_episodes: 0,
                merges: total_source_directories.saturating_sub(total_shows),
                conflicts: 0,
            },
        };

        self.repository
            .prune_expired(SystemTime::now())
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))?;
        self.repository
            .save_scan(ScanSessionRecord {
                result: scan_result.clone(),
                base_path: request.base_path,
                expires_at,
            })
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))?;
        Ok(scan_result)
    }

    async fn get_scan(&self, scan_id: Uuid) -> Result<Option<ScanResult>, ScanServiceError> {
        self.repository
            .prune_expired(SystemTime::now())
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))?;
        let Some(stored) = self
            .repository
            .get_scan(scan_id)
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))?
        else {
            return Ok(None);
        };

        if SystemTime::now() >= stored.expires_at {
            self.repository
                .delete_scan(scan_id)
                .await
                .map_err(|error| ScanServiceError::Internal(error.to_string()))?;
            return Ok(None);
        }

        Ok(Some(stored.result.clone()))
    }

    async fn expire_scan(&self, scan_id: Uuid) -> Result<bool, ScanServiceError> {
        self.repository
            .prune_expired(SystemTime::now())
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))?;
        self.repository
            .delete_scan(scan_id)
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))
    }
}

fn build_groups_from_base_path(base_path: &Path) -> Result<Vec<ScanGroup>, ScanServiceError> {
    let entries = match fs::read_dir(base_path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(ScanServiceError::Internal(format!(
                "failed to read base_path {}: {error}",
                base_path.display()
            )));
        }
    };

    let mut grouped: HashMap<String, Vec<String>> = HashMap::new();

    for entry in entries {
        let entry = entry.map_err(|error| {
            ScanServiceError::Internal(format!(
                "failed to inspect directory entry in {}: {error}",
                base_path.display()
            ))
        })?;

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let source_directory = entry.file_name().to_string_lossy().into_owned();
        if is_ignored_scan_directory(&source_directory) {
            continue;
        }

        let normalized_name = normalize_directory_name(&source_directory);
        grouped
            .entry(normalized_name)
            .and_modify(|directories| directories.push(source_directory.clone()))
            .or_insert_with(|| vec![source_directory]);
    }

    let mut groups = grouped
        .into_iter()
        .map(|(normalized_name, mut source_directories)| {
            source_directories.sort();
            ScanGroup {
                normalized_name,
                source_directories,
            }
        })
        .collect::<Vec<_>>();

    groups.sort_by(|left, right| left.normalized_name.cmp(&right.normalized_name));
    Ok(groups)
}

fn is_ignored_scan_directory(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(".trickplay")
}
