use serde::Deserialize;

use crate::ConfigError;

/// Turns a mention or an alias into a matching key.
///
/// Matching is exact on the key, with no fuzzy step: these rules are the only
/// way two spellings meet, so they stay deliberately small and live in config.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Normalizer {
    #[serde(default)]
    pub lowercase: bool,
    /// Drops every Unicode whitespace character, no-break spaces included.
    #[serde(default)]
    pub remove_whitespace: bool,
    /// Characters dropped wherever they appear.
    #[serde(default)]
    pub remove_chars: String,
}

impl Normalizer {
    pub fn from_toml_str(file: &str, text: &str) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|e| ConfigError::parse(file, e))
    }

    pub fn key(&self, text: &str) -> String {
        let mut key = String::with_capacity(text.len());
        for c in text.trim().chars() {
            if (self.remove_whitespace && c.is_whitespace()) || self.remove_chars.contains(c) {
                continue;
            }
            if self.lowercase {
                key.extend(c.to_lowercase());
            } else {
                key.push(c);
            }
        }
        key
    }
}
