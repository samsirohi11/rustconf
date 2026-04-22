use clap::Args;
use coreconf_compiler::{compile_paths, emit_bundle_json, emit_sid_json};
use std::path::PathBuf;

#[derive(Args)]
pub struct CompileArgs {
    pub input: PathBuf,
    #[arg(long)]
    pub bundle_out: PathBuf,
    #[arg(long)]
    pub sid_out: PathBuf,
}

pub fn run(args: CompileArgs) -> Result<(), String> {
    let bundle = compile_paths(&[args.input]).map_err(|err| err.to_string())?;
    if let Some(parent) = args.bundle_out.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    if let Some(parent) = args.sid_out.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(
        &args.bundle_out,
        emit_bundle_json(&bundle).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    std::fs::write(
        &args.sid_out,
        emit_sid_json(&bundle).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}
