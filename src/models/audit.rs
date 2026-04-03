use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditOpStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub op_id: Uuid,
    pub op_index: usize,
    pub session_id: Uuid,
    pub op_type: String,
    pub source_path: String,
    pub target_path: String,
    pub file_size: Option<u64>,
    pub status: AuditOpStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSession {
    pub id: Uuid,
    pub media_type: String,
    pub base_path: String,
    pub scan_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
    pub status: String,
    pub dir_count: u64,
    pub file_count: u64,
    pub merge_count: u64,
}
