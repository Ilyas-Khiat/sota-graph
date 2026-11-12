use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use sota_ingest::cache::Cache;

mod config;
mod gold;
mod papers;

/// Who beats whom, on which benchmark, by how much, with a source for every number
#[derive(Debug, Parser)]
#[command(name = "sota", version, arg_required_else_help = true)]
struct Cli {
    /// Directory holding ontology.toml, normalization.toml and registry/*.toml
    #[arg(long, global = true, default_value = "config")]
    config: PathBuf,

    /// Directory for downloaded pages (git-ignored)
    #[arg(long, global = true, default_value = "cache")]
    cache: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Load and validate the ontology and the registry, then print a summary
    CheckConfig,
    /// Show which registry entry a mention resolves to, if any
    Resolve {
        kind: Kind,
        /// The text as written in a paper, e.g. "FB15K 237" or "H@10"
        mention: String,
    },
    /// Download the arXiv HTML of every paper in a list (pinned versions, 15 s apart)
    Fetch {
        /// File with one pinned arXiv id per line, e.g. papers/kgc.txt
        #[arg(long)]
        list: PathBuf,
    },
    /// Print the tables of a cached paper as Markdown, with links to the source
    Tables {
        /// Pinned arXiv id, e.g. 1902.10197v1
        id: String,
        /// Only this table, e.g. S4.T5
        #[arg(long)]
        table: Option<String>,
    },
    /// Gold set: hand-checked meaning of results tables
    Gold {
        /// Directory of gold specs (<topic>/<paper>.toml), searched recursively
        #[arg(long, global = true, default_value = "gold")]
        dir: PathBuf,
        #[command(subcommand)]
        command: GoldCommand,
    },
}

#[derive(Debug, Subcommand)]
enum GoldCommand {
    /// Validate every spec and its snapshot; re-expand from the cache when the paper is there
    Check,
    /// Expand specs against cached papers and write their snapshots
    Expand {
        /// Only this paper, e.g. 1902.10197v1
        #[arg(long)]
        paper: Option<String>,
    },
    /// Print a paper's gold mapping with its values, for review against the source
    Show {
        /// Pinned arXiv id
        paper: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Kind {
    Dataset,
    Metric,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// `Ok(false)`: the command ran but something it reports failed.
fn run(cli: Cli) -> anyhow::Result<bool> {
    let cache = Cache::new(cli.cache.join("arxiv"));
    match cli.command {
        Command::CheckConfig => {
            let config = config::load(&cli.config)?;
            let (o, r) = (&config.ontology, &config.registry);
            println!(
                "ontology v{}: {} node types, {} relationship types",
                o.version,
                o.nodes.len(),
                o.relationships.len()
            );
            println!(
                "registry ({}): {} datasets, {} metrics, {} settings",
                r.topics().join(", "),
                r.datasets().len(),
                r.metrics().len(),
                r.settings().len()
            );
            println!("OK");
        }
        Command::Resolve { kind, mention } => {
            let config = config::load(&cli.config)?;
            let r = &config.registry;
            let hit = match kind {
                Kind::Dataset => r.resolve_dataset(&mention).map(|d| (&d.id, &d.name)),
                Kind::Metric => r.resolve_metric(&mention).map(|m| (&m.id, &m.name)),
            };
            match hit {
                Some((id, name)) => println!("{id}\t{name}"),
                None => println!(
                    "unknown\tkey {:?} matches no entry (would go to review)",
                    r.normalizer().key(&mention)
                ),
            }
        }
        Command::Fetch { list } => return papers::fetch(&list, &cache),
        Command::Tables { id, table } => papers::tables(&id, table.as_deref(), &cache)?,
        Command::Gold { dir, command } => {
            let registry = config::load(&cli.config)?.registry;
            return match command {
                GoldCommand::Check => gold::check(&dir, &cache, &registry),
                GoldCommand::Expand { paper } => {
                    gold::expand(&dir, paper.as_deref(), &cache, &registry)
                }
                GoldCommand::Show { paper } => gold::show(&paper, &dir, &cache, &registry),
            };
        }
    }
    Ok(true)
}
