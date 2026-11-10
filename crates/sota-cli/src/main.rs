use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

mod config;

/// Who beats whom, on which benchmark, by how much, with a source for every number
#[derive(Debug, Parser)]
#[command(name = "sota", version, arg_required_else_help = true)]
struct Cli {
    /// Directory holding ontology.toml, normalization.toml and registry/*.toml
    #[arg(long, global = true, default_value = "config")]
    config: PathBuf,

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
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Kind {
    Dataset,
    Metric,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let config = config::load(&cli.config)?;
    match cli.command {
        Command::CheckConfig => {
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
    }
    Ok(())
}
