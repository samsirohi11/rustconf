use clap::Args;
use std::path::PathBuf;

use crate::CliError;

/// Convert JSON data to CORECONF CBOR (or vice-versa) using SID artifacts.
#[derive(Args)]
pub struct ConvertArgs {
    /// Path(s) to .sid JSON files describing the YANG model
    #[arg(long, required = true, num_args = 1..)]
    pub sid: Vec<String>,

    /// Path to the JSON input file to convert
    #[arg(long)]
    pub input: PathBuf,

    /// Path to write the CBOR output
    #[arg(long)]
    pub output: PathBuf,

    /// Reverse: convert CBOR back to JSON instead
    #[arg(long, default_value_t = false)]
    pub reverse: bool,
}

pub fn run(args: ConvertArgs) -> Result<(), CliError> {
    let model = crate::load_model(&args.sid)?;

    if args.reverse {
        let cbor_data = std::fs::read(&args.input)?;
        let json =
            coreconf_model::decode_cbor_to_json(&model, &cbor_data).map_err(CliError::Model)?;

        let value: serde_json::Value = serde_json::from_str(&json)?;
        let pretty = serde_json::to_string_pretty(&value)?;
        std::fs::write(&args.output, &pretty)?;

        eprintln!(
            "Converted {} → {} ({} bytes CBOR → {} bytes JSON)",
            args.input.display(),
            args.output.display(),
            cbor_data.len(),
            pretty.len()
        );
    } else {
        let json_data = std::fs::read_to_string(&args.input)?;
        let cbor =
            coreconf_model::encode_json_to_cbor(&model, &json_data).map_err(CliError::Model)?;

        std::fs::write(&args.output, &cbor)?;

        eprintln!(
            "Converted {} → {} ({} bytes JSON → {} bytes CBOR)",
            args.input.display(),
            args.output.display(),
            json_data.len(),
            cbor.len()
        );
    }

    Ok(())
}
