//! What the model sees, and the shape its answer must take.

use serde_json::{Value, json};
use sota_core::{Document, Paragraph, Registry, Table, mentions_value};

use crate::llm::LlmRequest;

/// Candidate evidence paragraphs at most, and characters per paragraph.
const MAX_PARAGRAPHS: usize = 15;
const MAX_PARAGRAPH_CHARS: usize = 700;

pub fn request_for(doc: &Document, table: &Table, registry: &Registry) -> LlmRequest {
    let evidence: Vec<String> = evidence_paragraphs(doc, table, registry)
        .iter()
        .map(|p| p.id.clone())
        .collect();
    LlmRequest {
        system: system_prompt(registry),
        user: user_prompt(doc, table, registry),
        schema_name: "table_mapping".to_owned(),
        schema: schema(registry, &evidence),
    }
}

/// Stable across tables (instructions and registry), so it can be cached.
pub fn system_prompt(registry: &Registry) -> String {
    let mut s = String::from(INSTRUCTIONS);
    s.push_str("\n\n## Registry\n\nDatasets (id: name; other spellings):\n");
    for d in registry.datasets() {
        s.push_str(&format!(
            "- {}: {}; {}\n",
            d.id,
            d.name,
            d.aliases.join(", ")
        ));
    }
    s.push_str("\nMetrics (id: name; other spellings; scales it may be printed in):\n");
    for m in registry.metrics() {
        let scales: Vec<String> = m
            .accepted_scales
            .iter()
            .map(|x| format!("{x:?}").to_lowercase())
            .collect();
        s.push_str(&format!(
            "- {}: {}; {}; {}\n",
            m.id,
            m.name,
            m.aliases.join(", "),
            scales.join("/")
        ));
    }
    s.push_str("\nSettings (id: values):\n");
    for st in registry.settings() {
        let values: Vec<&str> = st.values.iter().map(|v| v.id.as_str()).collect();
        let rule = match (&st.default, st.required) {
            (_, true) => "required: results without it go to review".to_owned(),
            (Some(d), _) => format!("default {d}: state it only when the paper says otherwise"),
            (None, false) => "optional".to_owned(),
        };
        s.push_str(&format!(
            "- {} ({}): {} [{rule}]\n",
            st.id,
            st.name,
            values.join(", ")
        ));
    }
    s
}

const INSTRUCTIONS: &str = r#"You map one table from a research paper to its meaning, for a knowledge graph of benchmark results.

Never output numbers. Code reads every value from the cell you point to, and checks your mapping against the table itself: a row label must read exactly the method you name, a column header must name the dataset and metric you give. Anything that fails goes to human review, so precision matters more than coverage.

Answer with:
1. is_results_table: true only if the table reports results of methods on benchmarks. Dataset statistics, hyperparameters, complexity or feature comparisons, examples: false, with everything else empty.
2. orientation: "methods_in_rows" when each row is a method; "datasets_in_rows" when each row is a dataset and the methods are in the column headers.
3. header_rows: how many rows at the top hold headers. label_col: the column holding the row labels. dataset_in: "header" when the column headers name the datasets, "caption" when the table covers one dataset named only in the caption (for example "ablation study on FB15k-237").
4. columns: one entry per column that holds results. metric and dataset are registry ids; use "unknown" when the column's metric or dataset is not in the registry. scale is how the numbers are printed: "fraction" (0.338 or .338), "percent" (33.8), "raw" (counts such as mean rank). Methods in rows: method and role null. Datasets in rows: dataset null, method copied from the header, and its role.
5. rows: methods in rows: one entry per method row, method copied exactly as the label cell reads (keep markers such as *, (ours), [\varheart]), and its role: "proposed" (introduced by this paper), "variant" (an ablation, a simplified or renamed version of the proposed method, such as pRotatE for RotatE), "baseline" (anything else). A method keeps the same role in every table of the paper. Skip rows that only group others. If an inner header row partway down the table switches the metric or setting, map only the rows under the top header and list the rows below the inner header in review. Datasets in rows: the dataset id, method and role null.
6. settings: settings the paper states for the whole table; columns[].settings: settings that differ for one column, such as a "Raw" column in a filtered table, a "Prediction Head" or "Prediction Tail" column (queries), or a column for one relation category ("1-to-N"); cite "header" when the header says it. Every claim cites its evidence by id only: a paragraph id from the list given (for example S4.p2, without its text), "caption", or for a column "header". Code checks that the cited text says it. Leave out settings at their default. Do not rely on knowledge from outside this paper: if the paper does not state a required setting, leave it out and the results will go to review, which is the correct outcome.
7. review: cells you do not trust as results, with the reason: an obvious typo, a value the paper says was measured differently (other negatives, other protocol). col null means the whole row."#;

/// The table as a grid with coordinates, its caption, and candidate evidence.
pub fn user_prompt(doc: &Document, table: &Table, registry: &Registry) -> String {
    let mut s = format!(
        "Paper: {} ({})\nTable: {} ({})\nCaption: {}\n\n",
        doc.title,
        doc.versioned_id(),
        table.id,
        table.label,
        table.caption
    );
    s.push_str("Grid (row r, column c; a cell spanning several rows or columns is shown once, at its top-left):\n");
    for r in 0..table.n_rows() {
        let mut cells: Vec<&sota_core::Cell> = table.cells.iter().filter(|c| c.row == r).collect();
        cells.sort_by_key(|c| c.col);
        let rendered: Vec<String> = cells
            .iter()
            .map(|c| {
                let mut t = format!(
                    "c{}={}",
                    c.col,
                    serde_json::to_string(&c.text).unwrap_or_default()
                );
                if c.row_span > 1 {
                    t.push_str(&format!(" [rows r{}-r{}]", c.row, c.row + c.row_span - 1));
                }
                if c.col_span > 1 {
                    t.push_str(&format!(" [cols c{}-c{}]", c.col, c.col + c.col_span - 1));
                }
                t
            })
            .collect();
        s.push_str(&format!("r{r}: {}\n", rendered.join(" | ")));
    }
    let evidence = evidence_paragraphs(doc, table, registry);
    s.push_str("\nParagraphs that may be evidence (cite them by id):\n");
    if evidence.is_empty() {
        s.push_str("(none)\n");
    }
    for p in evidence {
        let text: String = p.text.chars().take(MAX_PARAGRAPH_CHARS).collect();
        let more = if p.text.chars().count() > MAX_PARAGRAPH_CHARS {
            " ..."
        } else {
            ""
        };
        s.push_str(&format!("[{}] {text}{more}\n", p.id));
    }
    s
}

/// Paragraphs that cite the table, or name a setting value that needs
/// evidence (every value of a required setting, non-default values of others),
/// at most `MAX_PARAGRAPHS`, in document order.
///
/// When there are more, they are chosen by what the checks need, not by
/// position: first those citing the table, then those naming a value of a
/// required setting (without one, the whole table goes to review), then the
/// rest. Optional values that are also everyday words ("head entity", "raw
/// text") can then no longer crowd out the evaluation section.
pub fn evidence_paragraphs<'a>(
    doc: &'a Document,
    table: &Table,
    registry: &Registry,
) -> Vec<&'a Paragraph> {
    let number = table
        .label
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .trim_end_matches('.')
        .to_owned();
    let cites_table = |text: &str| {
        !number.is_empty()
            && ["Table ", "Tab. ", "Tab "].iter().any(|prefix| {
                let needle = format!("{prefix}{number}");
                text.match_indices(&needle).any(|(i, _)| {
                    !text[i + needle.len()..].starts_with(|c: char| c.is_ascii_digit())
                })
            })
    };
    let names_setting = |text: &str, required: bool| {
        registry
            .settings()
            .iter()
            .filter(|s| s.required == required)
            .any(|s| {
                s.values
                    .iter()
                    .filter(|v| s.default.as_deref() != Some(v.id.as_str()))
                    .any(|v| mentions_value(text, &s.id, &v.id, registry))
            })
    };
    let tier = |text: &str| {
        if cites_table(text) {
            Some(0)
        } else if names_setting(text, true) {
            Some(1)
        } else if names_setting(text, false) {
            Some(2)
        } else {
            None
        }
    };
    // (tier, position): sorted by tier to choose, then by position to show.
    let mut chosen: Vec<(u8, usize, &Paragraph)> = doc
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .enumerate()
        .filter_map(|(i, p)| tier(&p.text).map(|t| (t, i, p)))
        .collect();
    chosen.sort_by_key(|&(t, i, _)| (t, i));
    chosen.truncate(MAX_PARAGRAPHS);
    chosen.sort_by_key(|&(_, i, _)| i);
    chosen.into_iter().map(|(_, _, p)| p).collect()
}

/// Strict JSON schema of `sota_core::TableMapping`, with ids closed over the
/// registry, and evidence closed over what this table's prompt offers.
pub fn schema(registry: &Registry, evidence_ids: &[String]) -> Value {
    let mut metrics: Vec<Value> = registry.metrics().iter().map(|m| json!(m.id)).collect();
    metrics.push(json!("unknown"));
    let mut datasets: Vec<Value> = registry.datasets().iter().map(|d| json!(d.id)).collect();
    datasets.push(json!("unknown"));
    datasets.push(Value::Null);
    let settings: Vec<Value> = registry.settings().iter().map(|s| json!(s.id)).collect();
    let mut values: Vec<Value> = Vec::new();
    for s in registry.settings() {
        for v in &s.values {
            if !values.contains(&json!(v.id)) {
                values.push(json!(v.id));
            }
        }
    }
    let mut evidence: Vec<Value> = vec![json!("caption"), json!("header")];
    evidence.extend(evidence_ids.iter().map(|id| json!(id)));
    let claim = json!({
        "type": "object", "additionalProperties": false,
        "required": ["setting", "value", "evidence"],
        "properties": {
            "setting": { "type": "string", "enum": settings },
            "value": { "type": "string", "enum": values },
            "evidence": { "type": "string", "enum": evidence },
        },
    });
    let role =
        json!({ "type": ["string", "null"], "enum": ["proposed", "baseline", "variant", null] });
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["is_results_table", "orientation", "header_rows", "label_col", "dataset_in", "settings", "columns", "rows", "review"],
        "properties": {
            "is_results_table": { "type": "boolean" },
            "orientation": { "type": "string", "enum": ["methods_in_rows", "datasets_in_rows"] },
            "header_rows": { "type": "integer" },
            "label_col": { "type": "integer" },
            "dataset_in": { "type": "string", "enum": ["header", "caption"] },
            "settings": { "type": "array", "items": claim },
            "columns": { "type": "array", "items": {
                "type": "object", "additionalProperties": false,
                "required": ["col", "metric", "dataset", "method", "role", "scale", "settings"],
                "properties": {
                    "col": { "type": "integer" },
                    "metric": { "type": "string", "enum": metrics },
                    "dataset": { "type": ["string", "null"], "enum": datasets },
                    "method": { "type": ["string", "null"] },
                    "role": role,
                    "scale": { "type": "string", "enum": ["fraction", "percent", "raw"] },
                    "settings": { "type": "array", "items": claim },
                },
            }},
            "rows": { "type": "array", "items": {
                "type": "object", "additionalProperties": false,
                "required": ["row", "method", "role", "dataset"],
                "properties": {
                    "row": { "type": "integer" },
                    "method": { "type": ["string", "null"] },
                    "role": role,
                    "dataset": { "type": ["string", "null"], "enum": datasets },
                },
            }},
            "review": { "type": "array", "items": {
                "type": "object", "additionalProperties": false,
                "required": ["row", "col", "reason"],
                "properties": {
                    "row": { "type": "integer" },
                    "col": { "type": ["integer", "null"] },
                    "reason": { "type": "string" },
                },
            }},
        },
    })
}
