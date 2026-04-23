use clap::Args;
use coreconf_schema::CompiledSchemaBundle;
use coreconf_server::{CoreconfServer, MemoryAuthorizer, NoopAuditSink, SqliteStore, Store};

#[derive(Args)]
pub struct ServeArgs {
    #[arg(long)]
    pub bundle: std::path::PathBuf,
    #[arg(long)]
    pub db: std::path::PathBuf,
    #[arg(long)]
    pub seed_json: Option<String>,
    #[arg(long, default_value = "5683")]
    pub port: u16,
}

pub fn run(args: ServeArgs) -> Result<(), String> {
    let bundle_json = std::fs::read_to_string(&args.bundle).map_err(|err| err.to_string())?;
    let bundle: CompiledSchemaBundle =
        serde_json::from_str(&bundle_json).map_err(|err| err.to_string())?;
    let schema_version = schema_version(&bundle);

    let mut store = SqliteStore::open(&args.db)?;
    store.write_bundle(&schema_version, &bundle)?;
    if let Some(seed_json) = &args.seed_json {
        let snapshot = serde_json::from_str(seed_json).map_err(|err| err.to_string())?;
        store.write_snapshot(&schema_version, &snapshot)?;
    }
    store.set_active_schema_version(&schema_version)?;

    let _server = CoreconfServer::from_bundle(
        bundle,
        store,
        MemoryAuthorizer::default(),
        NoopAuditSink::default(),
    )?;

    println!(
        "Serving schema from {} with db {} on UDP {}",
        args.bundle.display(),
        args.db.display(),
        args.port
    );
    Ok(())
}

fn schema_version(bundle: &CompiledSchemaBundle) -> String {
    let module = bundle.modules.first();
    format!(
        "{}@{}",
        module.map(|entry| entry.name.as_str()).unwrap_or("unknown"),
        module.map(|entry| entry.revision.as_str()).unwrap_or("unknown")
    )
}
