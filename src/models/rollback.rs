use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPreview {
    pub session_id: Uuid,
    pub operation_count: u64,
    pub blocked_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRequest {
    pub session_id: Uuid,
    pub idempotency_key: String,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResult {
    pub session_id: Uuid,
    pub status: String,
    pub reverted_operations: u64,
}
