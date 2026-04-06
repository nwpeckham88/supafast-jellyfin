use std::{collections::HashMap, sync::RwLock};

use async_trait::async_trait;
use sqlx::{
    types::chrono::{DateTime, Utc},
    PgPool, Row,
};
use thiserror::Error;
use uuid::Uuid;

use crate::models::{
    audit::{AuditEntry, AuditOpStatus},
    execute::NormalizeJobStatus,
};

#[derive(Debug, Clone)]
pub struct JobRecord {
    pub job_id: Uuid,
    pub created_at_ms: u128,
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
    #[error("execute repository query failed: {0}")]
    Query(String),
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

    async fn claim_job(
        &self,
        candidate: JobRecord,
    ) -> Result<ClaimJobResult, ExecuteRepositoryError>;

    async fn get_job(&self, job_id: Uuid) -> Result<Option<JobRecord>, ExecuteRepositoryError>;

    async fn list_jobs(&self) -> Result<Vec<JobRecord>, ExecuteRepositoryError>;

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

    async fn claim_job(
        &self,
        candidate: JobRecord,
    ) -> Result<ClaimJobResult, ExecuteRepositoryError> {
        let mut guard = self
            .state
            .write()
            .map_err(|_| ExecuteRepositoryError::LockPoisoned)?;
        let maybe_existing = guard
            .key_to_job_id
            .get(&composite_key(
                candidate.scan_id,
                &candidate.idempotency_key,
            ))
            .and_then(|job_id| guard.jobs_by_id.get(job_id))
            .cloned();

        if let Some(existing) = maybe_existing {
            if existing.request_fingerprint == candidate.request_fingerprint {
                return Ok(ClaimJobResult::Replayed(existing));
            }
            return Ok(ClaimJobResult::Conflict);
        }

        guard.key_to_job_id.insert(
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

    async fn list_jobs(&self) -> Result<Vec<JobRecord>, ExecuteRepositoryError> {
        let guard = self
            .state
            .read()
            .map_err(|_| ExecuteRepositoryError::LockPoisoned)?;

        Ok(guard.jobs_by_id.values().cloned().collect())
    }

    async fn update_job(&self, record: JobRecord) -> Result<(), ExecuteRepositoryError> {
        let mut guard = self
            .state
            .write()
            .map_err(|_| ExecuteRepositoryError::LockPoisoned)?;

        let key = composite_key(record.scan_id, &record.idempotency_key);
        if !guard.key_to_job_id.contains_key(&key) {
            guard.key_to_job_id.insert(key, record.job_id);
        }
        guard.jobs_by_id.insert(record.job_id, record);

        Ok(())
    }
}

fn composite_key(scan_id: Uuid, idempotency_key: &str) -> String {
    format!("{scan_id}:{idempotency_key}")
}

pub struct PgExecuteRepository {
    pool: PgPool,
}

impl PgExecuteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecuteRepository for PgExecuteRepository {
    async fn get_by_idempotency_key(
        &self,
        scan_id: Uuid,
        idempotency_key: &str,
    ) -> Result<Option<JobRecord>, ExecuteRepositoryError> {
        let row = sqlx::query(
            "SELECT id, created_at, media_type, base_path, scan_id, idempotency_key, \
                    request_fingerprint, plan_hash, status, error \
             FROM audit_sessions \
             WHERE scan_id = $1 AND idempotency_key = $2 \
             LIMIT 1",
        )
        .bind(scan_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let job = row_to_job_record(&row)?;
        let entries = fetch_entries(&self.pool, job.job_id).await?;
        Ok(Some(JobRecord { entries, ..job }))
    }

    async fn claim_job(
        &self,
        candidate: JobRecord,
    ) -> Result<ClaimJobResult, ExecuteRepositoryError> {
        // Try to find existing session with same idempotency_key
        if let Some(existing) = self
            .get_by_idempotency_key(candidate.scan_id, &candidate.idempotency_key)
            .await?
        {
            if existing.request_fingerprint == candidate.request_fingerprint {
                return Ok(ClaimJobResult::Replayed(existing));
            }
            return Ok(ClaimJobResult::Conflict);
        }

        // Insert new audit_session
        let status_str = job_status_to_str(candidate.status);
        sqlx::query(
            "INSERT INTO audit_sessions \
             (id, media_type, base_path, scan_id, idempotency_key, \
              request_fingerprint, plan_hash, status, error) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(candidate.job_id)
        .bind(&candidate.media_type)
        .bind(&candidate.base_path)
        .bind(candidate.scan_id)
        .bind(&candidate.idempotency_key)
        .bind(&candidate.request_fingerprint)
        .bind(&candidate.plan_hash)
        .bind(status_str)
        .bind(&candidate.error)
        .execute(&self.pool)
        .await
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;

        // Insert audit entries
        for entry in &candidate.entries {
            insert_entry(&self.pool, &entry).await?;
        }

        Ok(ClaimJobResult::Created(candidate))
    }

    async fn get_job(&self, job_id: Uuid) -> Result<Option<JobRecord>, ExecuteRepositoryError> {
        let row = sqlx::query(
            "SELECT id, created_at, media_type, base_path, scan_id, idempotency_key, \
                    request_fingerprint, plan_hash, status, error \
             FROM audit_sessions \
             WHERE id = $1 \
             LIMIT 1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let job = row_to_job_record(&row)?;
        let entries = fetch_entries(&self.pool, job.job_id).await?;
        Ok(Some(JobRecord { entries, ..job }))
    }

    async fn list_jobs(&self) -> Result<Vec<JobRecord>, ExecuteRepositoryError> {
        let rows = sqlx::query(
            "SELECT id, created_at, media_type, base_path, scan_id, idempotency_key, \
                    request_fingerprint, plan_hash, status, error \
             FROM audit_sessions \
             ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;

        let mut jobs = Vec::with_capacity(rows.len());
        for row in &rows {
            let job = row_to_job_record(row)?;
            let entries = fetch_entries(&self.pool, job.job_id).await?;
            jobs.push(JobRecord { entries, ..job });
        }
        Ok(jobs)
    }

    async fn update_job(&self, record: JobRecord) -> Result<(), ExecuteRepositoryError> {
        let status_str = job_status_to_str(record.status);

        // Count dir/file/merge ops
        let dir_count = record
            .entries
            .iter()
            .filter(|e| e.op_type == "rename_dir" && e.status == AuditOpStatus::Succeeded)
            .count() as i32;
        let file_count = record
            .entries
            .iter()
            .filter(|e| {
                (e.op_type == "rename_file" || e.op_type == "move_file")
                    && e.status == AuditOpStatus::Succeeded
            })
            .count() as i32;
        let merge_count = record
            .entries
            .iter()
            .filter(|e| e.op_type == "move_file" && e.status == AuditOpStatus::Succeeded)
            .count() as i32;

        let completed_at: Option<DateTime<Utc>> =
            if matches!(record.status, NormalizeJobStatus::Completed | NormalizeJobStatus::Failed) {
                Some(Utc::now())
            } else {
                None
            };
        let started_at: Option<DateTime<Utc>> =
            if !matches!(record.status, NormalizeJobStatus::Pending) {
                Some(Utc::now())
            } else {
                None
            };

        sqlx::query(
            "UPDATE audit_sessions SET \
                status = $2, error = $3, \
                dir_count = $4, file_count = $5, merge_count = $6, \
                started_at = COALESCE(started_at, $7), \
                completed_at = COALESCE($8, completed_at) \
             WHERE id = $1",
        )
        .bind(record.job_id)
        .bind(status_str)
        .bind(&record.error)
        .bind(dir_count)
        .bind(file_count)
        .bind(merge_count)
        .bind(started_at)
        .bind(completed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;

        // Upsert entries (update status + error_message)
        for entry in &record.entries {
            let status_str = audit_status_to_str(entry.status);
            sqlx::query(
                "UPDATE audit_entries SET status = $2, error_message = $3 \
                 WHERE id = $1",
            )
            .bind(entry.op_id)
            .bind(status_str)
            .bind(&entry.error_message)
            .execute(&self.pool)
            .await
            .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
        }

        Ok(())
    }
}

fn row_to_job_record(row: &sqlx::postgres::PgRow) -> Result<JobRecord, ExecuteRepositoryError> {
    let job_id: Uuid = row
        .try_get("id")
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
    let created_at: DateTime<Utc> = row
        .try_get("created_at")
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
    let media_type: String = row
        .try_get("media_type")
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
    let base_path: String = row
        .try_get("base_path")
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
    let scan_id: Option<Uuid> = row
        .try_get("scan_id")
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
    let idempotency_key: Option<String> = row
        .try_get("idempotency_key")
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
    let request_fingerprint: Option<String> = row
        .try_get("request_fingerprint")
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
    let plan_hash: Option<String> = row
        .try_get("plan_hash")
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
    let status_str: String = row
        .try_get("status")
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
    let error: Option<String> = row
        .try_get("error")
        .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;

    let status = str_to_job_status(&status_str);
    let created_at_ms = created_at.timestamp_millis() as u128;

    Ok(JobRecord {
        job_id,
        created_at_ms,
        scan_id: scan_id.unwrap_or(Uuid::nil()),
        idempotency_key: idempotency_key.unwrap_or_default(),
        media_type,
        base_path,
        request_fingerprint: request_fingerprint.unwrap_or_default(),
        plan_hash: plan_hash.unwrap_or_default(),
        status,
        error,
        entries: Vec::new(),
    })
}

async fn fetch_entries(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<AuditEntry>, ExecuteRepositoryError> {
    let rows = sqlx::query(
        "SELECT id, op_index, op_type, source_path, target_path, file_size, status, error_message \
         FROM audit_entries \
         WHERE session_id = $1 \
         ORDER BY op_index ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;

    let mut entries = Vec::with_capacity(rows.len());
    for row in &rows {
        let op_id: Uuid = row
            .try_get("id")
            .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
        let op_index: i32 = row
            .try_get("op_index")
            .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
        let op_type: String = row
            .try_get("op_type")
            .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
        let source_path: String = row
            .try_get("source_path")
            .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
        let target_path: String = row
            .try_get("target_path")
            .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
        let file_size: Option<i64> = row
            .try_get("file_size")
            .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
        let status_str: String = row
            .try_get("status")
            .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;
        let error_message: Option<String> = row
            .try_get("error_message")
            .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;

        entries.push(AuditEntry {
            op_id,
            op_index: op_index as usize,
            session_id,
            op_type,
            source_path,
            target_path,
            file_size: file_size.map(|v| v as u64),
            status: str_to_audit_status(&status_str),
            error_message,
        });
    }
    Ok(entries)
}

async fn insert_entry(pool: &PgPool, entry: &AuditEntry) -> Result<(), ExecuteRepositoryError> {
    let status_str = audit_status_to_str(entry.status);
    let file_size = entry.file_size.map(|v| v as i64);

    sqlx::query(
        "INSERT INTO audit_entries \
         (id, session_id, op_index, op_type, source_path, target_path, file_size, status, error_message) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(entry.op_id)
    .bind(entry.session_id)
    .bind(entry.op_index as i32)
    .bind(&entry.op_type)
    .bind(&entry.source_path)
    .bind(&entry.target_path)
    .bind(file_size)
    .bind(status_str)
    .bind(&entry.error_message)
    .execute(pool)
    .await
    .map_err(|e| ExecuteRepositoryError::Query(e.to_string()))?;

    Ok(())
}

fn job_status_to_str(status: NormalizeJobStatus) -> &'static str {
    match status {
        NormalizeJobStatus::Pending | NormalizeJobStatus::Running => "running",
        NormalizeJobStatus::Completed => "completed",
        NormalizeJobStatus::Failed => "failed",
    }
}

fn str_to_job_status(s: &str) -> NormalizeJobStatus {
    match s {
        "completed" => NormalizeJobStatus::Completed,
        "failed" => NormalizeJobStatus::Failed,
        "rolled_back" => NormalizeJobStatus::Completed,
        _ => NormalizeJobStatus::Running,
    }
}

fn audit_status_to_str(status: AuditOpStatus) -> &'static str {
    match status {
        AuditOpStatus::Pending => "pending",
        AuditOpStatus::Running => "running",
        AuditOpStatus::Succeeded => "succeeded",
        AuditOpStatus::Failed => "failed",
    }
}

fn str_to_audit_status(s: &str) -> AuditOpStatus {
    match s {
        "succeeded" => AuditOpStatus::Succeeded,
        "failed" => AuditOpStatus::Failed,
        "running" => AuditOpStatus::Running,
        _ => AuditOpStatus::Pending,
    }
}
