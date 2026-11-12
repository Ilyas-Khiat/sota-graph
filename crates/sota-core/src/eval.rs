//! Scoring an extraction against the gold set, cell by cell.
//!
//! A result is a true positive only when the same cell carries the same
//! meaning: dataset, metric, every setting, role, method label and value.
//! The method *id* is not scored: identities across papers are resolved later.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::{GoldFile, GoldResult, ReviewItem};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Score {
    pub tp: usize,
    pub fp: usize,
    pub fn_: usize,
    /// Gold cells held back for review (typos, non-comparable rows).
    pub held_back: usize,
    /// ... that the extraction sent to review.
    pub held_back_caught: usize,
    /// ... that the extraction turned into results anyway.
    pub held_back_leaked: usize,
    /// Gold results the extraction sent to review instead.
    pub wrongly_reviewed: usize,
    /// Results extracted from tables the gold lists as holding none.
    pub negative_table_results: usize,
    /// Results from tables the gold does not cover at all.
    pub unscored: usize,
    /// One line per wrong field, for reading.
    pub mismatches: Vec<String>,
}

impl Score {
    pub fn precision(&self) -> f64 {
        ratio(self.tp, self.tp + self.fp)
    }

    pub fn recall(&self) -> f64 {
        ratio(self.tp, self.tp + self.fn_)
    }

    pub fn add(&mut self, other: &Score) {
        self.tp += other.tp;
        self.fp += other.fp;
        self.fn_ += other.fn_;
        self.held_back += other.held_back;
        self.held_back_caught += other.held_back_caught;
        self.held_back_leaked += other.held_back_leaked;
        self.wrongly_reviewed += other.wrongly_reviewed;
        self.negative_table_results += other.negative_table_results;
        self.unscored += other.unscored;
        self.mismatches.extend(other.mismatches.iter().cloned());
    }
}

fn ratio(a: usize, b: usize) -> f64 {
    if b == 0 { 1.0 } else { a as f64 / b as f64 }
}

type Key = (String, usize, usize);

fn key(r: &GoldResult) -> Key {
    (r.table.clone(), r.row, r.col)
}

/// Fields that differ between a gold result and an extracted one.
fn differences(g: &GoldResult, e: &GoldResult) -> Vec<String> {
    let mut d = Vec::new();
    let mut check = |field: &str, same: bool, gold: String, got: String| {
        if !same {
            d.push(format!("{field}: gold {gold}, extracted {got}"));
        }
    };
    check(
        "method",
        g.method == e.method,
        format!("{:?}", g.method),
        format!("{:?}", e.method),
    );
    check(
        "role",
        g.role == e.role,
        format!("{:?}", g.role),
        format!("{:?}", e.role),
    );
    check(
        "dataset",
        g.dataset == e.dataset,
        g.dataset.clone(),
        e.dataset.clone(),
    );
    check(
        "metric",
        g.metric == e.metric,
        g.metric.clone(),
        e.metric.clone(),
    );
    check(
        "settings",
        g.settings == e.settings,
        format!("{:?}", g.settings),
        format!("{:?}", e.settings),
    );
    check(
        "value",
        g.value == e.value,
        g.value.to_string(),
        e.value.to_string(),
    );
    d
}

pub fn score_extraction(
    spec: &GoldFile,
    gold: &[GoldResult],
    extracted: &[GoldResult],
    review: &[ReviewItem],
) -> Score {
    let mut s = Score::default();
    let gold_tables: HashSet<&str> = spec.tables.iter().map(|t| t.id.as_str()).collect();
    let negative: HashSet<&str> = spec.negative_tables.iter().map(String::as_str).collect();

    // Held-back cells, expanded over the table's mapped columns.
    let mut held: HashSet<Key> = HashSet::new();
    for t in &spec.tables {
        for r in &t.review_cells {
            let cols: Vec<usize> = match r.col {
                Some(c) => vec![c],
                None => t.columns.iter().map(|c| c.col).collect(),
            };
            for c in cols {
                held.insert((t.id.clone(), r.row, c));
            }
        }
    }
    let gold_by_key: BTreeMap<Key, &GoldResult> = gold.iter().map(|r| (key(r), r)).collect();
    let extracted_keys: HashSet<Key> = extracted.iter().map(key).collect();
    let covered = |k: &Key| review.iter().any(|i| i.table == k.0 && i.covers(k.1, k.2));

    for k in &held {
        s.held_back += 1;
        if extracted_keys.contains(k) {
            s.held_back_leaked += 1;
        } else if covered(k) {
            s.held_back_caught += 1;
        }
    }

    let mut matched = HashSet::new();
    for e in extracted {
        let k = key(e);
        if negative.contains(e.table.as_str()) {
            s.fp += 1;
            s.negative_table_results += 1;
            continue;
        }
        if !gold_tables.contains(e.table.as_str()) {
            s.unscored += 1;
            continue;
        }
        if held.contains(&k) {
            continue; // counted as leaked above
        }
        match gold_by_key.get(&k) {
            Some(g) => {
                let d = differences(g, e);
                if d.is_empty() {
                    s.tp += 1;
                    matched.insert(k);
                } else {
                    s.fp += 1;
                    for field in d {
                        s.mismatches.push(format!("{}: {field}", e.locator));
                    }
                }
            }
            None => {
                s.fp += 1;
                s.mismatches
                    .push(format!("{}: not in the gold set", e.locator));
            }
        }
    }
    for (k, g) in &gold_by_key {
        if !matched.contains(k) {
            s.fn_ += 1;
            if !extracted_keys.contains(k) && covered(k) {
                s.wrongly_reviewed += 1;
            } else if !extracted_keys.contains(k) {
                s.mismatches.push(format!("{}: missed", g.locator));
            }
        }
    }
    s.mismatches.sort();
    s
}
