use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    models::{
        audit::{AuditEntry, AuditOpStatus},
        execute::{
            ExecuteNormalizeRequest, ExecuteNormalizeResponse, ExtraFileType, MovieRenameMode,
            NormalizeJobEntriesResponse, NormalizeJobListResponse, NormalizeJobResponse,
            NormalizeJobStatus, NormalizeJobSummary, PaginationMeta,
        },
        plan::{NormalizePlan, PlanOpType, PlanOperation},
        scan::{ScanGroup, ScanResult},
    },
    repositories::execute_repo::{
        ClaimJobResult, ExecuteRepository, ExecuteRepositoryError, JobRecord,
    },
    parsers::{
        directory::{append_metadata_tags, extract_metadata_tags, normalize_directory_name},
        episode::normalize_episode_filename_with_nfo,
        season::normalize_season_directory_name,
    },
    safety::{base_path_lock::BasePathLockManager, fs_ops, path_guard},
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
    #[error("preflight validation failed: {0}")]
    Preflight(String),
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

    async fn list_jobs(
        &self,
        page: usize,
        page_size: usize,
    ) -> Result<NormalizeJobListResponse, ExecuteServiceError>;

    async fn get_job_entries(
        &self,
        job_id: Uuid,
        page: usize,
        page_size: usize,
    ) -> Result<Option<NormalizeJobEntriesResponse>, ExecuteServiceError>;
}

pub struct InMemoryExecuteService {
    scan_service: Arc<dyn ScanService>,
    repository: Arc<dyn ExecuteRepository>,
    base_path_lock: Arc<BasePathLockManager>,
    default_movie_rename_mode: MovieRenameMode,
    default_remove_extra_file_types: Vec<ExtraFileType>,
}

const DRY_RUN_PROGRESS_BATCH_SIZE: usize = 100;

impl InMemoryExecuteService {
    pub fn new(
        scan_service: Arc<dyn ScanService>,
        repository: Arc<dyn ExecuteRepository>,
        base_path_lock: Arc<BasePathLockManager>,
        default_movie_rename_mode: MovieRenameMode,
        default_remove_extra_file_types: Vec<ExtraFileType>,
    ) -> Self {
        Self {
            scan_service,
            repository,
            base_path_lock,
            default_movie_rename_mode,
            default_remove_extra_file_types,
        }
    }
}

#[async_trait]
impl ExecuteService for InMemoryExecuteService {
    async fn execute(
        &self,
        mut request: ExecuteNormalizeRequest,
    ) -> Result<ExecuteNormalizeResponse, ExecuteServiceError> {
        if request.movie_rename_mode.is_none() {
            request.movie_rename_mode = Some(self.default_movie_rename_mode);
        }
        if request.remove_extra_file_types.is_none() {
            request.remove_extra_file_types = Some(self.default_remove_extra_file_types.clone());
        }

        let request_fingerprint = compute_request_fingerprint(&request)?;

        if request.idempotency_key.trim().is_empty() {
            return Err(preflight("idempotency_key must not be empty"));
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
        validate_plan_for_media_type(&plan)?;
        preflight_validate(&plan, request.dry_run)?;

        if !self
            .base_path_lock
            .try_acquire(&canonical_request_base)
            .await
        {
            return Err(preflight(
                "another normalize job is already active for this base path",
            ));
        }

        let job_id = Uuid::new_v4();
        let entries = plan
            .operations
            .iter()
            .map(|operation| AuditEntry {
                op_id: operation.op_id,
                op_index: operation.op_index,
                session_id: job_id,
                op_type: map_plan_op_type(operation.op_type).to_string(),
                source_path: operation.source_path.clone(),
                target_path: operation.target_path.clone(),
                file_size: None,
                status: AuditOpStatus::Pending,
                error_message: None,
            })
            .collect::<Vec<_>>();

        let pending_job = JobRecord {
            job_id,
            created_at_ms: current_timestamp_ms(),
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
                self.base_path_lock.release(&canonical_request_base).await;
                return Err(map_repo_error(error));
            }
        };

        match claimed {
            ClaimJobResult::Conflict => {
                self.base_path_lock.release(&canonical_request_base).await;
                Err(ExecuteServiceError::IdempotencyConflict)
            }
            ClaimJobResult::Replayed(existing) => {
                self.base_path_lock.release(&canonical_request_base).await;
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
                let base_path_lock = self.base_path_lock.clone();
                let base_key = canonical_request_base.clone();
                tokio::spawn(async move {
                    run_job(repository, created, dry_run, base_path_lock, base_key).await;
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
        let Some(job) = self
            .repository
            .get_job(job_id)
            .await
            .map_err(map_repo_error)?
        else {
            return Ok(None);
        };

        let summary = summarize_job(&job);

        Ok(Some(NormalizeJobResponse {
            job_id: summary.job_id,
            scan_id: summary.scan_id,
            status: summary.status,
            plan_hash: summary.plan_hash,
            total_ops: summary.total_ops,
            completed_ops: summary.completed_ops,
            failed_ops: summary.failed_ops,
            error: summary.error,
            entries: job.entries,
        }))
    }

    async fn list_jobs(
        &self,
        page: usize,
        page_size: usize,
    ) -> Result<NormalizeJobListResponse, ExecuteServiceError> {
        if page == 0 || page_size == 0 {
            return Err(ExecuteServiceError::Internal(
                "invalid pagination parameters".to_string(),
            ));
        }

        let mut jobs = self.repository.list_jobs().await.map_err(map_repo_error)?;
        jobs.sort_by(|left, right| right.created_at_ms.cmp(&left.created_at_ms));

        let total = jobs.len();
        let pagination = build_pagination_meta(total, page, page_size);
        let (start, end) = page_bounds(total, page, page_size);

        let paged_jobs = jobs[start..end]
            .iter()
            .map(summarize_job)
            .collect::<Vec<_>>();

        Ok(NormalizeJobListResponse {
            jobs: paged_jobs,
            pagination,
        })
    }

    async fn get_job_entries(
        &self,
        job_id: Uuid,
        page: usize,
        page_size: usize,
    ) -> Result<Option<NormalizeJobEntriesResponse>, ExecuteServiceError> {
        if page == 0 || page_size == 0 {
            return Err(ExecuteServiceError::Internal(
                "invalid pagination parameters".to_string(),
            ));
        }

        let Some(job) = self
            .repository
            .get_job(job_id)
            .await
            .map_err(map_repo_error)?
        else {
            return Ok(None);
        };

        let total = job.entries.len();
        let pagination = build_pagination_meta(total, page, page_size);
        let (start, end) = page_bounds(total, page, page_size);

        Ok(Some(NormalizeJobEntriesResponse {
            job_id,
            entries: job.entries[start..end].to_vec(),
            pagination,
        }))
    }
}

async fn run_job(
    repository: Arc<dyn ExecuteRepository>,
    mut job: JobRecord,
    dry_run: bool,
    base_path_lock: Arc<BasePathLockManager>,
    base_key: String,
) {
    job.status = NormalizeJobStatus::Running;
    if repository.update_job(job.clone()).await.is_err() {
        base_path_lock.release(&base_key).await;
        return;
    }

    let canonical_base = match Path::new(&job.base_path).canonicalize() {
        Ok(path) => path,
        Err(_) => {
            job.status = NormalizeJobStatus::Failed;
            job.error = Some("failed to resolve base path".to_string());
            let _ = repository.update_job(job).await;
            base_path_lock.release(&base_key).await;
            return;
        }
    };

    if dry_run {
        let total_entries = job.entries.len();
        for index in 0..total_entries {
            job.entries[index].status = AuditOpStatus::Succeeded;
            job.entries[index].error_message = None;

            let processed = index + 1;
            let should_publish = processed % DRY_RUN_PROGRESS_BATCH_SIZE == 0 || processed == total_entries;
            if should_publish {
                let _ = repository.update_job(job.clone()).await;
                tokio::task::yield_now().await;
            }
        }

        job.status = NormalizeJobStatus::Completed;
        let _ = repository.update_job(job).await;
        base_path_lock.release(&base_key).await;
        return;
    }

    for index in 0..job.entries.len() {
        job.entries[index].status = AuditOpStatus::Running;
        let _ = repository.update_job(job.clone()).await;

        let source_path = job.entries[index].source_path.clone();
        let target_path = job.entries[index].target_path.clone();
        let op_type = job.entries[index].op_type.clone();
        let revalidation_failed = if op_type == "delete_dir" || op_type == "delete_file" {
            ensure_absolute_under_base(&canonical_base, Path::new(&source_path), true).is_err()
        } else {
            revalidate_operation_paths(
                &canonical_base,
                Path::new(&source_path),
                Path::new(&target_path),
            )
            .is_err()
        };
        if revalidation_failed {
            tracing::warn!("execute preflight revalidation failed");
            job.entries[index].status = AuditOpStatus::Failed;
            job.entries[index].error_message =
                Some("operation failed safety revalidation".to_string());
            job.status = NormalizeJobStatus::Failed;
            job.error = Some("operation failed safety revalidation".to_string());
            let _ = repository.update_job(job).await;
            base_path_lock.release(&base_key).await;
            return;
        }

        let source_for_op = source_path.clone();
        let target_for_op = target_path.clone();
        let fs_outcome = tokio::task::spawn_blocking(move || {
            if op_type == "delete_dir" {
                std::fs::remove_dir(Path::new(&source_for_op))
            } else if op_type == "delete_file" {
                std::fs::remove_file(Path::new(&source_for_op))
            } else {
                fs_ops::rename_with_exdev_fallback(
                    Path::new(&source_for_op),
                    Path::new(&target_for_op),
                )
            }
        })
        .await;
        match fs_outcome {
            Ok(Ok(())) => {
                job.entries[index].status = AuditOpStatus::Succeeded;
                let _ = repository.update_job(job.clone()).await;
            }
            Ok(Err(_)) | Err(_) => {
                tracing::warn!(op_type = %job.entries[index].op_type, "filesystem operation failed");
                job.entries[index].status = AuditOpStatus::Failed;
                job.entries[index].error_message = Some("filesystem operation failed".to_string());
                job.status = NormalizeJobStatus::Failed;
                job.error = Some("filesystem operation failed".to_string());
                let _ = repository.update_job(job).await;
                base_path_lock.release(&base_key).await;
                return;
            }
        }
    }

    job.status = NormalizeJobStatus::Completed;
    let _ = repository.update_job(job).await;
    base_path_lock.release(&base_key).await;
}

fn build_plan(
    scan: &ScanResult,
    request: &ExecuteNormalizeRequest,
) -> Result<NormalizePlan, ExecuteServiceError> {
    let base_path = Path::new(&request.base_path);
    if !base_path.is_absolute() {
        return Err(preflight("base_path must be absolute"));
    }
    let canonical_base = base_path
        .canonicalize()
        .map_err(|_| preflight("base_path does not exist or is not accessible"))?;

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

    let movie_rename_mode = request.movie_rename_mode.unwrap_or(MovieRenameMode::Clean);
    let remove_extra_file_types = request
        .remove_extra_file_types
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect::<HashSet<_>>();
    let mut operation_candidates = Vec::new();

    for group in &scan.groups {
        let selected = conflict_resolutions
            .get(group.normalized_name.as_str())
            .copied();
        let selected_target_directory = selected.map(|selected_source| {
            build_target_directory_name_for_source(
                selected_source,
                group,
                &overrides,
                &request.media_type,
                movie_rename_mode,
            )
        });
        if let Some(selected) = selected {
            if !group
                .source_directories
                .iter()
                .any(|source| source == selected)
            {
                return Err(preflight(
                    "conflict resolution selected_source_directory is not in scan group",
                ));
            }
        } else if group.source_directories.len() > 1 {
            return Err(preflight(missing_conflict_resolution_message(group)));
        }

        for source_directory in &group.source_directories {
            if let Some(selected) = selected {
                if source_directory != selected {
                    let source_relative = parse_relative_component(source_directory)?;
                    let source_path =
                        path_guard::resolve_under_base(&canonical_base, &source_relative)
                            .map_err(|_| preflight("merge source resolved outside base path"))?;

                    let merge_target_relative = parse_relative_component(selected)?;
                    let merge_target_path = path_guard::resolve_under_base(
                        &canonical_base,
                        &merge_target_relative,
                    )
                    .map_err(|_| preflight("merge target resolved outside base path"))?;

                    operation_candidates.extend(
                        build_group_merge_operations(
                            &source_path,
                            &merge_target_path,
                            &request.media_type,
                            selected_target_directory.as_deref(),
                            &remove_extra_file_types,
                        )
                        .map_err(|_| preflight("failed to build merge operations from filesystem"))?,
                    );
                    continue;
                }
            }

            let target_directory = build_target_directory_name_for_source(
                source_directory,
                group,
                &overrides,
                &request.media_type,
                movie_rename_mode,
            );

            let source_relative = parse_relative_component(source_directory)?;
            let target_relative = parse_relative_component(&target_directory)?;
            let source_path = path_guard::resolve_under_base(&canonical_base, &source_relative)
                .map_err(|_| preflight("source path resolved outside base path"))?;
            let target_path = path_guard::resolve_under_base(&canonical_base, &target_relative)
                .map_err(|_| preflight("target path resolved outside base path"))?;

            if request.media_type == "tv" {
                operation_candidates.extend(
                    build_tv_nested_operations(&source_path)
                        .map_err(|_| preflight("failed to inspect nested TV paths"))?,
                );
            } else if request.media_type == "movie" {
                operation_candidates.extend(
                    build_movie_file_rename_operations(&source_path, &target_directory)
                        .map_err(|_| preflight("failed to inspect movie files"))?,
                );
            }

            if source_directory == &target_directory {
                continue;
            }

            operation_candidates.push((
                source_directory.clone(),
                target_directory,
                PlanOpType::RenameDir,
                source_path,
                target_path,
            ));
        }
    }

    operation_candidates.sort_by(|left, right| {
        let left_depth = left.3.components().count();
        let right_depth = right.3.components().count();

        right_depth
            .cmp(&left_depth)
            .then_with(|| op_type_rank(left.2).cmp(&op_type_rank(right.2)))
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.0.cmp(&right.0))
            .then_with(|| left.3.cmp(&right.3))
            .then_with(|| left.4.cmp(&right.4))
    });

    let operations = operation_candidates
        .into_iter()
        .map(|(_, _, op_type, source_path, target_path)| PlanOperation {
            op_id: Uuid::nil(),
            op_index: 0,
            op_type,
            source_path: source_path.to_string_lossy().to_string(),
            target_path: target_path.to_string_lossy().to_string(),
        })
        .collect::<Vec<_>>();

    let mut operations = operations;
    if request.media_type == "movie" {
        resolve_movie_file_target_collisions(&mut operations);
    }

    let mut operations = append_trickplay_companion_operations(operations);
    sort_plan_operations(&mut operations);
    for (index, operation) in operations.iter_mut().enumerate() {
        operation.op_id = Uuid::from_u128((index + 1) as u128);
        operation.op_index = index;
    }

    let plan_hash = compute_plan_hash(
        scan.scan_id,
        &request.base_path,
        &request.media_type,
        &operations,
    )?;

    Ok(NormalizePlan {
        plan_hash,
        scan_id: scan.scan_id,
        idempotency_key: request.idempotency_key.clone(),
        base_path: request.base_path.clone(),
        media_type: request.media_type.clone(),
        operations,
    })
}

fn build_group_merge_operations(
    source_group_path: &Path,
    target_group_path: &Path,
    media_type: &str,
    movie_target_directory_name: Option<&str>,
    remove_extra_file_types: &HashSet<ExtraFileType>,
) -> Result<Vec<(String, String, PlanOpType, PathBuf, PathBuf)>, std::io::Error> {
    let mut operations = Vec::new();

    for entry in std::fs::read_dir(source_group_path)? {
        let entry = entry?;
        let source_path = entry.path();
        let source_name = entry.file_name().to_string_lossy().to_string();

        if source_name.starts_with('.') {
            continue;
        }

        let target_name = if media_type == "tv" && source_path.is_dir() {
            normalize_season_directory_name(&source_name)
        } else if media_type == "tv"
            && source_path.is_file()
            && is_episode_container_dir(source_group_path)
        {
            normalize_episode_filename_with_nfo(source_group_path, &source_name)
        } else if media_type == "movie" && source_path.is_file() {
            movie_target_directory_name
                .map(|name| normalize_movie_related_filename(&source_name, name))
                .unwrap_or_else(|| source_name.clone())
        } else {
            source_name.clone()
        };

        let target_path = target_group_path.join(target_name.clone());
        if source_path == target_path {
            continue;
        }

        if source_path.is_dir() {
            if media_type == "movie"
                && should_remove_extra_movie_directory(&source_path, remove_extra_file_types)
            {
                operations.extend(build_delete_tree_operations(&source_path)?);
                continue;
            }

            if target_path.exists() {
                // Both source and target dirs exist — recurse to merge contents
                operations.extend(build_group_merge_operations(
                    &source_path,
                    &target_path,
                    media_type,
                    movie_target_directory_name,
                    remove_extra_file_types,
                )?);
            } else {
                // Move entire directory to target (it's a merge move, not a simple rename)
                if media_type == "tv" && is_episode_container_dir(&source_path) {
                    operations.extend(build_episode_file_rename_ops(&source_path)?);
                }
                operations.push((
                    source_name,
                    target_name,
                    PlanOpType::RenameDir,
                    source_path,
                    target_path.to_path_buf(),
                ));
            }
        } else {
            if media_type == "movie"
                && should_remove_extra_movie_file(&source_path, &source_name, remove_extra_file_types)
            {
                operations.push((
                    source_name.clone(),
                    source_name.clone(),
                    PlanOpType::DeleteFile,
                    source_path,
                    target_group_path.join(source_name),
                ));
                continue;
            }

            // File move during merge uses MoveFile op type
            operations.push((
                source_name,
                target_name,
                PlanOpType::MoveFile,
                source_path,
                target_path.to_path_buf(),
            ));
        }
    }

    // Add delete_dir for the source directory after its contents are moved
    let source_dir_name = source_group_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    operations.push((
        source_dir_name.clone(),
        source_dir_name,
        PlanOpType::DeleteDir,
        source_group_path.to_path_buf(),
        source_group_path.to_path_buf(), // target is same as source for delete
    ));

    Ok(operations)
}

fn build_tv_nested_operations(
    source_show_path: &Path,
) -> Result<Vec<(String, String, PlanOpType, PathBuf, PathBuf)>, std::io::Error> {
    let mut nested = build_tv_episode_file_operations(source_show_path)?;

    for season_entry in std::fs::read_dir(source_show_path)? {
        let season_entry = season_entry?;
        let season_path = season_entry.path();
        if !season_path.is_dir() {
            continue;
        }

        let season_name = season_entry.file_name().to_string_lossy().to_string();
        if is_ignored_nested_directory(&season_name) {
            continue;
        }

        let normalized_season = normalize_season_directory_name(&season_name);

        if normalized_season != season_name {
            let target_season = source_show_path.join(&normalized_season);
            nested.push((
                season_name.clone(),
                normalized_season,
                PlanOpType::RenameDir,
                season_path,
                target_season,
            ));
        }
    }

    Ok(nested)
}

fn build_tv_episode_file_operations(
    source_show_path: &Path,
) -> Result<Vec<(String, String, PlanOpType, PathBuf, PathBuf)>, std::io::Error> {
    let mut operations = Vec::new();

    for season_entry in std::fs::read_dir(source_show_path)? {
        let season_entry = season_entry?;
        let season_path = season_entry.path();
        if !season_path.is_dir() {
            continue;
        }

        let season_name = season_entry.file_name().to_string_lossy().to_string();
        if is_ignored_nested_directory(&season_name) {
            continue;
        }

        for episode_entry in std::fs::read_dir(&season_path)? {
            let episode_entry = episode_entry?;
            let episode_path = episode_entry.path();
            if !episode_path.is_file() {
                continue;
            }

            let episode_name = episode_entry.file_name().to_string_lossy().to_string();
            let normalized_episode = normalize_episode_filename_with_nfo(&season_path, &episode_name);
            if normalized_episode == episode_name {
                continue;
            }

            let target_episode = season_path.join(normalized_episode);
            operations.push((
                season_name.clone(),
                episode_name,
                PlanOpType::RenameFile,
                episode_path,
                target_episode,
            ));
        }
    }

    Ok(operations)
}

fn op_type_rank(op_type: PlanOpType) -> u8 {
    match op_type {
        PlanOpType::RenameFile | PlanOpType::MoveFile => 0,
        PlanOpType::RenameDir => 1,
        PlanOpType::DeleteFile => 2,
        PlanOpType::DeleteDir => 3,
    }
}

fn is_ignored_nested_directory(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(".trickplay")
}

fn parse_relative_component(value: &str) -> Result<PathBuf, ExecuteServiceError> {
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(preflight("path component must be relative"));
    }

    let mut components = path.components();
    let only_component = components.next();
    if components.next().is_some() {
        return Err(preflight("path component must be a single directory segment"));
    }

    if !matches!(only_component, Some(Component::Normal(_))) {
        return Err(preflight("path component contains invalid traversal or special segments"));
    }

    Ok(PathBuf::from(value))
}

fn preflight_validate(plan: &NormalizePlan, dry_run: bool) -> Result<(), ExecuteServiceError> {
    let canonical_base = Path::new(&plan.base_path)
        .canonicalize()
        .map_err(|_| preflight("base_path could not be resolved during preflight"))?;

    let mut targets = HashSet::new();

    for operation in &plan.operations {
        let source = Path::new(&operation.source_path);
        let target = Path::new(&operation.target_path);

        // Delete ops only need source validation.
        if matches!(operation.op_type, PlanOpType::DeleteDir | PlanOpType::DeleteFile) {
            ensure_absolute_under_base(&canonical_base, source, true)?;
            continue;
        }

        revalidate_operation_paths(&canonical_base, source, target)?;

        if !targets.insert(target.to_path_buf()) {
            return Err(preflight("multiple operations target the same path"));
        }

        if !source.exists() {
            return Err(preflight("source path does not exist"));
        }

        if !dry_run && target.exists() {
            let same_path = match (source.canonicalize(), target.canonicalize()) {
                (Ok(source), Ok(target)) => source == target,
                _ => false,
            };

            if !same_path {
                return Err(preflight("target path already exists"));
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
        return Err(preflight("operation path is not absolute"));
    }

    let parent = candidate
        .parent()
        .ok_or_else(|| preflight("operation path has no parent directory"))?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| preflight("operation parent path could not be resolved"))?;

    if !canonical_parent.starts_with(canonical_base) {
        return Err(preflight("operation path escapes base path"));
    }

    if must_exist && !candidate.exists() {
        return Err(preflight("required operation source path does not exist"));
    }

    if candidate.exists() {
        let canonical_candidate = candidate
            .canonicalize()
            .map_err(|_| preflight("operation candidate path could not be resolved"))?;
        if !canonical_candidate.starts_with(canonical_base) {
            return Err(preflight("operation candidate escapes base path"));
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

    let hash = Sha256::digest(canonical.as_bytes());
    Ok(format!("{:064x}", hash))
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
                normalize_requested_directory_name(&override_.normalized_name),
            )
        })
        .collect::<Vec<_>>();
    overrides.sort();

    let movie_rename_mode = request.movie_rename_mode.unwrap_or(MovieRenameMode::Clean);
    let mut remove_extra_file_types = request.remove_extra_file_types.clone().unwrap_or_default();
    remove_extra_file_types.sort_by_key(|value| match value {
        ExtraFileType::Media => 0,
        ExtraFileType::Nfo => 1,
        ExtraFileType::Srt => 2,
        ExtraFileType::Trickplay => 3,
    });
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
        movie_rename_mode,
        remove_extra_file_types,
        request.dry_run,
        overrides,
        resolutions,
    ))
    .map_err(|error| ExecuteServiceError::Internal(error.to_string()))?;

    let hash = Sha256::digest(payload.as_bytes());
    Ok(format!("{:064x}", hash))
}

fn map_repo_error(error: ExecuteRepositoryError) -> ExecuteServiceError {
    ExecuteServiceError::Internal(error.to_string())
}

fn canonicalize_base_path(path: &str) -> Result<String, ExecuteServiceError> {
    let canonical = Path::new(path)
        .canonicalize()
        .map_err(|_| preflight("base_path does not exist or is not accessible"))?;
    Ok(canonical.to_string_lossy().to_string())
}

fn preflight(reason: impl Into<String>) -> ExecuteServiceError {
    ExecuteServiceError::Preflight(reason.into())
}

fn map_plan_op_type(op_type: PlanOpType) -> &'static str {
    match op_type {
        PlanOpType::RenameDir => "rename_dir",
        PlanOpType::RenameFile => "rename_file",
        PlanOpType::MoveFile => "move_file",
        PlanOpType::DeleteFile => "delete_file",
        PlanOpType::DeleteDir => "delete_dir",
    }
}

fn validate_plan_for_media_type(plan: &NormalizePlan) -> Result<(), ExecuteServiceError> {
    if plan.media_type == "movie" {
        for op in &plan.operations {
            if op.op_type != PlanOpType::RenameFile {
                continue;
            }

            let source_parent = Path::new(&op.source_path).parent();
            let target_parent = Path::new(&op.target_path).parent();
            if source_parent != target_parent {
                return Err(preflight(
                    "movie plan contains file rename operations that leave the source directory",
                ));
            }
        }
    }
    Ok(())
}

fn missing_conflict_resolution_message(group: &ScanGroup) -> String {
    match preferred_source_directory_for_group(group) {
        Some(preferred_source) => format!(
            "duplicate group '{}' requires conflict resolution; recommended selected_source_directory is '{}'",
            group.normalized_name, preferred_source
        ),
        None => format!(
            "duplicate group '{}' requires conflict resolution",
            group.normalized_name
        ),
    }
}

fn preferred_source_directory_for_group(group: &ScanGroup) -> Option<&str> {
    if group.episodes.is_empty() {
        return None;
    }

    let mut preferred_source: Option<&str> = None;
    let mut overlapping_slot_count = 0;

    for slot in &group.episodes {
        if slot.candidates.len() <= 1 {
            continue;
        }

        overlapping_slot_count += 1;
        if slot.conflict {
            return None;
        }

        let slot_preferred = slot.auto_selected_source_directory.as_deref()?;
        match preferred_source {
            Some(existing) if existing != slot_preferred => return None,
            Some(_) => {}
            None => preferred_source = Some(slot_preferred),
        }
    }

    if overlapping_slot_count == 0 {
        return None;
    }

    preferred_source
}

fn append_trickplay_companion_operations(operations: Vec<PlanOperation>) -> Vec<PlanOperation> {
    let mut output = operations;
    let mut companion_ops = Vec::new();

    for operation in &output {
        let source_trickplay = format!("{}.trickplay", operation.source_path);
        let target_trickplay = format!("{}.trickplay", operation.target_path);
        let source_path = Path::new(&source_trickplay);
        if !source_path.is_dir() {
            continue;
        }

        companion_ops.push(PlanOperation {
            op_id: Uuid::nil(),
            op_index: 0,
            op_type: PlanOpType::RenameDir,
            source_path: source_trickplay,
            target_path: target_trickplay,
        });
    }

    output.extend(companion_ops);
    output
}

fn sort_plan_operations(operations: &mut [PlanOperation]) {
    operations.sort_by(|left, right| {
        let left_depth = Path::new(&left.source_path).components().count();
        let right_depth = Path::new(&right.source_path).components().count();

        right_depth
            .cmp(&left_depth)
            .then_with(|| op_type_rank(left.op_type).cmp(&op_type_rank(right.op_type)))
            .then_with(|| left.target_path.cmp(&right.target_path))
            .then_with(|| left.source_path.cmp(&right.source_path))
    });
}

fn resolve_movie_file_target_collisions(operations: &mut [PlanOperation]) {
    let mut target_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, operation) in operations.iter().enumerate() {
        if !matches!(operation.op_type, PlanOpType::RenameFile | PlanOpType::MoveFile) {
            continue;
        }

        target_groups
            .entry(operation.target_path.clone())
            .or_default()
            .push(index);
    }

    if !target_groups.values().any(|indices| indices.len() > 1) {
        return;
    }

    let mut taken_targets = operations
        .iter()
        .enumerate()
        .filter_map(|(index, operation)| {
            let group = target_groups.get(&operation.target_path)?;
            (group.len() == 1).then_some((index, operation.target_path.clone()))
        })
        .collect::<HashMap<usize, String>>();

    for indices in target_groups.values_mut() {
        if indices.len() <= 1 {
            continue;
        }

        indices.sort_by(|left, right| {
            movie_collision_priority(&operations[*left])
                .cmp(&movie_collision_priority(&operations[*right]))
                .then_with(|| operations[*left].source_path.cmp(&operations[*right].source_path))
        });

        let keeper = indices[0];
        taken_targets.insert(keeper, operations[keeper].target_path.clone());

        for index in indices.iter().copied().skip(1) {
            let fallback_target =
                allocate_unique_movie_collision_target(&operations[index], taken_targets.values());
            operations[index].target_path = fallback_target.clone();
            taken_targets.insert(index, fallback_target);
        }
    }
}

fn movie_collision_priority(operation: &PlanOperation) -> u8 {
    match operation.op_type {
        PlanOpType::RenameFile => 0,
        PlanOpType::MoveFile => 1,
        _ => 2,
    }
}

fn allocate_unique_movie_collision_target<'a>(
    operation: &PlanOperation,
    taken_targets: impl IntoIterator<Item = &'a String>,
) -> String {
    let taken = taken_targets
        .into_iter()
        .map(|path| path.as_str())
        .collect::<HashSet<_>>();

    let source_path = Path::new(&operation.source_path);
    let target_path = Path::new(&operation.target_path);
    let parent = target_path
        .parent()
        .expect("movie collision target should have parent");
    let source_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("duplicate");

    let original_target = parent.join(source_name).to_string_lossy().to_string();
    if !taken.contains(original_target.as_str()) && original_target != operation.target_path {
        return original_target;
    }

    let (stem, suffix) = split_movie_stem_and_suffix(source_name);
    for index in 2..1000 {
        let candidate = parent
            .join(format!("{stem} ({index}){suffix}"))
            .to_string_lossy()
            .to_string();
        if !taken.contains(candidate.as_str()) {
            return candidate;
        }
    }

    operation.target_path.clone()
}

fn is_episode_container_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };

    let lower = name.to_ascii_lowercase();
    if lower == "specials" {
        return true;
    }

    if lower.starts_with("season ") {
        return true;
    }

    lower.len() > 1 && lower.starts_with('s') && lower[1..].chars().all(|c| c.is_ascii_digit())
}

fn build_episode_file_rename_ops(
    season_dir: &Path,
) -> Result<Vec<(String, String, PlanOpType, PathBuf, PathBuf)>, std::io::Error> {
    let mut operations = Vec::new();

    for entry in std::fs::read_dir(season_dir)? {
        let entry = entry?;
        let source_path = entry.path();
        if !source_path.is_file() {
            continue;
        }

        let source_name = entry.file_name().to_string_lossy().to_string();
        let target_name = normalize_episode_filename_with_nfo(season_dir, &source_name);
        if target_name == source_name {
            continue;
        }

        operations.push((
            source_name.clone(),
            target_name.clone(),
            PlanOpType::RenameFile,
            source_path,
            season_dir.join(target_name),
        ));
    }

    Ok(operations)
}

fn normalize_requested_directory_name(value: &str) -> String {
    let normalized = normalize_directory_name(value);
    if normalized.is_empty() {
        value.trim().to_string()
    } else {
        normalized
    }
}

fn build_target_directory_name_for_source(
    source_directory: &str,
    group: &ScanGroup,
    overrides: &HashMap<&str, &str>,
    media_type: &str,
    movie_rename_mode: MovieRenameMode,
) -> String {
    let requested_base_name = overrides
        .get(source_directory)
        .copied()
        .unwrap_or(group.normalized_name.as_str());

    if media_type != "movie" {
        return normalize_requested_directory_name(requested_base_name);
    }

    build_movie_directory_name(requested_base_name, group, movie_rename_mode)
}

fn build_movie_directory_name(
    requested_base_name: &str,
    group: &ScanGroup,
    movie_rename_mode: MovieRenameMode,
) -> String {
    let base_name = normalize_requested_directory_name(requested_base_name);
    if movie_rename_mode == MovieRenameMode::Clean {
        return base_name;
    }

    let metadata_tags = collect_group_metadata_tags(group);
    append_metadata_tags(&base_name, &metadata_tags)
}

fn collect_group_metadata_tags(group: &ScanGroup) -> Vec<String> {
    let mut tmdb = Vec::new();
    let mut imdb = Vec::new();
    let mut tvdb = Vec::new();
    let mut other = Vec::new();

    for source_directory in &group.source_directories {
        for tag in extract_metadata_tags(source_directory) {
            if tmdb.contains(&tag) || imdb.contains(&tag) || tvdb.contains(&tag) || other.contains(&tag)
            {
                continue;
            }

            if tag.starts_with("tmdbid-") {
                tmdb.push(tag);
            } else if tag.starts_with("imdbid-") {
                imdb.push(tag);
            } else if tag.starts_with("tvdbid-") {
                tvdb.push(tag);
            } else {
                other.push(tag);
            }
        }
    }

    tmdb.extend(imdb);
    tmdb.extend(tvdb);
    tmdb.extend(other);
    tmdb
}

fn build_movie_file_rename_operations(
    movie_dir: &Path,
    target_movie_name: &str,
) -> Result<Vec<(String, String, PlanOpType, PathBuf, PathBuf)>, std::io::Error> {
    let mut operations = Vec::new();
    let comparable_movie_name = normalize_directory_name(target_movie_name);

    for entry in std::fs::read_dir(movie_dir)? {
        let entry = entry?;
        let source_path = entry.path();
        if !source_path.is_file() {
            continue;
        }

        let source_name = entry.file_name().to_string_lossy().to_string();
        let target_name = normalize_movie_related_filename(&source_name, target_movie_name);
        if target_name == source_name {
            continue;
        }

        let normalized_stem = normalize_movie_stem(&source_name);
        let is_movie_nfo_target = target_name.eq_ignore_ascii_case("movie.nfo");
        if !is_movie_nfo_target && normalized_stem.as_deref() != Some(comparable_movie_name.as_str()) {
            continue;
        }

        operations.push((
            source_name.clone(),
            target_name.clone(),
            PlanOpType::RenameFile,
            source_path,
            movie_dir.join(target_name),
        ));
    }

    Ok(operations)
}

fn should_remove_extra_movie_directory(
    source_path: &Path,
    remove_extra_file_types: &HashSet<ExtraFileType>,
) -> bool {
    remove_extra_file_types.contains(&ExtraFileType::Trickplay) && is_trickplay_directory(source_path)
}

fn should_remove_extra_movie_file(
    source_path: &Path,
    source_name: &str,
    remove_extra_file_types: &HashSet<ExtraFileType>,
) -> bool {
    if remove_extra_file_types.contains(&ExtraFileType::Trickplay)
        && source_path
            .ancestors()
            .skip(1)
            .any(is_trickplay_directory)
    {
        return true;
    }

    if remove_extra_file_types.contains(&ExtraFileType::Nfo)
        && source_name.to_ascii_lowercase().ends_with(".nfo")
    {
        return true;
    }

    if remove_extra_file_types.contains(&ExtraFileType::Srt) && is_subtitle_file(source_name) {
        return true;
    }

    remove_extra_file_types.contains(&ExtraFileType::Media) && is_movie_media_file(source_name)
}

fn normalize_movie_related_filename(filename: &str, target_movie_name: &str) -> String {
    if filename.eq_ignore_ascii_case("movie.nfo") {
        return "movie.nfo".to_string();
    }

    if is_protected_movie_filename(filename) {
        return filename.to_string();
    }

    let Some(normalized_stem) = normalize_movie_stem(filename) else {
        return filename.to_string();
    };
    let comparable_movie_name = normalize_directory_name(target_movie_name);
    if normalized_stem != comparable_movie_name {
        return filename.to_string();
    }

    let (_, suffix) = split_movie_stem_and_suffix(filename);
    if suffix.eq_ignore_ascii_case(".nfo") {
        return "movie.nfo".to_string();
    }

    format!("{target_movie_name}{suffix}")
}

fn normalize_movie_stem(filename: &str) -> Option<String> {
    let (stem, _) = split_movie_stem_and_suffix(filename);
    let normalized = normalize_directory_name(&stem.replace('.', " "));
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn is_protected_movie_filename(filename: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "poster.jpg"
            | "fanart.jpg"
            | "banner.jpg"
            | "clearlogo.png"
            | "landscape.jpg"
            | "folder.jpg"
            | "cover.jpg"
            | "cover.png"
    ) {
        return true;
    }

    lower.starts_with("backdrop") && lower.ends_with(".jpg")
}

fn split_movie_stem_and_suffix(input: &str) -> (&str, &str) {
    let lower = input.to_ascii_lowercase();

    if lower.ends_with(".sharky.json") {
        let split = input.len() - ".sharky.json".len();
        return (&input[..split], &input[split..]);
    }

    const LANGUAGE_EXTENSIONS: &[&str] = &[".srt", ".ass", ".ssa", ".sub", ".vtt", ".sup"];
    for extension in LANGUAGE_EXTENSIONS {
        if !lower.ends_with(extension) {
            continue;
        }

        if let Some(last_dot) = input.rfind('.') {
            let before_extension = &input[..last_dot];
            if let Some(lang_dot) = before_extension.rfind('.') {
                let lang = &before_extension[lang_dot + 1..];
                if (2..=8).contains(&lang.len())
                    && lang
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-')
                {
                    return (&input[..lang_dot], &input[lang_dot..]);
                }
            }
        }
    }

    if let Some(dot) = input.rfind('.') {
        return (&input[..dot], &input[dot..]);
    }

    (input, "")
}

fn is_movie_media_file(name: &str) -> bool {
    let Some(extension) = Path::new(name).extension().and_then(|value| value.to_str()) else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "mkv" | "mp4" | "avi" | "m4v" | "mov" | "wmv" | "ts"
    )
}

fn is_subtitle_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".srt", ".ass", ".ssa", ".sub", ".vtt", ".sup"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn is_trickplay_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|name| name.ends_with(".trickplay"))
        .unwrap_or(false)
}

fn build_delete_tree_operations(
    path: &Path,
) -> Result<Vec<(String, String, PlanOpType, PathBuf, PathBuf)>, std::io::Error> {
    let mut operations = Vec::new();

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let child_path = entry.path();
        if child_path.is_dir() {
            operations.extend(build_delete_tree_operations(&child_path)?);
        } else {
            let name = entry.file_name().to_string_lossy().to_string();
            operations.push((
                name.clone(),
                name,
                PlanOpType::DeleteFile,
                child_path.clone(),
                child_path,
            ));
        }
    }

    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    operations.push((
        name.clone(),
        name,
        PlanOpType::DeleteDir,
        path.to_path_buf(),
        path.to_path_buf(),
    ));

    Ok(operations)
}

fn summarize_job(job: &JobRecord) -> NormalizeJobSummary {
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

    NormalizeJobSummary {
        job_id: job.job_id,
        scan_id: job.scan_id,
        status: job.status,
        plan_hash: job.plan_hash.clone(),
        total_ops,
        completed_ops,
        failed_ops,
        error: job.error.clone(),
    }
}

fn build_pagination_meta(total: usize, page: usize, page_size: usize) -> PaginationMeta {
    let total_pages = if total == 0 {
        0
    } else {
        ((total - 1) / page_size) + 1
    };

    PaginationMeta {
        page,
        page_size,
        total,
        total_pages,
    }
}

fn page_bounds(total: usize, page: usize, page_size: usize) -> (usize, usize) {
    let start = page.saturating_sub(1).saturating_mul(page_size);
    if start >= total {
        return (total, total);
    }

    let end = std::cmp::min(start + page_size, total);
    (start, end)
}

fn current_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
