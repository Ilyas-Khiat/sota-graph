//! Table mapping with a language model.
//!
//! - `prompt`: what the model sees (the table as a grid with coordinates, its
//!   caption, candidate evidence paragraphs) and the strict output schema,
//!   closed over the registry's ids.
//! - `llm`: the model behind an interface: Azure OpenAI over HTTP, and a
//!   record/replay wrapper so tests and evals run offline and deterministically.
//! - `pipeline`: one paper, table by table, each answer verified by
//!   `sota_core::verify_mapping` into results and a review queue.

pub mod llm;
pub mod pipeline;
pub mod prompt;
