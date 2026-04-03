pub fn normalize_directory_name(input: &str) -> String {
    let without_brackets = strip_bracket_groups(input);
    let normalized_separators = without_brackets.replace(':', " - ").replace('_', " ");
    let collapsed = collapse_whitespace(&normalized_separators);
    let cleaned_for_year = trim_trailing_separators(&collapsed);

    let (title_without_year, year) = extract_year(&cleaned_for_year);
    let cleaned_title = trim_trailing_separators(&collapse_whitespace(&title_without_year));

    if cleaned_title.is_empty() {
        return year.unwrap_or_default();
    }

    match year {
        Some(year) => format!("{cleaned_title} ({year})"),
        None => cleaned_title,
    }
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

        let trimmed = bracket_content.trim();
        if is_metadata_tag(trimmed) {
            continue;
        }

        if is_year(trimmed) {
            out.push(' ');
            out.push_str(trimmed);
            out.push(' ');
        } else {
            out.push('[');
            out.push_str(trimmed);
            out.push(']');
        }
    }

    out
}

fn extract_year(input: &str) -> (String, Option<String>) {
    let trimmed = input.trim();

    if let Some((title, year)) = extract_parenthesized_year(trimmed) {
        return (title, Some(year));
    }

    if let Some((title, year)) = extract_trailing_year(trimmed) {
        return (title, Some(year));
    }

    (trimmed.to_string(), None)
}

fn extract_parenthesized_year(input: &str) -> Option<(String, String)> {
    if !input.ends_with(')') {
        return None;
    }

    let open_idx = input.rfind('(')?;
    let year = &input[open_idx + 1..input.len() - 1];
    if !is_year(year.trim()) {
        return None;
    }

    let title = input[..open_idx].trim_end();
    Some((title.to_string(), year.trim().to_string()))
}

fn extract_trailing_year(input: &str) -> Option<(String, String)> {
    let mut parts = input.split_whitespace().collect::<Vec<_>>();
    let last = parts.last().copied()?;
    if !is_year(last) {
        return None;
    }

    parts.pop();
    let title = parts.join(" ");
    Some((trim_trailing_separators(&title), last.to_string()))
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

fn is_metadata_tag(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("tmdbid-") || lower.starts_with("tvdbid-") || lower.starts_with("imdbid-")
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}
