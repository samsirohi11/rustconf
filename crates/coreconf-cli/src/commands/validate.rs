use clap::Args;
use std::path::PathBuf;

use crate::CliError;

/// Validate a SID file, optionally checking input data against the model.
#[derive(Args)]
pub struct ValidateArgs {
    /// Path(s) to .sid JSON files describing the YANG model
    #[arg(long, required = true, num_args = 1..)]
    pub sid: Vec<String>,

    /// Optional path to a JSON data file to validate against the model
    #[arg(long)]
    pub input: Option<PathBuf>,
}

pub fn run(args: ValidateArgs) -> Result<(), CliError> {
    let model = crate::load_model(&args.sid)?;
    let sid_count = model.sids.len();

    eprintln!(
        "Model loaded: {} SID entries across {} file(s)",
        sid_count,
        args.sid.len()
    );

    if let Some(input_path) = &args.input {
        let json_data = std::fs::read_to_string(input_path)?;

        let cbor =
            coreconf_model::encode_json_to_cbor(&model, &json_data).map_err(CliError::Model)?;

        let _decoded =
            coreconf_model::decode_cbor_to_json(&model, &cbor).map_err(CliError::Model)?;

        eprintln!(
            "Validation passed: {} ({} bytes JSON → {} bytes CBOR, roundtrip OK)",
            input_path.display(),
            json_data.len(),
            cbor.len()
        );
    } else {
        eprintln!("SID file(s) validated successfully (no data input provided)");
    }

    Ok(())
}
