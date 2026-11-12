use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::{ConfigError, Normalizer, ident};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    HigherIsBetter,
    LowerIsBetter,
}

/// How a metric's numbers are written. Values are stored on the metric's
/// canonical scale; `Percent` converts only to and from `Fraction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    Fraction,
    Percent,
    Raw,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dataset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub splits: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub direction: Direction,
    pub canonical_scale: Scale,
    pub accepted_scales: Vec<Scale>,
    /// Bounds on the canonical scale, used to reject impossible values.
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub notes: String,
}

/// A condition two results must share to be comparable, e.g. the evaluation protocol.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setting {
    pub id: String,
    pub name: String,
    /// Metric ids this setting qualifies.
    pub applies_to: Vec<String>,
    /// A result on one of `applies_to` without this setting goes to review.
    #[serde(default)]
    pub required: bool,
    /// The value a result has when the paper does not state one (the field's
    /// standard practice). Never set together with `required`.
    #[serde(default)]
    pub default: Option<String>,
    pub values: Vec<SettingValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingValue {
    pub id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    topic: String,
    #[serde(default, rename = "dataset")]
    datasets: Vec<Dataset>,
    #[serde(default, rename = "metric")]
    metrics: Vec<Metric>,
    #[serde(default, rename = "setting")]
    settings: Vec<Setting>,
}

/// Normalized spelling -> (entry position, the spelling that claimed it).
type SpellingIndex = HashMap<String, (usize, String)>;

/// Every dataset, metric and setting value a result may refer to, merged
/// from one file per topic. Lookups are exact on the normalized key; a miss
/// means "unknown", never a guess.
#[derive(Debug)]
pub struct Registry {
    normalizer: Normalizer,
    topics: Vec<String>,
    datasets: Vec<Dataset>,
    metrics: Vec<Metric>,
    settings: Vec<Setting>,
    dataset_index: SpellingIndex,
    metric_index: SpellingIndex,
    /// Parallel to `settings`.
    setting_value_index: Vec<SpellingIndex>,
}

impl Registry {
    /// Parses every `(file name, text)` source, merges them, and validates the
    /// whole: collisions between files are caught like collisions within one.
    pub fn build(normalizer: Normalizer, sources: &[(&str, &str)]) -> Result<Self, ConfigError> {
        let mut topics = Vec::new();
        let (mut datasets, mut metrics, mut settings) = (Vec::new(), Vec::new(), Vec::new());
        for &(file, text) in sources {
            let parsed: RegistryFile =
                toml::from_str(text).map_err(|e| ConfigError::parse(file, e))?;
            topics.push(parsed.topic);
            datasets.extend(parsed.datasets);
            metrics.extend(parsed.metrics);
            settings.extend(parsed.settings);
        }

        let mut problems = Vec::new();
        if topics.iter().any(|t| t.trim().is_empty()) {
            problems.push("a registry file has an empty `topic`".to_owned());
        }
        check_ids(
            "dataset",
            datasets.iter().map(|d| d.id.as_str()),
            &mut problems,
        );
        check_ids(
            "metric",
            metrics.iter().map(|m| m.id.as_str()),
            &mut problems,
        );
        check_ids(
            "setting",
            settings.iter().map(|s| s.id.as_str()),
            &mut problems,
        );
        for m in &metrics {
            check_metric(m, &mut problems);
        }
        let metric_ids: HashSet<&str> = metrics.iter().map(|m| m.id.as_str()).collect();
        for s in &settings {
            check_setting(s, &metric_ids, &mut problems);
        }

        let dataset_index = index_spellings(
            &normalizer,
            "dataset",
            datasets
                .iter()
                .map(|d| (d.id.as_str(), spellings(&d.id, &d.name, &d.aliases))),
            &mut problems,
        );
        let metric_index = index_spellings(
            &normalizer,
            "metric",
            metrics
                .iter()
                .map(|m| (m.id.as_str(), spellings(&m.id, &m.name, &m.aliases))),
            &mut problems,
        );
        let setting_value_index = settings
            .iter()
            .map(|s| {
                let kind = format!("setting {} value", s.id);
                index_spellings(
                    &normalizer,
                    &kind,
                    s.values
                        .iter()
                        .map(|v| (v.id.as_str(), spellings(&v.id, &v.id, &v.aliases))),
                    &mut problems,
                )
            })
            .collect();

        ConfigError::check(problems)?;
        Ok(Self {
            normalizer,
            topics,
            datasets,
            metrics,
            settings,
            dataset_index,
            metric_index,
            setting_value_index,
        })
    }

    pub fn normalizer(&self) -> &Normalizer {
        &self.normalizer
    }

    pub fn topics(&self) -> &[String] {
        &self.topics
    }

    pub fn datasets(&self) -> &[Dataset] {
        &self.datasets
    }

    pub fn metrics(&self) -> &[Metric] {
        &self.metrics
    }

    pub fn settings(&self) -> &[Setting] {
        &self.settings
    }

    /// By exact id (not by spelling).
    pub fn dataset(&self, id: &str) -> Option<&Dataset> {
        self.datasets.iter().find(|d| d.id == id)
    }

    /// By exact id (not by spelling).
    pub fn metric(&self, id: &str) -> Option<&Metric> {
        self.metrics.iter().find(|m| m.id == id)
    }

    pub fn setting(&self, id: &str) -> Option<&Setting> {
        self.settings.iter().find(|s| s.id == id)
    }

    pub fn resolve_dataset(&self, mention: &str) -> Option<&Dataset> {
        self.lookup(&self.dataset_index, mention)
            .map(|i| &self.datasets[i])
    }

    pub fn resolve_metric(&self, mention: &str) -> Option<&Metric> {
        self.lookup(&self.metric_index, mention)
            .map(|i| &self.metrics[i])
    }

    pub fn resolve_setting_value(&self, setting_id: &str, mention: &str) -> Option<&SettingValue> {
        let pos = self.settings.iter().position(|s| s.id == setting_id)?;
        self.lookup(&self.setting_value_index[pos], mention)
            .map(|i| &self.settings[pos].values[i])
    }

    fn lookup(&self, index: &SpellingIndex, mention: &str) -> Option<usize> {
        index.get(&self.normalizer.key(mention)).map(|&(i, _)| i)
    }
}

fn spellings<'a>(id: &'a str, name: &'a str, aliases: &'a [String]) -> Vec<&'a str> {
    [id, name]
        .into_iter()
        .chain(aliases.iter().map(String::as_str))
        .collect()
}

fn check_ids<'a>(kind: &str, ids: impl Iterator<Item = &'a str>, problems: &mut Vec<String>) {
    let mut seen = HashSet::new();
    for id in ids {
        if !ident::is_registry_id(id) {
            problems.push(format!(
                "{kind} {id:?}: not a valid id (lowercase letters, digits and @._-, e.g. \"fb15k-237\")"
            ));
        }
        if !seen.insert(id) {
            problems.push(format!("duplicate {kind} id {id:?}"));
        }
    }
}

fn check_metric(m: &Metric, problems: &mut Vec<String>) {
    let id = &m.id;
    if !m.accepted_scales.contains(&m.canonical_scale) {
        problems.push(format!(
            "metric {id}: canonical scale {:?} is not in accepted_scales",
            m.canonical_scale
        ));
    }
    if m.accepted_scales.contains(&Scale::Percent)
        && !matches!(m.canonical_scale, Scale::Fraction | Scale::Percent)
    {
        problems.push(format!(
            "metric {id}: accepts percent, but its canonical scale {:?} cannot be converted from percent",
            m.canonical_scale
        ));
    }
    for (bound, value) in [("min", m.min), ("max", m.max)] {
        if value.is_some_and(|v| !v.is_finite()) {
            problems.push(format!("metric {id}: {bound} must be a finite number"));
        }
    }
    if let (Some(min), Some(max)) = (m.min, m.max)
        && min >= max
    {
        problems.push(format!(
            "metric {id}: min ({min}) must be below max ({max})"
        ));
    }
}

fn check_setting(s: &Setting, metric_ids: &HashSet<&str>, problems: &mut Vec<String>) {
    let id = &s.id;
    if s.applies_to.is_empty() {
        problems.push(format!("setting {id}: applies_to lists no metric"));
    }
    for metric in s
        .applies_to
        .iter()
        .filter(|m| !metric_ids.contains(m.as_str()))
    {
        problems.push(format!(
            "setting {id}: applies_to names unknown metric {metric:?}"
        ));
    }
    if s.values.is_empty() {
        problems.push(format!("setting {id}: lists no values"));
    }
    if let Some(default) = &s.default {
        if s.required {
            problems.push(format!(
                "setting {id}: is required, so it cannot have a default"
            ));
        }
        if !s.values.iter().any(|v| &v.id == default) {
            problems.push(format!(
                "setting {id}: default {default:?} is not one of its values"
            ));
        }
    }
    check_ids(
        &format!("setting {id} value"),
        s.values.iter().map(|v| v.id.as_str()),
        problems,
    );
}

/// Maps every spelling of every entry to that entry. Reports spellings that
/// normalize to nothing, and spellings two entries would share. Duplicate ids
/// are left to `check_ids`, which words them better.
fn index_spellings<'a>(
    normalizer: &Normalizer,
    kind: &str,
    entries: impl Iterator<Item = (&'a str, Vec<&'a str>)>,
    problems: &mut Vec<String>,
) -> SpellingIndex {
    let mut index = SpellingIndex::new();
    let mut owners: Vec<&str> = Vec::new();
    for (pos, (id, spellings)) in entries.enumerate() {
        owners.push(id);
        for spelling in spellings {
            let key = normalizer.key(spelling);
            if key.is_empty() {
                problems.push(format!(
                    "{kind} {id}: spelling {spelling:?} normalizes to an empty key"
                ));
                continue;
            }
            match index.get(&key) {
                None => {
                    index.insert(key, (pos, spelling.to_owned()));
                }
                Some((other, _)) if *other == pos || owners[*other] == id => {}
                Some((other, first)) => problems.push(format!(
                    "{kind} spelling {spelling:?} of {id:?} and {first:?} of {:?} both normalize to {key:?}",
                    owners[*other]
                )),
            }
        }
    }
    index
}
