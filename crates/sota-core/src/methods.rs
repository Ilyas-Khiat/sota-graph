//! Method identity, decided by config and code, never by the model.
//!
//! A label as printed ("TransE [\varheart]", "TuckER (ours)", "ConvE*") is
//! cleaned of citation brackets and suffix markers, then matched exactly,
//! after the shared normalization, against each method's id, name and
//! aliases. No fuzzy step: a label that matches nothing is a new method, and
//! is reported as unconfirmed.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::{ConfigError, Normalizer, ident, method_slug};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Known caveats, shown wherever the method is ranked.
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MethodsFile {
    topic: String,
    #[serde(default)]
    suffix_markers: Vec<String>,
    #[serde(default, rename = "method")]
    methods: Vec<MethodEntry>,
}

/// What a label resolves to. `known` is false for a label no entry matches:
/// its id is made from the label and needs a human to confirm it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodRef {
    pub id: String,
    pub name: String,
    pub known: bool,
}

#[derive(Debug)]
pub struct MethodRegistry {
    normalizer: Normalizer,
    markers: Vec<String>,
    methods: Vec<MethodEntry>,
    index: HashMap<String, usize>,
}

impl MethodRegistry {
    pub fn build(normalizer: Normalizer, sources: &[(&str, &str)]) -> Result<Self, ConfigError> {
        let mut markers = Vec::new();
        let mut methods = Vec::new();
        let mut problems = Vec::new();
        for &(file, text) in sources {
            let parsed: MethodsFile =
                toml::from_str(text).map_err(|e| ConfigError::parse(file, e))?;
            if parsed.topic.trim().is_empty() {
                problems.push(format!("{file}: empty topic"));
            }
            for m in parsed.suffix_markers {
                if !markers.contains(&m) {
                    markers.push(m);
                }
            }
            methods.extend(parsed.methods);
        }
        let mut registry = Self {
            normalizer,
            markers,
            methods,
            index: HashMap::new(),
        };

        let mut ids = HashSet::new();
        let mut first_spelling: HashMap<String, (usize, String)> = HashMap::new();
        for (pos, m) in registry.methods.iter().enumerate() {
            if !ident::is_registry_id(&m.id) {
                problems.push(format!(
                    "method {:?}: not a valid id (lowercase slug)",
                    m.id
                ));
            }
            if !ids.insert(m.id.clone()) {
                problems.push(format!("duplicate method id {:?}", m.id));
                continue;
            }
            let spellings = [m.id.as_str(), m.name.as_str()]
                .into_iter()
                .chain(m.aliases.iter().map(String::as_str));
            for spelling in spellings {
                let key = registry.key(spelling);
                if key.is_empty() {
                    problems.push(format!(
                        "method {}: spelling {spelling:?} is empty once cleaned",
                        m.id
                    ));
                    continue;
                }
                match first_spelling.get(&key) {
                    None => {
                        first_spelling.insert(key.clone(), (pos, spelling.to_owned()));
                        registry.index.insert(key, pos);
                    }
                    Some((other, _)) if *other == pos => {}
                    Some((other, first)) => problems.push(format!(
                        "method spelling {spelling:?} of {:?} and {first:?} of {:?} both normalize to {key:?}",
                        m.id, registry.methods[*other].id
                    )),
                }
            }
        }
        ConfigError::check(problems)?;
        Ok(registry)
    }

    pub fn methods(&self) -> &[MethodEntry] {
        &self.methods
    }

    pub fn method(&self, id: &str) -> Option<&MethodEntry> {
        self.methods.iter().find(|m| m.id == id)
    }

    /// The label without citation brackets and suffix markers, whitespace collapsed.
    pub fn clean_label(&self, label: &str) -> String {
        let mut text = String::new();
        let mut depth = 0usize;
        for c in label.chars() {
            match c {
                '[' => depth += 1,
                ']' if depth > 0 => depth -= 1,
                _ if depth == 0 => text.push(c),
                _ => {}
            }
        }
        let mut text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        loop {
            let lower = text.to_lowercase();
            let Some(marker) = self
                .markers
                .iter()
                .find(|m| !m.is_empty() && lower.ends_with(&m.to_lowercase()))
            else {
                break;
            };
            let cut = text.len() - marker.len();
            if !text.is_char_boundary(cut) {
                break;
            }
            text = text[..cut].trim_end().to_owned();
        }
        text
    }

    fn key(&self, label: &str) -> String {
        self.normalizer.key(&self.clean_label(label))
    }

    pub fn resolve(&self, label: &str) -> MethodRef {
        match self.index.get(&self.key(label)) {
            Some(&pos) => {
                let m = &self.methods[pos];
                MethodRef {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    known: true,
                }
            }
            None => {
                let name = self.clean_label(label);
                MethodRef {
                    id: method_slug(&name),
                    name,
                    known: false,
                }
            }
        }
    }
}
