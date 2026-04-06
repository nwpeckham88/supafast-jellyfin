use supafast_jellyfin::parsers::directory::normalize_directory_name;

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
        (
            "Animal Control - 2023 [tvdbid-423022]",
            "Animal Control (2023)",
        ),
        (
            "Animal Control 1997 - [tmdbid-214162]",
            "Animal Control (1997)",
        ),
        ("Animaniacs 1993 - [tmdbid-82]", "Animaniacs (1993)"),
        (
            "Spider-Man: Into the Spider-Verse (2018)",
            "Spider-Man - Into the Spider-Verse (2018)",
        ),
        (
            "TRON∶ Ares (2025) - [imdb-tt6604188]",
            "TRON - Ares (2025)",
        ),
        (
            "Now You See Me∶ Now You Donˈt (2025) - [imdb-tt4712810]",
            "Now You See Me - Now You Don't (2025)",
        ),
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
