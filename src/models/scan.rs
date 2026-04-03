use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub total_shows: u64,
    pub total_episodes: u64,
    pub merges: u64,
    pub conflicts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanGroup {
    pub normalized_name: String,
    pub source_directories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub scan_id: Uuid,
    pub media_type: String,
    pub groups: Vec<ScanGroup>,
    pub summary: ScanSummary,
}
