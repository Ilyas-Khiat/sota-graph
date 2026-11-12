use std::collections::HashSet;

use serde::Deserialize;

use crate::{ConfigError, ident};

const SUPPORTED_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyType {
    String,
    Float,
    Integer,
    Boolean,
    Date,
    StringList,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Property {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: PropertyType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeType {
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// The property that identifies a node of this type (a uniqueness constraint).
    pub key: String,
    pub properties: Vec<Property>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipType {
    #[serde(rename = "type")]
    pub rel_type: String,
    #[serde(default)]
    pub description: String,
    /// Allowed source labels.
    pub from: Vec<String>,
    /// Allowed target labels.
    pub to: Vec<String>,
    #[serde(default)]
    pub properties: Vec<Property>,
}

/// The node and relationship types the graph may hold, loaded from config.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ontology {
    pub version: u32,
    #[serde(rename = "node")]
    pub nodes: Vec<NodeType>,
    #[serde(rename = "relationship", default)]
    pub relationships: Vec<RelationshipType>,
}

impl Ontology {
    /// Parses and validates. `file` only labels error messages.
    pub fn from_toml_str(file: &str, text: &str) -> Result<Self, ConfigError> {
        let ontology: Self = toml::from_str(text).map_err(|e| ConfigError::parse(file, e))?;
        ConfigError::check(ontology.problems())?;
        Ok(ontology)
    }

    pub fn node(&self, label: &str) -> Option<&NodeType> {
        self.nodes.iter().find(|n| n.label == label)
    }

    pub fn relationship(&self, rel_type: &str) -> Option<&RelationshipType> {
        self.relationships.iter().find(|r| r.rel_type == rel_type)
    }

    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.version != SUPPORTED_VERSION {
            problems.push(format!(
                "unsupported ontology version {} (expected {SUPPORTED_VERSION})",
                self.version
            ));
        }

        let mut labels = HashSet::new();
        for node in &self.nodes {
            if !ident::is_label(&node.label) {
                problems.push(format!(
                    "node {:?}: not a valid label (PascalCase ASCII, e.g. \"Paper\")",
                    node.label
                ));
            }
            if !labels.insert(node.label.as_str()) {
                problems.push(format!("duplicate node label {:?}", node.label));
            }
            let owner = format!("node {}", node.label);
            check_properties(&owner, &node.properties, &mut problems);
            match node.properties.iter().find(|p| p.name == node.key) {
                None => problems.push(format!(
                    "{owner}: key {:?} is not one of its properties",
                    node.key
                )),
                Some(p) if !p.required => problems.push(format!(
                    "{owner}: key {:?} must be a required property",
                    node.key
                )),
                Some(_) => {}
            }
        }

        let mut rel_types = HashSet::new();
        for rel in &self.relationships {
            let owner = format!("relationship {}", rel.rel_type);
            if !ident::is_relationship_type(&rel.rel_type) {
                problems.push(format!(
                    "relationship {:?}: not a valid relationship type (UPPER_SNAKE_CASE, e.g. \"BUILDS_ON\")",
                    rel.rel_type
                ));
            }
            if !rel_types.insert(rel.rel_type.as_str()) {
                problems.push(format!("duplicate relationship type {:?}", rel.rel_type));
            }
            for (side, ends) in [("from", &rel.from), ("to", &rel.to)] {
                if ends.is_empty() {
                    problems.push(format!("{owner}: `{side}` lists no label"));
                }
                for label in ends.iter().filter(|l| !labels.contains(l.as_str())) {
                    problems.push(format!("{owner}: `{side}` names unknown label {label:?}"));
                }
            }
            check_properties(&owner, &rel.properties, &mut problems);
        }
        problems
    }
}

fn check_properties(owner: &str, properties: &[Property], problems: &mut Vec<String>) {
    let mut names = HashSet::new();
    for p in properties {
        if !ident::is_property_name(&p.name) {
            problems.push(format!(
                "{owner}: {:?} is not a valid property name (lower_snake_case)",
                p.name
            ));
        }
        if !names.insert(p.name.as_str()) {
            problems.push(format!("{owner}: duplicate property {:?}", p.name));
        }
    }
}
