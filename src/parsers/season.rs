pub fn normalize_season_directory_name(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return input.to_string();
    }

    if let Some(number) = parse_season_number(trimmed) {
        return format!("Season {number}");
    }

    input.to_string()
}

fn parse_season_number(value: &str) -> Option<u32> {
    let lower = value.to_ascii_lowercase();

    if let Some(rest) = lower.strip_prefix('s') {
        if let Some(number) = parse_number_token(rest) {
            return Some(number);
        }
    }

    if let Some(rest) = lower.strip_prefix("season") {
        if let Some(number) = parse_number_token(rest) {
            return Some(number);
        }
    }

    None
}

fn parse_number_token(raw: &str) -> Option<u32> {
    let token = raw
        .trim_start_matches(|c: char| c == ' ' || c == '_' || c == '-')
        .trim();
    if token.is_empty() {
        return None;
    }

    if !token.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let value = token.parse::<u32>().ok()?;
    if value == 0 {
        return None;
    }

    Some(value)
}
