use clap::Args;

#[derive(Args)]
pub struct InspectArgs {
    #[arg(long)]
    pub bundle: std::path::PathBuf,
}

pub fn run(args: InspectArgs) -> Result<(), String> {
    let content = std::fs::read_to_string(&args.bundle).map_err(|err| err.to_string())?;
    println!("{}", content);
    Ok(())
}
