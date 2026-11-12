//! The gold set: what a careful human reads in a results table.
//!
//! A gold file does not list numbers. It maps each table's rows and columns to
//! meanings (method, dataset, metric, settings), and `expand` reads the numbers
//! out of the parsed table. While expanding, it checks the mapping against the
//! paper itself: row labels must read the declared method (or name the declared
//! dataset, in a transposed table), and column headers must resolve, through the
//! registry, to the declared metric and dataset (or method) without
//! contradicting the declared settings. A reviewer checks meanings, not
//! hundreds of numbers.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    ArxivId, ConfigError, Document, Registry, Scale, Table, ident, is_missing, parse_number,
    to_canonical,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Introduced by this paper.
    Proposed,
    /// Someone else's method, run or copied by this paper.
    Baseline,
    /// An ablation or variant of the proposed method.
    Variant,
}

/// Which axis holds the methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Rows are methods; columns are (dataset, metric). The usual layout.
    #[default]
    MethodsInRows,
    /// Rows are datasets; columns are (method, metric), e.g. ULTRA's Table 10.
    DatasetsInRows,
}

/// Where the dataset of each column is named (methods-in-rows tables).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetIn {
    #[default]
    Header,
    Caption,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldFile {
    /// Pinned arXiv id, e.g. "1902.10197v1".
    pub paper: String,
    #[serde(default)]
    pub title: String,
    /// Tables that hold no results (statistics, hyperparameters): anything
    /// extracted from them is a false positive.
    #[serde(default)]
    pub negative_tables: Vec<String>,
    #[serde(rename = "table")]
    pub tables: Vec<GoldTable>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldTable {
    pub id: String,
    #[serde(default = "default_split")]
    pub split: String,
    /// Rows at the top that hold headers; data rows start below.
    #[serde(default = "one")]
    pub header_rows: usize,
    /// Column holding the row labels.
    #[serde(default)]
    pub label_col: usize,
    #[serde(default)]
    pub orientation: Orientation,
    #[serde(default)]
    pub dataset_in: DatasetIn,
    /// Setting id -> value id, e.g. protocol = "filtered".
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    /// Where the paper states the settings (paragraph id and a paraphrase).
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub notes: String,
    pub columns: Vec<GoldColumn>,
    pub rows: Vec<GoldRow>,
    /// Cells printed on another scale than their column.
    #[serde(default)]
    pub cell_scales: Vec<CellScale>,
    /// Cells (or whole rows) that must not become results: a typo, a
    /// non-comparable setting. An extractor is expected to send them to review.
    #[serde(default)]
    pub review_cells: Vec<ReviewCell>,
}

/// A column: its metric, plus the dataset (methods in rows) or the method
/// (datasets in rows).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldColumn {
    pub col: usize,
    pub metric: String,
    #[serde(default)]
    pub dataset: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub method_id: String,
    #[serde(default)]
    pub role: Option<Role>,
    /// As printed; defaults to the metric's canonical scale.
    #[serde(default)]
    pub scale: Option<Scale>,
    /// Overrides the table's settings for this column (e.g. a raw-MRR column).
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

/// A row: its method (methods in rows) or its dataset (datasets in rows).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldRow {
    pub row: usize,
    /// Exactly as the label cell reads.
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub method_id: String,
    #[serde(default)]
    pub role: Option<Role>,
    #[serde(default)]
    pub dataset: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellScale {
    pub row: usize,
    pub col: usize,
    pub scale: Scale,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewCell {
    pub row: usize,
    /// `None`: every mapped column of the row.
    #[serde(default)]
    pub col: Option<usize>,
    pub reason: String,
}

/// One gold result: a number as printed in one paper, with its meaning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldResult {
    pub paper: String,
    pub table: String,
    pub row: usize,
    pub col: usize,
    pub locator: String,
    pub method: String,
    pub method_id: String,
    pub role: Role,
    pub dataset: String,
    pub metric: String,
    pub split: String,
    /// Every applicable setting, defaults included.
    pub settings: BTreeMap<String, String>,
    pub value_text: String,
    /// On the metric's canonical scale.
    pub value: f64,
    pub std: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Expansion {
    pub results: Vec<GoldResult>,
    /// Mapped cells that hold no result ("-").
    pub missing: usize,
    /// Cells left out because the spec marks them for review.
    pub review: usize,
}

/// Which part of a table a problem concerns, so a caller can set aside just
/// that part (extraction sends it to review) instead of the whole table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Table,
    Row(usize),
    Col(usize),
    Cell(usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub scope: Scope,
    pub message: String,
}

/// What one cell means, from its row and column.
struct Meaning<'a> {
    method: &'a str,
    method_id: &'a str,
    role: Role,
    dataset: &'a str,
}

fn default_split() -> String {
    "test".to_owned()
}

fn one() -> usize {
    1
}

impl GoldFile {
    pub fn from_toml_str(file: &str, text: &str) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|e| ConfigError::parse(file, e))
    }

    /// Checks that need no paper: ids, settings, structure.
    pub fn validate(&self, registry: &Registry) -> Vec<String> {
        let mut problems = Vec::new();
        match self.paper.parse::<ArxivId>() {
            Ok(id) if id.version.is_some() => {}
            _ => problems.push(format!(
                "paper {:?}: must be a pinned arXiv id (e.g. 1902.10197v1)",
                self.paper
            )),
        }
        if self.tables.is_empty() {
            problems.push("no [[table]]".to_owned());
        }
        for id in &self.negative_tables {
            if self.tables.iter().any(|t| &t.id == id) {
                problems.push(format!("table {id} is both mapped and listed as negative"));
            }
        }
        let mut ids = HashSet::new();
        for table in &self.tables {
            if !ids.insert(table.id.as_str()) {
                problems.push(format!("duplicate table {}", table.id));
            }
            for p in table.problems(registry) {
                problems.push(format!("table {}: {p}", table.id));
            }
        }
        problems
    }

    /// Checks committed results against this spec without the paper (CI has
    /// no cache): every result must sit in a mapped cell, carry that cell's
    /// meaning, and hold the value its printed text reads on the right scale.
    pub fn check_snapshot(&self, results: &[GoldResult], registry: &Registry) -> Vec<String> {
        let mut problems = self.validate(registry);
        if !problems.is_empty() {
            return problems;
        }
        let mut seen = HashSet::new();
        for r in results {
            let at = format!("{}#{}/r{}/c{}", r.paper, r.table, r.row, r.col);
            if r.paper != self.paper {
                problems.push(format!("{at}: paper is not {}", self.paper));
                continue;
            }
            if r.locator != at {
                problems.push(format!(
                    "{at}: locator {:?} does not match its cell",
                    r.locator
                ));
            }
            if !seen.insert((r.table.clone(), r.row, r.col)) {
                problems.push(format!("{at}: duplicate result"));
                continue;
            }
            let table = self.tables.iter().find(|t| t.id == r.table);
            let row = table.and_then(|t| t.rows.iter().find(|x| x.row == r.row));
            let col = table.and_then(|t| t.columns.iter().find(|x| x.col == r.col));
            let (Some(table), Some(row), Some(col)) = (table, row, col) else {
                problems.push(format!("{at}: cell is not mapped by the spec"));
                continue;
            };
            if table.in_review(r.row, r.col) {
                problems.push(format!("{at}: the spec marks this cell for review"));
                continue;
            }
            let Some(meaning) = table.meaning(row, col) else {
                continue; // validation reports the missing fields
            };
            let checks = [
                ("method", r.method == meaning.method),
                ("method_id", r.method_id == meaning.method_id),
                ("role", r.role == meaning.role),
                ("dataset", r.dataset == meaning.dataset),
                ("metric", r.metric == col.metric),
                ("split", r.split == table.split),
                ("settings", r.settings == table.settings_for(col, registry)),
            ];
            for (field, ok) in checks {
                if !ok {
                    problems.push(format!("{at}: {field} differs from the spec"));
                }
            }
            let Some(metric) = registry.metric(&col.metric) else {
                continue;
            };
            let scale = table.scale_for(r.row, col, metric.canonical_scale);
            match parse_number(&r.value_text)
                .and_then(|v| to_canonical(&v, scale, metric).map(|x| (x, v.std)))
            {
                Ok((value, std)) if value == r.value && std == r.std => {}
                Ok((value, _)) => problems.push(format!(
                    "{at}: value {} does not match its text {:?} (reads {value})",
                    r.value, r.value_text
                )),
                Err(e) => problems.push(format!("{at}: {e}")),
            }
        }
        problems
    }

    /// Reads the mapped cells out of `doc`, checking labels and headers on the way.
    pub fn expand(&self, doc: &Document, registry: &Registry) -> Result<Expansion, Vec<String>> {
        let mut problems = self.validate(registry);
        if !problems.is_empty() {
            return Err(problems);
        }
        if doc.versioned_id() != self.paper {
            return Err(vec![format!(
                "spec is for {}, document is {}",
                self.paper,
                doc.versioned_id()
            )]);
        }
        let mut expansion = Expansion {
            results: Vec::new(),
            missing: 0,
            review: 0,
        };
        for spec in &self.tables {
            let Some(table) = doc.table(&spec.id) else {
                let known: Vec<&str> = doc.tables.iter().map(|t| t.id.as_str()).collect();
                problems.push(format!(
                    "table {} not found in {}; it has {}",
                    spec.id,
                    self.paper,
                    known.join(", ")
                ));
                continue;
            };
            let mut scoped = Vec::new();
            spec.check_against(table, registry, &mut scoped);
            if scoped.is_empty() {
                spec.read_cells(doc, table, registry, &mut expansion, &mut scoped);
            }
            problems.extend(scoped.into_iter().map(|p| p.message));
        }
        if problems.is_empty() {
            Ok(expansion)
        } else {
            Err(problems)
        }
    }
}

impl GoldTable {
    /// The scale a cell is printed in: a cell override, else its column's, else canonical.
    fn scale_for(&self, row: usize, col: &GoldColumn, canonical: Scale) -> Scale {
        self.cell_scales
            .iter()
            .find(|s| (s.row, s.col) == (row, col.col))
            .map(|s| s.scale)
            .or(col.scale)
            .unwrap_or(canonical)
    }

    /// The settings the spec states for a column: the table's, then the column's.
    fn stated_settings(&self, col: &GoldColumn) -> BTreeMap<String, String> {
        let mut settings = self.settings.clone();
        settings.extend(col.settings.clone());
        settings
    }

    /// The stated settings plus the registry default of every unstated setting
    /// that applies to the column's metric: a result carries all of them.
    fn settings_for(&self, col: &GoldColumn, registry: &Registry) -> BTreeMap<String, String> {
        let mut settings = self.stated_settings(col);
        for s in registry.settings() {
            if let Some(default) = &s.default
                && s.applies_to.contains(&col.metric)
            {
                settings
                    .entry(s.id.clone())
                    .or_insert_with(|| default.clone());
            }
        }
        settings
    }

    fn in_review(&self, row: usize, col: usize) -> bool {
        self.review_cells
            .iter()
            .any(|r| r.row == row && r.col.is_none_or(|c| c == col))
    }

    /// `None` when a required field is missing (validation reports it).
    fn meaning<'a>(&'a self, row: &'a GoldRow, col: &'a GoldColumn) -> Option<Meaning<'a>> {
        let (method, method_id, role, dataset) = match self.orientation {
            Orientation::MethodsInRows => (&row.method, &row.method_id, row.role, &col.dataset),
            Orientation::DatasetsInRows => (&col.method, &col.method_id, col.role, &row.dataset),
        };
        Some(Meaning {
            method,
            method_id,
            role: role?,
            dataset,
        })
    }

    fn problems(&self, registry: &Registry) -> Vec<String> {
        let mut p = Vec::new();
        if self.header_rows == 0 {
            p.push("header_rows must be at least 1".to_owned());
        }
        if self.columns.is_empty() {
            p.push("no columns".to_owned());
        }
        if self.rows.is_empty() {
            p.push("no rows".to_owned());
        }
        let methods_in_rows = self.orientation == Orientation::MethodsInRows;

        let mut cols = HashSet::new();
        for c in &self.columns {
            let at = format!("col {}", c.col);
            if !cols.insert(c.col) {
                p.push(format!("duplicate col {}", c.col));
            }
            if c.col == self.label_col {
                p.push(format!("{at} is the label column"));
            }
            if methods_in_rows {
                check_dataset(&at, &c.dataset, &self.split, registry, &mut p);
                if !c.method.is_empty() || !c.method_id.is_empty() || c.role.is_some() {
                    p.push(format!(
                        "{at}: methods belong in the rows (orientation = methods_in_rows)"
                    ));
                }
            } else {
                check_method(&at, &c.method, &c.method_id, c.role, &mut p);
                if !c.dataset.is_empty() {
                    p.push(format!(
                        "{at}: datasets belong in the rows (orientation = datasets_in_rows)"
                    ));
                }
            }
            match registry.metric(&c.metric) {
                None => p.push(format!("{at}: unknown metric id {:?}", c.metric)),
                Some(m) => {
                    if let Some(scale) = c.scale
                        && !m.accepted_scales.contains(&scale)
                    {
                        p.push(format!("{at}: {} does not accept {scale:?}", m.id));
                    }
                }
            }
        }

        let all_settings = self
            .settings
            .iter()
            .chain(self.columns.iter().flat_map(|c| c.settings.iter()));
        let mut reported = HashSet::new();
        for (setting, value) in all_settings {
            let problem = match registry.setting(setting) {
                None => format!("unknown setting {setting:?}"),
                Some(s) if !s.values.iter().any(|v| &v.id == value) => {
                    format!("setting {setting}: unknown value {value:?}")
                }
                Some(_) => continue,
            };
            if reported.insert(problem.clone()) {
                p.push(problem);
            }
        }
        for c in &self.columns {
            let settings = self.stated_settings(c);
            for s in registry.settings().iter().filter(|s| s.required) {
                if s.applies_to.contains(&c.metric) && !settings.contains_key(&s.id) {
                    let problem = format!("setting {} is required for metric {}", s.id, c.metric);
                    if reported.insert(problem.clone()) {
                        p.push(problem);
                    }
                }
            }
        }

        let mut rows = HashSet::new();
        for r in &self.rows {
            let at = format!("row {}", r.row);
            if !rows.insert(r.row) {
                p.push(format!("duplicate row {}", r.row));
            }
            if r.row < self.header_rows {
                p.push(format!(
                    "{at} is inside the header (header_rows = {})",
                    self.header_rows
                ));
            }
            if methods_in_rows {
                check_method(&at, &r.method, &r.method_id, r.role, &mut p);
                if !r.dataset.is_empty() {
                    p.push(format!(
                        "{at}: datasets belong in the columns (orientation = methods_in_rows)"
                    ));
                }
            } else {
                check_dataset(&at, &r.dataset, &self.split, registry, &mut p);
                if !r.method.is_empty() || !r.method_id.is_empty() || r.role.is_some() {
                    p.push(format!(
                        "{at}: methods belong in the columns (orientation = datasets_in_rows)"
                    ));
                }
            }
        }

        for cs in &self.cell_scales {
            match self.columns.iter().find(|c| c.col == cs.col) {
                None => p.push(format!("cell_scales: col {} is not mapped", cs.col)),
                Some(c) => {
                    if registry
                        .metric(&c.metric)
                        .is_some_and(|m| !m.accepted_scales.contains(&cs.scale))
                    {
                        p.push(format!(
                            "cell_scales: {} does not accept {:?}",
                            c.metric, cs.scale
                        ));
                    }
                }
            }
            if !rows.contains(&cs.row) {
                p.push(format!("cell_scales: row {} is not mapped", cs.row));
            }
        }

        for r in &self.review_cells {
            if r.reason.trim().is_empty() {
                p.push(format!("review row {}: give a reason", r.row));
            }
            if r.row < self.header_rows {
                p.push(format!("review row {} is inside the header", r.row));
            }
            if let Some(col) = r.col
                && !cols.contains(&col)
            {
                p.push(format!("review row {}: col {col} is not mapped", r.row));
            }
        }
        p
    }

    /// Ranges, row labels and column headers against the parsed table.
    pub(crate) fn check_against(
        &self,
        table: &Table,
        registry: &Registry,
        problems: &mut Vec<Problem>,
    ) {
        let at = |scope: Scope, m: String| Problem {
            scope,
            message: format!("table {}: {m}", self.id),
        };
        let (n_rows, n_cols) = (table.n_rows(), table.n_cols());
        let mut out_of_range = false;
        for r in self.rows.iter().filter(|r| r.row >= n_rows) {
            problems.push(at(
                Scope::Row(r.row),
                format!("row {} is out of range ({n_rows} rows)", r.row),
            ));
            out_of_range = true;
        }
        for col in self.columns.iter().map(|c| c.col).filter(|&c| c >= n_cols) {
            problems.push(at(
                Scope::Col(col),
                format!("col {col} is out of range ({n_cols} cols)"),
            ));
            out_of_range = true;
        }
        if self.label_col >= n_cols {
            let col = self.label_col;
            problems.push(at(
                Scope::Table,
                format!("col {col} is out of range ({n_cols} cols)"),
            ));
            out_of_range = true;
        }
        if out_of_range {
            return;
        }

        let grid = table.grid();
        let text = |r: usize, c: usize| grid[r][c].map_or("", |cell| cell.text.as_str());
        for r in &self.rows {
            let label = text(r.row, self.label_col);
            match self.orientation {
                Orientation::MethodsInRows => {
                    if label != r.method {
                        problems.push(at(
                            Scope::Row(r.row),
                            format!(
                                "row {}: label cell reads {label:?}, the spec says {:?}",
                                r.row, r.method
                            ),
                        ));
                    }
                }
                Orientation::DatasetsInRows => {
                    let names = words(&[label.to_owned()]).iter().any(|w| {
                        registry
                            .resolve_dataset(w)
                            .is_some_and(|d| d.id == r.dataset)
                    });
                    if !names {
                        problems.push(at(Scope::Row(r.row), format!(
                            "row {}: label cell reads {label:?}, which does not name dataset {}",
                            r.row, r.dataset
                        )));
                    }
                }
            }
        }

        let caption_words = words(std::slice::from_ref(&table.caption));
        for c in &self.columns {
            let mut headers: Vec<String> = Vec::new();
            for r in 0..self.header_rows.min(n_rows) {
                let t = text(r, c.col).to_owned();
                if !t.is_empty() && !headers.contains(&t) {
                    headers.push(t);
                }
            }
            // "Hits" above "@10", or "Hits at" above "10": also try neighbours joined.
            let candidates = words(&with_joins(&headers));

            // A header that names a setting value must agree with the spec,
            // defaults included: a "0-shot" column cannot be supervised.
            for (setting, value) in self.settings_for(c, registry) {
                let contradicting = candidates.iter().find_map(|w| {
                    registry
                        .resolve_setting_value(&setting, w)
                        .filter(|v| v.id != value)
                        .map(|v| v.id.clone())
                });
                if let Some(other) = contradicting {
                    problems.push(at(Scope::Col(c.col), format!(
                        "col {}: header {headers:?} says {setting} = {other}, the spec says {value}",
                        c.col
                    )));
                }
            }

            let names_metric = candidates
                .iter()
                .any(|w| registry.resolve_metric(w).is_some_and(|m| m.id == c.metric));
            if !names_metric {
                problems.push(at(
                    Scope::Col(c.col),
                    format!(
                        "col {}: header {headers:?} does not name metric {}",
                        c.col, c.metric
                    ),
                ));
            }

            match self.orientation {
                Orientation::MethodsInRows => {
                    let place = match self.dataset_in {
                        DatasetIn::Header => &candidates,
                        DatasetIn::Caption => &caption_words,
                    };
                    let names_dataset = place.iter().any(|w| {
                        registry
                            .resolve_dataset(w)
                            .is_some_and(|d| d.id == c.dataset)
                    });
                    if !names_dataset {
                        problems.push(at(
                            Scope::Col(c.col),
                            format!(
                                "col {}: {} {headers:?} does not name dataset {}",
                                c.col,
                                match self.dataset_in {
                                    DatasetIn::Header => "header",
                                    DatasetIn::Caption => "caption (not header)",
                                },
                                c.dataset
                            ),
                        ));
                    }
                }
                Orientation::DatasetsInRows => {
                    if !headers.iter().any(|h| h.contains(&c.method)) {
                        problems.push(at(
                            Scope::Col(c.col),
                            format!(
                                "col {}: header {headers:?} does not name method {:?}",
                                c.col, c.method
                            ),
                        ));
                    }
                }
            }
        }

        // Inner headers: an unmapped row inside the data region (a block heading
        // such as "Prediction Head (MRR)") can switch the metric for the rows
        // below it. A mapped row under one that names another metric for a
        // mapped column must not be read under the top header's metric.
        let mapped: HashSet<usize> = self.rows.iter().map(|r| r.row).collect();
        for r in &self.rows {
            let inner = (self.header_rows..r.row)
                .filter(|rr| !mapped.contains(rr))
                .find_map(|rr| {
                    self.columns.iter().find_map(|c| {
                        let t = text(rr, c.col);
                        if t.is_empty() {
                            return None;
                        }
                        words(&[t.to_owned()]).iter().find_map(|w| {
                            registry
                                .resolve_metric(w)
                                .filter(|m| m.id != c.metric)
                                .map(|m| (rr, c.col, m.id.clone(), c.metric.clone()))
                        })
                    })
                });
            if let Some((rr, col, other, declared)) = inner {
                problems.push(at(
                    Scope::Row(r.row),
                    format!(
                        "row {}: sits under an inner header (row {rr}) naming metric {other} in col {col}, the spec says {declared}",
                        r.row
                    ),
                ));
            }
        }
    }

    pub(crate) fn read_cells(
        &self,
        doc: &Document,
        table: &Table,
        registry: &Registry,
        expansion: &mut Expansion,
        problems: &mut Vec<Problem>,
    ) {
        let grid = table.grid();
        // Review rows need not be mapped; count what they would have yielded.
        for review in self
            .review_cells
            .iter()
            .filter(|x| !self.rows.iter().any(|r| r.row == x.row))
        {
            expansion.review += if review.col.is_some() {
                1
            } else {
                self.columns.len()
            };
        }
        for r in &self.rows {
            for c in &self.columns {
                let locator = doc.locator(table, r.row, c.col);
                if self.in_review(r.row, c.col) {
                    expansion.review += 1;
                    continue;
                }
                let at = |message: String| Problem {
                    scope: Scope::Cell(r.row, c.col),
                    message,
                };
                let Some(cell) = grid[r.row][c.col] else {
                    problems.push(at(format!("{locator}: no cell")));
                    continue;
                };
                if (cell.row, cell.col) != (r.row, c.col) {
                    problems.push(at(format!(
                        "{locator}: covered by a span from r{}/c{}",
                        cell.row, cell.col
                    )));
                    continue;
                }
                if is_missing(&cell.text) {
                    expansion.missing += 1;
                    continue;
                }
                // Validation guarantees the metric and the meaning fields exist.
                let (Some(metric), Some(meaning)) =
                    (registry.metric(&c.metric), self.meaning(r, c))
                else {
                    continue;
                };
                let scale = self.scale_for(r.row, c, metric.canonical_scale);
                let value = parse_number(&cell.text).and_then(|v| {
                    to_canonical(&v, scale, metric).map(|canonical| (canonical, v.std))
                });
                match value {
                    Err(e) => problems.push(at(format!("{locator}: {e}"))),
                    Ok((value, std)) => expansion.results.push(GoldResult {
                        paper: doc.versioned_id(),
                        table: table.id.clone(),
                        row: r.row,
                        col: c.col,
                        locator,
                        method: meaning.method.to_owned(),
                        method_id: meaning.method_id.to_owned(),
                        role: meaning.role,
                        dataset: meaning.dataset.to_owned(),
                        metric: c.metric.clone(),
                        split: self.split.clone(),
                        settings: self.settings_for(c, registry),
                        value_text: cell.text.clone(),
                        value,
                        std,
                    }),
                }
            }
        }
    }
}

fn check_method(at: &str, method: &str, method_id: &str, role: Option<Role>, p: &mut Vec<String>) {
    if method.trim().is_empty() {
        p.push(format!("{at}: missing method"));
    }
    if !ident::is_registry_id(method_id) {
        p.push(format!(
            "{at}: method_id {method_id:?} is not a valid id (lowercase slug)"
        ));
    }
    if role.is_none() {
        p.push(format!(
            "{at}: missing role (proposed, baseline or variant)"
        ));
    }
}

fn check_dataset(at: &str, dataset: &str, split: &str, registry: &Registry, p: &mut Vec<String>) {
    if dataset.is_empty() {
        p.push(format!("{at}: missing dataset"));
        return;
    }
    match registry.dataset(dataset) {
        None => p.push(format!("{at}: unknown dataset id {dataset:?}")),
        Some(d) if !d.splits.is_empty() && !d.splits.iter().any(|s| s == split) => {
            p.push(format!("split {split:?} is not a split of {}", d.id));
        }
        Some(_) => {}
    }
}

/// The texts, then every run of two or more consecutive texts joined by a space.
fn with_joins(texts: &[String]) -> Vec<String> {
    let mut out = texts.to_vec();
    for start in 0..texts.len() {
        for end in start + 2..=texts.len() {
            out.push(texts[start..end].join(" "));
        }
    }
    out
}

/// Every text whole, then word by word, with arrows, brackets and markers
/// stripped: "MRR ↑", "FB15k-237 (filtered)" and "H@10(%)" all yield their name.
fn words(texts: &[String]) -> Vec<String> {
    let clean = |s: &str| {
        s.trim_matches(|c: char| {
            "()[]{}%*\u{2191}\u{2193}\u{2020}\u{2021},.;:".contains(c) || c.is_whitespace()
        })
        .to_owned()
    };
    let mut out = Vec::new();
    for text in texts {
        out.push(clean(text));
        for word in text.split_whitespace() {
            out.push(clean(word));
        }
        for part in text.split(['(', ')']) {
            out.push(clean(part));
        }
    }
    out.retain(|w| !w.is_empty());
    out
}
