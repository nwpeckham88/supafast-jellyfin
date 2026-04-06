use std::path::{Path, PathBuf};

pub fn normalize_episode_filename(input: &str) -> String {
    if is_protected_filename(input) {
        return input.to_string();
    }

    let (stem, suffix) = split_stem_and_suffix(input);
    let has_release_signature = contains_release_signature(stem);
    let mut cleaned = strip_bracket_groups(stem);
    cleaned = collapse_whitespace(&cleaned);
    cleaned = remove_trailing_release_group(&cleaned, has_release_signature);
    cleaned = trim_trailing_separators(&cleaned);

    if let Some((season, episode, title)) = extract_episode_identity(&cleaned) {
        let canonical = if title.is_empty() {
            format!("{season:02}x{episode:02}")
        } else {
            format!("{season:02}x{episode:02} - {title}")
        };
        return format!("{canonical}{suffix}");
    }

    format!("{cleaned}{suffix}")
}

pub fn normalize_episode_filename_with_nfo(season_dir: &Path, input: &str) -> String {
    if is_protected_filename(input) {
        return input.to_string();
    }

    let fallback = normalize_episode_filename(input);
    let Some(metadata) = load_episode_metadata_from_nfo(season_dir, input) else {
        return fallback;
    };

    // Prefer sidecar metadata, but do not override an explicit filename identity when it disagrees.
    let (stem, suffix) = split_stem_and_suffix(input);
    let stem_for_identity = stem.strip_suffix("-thumb").unwrap_or(stem);
    if let Some((season, episode, _)) = extract_episode_identity(stem_for_identity) {
        if season != metadata.season || episode != metadata.episode {
            return fallback;
        }
    }

    let is_thumb = stem.to_ascii_lowercase().ends_with("-thumb");
    let mut title = sanitize_nfo_title(&metadata.title);
    if title.is_empty() {
        return fallback;
    }

    if is_thumb {
        title.push_str("-thumb");
    }

    format!("{:02}x{:02} - {}{}", metadata.season, metadata.episode, title, suffix)
}

pub fn is_protected_filename(filename: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "season.nfo" | "tvshow.nfo" | "poster.jpg" | "fanart.jpg" | "banner.jpg" | "clearlogo.png"
    ) {
        return true;
    }

    if lower.starts_with("backdrop") && lower.ends_with(".jpg") {
        return true;
    }

    lower.starts_with("season") && lower.ends_with("-poster.jpg")
}

fn split_stem_and_suffix(input: &str) -> (&str, &str) {
    let lower = input.to_ascii_lowercase();

    if lower.ends_with(".sharky.json") {
        let split = input.len() - ".sharky.json".len();
        return (&input[..split], &input[split..]);
    }

    if lower.ends_with(".srt") {
        if let Some(last_dot) = input.rfind('.') {
            let before_srt = &input[..last_dot];
            if let Some(lang_dot) = before_srt.rfind('.') {
                let lang = &before_srt[lang_dot + 1..];
                if (2..=3).contains(&lang.len()) && lang.chars().all(|c| c.is_ascii_alphabetic()) {
                    return (&input[..lang_dot], &input[lang_dot..]);
                }
            }
        }
    }

    if let Some(dot) = input.rfind('.') {
        return (&input[..dot], &input[dot..]);
    }

    (input, "")
}

fn strip_bracket_groups(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '[' {
            out.push(ch);
            continue;
        }

        let mut bracket_content = String::new();
        let mut found_close = false;
        while let Some(next) = chars.next() {
            if next == ']' {
                found_close = true;
                break;
            }
            bracket_content.push(next);
        }

        if !found_close {
            out.push('[');
            out.push_str(&bracket_content);
            break;
        }

        if is_year(bracket_content.trim()) {
            out.push(' ');
            out.push_str(bracket_content.trim());
            out.push(' ');
        }
    }

    out
}

fn remove_trailing_release_group(input: &str, has_release_signature: bool) -> String {
    if !has_release_signature {
        return input.trim_end().to_string();
    }

    let trimmed = input.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with("-thumb") {
        let thumb_idx = trimmed.len() - "-thumb".len();
        let before_thumb = trimmed[..thumb_idx].trim_end();
        if let Some(group_idx) = before_thumb.rfind('-') {
            let maybe_group = before_thumb[group_idx + 1..].trim();
            if maybe_group
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            {
                return format!("{}-thumb", before_thumb[..group_idx].trim_end());
            }
        }
        return trimmed.to_string();
    }

    let Some(idx) = trimmed.rfind('-') else {
        return trimmed.to_string();
    };

    let group = trimmed[idx + 1..].trim();
    if group.is_empty() {
        return trimmed.to_string();
    }

    if group
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return trimmed[..idx].trim_end().to_string();
    }

    trimmed.to_string()
}

fn contains_release_signature(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    [
        "[webdl", "[web-dl", "[webrip", "[bluray", "[hdtv", "[remux", "[h264", "[h265", "[x264",
        "[x265", "[hevc", "[av1",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn trim_trailing_separators(input: &str) -> String {
    let mut value = input.trim().to_string();
    loop {
        let trimmed = value.trim_end();
        let next = if let Some(prefix) = trimmed.strip_suffix(" -") {
            prefix.trim_end().to_string()
        } else if let Some(prefix) = trimmed.strip_suffix('-') {
            prefix.trim_end().to_string()
        } else {
            break;
        };

        if next == value {
            break;
        }

        value = next;
    }
    value
}

fn is_year(value: &str) -> bool {
    value.len() == 4 && value.chars().all(|c| c.is_ascii_digit())
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_episode_identity(input: &str) -> Option<(u32, u32, String)> {
    if let Some((season, episode, end_index)) = find_sxxexx_token(input) {
        let title = normalize_title_fragment(&input[end_index..]);
        return Some((season, episode, title));
    }

    if let Some((season, episode, end_index)) = find_x_notation_token(input) {
        let title = normalize_title_fragment(&input[end_index..]);
        return Some((season, episode, title));
    }

    None
}

fn find_sxxexx_token(value: &str) -> Option<(u32, u32, usize)> {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let current = bytes[i].to_ascii_lowercase();
        if current != b's' {
            i += 1;
            continue;
        }

        let Some((season, season_end)) = parse_number(bytes, i + 1, 2) else {
            i += 1;
            continue;
        };
        if season == 0 {
            i += 1;
            continue;
        }

        if season_end >= bytes.len() || bytes[season_end].to_ascii_lowercase() != b'e' {
            i += 1;
            continue;
        }

        let Some((episode, episode_end)) = parse_number(bytes, season_end + 1, 2) else {
            i += 1;
            continue;
        };
        if episode == 0 {
            i += 1;
            continue;
        }

        if !is_valid_token_boundary(bytes, i, episode_end) {
            i += 1;
            continue;
        }

        return Some((season, episode, episode_end));
    }

    None
}

fn find_x_notation_token(value: &str) -> Option<(u32, u32, usize)> {
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let Some((season, season_end)) = parse_number(bytes, i, 2) else {
            i += 1;
            continue;
        };
        if season == 0 || season_end >= bytes.len() || bytes[season_end] != b'x' {
            i += 1;
            continue;
        }

        let Some((episode, episode_end)) = parse_number(bytes, season_end + 1, 2) else {
            i += 1;
            continue;
        };
        if episode == 0 {
            i += 1;
            continue;
        }

        if !is_valid_token_boundary(bytes, i, episode_end) {
            i += 1;
            continue;
        }

        return Some((season, episode, episode_end));
    }

    None
}

fn parse_number(bytes: &[u8], start: usize, max_digits: usize) -> Option<(u32, usize)> {
    if start >= bytes.len() || !bytes[start].is_ascii_digit() {
        return None;
    }

    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() && (end - start) < max_digits {
        end += 1;
    }

    let digits = std::str::from_utf8(&bytes[start..end]).ok()?;
    let parsed = digits.parse::<u32>().ok()?;
    Some((parsed, end))
}

fn is_valid_token_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    let left_ok = if start == 0 {
        true
    } else {
        !bytes[start - 1].is_ascii_alphanumeric()
    };

    let right_ok = if end >= bytes.len() {
        true
    } else {
        !bytes[end].is_ascii_alphanumeric()
    };

    left_ok && right_ok
}

fn normalize_title_fragment(raw: &str) -> String {
    let mut title = raw
        .trim_start_matches(|c: char| c == ' ' || c == '-' || c == '_' || c == '.')
        .trim()
        .to_string();

    if title.is_empty() {
        return title;
    }

    let lower = title.to_ascii_lowercase();
    if lower.ends_with("-thumb") {
        let stripped = title[..title.len() - "-thumb".len()].trim_end();
        title = format!("{}-thumb", stripped);
    }

    trim_trailing_separators(&collapse_whitespace(&title))
}

#[derive(Debug, Clone)]
struct EpisodeNfoMetadata {
    season: u32,
    episode: u32,
    title: String,
}

fn load_episode_metadata_from_nfo(season_dir: &Path, input: &str) -> Option<EpisodeNfoMetadata> {
    let nfo_path = sidecar_nfo_path(season_dir, input)?;
    let contents = std::fs::read_to_string(nfo_path).ok()?;
    parse_episode_nfo_metadata(&contents)
}

fn sidecar_nfo_path(season_dir: &Path, input: &str) -> Option<PathBuf> {
    let (stem, suffix) = split_stem_and_suffix(input);
    if suffix.eq_ignore_ascii_case(".nfo") {
        let direct = season_dir.join(input);
        if direct.exists() {
            return Some(direct);
        }

        let swapped = if input.ends_with(".nfo") {
            season_dir.join(input.trim_end_matches(".nfo").to_string() + ".NFO")
        } else if input.ends_with(".NFO") {
            season_dir.join(input.trim_end_matches(".NFO").to_string() + ".nfo")
        } else {
            direct
        };

        if swapped.exists() {
            return Some(swapped);
        }

        return None;
    }

    let stem_without_thumb = stem
        .strip_suffix("-thumb")
        .or_else(|| stem.strip_suffix("-THUMB"))
        .unwrap_or(stem);

    if stem_without_thumb.is_empty() {
        return None;
    }

    let lower = season_dir.join(format!("{stem_without_thumb}.nfo"));
    if lower.exists() {
        return Some(lower);
    }

    let upper = season_dir.join(format!("{stem_without_thumb}.NFO"));
    if upper.exists() {
        return Some(upper);
    }

    None
}

fn parse_episode_nfo_metadata(contents: &str) -> Option<EpisodeNfoMetadata> {
    let season = extract_nfo_tag(contents, "season")?.parse::<u32>().ok()?;
    let episode = extract_nfo_tag(contents, "episode")?.parse::<u32>().ok()?;
    let title = extract_nfo_tag(contents, "title")?;
    if season == 0 || episode == 0 {
        return None;
    }

    Some(EpisodeNfoMetadata {
        season,
        episode,
        title,
    })
}

fn extract_nfo_tag(contents: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = contents.find(&open)? + open.len();
    let tail = &contents[start..];
    let end = tail.find(&close)?;
    Some(tail[..end].trim().to_string())
}

fn sanitize_nfo_title(input: &str) -> String {
    let mut sanitized = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '/' | '\\' => sanitized.push('-'),
            c if c.is_control() => {}
            _ => sanitized.push(ch),
        }
    }

    trim_trailing_separators(&collapse_whitespace(&sanitized))
}
