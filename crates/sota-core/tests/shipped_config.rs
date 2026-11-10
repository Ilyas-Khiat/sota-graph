//! Guards the config files the repo ships, and keeps a regression table of
//! every spelling met in the wild. Add a row whenever a new one turns up.

use sota_core::{Normalizer, Ontology, Registry};

const ONTOLOGY: &str = include_str!("../../../config/ontology.toml");
const NORMALIZATION: &str = include_str!("../../../config/normalization.toml");
const KGC: &str = include_str!("../../../config/registry/kgc.toml");

fn registry() -> Registry {
    let normalizer = Normalizer::from_toml_str("normalization.toml", NORMALIZATION).unwrap();
    Registry::build(normalizer, &[("kgc.toml", KGC)]).unwrap()
}

#[test]
fn shipped_ontology_is_valid() {
    let ontology = Ontology::from_toml_str("ontology.toml", ONTOLOGY).unwrap();
    for label in ["Paper", "Method", "Dataset", "Metric", "Result"] {
        assert!(ontology.node(label).is_some(), "missing node type {label}");
    }
}

#[test]
fn shipped_registry_is_valid() {
    registry();
}

#[test]
fn dataset_spellings() {
    let r = registry();
    let table: &[(&str, Option<&str>)] = &[
        ("FB15k-237", Some("fb15k-237")),
        ("FB15K-237", Some("fb15k-237")),
        ("FB15k\u{2013}237", Some("fb15k-237")), // en dash, common in PDFs
        ("FB15k\u{2212}237", Some("fb15k-237")), // minus sign
        ("FB 15k-237", Some("fb15k-237")),
        ("FB237", Some("fb15k-237")),
        ("FB15k", Some("fb15k")),
        ("FB15K", Some("fb15k")),
        ("WN18RR", Some("wn18rr")),
        ("WN18-RR", Some("wn18rr")),
        ("WN18", Some("wn18")),
        ("YAGO3-10", Some("yago3-10")),
        ("YAGO3\u{2011}10", Some("yago3-10")), // non-breaking hyphen
        ("FB15k-238", None),
        ("FB15k-237 (filtered)", None), // the setting must be split off first
        ("Freebase", None),
        ("WordNet", None),
    ];
    for &(mention, expected) in table {
        let got = r.resolve_dataset(mention).map(|d| d.id.as_str());
        assert_eq!(got, expected, "dataset mention {mention:?}");
    }
}

#[test]
fn metric_spellings() {
    let r = registry();
    let table: &[(&str, Option<&str>)] = &[
        ("MRR", Some("mrr")),
        ("Mean Reciprocal Rank", Some("mrr")),
        ("MR", Some("mr")),
        ("Mean Rank", Some("mr")),
        ("Hits@1", Some("hits@1")),
        ("H@1", Some("hits@1")),
        ("Hit@1", Some("hits@1")),
        ("HITS@1", Some("hits@1")),
        ("Hits@3", Some("hits@3")),
        ("H@3", Some("hits@3")),
        ("Hits@10", Some("hits@10")),
        ("H@10", Some("hits@10")),
        ("Hits @ 10", Some("hits@10")),
        ("Hits@100", None),
        ("Hits", None),
        ("MAP", None),
    ];
    for &(mention, expected) in table {
        let got = r.resolve_metric(mention).map(|m| m.id.as_str());
        assert_eq!(got, expected, "metric mention {mention:?}");
    }
}

/// Filtered and raw ranking numbers are not comparable, so no KG-completion
/// result may be stored without its protocol.
#[test]
fn every_kgc_metric_requires_the_protocol_setting() {
    let r = registry();
    let protocol = r.setting("protocol").expect("protocol setting");
    assert!(protocol.required);
    for m in r.metrics() {
        assert!(
            protocol.applies_to.contains(&m.id),
            "{} has no protocol",
            m.id
        );
    }
}
