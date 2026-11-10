//! `sota fetch` and `sota tables`.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use sota_core::{ArxivId, Document};
use sota_ingest::arxiv_html;
use sota_ingest::cache::Cache;
use sota_ingest::fetch::{FetchOutcome, Fetcher, UreqHttp};

/// One pinned id per line; `#` starts a comment. Every line is checked before
/// anything is fetched.
pub fn read_list(path: &Path) -> Result<Vec<ArxivId>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for (n, line) in text.lines().enumerate() {
        let entry = line.split('#').next().unwrap_or_default().trim();
        if entry.is_empty() {
            continue;
        }
        let at = || format!("{}, line {}", path.display(), n + 1);
        let id: ArxivId = entry.parse().with_context(at)?;
        if id.version.is_none() {
            bail!("{}: {id} has no version; pin one (e.g. {id}v1)", at());
        }
        if !seen.insert(id.clone()) {
            bail!("{}: duplicate entry {id}", at());
        }
        ids.push(id);
    }
    Ok(ids)
}

pub fn fetch(list: &Path, cache: &Cache) -> Result<bool> {
    let ids = read_list(list)?;
    let to_download = ids.iter().filter(|id| !cache.contains(id)).count();
    println!(
        "{} papers listed, {} to download, 15 s apart (arXiv crawl delay)",
        ids.len(),
        to_download
    );

    let mut fetcher = Fetcher::new(UreqHttp::new(), std::thread::sleep);
    let (mut html, mut no_html, mut failed) = (Vec::new(), 0, 0);
    for (n, id) in ids.iter().enumerate() {
        print!("[{}/{}] {id:<16} ", n + 1, ids.len());
        std::io::stdout().flush().ok();
        match fetcher.fetch(id, cache) {
            Ok(FetchOutcome::Cached(_)) => {
                println!("cached");
                html.push(id);
            }
            Ok(FetchOutcome::Fetched { bytes, .. }) => {
                println!("fetched ({} KB)", bytes / 1024);
                html.push(id);
            }
            Ok(FetchOutcome::NoHtml) => {
                println!("no HTML version (404)");
                no_html += 1;
            }
            Err(err) => {
                println!("FAILED: {err}");
                failed += 1;
            }
        }
    }

    println!(
        "papers  listed {} | html {} | no html {no_html} | failed {failed}",
        ids.len(),
        html.len()
    );
    let mut stats = Coverage::default();
    for id in html {
        stats.add(&load(id, cache)?);
    }
    println!("tables  {stats}");
    Ok(failed == 0)
}

pub fn tables(id: &str, only: Option<&str>, cache: &Cache) -> Result<()> {
    let id: ArxivId = id.parse()?;
    if id.version.is_none() {
        bail!("{id} has no version; pass a pinned one (e.g. {id}v1)");
    }
    if !cache.contains(&id) {
        bail!(
            "{id} is not in the cache ({}); run `sota fetch` first",
            cache.root().display()
        );
    }
    let doc = load(&id, cache)?;
    if let Some(only) = only
        && doc.table(only).is_none()
    {
        let known: Vec<&str> = doc.tables.iter().map(|t| t.id.as_str()).collect();
        bail!("{id} has no table {only}; it has: {}", known.join(", "));
    }

    println!("# {} ({})\n", doc.title, doc.versioned_id());
    for table in doc.tables.iter().filter(|t| only.is_none_or(|o| o == t.id)) {
        println!("## {} ({}) {}", table.label, table.id, doc.table_url(table));
        if !table.caption.is_empty() {
            println!("{}", table.caption);
        }
        if table.conversion_errors {
            println!("> warning: LaTeXML reported conversion errors in this table");
        }
        let holes = table.holes();
        if holes > 0 {
            println!("> warning: {holes} grid slot(s) not covered by any cell");
        }
        println!("\n{}", table.to_markdown());
    }
    if only.is_none() && !doc.unparsed_tables.is_empty() {
        println!(
            "Figures without a parseable table: {}",
            doc.unparsed_tables.join(", ")
        );
    }
    Ok(())
}

fn load(id: &ArxivId, cache: &Cache) -> Result<Document> {
    let html = cache
        .read_html(id)
        .with_context(|| format!("reading {}", cache.html_path(id).display()))?;
    let version = id.version.context("unpinned id")?;
    Ok(arxiv_html::parse(&html, &id.id, version))
}

#[derive(Default)]
struct Coverage {
    figures: usize,
    tables: usize,
    without_table: usize,
    with_holes: usize,
    with_errors: usize,
}

impl Coverage {
    fn add(&mut self, doc: &Document) {
        let figures: HashSet<&str> = doc.tables.iter().map(|t| t.figure_id.as_str()).collect();
        self.figures += figures.len() + doc.unparsed_tables.len();
        self.tables += doc.tables.len();
        self.without_table += doc.unparsed_tables.len();
        self.with_holes += doc.tables.iter().filter(|t| t.holes() > 0).count();
        self.with_errors += doc.tables.iter().filter(|t| t.conversion_errors).count();
    }
}

impl std::fmt::Display for Coverage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "table figures {} | tables parsed {} | figures without a table {} | tables with holes {} | tables with conversion errors {}",
            self.figures, self.tables, self.without_table, self.with_holes, self.with_errors
        )
    }
}
