use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    path::Path,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    models::{
        audit::AuditOpStatus,
        execute::NormalizeJobStatus,
        rollback::{
            RollbackExecuteRequest, RollbackExecuteResponse, RollbackJobStatus,
            RollbackOperationPreview, RollbackPaginationMeta, RollbackPreviewRequest,
            RollbackPreviewResponse, RollbackSessionSummary, RollbackSessionsResponse,
        },
    },
    repositories::execute_repo::{ExecuteRepository, ExecuteRepositoryError, JobRecord},
    safety::base_path_lock::BasePathLockManager,
};

#[derive(Debug, Error)]
pub enum RollbackServiceError {
    #[error("execute session not found")]
    SessionNotFound,
    #[error("execute session is not rollbackable")]
    SessionNotRollbackable,
    #[error("idempotency key reused with different payload")]
    IdempotencyConflict,
    #[error("preflight validation failed")]
    Preflight,
    #[error("internal rollback service error: {0}")]
    Internal(String),
}

#[async_trait]
pub trait RollbackService: Send + Sync {
    async fn list_sessions(
        &self,
        page: usize,
        page_size: usize,
    ) -> Result<RollbackSessionsResponse, RollbackServiceError>;

    async fn preview(
        &self,
        request: RollbackPreviewRequest,
    ) -> Result<RollbackPreviewResponse, RollbackServiceError>;

    async fn execute(
        &self,
        request: RollbackExecuteRequest,
    ) -> Result<RollbackExecuteResponse, RollbackServiceError>;
}

pub struct InMemoryRollbackService {
    execute_repository: Arc<dyn ExecuteRepository>,
    base_path_lock: Arc<BasePathLockManager>,
    rollback_state: Arc<RwLock<RollbackState>>,
}

#[derive(Default)]
struct RollbackState {
    jobs_by_id: HashMap<Uuid, RollbackJobRecord>,
    key_to_job_id: HashMap<String, Uuid>,
}

#[derive(Debug, Clone)]
struct RollbackJobRecord {
    rollback_job_id: Uuid,
    session_id: Uuid,
    idempotency_key: String,
    request_fingerprint: String,
    status: RollbackJobStatus,
    reverted_operations: usize,
}

enum ClaimRollbackResult {
    Created(RollbackJobRecord),
    Replayed(RollbackJobRecord),
    Conflict,
}

impl InMemoryRollbackService {
    pub fn new(
        execute_repository: Arc<dyn ExecuteRepository>,
        base_path_lock: Arc<BasePathLockManager>,
    ) -> Self {
        Self {
            execute_repository,
            base_path_lock,
            rollback_state: Arc::new(RwLock::new(RollbackState::default())),
        }
    }
}

#[async_trait]
impl RollbackService for InMemoryRollbackService {
    async fn list_sessions(
        &self,
        page: usize,
        page_size: usize,
    ) -> Result<RollbackSessionsResponse, RollbackServiceError> {
        if page == 0 || page_size == 0 {
            return Err(RollbackServiceError::Internal(
                "invalid pagination parameters".to_string(),
            ));
        }

        let mut sessions = self
            .execute_repository
            .list_jobs()
            .await
            .map_err(map_repo_error)?
            .into_iter()
            .filter(|job| {
                is_terminal_rollback_source(job)
                    && job
                        .entries
                        .iter()
                        .any(|entry| entry.status == AuditOpStatus::Succeeded)
            })
            .map(|job| {
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

                RollbackSessionSummary {
                    session_id: job.job_id,
                    scan_id: job.scan_id,
                    media_type: job.media_type,
                    base_path: job.base_path,
                    status: format!("{:?}", job.status).to_lowercase(),
                    total_ops,
                    completed_ops,
                    failed_ops,
                }
            })
            .collect::<Vec<_>>();

        sessions.sort_by(|left, right| {
            left.base_path
                .cmp(&right.base_path)
                .then_with(|| left.session_id.cmp(&right.session_id))
        });

        let total = sessions.len();
        let total_pages = if total == 0 {
            0
        } else {
            ((total - 1) / page_size) + 1
        };
        let start = page.saturating_sub(1).saturating_mul(page_size);
        let end = std::cmp::min(start + page_size, total);
        let paged = if start >= total {
            Vec::new()
        } else {
            sessions[start..end].to_vec()
        };

        Ok(RollbackSessionsResponse {
            sessions: paged,
            pagination: RollbackPaginationMeta {
                page,
                page_size,
                total,
                total_pages,
            },
        })
    }

    async fn preview(
        &self,
        request: RollbackPreviewRequest,
    ) -> Result<RollbackPreviewResponse, RollbackServiceError> {
        let session = find_session(self.execute_repository.as_ref(), request.session_id).await?;

        let operations = build_rollback_operations(&session)?;
        let blocked_count = operations
            .iter()
            .filter(|operation| operation.blocked_reason.is_some())
            .count();

        Ok(RollbackPreviewResponse {
            session_id: request.session_id,
            operation_count: operations.len(),
            blocked_count,
            operations,
        })
    }

    async fn execute(
        &self,
        mut request: RollbackExecuteRequest,
    ) -> Result<RollbackExecuteResponse, RollbackServiceError> {
        let normalized_key = request.idempotency_key.trim().to_string();
        if normalized_key.is_empty() {
            return Err(RollbackServiceError::Preflight);
        }
        if normalized_key.len() > 128 {
            return Err(RollbackServiceError::Preflight);
        }
        request.idempotency_key = normalized_key;

        let request_fingerprint = compute_request_fingerprint(&request)?;

        if let Some(existing) = find_existing_rollback(
            &self.rollback_state,
            request.session_id,
            &request.idempotency_key,
        )? {
            if existing.request_fingerprint != request_fingerprint {
                return Err(RollbackServiceError::IdempotencyConflict);
            }

            return Ok(RollbackExecuteResponse {
                rollback_job_id: existing.rollback_job_id,
                session_id: request.session_id,
                status: existing.status,
                reverted_operations: existing.reverted_operations,
                replayed: true,
            });
        }

        let session = find_session(self.execute_repository.as_ref(), request.session_id).await?;
        let operations = build_rollback_operations(&session)?;
        if operations.is_empty() {
            return Err(RollbackServiceError::SessionNotRollbackable);
        }

        if operations
            .iter()
            .any(|operation| operation.blocked_reason.is_some())
        {
            return Err(RollbackServiceError::Preflight);
        }

        let canonical_base = Path::new(&session.base_path)
            .canonicalize()
            .map_err(|_| RollbackServiceError::Preflight)?
            .to_string_lossy()
            .to_string();

        if !self.base_path_lock.try_acquire(&canonical_base).await {
            return Err(RollbackServiceError::Preflight);
        }

        let pending_record = RollbackJobRecord {
            rollback_job_id: Uuid::new_v4(),
            session_id: request.session_id,
            idempotency_key: request.idempotency_key,
            request_fingerprint,
            status: RollbackJobStatus::Pending,
            reverted_operations: 0,
        };
        let idempotency_key = pending_record.idempotency_key.clone();

        let claim = claim_rollback_job(
            &self.rollback_state,
            request.session_id,
            &idempotency_key,
            pending_record,
        )?;

        let result = match claim {
            ClaimRollbackResult::Conflict => {
                self.base_path_lock.release(&canonical_base).await;
                Err(RollbackServiceError::IdempotencyConflict)
            }
            ClaimRollbackResult::Replayed(existing) => {
                self.base_path_lock.release(&canonical_base).await;
                Ok(RollbackExecuteResponse {
                    rollback_job_id: existing.rollback_job_id,
                    session_id: request.session_id,
                    status: existing.status,
                    reverted_operations: existing.reverted_operations,
                    replayed: true,
                })
            }
            ClaimRollbackResult::Created(created) => {
                let run_result = run_rollback_job(
                    self.rollback_state.clone(),
                    created,
                    request.dry_run,
                    &canonical_base,
                    &operations,
                )
                .await;

                self.base_path_lock.release(&canonical_base).await;
                run_result
            }
        };
        result
    }
}

async fn run_rollback_job(
    rollback_state: Arc<RwLock<RollbackState>>,
    mut job_record: RollbackJobRecord,
    dry_run: bool,
    canonical_base: &str,
    operations: &[RollbackOperationPreview],
) -> Result<RollbackExecuteResponse, RollbackServiceError> {
    let rollback_job_id = job_record.rollback_job_id;

    job_record.status = RollbackJobStatus::Running;
    update_rollback_job(&rollback_state, job_record.clone())?;

    if dry_run {
        job_record.status = RollbackJobStatus::Completed;
        job_record.reverted_operations = operations.len();
        update_rollback_job(&rollback_state, job_record.clone())?;
        return Ok(RollbackExecuteResponse {
            rollback_job_id,
            session_id: job_record.session_id,
            status: job_record.status,
            reverted_operations: job_record.reverted_operations,
            replayed: false,
        });
    }

    let canonical_base = Path::new(canonical_base);
    let mut applied_operations: Vec<(String, String)> = Vec::new();
    for operation in operations {
        let source = Path::new(&operation.source_path);
        let target = Path::new(&operation.target_path);

        if classify_blocked_reason(canonical_base, source, target).is_some() {
            job_record.status = RollbackJobStatus::Failed;
            update_rollback_job(&rollback_state, job_record.clone())?;
            return Err(RollbackServiceError::Preflight);
        }

        let rename_result = tokio::task::spawn_blocking({
            let source = source.to_path_buf();
            let target = target.to_path_buf();
            move || std::fs::rename(source, target)
        })
        .await;

        match rename_result {
            Ok(Ok(())) => {
                applied_operations
                    .push((operation.source_path.clone(), operation.target_path.clone()));
                job_record.reverted_operations += 1;
                update_rollback_job(&rollback_state, job_record.clone())?;
            }
            Ok(Err(_)) | Err(_) => {
                for (previous_source, previous_target) in applied_operations.iter().rev() {
                    let _ = std::fs::rename(previous_target, previous_source);
                }
                job_record.status = RollbackJobStatus::Failed;
                update_rollback_job(&rollback_state, job_record.clone())?;
                return Err(RollbackServiceError::Preflight);
            }
        }
    }

    job_record.status = RollbackJobStatus::Completed;
    update_rollback_job(&rollback_state, job_record.clone())?;

    Ok(RollbackExecuteResponse {
        rollback_job_id,
        session_id: job_record.session_id,
        status: job_record.status,
        reverted_operations: job_record.reverted_operations,
        replayed: false,
    })
}

fn build_rollback_operations(
    session: &JobRecord,
) -> Result<Vec<RollbackOperationPreview>, RollbackServiceError> {
    let canonical_base = Path::new(&session.base_path)
        .canonicalize()
        .map_err(|_| RollbackServiceError::Preflight)?;

    let mut operations = session
        .entries
        .iter()
        .filter(|entry| entry.status == AuditOpStatus::Succeeded)
        .map(|entry| {
            let source_path = entry.target_path.clone();
            let target_path = entry.source_path.clone();

            let blocked_reason = classify_blocked_reason(
                &canonical_base,
                Path::new(&source_path),
                Path::new(&target_path),
            );

            RollbackOperationPreview {
                op_id: entry.op_id,
                op_index: entry.op_index,
                op_type: entry.op_type.clone(),
                source_path,
                target_path,
                blocked_reason,
            }
        })
        .collect::<Vec<_>>();

    operations.sort_by(|left, right| right.op_index.cmp(&left.op_index));
    Ok(operations)
}

fn classify_blocked_reason(canonical_base: &Path, source: &Path, target: &Path) -> Option<String> {
    if validate_under_base(canonical_base, source, true).is_err() {
        return Some("source path failed safety validation".to_string());
    }

    if validate_under_base(canonical_base, target, false).is_err() {
        return Some("target path failed safety validation".to_string());
    }

    if !source.exists() {
        return Some("source path does not exist".to_string());
    }

    if target.exists() {
        let same_path = match (source.canonicalize(), target.canonicalize()) {
            (Ok(source_path), Ok(target_path)) => source_path == target_path,
            _ => false,
        };

        if !same_path {
            return Some("target path already exists".to_string());
        }
    }

    None
}

fn validate_under_base(
    canonical_base: &Path,
    candidate: &Path,
    must_exist: bool,
) -> Result<(), RollbackServiceError> {
    if !candidate.is_absolute() {
        return Err(RollbackServiceError::Preflight);
    }

    let parent = candidate.parent().ok_or(RollbackServiceError::Preflight)?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| RollbackServiceError::Preflight)?;

    if !canonical_parent.starts_with(canonical_base) {
        return Err(RollbackServiceError::Preflight);
    }

    if must_exist && !candidate.exists() {
        return Err(RollbackServiceError::Preflight);
    }

    if candidate.exists() {
        let canonical_candidate = candidate
            .canonicalize()
            .map_err(|_| RollbackServiceError::Preflight)?;

        if !canonical_candidate.starts_with(canonical_base) {
            return Err(RollbackServiceError::Preflight);
        }
    }

    Ok(())
}

async fn find_session(
    execute_repository: &dyn ExecuteRepository,
    session_id: Uuid,
) -> Result<JobRecord, RollbackServiceError> {
    let jobs = execute_repository
        .list_jobs()
        .await
        .map_err(map_repo_error)?;
    let Some(session) = jobs.into_iter().find(|job| job.job_id == session_id) else {
        return Err(RollbackServiceError::SessionNotFound);
    };

    if !is_terminal_rollback_source(&session)
        || !session
            .entries
            .iter()
            .any(|entry| entry.status == AuditOpStatus::Succeeded)
    {
        return Err(RollbackServiceError::SessionNotRollbackable);
    }

    Ok(session)
}

fn claim_rollback_job(
    rollback_state: &RwLock<RollbackState>,
    session_id: Uuid,
    idempotency_key: &str,
    candidate: RollbackJobRecord,
) -> Result<ClaimRollbackResult, RollbackServiceError> {
    let mut state = rollback_state
        .write()
        .map_err(|_| RollbackServiceError::Internal("rollback state lock poisoned".to_string()))?;

    let key = rollback_key(session_id, idempotency_key);
    if let Some(existing_job_id) = state.key_to_job_id.get(&key) {
        let Some(existing) = state.jobs_by_id.get(existing_job_id).cloned() else {
            return Err(RollbackServiceError::Internal(
                "rollback idempotency state corrupted".to_string(),
            ));
        };

        if existing.request_fingerprint == candidate.request_fingerprint {
            return Ok(ClaimRollbackResult::Replayed(existing));
        }

        return Ok(ClaimRollbackResult::Conflict);
    }

    state.key_to_job_id.insert(key, candidate.rollback_job_id);
    state
        .jobs_by_id
        .insert(candidate.rollback_job_id, candidate.clone());
    Ok(ClaimRollbackResult::Created(candidate))
}

fn update_rollback_job(
    rollback_state: &RwLock<RollbackState>,
    record: RollbackJobRecord,
) -> Result<(), RollbackServiceError> {
    let mut state = rollback_state
        .write()
        .map_err(|_| RollbackServiceError::Internal("rollback state lock poisoned".to_string()))?;

    state.key_to_job_id.insert(
        rollback_key(record.session_id, &record.idempotency_key),
        record.rollback_job_id,
    );
    state.jobs_by_id.insert(record.rollback_job_id, record);
    Ok(())
}

fn find_existing_rollback(
    rollback_state: &RwLock<RollbackState>,
    session_id: Uuid,
    idempotency_key: &str,
) -> Result<Option<RollbackJobRecord>, RollbackServiceError> {
    let state = rollback_state
        .read()
        .map_err(|_| RollbackServiceError::Internal("rollback state lock poisoned".to_string()))?;

    let key = rollback_key(session_id, idempotency_key);
    let Some(job_id) = state.key_to_job_id.get(&key) else {
        return Ok(None);
    };

    Ok(state.jobs_by_id.get(job_id).cloned())
}

fn compute_request_fingerprint(
    request: &RollbackExecuteRequest,
) -> Result<String, RollbackServiceError> {
    let payload = serde_json::to_string(&(request.session_id, request.dry_run))
        .map_err(|error| RollbackServiceError::Internal(error.to_string()))?;

    let mut hasher = DefaultHasher::new();
    payload.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn rollback_key(session_id: Uuid, idempotency_key: &str) -> String {
    format!("{session_id}:{idempotency_key}")
}

fn map_repo_error(error: ExecuteRepositoryError) -> RollbackServiceError {
    RollbackServiceError::Internal(error.to_string())
}

fn is_terminal_rollback_source(session: &JobRecord) -> bool {
    matches!(
        session.status,
        NormalizeJobStatus::Completed | NormalizeJobStatus::Failed
    )
}
