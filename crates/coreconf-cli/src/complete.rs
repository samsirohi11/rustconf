//! Autocomplete support for the shell and live CLI commands.
//!
//! Provides tab-completion for commands and SID-model paths using
//! the rustyline crate.  Path completion queries the loaded
//! CompositeModel for available identifiers matching the current
//! input prefix.

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};

use coreconf_model::CompositeModel;

/// A rustyline `Helper` that autocompletes CORECONF commands and paths.
pub struct CoreconfCompleter {
    pub model: CompositeModel,
}

impl CoreconfCompleter {
    const COMMANDS: &[&str] = &[
        "get", "set", "delete", "dump", "diff", "save", "reload", "push", "quit", "exit", "help",
    ];

    /// Return every model identifier whose string representation
    /// starts with the given prefix (case-insensitive).
    /// Model keys already include a leading `/`, so we match them directly.
    pub fn matching_identifiers(&self, prefix: &str) -> Vec<String> {
        let prefix_lower = prefix.to_lowercase();
        if prefix_lower.is_empty() {
            return Vec::new();
        }

        let mut matches: Vec<String> = self
            .model
            .sids
            .keys()
            .filter(|path| path.to_lowercase().starts_with(&prefix_lower))
            .cloned()
            .collect();

        matches.sort_by_key(|m| m.len());
        matches.dedup();
        matches
    }

    /// Shared completion logic — used by both Tab-complete and hint.
    fn completions_for(&self, line: &str, pos: usize) -> rustyline::Result<(usize, Vec<Pair>)> {
        let (start, word) = word_under_cursor(line, pos);

        if word.starts_with('/') {
            let candidates: Vec<Pair> = self
                .matching_identifiers(&word)
                .into_iter()
                .map(|p| Pair {
                    display: p.clone(),
                    replacement: p,
                })
                .collect();
            return Ok((start, candidates));
        }

        let candidates: Vec<Pair> = Self::COMMANDS
            .iter()
            .filter(|c| c.starts_with(&word))
            .map(|c| Pair {
                display: c.to_string(),
                replacement: format!("{c} "),
            })
            .collect();
        Ok((start, candidates))
    }
}

impl Completer for CoreconfCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        self.completions_for(line, pos)
    }
}

impl Hinter for CoreconfCompleter {
    type Hint = String;

    /// Show the top suggestion as a grey inline hint while typing.
    /// Press Right arrow to accept it, or Tab for the full dropdown.
    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<String> {
        let (_start, word) = word_under_cursor(line, pos);
        if word.is_empty() {
            return None;
        }
        if word.starts_with('/') {
            let matches = self.matching_identifiers(&word);
            matches
                .into_iter()
                .next()
                .map(|m| m[word.len()..].to_string())
        } else {
            Self::COMMANDS
                .iter()
                .find(|c| c.starts_with(&word))
                .map(|c| c[word.len()..].to_string())
        }
    }
}

impl Highlighter for CoreconfCompleter {}

impl Validator for CoreconfCompleter {}

impl Helper for CoreconfCompleter {}

/// Extract the word under the cursor for autocompletion.
fn word_under_cursor(line: &str, pos: usize) -> (usize, String) {
    let line_bytes = line.as_bytes();
    let start = (0..pos)
        .rev()
        .take_while(|&i| line_bytes.get(i).is_some_and(|b| !b.is_ascii_whitespace()))
        .last()
        .unwrap_or(pos);
    let end = (pos..line.len())
        .take_while(|&i| line_bytes.get(i).is_some_and(|b| !b.is_ascii_whitespace()))
        .last()
        .map(|i| i + 1)
        .unwrap_or(pos);
    (start, line[start..end].to_string())
}
