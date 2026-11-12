//! `sota graph`: the knowledge graph in Neo4j (start it with `docker compose up -d`).
//!
//! Connection settings come from the environment, then `./.env`:
//! `NEO4J_HTTP_URL`, `NEO4J_USERNAME`, `NEO4J_PASSWORD`, `NEO4J_DATABASE`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;
use sota_core::{Direction, Report};
use sota_extract::pipeline::PaperExtraction;
use sota_store::Neo4j;

use crate::config::Config;
use crate::extract::{dotenv, lookup};

pub fn connect() -> Neo4j {
    let file = dotenv();
    Neo4j::from_vars(|name| lookup(&file, name))
}

/// Creates the ontology's uniqueness constraints (idempotent).
pub fn init(config: &Config) -> Result<bool> {
    let db = connect();
    for c in sota_store::constraints(&config.ontology) {
        db.run(&c, serde_json::json!({}))?;
        println!("{c}");
    }
    println!("OK");
    Ok(true)
}

/// MERGEs every verified result of the extraction outputs in `from`, then
/// reads each leaderboard slice back and checks it matches the files.
pub fn load(from: &Path, config: &Config) -> Result<bool> {
    let extractions = read_extractions(from)?;
    let mut rows = Vec::new();
    let mut reports = Vec::new();
    for x in &extractions {
        for r in &x.results {
            let report = Report::from_result(r, &config.methods);
            rows.push(row(&report, &x.title, config)?);
            reports.push(report);
        }
    }
    let db = connect();
    let n = sota_store::load(&db, &config.ontology, &rows)?;
    println!("loaded {n} results from {} papers", extractions.len());
    let ok = verify(&db, &reports)?;
    print_stats(&db)?;
    Ok(ok)
}

pub fn stats() -> Result<bool> {
    print_stats(&connect())?;
    Ok(true)
}

pub fn fetch_reports(dataset: &str, metric: &str) -> Result<Vec<Report>> {
    Ok(sota_store::fetch_reports(&connect(), dataset, metric)?)
}

fn print_stats(db: &Neo4j) -> Result<()> {
    for (name, count) in sota_store::stats(db)? {
        println!("{name:<12} {count}");
    }
    Ok(())
}

/// A store row completed with the registry's names and the metric direction.
fn row(report: &Report, title: &str, config: &Config) -> Result<Value> {
    let registry = &config.registry;
    let dataset = registry.dataset(&report.dataset).with_context(|| {
        format!(
            "{}: dataset {} not in the registry",
            report.locator, report.dataset
        )
    })?;
    let metric = registry.metric(&report.metric).with_context(|| {
        format!(
            "{}: metric {} not in the registry",
            report.locator, report.metric
        )
    })?;
    let mut row = sota_store::result_row(report, title);
    let object = row.as_object_mut().expect("result_row returns an object");
    object.insert("dataset_name".into(), dataset.name.clone().into());
    object.insert("metric_name".into(), metric.name.clone().into());
    let direction = match metric.direction {
        Direction::HigherIsBetter => "higher_is_better",
        Direction::LowerIsBetter => "lower_is_better",
    };
    object.insert("direction".into(), direction.into());
    Ok(row)
}

/// Every (dataset, metric) slice read back from the graph must hold the
/// files' results, value for value. Results already in the graph from other
/// runs are allowed; missing or different ones are not.
fn verify(db: &Neo4j, reports: &[Report]) -> Result<bool> {
    let slices: BTreeSet<(&str, &str)> = reports
        .iter()
        .map(|r| (r.dataset.as_str(), r.metric.as_str()))
        .collect();
    let mut problems = Vec::new();
    for (dataset, metric) in &slices {
        let graph: BTreeMap<String, Report> = sota_store::fetch_reports(db, dataset, metric)?
            .into_iter()
            .map(|r| (r.locator.clone(), r))
            .collect();
        for r in reports
            .iter()
            .filter(|r| r.dataset == *dataset && r.metric == *metric)
        {
            match graph.get(&r.locator) {
                None => problems.push(format!("{}: missing from the graph", r.locator)),
                Some(g) if !same(g, r) => {
                    problems.push(format!("{}: graph has {g:?}", r.locator));
                }
                Some(_) => {}
            }
        }
    }
    if problems.is_empty() {
        println!(
            "round trip: {} results in {} dataset/metric slices read back identical",
            reports.len(),
            slices.len()
        );
        return Ok(true);
    }
    println!("round trip: {} problem(s)", problems.len());
    for p in problems.iter().take(20) {
        println!("- {p}");
    }
    Ok(false)
}

/// Equal, except that the value may differ in its last bits after the JSON
/// round trip through the server.
fn same(a: &Report, b: &Report) -> bool {
    let close = (a.value - b.value).abs() <= 1e-12 * a.value.abs().max(1.0);
    close
        && Report {
            value: b.value,
            ..a.clone()
        } == *b
}

pub fn read_extractions(dir: &Path) -> Result<Vec<PaperExtraction>> {
    let entries = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    let mut paths: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
    {
        let text = fs::read_to_string(path)?;
        // Other JSON files (the eval report) are not extractions.
        if let Ok(x) = serde_json::from_str::<PaperExtraction>(&text) {
            out.push(x);
        }
    }
    Ok(out)
}
