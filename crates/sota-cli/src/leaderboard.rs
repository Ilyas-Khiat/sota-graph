//! `sota leaderboard`: comparable results ranked by code, every value linked
//! to the table cell it was read from.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use sota_core::{
    Direction, GoldResult, Leaderboard, MethodRegistry, Registry, Report, leaderboards,
};

use crate::config::Config;
use crate::graph;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Source {
    /// The knowledge graph (Neo4j)
    Graph,
    /// Extraction outputs on disk (out/extract)
    Files,
    /// The hand-checked gold snapshots (gold/**/*.results.jsonl)
    Gold,
}

pub struct LeaderboardArgs {
    pub dataset: String,
    pub metric: String,
    pub settings: Vec<String>,
    pub source: Source,
    pub from: std::path::PathBuf,
    pub top: Option<usize>,
    pub all_methods: bool,
}

pub fn leaderboard(args: &LeaderboardArgs, config: &Config) -> Result<bool> {
    let LeaderboardArgs {
        dataset,
        metric,
        settings,
        source,
        from,
        top,
        all_methods,
    } = args;
    let (source, top, all_methods) = (*source, *top, *all_methods);
    let registry = &config.registry;
    let d = registry
        .resolve_dataset(dataset)
        .with_context(|| format!("unknown dataset {dataset:?} (see config/registry)"))?;
    let m = registry
        .resolve_metric(metric)
        .with_context(|| format!("unknown metric {metric:?} (see config/registry)"))?;
    let filters = parse_filters(settings, registry)?;

    let reports = match source {
        Source::Files => reports_from_files(from, &config.methods)?,
        Source::Graph => graph::fetch_reports(&d.id, &m.id)?,
        Source::Gold => reports_from_gold(from, &config.methods)?,
    };
    // Only methods whose identity config confirms are ranked; the rest are
    // listed with their sources, so a new method is one alias away.
    let (ranked, unconfirmed): (Vec<Report>, Vec<Report>) = reports
        .into_iter()
        .partition(|r| r.method_known || all_methods);
    let boards = leaderboards(&ranked, registry, &d.id, &m.id, &filters);
    if boards.is_empty() {
        println!(
            "no comparable results for {} {}{}",
            d.name,
            m.name,
            describe(&filters)
        );
    }
    for board in &boards {
        print!("{}", render(board, &d.name, &m.name, top, &config.methods));
    }
    let mut left_out: Vec<&Report> = unconfirmed
        .iter()
        .filter(|r| {
            r.dataset == d.id
                && r.metric == m.id
                && filters.iter().all(|(k, v)| r.settings.get(k) == Some(v))
        })
        .collect();
    left_out.sort_by(|a, b| {
        a.method_name
            .cmp(&b.method_name)
            .then(a.locator.cmp(&b.locator))
    });
    if !left_out.is_empty() {
        println!(
            "Not ranked: {} result(s) whose method matches no known method (add it to config/methods, or pass --all-methods):",
            left_out.len()
        );
        for r in left_out.iter().take(20) {
            println!("- {} {} in {}", r.method_name, r.value, link(&r.locator));
        }
        if left_out.len() > 20 {
            println!("- ... and {} more", left_out.len() - 20);
        }
    }
    Ok(true)
}

fn describe(filters: &BTreeMap<String, String>) -> String {
    if filters.is_empty() {
        String::new()
    } else {
        let f: Vec<String> = filters.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!(" with {}", f.join(", "))
    }
}

/// `protocol=filtered` style arguments, checked against the registry.
fn parse_filters(settings: &[String], registry: &Registry) -> Result<BTreeMap<String, String>> {
    let mut filters = BTreeMap::new();
    for s in settings {
        let Some((k, v)) = s.split_once('=') else {
            bail!("--setting {s:?}: expected name=value, e.g. protocol=filtered");
        };
        let (k, v) = (k.trim(), v.trim());
        if registry.setting(k).is_none() {
            bail!("unknown setting {k:?} (see config/registry)");
        }
        let Some(value) = registry.resolve_setting_value(k, v) else {
            bail!("unknown value {v:?} for setting {k}");
        };
        filters.insert(k.to_owned(), value.id.clone());
    }
    Ok(filters)
}

/// Every verified result in the extraction outputs of `dir`.
pub fn reports_from_files(dir: &Path, methods: &MethodRegistry) -> Result<Vec<Report>> {
    Ok(graph::read_extractions(dir)?
        .iter()
        .flat_map(|x| x.results.iter().map(|r| Report::from_result(r, methods)))
        .collect())
}

/// Every result of the gold snapshots under `dir`, searched recursively.
pub fn reports_from_gold(dir: &Path, methods: &MethodRegistry) -> Result<Vec<Report>> {
    let mut paths = Vec::new();
    collect_snapshots(dir, &mut paths)?;
    paths.sort();
    let mut reports = Vec::new();
    for path in &paths {
        let text = fs::read_to_string(path)?;
        for (i, line) in text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
        {
            let r: GoldResult = serde_json::from_str(line)
                .with_context(|| format!("{}:{}", path.display(), i + 1))?;
            reports.push(Report::from_result(&r, methods));
        }
    }
    Ok(reports)
}

fn collect_snapshots(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_snapshots(&path, out)?;
        } else if path.to_string_lossy().ends_with(".results.jsonl") {
            out.push(path);
        }
    }
    Ok(())
}

/// `https://arxiv.org/html/<paper>#<figure>` from a locator `paper#table/rR/cC`.
fn link(locator: &str) -> String {
    let (paper, rest) = locator.split_once('#').unwrap_or((locator, ""));
    let table = rest.split('/').next().unwrap_or_default();
    // "S4.T3.t2" is the second table of figure "S4.T3".
    let figure = match table.rsplit_once(".t") {
        Some((fig, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => fig,
        _ => table,
    };
    format!("[{paper} {table}](https://arxiv.org/html/{paper}#{figure})")
}

fn render(
    board: &Leaderboard,
    dataset: &str,
    metric: &str,
    top: Option<usize>,
    methods: &MethodRegistry,
) -> String {
    let direction = match board.direction {
        Direction::HigherIsBetter => "higher is better",
        Direction::LowerIsBetter => "lower is better",
    };
    let settings: Vec<String> = board
        .key
        .settings
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    let mut s = format!(
        "## {dataset} · {metric} ({direction})\nsplit {} | {}\n\n| # | method | value | source | other reports | conflict |\n| --- | --- | --- | --- | --- | --- |\n",
        board.key.split,
        settings.join(", ")
    );
    let shown = board.entries.iter().take(top.unwrap_or(usize::MAX));
    let mut notes = Vec::new();
    let mut conflicts = Vec::new();
    for e in shown {
        let name = if e.method_known {
            e.method_name.clone()
        } else {
            format!("{} ?", e.method_name)
        };
        let others: Vec<String> = e
            .others
            .iter()
            .map(|o| format!("{} ({})", o.value, o.paper))
            .collect();
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            e.rank,
            name,
            e.value,
            link(&e.source.locator),
            others.join(", "),
            if e.conflict { "yes" } else { "" }
        ));
        if e.conflict {
            let all: Vec<String> = std::iter::once(&e.source)
                .chain(&e.others)
                .map(|r| format!("{} in {}", r.value_text, link(&r.locator)))
                .collect();
            conflicts.push(format!("- {}: {}", e.method_name, all.join(" vs ")));
        }
        if let Some(m) = methods.method(&e.method_id)
            && !m.notes.is_empty()
        {
            notes.push(format!("- {}: {}", m.name, m.notes));
        }
    }
    if board.entries.iter().any(|e| !e.method_known) {
        notes.push("- `?`: the label matches no known method; identity unconfirmed (add an alias in config/methods).".to_owned());
    }
    if !conflicts.is_empty() {
        s.push_str(&format!(
            "\nConflicting reports (the method's own paper is listed first when it has one):\n{}\n",
            conflicts.join("\n")
        ));
    }
    if !notes.is_empty() {
        s.push_str(&format!("\nNotes:\n{}\n", notes.join("\n")));
    }
    s.push('\n');
    s
}
