use supafast_jellyfin::parsers::season::normalize_season_directory_name;

#[test]
fn normalizes_season_aliases_to_canonical_format() {
    let cases = [
        ("Season 01", "Season 1"),
        ("season_2", "Season 2"),
        ("S03", "Season 3"),
        ("s4", "Season 4"),
    ];

    for (input, expected) in cases {
        assert_eq!(normalize_season_directory_name(input), expected, "input={input}");
    }
}

#[test]
fn preserves_non_season_directory_names() {
    let cases = ["Extras", "Specials", "Behind the Scenes"];

    for input in cases {
        assert_eq!(normalize_season_directory_name(input), input, "input={input}");
    }
}
