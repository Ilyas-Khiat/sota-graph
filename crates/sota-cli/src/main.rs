use clap::Parser;

/// Who beats whom, on which benchmark, by how much, with a source for every number.
#[derive(Debug, Parser)]
#[command(name = "sota", version, arg_required_else_help = true)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
