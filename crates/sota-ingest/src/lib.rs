//! Input channels for sota-graph. Each channel turns a source into the shared
//! `sota_core::Document`. Parsing is pure; fetching and caching are kept apart
//! so tests never touch the network.

pub mod arxiv_html;
pub mod cache;
pub mod fetch;
