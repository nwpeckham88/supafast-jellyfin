use supafast_jellyfin::parsers::episode::{
    is_protected_filename, normalize_episode_filename, normalize_episode_filename_with_nfo,
};

#[test]
fn normalizes_verified_episode_examples() {
    let cases = [
        (
            "Animal Control - S04E06 - Roosters and Moles [WEBDL-1080p][h264]-ETHEL.mkv",
            "04x06 - Roosters and Moles.mkv",
        ),
        (
            "Animal Control - S04E06 - Roosters and Moles [WEBDL-1080p][h264]-ETHEL-thumb.jpg",
            "04x06 - Roosters and Moles-thumb.jpg",
        ),
        (
            "A Perfect Planet - S01E01 - Volcano [tmdbid-115194].nfo",
            "01x01 - Volcano.nfo",
        ),
        (
            "A Perfect Planet - S01E01 - Volcano [tmdbid-115194].eng.srt",
            "01x01 - Volcano.eng.srt",
        ),
        (
            "Animal Control - S04E06 - Roosters and Moles [WEBDL-1080p][h264]-ETHEL.sharky.json",
            "04x06 - Roosters and Moles.sharky.json",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(normalize_episode_filename(input), expected, "input={input}");
    }
}

#[test]
fn keeps_protected_files_unchanged() {
    let protected = [
        "season.nfo",
        "tvshow.nfo",
        "poster.jpg",
        "fanart.jpg",
        "banner.jpg",
        "clearlogo.png",
        "backdrop1.jpg",
        "season01-poster.jpg",
    ];

    for input in protected {
        assert!(is_protected_filename(input), "input={input}");
        assert_eq!(normalize_episode_filename(input), input, "input={input}");
    }
}

#[test]
fn keeps_legitimate_trailing_uppercase_title_token() {
    let input = "Space Show - S01E01 - Mission NASA.mkv";
    assert_eq!(normalize_episode_filename(input), "01x01 - Mission NASA.mkv");
}

#[test]
fn sidecar_nfo_metadata_is_preferred_for_episode_title_and_indices() {
    let temp = tempfile::tempdir().expect("tempdir");
    let season = temp.path();
    std::fs::write(
        season.join("Animal Control S04E09 1080p WEB h264-ETHEL.nfo"),
        r#"<episodedetails><season>4</season><episode>9</episode><title>Dogs and Geese</title></episodedetails>"#,
    )
    .expect("write nfo");

    let normalized =
        normalize_episode_filename_with_nfo(season, "Animal Control S04E09 1080p WEB h264-ETHEL.mkv");
    assert_eq!(normalized, "04x09 - Dogs and Geese.mkv");
}

#[test]
fn sidecar_nfo_falls_back_to_filename_when_missing_or_invalid() {
    let temp = tempfile::tempdir().expect("tempdir");
    let season = temp.path();
    std::fs::write(
        season.join("Example S01E02.nfo"),
        r#"<episodedetails><season></season><episode>2</episode><title>Pilot</title></episodedetails>"#,
    )
    .expect("write nfo");

    let normalized = normalize_episode_filename_with_nfo(season, "Example S01E02.mkv");
    assert_eq!(normalized, "01x02.mkv");
}

#[test]
fn sidecar_nfo_does_not_override_conflicting_filename_identity() {
    let temp = tempfile::tempdir().expect("tempdir");
    let season = temp.path();
    std::fs::write(
        season.join("Example S01E02.nfo"),
        r#"<episodedetails><season>5</season><episode>7</episode><title>Wrong Episode</title></episodedetails>"#,
    )
    .expect("write nfo");

    let normalized = normalize_episode_filename_with_nfo(season, "Example S01E02.mkv");
    assert_eq!(normalized, "01x02.mkv");
}
