use std::{collections::HashMap, sync::RwLock};

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::models::{audit::AuditEntry, execute::NormalizeJobStatus};

#[derive(Debug, Clone)]
pub struct JobRecord {
    pub job_id: Uuid,
    pub scan_id: Uuid,
    pub idempotency_key: String,
    pub media_type: String,
    pub base_path: String,
    pub request_fingerprint: String,
    pub plan_hash: String,
    pub status: NormalizeJobStatus,
    pub error: Option<String>,
    pub entries: Vec<AuditEntry>,
}

#[derive(Debug, Error)]
pub enum ExecuteRepositoryError {
    #[error("execute repository lock poisoned")]
    LockPoisoned,
}

#[derive(Debug, Clone)]
pub enum ClaimJobResult {
    Created(JobRecord),
    Replayed(JobRecord),
    Conflict,
}

#[async_trait]
pub trait ExecuteRepository: Send + Sync {
    async fn get_by_idempotency_key(
        &self,
        scan_id: Uuid,
        idempotency_key: &str,
    ) -> Result<Option<JobRecord>, ExecuteRepositoryError>;

    async fn claim_job(&self, candidate: JobRecord) -> Result<ClaimJobResult, ExecuteRepositoryError>;

    async fn get_job(&self, job_id: Uuid) -> Result<Option<JobRecord>, ExecuteRepositoryError>;

    async fn update_job(&self, record: JobRecord) -> Result<(), ExecuteRepositoryError>;
}

#[derive(Default)]
pub struct InMemoryExecuteRepository {
    state: RwLock<RepositoryState>,
}

#[derive(Default)]
struct RepositoryState {
    jobs_by_id: HashMap<Uuid, JobRecord>,
    key_to_job_id: HashMap<String, Uuid>,
}

impl InMemoryExecuteRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ExecuteRepository for InMemoryExecuteRepository {
    async fn get_by_idempotency_key(
        &self,
        scan_id: Uuid,
        idempotency_key: &str,
    ) -> Result<Option<JobRecord>, ExecuteRepositoryError> {
        let guard = self
            .state
            .read()
            .map_err(|_| ExecuteRepositoryError::LockPoisoned)?;
        let key = composite_key(scan_id, idempotency_key);
        let maybe_job_id = guard.key_to_job_id.get(&key).cloned();
        let Some(job_id) = maybe_job_id else {
            return Ok(None);
        };
        Ok(guard.jobs_by_id.get(&job_id).cloned())
    }

    async fn claim_job(&self, candidate: JobRecord) -> Result<ClaimJobResult, ExecuteRepositoryError> {
        let mut guard = self
            .state
            .write()
            .map_err(|_| ExecuteRepositoryError::LockPoisoned)?;
        let maybe_existing = guard
            .key_to_job_id
            .get(&composite_key(candidate.scan_id, &candidate.idempotency_key))
            .and_then(|job_id| guard.jobs_by_id.get(job_id))
            .cloned();

        if let Some(existing) = maybe_existing {
            if existing.request_fingerprint == candidate.request_fingerprint {
                return Ok(ClaimJobResult::Replayed(existing));
            }
            return Ok(ClaimJobResult::Conflict);
        }

        guard
            .key_to_job_id
            .insert(
                composite_key(candidate.scan_id, &candidate.idempotency_key),
                candidate.job_id,
            );
        guard.jobs_by_id.insert(candidate.job_id, candidate.clone());
        Ok(ClaimJobResult::Created(candidate))
    }

    async fn get_job(&self, job_id: Uuid) -> Result<Option<JobRecord>, ExecuteRepositoryError> {
        let guard = self
            .state
            .read()
            .map_err(|_| ExecuteRepositoryError::LockPoisoned)?;
        Ok(guard.jobs_by_id.get(&job_id).cloned())
    }

    async fn update_job(&self, record: JobRecord) -> Result<(), ExecuteRepositoryError> {
        let mut guard = self
            .state
            .write()
            .map_err(|_| ExecuteRepositoryError::LockPoisoned)?;

        let key = composite_key(record.scan_id, &record.idempotency_key);
        if !guard.key_to_job_id.contains_key(&key) {
            guard
                .key_to_job_id
            .insert(key, record.job_id);
        }
        guard.jobs_by_id.insert(record.job_id, record);

        Ok(())
    }
}

fn composite_key(scan_id: Uuid, idempotency_key: &str) -> String {
    format!("{scan_id}:{idempotency_key}")
}
