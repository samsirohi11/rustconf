use clap::Parser;

mod cli;

fn main() {
    let args = cli::Cli::parse();

    if let Err(error) = cli::run(args) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
