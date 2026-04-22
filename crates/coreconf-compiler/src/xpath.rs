#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum XPathError {
    #[error("invalid xpath expression: {0}")]
    Invalid(String),
}

pub fn validate_xpath(input: &str) -> Result<(), XPathError> {
    if input.is_empty() || input.ends_with('[') {
        return Err(XPathError::Invalid(input.to_string()));
    }
    Ok(())
}
