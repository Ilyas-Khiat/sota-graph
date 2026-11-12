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

/// One word of running text without the marks around it: quotes of any
/// script, brackets, emphasis, arrows, daggers, trailing punctuation. Only the
/// edges are touched: "“Filtered”," gives "Filtered", "(1-to-n)." gives
/// "1-to-n", while "head+tail" and "H@10" stay whole. A mark is any character
/// that is not a letter or a digit, in any script, so there is no list of
/// quote characters to keep up to date.
pub fn clean_word(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric())
}
