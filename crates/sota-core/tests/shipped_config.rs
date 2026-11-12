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
        ("Hits at 10", Some("hits@10")), // ComplEx, Table 2 (split over two header rows)
        ("Hits at 1", Some("hits@1")),
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

#[test]
fn setting_value_spellings() {
    let r = registry();
    let table: &[(&str, &str, Option<&str>)] = &[
        ("protocol", "Filter", Some("filtered")), // ComplEx, Table 2
        ("protocol", "Raw", Some("raw")),
        ("queries", "tail prediction", Some("tail")), // A*Net, Table 2
        ("queries", "head and tail", Some("both")),
        ("regime", "0-shot", Some("zero-shot")), // ULTRA, Table 10
        ("regime", "zero shot", Some("zero-shot")),
        ("regime", "fine-tuned", Some("fine-tuned")),
        ("regime", "finetuned", Some("fine-tuned")),
        ("regime", "pre-trained", None),
        ("queries", "Head", Some("head")), // RotatE, Table 9: "Prediction Head (Hits@10)"
        ("queries", "Tail", Some("tail")),
        ("queries", "head prediction", Some("head")),
        ("relation_category", "1-to-1", Some("1-to-1")), // RotatE, Table 9
        ("relation_category", "1-to-N", Some("1-to-n")),
        ("relation_category", "N-to-1", Some("n-to-1")),
        ("relation_category", "N-to-N", Some("n-to-n")),
        ("relation_category", "many-to-one", Some("n-to-1")),
        ("relation_category", "1-to-2", None),
    ];
    for &(setting, mention, expected) in table {
        let got = r
            .resolve_setting_value(setting, mention)
            .map(|v| v.id.as_str());
        assert_eq!(got, expected, "{setting} mention {mention:?}");
    }
}

/// Unstated query direction and regime mean the standard ones; the protocol
/// has no standard and must always be stated.
#[test]
fn comparability_settings_and_their_defaults() {
    let r = registry();
    assert_eq!(r.setting("protocol").unwrap().default, None);
    assert_eq!(
        r.setting("queries").unwrap().default.as_deref(),
        Some("both")
    );
    assert_eq!(
        r.setting("regime").unwrap().default.as_deref(),
        Some("supervised")
    );
    assert_eq!(
        r.setting("relation_category").unwrap().default.as_deref(),
        Some("all")
    );
}
