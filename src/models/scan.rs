use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::parsers::quality::QualityInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub total_shows: u64,
    pub total_episodes: u64,
    pub merges: u64,
    pub conflicts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanEpisodeCandidate {
    pub source_directory: String,
    pub relative_path: String,
    pub normalized_relative_path: String,
    pub season_number: u32,
    pub episode_number: u32,
    pub extension: String,
    pub file_size: u64,
    pub quality: QualityInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanEpisodeSlot {
    pub season_number: u32,
    pub episode_number: u32,
    pub candidates: Vec<ScanEpisodeCandidate>,
    pub auto_selected_source_directory: Option<String>,
    pub conflict: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanGroup {
    pub normalized_name: String,
    pub source_directories: Vec<String>,
    #[serde(default)]
    pub episode_count: u64,
    #[serde(default)]
    pub conflict_count: u64,
    #[serde(default)]
    pub episodes: Vec<ScanEpisodeSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub scan_id: Uuid,
    pub media_type: String,
    pub base_path: String,
    pub groups: Vec<ScanGroup>,
    pub summary: ScanSummary,
}
