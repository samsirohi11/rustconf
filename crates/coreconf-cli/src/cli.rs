use clap::{Parser, Subcommand};

use coreconf_cli::CliError;
use coreconf_cli::commands;

#[derive(Parser)]
#[command(
    name = "coreconf-cli",
    about = "CORECONF operator CLI — convert, validate, shell, live, and serve",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Convert JSON data to CORECONF CBOR using SID artifacts
    Convert(commands::convert::ConvertArgs),

    /// Validate a SID file or data against a model
    Validate(commands::validate::ValidateArgs),

    /// Start an interactive CORECONF shell with a local datastore
    Shell(commands::shell::ShellArgs),

    /// Start an interactive live CORECONF session against a remote CoAP server
    Live(commands::live::LiveArgs),

    /// Start a CORECONF CoAP server backed by a local datastore
    Serve(commands::serve::ServeArgs),
}

pub fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Commands::Convert(args) => commands::convert::run(args),
        Commands::Validate(args) => commands::validate::run(args),
        Commands::Shell(args) => commands::shell::run(args),
        Commands::Live(args) => commands::live::run(args),
        Commands::Serve(args) => commands::serve::run(args),
    }
}
