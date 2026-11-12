use std::fs;

use sota_core::{ConfigError, MethodRegistry, Normalizer};

const NORMALIZATION: &str = include_str!("../../../config/normalization.toml");
const KGC_METHODS: &str = include_str!("../../../config/methods/kgc.toml");

fn normalizer() -> Normalizer {
    Normalizer::from_toml_str("normalization.toml", NORMALIZATION).unwrap()
}

fn shipped() -> MethodRegistry {
    MethodRegistry::build(normalizer(), &[("kgc.toml", KGC_METHODS)]).unwrap()
}

const SMALL: &str = r#"
topic = "t"
suffix_markers = ["*", "(ours)", "(Proposed Method)"]

[[method]]
id = "r-gcn"
name = "R-GCN"

[[method]]
id = "transh"
name = "TransH"
aliases = ["TransH (bern)"]

[[method]]
id = "astarnet"
name = "A*Net"
"#;

fn small() -> MethodRegistry {
    MethodRegistry::build(normalizer(), &[("m.toml", SMALL)]).unwrap()
}

#[test]
fn labels_lose_markers_and_citations_but_keep_their_name() {
    let m = small();
    assert_eq!(m.clean_label("TransE [\\varheart]"), "TransE");
    assert_eq!(m.clean_label("TuckER (ours)"), "TuckER");
    assert_eq!(m.clean_label("CompGCN (Proposed Method)"), "CompGCN");
    assert_eq!(m.clean_label("ConvE*"), "ConvE");
    assert_eq!(m.clean_label("A*Net"), "A*Net"); // a star inside a name stays
    assert_eq!(m.clean_label("TransH (bern)"), "TransH (bern)"); // not a listed marker
    assert_eq!(m.clean_label("  HolE*  [12] "), "HolE");
}

#[test]
fn spellings_resolve_to_one_method() {
    let m = small();
    for label in ["R-GCN", "RGCN", "R-GCN [36]", "rgcn*"] {
        let r = m.resolve(label);
        assert_eq!((r.id.as_str(), r.known), ("r-gcn", true), "{label}");
    }
    assert_eq!(m.resolve("TransH (bern)").id, "transh");
    assert_eq!(m.resolve("A*Net").id, "astarnet");
}

#[test]
fn an_unknown_label_becomes_a_new_unconfirmed_method() {
    let r = small().resolve("FooNet (ours)");
    assert_eq!(
        (r.id.as_str(), r.name.as_str(), r.known),
        ("foonet", "FooNet", false)
    );
}

#[test]
fn the_registry_is_validated() {
    let bad = SMALL.replace(
        r#"aliases = ["TransH (bern)"]"#,
        r#"aliases = ["TransH (bern)", "RGCN"]"#,
    );
    match MethodRegistry::build(normalizer(), &[("m.toml", &bad)]) {
        Err(ConfigError::Invalid { problems }) => {
            assert!(
                problems
                    .iter()
                    .any(|p| p.contains("RGCN") && p.contains("r-gcn")),
                "{problems:?}"
            )
        }
        other => panic!("expected a collision, got {other:?}"),
    }
    let bad = SMALL.replace(r#"id = "transh""#, r#"id = "TransH""#);
    assert!(MethodRegistry::build(normalizer(), &[("m.toml", &bad)]).is_err());
}

/// Every method id the gold set uses must be what the shipped registry
/// resolves its labels to: identity is decided by config, and the gold set
/// is the check.
#[test]
fn the_shipped_registry_resolves_every_gold_label_to_its_gold_id() {
    let m = shipped();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../gold/kgc");
    let mut checked = 0;
    let mut wrong = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if !path.to_string_lossy().ends_with(".results.jsonl") {
            continue;
        }
        for line in fs::read_to_string(&path)
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
        {
            let r: serde_json::Value = serde_json::from_str(line).unwrap();
            let (label, gold) = (
                r["method"].as_str().unwrap(),
                r["method_id"].as_str().unwrap(),
            );
            let got = m.resolve(label);
            checked += 1;
            if got.id != gold || !got.known {
                wrong.push(format!(
                    "{label:?}: gold {gold}, resolved {} (known {})",
                    got.id, got.known
                ));
            }
        }
    }
    wrong.sort();
    wrong.dedup();
    assert!(checked > 900, "{checked}");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
