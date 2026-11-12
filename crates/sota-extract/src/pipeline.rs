//! One paper, table by table.

use serde::{Deserialize, Serialize};
use sota_core::{
    Document, GoldResult, Registry, ReviewBy, ReviewItem, TableMapping, verify_mapping,
};

use crate::llm::{LlmClient, LlmError, Usage};
use crate::prompt::request_for;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableRun {
    pub table: String,
    /// The model's answer, kept for inspection.
    pub mapping: Option<TableMapping>,
    pub error: Option<String>,
    pub results: usize,
    pub review: usize,
    pub out_of_registry: Vec<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperExtraction {
    pub paper: String,
    pub model: String,
    pub tables: Vec<TableRun>,
    pub results: Vec<GoldResult>,
    pub review: Vec<ReviewItem>,
    pub usage: Usage,
    /// Set when an error made further calls pointless (bad key, wrong
    /// endpoint or deployment): the run stopped there.
    #[serde(default)]
    pub aborted: Option<String>,
}

/// Errors that will repeat for every table: stop at the first one.
fn is_fatal(e: &LlmError) -> bool {
    matches!(
        e,
        LlmError::Http {
            status: 401 | 403 | 404,
            ..
        }
    )
}

/// Maps every table of `doc` (or only `only`), verifies each answer, and
/// collects results and review items. A failed call or a malformed answer
/// sends its table to review; the other tables still count.
pub fn extract_paper(
    doc: &Document,
    registry: &Registry,
    client: &mut dyn LlmClient,
    only: Option<&str>,
) -> PaperExtraction {
    let mut x = PaperExtraction {
        paper: doc.versioned_id(),
        model: client.model(),
        tables: Vec::new(),
        results: Vec::new(),
        review: Vec::new(),
        usage: Usage::default(),
        aborted: None,
    };
    for table in doc.tables.iter().filter(|t| only.is_none_or(|o| o == t.id)) {
        let mut run = TableRun {
            table: table.id.clone(),
            mapping: None,
            error: None,
            results: 0,
            review: 0,
            out_of_registry: Vec::new(),
            usage: Usage::default(),
        };
        let failed = |reason: String| ReviewItem {
            paper: doc.versioned_id(),
            table: table.id.clone(),
            row: None,
            col: None,
            reason,
            by: ReviewBy::Check,
        };
        match client.complete(&request_for(doc, table, registry)) {
            Err(e) => {
                run.error = Some(e.to_string());
                x.review.push(failed(format!("extraction failed: {e}")));
                run.review = 1;
                if is_fatal(&e) {
                    x.aborted = Some(format!("stopped after {}: {e}", table.id));
                    x.tables.push(run);
                    break;
                }
            }
            Ok(completion) => {
                run.usage = completion.usage;
                x.usage.add(&completion.usage);
                match serde_json::from_str::<TableMapping>(&completion.content) {
                    Err(e) => {
                        run.error = Some(e.to_string());
                        x.review
                            .push(failed(format!("not a valid table mapping: {e}")));
                        run.review = 1;
                    }
                    Ok(mapping) => {
                        let outcome = verify_mapping(&mapping, doc, table, registry);
                        run.results = outcome.results.len();
                        run.review = outcome.review.len();
                        run.out_of_registry = outcome.out_of_registry;
                        run.mapping = Some(mapping);
                        x.results.extend(outcome.results);
                        x.review.extend(outcome.review);
                    }
                }
            }
        }
        x.tables.push(run);
    }
    x
}
