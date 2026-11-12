//! Offline tests: schema, row encoding and the writer's vocabulary. The
//! tests against a running Neo4j are in `live.rs` and opt-in.

use std::collections::BTreeMap;

use sota_core::{Ontology, Report, Role};
use sota_store::{
    NODE_LABELS, RELATIONSHIPS, basic_auth, constraints, decode_settings, encode_settings,
    result_row,
};

const ONTOLOGY: &str = include_str!("../../../config/ontology.toml");

fn ontology() -> Ontology {
    Ontology::from_toml_str("ontology.toml", ONTOLOGY).unwrap()
}

#[test]
fn constraints_come_from_the_ontology_keys() {
    let c = constraints(&ontology());
    assert_eq!(c.len(), 5);
    assert!(c.contains(
        &"CREATE CONSTRAINT paper_arxiv_id IF NOT EXISTS FOR (n:Paper) REQUIRE n.arxiv_id IS UNIQUE".to_owned()
    ));
    assert!(
        c.contains(
            &"CREATE CONSTRAINT result_id IF NOT EXISTS FOR (n:Result) REQUIRE n.id IS UNIQUE"
                .to_owned()
        )
    );
}

/// The writer may only create what the ontology declares, with the declared ends.
#[test]
fn the_writer_speaks_the_ontology() {
    let o = ontology();
    for label in NODE_LABELS {
        assert!(
            o.node(label).is_some(),
            "label {label} is not in the ontology"
        );
    }
    for (from, rel, to) in RELATIONSHIPS {
        let r = o
            .relationship(rel)
            .unwrap_or_else(|| panic!("{rel} is not in the ontology"));
        assert!(
            r.from.iter().any(|f| f == from) && r.to.iter().any(|t| t == to),
            "{from}-{rel}->{to}"
        );
    }
}

#[test]
fn settings_round_trip_as_sorted_strings() {
    let mut s = BTreeMap::new();
    s.insert("queries".to_owned(), "both".to_owned());
    s.insert("protocol".to_owned(), "filtered".to_owned());
    let encoded = encode_settings(&s);
    assert_eq!(encoded, ["protocol=filtered", "queries=both"]);
    assert_eq!(decode_settings(&encoded), s);
}

#[test]
fn a_result_row_carries_everything_the_ontology_requires() {
    let mut settings = BTreeMap::new();
    settings.insert("protocol".to_owned(), "filtered".to_owned());
    let r = Report {
        paper: "1902.10197v1".into(),
        locator: "1902.10197v1#S4.T5/r7/c2".into(),
        method_id: "rotate".into(),
        method_name: "RotatE".into(),
        method_known: true,
        role: Role::Proposed,
        dataset: "fb15k-237".into(),
        metric: "mrr".into(),
        split: "test".into(),
        settings,
        value: 0.338,
        value_text: ".338".into(),
    };
    let row = result_row(&r, "RotatE: Knowledge Graph Embedding");
    assert_eq!(row["paper_id"], "1902.10197");
    assert_eq!(row["paper_version"], "1902.10197v1");
    assert_eq!(row["id"], "1902.10197v1#S4.T5/r7/c2");
    assert_eq!(row["role"], "proposed");
    assert_eq!(row["raw_text"], ".338");
    assert_eq!(row["channel"], "arxiv_html");
    assert_eq!(row["settings"], serde_json::json!(["protocol=filtered"]));
    assert_eq!(row["method_known"], true);
}

#[test]
fn basic_auth_is_standard_base64() {
    assert_eq!(
        basic_auth("neo4j", "sota-graph-local"),
        "Basic bmVvNGo6c290YS1ncmFwaC1sb2NhbA=="
    );
    assert_eq!(basic_auth("a", ""), "Basic YTo=");
}
