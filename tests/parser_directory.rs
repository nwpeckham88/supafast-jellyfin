use supafast_jellyfin::parsers::directory::{
    append_metadata_tags, extract_metadata_tags, normalize_directory_name,
};

#[test]
fn normalizes_real_world_sample_directory_fixtures() {
    let fixtures = [
        include_str!("fixtures/sample_directories_movies.txt"),
        include_str!("fixtures/sample_directories_tv.txt"),
    ];

    for fixture in fixtures {
        for (line_number, line) in fixture.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let Some((input, expected)) = trimmed.split_once("=>") else {
                panic!("invalid fixture format at line {}: {trimmed}", line_number + 1);
            };

            let input = input.trim();
            let expected = expected.trim();
            assert_eq!(normalize_directory_name(input), expected, "input={input}");
        }
    }
}

#[test]
fn normalizes_verified_directory_examples() {
    let cases = [
        (
            "A Perfect Planet 2021 - [tmdbid-115194]",
            "A Perfect Planet (2021)",
        ),
        (
            "Alien - Earth 2025 - [tmdbid-157239]",
            "Alien - Earth (2025)",
        ),
        ("Animaniacs 1993 - [tmdbid-82]", "Animaniacs (1993)"),
    ];

    for (input, expected) in cases {
        assert_eq!(normalize_directory_name(input), expected, "input={input}");
    }
}

#[test]
fn removes_trailing_separator_artifacts() {
    assert_eq!(
        normalize_directory_name("Show Name 2020 - -"),
        "Show Name (2020)"
    );
    assert_eq!(
        normalize_directory_name("Show Name (2020) -"),
        "Show Name (2020)"
    );
}

#[test]
fn preserves_non_metadata_brackets() {
    assert_eq!(normalize_directory_name("[REC] 2007"), "[REC] (2007)");
}

#[test]
fn extracts_and_canonicalizes_supported_metadata_tags() {
    assert_eq!(
        extract_metadata_tags("Ghostbusters (1984) [tmdbid-620] [imdb-tt0087332]"),
        vec!["tmdbid-620".to_string(), "imdbid-tt0087332".to_string()]
    );
    assert_eq!(extract_metadata_tags("Predator 2 (1990) - [imdb-]"), Vec::<String>::new());
}

#[test]
fn appends_metadata_tags_in_jellyfin_style() {
    assert_eq!(
        append_metadata_tags(
            "Ghostbusters (1984)",
            &["tmdbid-620".to_string(), "imdbid-tt0087332".to_string()]
        ),
        "Ghostbusters (1984) [tmdbid-620] [imdbid-tt0087332]"
    );
}
