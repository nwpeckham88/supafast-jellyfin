use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackSessionSummary {
    pub session_id: Uuid,
    pub scan_id: Uuid,
    pub media_type: String,
    pub base_path: String,
    pub status: String,
    pub total_ops: usize,
    pub completed_ops: usize,
    pub failed_ops: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackSessionsResponse {
    pub sessions: Vec<RollbackSessionSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RollbackPreviewRequest {
    pub session_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackOperationPreview {
    pub op_id: Uuid,
    pub op_index: usize,
    pub op_type: String,
    pub source_path: String,
    pub target_path: String,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPreviewResponse {
    pub session_id: Uuid,
    pub operation_count: usize,
    pub blocked_count: usize,
    pub operations: Vec<RollbackOperationPreview>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RollbackExecuteRequest {
    pub session_id: Uuid,
    pub idempotency_key: String,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackExecuteResponse {
    pub rollback_job_id: Uuid,
    pub session_id: Uuid,
    pub status: RollbackJobStatus,
    pub reverted_operations: usize,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RollbackJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}
