use clap::Args;
use coreconf_compiler::{
    compile_paths, emit_bundle_json, emit_sid_json, emit_tree, emit_yang, emit_yin,
};
use std::path::PathBuf;

#[derive(Args)]
pub struct CompileArgs {
    #[arg(required = true)]
    pub input: Vec<PathBuf>,
    #[arg(long)]
    pub bundle_out: PathBuf,
    #[arg(long)]
    pub sid_out: PathBuf,
    #[arg(long)]
    pub tree_out: Option<PathBuf>,
    #[arg(long)]
    pub yang_out: Option<PathBuf>,
    #[arg(long)]
    pub yin_out: Option<PathBuf>,
}

pub fn run(args: CompileArgs) -> Result<(), String> {
    let bundle = compile_paths(&args.input).map_err(|err| err.to_string())?;
    write_output(
        &args.bundle_out,
        emit_bundle_json(&bundle).map_err(|err| err.to_string())?,
    )?;
    write_output(
        &args.sid_out,
        emit_sid_json(&bundle).map_err(|err| err.to_string())?,
    )?;

    if let Some(path) = &args.tree_out {
        write_output(path, emit_tree(&bundle))?;
    }
    if let Some(path) = &args.yang_out {
        write_output(path, emit_yang(&bundle))?;
    }
    if let Some(path) = &args.yin_out {
        write_output(path, emit_yin(&bundle))?;
    }

    Ok(())
}

fn write_output(path: &std::path::Path, contents: String) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(path, contents).map_err(|err| err.to_string())
}
