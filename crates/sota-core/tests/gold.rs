use sota_core::{
    Cell, Channel, ConfigError, Document, GoldFile, Normalizer, Registry, Role, Table,
};

const KGC: &str = include_str!("../../../config/registry/kgc.toml");
const NORMALIZATION: &str = include_str!("../../../config/normalization.toml");

fn registry() -> Registry {
    let n = Normalizer::from_toml_str("normalization.toml", NORMALIZATION).unwrap();
    Registry::build(n, &[("kgc.toml", KGC)]).unwrap()
}

/// | Method       | FB15k-237 (2 cols) |        |
/// |              | MR                 | MRR    |
/// | TransE       | 357                | .294   |
/// | Ours (large) | 177                | 0.338\pm 0.002 |
/// | Ablation     | -                  | 33.1   |   <- a percent column in a fraction table
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
        sections: Vec::new(),
        unparsed_tables: Vec::new(),
        tables: vec![Table {
            id: "S4.T1".into(),
            figure_id: "S4.T1".into(),
            label: "Table 1".into(),
            caption: "Link prediction on FB15k-237.".into(),
            conversion_errors: false,
            cells: vec![
                c(0, 0, 2, 1, "Method"),
                c(0, 1, 1, 2, "FB15k-237"),
                c(1, 1, 1, 1, "MR"),
                c(1, 2, 1, 1, "MRR"),
                c(2, 0, 1, 1, "TransE"),
                c(2, 1, 1, 1, "357"),
                c(2, 2, 1, 1, ".294"),
                c(3, 0, 1, 1, "Ours (large)"),
                c(3, 1, 1, 1, "177"),
                c(3, 2, 1, 1, "0.338\\pm 0.002"),
                c(4, 0, 1, 1, "Ablation"),
                c(4, 1, 1, 1, "-"),
                c(4, 2, 1, 1, "33.1"),
            ],
        }],
    }
}

const SPEC: &str = r#"
paper = "2401.01234v3"

[[table]]
id = "S4.T1"
split = "test"
header_rows = 2
settings = { protocol = "filtered" }
evidence = "S4.p1 states filtered ranking"
columns = [
  { col = 1, dataset = "fb15k-237", metric = "mr" },
  { col = 2, dataset = "fb15k-237", metric = "mrr" },
]
rows = [
  { row = 2, method = "TransE", method_id = "transe", role = "baseline" },
  { row = 3, method = "Ours (large)", method_id = "ours", role = "proposed" },
]
"#;

fn spec(text: &str) -> GoldFile {
    GoldFile::from_toml_str("gold.toml", text).unwrap()
}

fn validation_problems(text: &str) -> Vec<String> {
    spec(text).validate(&registry())
}

fn expansion_problems(text: &str) -> Vec<String> {
    spec(text)
        .expand(&doc(), &registry())
        .expect_err("expected expansion problems")
}

fn has(problems: &[String], needles: &[&str]) -> bool {
    problems
        .iter()
        .any(|p| needles.iter().all(|needle| p.contains(needle)))
}

#[test]
fn a_correct_spec_validates_and_expands() {
    let gold = spec(SPEC);
    assert_eq!(gold.validate(&registry()), Vec::<String>::new());
    let expansion = gold.expand(&doc(), &registry()).unwrap();
    assert_eq!(expansion.missing, 0);
    let r = &expansion.results;
    assert_eq!(r.len(), 4);

    let ours_mrr = r
        .iter()
        .find(|x| x.method_id == "ours" && x.metric == "mrr")
        .unwrap();
    assert_eq!(ours_mrr.value, 0.338);
    assert_eq!(ours_mrr.std, Some(0.002));
    assert_eq!(ours_mrr.value_text, "0.338\\pm 0.002");
    assert_eq!(ours_mrr.locator, "2401.01234v3#S4.T1/r3/c2");
    assert_eq!(ours_mrr.role, Role::Proposed);
    assert_eq!(ours_mrr.dataset, "fb15k-237");
    assert_eq!(ours_mrr.split, "test");
    assert_eq!(
        ours_mrr.settings.get("protocol").map(String::as_str),
        Some("filtered")
    );

    let transe_mr = r
        .iter()
        .find(|x| x.method_id == "transe" && x.metric == "mr")
        .unwrap();
    assert_eq!(transe_mr.value, 357.0);
}

#[test]
fn a_percent_cell_in_a_fraction_column_is_caught() {
    let text = SPEC.replace(
        r#"role = "proposed" },"#,
        r#"role = "proposed" },
  { row = 4, method = "Ablation", method_id = "ours-ablation", role = "variant" },"#,
    );
    // Row 4 prints MRR in percent (33.1) while the column is a fraction.
    let problems = expansion_problems(&text);
    assert!(
        has(&problems, &["r4/c2", "outside the bounds"]),
        "{problems:?}"
    );
}

#[test]
fn a_cell_scale_override_fixes_a_percent_cell() {
    let text = SPEC.replace(
        r#"role = "proposed" },"#,
        r#"role = "proposed" },
  { row = 4, method = "Ablation", method_id = "ours-ablation", role = "variant" },"#,
    ) + "cell_scales = [{ row = 4, col = 2, scale = \"percent\" }]\n";
    let expansion = spec(&text).expand(&doc(), &registry()).unwrap();
    assert_eq!(expansion.missing, 1); // row 4, MR is "-"
    let ablation = expansion
        .results
        .iter()
        .find(|x| x.method_id == "ours-ablation")
        .unwrap();
    assert_eq!(ablation.value, 0.331);
}

#[test]
fn a_wrong_row_label_is_caught() {
    let problems =
        expansion_problems(&SPEC.replace(r#"method = "Ours (large)""#, r#"method = "Ours""#));
    assert!(has(&problems, &["row 3", "Ours (large)"]), "{problems:?}");
}

#[test]
fn a_wrong_column_metric_is_caught_by_its_header() {
    let problems = expansion_problems(&SPEC.replace(
        r#"{ col = 2, dataset = "fb15k-237", metric = "mrr" }"#,
        r#"{ col = 2, dataset = "fb15k-237", metric = "hits@10" }"#,
    ));
    assert!(has(&problems, &["col 2", "hits@10", "MRR"]), "{problems:?}");
}

#[test]
fn a_wrong_column_dataset_is_caught_by_its_header() {
    let problems = expansion_problems(&SPEC.replace(
        r#"{ col = 1, dataset = "fb15k-237", metric = "mr" }"#,
        r#"{ col = 1, dataset = "wn18rr", metric = "mr" }"#,
    ));
    assert!(has(&problems, &["col 1", "wn18rr"]), "{problems:?}");
}

#[test]
fn the_dataset_may_come_from_the_caption_instead() {
    // Same table, but say the header did not name the dataset.
    let mut d = doc();
    d.tables[0].cells[1].text = "Results".into();
    let text = SPEC.replace(
        "header_rows = 2",
        "header_rows = 2\ndataset_in = \"caption\"",
    );
    assert!(spec(&text).expand(&d, &registry()).is_ok());
    assert!(spec(SPEC).expand(&d, &registry()).is_err());
}

#[test]
fn out_of_range_and_unknown_tables_are_caught() {
    let problems = expansion_problems(&SPEC.replace("row = 3,", "row = 9,"));
    assert!(has(&problems, &["row 9"]), "{problems:?}");
    let problems = expansion_problems(&SPEC.replace(r#"id = "S4.T1""#, r#"id = "S9.T9""#));
    assert!(has(&problems, &["S9.T9"]), "{problems:?}");
}

#[test]
fn validation_checks_ids_against_the_registry() {
    let p = validation_problems(&SPEC.replace(
        r#"dataset = "fb15k-237", metric = "mr""#,
        r#"dataset = "FB15k-237", metric = "mr""#,
    ));
    assert!(has(&p, &["FB15k-237", "dataset"]), "{p:?}"); // ids, not aliases
    let p = validation_problems(&SPEC.replace(r#"metric = "mrr""#, r#"metric = "map""#));
    assert!(has(&p, &["map"]), "{p:?}");
}

#[test]
fn validation_requires_required_settings_with_known_values() {
    let p = validation_problems(
        &SPEC.replace(r#"settings = { protocol = "filtered" }"#, "settings = {}"),
    );
    assert!(has(&p, &["protocol", "required"]), "{p:?}");
    let p = validation_problems(&SPEC.replace(r#"protocol = "filtered""#, r#"protocol = "filt""#));
    assert!(has(&p, &["filt"]), "{p:?}"); // value ids, not aliases
    let p = validation_problems(&SPEC.replace(
        r#"protocol = "filtered""#,
        r#"protocol = "filtered", seed = "1""#,
    ));
    assert!(has(&p, &["seed"]), "{p:?}");
}

#[test]
fn validation_checks_structure() {
    let p =
        validation_problems(&SPEC.replace(r#"paper = "2401.01234v3""#, r#"paper = "2401.01234""#));
    assert!(has(&p, &["pinned"]), "{p:?}");
    let p = validation_problems(&SPEC.replace("row = 3,", "row = 2,"));
    assert!(has(&p, &["duplicate row 2"]), "{p:?}");
    let p = validation_problems(&SPEC.replace("col = 2,", "col = 1,"));
    assert!(has(&p, &["duplicate col 1"]), "{p:?}");
    let p = validation_problems(&SPEC.replace("row = 2,", "row = 1,"));
    assert!(has(&p, &["row 1", "header"]), "{p:?}");
    let p = validation_problems(&SPEC.replace(r#"method_id = "ours""#, r#"method_id = "Ours""#));
    assert!(has(&p, &["Ours", "method_id"]), "{p:?}");
    let p = validation_problems(&SPEC.replace(r#"split = "test""#, r#"split = "dev""#));
    assert!(has(&p, &["dev", "split"]), "{p:?}");
}

#[test]
fn unknown_fields_and_roles_are_parse_errors() {
    for text in [
        SPEC.replace("header_rows", "headers"),
        SPEC.replace(r#"role = "baseline""#, r#"role = "theirs""#),
    ] {
        assert!(
            matches!(
                GoldFile::from_toml_str("gold.toml", &text),
                Err(ConfigError::Parse { .. })
            ),
            "{text}"
        );
    }
}

// ------------------------------------------------ snapshot checks (no paper needed)

fn snapshot() -> Vec<sota_core::GoldResult> {
    spec(SPEC).expand(&doc(), &registry()).unwrap().results
}

fn snapshot_problems(results: &[sota_core::GoldResult]) -> Vec<String> {
    spec(SPEC).check_snapshot(results, &registry())
}

#[test]
fn a_fresh_snapshot_agrees_with_its_spec() {
    assert_eq!(snapshot_problems(&snapshot()), Vec::<String>::new());
}

#[test]
fn a_snapshot_out_of_step_with_its_spec_is_caught() {
    let mut s = snapshot();
    s[0].role = Role::Proposed; // TransE is a baseline in the spec
    assert!(has(&snapshot_problems(&s), &["r2/c1", "role"]));

    let mut s = snapshot();
    s[1].metric = "hits@10".into();
    assert!(has(&snapshot_problems(&s), &["r2/c2", "metric"]));

    let mut s = snapshot();
    s[1].value = 0.3; // .294 does not read 0.3
    assert!(has(&snapshot_problems(&s), &["r2/c2", "value"]));

    let mut s = snapshot();
    s[0].locator = "2401.01234v3#S4.T1/r9/c9".into();
    assert!(has(&snapshot_problems(&s), &["locator"]));
}

#[test]
fn unmapped_or_duplicate_snapshot_rows_are_caught() {
    let mut s = snapshot();
    let mut extra = s[0].clone();
    extra.row = 4;
    extra.locator = "2401.01234v3#S4.T1/r4/c1".into();
    s.push(extra);
    assert!(has(&snapshot_problems(&s), &["r4/c1", "not mapped"]));

    let mut s = snapshot();
    s.push(s[0].clone());
    assert!(has(&snapshot_problems(&s), &["duplicate", "r2/c1"]));

    let mut s = snapshot();
    s[0].paper = "2401.01234v2".into();
    assert!(has(&snapshot_problems(&s), &["2401.01234v2"]));
}

// ------------------------------------------------ shapes met in real tables

/// ComplEx-style header (Table 2 of 1606.06357): three header rows, the MRR
/// split into Filter / Raw, and "Hits at" above "1" / "10".
fn split_header_doc() -> Document {
    let c = |row, col, row_span, col_span, text: &str| Cell {
        row,
        col,
        row_span,
        col_span,
        text: text.into(),
        bold: false,
        header: row < 3,
        cites: Vec::new(),
    };
    let mut d = doc();
    d.tables[0].cells = vec![
        c(0, 0, 1, 1, ""),
        c(0, 1, 1, 4, "WN18"),
        c(1, 0, 1, 1, ""),
        c(1, 1, 1, 2, "MRR"),
        c(1, 3, 1, 2, "Hits at"),
        c(2, 0, 1, 1, "Model"),
        c(2, 1, 1, 1, "Filter"),
        c(2, 2, 1, 1, "Raw"),
        c(2, 3, 1, 1, "1"),
        c(2, 4, 1, 1, "10"),
        c(3, 0, 1, 1, "TransE"),
        c(3, 1, 1, 1, "0.454"),
        c(3, 2, 1, 1, "0.335"),
        c(3, 3, 1, 1, "0.089"),
        c(3, 4, 1, 1, "0.934"),
        c(4, 0, 1, 1, "HolE*"),
        c(4, 1, 1, 1, "0.938"),
        c(4, 2, 1, 1, "0.616"),
        c(4, 3, 1, 1, "0.93"),
        c(4, 4, 1, 1, "958"),
    ];
    d
}

const SPLIT_SPEC: &str = r#"
paper = "2401.01234v3"

[[table]]
id = "S4.T1"
header_rows = 3
settings = { protocol = "filtered" }
columns = [
  { col = 1, dataset = "wn18", metric = "mrr" },
  { col = 2, dataset = "wn18", metric = "mrr", settings = { protocol = "raw" } },
  { col = 3, dataset = "wn18", metric = "hits@1" },
  { col = 4, dataset = "wn18", metric = "hits@10" },
]
rows = [
  { row = 3, method = "TransE", method_id = "transe", role = "baseline" },
  { row = 4, method = "HolE*", method_id = "hole", role = "baseline" },
]
review_cells = [{ row = 4, col = 4, reason = "printed 958, a typo for .958" }]
"#;

fn split_expansion(text: &str) -> Result<sota_core::Expansion, Vec<String>> {
    spec(text).expand(&split_header_doc(), &registry())
}

#[test]
fn headers_split_over_rows_are_joined() {
    let e = split_expansion(SPLIT_SPEC).unwrap();
    let hits10: Vec<f64> = e
        .results
        .iter()
        .filter(|r| r.metric == "hits@10")
        .map(|r| r.value)
        .collect();
    assert_eq!(hits10, [0.934]); // "Hits at" + "10"
}

#[test]
fn settings_can_differ_per_column() {
    let e = split_expansion(SPLIT_SPEC).unwrap();
    let protocol =
        |col: usize| e.results.iter().find(|r| r.col == col).unwrap().settings["protocol"].clone();
    assert_eq!(
        (protocol(1), protocol(2), protocol(3)),
        ("filtered".into(), "raw".into(), "filtered".into())
    );
}

#[test]
fn a_column_setting_contradicting_its_header_is_caught() {
    let text = SPLIT_SPEC.replace(r#", settings = { protocol = "raw" }"#, "");
    let problems = split_expansion(&text).unwrap_err();
    assert!(
        has(&problems, &["col 2", "protocol", "raw", "filtered"]),
        "{problems:?}"
    );
}

#[test]
fn review_cells_are_left_out_and_counted() {
    let e = split_expansion(SPLIT_SPEC).unwrap();
    assert_eq!(e.review, 1);
    assert!(!e.results.iter().any(|r| r.locator.ends_with("/r4/c4")));
    // Without the review entry, the typo fails the expansion.
    let text = SPLIT_SPEC.replace(
        r#"review_cells = [{ row = 4, col = 4, reason = "printed 958, a typo for .958" }]"#,
        "",
    );
    assert!(has(
        &split_expansion(&text).unwrap_err(),
        &["r4/c4", "outside the bounds"]
    ));
}

#[test]
fn a_whole_row_can_go_to_review_without_being_mapped() {
    let text = SPLIT_SPEC
        .replace(
            r#"review_cells = [{ row = 4, col = 4, reason = "printed 958, a typo for .958" }]"#,
            r#"review_cells = [{ row = 4, reason = "evaluated with 50 negatives" }]"#,
        )
        .replace(
            r#"  { row = 4, method = "HolE*", method_id = "hole", role = "baseline" },"#,
            "",
        );
    let e = split_expansion(&text).unwrap();
    assert_eq!(e.results.len(), 4);
    assert_eq!(e.review, 4);
}

#[test]
fn review_entries_are_validated() {
    let bad = SPLIT_SPEC.replace(
        r#"reason = "printed 958, a typo for .958""#,
        r#"reason = """#,
    );
    assert!(has(
        &spec(&bad).validate(&registry()),
        &["review", "reason"]
    ));
    let bad = SPLIT_SPEC.replace("row = 4, col = 4, reason", "row = 4, col = 7, reason");
    assert!(has(&spec(&bad).validate(&registry()), &["review", "col 7"]));
}

#[test]
fn a_snapshot_result_in_a_review_cell_is_caught() {
    let gold = spec(SPLIT_SPEC);
    let mut results = gold
        .expand(&split_header_doc(), &registry())
        .unwrap()
        .results;
    let mut typo = results[0].clone();
    typo.row = 4;
    typo.col = 4;
    typo.locator = "2401.01234v3#S4.T1/r4/c4".into();
    results.push(typo);
    assert!(has(
        &gold.check_snapshot(&results, &registry()),
        &["r4/c4", "review"]
    ));
}

// ------------------------------------------------ defaults and transposed tables

#[test]
fn results_carry_setting_defaults() {
    let e = spec(SPEC).expand(&doc(), &registry()).unwrap();
    let s = &e.results[0].settings;
    assert_eq!(s.get("queries").map(String::as_str), Some("both"));
    assert_eq!(s.get("regime").map(String::as_str), Some("supervised"));
    // A fresh snapshot still agrees with its spec.
    assert_eq!(
        spec(SPEC).check_snapshot(&e.results, &registry()),
        Vec::<String>::new()
    );
}

/// ULTRA-style table (A6.T10 of 2310.04562): datasets in rows, (method, regime)
/// x metric in columns.
fn transposed_doc() -> Document {
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
    let mut d = doc();
    d.tables[0].cells = vec![
        c(0, 0, 1, 1, "Dataset"),
        c(0, 1, 1, 2, "Ultra 0-shot"),
        c(0, 3, 1, 2, "Ultra fine-tuned"),
        c(1, 0, 1, 1, ""),
        c(1, 1, 1, 1, "MRR"),
        c(1, 2, 1, 1, "Hits@10"),
        c(1, 3, 1, 1, "MRR"),
        c(1, 4, 1, 1, "Hits@10"),
        c(2, 0, 1, 1, "FB15k237"),
        c(2, 1, 1, 1, ""),
        c(2, 2, 1, 1, ""),
        c(2, 3, 1, 1, "0.368"),
        c(2, 4, 1, 1, "0.564"),
        c(3, 0, 1, 1, "YAGO 310"),
        c(3, 1, 1, 1, "0.451"),
        c(3, 2, 1, 1, "0.615"),
        c(3, 3, 1, 1, "0.557 ± 0.009"),
        c(3, 4, 1, 1, "0.710 ± 0.003"),
    ];
    d
}

const TRANSPOSED: &str = r#"
paper = "2401.01234v3"

[[table]]
id = "S4.T1"
orientation = "datasets_in_rows"
header_rows = 2
settings = { protocol = "filtered" }
columns = [
  { col = 1, metric = "mrr", method = "Ultra", method_id = "ultra", role = "proposed", settings = { regime = "zero-shot" } },
  { col = 2, metric = "hits@10", method = "Ultra", method_id = "ultra", role = "proposed", settings = { regime = "zero-shot" } },
  { col = 3, metric = "mrr", method = "Ultra", method_id = "ultra", role = "proposed", settings = { regime = "fine-tuned" } },
  { col = 4, metric = "hits@10", method = "Ultra", method_id = "ultra", role = "proposed", settings = { regime = "fine-tuned" } },
]
rows = [
  { row = 2, dataset = "fb15k-237" },
  { row = 3, dataset = "yago3-10" },
]
"#;

#[test]
fn a_transposed_table_expands_with_datasets_from_rows() {
    let e = spec(TRANSPOSED)
        .expand(&transposed_doc(), &registry())
        .unwrap();
    assert_eq!((e.results.len(), e.missing), (6, 2));
    let yago_ft = e
        .results
        .iter()
        .find(|r| {
            r.dataset == "yago3-10" && r.metric == "mrr" && r.settings["regime"] == "fine-tuned"
        })
        .unwrap();
    assert_eq!((yago_ft.value, yago_ft.std), (0.557, Some(0.009)));
    assert_eq!(
        (yago_ft.method.as_str(), yago_ft.method_id.as_str()),
        ("Ultra", "ultra")
    );
    let snapshot_problems = spec(TRANSPOSED).check_snapshot(&e.results, &registry());
    assert_eq!(snapshot_problems, Vec::<String>::new());
}

#[test]
fn a_transposed_row_label_must_name_its_dataset() {
    let text = TRANSPOSED.replace(
        r#"{ row = 3, dataset = "yago3-10" }"#,
        r#"{ row = 3, dataset = "wn18rr" }"#,
    );
    let p = spec(&text)
        .expand(&transposed_doc(), &registry())
        .unwrap_err();
    assert!(has(&p, &["row 3", "YAGO 310", "wn18rr"]), "{p:?}");
}

#[test]
fn a_transposed_column_header_must_name_its_method_and_agree_on_settings() {
    let text = TRANSPOSED.replacen(r#"method = "Ultra""#, r#"method = "NBFNet""#, 1);
    let p = spec(&text)
        .expand(&transposed_doc(), &registry())
        .unwrap_err();
    assert!(has(&p, &["col 1", "NBFNet"]), "{p:?}");

    let text = TRANSPOSED.replacen(r#"regime = "zero-shot""#, r#"regime = "fine-tuned""#, 1);
    let p = spec(&text)
        .expand(&transposed_doc(), &registry())
        .unwrap_err();
    assert!(
        has(&p, &["col 1", "regime", "zero-shot", "fine-tuned"]),
        "{p:?}"
    );
}

#[test]
fn each_orientation_requires_its_own_fields() {
    let p = validation_problems(&SPEC.replace(r#", method_id = "transe""#, ""));
    assert!(has(&p, &["row 2", "method_id"]), "{p:?}");
    let p = spec(&TRANSPOSED.replacen(r#", method = "Ultra""#, "", 1)).validate(&registry());
    assert!(has(&p, &["col 1", "method"]), "{p:?}");
    let p = spec(&TRANSPOSED.replace(r#"{ row = 2, dataset = "fb15k-237" }"#, r#"{ row = 2 }"#))
        .validate(&registry());
    assert!(has(&p, &["row 2", "dataset"]), "{p:?}");
}
