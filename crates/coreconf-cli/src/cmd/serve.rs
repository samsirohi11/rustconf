use clap::Args;

#[derive(Args)]
pub struct ServeArgs {
    #[arg(long)]
    pub bundle: std::path::PathBuf,
    #[arg(long, default_value = "5683")]
    pub port: u16,
}

pub fn run(args: ServeArgs) -> Result<(), String> {
    println!("Serving bundle {} on UDP {}", args.bundle.display(), args.port);
    Ok(())
}
