use sota_core::{ConfigError, Ontology};

const MINIMAL: &str = r#"
version = 1

[[node]]
label = "Paper"
key = "arxiv_id"
properties = [{ name = "arxiv_id", type = "string", required = true }]

[[node]]
label = "Method"
key = "id"
properties = [
  { name = "id", type = "string", required = true },
  { name = "name", type = "string" },
]

[[relationship]]
type = "PROPOSES"
from = ["Paper"]
to = ["Method"]
"#;

fn problems(text: &str) -> Vec<String> {
    match Ontology::from_toml_str("test.toml", text) {
        Err(ConfigError::Invalid { problems }) => problems,
        other => panic!("expected validation problems, got {other:?}"),
    }
}

fn has(problems: &[String], needles: &[&str]) -> bool {
    problems
        .iter()
        .any(|p| needles.iter().all(|needle| p.contains(needle)))
}

#[test]
fn minimal_ontology_loads() {
    let ontology = Ontology::from_toml_str("test.toml", MINIMAL).unwrap();
    assert_eq!(ontology.nodes.len(), 2);
    assert_eq!(ontology.node("Paper").unwrap().key, "arxiv_id");
    assert_eq!(ontology.relationship("PROPOSES").unwrap().to, ["Method"]);
    assert!(ontology.node("Author").is_none());
}

#[test]
fn relationship_to_unknown_label_is_rejected() {
    let p = problems(&MINIMAL.replace(r#"to = ["Method"]"#, r#"to = ["Author"]"#));
    assert!(has(&p, &["PROPOSES", "Author"]), "{p:?}");
}

#[test]
fn key_must_be_a_property() {
    let p = problems(&MINIMAL.replace(r#"key = "id""#, r#"key = "slug""#));
    assert!(has(&p, &["Method", "slug"]), "{p:?}");
}

#[test]
fn key_must_be_required() {
    let p = problems(&MINIMAL.replace(r#"key = "id""#, r#"key = "name""#));
    assert!(has(&p, &["Method", "name", "required"]), "{p:?}");
}

#[test]
fn duplicate_labels_are_rejected() {
    let text = format!(
        "{MINIMAL}\n[[node]]\nlabel = \"Paper\"\nkey = \"x\"\nproperties = [{{ name = \"x\", type = \"string\", required = true }}]\n"
    );
    let p = problems(&text);
    assert!(has(&p, &["duplicate", "Paper"]), "{p:?}");
}

#[test]
fn duplicate_property_names_are_rejected() {
    let text = MINIMAL.replace(
        r#"{ name = "name", type = "string" },"#,
        r#"{ name = "name", type = "string" }, { name = "name", type = "float" },"#,
    );
    let p = problems(&text);
    assert!(has(&p, &["duplicate", "name"]), "{p:?}");
}

/// Labels, relationship types and property names are pasted into Cypher, where
/// they cannot be parameters, so anything but a plain identifier is refused.
#[test]
fn identifiers_must_be_safe_for_cypher() {
    let text = MINIMAL
        .replace(
            r#"label = "Method""#,
            r#"label = "Method`) DETACH DELETE n //""#,
        )
        .replace(r#"type = "PROPOSES""#, r#"type = "proposes""#)
        .replace(r#"name = "name""#, r#"name = "Name""#);
    let p = problems(&text);
    assert!(has(&p, &["not a valid label"]), "{p:?}");
    assert!(has(&p, &["not a valid relationship type"]), "{p:?}");
    assert!(has(&p, &["not a valid property name"]), "{p:?}");
}

#[test]
fn every_problem_is_reported_at_once() {
    let text = MINIMAL
        .replace(r#"to = ["Method"]"#, r#"to = ["Author"]"#)
        .replace(r#"key = "id""#, r#"key = "slug""#);
    assert!(problems(&text).len() >= 2);
}

#[test]
fn unsupported_version_is_rejected() {
    let p = problems(&MINIMAL.replace("version = 1", "version = 2"));
    assert!(has(&p, &["version"]), "{p:?}");
}

#[test]
fn unknown_fields_are_a_parse_error_naming_the_file() {
    let text = MINIMAL.replace(r#"label = "Paper""#, r#"lable = "Paper""#);
    match Ontology::from_toml_str("ontology.toml", &text) {
        Err(ConfigError::Parse { file, .. }) => assert_eq!(file, "ontology.toml"),
        other => panic!("expected a parse error, got {other:?}"),
    }
}
