use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::{Hash, Hasher},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    models::{
        audit::{AuditEntry, AuditOpStatus},
        execute::{
            ExecuteNormalizeRequest, ExecuteNormalizeResponse, NormalizeJobResponse,
            NormalizeJobStatus,
        },
        plan::{NormalizePlan, PlanOpType, PlanOperation},
        scan::ScanResult,
    },
    repositories::execute_repo::{ClaimJobResult, ExecuteRepository, ExecuteRepositoryError, JobRecord},
    safety::path_guard,
    services::scan::ScanService,
};

#[derive(Debug, Error)]
pub enum ExecuteServiceError {
    #[error("scan session not found")]
    ScanNotFound,
    #[error("request does not match scan session")]
    ScanMismatch,
    #[error("idempotency key reused with different payload")]
    IdempotencyConflict,
    #[error("preflight validation failed")]
    Preflight,
    #[error("internal execute service error: {0}")]
    Internal(String),
}

#[async_trait]
pub trait ExecuteService: Send + Sync {
    async fn execute(
        &self,
        request: ExecuteNormalizeRequest,
    ) -> Result<ExecuteNormalizeResponse, ExecuteServiceError>;

    async fn get_job_status(
        &self,
        job_id: Uuid,
    ) -> Result<Option<NormalizeJobResponse>, ExecuteServiceError>;
}

pub struct InMemoryExecuteService {
    scan_service: Arc<dyn ScanService>,
    repository: Arc<dyn ExecuteRepository>,
    active_base_paths: Arc<Mutex<HashSet<String>>>,
}

impl InMemoryExecuteService {
    pub fn new(scan_service: Arc<dyn ScanService>, repository: Arc<dyn ExecuteRepository>) -> Self {
        Self {
            scan_service,
            repository,
            active_base_paths: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

#[async_trait]
impl ExecuteService for InMemoryExecuteService {
    async fn execute(
        &self,
        request: ExecuteNormalizeRequest,
    ) -> Result<ExecuteNormalizeResponse, ExecuteServiceError> {
        let request_fingerprint = compute_request_fingerprint(&request)?;

        if request.idempotency_key.trim().is_empty() {
            return Err(ExecuteServiceError::Preflight);
        }

        let scan = self
            .scan_service
            .get_scan(request.scan_id)
            .await
            .map_err(|error| ExecuteServiceError::Internal(error.to_string()))?
            .ok_or(ExecuteServiceError::ScanNotFound)?;

        let canonical_scan_base = canonicalize_base_path(&scan.base_path)?;
        let canonical_request_base = canonicalize_base_path(&request.base_path)?;

        if scan.media_type != request.media_type || canonical_scan_base != canonical_request_base {
            return Err(ExecuteServiceError::ScanMismatch);
        }

        if let Some(existing) = self
            .repository
            .get_by_idempotency_key(request.scan_id, &request.idempotency_key)
            .await
            .map_err(map_repo_error)?
        {
            if existing.request_fingerprint != request_fingerprint {
                return Err(ExecuteServiceError::IdempotencyConflict);
            }

            return Ok(ExecuteNormalizeResponse {
                job_id: existing.job_id,
                scan_id: existing.scan_id,
                status: existing.status,
                replayed: true,
            });
        }

        let plan = build_plan(&scan, &request)?;
        preflight_validate(&plan, request.dry_run)?;

        {
            let mut active = self.active_base_paths.lock().await;
            if !active.insert(canonical_request_base.clone()) {
                return Err(ExecuteServiceError::Preflight);
            }
        }

        let job_id = Uuid::new_v4();
        let entries = plan
            .operations
            .iter()
            .map(|operation| AuditEntry {
                op_id: operation.op_id,
                op_index: operation.op_index,
                session_id: job_id,
                op_type: "rename_dir".to_string(),
                source_path: operation.source_path.clone(),
                target_path: operation.target_path.clone(),
                file_size: None,
                status: AuditOpStatus::Pending,
                error_message: None,
            })
            .collect::<Vec<_>>();

        let pending_job = JobRecord {
            job_id,
            scan_id: request.scan_id,
            idempotency_key: request.idempotency_key.clone(),
            media_type: request.media_type.clone(),
            base_path: request.base_path.clone(),
            request_fingerprint,
            plan_hash: plan.plan_hash,
            status: NormalizeJobStatus::Pending,
            error: None,
            entries,
        };

        let claimed = match self.repository.claim_job(pending_job.clone()).await {
            Ok(result) => result,
            Err(error) => {
                release_active_base(&self.active_base_paths, &canonical_request_base).await;
                return Err(map_repo_error(error));
            }
        };

        match claimed {
            ClaimJobResult::Conflict => {
                release_active_base(&self.active_base_paths, &canonical_request_base).await;
                Err(ExecuteServiceError::IdempotencyConflict)
            }
            ClaimJobResult::Replayed(existing) => {
                release_active_base(&self.active_base_paths, &canonical_request_base).await;
                Ok(ExecuteNormalizeResponse {
                    job_id: existing.job_id,
                    scan_id: existing.scan_id,
                    status: existing.status,
                    replayed: true,
                })
            }
            ClaimJobResult::Created(created) => {
                let repository = self.repository.clone();
                let dry_run = request.dry_run;
                let active_base_paths = self.active_base_paths.clone();
                let base_key = canonical_request_base.clone();
                tokio::spawn(async move {
                    run_job(repository, created, dry_run, active_base_paths, base_key).await;
                });

                Ok(ExecuteNormalizeResponse {
                    job_id,
                    scan_id: request.scan_id,
                    status: NormalizeJobStatus::Pending,
                    replayed: false,
                })
            }
        }
    }

    async fn get_job_status(
        &self,
        job_id: Uuid,
    ) -> Result<Option<NormalizeJobResponse>, ExecuteServiceError> {
        let Some(job) = self.repository.get_job(job_id).await.map_err(map_repo_error)? else {
            return Ok(None);
        };

        let total_ops = job.entries.len();
        let completed_ops = job
            .entries
            .iter()
            .filter(|entry| entry.status == AuditOpStatus::Succeeded)
            .count();
        let failed_ops = job
            .entries
            .iter()
            .filter(|entry| entry.status == AuditOpStatus::Failed)
            .count();

        Ok(Some(NormalizeJobResponse {
            job_id: job.job_id,
            scan_id: job.scan_id,
            status: job.status,
            plan_hash: job.plan_hash,
            total_ops,
            completed_ops,
            failed_ops,
            error: job.error,
            entries: job.entries,
        }))
    }
}

async fn run_job(
    repository: Arc<dyn ExecuteRepository>,
    mut job: JobRecord,
    dry_run: bool,
    active_base_paths: Arc<Mutex<HashSet<String>>>,
    base_key: String,
) {
    job.status = NormalizeJobStatus::Running;
    if repository.update_job(job.clone()).await.is_err() {
        release_active_base(&active_base_paths, &base_key).await;
        return;
    }

    let canonical_base = match Path::new(&job.base_path).canonicalize() {
        Ok(path) => path,
        Err(_) => {
            job.status = NormalizeJobStatus::Failed;
            job.error = Some("failed to resolve base path".to_string());
            let _ = repository.update_job(job).await;
            release_active_base(&active_base_paths, &base_key).await;
            return;
        }
    };

    for index in 0..job.entries.len() {
        job.entries[index].status = AuditOpStatus::Running;
        let _ = repository.update_job(job.clone()).await;

        if dry_run {
            job.entries[index].status = AuditOpStatus::Succeeded;
            let _ = repository.update_job(job.clone()).await;
            continue;
        }

        let source_path = job.entries[index].source_path.clone();
        let target_path = job.entries[index].target_path.clone();
        if revalidate_operation_paths(&canonical_base, Path::new(&source_path), Path::new(&target_path)).is_err() {
            tracing::warn!("execute preflight revalidation failed");
            job.entries[index].status = AuditOpStatus::Failed;
            job.entries[index].error_message = Some("operation failed safety revalidation".to_string());
            job.status = NormalizeJobStatus::Failed;
            job.error = Some("operation failed safety revalidation".to_string());
            let _ = repository.update_job(job).await;
            release_active_base(&active_base_paths, &base_key).await;
            return;
        }

        let rename_outcome = tokio::task::spawn_blocking(move || std::fs::rename(&source_path, &target_path)).await;
        match rename_outcome {
            Ok(Ok(())) => {
                job.entries[index].status = AuditOpStatus::Succeeded;
                let _ = repository.update_job(job.clone()).await;
            }
            Ok(Err(_)) | Err(_) => {
                tracing::warn!("filesystem rename failed");
                job.entries[index].status = AuditOpStatus::Failed;
                job.entries[index].error_message = Some("filesystem operation failed".to_string());
                job.status = NormalizeJobStatus::Failed;
                job.error = Some("filesystem operation failed".to_string());
                let _ = repository.update_job(job).await;
                release_active_base(&active_base_paths, &base_key).await;
                return;
            }
        }
    }

    job.status = NormalizeJobStatus::Completed;
    let _ = repository.update_job(job).await;
    release_active_base(&active_base_paths, &base_key).await;
}

fn build_plan(
    scan: &ScanResult,
    request: &ExecuteNormalizeRequest,
) -> Result<NormalizePlan, ExecuteServiceError> {
    let base_path = Path::new(&request.base_path);
    if !base_path.is_absolute() {
        return Err(ExecuteServiceError::Preflight);
    }
    let canonical_base = base_path
        .canonicalize()
        .map_err(|_| ExecuteServiceError::Preflight)?;

    let overrides: HashMap<&str, &str> = request
        .name_overrides
        .iter()
        .map(|override_| {
            (
                override_.source_directory.as_str(),
                override_.normalized_name.as_str(),
            )
        })
        .collect();

    let conflict_resolutions: HashMap<&str, &str> = request
        .conflict_resolutions
        .iter()
        .map(|resolution| {
            (
                resolution.normalized_name.as_str(),
                resolution.selected_source_directory.as_str(),
            )
        })
        .collect();

    let mut operations = Vec::new();

    for group in &scan.groups {
        if let Some(selected) = conflict_resolutions.get(group.normalized_name.as_str()) {
            if !group.source_directories.iter().any(|source| source == selected) {
                return Err(ExecuteServiceError::Preflight);
            }
        }

        for source_directory in &group.source_directories {
            if let Some(selected) = conflict_resolutions.get(group.normalized_name.as_str()) {
                if source_directory != selected {
                    continue;
                }
            }

            let target_directory = overrides
                .get(source_directory.as_str())
                .copied()
                .unwrap_or(group.normalized_name.as_str());

            if source_directory == target_directory {
                continue;
            }

            let source_relative = parse_relative_component(source_directory)?;
            let target_relative = parse_relative_component(target_directory)?;
            let source_path = path_guard::resolve_under_base(&canonical_base, &source_relative)
                .map_err(|_| ExecuteServiceError::Preflight)?;
            let target_path = path_guard::resolve_under_base(&canonical_base, &target_relative)
                .map_err(|_| ExecuteServiceError::Preflight)?;

            operations.push((
                source_directory.clone(),
                target_directory.to_string(),
                source_path,
                target_path,
            ));
        }
    }

    operations.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| left.0.cmp(&right.0))
            .then_with(|| left.2.cmp(&right.2))
    });

    let operations = operations
        .into_iter()
        .enumerate()
        .map(|(index, (_, _, source_path, target_path))| PlanOperation {
            op_id: Uuid::from_u128((index + 1) as u128),
            op_index: index,
            op_type: PlanOpType::RenameDir,
            source_path: source_path.to_string_lossy().to_string(),
            target_path: target_path.to_string_lossy().to_string(),
        })
        .collect::<Vec<_>>();

    let plan_hash = compute_plan_hash(scan.scan_id, &request.base_path, &request.media_type, &operations)?;

    Ok(NormalizePlan {
        plan_hash,
        scan_id: scan.scan_id,
        idempotency_key: request.idempotency_key.clone(),
        base_path: request.base_path.clone(),
        media_type: request.media_type.clone(),
        operations,
    })
}

fn parse_relative_component(value: &str) -> Result<PathBuf, ExecuteServiceError> {
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(ExecuteServiceError::Preflight);
    }

    let mut components = path.components();
    let only_component = components.next();
    if components.next().is_some() {
        return Err(ExecuteServiceError::Preflight);
    }

    if !matches!(only_component, Some(Component::Normal(_))) {
        return Err(ExecuteServiceError::Preflight);
    }

    Ok(PathBuf::from(value))
}

fn preflight_validate(plan: &NormalizePlan, dry_run: bool) -> Result<(), ExecuteServiceError> {
    let canonical_base = Path::new(&plan.base_path)
        .canonicalize()
        .map_err(|_| ExecuteServiceError::Preflight)?;

    let mut targets = HashSet::new();

    for operation in &plan.operations {
        let source = Path::new(&operation.source_path);
        let target = Path::new(&operation.target_path);

        revalidate_operation_paths(&canonical_base, source, target)?;

        if !targets.insert(target.to_path_buf()) {
            return Err(ExecuteServiceError::Preflight);
        }

        if !source.exists() {
            return Err(ExecuteServiceError::Preflight);
        }

        if !dry_run && target.exists() {
            let same_path = match (source.canonicalize(), target.canonicalize()) {
                (Ok(source), Ok(target)) => source == target,
                _ => false,
            };

            if !same_path {
                return Err(ExecuteServiceError::Preflight);
            }
        }
    }

    Ok(())
}

fn revalidate_operation_paths(
    canonical_base: &Path,
    source: &Path,
    target: &Path,
) -> Result<(), ExecuteServiceError> {
    ensure_absolute_under_base(canonical_base, source, true)?;
    ensure_absolute_under_base(canonical_base, target, false)?;
    Ok(())
}

fn ensure_absolute_under_base(
    canonical_base: &Path,
    candidate: &Path,
    must_exist: bool,
) -> Result<(), ExecuteServiceError> {
    if !candidate.is_absolute() {
        return Err(ExecuteServiceError::Preflight);
    }

    let parent = candidate.parent().ok_or(ExecuteServiceError::Preflight)?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| ExecuteServiceError::Preflight)?;

    if !canonical_parent.starts_with(canonical_base) {
        return Err(ExecuteServiceError::Preflight);
    }

    if must_exist && !candidate.exists() {
        return Err(ExecuteServiceError::Preflight);
    }

    if candidate.exists() {
        let canonical_candidate = candidate
            .canonicalize()
            .map_err(|_| ExecuteServiceError::Preflight)?;
        if !canonical_candidate.starts_with(canonical_base) {
            return Err(ExecuteServiceError::Preflight);
        }
    }

    Ok(())
}

fn compute_plan_hash(
    scan_id: Uuid,
    base_path: &str,
    media_type: &str,
    operations: &[PlanOperation],
) -> Result<String, ExecuteServiceError> {
    let canonical = serde_json::to_string(&(scan_id, base_path, media_type, operations))
        .map_err(|error| ExecuteServiceError::Internal(error.to_string()))?;

    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn compute_request_fingerprint(
    request: &ExecuteNormalizeRequest,
) -> Result<String, ExecuteServiceError> {
    let canonical_base_path = canonicalize_base_path(&request.base_path)?;

    let mut overrides = request
        .name_overrides
        .iter()
        .map(|override_| {
            (
                override_.source_directory.clone(),
                override_.normalized_name.clone(),
            )
        })
        .collect::<Vec<_>>();
    overrides.sort();

    let mut resolutions = request
        .conflict_resolutions
        .iter()
        .map(|resolution| {
            (
                resolution.normalized_name.clone(),
                resolution.selected_source_directory.clone(),
            )
        })
        .collect::<Vec<_>>();
    resolutions.sort();

    let payload = serde_json::to_string(&(
        request.scan_id,
        request.media_type.as_str(),
        canonical_base_path,
        request.dry_run,
        overrides,
        resolutions,
    ))
    .map_err(|error| ExecuteServiceError::Internal(error.to_string()))?;

    let mut hasher = DefaultHasher::new();
    payload.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn map_repo_error(error: ExecuteRepositoryError) -> ExecuteServiceError {
    ExecuteServiceError::Internal(error.to_string())
}

fn canonicalize_base_path(path: &str) -> Result<String, ExecuteServiceError> {
    let canonical = Path::new(path)
        .canonicalize()
        .map_err(|_| ExecuteServiceError::Preflight)?;
    Ok(canonical.to_string_lossy().to_string())
}

async fn release_active_base(active_base_paths: &Mutex<HashSet<String>>, base_key: &str) {
    let mut active = active_base_paths.lock().await;
    active.remove(base_key);
}
