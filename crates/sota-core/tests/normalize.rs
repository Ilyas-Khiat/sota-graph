use sota_core::Normalizer;

fn strict() -> Normalizer {
    Normalizer {
        lowercase: true,
        remove_whitespace: true,
        remove_chars: "-_\u{2013}".into(),
    }
}

#[test]
fn key_lowercases_and_drops_whitespace_and_listed_chars() {
    assert_eq!(strict().key("  FB15k\u{2013}237 "), "fb15k237");
    assert_eq!(strict().key("Hits @ 10"), "hits@10");
    assert_eq!(strict().key("fb_15k-237"), "fb15k237");
}

#[test]
fn non_breaking_spaces_count_as_whitespace() {
    assert_eq!(strict().key("FB15k\u{00A0}237\u{202F}"), "fb15k237");
}

#[test]
fn characters_not_listed_are_kept() {
    assert_eq!(strict().key("filt."), "filt.");
    assert_eq!(strict().key("H@10"), "h@10");
}

#[test]
fn rules_that_are_off_do_nothing() {
    let off = Normalizer {
        lowercase: false,
        remove_whitespace: false,
        remove_chars: String::new(),
    };
    assert_eq!(off.key(" FB15k-237 "), "FB15k-237");
}

#[test]
fn loads_from_toml_and_rejects_unknown_fields() {
    let n =
        Normalizer::from_toml_str("n.toml", "lowercase = true\nremove_chars = \"-\"\n").unwrap();
    assert!(n.lowercase && !n.remove_whitespace);
    assert!(Normalizer::from_toml_str("n.toml", "lower_case = true\n").is_err());
}
