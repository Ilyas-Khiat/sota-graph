//! Comparisons, computed by code: never by the model.
//!
//! Two numbers are comparable only when dataset, split, metric and every
//! setting are identical; each such key is its own leaderboard. Within one,
//! a method's number is the one its own paper reports (self-report); other
//! papers' reports are kept beside it, and any two that disagree beyond
//! rounding are flagged as a conflict instead of being silently resolved.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Direction, GoldResult, MethodRegistry, Registry, Role, parse_number};

/// One number, as one paper reports it, with its resolved method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub paper: String,
    pub locator: String,
    pub method_id: String,
    pub method_name: String,
    /// False when the label matched no known method (identity unconfirmed).
    pub method_known: bool,
    pub role: Role,
    pub dataset: String,
    pub metric: String,
    pub split: String,
    pub settings: BTreeMap<String, String>,
    pub value: f64,
    pub value_text: String,
}

impl Report {
    pub fn from_result(r: &GoldResult, methods: &MethodRegistry) -> Self {
        let m = methods.resolve(&r.method);
        Self {
            paper: r.paper.clone(),
            locator: r.locator.clone(),
            method_id: m.id,
            method_name: m.name,
            method_known: m.known,
            role: r.role,
            dataset: r.dataset.clone(),
            metric: r.metric.clone(),
            split: r.split.clone(),
            settings: r.settings.clone(),
            value: r.value,
            value_text: r.value_text.clone(),
        }
    }

    fn self_reported(&self) -> bool {
        matches!(self.role, Role::Proposed | Role::Variant)
    }
}

/// What makes numbers comparable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Key {
    pub dataset: String,
    pub split: String,
    pub metric: String,
    pub settings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    /// Competition ranking: ties share a rank, the next rank skips.
    pub rank: usize,
    pub method_id: String,
    pub method_name: String,
    pub method_known: bool,
    pub value: f64,
    /// Where `value` comes from.
    pub source: Report,
    /// Every other report of this method under this key.
    pub others: Vec<Report>,
    /// Two reports of this method disagree beyond rounding.
    pub conflict: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Leaderboard {
    pub key: Key,
    pub direction: Direction,
    pub entries: Vec<Entry>,
}

/// Digits after the decimal point on the canonical scale: `47.6` printed in
/// percent is 0.476, three digits.
fn canonical_decimals(text: &str, value: f64) -> u32 {
    match parse_number(text) {
        Ok(p) => {
            let percent = (p.value - value).abs() > 1e-9 && (p.value / 100.0 - value).abs() < 1e-9;
            p.decimals + if percent { 2 } else { 0 }
        }
        Err(_) => 3,
    }
}

/// Whether two printed numbers can be the same value rounded differently:
/// `.43` covers 0.425 to 0.435, so it agrees with `0.432` but not with `.44`.
pub fn values_agree(a_text: &str, a: f64, b_text: &str, b: f64) -> bool {
    let half_unit = |text: &str, v: f64| 0.5 * 10f64.powi(-(canonical_decimals(text, v) as i32));
    (a - b).abs() <= half_unit(a_text, a).max(half_unit(b_text, b)) + 1e-12
}

fn agree(a: &Report, b: &Report) -> bool {
    values_agree(&a.value_text, a.value, &b.value_text, b.value)
}

/// One leaderboard per comparability key, for `dataset` and `metric` (ids),
/// keeping only reports whose settings match every `filters` entry.
pub fn leaderboards(
    reports: &[Report],
    registry: &Registry,
    dataset: &str,
    metric: &str,
    filters: &BTreeMap<String, String>,
) -> Vec<Leaderboard> {
    let Some(direction) = registry.metric(metric).map(|m| m.direction) else {
        return Vec::new();
    };
    let mut groups: BTreeMap<Key, BTreeMap<String, Vec<&Report>>> = BTreeMap::new();
    for r in reports.iter().filter(|r| {
        r.dataset == dataset
            && r.metric == metric
            && filters.iter().all(|(k, v)| r.settings.get(k) == Some(v))
    }) {
        let key = Key {
            dataset: r.dataset.clone(),
            split: r.split.clone(),
            metric: r.metric.clone(),
            settings: r.settings.clone(),
        };
        groups
            .entry(key)
            .or_default()
            .entry(r.method_id.clone())
            .or_default()
            .push(r);
    }

    groups
        .into_iter()
        .map(|(key, by_method)| {
            let mut entries: Vec<Entry> = by_method.into_values().map(entry).collect();
            let better = |a: f64, b: f64| match direction {
                Direction::HigherIsBetter => a > b,
                Direction::LowerIsBetter => a < b,
            };
            entries.sort_by(|a, b| {
                let ord = if better(a.value, b.value) {
                    std::cmp::Ordering::Less
                } else if better(b.value, a.value) {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                };
                ord.then_with(|| a.method_name.cmp(&b.method_name))
            });
            let values: Vec<f64> = entries.iter().map(|e| e.value).collect();
            for e in entries.iter_mut() {
                e.rank = 1 + values.iter().filter(|&&v| better(v, e.value)).count();
            }
            Leaderboard {
                key,
                direction,
                entries,
            }
        })
        .collect()
}

/// The number that speaks for a method: its own paper's report when there is
/// one, otherwise the value most other reports agree with (ties: earliest paper).
fn entry(mut reports: Vec<&Report>) -> Entry {
    reports.sort_by(|a, b| a.locator.cmp(&b.locator));
    let primary = match reports.iter().find(|r| r.self_reported()) {
        Some(r) => *r,
        None => *reports
            .iter()
            .max_by(|a, b| {
                let support = |x: &Report| reports.iter().filter(|o| agree(x, o)).count();
                support(a)
                    .cmp(&support(b))
                    .then_with(|| b.paper.cmp(&a.paper))
            })
            .expect("a method has at least one report"),
    };
    let conflict = reports
        .iter()
        .enumerate()
        .any(|(i, a)| reports[i + 1..].iter().any(|b| !agree(a, b)));
    let others = reports
        .iter()
        .filter(|r| r.locator != primary.locator)
        .map(|r| (*r).clone())
        .collect();
    Entry {
        rank: 0,
        method_id: primary.method_id.clone(),
        method_name: primary.method_name.clone(),
        method_known: primary.method_known,
        value: primary.value,
        source: primary.clone(),
        others,
        conflict,
    }
}
