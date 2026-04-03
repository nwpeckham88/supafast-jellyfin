use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanOpType {
    RenameDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanOperation {
    pub op_id: Uuid,
    pub op_index: usize,
    pub op_type: PlanOpType,
    pub source_path: String,
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizePlan {
    pub plan_hash: String,
    pub scan_id: Uuid,
    pub idempotency_key: String,
    pub base_path: String,
    pub media_type: String,
    pub operations: Vec<PlanOperation>,
}
