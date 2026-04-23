use clap::Args;
use coreconf_schema::CompiledSchemaBundle;
use coreconf_server::{SqliteStore, Store};

#[derive(Args)]
pub struct InspectArgs {
    #[arg(long)]
    pub bundle: std::path::PathBuf,
    #[arg(long)]
    pub db: Option<std::path::PathBuf>,
}

pub fn run(args: InspectArgs) -> Result<(), String> {
    let content = std::fs::read_to_string(&args.bundle).map_err(|err| err.to_string())?;
    let bundle: CompiledSchemaBundle =
        serde_json::from_str(&content).map_err(|err| err.to_string())?;

    println!("modules: {}", bundle.modules.len());
    println!("nodes: {}", bundle.nodes.len());
    println!("operations: {}", bundle.operations.len());

    if let Some(db) = args.db {
        let store = SqliteStore::open(db)?;
        println!(
            "active schema version: {}",
            store.active_schema_version()?.unwrap_or_else(|| "none".into())
        );
        println!("audit events: {}", store.read_audit()?.len());
    }

    Ok(())
}
