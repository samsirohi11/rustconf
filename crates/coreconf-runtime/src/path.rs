use coreconf_model::{CoreconfError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredicatePath {
    pub canonical_path: String,
    pub predicates: Vec<(String, String)>,
}

impl PredicatePath {
    pub fn parse(input: &str) -> Result<Self> {
        if input.is_empty() {
            return Ok(Self {
                canonical_path: "/".into(),
                predicates: Vec::new(),
            });
        }

        let mut canonical_segments = Vec::new();
        let mut predicates = Vec::new();

        for segment in split_segments(input)? {
            let (base, segment_predicates) = parse_segment(&segment)?;
            if !base.is_empty() {
                canonical_segments.push(base);
            }
            predicates.extend(segment_predicates);
        }

        let canonical_path = if canonical_segments.is_empty() {
            "/".into()
        } else {
            format!("/{}", canonical_segments.join("/"))
        };

        Ok(Self {
            canonical_path,
            predicates,
        })
    }
}

fn split_segments(input: &str) -> Result<Vec<String>> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut bracket_depth = 0usize;
    let mut quote = None;
    let mut escaped = false;

    for ch in input.chars() {
        if let Some(active_quote) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '/' if bracket_depth == 0 => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
            }
            '\'' | '"' if bracket_depth > 0 => {
                quote = Some(ch);
                current.push(ch);
            }
            '[' => {
                bracket_depth += 1;
                current.push(ch);
            }
            ']' => {
                if bracket_depth == 0 {
                    return Err(CoreconfError::ValidationError(format!(
                        "unmatched closing bracket in path '{input}'"
                    )));
                }
                bracket_depth -= 1;
                current.push(ch);
            }
            _ => current.push(ch),
        }
    }

    if quote.is_some() {
        return Err(CoreconfError::ValidationError(format!(
            "unterminated quoted predicate value in path '{input}'"
        )));
    }

    if bracket_depth != 0 {
        return Err(CoreconfError::ValidationError(format!(
            "unterminated predicate in path '{input}'"
        )));
    }

    if !current.is_empty() {
        segments.push(current);
    }

    Ok(segments)
}

fn parse_segment(segment: &str) -> Result<(String, Vec<(String, String)>)> {
    let mut base = String::new();
    let mut predicates = Vec::new();
    let chars: Vec<char> = segment.chars().collect();
    let mut index = 0usize;

    while index < chars.len() && chars[index] != '[' {
        base.push(chars[index]);
        index += 1;
    }

    while index < chars.len() {
        if chars[index] != '[' {
            return Err(CoreconfError::ValidationError(format!(
                "unexpected character '{}' in path segment '{segment}'",
                chars[index]
            )));
        }
        index += 1;

        let predicate_start = index;
        let mut quote = None;
        let mut escaped = false;
        while index < chars.len() {
            let ch = chars[index];
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == active_quote {
                    quote = None;
                }
            } else if ch == '\'' || ch == '"' {
                quote = Some(ch);
            } else if ch == ']' {
                break;
            }
            index += 1;
        }
        if index >= chars.len() {
            return Err(CoreconfError::ValidationError(format!(
                "unterminated predicate in path segment '{segment}'"
            )));
        }

        let predicate = chars[predicate_start..index].iter().collect::<String>();
        let (name, value) = parse_predicate(&predicate)?;
        predicates.push((name, value));
        index += 1;
    }

    Ok((base, predicates))
}

fn parse_predicate(predicate: &str) -> Result<(String, String)> {
    let (name, raw_value) = predicate.split_once('=').ok_or_else(|| {
        CoreconfError::ValidationError(format!("predicate '{predicate}' is missing '='"))
    })?;

    let value = raw_value.trim();
    if value.len() < 2 {
        return Err(CoreconfError::ValidationError(format!(
            "predicate '{predicate}' is missing quotes"
        )));
    }

    let quote = value.chars().next().unwrap_or_default();
    if (quote != '\'' && quote != '"') || !value.ends_with(quote) {
        return Err(CoreconfError::ValidationError(format!(
            "predicate '{predicate}' must use matching quotes"
        )));
    }

    Ok((
        name.trim().to_string(),
        unescape_predicate_value(&value[1..value.len() - 1])?,
    ))
}

fn unescape_predicate_value(value: &str) -> Result<String> {
    let mut unescaped = String::with_capacity(value.len());
    let mut escaped = false;

    for ch in value.chars() {
        if escaped {
            unescaped.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            unescaped.push(ch);
        }
    }

    if escaped {
        return Err(CoreconfError::ValidationError(
            "predicate value ends with unfinished escape".into(),
        ));
    }

    Ok(unescaped)
}
