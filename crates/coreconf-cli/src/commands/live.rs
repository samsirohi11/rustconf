use clap::Args;
use coreconf_runtime::transport::coap_lite::CoapLiteClient;
use rustyline::error::ReadlineError;
use rustyline::Editor;

use crate::commands::shell::changes_to_text;
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

    let mut session = LiveSession::empty(model, client);

    eprintln!("Commands: discover [d=0], get <path>, set <path> <json-value>, delete <path>, push, reload, quit");
    eprintln!("No startup GET was sent; run `discover d=0` or `reload` when needed.");
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
                    "help" | "?" => {
                        eprintln!("Commands:");
                        eprintln!(
                            "  discover [d=0]            discover server resources without GET /c"
                        );
                        eprintln!(
                            "  get <path>                 read a value from the local working copy"
                        );
                        eprintln!("  set <path> <json-value>    stage a change");
                        eprintln!("  delete <path>              stage a deletion");
                        eprintln!("  diff                       show staged changes");
                        eprintln!("  push                       send staged changes to server");
                        eprintln!("  reload                     fetch fresh snapshot from server");
                        eprintln!("  help | ?                   show this help");
                        eprintln!("  quit | exit | q            disconnect");
                        Ok(())
                    }

                    "quit" | "exit" | "q" => return Ok(()),

                    "discover" => (|| -> Result<(), CliError> {
                        let query = parts.next().unwrap_or("d=0");
                        println!("{}", session.discover(Some(query))?);
                        Ok(())
                    })(),

                    "get" => (|| -> Result<(), CliError> {
                        let path = required(parts.next(), "usage: get <path>")?;
                        match session.get(path)? {
                            Some(value) => {
                                println!("{}", serde_json::to_string_pretty(&value)?)
                            }
                            None => eprintln!(
                                "(not found; run `reload` first if you need a remote snapshot)"
                            ),
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
                        let changes = session.staged_changes()?;
                        if changes.is_empty() {
                            eprintln!("(no staged changes)");
                        } else {
                            for line in changes_to_text(&changes, Some(session.model())) {
                                println!("{line}");
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
