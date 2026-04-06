use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    models::scan::{
        ScanEpisodeCandidate, ScanEpisodeSlot, ScanGroup, ScanResult, ScanSummary,
    },
    parsers::{
        directory::normalize_directory_name,
        episode::{normalize_episode_filename_with_nfo, parse_episode_identity},
        quality::extract_quality_info,
        season::normalize_season_directory_name,
    },
    repositories::scan_repo::{InMemoryScanRepository, ScanRepository, ScanSessionRecord},
};

#[derive(Debug, Clone)]
struct SourceDirectoryRecord {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct NewScanRequest {
    pub media_type: String,
    pub base_path: String,
    pub ttl_minutes: u64,
}

#[derive(Debug, Error)]
pub enum ScanServiceError {
    #[error("scan session not found")]
    NotFound,
    #[error("internal scan service error: {0}")]
    Internal(String),
}

#[async_trait]
pub trait ScanService: Send + Sync {
    async fn create_scan(&self, request: NewScanRequest) -> Result<ScanResult, ScanServiceError>;
    async fn get_scan(&self, scan_id: Uuid) -> Result<Option<ScanResult>, ScanServiceError>;
    async fn expire_scan(&self, scan_id: Uuid) -> Result<bool, ScanServiceError>;
}

pub struct InMemoryScanService {
    repository: Arc<dyn ScanRepository>,
    follow_symlinks: bool,
}

impl InMemoryScanService {
    pub fn new() -> Self {
        Self {
            repository: Arc::new(InMemoryScanRepository::new()),
            follow_symlinks: false,
        }
    }

    pub fn with_repository(repository: Arc<dyn ScanRepository>) -> Self {
        Self {
            repository,
            follow_symlinks: false,
        }
    }

    pub fn with_follow_symlinks(mut self, follow_symlinks: bool) -> Self {
        self.follow_symlinks = follow_symlinks;
        self
    }
}

#[async_trait]
impl ScanService for InMemoryScanService {
    async fn create_scan(&self, request: NewScanRequest) -> Result<ScanResult, ScanServiceError> {
        let scan_id = Uuid::new_v4();
        let groups = build_groups_from_base_path(
            Path::new(&request.base_path),
            &request.media_type,
            self.follow_symlinks,
        )?;
        let ttl_secs = request
            .ttl_minutes
            .checked_mul(60)
            .ok_or_else(|| ScanServiceError::Internal("scan ttl overflow".to_string()))?;
        let ttl = Duration::from_secs(ttl_secs);
        let expires_at = SystemTime::now()
            .checked_add(ttl)
            .ok_or_else(|| ScanServiceError::Internal("scan ttl overflow".to_string()))?;
        let total_source_directories = groups
            .iter()
            .map(|group| group.source_directories.len() as u64)
            .sum::<u64>();
        let total_shows = groups.len() as u64;
        let total_episodes = groups.iter().map(|group| group.episode_count).sum::<u64>();
        let total_conflicts = groups.iter().map(|group| group.conflict_count).sum::<u64>();
        let scan_result = ScanResult {
            scan_id,
            media_type: request.media_type,
            base_path: request.base_path.clone(),
            groups,
            summary: ScanSummary {
                total_shows,
                total_episodes,
                merges: total_source_directories.saturating_sub(total_shows),
                conflicts: total_conflicts,
            },
        };

        self.repository
            .prune_expired(SystemTime::now())
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))?;
        self.repository
            .save_scan(ScanSessionRecord {
                result: scan_result.clone(),
                base_path: request.base_path,
                expires_at,
            })
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))?;
        Ok(scan_result)
    }

    async fn get_scan(&self, scan_id: Uuid) -> Result<Option<ScanResult>, ScanServiceError> {
        self.repository
            .prune_expired(SystemTime::now())
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))?;
        let Some(stored) = self
            .repository
            .get_scan(scan_id)
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))?
        else {
            return Ok(None);
        };

        if SystemTime::now() >= stored.expires_at {
            self.repository
                .delete_scan(scan_id)
                .await
                .map_err(|error| ScanServiceError::Internal(error.to_string()))?;
            return Ok(None);
        }

        Ok(Some(stored.result.clone()))
    }

    async fn expire_scan(&self, scan_id: Uuid) -> Result<bool, ScanServiceError> {
        self.repository
            .prune_expired(SystemTime::now())
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))?;
        self.repository
            .delete_scan(scan_id)
            .await
            .map_err(|error| ScanServiceError::Internal(error.to_string()))
    }
}

fn build_groups_from_base_path(
    base_path: &Path,
    media_type: &str,
    follow_symlinks: bool,
) -> Result<Vec<ScanGroup>, ScanServiceError> {
    let entries = match fs::read_dir(base_path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(ScanServiceError::Internal(format!(
                "failed to read base_path {}: {error}",
                base_path.display()
            )));
        }
    };

    let mut grouped: HashMap<String, Vec<SourceDirectoryRecord>> = HashMap::new();

    for entry in entries {
        let entry = entry.map_err(|error| {
            ScanServiceError::Internal(format!(
                "failed to inspect directory entry in {}: {error}",
                base_path.display()
            ))
        })?;

        let path = entry.path();
        let is_dir = if follow_symlinks {
            path.is_dir()
        } else {
            // Use symlink_metadata to avoid following symlinks
            path.symlink_metadata()
                .map(|m| m.is_dir())
                .unwrap_or(false)
        };
        if !is_dir {
            continue;
        }

        let source_directory = entry.file_name().to_string_lossy().into_owned();
        if is_ignored_scan_directory(&source_directory) {
            continue;
        }

        let normalized_name = normalize_directory_name(&source_directory);
        grouped
            .entry(normalized_name)
            .or_default()
            .push(SourceDirectoryRecord {
                name: source_directory,
                path,
            });
    }

    let mut groups = grouped
        .into_iter()
        .map(|(normalized_name, mut source_directories)| {
            source_directories.sort_by(|left, right| left.name.cmp(&right.name));
            let source_directory_names = source_directories
                .iter()
                .map(|directory| directory.name.clone())
                .collect::<Vec<_>>();
            let (episodes, episode_count, conflict_count) = if media_type == "tv" {
                let episodes = inspect_tv_group(&source_directories)?;
                let episode_count = episodes.len() as u64;
                let conflict_count = episodes.iter().filter(|slot| slot.conflict).count() as u64;
                (episodes, episode_count, conflict_count)
            } else {
                (Vec::new(), 0, 0)
            };
            Ok(ScanGroup {
                normalized_name,
                source_directories: source_directory_names,
                episode_count,
                conflict_count,
                episodes,
            })
        })
        .collect::<Result<Vec<_>, ScanServiceError>>()?;

    groups.sort_by(|left, right| left.normalized_name.cmp(&right.normalized_name));
    Ok(groups)
}

fn is_ignored_scan_directory(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(".trickplay")
}

fn inspect_tv_group(
    source_directories: &[SourceDirectoryRecord],
) -> Result<Vec<ScanEpisodeSlot>, ScanServiceError> {
    let mut slots: HashMap<(u32, u32), Vec<ScanEpisodeCandidate>> = HashMap::new();

    for source_directory in source_directories {
        let candidates = collect_tv_episode_candidates(source_directory)?;
        for candidate in candidates {
            slots.entry((candidate.season_number, candidate.episode_number))
                .or_default()
                .push(candidate);
        }
    }

    let mut episodes = slots
        .into_iter()
        .map(|((season_number, episode_number), mut candidates)| {
            candidates.sort_by(|left, right| {
                left.source_directory
                    .cmp(&right.source_directory)
                    .then_with(|| left.relative_path.cmp(&right.relative_path))
            });

            let auto_selected_source_directory =
                determine_auto_selected_source_directory(&candidates);
            let conflict = candidates.len() > 1 && auto_selected_source_directory.is_none();

            ScanEpisodeSlot {
                season_number,
                episode_number,
                candidates,
                auto_selected_source_directory,
                conflict,
            }
        })
        .collect::<Vec<_>>();

    episodes.sort_by(|left, right| {
        left.season_number
            .cmp(&right.season_number)
            .then_with(|| left.episode_number.cmp(&right.episode_number))
    });
    Ok(episodes)
}

fn collect_tv_episode_candidates(
    source_directory: &SourceDirectoryRecord,
) -> Result<Vec<ScanEpisodeCandidate>, ScanServiceError> {
    let mut candidates = Vec::new();
    let Ok(season_entries) = fs::read_dir(&source_directory.path) else {
        return Ok(candidates);
    };

    for season_entry in season_entries {
        let Ok(season_entry) = season_entry else {
            continue;
        };
        let season_path = season_entry.path();
        if !season_path.is_dir() {
            continue;
        }

        let season_name = season_entry.file_name().to_string_lossy().to_string();
        if is_ignored_scan_directory(&season_name) {
            continue;
        }

        let normalized_season_name = normalize_season_directory_name(&season_name);
        let Ok(episode_entries) = fs::read_dir(&season_path) else {
            continue;
        };

        for episode_entry in episode_entries {
            let Ok(episode_entry) = episode_entry else {
                continue;
            };
            let episode_path = episode_entry.path();
            if !episode_path.is_file() {
                continue;
            }

            let episode_name = episode_entry.file_name().to_string_lossy().to_string();
            if !is_episode_video_file(&episode_name) {
                continue;
            }

            let normalized_episode_name =
                normalize_episode_filename_with_nfo(&season_path, &episode_name);
            let Some((season_number, episode_number)) =
                parse_episode_identity(&normalized_episode_name)
            else {
                continue;
            };

            let file_size = episode_entry
                .metadata()
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            let extension = Path::new(&episode_name)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();

            candidates.push(ScanEpisodeCandidate {
                source_directory: source_directory.name.clone(),
                relative_path: format!("{season_name}/{episode_name}"),
                normalized_relative_path: format!(
                    "{normalized_season_name}/{normalized_episode_name}"
                ),
                season_number,
                episode_number,
                extension,
                file_size,
                quality: extract_quality_info(&episode_name, Some(file_size)),
            });
        }
    }

    Ok(candidates)
}

fn determine_auto_selected_source_directory(
    candidates: &[ScanEpisodeCandidate],
) -> Option<String> {
    if candidates.len() < 2 {
        return None;
    }

    let first = candidates.first()?;
    if candidates
        .iter()
        .skip(1)
        .all(|candidate| scan_candidates_are_equivalent(first, candidate))
    {
        return candidates
            .iter()
            .max_by(|left, right| {
                left.file_size
                    .cmp(&right.file_size)
                    .then_with(|| left.source_directory.cmp(&right.source_directory))
            })
            .map(|candidate| candidate.source_directory.clone());
    }

    None
}

fn scan_candidates_are_equivalent(
    left: &ScanEpisodeCandidate,
    right: &ScanEpisodeCandidate,
) -> bool {
    if left.extension != right.extension {
        return false;
    }

    if left.quality.resolution != right.quality.resolution
        || left.quality.codec != right.quality.codec
        || left.quality.source != right.quality.source
    {
        return false;
    }

    let max_size = left.file_size.max(right.file_size);
    if max_size == 0 {
        return true;
    }

    let delta = left.file_size.abs_diff(right.file_size) as f64;
    let ratio = delta / max_size as f64;
    ratio <= 0.05
}

fn is_episode_video_file(name: &str) -> bool {
    let Some(extension) = Path::new(name).extension().and_then(|value| value.to_str()) else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "mkv" | "mp4" | "avi" | "m4v" | "mov" | "wmv" | "ts"
    )
}
