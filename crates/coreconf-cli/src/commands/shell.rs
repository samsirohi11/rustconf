use clap::Args;
use coreconf_model::CompositeModel;
use coreconf_runtime::EditableFormat;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::complete::CoreconfCompleter;
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
    let completer_model = model.clone();
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
        match rl.readline("coreconf> ") {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(line);

                match dispatch_command(&mut session, line, backup_by_default) {
                    Ok(ShellAction::Continue) => {}
                    Ok(ShellAction::Quit) => break,
                    Err(error) => eprintln!("error: {error}"),
                }
            }
            Err(ReadlineError::Interrupted) => {
                eprintln!("^C");
                if session.is_dirty()? {
                    return Err(invalid_input(
                        "unsaved staged edits; use save, reload, or quit --discard",
                    ));
                }
                break;
            }
            Err(ReadlineError::Eof) => {
                if session.is_dirty()? {
                    return Err(invalid_input(
                        "unsaved staged edits; use save, reload, or quit --discard",
                    ));
                }
                break;
            }
            Err(e) => {
                return Err(CliError::Io(std::io::Error::other(e)));
            }
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
                for line in changes_to_text(&changes, Some(session.model())) {
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

fn changes_to_text(changes: &[StagedChange], model: Option<&CompositeModel>) -> Vec<String> {
    changes
        .iter()
        .flat_map(|change| match (&change.before, &change.after) {
            (None, Some(after)) => {
                vec![style_green(&format!(
                    "A {} {}",
                    change.path,
                    format_value(model, after)
                ))]
            }
            (Some(before), None) => {
                vec![style_red(&format!(
                    "D {} {}",
                    change.path,
                    format_value(model, before)
                ))]
            }
            (Some(before), Some(after)) => {
                let mut lines = vec![format!("{} {}", style_yellow("M"), change.path)];
                lines.extend(diff_value_changes("", before, after, model));
                lines
            }
            (None, None) => {
                vec![style_red(&format!("D {}", change.path))]
            }
        })
        .collect()
}

/// Render a JSON value for diff display, resolving SID integers to names
/// when a model is available.
fn format_value(model: Option<&CompositeModel>, value: &Value) -> String {
    let resolved = model.map(|m| resolve_sids_in_value(m, value.clone())).unwrap_or_else(|| value.clone());
    compact_json(&resolved)
}

/// Recursively walk a JSON value and replace any integer that is a known
/// SID with its human-readable identifier string.
fn resolve_sids_in_value(model: &CompositeModel, value: Value) -> Value {
    match value {
        Value::Number(ref n) => {
            if let Some(sid) = n.as_i64()
                && let Some(name) = model.get_identifier(sid)
            {
                Value::String(name.to_string())
            } else {
                value
            }
        }
        Value::Object(map) => {
            let resolved: serde_json::Map<String, Value> = map
                .into_iter()
                .map(|(k, v)| (k, resolve_sids_in_value(model, v)))
                .collect();
            Value::Object(resolved)
        }
        Value::Array(arr) => Value::Array(
            arr.into_iter()
                .map(|v| resolve_sids_in_value(model, v))
                .collect(),
        ),
        other => other,
    }
}

/// Recursively diff two JSON values, returning human-readable change lines
/// showing only the leaf-level differences.
fn diff_value_changes(
    prefix: &str,
    before: &Value,
    after: &Value,
    model: Option<&CompositeModel>,
) -> Vec<String> {
    if before == after {
        return vec![];
    }
    let mut lines = Vec::new();
    match (before, after) {
        (Value::Object(b), Value::Object(a)) => {
            let all_keys: BTreeSet<&String> = b.keys().chain(a.keys()).collect();
            for key in all_keys {
                let child_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}/{key}")
                };
                match (b.get(key), a.get(key)) {
                    (Some(bv), Some(av)) => {
                        lines.extend(diff_value_changes(&child_prefix, bv, av, model));
                    }
                    (Some(bv), None) => {
                        lines.push(style_red(&format!(
                            "  - {child_prefix}: {}",
                            format_value(model, bv)
                        )));
                    }
                    (None, Some(av)) => {
                        lines.push(style_green(&format!(
                            "  + {child_prefix}: {}",
                            format_value(model, av)
                        )));
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        (Value::Array(b_arr), Value::Array(a_arr)) => {
            let max_len = b_arr.len().max(a_arr.len());
            for i in 0..max_len {
                let child_prefix = format!("{prefix}[{i}]");
                match (b_arr.get(i), a_arr.get(i)) {
                    (Some(bv), Some(av)) => {
                        lines.extend(diff_value_changes(&child_prefix, bv, av, model));
                    }
                    (Some(bv), None) => {
                        lines.push(style_red(&format!(
                            "  - {child_prefix}: {}",
                            format_value(model, bv)
                        )));
                    }
                    (None, Some(av)) => {
                        lines.push(style_green(&format!(
                            "  + {child_prefix}: {}",
                            format_value(model, av)
                        )));
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        _ => {
            lines.push(style_red(&format!("  - {}", format_value(model, before))));
            lines.push(style_green(&format!("  + {}", format_value(model, after))));
        }
    }
    lines
}

fn style_green(text: &str) -> String {
    format!("\x1b[32m{text}\x1b[0m")
}
fn style_red(text: &str) -> String {
    format!("\x1b[31m{text}\x1b[0m")
}
fn style_yellow(text: &str) -> String {
    format!("\x1b[33m{text}\x1b[0m")
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
