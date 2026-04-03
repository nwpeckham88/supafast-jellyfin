use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::audit::AuditEntry;

#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteNormalizeRequest {
    pub scan_id: Uuid,
    pub idempotency_key: String,
    pub media_type: String,
    pub base_path: String,
    pub name_overrides: Vec<NameOverride>,
    pub conflict_resolutions: Vec<ConflictResolution>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NameOverride {
    pub source_directory: String,
    pub normalized_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConflictResolution {
    pub normalized_name: String,
    pub selected_source_directory: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecuteNormalizeResponse {
    pub job_id: Uuid,
    pub scan_id: Uuid,
    pub status: NormalizeJobStatus,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NormalizeJobResponse {
    pub job_id: Uuid,
    pub scan_id: Uuid,
    pub status: NormalizeJobStatus,
    pub plan_hash: String,
    pub total_ops: usize,
    pub completed_ops: usize,
    pub failed_ops: usize,
    pub error: Option<String>,
    pub entries: Vec<AuditEntry>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NormalizeJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}
