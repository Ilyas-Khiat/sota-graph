//! Against a running Neo4j (`docker compose up -d`). Opt-in:
//! `cargo test -p sota-store --test live -- --ignored`
//!
//! Every node these tests create has an id starting with `test-` (results and
//! papers: `test.`), and only those are deleted; the user's graph is never touched.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use serde_json::{Value, json};
use sota_core::{Ontology, Report, Role};
use sota_store::{Neo4j, fetch_reports, load, result_row};

const ONTOLOGY: &str = include_str!("../../../config/ontology.toml");

/// The tests share the `test-` nodes: one at a time.
static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn db() -> Neo4j {
    Neo4j::from_vars(|name| std::env::var(name).ok())
}

fn clean(db: &Neo4j) {
    db.run(
        "MATCH (n) WHERE n.id STARTS WITH 'test-' OR n.id STARTS WITH 'test.'
                      OR n.arxiv_id STARTS WITH 'test.'
         DETACH DELETE n",
        json!({}),
    )
    .unwrap();
}

fn report(row: usize, method: &str, role: Role, value: f64) -> Report {
    let mut settings = BTreeMap::new();
    settings.insert("protocol".to_owned(), "filtered".to_owned());
    Report {
        paper: "test.0001v2".into(),
        locator: format!("test.0001v2#T1/r{row}/c1"),
        method_id: method.into(),
        method_name: method.into(),
        method_known: true,
        role,
        dataset: "test-dataset".into(),
        metric: "test-metric".into(),
        split: "test".into(),
        settings,
        value,
        value_text: value.to_string(),
    }
}

fn rows(reports: &[Report]) -> Vec<Value> {
    reports
        .iter()
        .map(|r| {
            let mut row = result_row(r, "A test paper");
            let o = row.as_object_mut().unwrap();
            o.insert("dataset_name".into(), "Test dataset".into());
            o.insert("metric_name".into(), "Test metric".into());
            o.insert("direction".into(), "higher_is_better".into());
            row
        })
        .collect()
}

fn count(db: &Neo4j, statement: &str) -> i64 {
    db.run(statement, json!({})).unwrap()[0][0]
        .as_i64()
        .unwrap()
}

#[test]
#[ignore = "needs Neo4j: docker compose up -d"]
fn load_is_idempotent_and_reads_back_identical() {
    let _serial = serial();
    let db = db();
    let ontology = Ontology::from_toml_str("ontology.toml", ONTOLOGY).unwrap();
    clean(&db);
    let reports = vec![
        report(1, "test-ours", Role::Proposed, 0.412),
        report(2, "test-base", Role::Baseline, 0.3),
    ];
    load(&db, &ontology, &rows(&reports)).unwrap();
    load(&db, &ontology, &rows(&reports)).unwrap();

    let results = count(
        &db,
        "MATCH (:Paper {arxiv_id: 'test.0001'})-[:REPORTS]->(r:Result) RETURN count(r)",
    );
    let proposes = count(
        &db,
        "MATCH (:Paper {arxiv_id: 'test.0001'})-[:PROPOSES]->(m:Method) RETURN count(m)",
    );
    let back = fetch_reports(&db, "test-dataset", "test-metric").unwrap();
    clean(&db);

    assert_eq!(results, 2, "a second load must not duplicate");
    assert_eq!(proposes, 1, "only the proposed method is PROPOSED");
    assert_eq!(back, reports);
}

#[test]
#[ignore = "needs Neo4j: docker compose up -d"]
fn a_new_version_of_a_paper_is_one_paper() {
    let _serial = serial();
    let db = db();
    let ontology = Ontology::from_toml_str("ontology.toml", ONTOLOGY).unwrap();
    clean(&db);
    let mut v3 = report(1, "test-ours", Role::Proposed, 0.42);
    v3.paper = "test.0001v3".into();
    v3.locator = "test.0001v3#T1/r1/c1".into();
    let reports = vec![report(1, "test-ours", Role::Proposed, 0.412), v3];
    load(&db, &ontology, &rows(&reports)).unwrap();
    let versions = db
        .run(
            "MATCH (p:Paper {arxiv_id: 'test.0001'}) RETURN p.versions_seen",
            json!({}),
        )
        .unwrap();
    clean(&db);
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0][0], json!(["test.0001v2", "test.0001v3"]));
}

fn test_nodes(db: &Neo4j) -> i64 {
    count(
        db,
        "MATCH (n) WHERE n.id STARTS WITH 'test' OR n.arxiv_id STARTS WITH 'test'
         RETURN count(n)",
    )
}

#[test]
#[ignore = "needs Neo4j: docker compose up -d"]
fn cleaning_up_leaves_no_test_node_behind() {
    let _serial = serial();
    let db = db();
    let ontology = Ontology::from_toml_str("ontology.toml", ONTOLOGY).unwrap();
    load(
        &db,
        &ontology,
        &rows(&[report(1, "test-ours", Role::Proposed, 0.412)]),
    )
    .unwrap();
    clean(&db);
    assert_eq!(test_nodes(&db), 0);
}

/// Adding an alias moves a result to another method id: the reload must
/// leave it attached to the new method only, with the paper proposing it.
#[test]
#[ignore = "needs Neo4j: docker compose up -d"]
fn reloading_moves_a_result_to_its_new_method() {
    let _serial = serial();
    let db = db();
    let ontology = Ontology::from_toml_str("ontology.toml", ONTOLOGY).unwrap();
    clean(&db);
    let before = report(1, "test-old", Role::Proposed, 0.412);
    let mut after = report(1, "test-new", Role::Proposed, 0.412);
    after.method_name = "Test New".into();
    load(&db, &ontology, &rows(&[before])).unwrap();
    load(&db, &ontology, &rows(std::slice::from_ref(&after))).unwrap();

    let back = fetch_reports(&db, "test-dataset", "test-metric").unwrap();
    let proposed = db
        .run(
            "MATCH (:Paper {arxiv_id: 'test.0001'})-[:PROPOSES]->(m:Method) RETURN m.id",
            json!({}),
        )
        .unwrap();
    let old = count(&db, "MATCH (m:Method {id: 'test-old'}) RETURN count(m)");
    clean(&db);

    assert_eq!(back, [after], "one result, on the new method only");
    assert_eq!(proposed, [[json!("test-new")]]);
    assert_eq!(old, 0, "a method left with no result is removed");
}

#[test]
#[ignore = "needs Neo4j: docker compose up -d"]
fn reloading_updates_role_and_method_identity() {
    let _serial = serial();
    let db = db();
    let ontology = Ontology::from_toml_str("ontology.toml", ONTOLOGY).unwrap();
    clean(&db);
    let mut before = report(1, "test-m", Role::Proposed, 0.412);
    before.method_name = "test m".into();
    before.method_known = false;
    let mut after = report(1, "test-m", Role::Baseline, 0.412);
    after.method_name = "Test M".into();
    load(&db, &ontology, &rows(&[before])).unwrap();
    load(&db, &ontology, &rows(std::slice::from_ref(&after))).unwrap();

    let back = fetch_reports(&db, "test-dataset", "test-metric").unwrap();
    let proposes = count(
        &db,
        "MATCH (:Paper {arxiv_id: 'test.0001'})-[:PROPOSES]->() RETURN count(*)",
    );
    clean(&db);

    assert_eq!(back, [after], "name and known follow the config");
    assert_eq!(proposes, 0, "a baseline is not proposed");
}

#[test]
#[ignore = "needs Neo4j: docker compose up -d"]
fn cypher_errors_are_reported_with_their_code() {
    let err = db().run("RETURN $missing +", json!({})).unwrap_err();
    assert!(err.to_string().contains("SyntaxError"), "{err}");
}
