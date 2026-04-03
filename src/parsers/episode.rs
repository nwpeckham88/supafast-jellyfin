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

    format!("{cleaned}{suffix}")
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
