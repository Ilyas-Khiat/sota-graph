//! `sota gold check | expand | show`.
//!
//! Each spec `<dir>/<topic>/<paper>.toml` has a snapshot next to it,
//! `<paper>.results.jsonl`: the results the spec expands to, committed so CI
//! can check them without the papers, and so parser changes show up as diffs.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sota_core::{
    ArxivId, Document, GoldFile, GoldResult, Orientation, Registry, Role, parse_number,
};
use sota_ingest::arxiv_html;
use sota_ingest::cache::Cache;

struct Spec {
    path: PathBuf,
    gold: Result<GoldFile, String>,
}

impl Spec {
    fn snapshot_path(&self) -> PathBuf {
        self.path.with_extension("results.jsonl")
    }

    fn name(&self) -> String {
        match &self.gold {
            Ok(g) => g.paper.clone(),
            Err(_) => self.path.display().to_string(),
        }
    }
}

pub fn check(dir: &Path, cache: &Cache, registry: &Registry) -> Result<bool> {
    let specs = discover(dir)?;
    let (mut total, mut failed) = (0, 0);
    for spec in &specs {
        let (problems, note, n) = check_one(spec, cache, registry);
        total += n;
        if problems.is_empty() {
            println!("{:<16} {n:>4} results  ok ({note})", spec.name());
        } else {
            failed += 1;
            println!("{:<16} FAILED", spec.name());
            for p in &problems {
                println!("  - {p}");
            }
        }
    }
    println!("{} spec(s), {total} results, {failed} failed", specs.len());
    Ok(failed == 0)
}

fn check_one(
    spec: &Spec,
    cache: &Cache,
    registry: &Registry,
) -> (Vec<String>, &'static str, usize) {
    let gold = match &spec.gold {
        Ok(g) => g,
        Err(e) => return (vec![e.clone()], "", 0),
    };
    let snapshot = match read_snapshot(&spec.snapshot_path()) {
        Ok(s) => s,
        Err(e) => return (vec![format!("{e:#}; run `sota gold expand`")], "", 0),
    };
    let mut problems = gold.check_snapshot(&snapshot, registry);
    let n = snapshot.len();
    let id = match gold.paper.parse::<ArxivId>() {
        Ok(id) if cache.contains(&id) => id,
        _ => return (problems, "structural checks only: paper not cached", n),
    };
    match load(&id, cache).map(|doc| gold.expand(&doc, registry)) {
        Err(e) => problems.push(format!("{e:#}")),
        Ok(Err(expansion_problems)) => problems.extend(expansion_problems),
        Ok(Ok(expansion)) => {
            let (mut fresh, mut committed) = (expansion.results, snapshot);
            fresh.sort_by_key(|r| (r.table.clone(), r.row, r.col));
            committed.sort_by_key(|r| (r.table.clone(), r.row, r.col));
            if fresh != committed {
                let differ = fresh.iter().filter(|r| !committed.contains(r)).count()
                    + committed.iter().filter(|r| !fresh.contains(r)).count();
                problems.push(format!(
                    "stale snapshot: {differ} result(s) differ from what the paper gives; run `sota gold expand`"
                ));
            }
        }
    }
    (problems, "re-expanded from cache", n)
}

pub fn expand(dir: &Path, only: Option<&str>, cache: &Cache, registry: &Registry) -> Result<bool> {
    let specs = discover(dir)?;
    let mut ok = true;
    let mut matched = false;
    for spec in &specs {
        let gold = match &spec.gold {
            Ok(g) => g,
            Err(e) => {
                ok = false;
                println!("{e}");
                continue;
            }
        };
        if only.is_some_and(|p| p != gold.paper) {
            continue;
        }
        matched = true;
        let doc = gold
            .paper
            .parse::<ArxivId>()
            .map_err(anyhow::Error::from)
            .and_then(|id| {
                if cache.contains(&id) {
                    load(&id, cache)
                } else {
                    bail!("not cached; run `sota fetch` first")
                }
            });
        let result = doc.map(|d| gold.expand(&d, registry));
        match result {
            Err(e) => {
                ok = false;
                println!("{:<16} FAILED: {e:#}", gold.paper);
            }
            Ok(Err(problems)) => {
                ok = false;
                println!("{:<16} FAILED", gold.paper);
                for p in problems {
                    println!("  - {p}");
                }
            }
            Ok(Ok(expansion)) => {
                let lines = expansion
                    .results
                    .iter()
                    .map(serde_json::to_string)
                    .collect::<Result<Vec<_>, _>>()?;
                let path = spec.snapshot_path();
                fs::write(&path, lines.join("\n") + "\n")
                    .with_context(|| format!("writing {}", path.display()))?;
                println!(
                    "{:<16} {} results, {} missing, {} for review -> {}",
                    gold.paper,
                    expansion.results.len(),
                    expansion.missing,
                    expansion.review,
                    path.display()
                );
            }
        }
    }
    if let Some(paper) = only
        && !matched
    {
        bail!("no gold spec for {paper} under {}", dir.display());
    }
    Ok(ok)
}

pub fn show(paper: &str, dir: &Path, cache: &Cache, registry: &Registry) -> Result<bool> {
    let specs = discover(dir)?;
    let gold = specs
        .iter()
        .filter_map(|s| s.gold.as_ref().ok())
        .find(|g| g.paper == paper)
        .with_context(|| format!("no gold spec for {paper} under {}", dir.display()))?;
    let id: ArxivId = paper.parse()?;
    if !cache.contains(&id) {
        bail!("{paper} is not cached; run `sota fetch` first");
    }
    let doc = load(&id, cache)?;
    let expansion = match gold.expand(&doc, registry) {
        Ok(e) => e,
        Err(problems) => {
            println!("{paper}: the spec does not hold against the paper:");
            for p in problems {
                println!("  - {p}");
            }
            return Ok(false);
        }
    };

    println!("# {} ({paper})\n", doc.title);
    for spec in &gold.tables {
        let Some(table) = doc.table(&spec.id) else {
            continue;
        };
        println!("## {} ({}) {}", table.label, table.id, doc.table_url(table));
        let settings: Vec<String> = spec
            .settings
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        println!(
            "split {} | {} | evidence: {}",
            spec.split,
            settings.join(", "),
            if spec.evidence.is_empty() {
                "-"
            } else {
                &spec.evidence
            }
        );
        if !spec.notes.is_empty() {
            println!("notes: {}", spec.notes);
        }
        for review in &spec.review_cells {
            let cell = review
                .col
                .map_or("all columns".to_owned(), |c| format!("col {c}"));
            println!("review: row {}, {cell}: {}", review.row, review.reason);
        }
        let role = |r: Option<Role>| {
            r.map_or("?", |r| match r {
                Role::Proposed => "proposed",
                Role::Baseline => "baseline",
                Role::Variant => "variant",
            })
        };
        let headers: Vec<String> = spec
            .columns
            .iter()
            .map(|c| {
                let what = match spec.orientation {
                    Orientation::MethodsInRows => c.dataset.clone(),
                    Orientation::DatasetsInRows => format!("{} ({})", c.method_id, role(c.role)),
                };
                let extra: Vec<String> =
                    c.settings.iter().map(|(k, v)| format!("{k}={v}")).collect();
                if extra.is_empty() {
                    format!("{what} {}", c.metric)
                } else {
                    format!("{what} {} [{}]", c.metric, extra.join(", "))
                }
            })
            .collect();
        println!(
            "
| row label (as printed) | means | {} |",
            headers.join(" | ")
        );
        println!("|{}", " --- |".repeat(2 + headers.len()));
        let grid = table.grid();
        for row in &spec.rows {
            let label = grid
                .get(row.row)
                .and_then(|r| r.get(spec.label_col).copied().flatten())
                .map_or(String::new(), |c| c.text.replace('|', "\\|"));
            let means = match spec.orientation {
                Orientation::MethodsInRows => format!("{} ({})", row.method_id, role(row.role)),
                Orientation::DatasetsInRows => row.dataset.clone(),
            };
            let cells: Vec<String> = spec
                .columns
                .iter()
                .map(|c| {
                    if spec
                        .review_cells
                        .iter()
                        .any(|r| r.row == row.row && r.col.is_none_or(|x| x == c.col))
                    {
                        return "review".to_owned();
                    }
                    expansion
                        .results
                        .iter()
                        .find(|r| r.table == spec.id && r.row == row.row && r.col == c.col)
                        .map_or_else(|| "-".to_owned(), shown)
                })
                .collect();
            println!("| {label} | {means} | {} |", cells.join(" | "));
        }
        println!();
    }
    println!(
        "{} results, {} mapped cells without a value, {} cells for review",
        expansion.results.len(),
        expansion.missing,
        expansion.review
    );
    Ok(true)
}

/// The printed text, plus the stored value when a scale conversion changed it.
fn shown(r: &GoldResult) -> String {
    let text = r.value_text.replace('|', "\\|");
    match parse_number(&r.value_text) {
        Ok(v) if v.value != r.value => format!("{text} (={})", r.value),
        _ => text,
    }
}

fn discover(dir: &Path) -> Result<Vec<Spec>> {
    let mut paths = Vec::new();
    collect_toml(dir, &mut paths).with_context(|| format!("reading {}", dir.display()))?;
    paths.sort();
    Ok(paths
        .into_iter()
        .map(|path| {
            let gold = fs::read_to_string(&path)
                .map_err(|e| format!("{}: {e}", path.display()))
                .and_then(|text| {
                    GoldFile::from_toml_str(&path.display().to_string(), &text)
                        .map_err(|e| e.to_string())
                });
            Spec { path, gold }
        })
        .collect())
}

fn collect_toml(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_toml(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "toml") {
            out.push(path);
        }
    }
    Ok(())
}

fn read_snapshot(path: &Path) -> Result<Vec<GoldResult>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("no snapshot {}", path.display()))?;
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(n, l)| {
            serde_json::from_str(l).with_context(|| format!("{}, line {}", path.display(), n + 1))
        })
        .collect()
}

fn load(id: &ArxivId, cache: &Cache) -> Result<Document> {
    let html = cache.read_html(id)?;
    Ok(arxiv_html::parse(
        &html,
        &id.id,
        id.version.context("unpinned id")?,
    ))
}
