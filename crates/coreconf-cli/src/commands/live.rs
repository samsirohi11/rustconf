use clap::Args;
use coreconf_runtime::transport::coap_lite::CoapLiteClient;
use std::io::{self, BufRead, Write};

use crate::CliError;
use crate::session::LiveSession;

/// Start an interactive live CORECONF session against a remote CoAP server.
#[derive(Args)]
pub struct LiveArgs {
    /// Path(s) to .sid JSON files describing the YANG model
    #[arg(long, required = true, num_args = 1..)]
    pub sid: Vec<String>,

    /// Server UDP address (e.g., 127.0.0.1:5683)
    #[arg(long, default_value = "127.0.0.1:5683")]
    pub server: String,

    /// CORECONF CoAP resource path
    #[arg(long, default_value = "c")]
    pub path: String,
}

pub fn run(args: LiveArgs) -> Result<(), CliError> {
    let model = crate::load_model(&args.sid)?;

    let client = CoapLiteClient::connect(model.clone(), &args.server, &args.path)
        .map_err(CliError::Model)?;

    eprintln!("Connected to coap://{}/{}", args.server, args.path);

    let mut session = LiveSession::new(model, client)?;

    eprintln!("Commands: get <path>, set <path> <json-value>, delete <path>, push, reload, quit");
    eprintln!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("coreconf-live> ");
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut parts = line.splitn(3, ' ');
        let command = parts.next().unwrap_or_default();

        match command {
            "quit" | "exit" | "q" => break,

            "get" => {
                let path = required(parts.next(), "usage: get <path>")?;
                match session.get(path)? {
                    Some(value) => println!("{}", serde_json::to_string_pretty(&value)?),
                    None => eprintln!("(not found)"),
                }
            }

            "set" => {
                let path = required(parts.next(), "usage: set <path> <json-value>")?;
                let raw_value = required(parts.next(), "usage: set <path> <json-value>")?;
                let value: serde_json::Value = serde_json::from_str(raw_value)?;
                session.set(path, value)?;
                eprintln!("staged");
            }

            "delete" => {
                let path = required(parts.next(), "usage: delete <path>")?;
                session.delete(path)?;
                eprintln!("staged");
            }

            "diff" => {
                let patch = session.pending_patch()?;
                if patch.is_empty() {
                    eprintln!("(no staged changes)");
                } else {
                    for (path, value) in &patch {
                        match value {
                            Some(v) => println!("M {path} {v}"),
                            None => println!("D {path}"),
                        }
                    }
                }
            }

            "push" => {
                let pending = session.pending_patch()?;
                if pending.is_empty() {
                    eprintln!("(no staged changes to push)");
                } else {
                    session.push()?;
                    eprintln!("pushed {} change(s)", pending.len());
                }
            }

            "reload" => {
                session.reload()?;
                eprintln!("reloaded from server");
            }

            _ => eprintln!("unknown command: {command}"),
        }
    }

    Ok(())
}

fn required<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str, CliError> {
    value.ok_or_else(|| CliError::InvalidInput(message.to_string()))
}
