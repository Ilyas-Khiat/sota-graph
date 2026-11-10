//! Opt-in checks against real arXiv pages in the local cache. The pages are
//! not in the repo (redistribution), so these are ignored by default:
//!
//!     sota fetch --list papers/kgc.txt
//!     cargo test -p sota-ingest --test real_cache -- --ignored
//!
//! Set SOTA_CACHE_DIR to use another cache than ./cache/arxiv.

use std::path::PathBuf;

use sota_core::{ArxivId, Document, Table};
use sota_ingest::arxiv_html::{parse, stated_version};
use sota_ingest::cache::Cache;

fn cache() -> Cache {
    let root = std::env::var_os("SOTA_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../cache/arxiv"))
        });
    Cache::new(root)
}

fn load(id: &str) -> Document {
    let id: ArxivId = id.parse().unwrap();
    let html = cache()
        .read_html(&id)
        .unwrap_or_else(|_| panic!("{id} not cached: run `sota fetch --list papers/kgc.txt`"));
    parse(&html, &id.id, id.version.unwrap())
}

fn table<'a>(doc: &'a Document, id: &str) -> &'a Table {
    doc.table(id)
        .unwrap_or_else(|| panic!("{} has no table {id}", doc.versioned_id()))
}

/// Grid row whose first column reads `method`.
fn row_of(t: &Table, method: &str) -> usize {
    t.grid()
        .iter()
        .position(|r| {
            r.first()
                .copied()
                .flatten()
                .is_some_and(|c| c.text == method)
        })
        .unwrap_or_else(|| panic!("no row {method:?} in {}", t.id))
}

fn text(t: &Table, row: usize, col: usize) -> &str {
    t.grid()[row][col].map_or("<hole>", |c| c.text.as_str())
}

#[test]
#[ignore = "needs the local paper cache"]
fn every_cached_paper_parses_into_full_grids() {
    let cache = cache();
    let ids = cache.list().unwrap();
    assert!(!ids.is_empty(), "empty cache at {}", cache.root().display());
    let mut problems = Vec::new();
    for id in &ids {
        let html = cache.read_html(id).unwrap();
        if stated_version(&html, &id.id) != id.version {
            problems.push(format!("{id}: page states another version"));
        }
        let doc = parse(&html, &id.id, id.version.unwrap());
        if doc.tables.is_empty() {
            problems.push(format!("{id}: no table parsed"));
        }
        for t in doc.tables.iter().filter(|t| t.holes() > 0) {
            problems.push(format!("{id} {}: {} hole(s)", t.id, t.holes()));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// RotatE, Table 5: plain <table> markup, two-level header, bold best values.
#[test]
#[ignore = "needs the local paper cache"]
fn rotate_main_results() {
    let doc = load("1902.10197v1");
    let t = table(&doc, "S4.T5");
    assert_eq!(text(t, 0, 1), "FB15k-237");
    assert_eq!(text(t, 1, 2), "MRR");
    let r = row_of(t, "RotatE");
    assert_eq!(text(t, r, 2), ".338"); // FB15k-237 MRR
    assert_eq!(text(t, r, 5), ".533"); // FB15k-237 H@10
    assert_eq!(text(t, r, 7), ".476"); // WN18RR MRR
    assert!(t.grid()[r][2].unwrap().bold);
}

/// NBFNet, Table 3: a resized table, written with <span> rows and cells.
#[test]
#[ignore = "needs the local paper cache"]
fn nbfnet_span_table() {
    let doc = load("2106.06935v4");
    let t = table(&doc, "S4.T3");
    assert!(t.n_rows() >= 10, "{} rows", t.n_rows());
    assert_eq!(t.holes(), 0);
    assert!(t.cells.iter().any(|c| c.text.starts_with("NBFNet")));
    assert!(t.caption.contains("taken from"), "{}", t.caption);
}

/// CompGCN, Table 5: one figure holding two tables.
#[test]
#[ignore = "needs the local paper cache"]
fn compgcn_two_tables_in_one_figure() {
    let doc = load("1911.03082v2");
    for id in ["S6.T5.t1", "S6.T5.t2"] {
        assert_eq!(table(&doc, id).figure_id, "S6.T5");
    }
}
