use sota_core::{ConfigError, Direction, Normalizer, Registry};

fn normalizer() -> Normalizer {
    Normalizer {
        lowercase: true,
        remove_whitespace: true,
        remove_chars: "-_".into(),
    }
}

const KGC: &str = r#"
topic = "kgc"

[[dataset]]
id = "fb15k-237"
name = "FB15k-237"
aliases = ["FB237"]

[[dataset]]
id = "fb15k"
name = "FB15k"

[[metric]]
id = "hits@10"
name = "Hits@10"
aliases = ["H@10"]
direction = "higher_is_better"
canonical_scale = "fraction"
accepted_scales = ["fraction", "percent"]
min = 0.0
max = 1.0

[[metric]]
id = "hits@1"
name = "Hits@1"
aliases = ["H@1"]
direction = "higher_is_better"
canonical_scale = "fraction"
accepted_scales = ["fraction", "percent"]

[[metric]]
id = "mr"
name = "MR"
aliases = ["Mean Rank"]
direction = "lower_is_better"
canonical_scale = "raw"
accepted_scales = ["raw"]

[[setting]]
id = "protocol"
name = "Evaluation protocol"
applies_to = ["hits@10", "hits@1", "mr"]
required = true
values = [
  { id = "filtered", aliases = ["filt."] },
  { id = "raw" },
]
"#;

fn build(sources: &[(&str, &str)]) -> Result<Registry, ConfigError> {
    Registry::build(normalizer(), sources)
}

fn kgc() -> Registry {
    build(&[("kgc.toml", KGC)]).unwrap()
}

fn problems(sources: &[(&str, &str)]) -> Vec<String> {
    match build(sources) {
        Err(ConfigError::Invalid { problems }) => problems,
        other => panic!("expected validation problems, got {other:?}"),
    }
}

fn has(problems: &[String], needles: &[&str]) -> bool {
    problems
        .iter()
        .any(|p| needles.iter().all(|needle| p.contains(needle)))
}

fn dataset(r: &Registry, mention: &str) -> Option<String> {
    r.resolve_dataset(mention).map(|d| d.id.clone())
}

fn metric(r: &Registry, mention: &str) -> Option<String> {
    r.resolve_metric(mention).map(|m| m.id.clone())
}

#[test]
fn id_name_and_aliases_resolve_after_normalization() {
    let r = kgc();
    for mention in ["fb15k-237", "FB15k-237", "FB15K 237", "fb_15k_237", "FB237"] {
        assert_eq!(
            dataset(&r, mention).as_deref(),
            Some("fb15k-237"),
            "{mention}"
        );
    }
}

#[test]
fn near_misses_stay_distinct() {
    let r = kgc();
    assert_eq!(dataset(&r, "FB15k").as_deref(), Some("fb15k"));
    assert_eq!(dataset(&r, "FB15k-23"), None);
    assert_eq!(metric(&r, "Hits@1").as_deref(), Some("hits@1"));
    assert_eq!(metric(&r, "H@10").as_deref(), Some("hits@10"));
    assert_eq!(metric(&r, "Hits@100"), None);
}

#[test]
fn unknown_or_empty_mentions_resolve_to_none() {
    let r = kgc();
    for mention in ["Freebase", "", "   ", "-"] {
        assert_eq!(dataset(&r, mention), None, "{mention:?}");
    }
}

#[test]
fn metrics_carry_direction_and_scale() {
    let r = kgc();
    let mr = r.resolve_metric("mean rank").unwrap();
    assert_eq!(mr.direction, Direction::LowerIsBetter);
    assert_eq!(
        r.resolve_metric("H@10").unwrap().direction,
        Direction::HigherIsBetter
    );
}

#[test]
fn setting_values_resolve_within_their_setting() {
    let r = kgc();
    let id = |s: &str, m: &str| r.resolve_setting_value(s, m).map(|v| v.id.clone());
    assert_eq!(id("protocol", "Filt.").as_deref(), Some("filtered"));
    assert_eq!(id("protocol", "RAW").as_deref(), Some("raw"));
    assert_eq!(id("protocol", "unfiltered"), None);
    assert_eq!(id("no-such-setting", "raw"), None);
}

#[test]
fn an_alias_shared_by_two_entries_is_rejected() {
    let text = KGC.replace(r#"aliases = ["FB237"]"#, r#"aliases = ["FB237", "FB15K"]"#);
    let p = problems(&[("kgc.toml", &text)]);
    assert!(has(&p, &["FB15K", "fb15k-237", "fb15k"]), "{p:?}");
}

#[test]
fn collisions_across_files_are_rejected() {
    let other = r#"
topic = "other"
[[dataset]]
id = "fb-237"
name = "Freebase 237"
aliases = ["FB237"]
"#;
    let p = problems(&[("kgc.toml", KGC), ("other.toml", other)]);
    assert!(has(&p, &["FB237", "fb-237"]), "{p:?}");
}

#[test]
fn duplicate_ids_are_rejected() {
    let other = "topic = \"other\"\n[[dataset]]\nid = \"fb15k\"\nname = \"FB15k again\"\n";
    let p = problems(&[("kgc.toml", KGC), ("other.toml", other)]);
    assert!(has(&p, &["duplicate", "fb15k"]), "{p:?}");
}

#[test]
fn ids_must_be_lowercase_slugs() {
    let p = problems(&[(
        "kgc.toml",
        &KGC.replace(r#"id = "fb15k""#, r#"id = "FB 15k""#),
    )]);
    assert!(has(&p, &["FB 15k", "not a valid id"]), "{p:?}");
}

#[test]
fn a_spelling_that_normalizes_to_nothing_is_rejected() {
    let p = problems(&[(
        "kgc.toml",
        &KGC.replace(r#"["FB237"]"#, r#"["FB237", "--"]"#),
    )]);
    assert!(has(&p, &["fb15k-237", "empty"]), "{p:?}");
}

#[test]
fn settings_must_apply_to_known_metrics() {
    let text = KGC.replace(r#""hits@1", "mr"]"#, r#""hits@1", "mrr"]"#);
    let p = problems(&[("kgc.toml", &text)]);
    assert!(has(&p, &["protocol", "mrr"]), "{p:?}");
}

#[test]
fn canonical_scale_must_be_accepted() {
    let text = KGC.replace(
        "canonical_scale = \"raw\"\naccepted_scales = [\"raw\"]",
        "canonical_scale = \"raw\"\naccepted_scales = [\"fraction\"]",
    );
    let p = problems(&[("kgc.toml", &text)]);
    assert!(has(&p, &["mr", "canonical"]), "{p:?}");
}

/// A percent is only convertible to a fraction; "percent" on a raw count is a config bug.
#[test]
fn percent_is_only_accepted_for_fraction_metrics() {
    let text = KGC.replace(
        r#"accepted_scales = ["raw"]"#,
        r#"accepted_scales = ["raw", "percent"]"#,
    );
    let p = problems(&[("kgc.toml", &text)]);
    assert!(has(&p, &["mr", "percent"]), "{p:?}");
}

#[test]
fn bounds_must_be_ordered() {
    let text = KGC.replace("min = 0.0\nmax = 1.0", "min = 1.0\nmax = 0.0");
    let p = problems(&[("kgc.toml", &text)]);
    assert!(has(&p, &["hits@10", "min"]), "{p:?}");
}

#[test]
fn parse_errors_name_the_file() {
    match build(&[("kgc.toml", KGC), ("broken.toml", "topic = ")]) {
        Err(ConfigError::Parse { file, .. }) => assert_eq!(file, "broken.toml"),
        other => panic!("expected a parse error, got {other:?}"),
    }
}

#[test]
fn a_setting_default_must_be_one_of_its_values() {
    let text = KGC.replace("required = true\n", "default = \"filt\"\n");
    let p = problems(&[("kgc.toml", &text)]);
    assert!(has(&p, &["protocol", "default", "filt"]), "{p:?}");
}

#[test]
fn a_required_setting_cannot_have_a_default() {
    let text = KGC.replace(
        "required = true\n",
        "required = true\ndefault = \"filtered\"\n",
    );
    let p = problems(&[("kgc.toml", &text)]);
    assert!(has(&p, &["protocol", "required", "default"]), "{p:?}");
}

#[test]
fn a_setting_default_is_exposed() {
    let text = KGC.replace("required = true\n", "default = \"filtered\"\n");
    let r = build(&[("kgc.toml", &text)]).unwrap();
    assert_eq!(
        r.setting("protocol").unwrap().default.as_deref(),
        Some("filtered")
    );
}
