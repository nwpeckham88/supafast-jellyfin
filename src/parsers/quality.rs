use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityInfo {
    pub resolution: Option<String>,
    pub codec: Option<String>,
    pub source: Option<String>,
    pub group: Option<String>,
    pub size_bytes: Option<u64>,
}

pub fn extract_quality_info(filename: &str, size_bytes: Option<u64>) -> QualityInfo {
    let lowered = filename.to_ascii_lowercase();

    QualityInfo {
        resolution: extract_resolution(&lowered),
        codec: extract_codec(&lowered),
        source: extract_source(&lowered),
        group: extract_group(filename),
        size_bytes,
    }
}

fn extract_resolution(value: &str) -> Option<String> {
    if value.contains("2160p") || value.contains("4k") {
        return Some("2160p".to_string());
    }
    if value.contains("1080p") {
        return Some("1080p".to_string());
    }
    if value.contains("720p") {
        return Some("720p".to_string());
    }
    if value.contains("480p") {
        return Some("480p".to_string());
    }
    None
}

fn extract_codec(value: &str) -> Option<String> {
    if value.contains("av1") {
        return Some("av1".to_string());
    }
    if value.contains("h265") {
        return Some("h265".to_string());
    }
    if value.contains("x265") {
        return Some("x265".to_string());
    }
    if value.contains("hevc") {
        return Some("hevc".to_string());
    }
    if value.contains("h264") {
        return Some("h264".to_string());
    }
    if value.contains("x264") {
        return Some("x264".to_string());
    }
    None
}

fn extract_source(value: &str) -> Option<String> {
    if value.contains("webdl") || value.contains("web-dl") {
        return Some("webdl".to_string());
    }
    if value.contains("webrip") {
        return Some("webrip".to_string());
    }
    if value.contains("bluray") {
        return Some("bluray".to_string());
    }
    if value.contains("hdtv") {
        return Some("hdtv".to_string());
    }
    if value.contains("remux") {
        return Some("remux".to_string());
    }
    None
}

fn extract_group(filename: &str) -> Option<String> {
    let stem = filename.rsplit_once('.').map_or(filename, |(left, _)| left);
    let idx = stem.rfind('-')?;
    let maybe_group = stem[idx + 1..].trim();

    if maybe_group.is_empty() || maybe_group.eq_ignore_ascii_case("thumb") {
        return None;
    }

    if is_episode_token(maybe_group) || maybe_group.starts_with("PART") {
        return None;
    }

    if (2..=12).contains(&maybe_group.len())
        && maybe_group
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Some(maybe_group.to_string());
    }

    None
}

fn is_episode_token(value: &str) -> bool {
    let upper = value.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    if bytes.len() != 6 {
        return false;
    }

    bytes[0] == b'S'
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3] == b'E'
        && bytes[4].is_ascii_digit()
        && bytes[5].is_ascii_digit()
}
