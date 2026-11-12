use std::collections::BTreeMap;

use sota_core::{Normalizer, Registry, Report, Role, leaderboards, values_agree};

const KGC: &str = include_str!("../../../config/registry/kgc.toml");
const NORMALIZATION: &str = include_str!("../../../config/normalization.toml");

fn registry() -> Registry {
    let n = Normalizer::from_toml_str("normalization.toml", NORMALIZATION).unwrap();
    Registry::build(n, &[("kgc.toml", KGC)]).unwrap()
}

fn report(paper: &str, method: &str, role: Role, metric: &str, text: &str, value: f64) -> Report {
    let mut settings = BTreeMap::new();
    for (k, v) in [
        ("protocol", "filtered"),
        ("queries", "both"),
        ("regime", "supervised"),
        ("relation_category", "all"),
    ] {
        settings.insert(k.to_owned(), v.to_owned());
    }
    Report {
        paper: paper.into(),
        locator: format!("{paper}#T/r1/c1"),
        method_id: method.to_lowercase(),
        method_name: method.into(),
        method_known: true,
        role,
        dataset: "fb15k-237".into(),
        metric: metric.into(),
        split: "test".into(),
        settings,
        value,
        value_text: text.into(),
    }
}

fn mrr(paper: &str, method: &str, role: Role, text: &str, value: f64) -> Report {
    report(paper, method, role, "mrr", text, value)
}

fn board(reports: &[Report], metric: &str) -> sota_core::Leaderboard {
    let mut boards = leaderboards(reports, &registry(), "fb15k-237", metric, &BTreeMap::new());
    assert_eq!(boards.len(), 1, "{boards:?}");
    boards.remove(0)
}

fn order(b: &sota_core::Leaderboard) -> Vec<(usize, String)> {
    b.entries
        .iter()
        .map(|e| (e.rank, e.method_id.clone()))
        .collect()
}

#[test]
fn ranks_follow_the_metric_direction() {
    let reports = [
        mrr("p1", "RotatE", Role::Proposed, ".338", 0.338),
        mrr("p2", "NBFNet", Role::Proposed, "0.415", 0.415),
        mrr("p3", "TransE", Role::Proposed, ".294", 0.294),
    ];
    assert_eq!(
        order(&board(&reports, "mrr")),
        [
            (1, "nbfnet".into()),
            (2, "rotate".into()),
            (3, "transe".into())
        ]
    );

    let mr = [
        report("p1", "RotatE", Role::Proposed, "mr", "177", 177.0),
        report("p2", "NBFNet", Role::Proposed, "mr", "114", 114.0),
    ];
    assert_eq!(
        order(&board(&mr, "mr")),
        [(1, "nbfnet".into()), (2, "rotate".into())]
    ); // lower is better
}

#[test]
fn ties_share_a_rank() {
    let reports = [
        mrr("p1", "A", Role::Proposed, "0.5", 0.5),
        mrr("p2", "B", Role::Proposed, "0.5", 0.5),
        mrr("p3", "C", Role::Proposed, "0.4", 0.4),
    ];
    let ranks: Vec<usize> = board(&reports, "mrr")
        .entries
        .iter()
        .map(|e| e.rank)
        .collect();
    assert_eq!(ranks, [1, 1, 3]);
}

#[test]
fn the_proposing_paper_speaks_for_its_method() {
    // RotatE's own number, and NBFNet's (different) report of it.
    let reports = [
        report(
            "2106.06935v4",
            "RotatE",
            Role::Baseline,
            "hits@10",
            "0.553",
            0.553,
        ),
        report(
            "1902.10197v1",
            "RotatE",
            Role::Proposed,
            "hits@10",
            ".533",
            0.533,
        ),
    ];
    let b = board(&reports, "hits@10");
    let e = &b.entries[0];
    assert_eq!((e.value, e.source.paper.as_str()), (0.533, "1902.10197v1"));
    assert_eq!(e.others.len(), 1);
    assert!(e.conflict, "0.553 is not .533 rounded");
}

#[test]
fn without_a_self_report_the_most_reported_value_wins() {
    let reports = [
        mrr("p1", "DistMult", Role::Baseline, ".241", 0.241),
        mrr("p2", "DistMult", Role::Baseline, "0.241", 0.241),
        mrr("p3", "DistMult", Role::Baseline, ".251", 0.251),
    ];
    let e = &board(&reports, "mrr").entries[0];
    assert_eq!(e.value, 0.241);
    assert!(e.conflict);
}

#[test]
fn rounding_differences_are_not_conflicts() {
    assert!(values_agree(".43", 0.43, "0.430", 0.43));
    assert!(values_agree(".43", 0.43, "0.432", 0.432)); // .43 covers .425 to .435
    assert!(values_agree("47.6", 0.476, ".476", 0.476)); // percent vs fraction
    assert!(!values_agree(".533", 0.533, "0.553", 0.553));
    assert!(!values_agree(".44", 0.44, "0.520", 0.52)); // ConvE on YAGO3-10, as AdaProp prints it
    assert!(values_agree("177", 177.0, "177", 177.0));
}

#[test]
fn different_settings_are_different_leaderboards() {
    let mut tail = mrr("p2", "NBFNet", Role::Proposed, "0.509", 0.509);
    tail.settings.insert("queries".into(), "tail".into());
    let reports = [mrr("p1", "NBFNet", Role::Proposed, "0.415", 0.415), tail];
    let boards = leaderboards(&reports, &registry(), "fb15k-237", "mrr", &BTreeMap::new());
    assert_eq!(boards.len(), 2);
    let tails: Vec<_> = boards
        .iter()
        .filter(|b| b.key.settings["queries"] == "tail")
        .collect();
    assert_eq!(tails[0].entries[0].value, 0.509);

    // A filter picks one.
    let mut only_both = BTreeMap::new();
    only_both.insert("queries".to_owned(), "both".to_owned());
    let boards = leaderboards(&reports, &registry(), "fb15k-237", "mrr", &only_both);
    assert_eq!((boards.len(), boards[0].entries[0].value), (1, 0.415));
}

#[test]
fn other_datasets_and_metrics_are_left_out() {
    let mut other = mrr("p1", "X", Role::Proposed, "0.9", 0.9);
    other.dataset = "wn18rr".into();
    let reports = [
        other,
        report("p1", "Y", Role::Proposed, "hits@10", "0.5", 0.5),
    ];
    assert!(leaderboards(&reports, &registry(), "fb15k-237", "mrr", &BTreeMap::new()).is_empty());
}
