use std::{collections::HashMap, sync::RwLock, time::SystemTime};

use async_trait::async_trait;
use sqlx::{
    types::chrono::{DateTime, Utc},
    PgPool, Row,
};
use thiserror::Error;
use uuid::Uuid;

use crate::models::scan::ScanResult;

#[derive(Debug, Clone)]
pub struct ScanSessionRecord {
    pub result: ScanResult,
    pub base_path: String,
    pub expires_at: SystemTime,
}

#[derive(Debug, Error)]
pub enum ScanRepositoryError {
    #[error("scan repository lock poisoned")]
    LockPoisoned,
    #[error("scan repository query failed: {0}")]
    Query(String),
    #[error("scan repository serialization failed: {0}")]
    Serialization(String),
}

#[async_trait]
pub trait ScanRepository: Send + Sync {
    async fn save_scan(&self, record: ScanSessionRecord) -> Result<(), ScanRepositoryError>;
    async fn get_scan(
        &self,
        scan_id: Uuid,
    ) -> Result<Option<ScanSessionRecord>, ScanRepositoryError>;
    async fn delete_scan(&self, scan_id: Uuid) -> Result<bool, ScanRepositoryError>;
    async fn prune_expired(&self, now: SystemTime) -> Result<(), ScanRepositoryError>;
}

#[derive(Default)]
pub struct InMemoryScanRepository {
    scans: RwLock<HashMap<Uuid, ScanSessionRecord>>,
}

impl InMemoryScanRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ScanRepository for InMemoryScanRepository {
    async fn save_scan(&self, record: ScanSessionRecord) -> Result<(), ScanRepositoryError> {
        let mut guard = self
            .scans
            .write()
            .map_err(|_| ScanRepositoryError::LockPoisoned)?;
        guard.insert(record.result.scan_id, record);
        Ok(())
    }

    async fn get_scan(
        &self,
        scan_id: Uuid,
    ) -> Result<Option<ScanSessionRecord>, ScanRepositoryError> {
        let guard = self
            .scans
            .read()
            .map_err(|_| ScanRepositoryError::LockPoisoned)?;
        Ok(guard.get(&scan_id).cloned())
    }

    async fn delete_scan(&self, scan_id: Uuid) -> Result<bool, ScanRepositoryError> {
        let mut guard = self
            .scans
            .write()
            .map_err(|_| ScanRepositoryError::LockPoisoned)?;
        Ok(guard.remove(&scan_id).is_some())
    }

    async fn prune_expired(&self, now: SystemTime) -> Result<(), ScanRepositoryError> {
        let mut guard = self
            .scans
            .write()
            .map_err(|_| ScanRepositoryError::LockPoisoned)?;
        guard.retain(|_, record| record.expires_at > now);
        Ok(())
    }
}

pub struct PgScanRepository {
    pool: PgPool,
}

impl PgScanRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScanRepository for PgScanRepository {
    async fn save_scan(&self, record: ScanSessionRecord) -> Result<(), ScanRepositoryError> {
        let result_json = serde_json::to_value(&record.result)
            .map_err(|error| ScanRepositoryError::Serialization(error.to_string()))?;
        let summary_json = serde_json::to_value(&record.result.summary)
            .map_err(|error| ScanRepositoryError::Serialization(error.to_string()))?;
        let expires_at: DateTime<Utc> = record.expires_at.into();

        sqlx::query(
            "INSERT INTO scan_sessions \
             (id, media_type, base_path, status, expires_at, summary_json, result_json) \
             VALUES ($1, $2, $3, 'completed', $4, $5, $6) \
             ON CONFLICT (id) DO UPDATE SET \
               media_type = EXCLUDED.media_type, \
               base_path = EXCLUDED.base_path, \
               status = EXCLUDED.status, \
               expires_at = EXCLUDED.expires_at, \
               summary_json = EXCLUDED.summary_json, \
               result_json = EXCLUDED.result_json",
        )
        .bind(record.result.scan_id)
        .bind(&record.result.media_type)
        .bind(&record.base_path)
        .bind(expires_at)
        .bind(summary_json)
        .bind(result_json)
        .execute(&self.pool)
        .await
        .map_err(|error| ScanRepositoryError::Query(error.to_string()))?;

        Ok(())
    }

    async fn get_scan(
        &self,
        scan_id: Uuid,
    ) -> Result<Option<ScanSessionRecord>, ScanRepositoryError> {
        let row = sqlx::query(
            "SELECT result_json, base_path, expires_at \
             FROM scan_sessions \
             WHERE id = $1 \
             LIMIT 1",
        )
        .bind(scan_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| ScanRepositoryError::Query(error.to_string()))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let result_json: serde_json::Value = row
            .try_get("result_json")
            .map_err(|error| ScanRepositoryError::Query(error.to_string()))?;
        let base_path: String = row
            .try_get("base_path")
            .map_err(|error| ScanRepositoryError::Query(error.to_string()))?;
        let expires_at: DateTime<Utc> = row
            .try_get("expires_at")
            .map_err(|error| ScanRepositoryError::Query(error.to_string()))?;
        let result: ScanResult = serde_json::from_value(result_json)
            .map_err(|error| ScanRepositoryError::Serialization(error.to_string()))?;

        Ok(Some(ScanSessionRecord {
            result,
            base_path,
            expires_at: expires_at.into(),
        }))
    }

    async fn delete_scan(&self, scan_id: Uuid) -> Result<bool, ScanRepositoryError> {
        let rows_affected = sqlx::query(
            "DELETE FROM scan_sessions \
             WHERE id = $1 \
               AND NOT EXISTS ( \
                   SELECT 1 FROM audit_sessions WHERE audit_sessions.scan_id = scan_sessions.id \
               )",
        )
            .bind(scan_id)
            .execute(&self.pool)
            .await
            .map_err(|error| ScanRepositoryError::Query(error.to_string()))?
            .rows_affected();
        Ok(rows_affected > 0)
    }

    async fn prune_expired(&self, now: SystemTime) -> Result<(), ScanRepositoryError> {
        let now: DateTime<Utc> = now.into();
        sqlx::query(
            "DELETE FROM scan_sessions \
             WHERE expires_at <= $1 \
               AND NOT EXISTS ( \
                   SELECT 1 FROM audit_sessions WHERE audit_sessions.scan_id = scan_sessions.id \
               )",
        )
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|error| ScanRepositoryError::Query(error.to_string()))?;
        Ok(())
    }
}
