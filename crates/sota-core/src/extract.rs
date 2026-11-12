//! Verifying what a model says a table means.
//!
//! The model never returns numbers. It returns a `TableMapping`: which row is
//! which method, which column is which dataset and metric, which settings the
//! paper states (each with the paragraph or caption that states it), and which
//! cells it does not trust. `verify_mapping` then reads every number out of
//! the table itself, through the same checks the gold set uses, and splits the
//! table into verified results and a review queue. A part that fails a check
//! goes to review on its own; the rest of the table still counts.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::gold::DatasetIn;
use crate::{
    Document, Expansion, GoldColumn, GoldResult, GoldRow, GoldTable, Orientation, Registry,
    ReviewCell, Role, Scale, Scope, Table,
};

/// What the model returns for one table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableMapping {
    /// False for statistics, hyperparameters, examples: anything without results.
    pub is_results_table: bool,
    pub orientation: Orientation,
    pub header_rows: usize,
    pub label_col: usize,
    /// Where each column's dataset is named: its header (usual) or the caption
    /// (a table on a single dataset, e.g. an ablation "on FB15k-237").
    #[serde(default)]
    pub dataset_in: DatasetIn,
    /// Settings that hold for the whole table.
    pub settings: Vec<SettingClaim>,
    pub columns: Vec<ColumnClaim>,
    pub rows: Vec<RowClaim>,
    /// Cells (or whole rows, `col: null`) the model does not trust, with why.
    pub review: Vec<ReviewClaim>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingClaim {
    pub setting: String,
    pub value: String,
    /// A paragraph id, "caption", or (for a column) "header": where the paper says so.
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnClaim {
    pub col: usize,
    /// A registry metric id, or "unknown".
    pub metric: String,
    /// Methods in rows: a registry dataset id, or "unknown".
    pub dataset: Option<String>,
    /// Datasets in rows: the method as printed in the header.
    pub method: Option<String>,
    pub role: Option<Role>,
    pub scale: Scale,
    pub settings: Vec<SettingClaim>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowClaim {
    pub row: usize,
    /// Methods in rows: the label exactly as printed.
    pub method: Option<String>,
    pub role: Option<Role>,
    /// Datasets in rows: a registry dataset id, or "unknown".
    pub dataset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewClaim {
    pub row: usize,
    pub col: Option<usize>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewBy {
    /// The model flagged it.
    Model,
    /// A code check rejected it.
    Check,
}

/// Something set aside for a human. `row`/`col` both `None`: the whole table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewItem {
    pub paper: String,
    pub table: String,
    pub row: Option<usize>,
    pub col: Option<usize>,
    pub reason: String,
    pub by: ReviewBy,
}

impl ReviewItem {
    /// Whether this item sets aside the cell at (`row`, `col`).
    pub fn covers(&self, row: usize, col: usize) -> bool {
        self.row.is_none_or(|r| r == row) && self.col.is_none_or(|c| c == col)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableOutcome {
    pub results: Vec<GoldResult>,
    pub review: Vec<ReviewItem>,
    /// Columns or rows on datasets or metrics the registry does not track.
    pub out_of_registry: Vec<String>,
    /// Mapped cells that hold no result ("-").
    pub missing: usize,
}

/// A placeholder identity from the label as printed ("Ours (large)" ->
/// "ours-large"). Merging identities across papers is not this step's job.
pub fn method_slug(label: &str) -> String {
    let mut slug = String::new();
    for c in label.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "method".to_owned()
    } else {
        slug
    }
}

pub fn verify_mapping(
    m: &TableMapping,
    doc: &Document,
    table: &Table,
    registry: &Registry,
) -> TableOutcome {
    let mut out = TableOutcome::default();
    if !m.is_results_table {
        return out;
    }
    let paper = doc.versioned_id();
    let item = |row: Option<usize>, col: Option<usize>, reason: String, by: ReviewBy| ReviewItem {
        paper: paper.clone(),
        table: table.id.clone(),
        row,
        col,
        reason,
        by,
    };
    for r in &m.review {
        out.review.push(item(
            Some(r.row),
            r.col,
            format!("model: {}", r.reason),
            ReviewBy::Model,
        ));
    }
    let header_rows = m.header_rows.max(1);
    let methods_in_rows = m.orientation == Orientation::MethodsInRows;

    // Settings for the whole table. A claim the evidence does not support is
    // harmless when it states the default, and disqualifying otherwise.
    let mut table_settings = BTreeMap::new();
    let mut table_bad = Vec::new();
    for claim in &m.settings {
        match check_claim(claim, None, doc, table, header_rows, registry) {
            Ok(()) => {
                table_settings.insert(claim.setting.clone(), claim.value.clone());
            }
            Err(reason) if is_default(claim, registry) => drop(reason),
            Err(reason) => table_bad.push(reason),
        }
    }

    let mut columns = Vec::new();
    let mut seen = HashSet::new();
    for c in &m.columns {
        if !seen.insert(c.col) || c.col == m.label_col {
            continue;
        }
        let Some(metric) = registry.metric(&c.metric) else {
            out.out_of_registry.push(format!(
                "col {}: metric {:?} is not in the registry",
                c.col, c.metric
            ));
            continue;
        };
        let dataset = c.dataset.clone().unwrap_or_default();
        let method = c.method.clone().unwrap_or_default();
        if methods_in_rows && registry.dataset(&dataset).is_none() {
            out.out_of_registry.push(format!(
                "col {}: dataset {dataset:?} is not in the registry",
                c.col
            ));
            continue;
        }
        let mut bad = table_bad.clone();
        if !methods_in_rows && (method.trim().is_empty() || c.role.is_none()) {
            bad.push("the model gave no method or role for this column".to_owned());
        }
        let mut col_settings = BTreeMap::new();
        for claim in &c.settings {
            match check_claim(claim, Some(c.col), doc, table, header_rows, registry) {
                Ok(()) => {
                    col_settings.insert(claim.setting.clone(), claim.value.clone());
                }
                Err(reason) if is_default(claim, registry) => drop(reason),
                Err(reason) => bad.push(reason),
            }
        }
        let mut stated = table_settings.clone();
        stated.extend(col_settings.clone());
        for s in registry
            .settings()
            .iter()
            .filter(|s| s.required && s.applies_to.contains(&c.metric))
        {
            if !stated.contains_key(&s.id) {
                bad.push(format!(
                    "{} is required but no evidence in the paper states it",
                    s.id
                ));
            }
        }
        if !metric.accepted_scales.contains(&c.scale) {
            bad.push(format!(
                "{} is never printed on the {:?} scale",
                metric.id, c.scale
            ));
        }
        if !bad.is_empty() {
            out.review
                .push(item(None, Some(c.col), bad.join("; "), ReviewBy::Check));
            continue;
        }
        columns.push(GoldColumn {
            col: c.col,
            metric: c.metric.clone(),
            dataset: if methods_in_rows {
                dataset
            } else {
                String::new()
            },
            method_id: if methods_in_rows {
                String::new()
            } else {
                method_slug(&method)
            },
            method: if methods_in_rows {
                String::new()
            } else {
                method
            },
            role: if methods_in_rows { None } else { c.role },
            scale: Some(c.scale),
            settings: col_settings,
        });
    }

    // Where each label sits in the label column, below the header.
    let grid = table.grid();
    let mut label_rows: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (r, cells) in grid.iter().enumerate().skip(header_rows) {
        if let Some(cell) = cells.get(m.label_col).copied().flatten()
            && cell.row == r
        {
            label_rows.entry(cell.text.as_str()).or_default().push(r);
        }
    }
    let label_at = |r: usize| {
        grid.get(r)
            .and_then(|cells| cells.get(m.label_col).copied().flatten())
            .map(|c| c.text.as_str())
    };
    // Rows already held by a claim whose index matches its label.
    let claimed: HashSet<usize> = m
        .rows
        .iter()
        .filter(|r| r.method.is_some() && label_at(r.row) == r.method.as_deref())
        .map(|r| r.row)
        .collect();

    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for r in &m.rows {
        // A label copied exactly but given at the wrong index (row numbers
        // shifted after a row-spanning group label) is found by its text,
        // but only when the label is unique and its row is not claimed.
        let mut row = r.row;
        if methods_in_rows
            && let Some(method) = &r.method
            && label_at(row) != Some(method.as_str())
            && let Some([only]) = label_rows.get(method.as_str()).map(Vec::as_slice)
            && !claimed.contains(only)
        {
            row = *only;
        }
        if row < header_rows {
            continue;
        }
        if !seen.insert(row) {
            // Never drop a claim silently: two claims on one row means the
            // mapping is confused there (a label printed more than once).
            out.review.push(item(
                Some(row),
                None,
                format!(
                    "two rows of the mapping claim row {row}; {:?} could not be placed",
                    r.method.clone().or(r.dataset.clone()).unwrap_or_default()
                ),
                ReviewBy::Check,
            ));
            continue;
        }
        if methods_in_rows {
            let method = r.method.clone().unwrap_or_default();
            if method.trim().is_empty() || r.role.is_none() {
                out.review.push(item(
                    Some(row),
                    None,
                    "the model gave no method or role for this row".to_owned(),
                    ReviewBy::Check,
                ));
                continue;
            }
            rows.push(GoldRow {
                row,
                method_id: method_slug(&method),
                method,
                role: r.role,
                dataset: String::new(),
            });
        } else {
            let dataset = r.dataset.clone().unwrap_or_default();
            if registry.dataset(&dataset).is_none() {
                out.out_of_registry.push(format!(
                    "row {}: dataset {dataset:?} is not in the registry",
                    r.row
                ));
                continue;
            }
            rows.push(GoldRow {
                row: r.row,
                method: String::new(),
                method_id: String::new(),
                role: None,
                dataset,
            });
        }
    }
    if rows.is_empty() || columns.is_empty() {
        return out;
    }

    let mut spec = GoldTable {
        id: table.id.clone(),
        split: "test".to_owned(),
        header_rows,
        label_col: m.label_col,
        orientation: m.orientation,
        dataset_in: m.dataset_in,
        settings: table_settings,
        evidence: String::new(),
        notes: String::new(),
        columns,
        rows,
        cell_scales: Vec::new(),
        review_cells: m
            .review
            .iter()
            .map(|r| ReviewCell {
                row: r.row,
                col: r.col,
                reason: r.reason.clone(),
            })
            .collect(),
    };

    // Labels and headers: a failing row or column goes to review alone.
    // Checking stops early on out-of-range indices, so after pruning, check
    // again until a pass finds nothing: one bad index must never switch off
    // the label and header checks for the rest of the table.
    loop {
        let mut problems = Vec::new();
        spec.check_against(table, registry, &mut problems);
        if problems.is_empty() {
            break;
        }
        let mut by_scope: BTreeMap<(Option<usize>, Option<usize>), Vec<String>> = BTreeMap::new();
        for p in problems {
            let key = match p.scope {
                Scope::Table => (None, None),
                Scope::Row(r) => (Some(r), None),
                Scope::Col(c) => (None, Some(c)),
                Scope::Cell(r, c) => (Some(r), Some(c)),
            };
            by_scope.entry(key).or_default().push(p.message);
        }
        let mut pruned = false;
        for ((row, col), messages) in by_scope {
            out.review
                .push(item(row, col, messages.join("; "), ReviewBy::Check));
            match (row, col) {
                (None, None) => return out,
                (Some(r), None) => spec.rows.retain(|x| x.row != r),
                (None, Some(c)) => spec.columns.retain(|x| x.col != c),
                (Some(_), Some(_)) => continue,
            }
            pruned = true;
        }
        if spec.rows.is_empty() || spec.columns.is_empty() {
            return out;
        }
        if !pruned {
            break;
        }
    }

    // Values: an unreadable or impossible cell goes to review alone.
    let mut expansion = Expansion {
        results: Vec::new(),
        missing: 0,
        review: 0,
    };
    let mut cell_problems = Vec::new();
    spec.read_cells(doc, table, registry, &mut expansion, &mut cell_problems);
    for p in cell_problems {
        if let Scope::Cell(r, c) = p.scope {
            out.review
                .push(item(Some(r), Some(c), p.message, ReviewBy::Check));
        }
    }
    out.results = expansion.results;
    out.missing = expansion.missing;
    out
}

fn is_default(claim: &SettingClaim, registry: &Registry) -> bool {
    registry
        .setting(&claim.setting)
        .and_then(|s| s.default.as_ref())
        .is_some_and(|d| d == &claim.value)
}

/// A claim holds when its setting and value exist and the cited text names the value.
fn check_claim(
    claim: &SettingClaim,
    col: Option<usize>,
    doc: &Document,
    table: &Table,
    header_rows: usize,
    registry: &Registry,
) -> Result<(), String> {
    let (setting, value, evidence) = (&claim.setting, &claim.value, &claim.evidence);
    let Some(s) = registry.setting(setting) else {
        return Err(format!("unknown setting {setting:?}"));
    };
    if !s.values.iter().any(|v| &v.id == value) {
        return Err(format!("{setting}: unknown value {value:?}"));
    }
    let text = match (evidence.as_str(), col) {
        ("caption", _) => table.caption.clone(),
        ("header", Some(c)) => {
            let grid = table.grid();
            (0..header_rows.min(grid.len()))
                .filter_map(|r| {
                    grid[r]
                        .get(c)
                        .copied()
                        .flatten()
                        .map(|cell| cell.text.clone())
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
        (id, _) => match doc
            .sections
            .iter()
            .flat_map(|s| &s.paragraphs)
            .find(|p| p.id == id)
        {
            Some(p) => p.text.clone(),
            None => {
                return Err(format!(
                    "{setting} = {value} cites {id:?}, which does not exist"
                ));
            }
        },
    };
    if mentions_value(&text, setting, value, registry) {
        Ok(())
    } else {
        Err(format!(
            "{setting} = {value} cites {evidence:?}, which does not say so"
        ))
    }
}

/// Whether `text` names `value` of `setting`, through any registry spelling,
/// in one to three consecutive words ("filtered", "tail prediction").
pub fn mentions_value(text: &str, setting: &str, value: &str, registry: &Registry) -> bool {
    let words: Vec<&str> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| ",.;:()[]{}'\"".contains(c)))
        .filter(|w| !w.is_empty())
        .collect();
    (1..=3).any(|n| {
        words.windows(n).any(|ws| {
            registry
                .resolve_setting_value(setting, &ws.join(" "))
                .is_some_and(|v| v.id == value)
        })
    })
}
