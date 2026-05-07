use clap::Args;
use coreconf_runtime::EditableFormat;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use crate::session::{FileSession, SaveOptions, Session, StagedChange};
use crate::CliError;

/// Start an interactive CORECONF shell with a local file-backed datastore.
#[derive(Args)]
pub struct ShellArgs {
    /// Path(s) to .sid JSON files describing the YANG model
    #[arg(long, required = true, num_args = 1..)]
    pub sid: Vec<String>,

    /// Editable datastore file to open as the source of truth
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// Explicit editable file format: json or cbor
    #[arg(long)]
    pub format: Option<String>,

    /// Legacy alias for pre-loading JSON into an in-memory session
    #[arg(long)]
    pub input: Option<PathBuf>,

    /// Disable the default backup on the first save
    #[arg(long, default_value_t = false)]
    pub no_backup: bool,
}

pub fn run(args: ShellArgs) -> Result<(), CliError> {
    let model = crate::load_model(&args.sid)?;
    let backup_by_default = !args.no_backup;

    let mut session = if let Some(file_path) = &args.file {
        let format = resolve_format(file_path, args.format.as_deref())?;
        ShellSession::File(Box::new(FileSession::open(model, file_path, format)?))
    } else if let Some(input_path) = &args.input {
        let json_data = std::fs::read_to_string(input_path)?;
        ShellSession::Memory(Box::new(Session::with_json(model, &json_data)?))
    } else {
        ShellSession::Memory(Box::new(Session::new(model)))
    };

    eprintln!("CORECONF interactive shell");
    eprintln!(
        "Commands: get <path>, set <path> <json-value>, delete <path>, dump, diff, save, reload, quit"
    );
    eprintln!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("coreconf> ");
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            if session.is_dirty()? {
                return Err(invalid_input(
                    "unsaved staged edits; use save, reload, or quit --discard",
                ));
            }
            eprintln!();
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match dispatch_command(&mut session, line, backup_by_default)? {
            ShellAction::Continue => {}
            ShellAction::Quit => break,
        }
    }

    Ok(())
}

enum ShellSession {
    Memory(Box<Session>),
    File(Box<FileSession>),
}

impl ShellSession {
    fn is_dirty(&self) -> Result<bool, CliError> {
        match self {
            Self::Memory(_) => Ok(false),
            Self::File(session) => session.is_dirty(),
        }
    }
}

enum ShellAction {
    Continue,
    Quit,
}

fn dispatch_command(
    session: &mut ShellSession,
    line: &str,
    backup_by_default: bool,
) -> Result<ShellAction, CliError> {
    let mut parts = line.splitn(3, ' ');
    let verb = parts.next().unwrap_or("");
    let path = parts.next();
    let rest = parts.next();

    match verb {
        "quit" | "exit" | "q" => {
            if matches!(path, Some("--discard")) {
                return Ok(ShellAction::Quit);
            }
            if session.is_dirty()? {
                return Err(invalid_input(
                    "unsaved staged edits; use save, reload, or quit --discard",
                ));
            }
            Ok(ShellAction::Quit)
        }

        "get" => {
            let path = required(path, "usage: get <path>")?;
            match session_get(session, path)? {
                Some(value) => println!("{}", serde_json::to_string_pretty(&value)?),
                None => eprintln!("(not found)"),
            }
            Ok(ShellAction::Continue)
        }

        "set" => {
            let path = required(path, "usage: set <path> <json-value>")?;
            let raw_value = required(rest, "usage: set <path> <json-value>")?;
            let value: serde_json::Value = serde_json::from_str(raw_value)?;
            match session {
                ShellSession::Memory(session) => session.set(path, value)?,
                ShellSession::File(session) => session.set(path, value)?,
            }
            eprintln!("staged");
            Ok(ShellAction::Continue)
        }

        "delete" => {
            let path = required(path, "usage: delete <path>")?;
            let deleted = match session {
                ShellSession::Memory(session) => session.delete(path)?,
                ShellSession::File(session) => session.delete(path)?,
            };
            if deleted {
                eprintln!("staged");
            } else {
                eprintln!("(not found)");
            }
            Ok(ShellAction::Continue)
        }

        "dump" => {
            let tree = match session {
                ShellSession::Memory(session) => session.dump(),
                ShellSession::File(session) => session.dump(),
            };
            println!("{}", serde_json::to_string_pretty(&tree)?);
            Ok(ShellAction::Continue)
        }

        "diff" => {
            let json = matches!(path, Some("--json"));
            let ShellSession::File(session) = session else {
                eprintln!("(no staged file edits)");
                return Ok(ShellAction::Continue);
            };
            let changes = session.staged_changes()?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&changes_to_json(&changes))?
                );
            } else if changes.is_empty() {
                eprintln!("(no staged file edits)");
            } else {
                for line in changes_to_text(&changes) {
                    println!("{line}");
                }
            }
            Ok(ShellAction::Continue)
        }

        "save" => {
            let ShellSession::File(session) = session else {
                return Err(invalid_input("save requires --file"));
            };
            let force = matches!(path, Some("--force"));
            session.save(SaveOptions {
                create_backup: backup_by_default,
                force,
            })?;
            eprintln!("saved {}", session.path().display());
            Ok(ShellAction::Continue)
        }

        "reload" => {
            let ShellSession::File(session) = session else {
                return Err(invalid_input("reload requires --file"));
            };
            session.reload()?;
            eprintln!("reloaded {}", session.path().display());
            Ok(ShellAction::Continue)
        }

        _ => Err(invalid_input(
            "unknown command; expected get, set, delete, dump, diff, save, reload, or quit",
        )),
    }
}

fn session_get(session: &ShellSession, path: &str) -> Result<Option<serde_json::Value>, CliError> {
    match session {
        ShellSession::Memory(session) => session.get(path),
        ShellSession::File(session) => session.get(path),
    }
}

fn resolve_format(
    path: &std::path::Path,
    explicit: Option<&str>,
) -> Result<EditableFormat, CliError> {
    if let Some(format) = explicit {
        return EditableFormat::parse(format)
            .ok_or_else(|| invalid_input("format must be json or cbor"));
    }
    EditableFormat::from_path(path).ok_or_else(|| {
        invalid_input("could not infer editable file format; pass --format json or --format cbor")
    })
}

fn changes_to_text(changes: &[StagedChange]) -> Vec<String> {
    changes
        .iter()
        .map(|change| match (&change.before, &change.after) {
            (None, Some(after)) => format!("A {} {}", change.path, compact_json(after)),
            (Some(before), None) => format!("D {} {}", change.path, compact_json(before)),
            (Some(before), Some(after)) => {
                format!(
                    "M {} {} -> {}",
                    change.path,
                    compact_json(before),
                    compact_json(after)
                )
            }
            (None, None) => format!("D {}", change.path),
        })
        .collect()
}

fn changes_to_json(changes: &[StagedChange]) -> serde_json::Value {
    serde_json::Value::Array(
        changes
            .iter()
            .map(|change| match (&change.before, &change.after) {
                (None, Some(after)) => {
                    serde_json::json!({"op":"add","path":change.path,"value":after})
                }
                (Some(_), Some(after)) => {
                    serde_json::json!({"op":"replace","path":change.path,"value":after})
                }
                (Some(_), None) | (None, None) => {
                    serde_json::json!({"op":"remove","path":change.path})
                }
            })
            .collect(),
    )
}

fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn required<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str, CliError> {
    value.ok_or_else(|| invalid_input(message))
}

fn invalid_input(message: &str) -> CliError {
    CliError::InvalidInput(message.to_string())
}
