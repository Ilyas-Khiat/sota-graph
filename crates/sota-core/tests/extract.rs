use sota_core::{
    Cell, Channel, Document, Normalizer, Paragraph, Registry, ReviewBy, Section, Table,
    TableMapping, verify_mapping,
};

const KGC: &str = include_str!("../../../config/registry/kgc.toml");
const NORMALIZATION: &str = include_str!("../../../config/normalization.toml");

fn registry() -> Registry {
    let n = Normalizer::from_toml_str("normalization.toml", NORMALIZATION).unwrap();
    Registry::build(n, &[("kgc.toml", KGC)]).unwrap()
}

/// | Method       | FB15k-237 (2 cols) |                | Family |
/// |              | MR                 | MRR            | MRR    |
/// | TransE       | 357                | .294           | .5     |
/// | Ours (large) | 177                | 0.338\pm 0.002 | .6     |
/// | HypER        | 2                  | 958            | .7     |
fn doc() -> Document {
    let c = |row, col, row_span, col_span, text: &str| Cell {
        row,
        col,
        row_span,
        col_span,
        text: text.into(),
        bold: false,
        header: row < 2,
        cites: Vec::new(),
    };
    Document {
        arxiv_id: "2401.01234".into(),
        version: 3,
        channel: Channel::ArxivHtml,
        title: "Synthetic".into(),
        unparsed_tables: Vec::new(),
        sections: vec![Section {
            id: "S4".into(),
            heading: "4 Experiments".into(),
            paragraphs: vec![
                Paragraph {
                    id: "S4.p1".into(),
                    text: "We follow the filtered ranking protocol.".into(),
                },
                Paragraph {
                    id: "S4.p2".into(),
                    text: "We train for 100 epochs on a single GPU.".into(),
                },
            ],
        }],
        tables: vec![Table {
            id: "S4.T1".into(),
            figure_id: "S4.T1".into(),
            label: "Table 1".into(),
            caption: "Link prediction results.".into(),
            conversion_errors: false,
            cells: vec![
                c(0, 0, 2, 1, "Method"),
                c(0, 1, 1, 2, "FB15k-237"),
                c(0, 3, 1, 1, "Family"),
                c(1, 1, 1, 1, "MR"),
                c(1, 2, 1, 1, "MRR"),
                c(1, 3, 1, 1, "MRR"),
                c(2, 0, 1, 1, "TransE"),
                c(2, 1, 1, 1, "357"),
                c(2, 2, 1, 1, ".294"),
                c(2, 3, 1, 1, ".5"),
                c(3, 0, 1, 1, "Ours (large)"),
                c(3, 1, 1, 1, "177"),
                c(3, 2, 1, 1, "0.338\\pm 0.002"),
                c(3, 3, 1, 1, ".6"),
                c(4, 0, 1, 1, "HypER"),
                c(4, 1, 1, 1, "2"),
                c(4, 2, 1, 1, "958"),
                c(4, 3, 1, 1, ".7"),
            ],
        }],
    }
}

/// A correct answer for the table above, as the model returns it.
const GOOD: &str = r#"{
  "is_results_table": true,
  "orientation": "methods_in_rows",
  "header_rows": 2,
  "label_col": 0,
  "settings": [{ "setting": "protocol", "value": "filtered", "evidence": "S4.p1" }],
  "columns": [
    { "col": 1, "metric": "mr", "dataset": "fb15k-237", "method": null, "role": null, "scale": "raw", "settings": [] },
    { "col": 2, "metric": "mrr", "dataset": "fb15k-237", "method": null, "role": null, "scale": "fraction", "settings": [] },
    { "col": 3, "metric": "mrr", "dataset": "unknown", "method": null, "role": null, "scale": "fraction", "settings": [] }
  ],
  "rows": [
    { "row": 2, "method": "TransE", "role": "baseline", "dataset": null },
    { "row": 3, "method": "Ours (large)", "role": "proposed", "dataset": null },
    { "row": 4, "method": "HypER", "role": "baseline", "dataset": null }
  ],
  "review": [{ "row": 4, "col": 2, "reason": "958 is a typo for .958" }]
}"#;

fn mapping(json: &str) -> TableMapping {
    serde_json::from_str(json).unwrap_or_else(|e| panic!("{e}\n{json}"))
}

fn run(json: &str) -> sota_core::TableOutcome {
    let d = doc();
    verify_mapping(&mapping(json), &d, &d.tables[0], &registry())
}

fn reasons(o: &sota_core::TableOutcome) -> Vec<String> {
    o.review
        .iter()
        .map(|r| format!("{:?}/{:?}: {}", r.row, r.col, r.reason))
        .collect()
}

#[test]
fn a_good_mapping_yields_verified_results() {
    let o = run(GOOD);
    assert_eq!(o.results.len(), 5, "{:?}", reasons(&o)); // 3 rows x 2 registry columns, minus the review cell
    let ours = o
        .results
        .iter()
        .find(|r| r.method == "Ours (large)" && r.metric == "mrr")
        .unwrap();
    assert_eq!((ours.value, ours.std), (0.338, Some(0.002)));
    assert_eq!(ours.locator, "2401.01234v3#S4.T1/r3/c2");
    assert_eq!(ours.settings["protocol"], "filtered");
    assert_eq!(ours.settings["queries"], "both"); // registry default
    assert_eq!(ours.method_id, "ours-large");
    // The model's own review cell is in the queue, not in the results.
    assert_eq!(o.review.len(), 1);
    assert_eq!(
        (o.review[0].row, o.review[0].col, o.review[0].by),
        (Some(4), Some(2), ReviewBy::Model)
    );
}

#[test]
fn columns_outside_the_registry_are_set_aside_not_reviewed() {
    let o = run(GOOD);
    assert_eq!(o.out_of_registry.len(), 1);
    assert!(
        o.out_of_registry[0].contains("col 3"),
        "{:?}",
        o.out_of_registry
    );
    assert!(!o.results.iter().any(|r| r.col == 3));
}

#[test]
fn a_typo_the_model_missed_is_caught_by_the_value_check() {
    let o = run(&GOOD.replace(
        r#"[{ "row": 4, "col": 2, "reason": "958 is a typo for .958" }]"#,
        "[]",
    ));
    assert!(!o.results.iter().any(|r| r.locator.ends_with("/r4/c2")));
    let typo = o
        .review
        .iter()
        .find(|r| (r.row, r.col) == (Some(4), Some(2)))
        .unwrap();
    assert_eq!(typo.by, ReviewBy::Check);
    assert!(
        typo.reason.contains("outside the bounds"),
        "{}",
        typo.reason
    );
}

#[test]
fn a_wrong_row_label_sends_only_that_row_to_review() {
    let o = run(&GOOD.replace(r#""method": "Ours (large)""#, r#""method": "Ours""#));
    assert!(!o.results.iter().any(|r| r.row == 3));
    assert!(o.results.iter().any(|r| r.row == 2));
    assert!(
        reasons(&o)
            .iter()
            .any(|r| r.starts_with("Some(3)/None") && r.contains("Ours (large)")),
        "{:?}",
        reasons(&o)
    );
}

#[test]
fn a_wrong_column_meaning_sends_only_that_column_to_review() {
    let o = run(&GOOD.replace(
        r#""col": 2, "metric": "mrr""#,
        r#""col": 2, "metric": "hits@10""#,
    ));
    assert!(!o.results.iter().any(|r| r.col == 2));
    assert!(o.results.iter().any(|r| r.col == 1));
    assert!(
        reasons(&o)
            .iter()
            .any(|r| r.starts_with("None/Some(2)") && r.contains("hits@10")),
        "{:?}",
        reasons(&o)
    );
}

#[test]
fn a_required_setting_needs_evidence_that_says_so() {
    // Cited paragraph exists but does not mention the protocol.
    let o = run(&GOOD.replace(r#""evidence": "S4.p1""#, r#""evidence": "S4.p2""#));
    assert!(o.results.is_empty(), "{:?}", o.results);
    assert!(
        reasons(&o)
            .iter()
            .any(|r| r.contains("protocol") && r.contains("S4.p2")),
        "{:?}",
        reasons(&o)
    );

    // Cited paragraph does not exist.
    let o = run(&GOOD.replace(r#""evidence": "S4.p1""#, r#""evidence": "S9.p9""#));
    assert!(o.results.is_empty());

    // Not stated at all.
    let o = run(&GOOD.replace(
        r#"[{ "setting": "protocol", "value": "filtered", "evidence": "S4.p1" }]"#,
        "[]",
    ));
    assert!(o.results.is_empty());
    assert!(
        reasons(&o).iter().any(|r| r.contains("protocol")),
        "{:?}",
        reasons(&o)
    );
}

#[test]
fn the_caption_can_be_the_evidence() {
    let mut d = doc();
    d.tables[0].caption = "Filtered link prediction results.".into();
    let m = mapping(&GOOD.replace(r#""evidence": "S4.p1""#, r#""evidence": "caption""#));
    let o = verify_mapping(&m, &d, &d.tables[0], &registry());
    assert_eq!(o.results.len(), 5, "{:?}", reasons(&o));
}

#[test]
fn an_unsupported_non_default_claim_goes_to_review_but_a_default_is_harmless() {
    let tail = GOOD.replace(
        r#"[{ "setting": "protocol", "value": "filtered", "evidence": "S4.p1" }]"#,
        r#"[{ "setting": "protocol", "value": "filtered", "evidence": "S4.p1" },
            { "setting": "queries", "value": "tail", "evidence": "S4.p1" }]"#,
    );
    let o = run(&tail);
    assert!(o.results.is_empty());
    assert!(
        reasons(&o)
            .iter()
            .any(|r| r.contains("queries") && r.contains("tail")),
        "{:?}",
        reasons(&o)
    );

    let both = tail.replace(r#""value": "tail""#, r#""value": "both""#);
    assert_eq!(run(&both).results.len(), 5);
}

#[test]
fn unknown_setting_ids_or_values_are_not_trusted() {
    let o = run(&GOOD.replace(r#""value": "filtered""#, r#""value": "filtered-ish""#));
    assert!(o.results.is_empty());
}

#[test]
fn a_row_without_a_role_goes_to_review() {
    let o = run(&GOOD.replace(
        r#""method": "TransE", "role": "baseline""#,
        r#""method": "TransE", "role": null"#,
    ));
    assert!(!o.results.iter().any(|r| r.row == 2));
    assert!(
        reasons(&o)
            .iter()
            .any(|r| r.starts_with("Some(2)/None") && r.contains("role")),
        "{:?}",
        reasons(&o)
    );
}

#[test]
fn a_table_that_is_not_a_results_table_yields_nothing() {
    let o = run(&GOOD.replace(
        r#""is_results_table": true"#,
        r#""is_results_table": false"#,
    ));
    assert!(o.results.is_empty() && o.review.is_empty());
}

#[test]
fn indices_out_of_range_go_to_review() {
    let o = run(&GOOD.replace(
        r#""row": 4, "method": "HypER""#,
        r#""row": 9, "method": "Nobody""#, // found nowhere: the index stands
    ));
    assert!(
        reasons(&o).iter().any(|r| r.contains("row 9")),
        "{:?}",
        reasons(&o)
    );
    assert!(o.results.iter().any(|r| r.row == 2));
}

#[test]
fn a_dataset_named_only_in_the_caption_can_be_declared_so() {
    // Header says "Results", the caption names FB15k-237 (like an ablation table).
    let mut d = doc();
    d.tables[0].cells[1].text = "Results".into();
    d.tables[0].caption = "Ablation study on FB15k-237.".into();
    let header = GOOD.replace(
        r#""label_col": 0,"#,
        r#""label_col": 0, "dataset_in": "header","#,
    );
    let o = verify_mapping(&mapping(&header), &d, &d.tables[0], &registry());
    assert!(o.results.is_empty(), "the header does not name the dataset");
    let caption = GOOD.replace(
        r#""label_col": 0,"#,
        r#""label_col": 0, "dataset_in": "caption","#,
    );
    let o = verify_mapping(&mapping(&caption), &d, &d.tables[0], &registry());
    assert_eq!(o.results.len(), 5, "{:?}", reasons(&o));
}

/// RotatE's Table 9 shape: an inner header row switches the metric for the
/// rows below it. Those rows must not be read under the top header's metric.
#[test]
fn rows_under_an_inner_header_naming_another_metric_go_to_review() {
    let mut d = doc();
    let c = |row, col, text: &str| sota_core::Cell {
        row,
        col,
        row_span: 1,
        col_span: 1,
        text: text.into(),
        bold: false,
        header: false,
        cites: Vec::new(),
    };
    // Replace row 4 (HypER) with an inner header, and add a data row below it.
    let t = &mut d.tables[0];
    t.cells.retain(|x| x.row != 4);
    t.cells.extend([
        c(4, 0, "Tasks"),
        c(4, 1, "MR"),
        c(4, 2, "Hits@10"),
        c(4, 3, "Hits@10"),
        c(5, 0, "RotatE"),
        c(5, 1, "180"),
        c(5, 2, ".533"),
        c(5, 3, ".9"),
    ]);
    let m = GOOD
        .replace(
            r#"{ "row": 4, "method": "HypER", "role": "baseline", "dataset": null }"#,
            r#"{ "row": 5, "method": "RotatE", "role": "baseline", "dataset": null }"#,
        )
        .replace(
            r#"[{ "row": 4, "col": 2, "reason": "958 is a typo for .958" }]"#,
            "[]",
        );
    let o = verify_mapping(&mapping(&m), &d, &d.tables[0], &registry());
    assert!(
        !o.results.iter().any(|r| r.row == 5),
        "{:?}",
        o.results.iter().filter(|r| r.row == 5).collect::<Vec<_>>()
    );
    assert!(o.results.iter().any(|r| r.row == 2 && r.col == 2));
    assert!(
        reasons(&o).iter().any(|r| r.starts_with("Some(5)/None")
            && r.contains("inner header")
            && r.contains("hits@10")),
        "{:?}",
        reasons(&o)
    );
}

/// AdaProp (2205.15319v2, Table 2): the model shifted its row numbers by one
/// after a row-spanning group label, so the last method pointed past the end.
/// The out-of-range row must not switch off the label check for the others.
#[test]
fn an_out_of_range_row_does_not_disable_the_other_checks() {
    let m = GOOD
        .replace(
            r#"{ "row": 3, "method": "Ours (large)""#,
            r#"{ "row": 3, "method": "Ours""#, // inexact: cannot be located by text
        )
        .replace(
            r#"{ "row": 4, "method": "HypER""#,
            r#"{ "row": 9, "method": "Nobody""#, // found nowhere
        );
    let o = run(&m);
    assert!(
        !o.results.iter().any(|r| r.row == 3),
        "row 3 reads \"Ours (large)\", not \"Ours\": {:?}",
        o.results
            .iter()
            .filter(|r| r.row == 3)
            .map(|r| &r.method)
            .collect::<Vec<_>>()
    );
    assert!(
        reasons(&o).iter().any(|r| r.contains("row 9")),
        "{:?}",
        reasons(&o)
    );
    assert!(
        reasons(&o)
            .iter()
            .any(|r| r.starts_with("Some(3)/None") && r.contains("Ours (large)")),
        "{:?}",
        reasons(&o)
    );
    assert!(
        o.results.iter().any(|r| r.row == 2),
        "the correct row still counts"
    );
}

/// The AdaProp error, by design: a row number off by one but a label
/// copied exactly. The label, unique in the label column, decides the row.
#[test]
fn a_row_is_located_by_its_unique_label_when_the_index_is_off() {
    let m = GOOD
        .replace(
            r#"{ "row": 2, "method": "TransE""#,
            r#"{ "row": 3, "method": "TransE""#,
        )
        .replace(
            r#"{ "row": 3, "method": "Ours (large)""#,
            r#"{ "row": 4, "method": "Ours (large)""#,
        )
        .replace(
            r#"{ "row": 4, "method": "HypER""#,
            r#"{ "row": 5, "method": "HypER""#,
        )
        .replace(
            r#"[{ "row": 4, "col": 2, "reason": "958 is a typo for .958" }]"#,
            "[]",
        );
    let o = run(&m);
    let transe = o
        .results
        .iter()
        .find(|r| r.method == "TransE" && r.metric == "mrr")
        .unwrap();
    assert_eq!((transe.row, transe.value), (2, 0.294));
    let ours = o
        .results
        .iter()
        .find(|r| r.method == "Ours (large)" && r.metric == "mrr")
        .unwrap();
    assert_eq!(ours.row, 3);
    // HypER's MRR cell (958) still fails its own check, at its real row.
    assert!(
        o.review
            .iter()
            .any(|r| (r.row, r.col) == (Some(4), Some(2))),
        "{:?}",
        reasons(&o)
    );
}

/// A label printed twice (RotatE's Table 9 lists TransE twice) cannot decide
/// a row on its own: the given index must already be right.
#[test]
fn a_repeated_label_never_moves_a_row() {
    let mut d = doc();
    for c in d.tables[0].cells.iter_mut() {
        if c.text == "HypER" {
            c.text = "TransE".into();
        }
    }
    let m = GOOD
        .replace(
            r#"{ "row": 4, "method": "HypER""#,
            r#"{ "row": 3, "method": "TransE""#,
        )
        .replace(
            r#"{ "row": 3, "method": "Ours (large)""#,
            r#"{ "row": 4, "method": "Ours (large)""#,
        );
    let o = verify_mapping(&mapping(&m), &d, &d.tables[0], &registry());
    assert!(
        !o.results.iter().any(|r| r.method == "TransE" && r.row == 3),
        "{:?}",
        o.results
    );
    assert!(
        reasons(&o).iter().any(|r| r.starts_with("Some(3)/None")),
        "{:?}",
        reasons(&o)
    );
}
