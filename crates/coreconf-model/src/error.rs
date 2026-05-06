use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreconfError {
    #[error("SID not found for identifier: {0}")]
    SidNotFound(String),

    #[error("Identifier not found for SID: {0}")]
    IdentifierNotFound(i64),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("CBOR decode error: {0}")]
    CborDecode(String),

    #[error("CBOR encode error: {0}")]
    CborEncode(String),

    #[error("Type conversion error: {0}")]
    TypeConversion(String),

    #[error("Invalid SID file: {0}")]
    InvalidSidFile(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Method not allowed: {0}")]
    MethodNotAllowed(String),

    #[error("Unsupported content format")]
    UnsupportedContentFormat,
}

pub type Result<T> = std::result::Result<T, CoreconfError>;
