use std::collections::BTreeMap;

use sota_core::{GoldFile, GoldResult, ReviewBy, ReviewItem, Role, score_extraction};

const SPEC: &str = r#"
paper = "2401.01234v3"
negative_tables = ["S2.T1"]

[[table]]
id = "S4.T1"
header_rows = 2
settings = { protocol = "filtered" }
columns = [
  { col = 1, dataset = "fb15k-237", metric = "mr" },
  { col = 2, dataset = "fb15k-237", metric = "mrr" },
]
rows = [
  { row = 2, method = "TransE", method_id = "transe", role = "baseline" },
  { row = 3, method = "Ours", method_id = "ours", role = "proposed" },
]
review_cells = [{ row = 4, reason = "typo" }]
"#;

fn result(
    table: &str,
    row: usize,
    col: usize,
    method: &str,
    role: Role,
    metric: &str,
    value: f64,
) -> GoldResult {
    let mut settings = BTreeMap::new();
    settings.insert("protocol".to_owned(), "filtered".to_owned());
    GoldResult {
        paper: "2401.01234v3".into(),
        table: table.into(),
        row,
        col,
        locator: format!("2401.01234v3#{table}/r{row}/c{col}"),
        method: method.into(),
        method_id: method.to_lowercase(),
        role,
        dataset: "fb15k-237".into(),
        metric: metric.into(),
        split: "test".into(),
        settings,
        value_text: value.to_string(),
        value,
        std: None,
    }
}

fn gold() -> Vec<GoldResult> {
    vec![
        result("S4.T1", 2, 1, "TransE", Role::Baseline, "mr", 357.0),
        result("S4.T1", 2, 2, "TransE", Role::Baseline, "mrr", 0.294),
        result("S4.T1", 3, 1, "Ours", Role::Proposed, "mr", 177.0),
        result("S4.T1", 3, 2, "Ours", Role::Proposed, "mrr", 0.338),
    ]
}

fn review(table: &str, row: Option<usize>, col: Option<usize>) -> ReviewItem {
    ReviewItem {
        paper: "2401.01234v3".into(),
        table: table.into(),
        row,
        col,
        reason: "x".into(),
        by: ReviewBy::Check,
    }
}

fn spec() -> GoldFile {
    GoldFile::from_toml_str("gold.toml", SPEC).unwrap()
}

#[test]
fn a_perfect_extraction_scores_one() {
    let s = score_extraction(&spec(), &gold(), &gold(), &[review("S4.T1", Some(4), None)]);
    assert_eq!((s.tp, s.fp, s.fn_), (4, 0, 0));
    assert_eq!((s.precision(), s.recall()), (1.0, 1.0));
    assert_eq!(
        (s.held_back, s.held_back_caught, s.held_back_leaked),
        (2, 2, 0)
    );
}

#[test]
fn a_wrong_field_is_both_a_false_positive_and_a_miss() {
    let mut extracted = gold();
    extracted[3].role = Role::Baseline;
    extracted[1].value = 0.249;
    let s = score_extraction(&spec(), &gold(), &extracted, &[]);
    assert_eq!((s.tp, s.fp, s.fn_), (2, 2, 2));
    assert!(
        s.mismatches
            .iter()
            .any(|m| m.contains("r3/c2") && m.contains("role")),
        "{:?}",
        s.mismatches
    );
    assert!(
        s.mismatches
            .iter()
            .any(|m| m.contains("r2/c2") && m.contains("value")),
        "{:?}",
        s.mismatches
    );
}

#[test]
fn missed_and_extra_results_are_counted() {
    let mut extracted = gold();
    extracted.pop();
    extracted.push(result("S4.T1", 5, 1, "Extra", Role::Baseline, "mr", 1.0));
    let s = score_extraction(&spec(), &gold(), &extracted, &[]);
    assert_eq!((s.tp, s.fp, s.fn_), (3, 1, 1));
}

#[test]
fn held_back_cells_must_go_to_review_not_into_results() {
    // Row 4 is held back in the gold: a result there leaked, and nothing covers the other cell.
    let extracted = [
        gold(),
        vec![result("S4.T1", 4, 2, "HypER", Role::Baseline, "mrr", 958.0)],
    ]
    .concat();
    let s = score_extraction(&spec(), &gold(), &extracted, &[]);
    assert_eq!(
        (s.held_back, s.held_back_caught, s.held_back_leaked),
        (2, 0, 1)
    );
    assert_eq!(
        s.fp, 0,
        "a leaked held-back cell is counted as leaked, not twice"
    );
}

#[test]
fn good_results_sent_to_review_are_counted() {
    let s = score_extraction(
        &spec(),
        &gold(),
        &gold()[..2],
        &[review("S4.T1", Some(3), None)],
    );
    assert_eq!((s.fn_, s.wrongly_reviewed), (2, 2));
}

#[test]
fn results_in_negative_tables_are_false_positives_and_other_tables_are_unscored() {
    let extracted = [
        gold(),
        vec![result(
            "S2.T1",
            2,
            1,
            "Stats",
            Role::Baseline,
            "mr",
            14541.0,
        )],
        vec![result("S9.T9", 2, 1, "Ablation", Role::Variant, "mr", 3.0)],
    ]
    .concat();
    let s = score_extraction(&spec(), &gold(), &extracted, &[]);
    assert_eq!((s.fp, s.negative_table_results, s.unscored), (1, 1, 1));
}

#[test]
fn scores_add_up_across_papers() {
    let a = score_extraction(&spec(), &gold(), &gold(), &[]);
    let mut total = a.clone();
    total.add(&a);
    assert_eq!((total.tp, total.held_back), (8, 4));
}
