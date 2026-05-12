use clap::Args;
use coreconf_runtime::transport::coap_lite::CoapLiteClient;
use rustyline::error::ReadlineError;
use rustyline::Editor;

use crate::complete::CoreconfCompleter;
use crate::session::LiveSession;
use crate::CliError;

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
    let completer_model = model.clone();

    let client = CoapLiteClient::connect(model.clone(), &args.server, &args.path)
        .map_err(CliError::Model)?;

    eprintln!("Connected to coap://{}/{}", args.server, args.path);

    let mut session = LiveSession::new(model, client)?;

    eprintln!("Commands: get <path>, set <path> <json-value>, delete <path>, push, reload, quit");
    eprintln!("Tab-complete: commands and model paths");
    eprintln!();

    let completer = CoreconfCompleter {
        model: completer_model,
    };
    let mut rl = Editor::with_config(
        rustyline::Config::builder()
            .completion_type(rustyline::CompletionType::List)
            .build(),
    )
    .map_err(|e| CliError::Io(std::io::Error::other(e)))?;
    rl.set_helper(Some(completer));

    loop {
        match rl.readline("coreconf-live> ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(line);

                let mut parts = line.splitn(3, ' ');
                let command = parts.next().unwrap_or_default();

                let result = match command {
                    "quit" | "exit" | "q" => return Ok(()),

                    "get" => (|| -> Result<(), CliError> {
                        let path = required(parts.next(), "usage: get <path>")?;
                        match session.get(path)? {
                            Some(value) => {
                                println!("{}", serde_json::to_string_pretty(&value)?)
                            }
                            None => eprintln!("(not found)"),
                        }
                        Ok(())
                    })(),

                    "set" => (|| -> Result<(), CliError> {
                        let path = required(parts.next(), "usage: set <path> <json-value>")?;
                        let raw_value = required(parts.next(), "usage: set <path> <json-value>")?;
                        let value: serde_json::Value = serde_json::from_str(raw_value)?;
                        session.set(path, value)?;
                        eprintln!("staged");
                        Ok(())
                    })(),

                    "delete" => (|| -> Result<(), CliError> {
                        let path = required(parts.next(), "usage: delete <path>")?;
                        session.delete(path)?;
                        eprintln!("staged");
                        Ok(())
                    })(),

                    "diff" => (|| -> Result<(), CliError> {
                        let patch = session.pending_patch()?;
                        if patch.is_empty() {
                            eprintln!("(no staged changes)");
                        } else {
                            for (path, value) in &patch {
                                match value {
                                    Some(v) => println!(
                                        "{} {path} {}",
                                        style_live_yellow("M"),
                                        style_live_yellow(&compact_json(v))
                                    ),
                                    None => println!("{} {path}", style_live_red("D")),
                                }
                            }
                        }
                        Ok(())
                    })(),

                    "push" => (|| -> Result<(), CliError> {
                        let pending = session.pending_patch()?;
                        if pending.is_empty() {
                            eprintln!("(no staged changes to push)");
                        } else {
                            session.push()?;
                            eprintln!("pushed {} change(s)", pending.len());
                        }
                        Ok(())
                    })(),

                    "reload" => (|| -> Result<(), CliError> {
                        session.reload()?;
                        eprintln!("reloaded from server");
                        Ok(())
                    })(),

                    _ => {
                        eprintln!("unknown command: {command}");
                        Ok(())
                    }
                };

                if let Err(error) = result {
                    eprintln!("error: {error}");
                }
            }
            Err(ReadlineError::Interrupted) => {
                eprintln!("^C");
                break;
            }
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                return Err(CliError::Io(std::io::Error::other(e)));
            }
        }
    }

    Ok(())
}

fn required<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str, CliError> {
    value.ok_or_else(|| CliError::InvalidInput(message.to_string()))
}

fn style_live_red(text: &str) -> String {
    format!("\x1b[31m{text}\x1b[0m")
}

fn style_live_yellow(text: &str) -> String {
    format!("\x1b[33m{text}\x1b[0m")
}

fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}
