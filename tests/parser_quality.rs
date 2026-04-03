use supafast_jellyfin::parsers::quality::extract_quality_info;

#[test]
fn extracts_full_quality_signature() {
    let info = extract_quality_info(
        "Animal Control - S04E06 [WEBDL-1080p][h264]-ETHEL.mkv",
        Some(1_500_000_000),
    );

    assert_eq!(info.resolution.as_deref(), Some("1080p"));
    assert_eq!(info.codec.as_deref(), Some("h264"));
    assert_eq!(info.source.as_deref(), Some("webdl"));
    assert_eq!(info.group.as_deref(), Some("ETHEL"));
    assert_eq!(info.size_bytes, Some(1_500_000_000));
}

#[test]
fn handles_missing_signals() {
    let info = extract_quality_info("Show - S01E01 - Pilot.mkv", None);

    assert_eq!(info.resolution, None);
    assert_eq!(info.codec, None);
    assert_eq!(info.source, None);
    assert_eq!(info.group, None);
    assert_eq!(info.size_bytes, None);
}

#[test]
fn does_not_treat_episode_token_as_group() {
    let info = extract_quality_info("Show - S01E01.mkv", None);
    assert_eq!(info.group, None);
}

#[test]
fn does_not_treat_part_token_as_group() {
    let info = extract_quality_info("Movie - PART2.mkv", None);
    assert_eq!(info.group, None);
}
