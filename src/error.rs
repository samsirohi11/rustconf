//! Error types for rust-coreconf

use thiserror::Error;

/// Main error type for coreconf operations
#[derive(Debug, Error)]
pub enum CoreconfError {
    /// SID not found for the given identifier path
    #[error("SID not found for identifier: {0}")]
    SidNotFound(String),

    /// Identifier not found for the given SID value
    #[error("Identifier not found for SID: {0}")]
    IdentifierNotFound(i64),

    /// IO error (file operations)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing/serialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// CBOR decoding error
    #[error("CBOR decode error: {0}")]
    CborDecode(String),

    /// CBOR encoding error
    #[error("CBOR encode error: {0}")]
    CborEncode(String),

    /// Type conversion error
    #[error("Type conversion error: {0}")]
    TypeConversion(String),

    /// Invalid SID file format
    #[error("Invalid SID file: {0}")]
    InvalidSidFile(String),

    /// YANG validation error (maps to CoAP 4.09 Conflict)
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Resource not found (maps to CoAP 4.04)
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    /// Method not allowed (maps to CoAP 4.05)
    #[error("Method not allowed: {0}")]
    MethodNotAllowed(String),

    /// Unsupported content format (maps to CoAP 4.15)
    #[error("Unsupported content format")]
    UnsupportedContentFormat,
}

/// Result type alias for coreconf operations
pub type Result<T> = std::result::Result<T, CoreconfError>;
