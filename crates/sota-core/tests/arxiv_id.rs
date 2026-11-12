use sota_core::ArxivId;

#[test]
fn parses_new_style_ids_with_and_without_version() {
    let id: ArxivId = "2106.06935v4".parse().unwrap();
    assert_eq!((id.id.as_str(), id.version), ("2106.06935", Some(4)));
    let id: ArxivId = "1902.10197".parse().unwrap();
    assert_eq!((id.id.as_str(), id.version), ("1902.10197", None));
    let id: ArxivId = " 0704.0001v12 ".parse().unwrap();
    assert_eq!((id.id.as_str(), id.version), ("0704.0001", Some(12)));
}

#[test]
fn parses_old_style_ids() {
    let id: ArxivId = "cs/0112017v1".parse().unwrap();
    assert_eq!((id.id.as_str(), id.version), ("cs/0112017", Some(1)));
    let id: ArxivId = "math.GT/0309136".parse().unwrap();
    assert_eq!(id.id, "math.GT/0309136");
}

#[test]
fn rejects_anything_else() {
    for bad in [
        "",
        "2106.0693",  // 5 digits since 2015
        "0704.00012", // 4 digits before 2015
        "2113.06935", // month 13
        "0612.0001",  // new style starts in April 2007
        "2106.069355v",
        "2106.06935v0",
        "21060.6935",
        "abs/2106.06935",
        "https://arxiv.org/abs/2106.06935",
        "cs/011201",
        "2106.06935v4 extra",
    ] {
        assert!(bad.parse::<ArxivId>().is_err(), "{bad:?} should not parse");
    }
}

#[test]
fn display_round_trips_and_file_stem_is_path_safe() {
    let id: ArxivId = "cs/0112017v1".parse().unwrap();
    assert_eq!(id.to_string(), "cs/0112017v1");
    assert_eq!(id.file_stem(), "cs_0112017v1");
    let id: ArxivId = "2106.06935".parse().unwrap();
    assert_eq!(id.to_string(), "2106.06935");
}
