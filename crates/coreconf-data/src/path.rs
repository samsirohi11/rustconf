use crate::error::{CoreconfError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathExpr {
    pub segments: Vec<PathSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathSegment {
    pub name: String,
    pub predicates: Vec<Predicate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Predicate {
    pub key: String,
    pub value: String,
}

impl PathExpr {
    pub fn parse(input: &str) -> Result<Self> {
        let mut segments = Vec::new();

        for raw in input.split('/').filter(|segment| !segment.is_empty()) {
            let name = raw.split('[').next().unwrap_or(raw).to_string();
            let mut predicates = Vec::new();
            let mut remainder = &raw[name.len()..];

            while let Some(start) = remainder.find('[') {
                let after_start = &remainder[start + 1..];
                let end = after_start.find(']').ok_or_else(|| {
                    CoreconfError::ValidationError(format!("unterminated predicate in path: {}", input))
                })?;
                let entry = &after_start[..end];
                let (key, value) = entry.split_once('=').ok_or_else(|| {
                    CoreconfError::ValidationError(format!("invalid predicate in path: {}", entry))
                })?;
                predicates.push(Predicate {
                    key: key.to_string(),
                    value: value.trim_matches('\'').to_string(),
                });
                remainder = &after_start[end + 1..];
            }

            segments.push(PathSegment { name, predicates });
        }

        Ok(Self { segments })
    }
}
