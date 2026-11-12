//! `sota extract` and `sota eval extract`.
//!
//! Extraction replays recorded answers by default and never calls a model;
//! `--live` calls Azure OpenAI and records every new answer, so the next run
//! (and CI) replays it for free.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::json;
use sota_core::{ArxivId, GoldResult, MethodRegistry, Registry, Score, score_extraction};
use sota_extract::llm::{AzureOpenAi, LlmClient, Recorded, Usage};
use sota_extract::pipeline::{PaperExtraction, extract_paper};
use sota_extract::prompt::request_for;
use sota_ingest::cache::Cache;

use crate::{gold, papers};

pub struct ExtractArgs {
    pub paper: Option<String>,
    pub list: Option<PathBuf>,
    pub table: Option<String>,
    pub live: bool,
    pub dry_run: bool,
    pub model: Option<String>,
    pub recordings: PathBuf,
    pub out: PathBuf,
}

/// `KEY=VALUE` lines of `./.env` (this directory only, never its parents).
pub fn dotenv() -> HashMap<String, String> {
    fs::read_to_string(".env")
        .map(|text| parse_dotenv(&text))
        .unwrap_or_default()
}

/// Comments and blank lines skipped; one pair of matching quotes, single or
/// double, removed from a value (`KEY='abc'` and `KEY="abc"` both give abc).
fn parse_dotenv(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            let v = v.trim();
            let unquoted = ['"', '\'']
                .iter()
                .find_map(|q| v.strip_prefix(*q).and_then(|s| s.strip_suffix(*q)))
                .unwrap_or(v);
            Some((k.trim().to_owned(), unquoted.to_owned()))
        })
        .collect()
}

/// The process environment first, then `./.env`.
pub fn lookup(file: &HashMap<String, String>, name: &str) -> Option<String> {
    std::env::var(name).ok().or_else(|| file.get(name).cloned())
}

pub fn extract(args: ExtractArgs, cache: &Cache, registry: &Registry) -> Result<bool> {
    let ids: Vec<ArxivId> = match (&args.paper, &args.list) {
        (Some(p), _) => vec![p.parse()?],
        (None, Some(list)) => papers::read_list(list)?,
        (None, None) => bail!("give --paper <id> or --list <file>"),
    };
    for id in &ids {
        if id.version.is_none() {
            bail!("{id} has no version; pin one (e.g. {id}v1)");
        }
        if !cache.contains(id) {
            bail!("{id} is not in the cache; run `sota fetch` first");
        }
    }
    if args.dry_run {
        return dry_run(&ids, &args, cache, registry);
    }
    let env = dotenv();
    if args.live {
        let client = AzureOpenAi::from_vars(|n| lookup(&env, n)).map_err(anyhow::Error::msg)?;
        let mut client = Recorded::live(client, &args.recordings);
        run(&ids, &args, cache, registry, &mut client)
    } else {
        let model = match &args.model {
            Some(m) => m.clone(),
            None => {
                let deployment = lookup(&env, "AZURE_OPENAI_DEPLOYMENT")
                    .context("no model: pass --model or set AZURE_OPENAI_DEPLOYMENT")?;
                let effort =
                    lookup(&env, "SOTA_REASONING_EFFORT").unwrap_or_else(|| "medium".into());
                format!("{deployment}/{effort}")
            }
        };
        let mut client: Recorded<AzureOpenAi> = Recorded::replay_only(&args.recordings, &model);
        run(&ids, &args, cache, registry, &mut client)
    }
}

fn run<C: LlmClient>(
    ids: &[ArxivId],
    args: &ExtractArgs,
    cache: &Cache,
    registry: &Registry,
    client: &mut Recorded<C>,
) -> Result<bool> {
    fs::create_dir_all(&args.out).with_context(|| format!("creating {}", args.out.display()))?;
    let mut ok = true;
    let mut usage = Usage::default();
    let (mut results, mut review) = (0, 0);
    for id in ids {
        let doc = gold::load(id, cache)?;
        let x = extract_paper(&doc, registry, client, args.table.as_deref());
        println!("{} ({})", x.paper, x.model);
        for t in &x.tables {
            match (&t.error, &t.mapping) {
                (Some(e), _) => {
                    ok = false;
                    println!("  {:<18} FAILED: {e}", t.table);
                }
                (None, Some(m)) if !m.is_results_table => {
                    println!("  {:<18} not a results table", t.table)
                }
                _ => println!(
                    "  {:<18} {} results, {} for review, {} out of registry",
                    t.table,
                    t.results,
                    t.review,
                    t.out_of_registry.len()
                ),
            }
        }
        let path = args.out.join(format!("{}.json", id.file_stem()));
        fs::write(&path, serde_json::to_string_pretty(&x)? + "\n")
            .with_context(|| format!("writing {}", path.display()))?;
        println!(
            "  -> {}: {} results, {} for review",
            path.display(),
            x.results.len(),
            x.review.len()
        );
        usage.add(&x.usage);
        results += x.results.len();
        review += x.review.len();
        if let Some(why) = &x.aborted {
            println!("  run {why}");
            ok = false;
            break;
        }
    }
    println!(
        "{} paper(s): {results} results, {review} for review | tokens in {} out {} (reasoning {}) | {} calls, {} replayed",
        ids.len(),
        usage.input_tokens,
        usage.output_tokens,
        usage.reasoning_tokens,
        client.calls,
        client.replayed
    );
    if !ok && client.calls == 0 && !args.live {
        println!(
            "Some answers are not recorded. Run with --live to call the model (it costs money)."
        );
    }
    Ok(ok)
}

/// What a run would send, without sending it: tables and an input-token
/// estimate (about 4 characters per token), so a paid run is approved with
/// numbers in hand. Output tokens depend on the model and are not estimated.
fn dry_run(
    ids: &[ArxivId],
    args: &ExtractArgs,
    cache: &Cache,
    registry: &Registry,
) -> Result<bool> {
    let (mut tables, mut chars, mut system) = (0usize, 0usize, 0usize);
    for id in ids {
        let doc = gold::load(id, cache)?;
        let mut paper_chars = 0;
        let mut paper_tables = 0;
        for table in doc
            .tables
            .iter()
            .filter(|t| args.table.as_deref().is_none_or(|o| o == t.id))
        {
            let req = request_for(&doc, table, registry);
            system = req.system.len() + req.schema.to_string().len();
            paper_chars += system + req.user.len();
            paper_tables += 1;
        }
        println!(
            "{id:<14} {paper_tables:>3} tables, ~{} input tokens",
            paper_chars / 4
        );
        tables += paper_tables;
        chars += paper_chars;
    }
    println!(
        "total: {tables} tables, ~{} input tokens (~{} of them the same instructions and schema on every call)",
        chars / 4,
        tables * system / 4
    );
    Ok(true)
}

pub fn eval_extract(dir: &Path, out: &Path, methods: &MethodRegistry) -> Result<bool> {
    let mut total = Score::default();
    let mut identity = Identity::default();
    let mut per_paper = serde_json::Map::new();
    let mut scored = 0;
    for spec in gold::discover(dir)? {
        let Ok(g) = &spec.gold else {
            continue;
        };
        let snapshot = gold::read_snapshot(&spec.snapshot_path())?;
        let id: ArxivId = g.paper.parse()?;
        let path = out.join(format!("{}.json", id.file_stem()));
        let Ok(text) = fs::read_to_string(&path) else {
            println!("{:<14} not extracted ({})", g.paper, path.display());
            continue;
        };
        let x: PaperExtraction =
            serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
        let s = score_extraction(g, &snapshot, &x.results, &x.review);
        identity.add(methods, &snapshot, &x.results);
        println!("{:<14} {}", g.paper, line(&s));
        per_paper.insert(g.paper.clone(), serde_json::to_value(&s)?);
        total.add(&s);
        scored += 1;
    }
    if scored == 0 {
        println!("nothing to score: run `sota extract` first");
        return Ok(false);
    }
    println!("{:<14} {}", "total", line(&total));
    if !total.mismatches.is_empty() {
        println!("\nfirst mismatches ({} in all):", total.mismatches.len());
        for m in total.mismatches.iter().take(15) {
            println!("  {m}");
        }
    }
    println!(
        "
method identity: {}/{} gold cells resolve to the gold method id",
        identity.agree, identity.total
    );
    for m in identity.wrong.iter().take(10) {
        println!("  {m}");
    }
    let mut unconfirmed: Vec<&String> = identity.unconfirmed.iter().collect();
    unconfirmed.sort();
    println!(
        "{} extracted label(s) match no known method (new, or need an alias): {}",
        unconfirmed.len(),
        unconfirmed
            .iter()
            .take(12)
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let report = json!({
        "total": total,
        "papers": per_paper,
        "method_identity": { "agree": identity.agree, "total": identity.total, "unconfirmed": unconfirmed },
    });
    let path = out.join("eval.json");
    fs::write(&path, serde_json::to_string_pretty(&report)? + "\n")
        .with_context(|| format!("writing {}", path.display()))?;
    println!("\nreport: {}", path.display());
    Ok(true)
}

/// Whether extracted labels resolve to the methods the gold set names.
#[derive(Default)]
struct Identity {
    agree: usize,
    total: usize,
    wrong: Vec<String>,
    unconfirmed: std::collections::BTreeSet<String>,
}

impl Identity {
    fn add(&mut self, methods: &MethodRegistry, gold: &[GoldResult], extracted: &[GoldResult]) {
        let gold: HashMap<(&str, usize, usize), &GoldResult> = gold
            .iter()
            .map(|g| ((g.table.as_str(), g.row, g.col), g))
            .collect();
        for e in extracted {
            let resolved = methods.resolve(&e.method);
            if !resolved.known {
                self.unconfirmed.insert(e.method.clone());
            }
            if let Some(g) = gold.get(&(e.table.as_str(), e.row, e.col)) {
                self.total += 1;
                if resolved.id == g.method_id {
                    self.agree += 1;
                } else {
                    self.wrong.push(format!(
                        "{}: {:?} resolves to {}, gold {}",
                        e.locator, e.method, resolved.id, g.method_id
                    ));
                }
            }
        }
    }
}

fn line(s: &Score) -> String {
    format!(
        "tp {} fp {} fn {} | precision {:.3} recall {:.3} | held back {}/{} caught, {} leaked | {} good sent to review | {} on negative tables | {} unscored (tables outside the gold set)",
        s.tp,
        s.fp,
        s.fn_,
        s.precision(),
        s.recall(),
        s.held_back_caught,
        s.held_back,
        s.held_back_leaked,
        s.wrongly_reviewed,
        s.negative_table_results,
        s.unscored
    )
}

#[cfg(test)]
mod tests {
    use super::parse_dotenv;

    #[test]
    fn dotenv_values_lose_one_pair_of_matching_quotes() {
        let env = parse_dotenv(
            "# comment
A='single'
B=\"double\"
C=bare
D = ' spaced '
E='mismatched\"

F=it's
",
        );
        assert_eq!(env["A"], "single");
        assert_eq!(env["B"], "double");
        assert_eq!(env["C"], "bare");
        assert_eq!(env["D"], " spaced ");
        assert_eq!(env["E"], "'mismatched\"");
        assert_eq!(env["F"], "it's");
        assert_eq!(env.len(), 6);
    }
}
