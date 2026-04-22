mod cmd;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "coreconf")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Compile(cmd::compile::CompileArgs),
    Inspect(cmd::inspect::InspectArgs),
    Serve(cmd::serve::ServeArgs),
}

fn main() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Compile(args) => cmd::compile::run(args),
        Commands::Inspect(args) => cmd::inspect::run(args),
        Commands::Serve(args) => cmd::serve::run(args),
    }
}
