use supafast_jellyfin::parsers::episode::{is_protected_filename, normalize_episode_filename};

#[test]
fn normalizes_verified_episode_examples() {
    let cases = [
        (
            "Animal Control - S04E06 - Roosters and Moles [WEBDL-1080p][h264]-ETHEL.mkv",
            "Animal Control - S04E06 - Roosters and Moles.mkv",
        ),
        (
            "Animal Control - S04E06 - Roosters and Moles [WEBDL-1080p][h264]-ETHEL-thumb.jpg",
            "Animal Control - S04E06 - Roosters and Moles-thumb.jpg",
        ),
        (
            "A Perfect Planet - S01E01 - Volcano [tmdbid-115194].nfo",
            "A Perfect Planet - S01E01 - Volcano.nfo",
        ),
        (
            "A Perfect Planet - S01E01 - Volcano [tmdbid-115194].eng.srt",
            "A Perfect Planet - S01E01 - Volcano.eng.srt",
        ),
        (
            "Animal Control - S04E06 - Roosters and Moles [WEBDL-1080p][h264]-ETHEL.sharky.json",
            "Animal Control - S04E06 - Roosters and Moles.sharky.json",
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
    assert_eq!(normalize_episode_filename(input), input);
}
