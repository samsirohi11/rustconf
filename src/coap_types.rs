//! CoMI-specific types and constants
//!
//! This module defines CoAP types for CORECONF protocol.
//! These abstractions allow the library to work with any CoAP implementation.

/// CoAP Content-Format identifiers for CORECONF
/// See: https://www.ietf.org/archive/id/draft-ietf-core-comi-20.html#section-2.3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ContentFormat {
    /// application/yang-data+cbor
    YangDataCbor = 112,
    /// application/yang-identifiers+cbor
    YangIdentifiersCbor = 311,
    /// application/yang-instances+cbor-seq
    YangInstancesCborSeq = 313,
}

impl ContentFormat {
    /// Convert from raw content-format ID
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            112 => Some(Self::YangDataCbor),
            311 => Some(Self::YangIdentifiersCbor),
            313 => Some(Self::YangInstancesCborSeq),
            _ => None,
        }
    }

    /// Get the raw content-format ID
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// CORECONF request methods
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Retrieve data nodes (RFC 8132)
    Fetch,
    /// Retrieve full datastore
    Get,
    /// Modify data nodes (RFC 8132)
    IPatch,
    /// Invoke RPC or Action
    Post,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Method::Fetch => f.write_str("FETCH"),
            Method::Get => f.write_str("GET"),
            Method::IPatch => f.write_str("iPATCH"),
            Method::Post => f.write_str("POST"),
        }
    }
}

/// CoAP response codes used by CORECONF
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseCode {
    // Success codes
    /// 2.01 Created
    Created,
    /// 2.04 Changed
    Changed,
    /// 2.05 Content
    Content,

    // Client error codes
    /// 4.00 Bad Request
    BadRequest,
    /// 4.01 Unauthorized
    Unauthorized,
    /// 4.02 Bad Option
    BadOption,
    /// 4.04 Not Found
    NotFound,
    /// 4.05 Method Not Allowed
    MethodNotAllowed,
    /// 4.08 Request Entity Incomplete
    RequestEntityIncomplete,
    /// 4.09 Conflict (YANG validation error)
    Conflict,
    /// 4.13 Request Entity Too Large
    RequestEntityTooLarge,
    /// 4.15 Unsupported Content-Format
    UnsupportedContentFormat,

    // Server error codes
    /// 5.00 Internal Server Error
    InternalServerError,
}

impl std::fmt::Display for ResponseCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (class, detail) = self.to_code_pair();
        write!(f, "{}.{:02}", class, detail)
    }
}

impl ResponseCode {
    /// Convert to CoAP response code format (class.detail)
    pub fn to_code_pair(self) -> (u8, u8) {
        match self {
            Self::Created => (2, 1),
            Self::Changed => (2, 4),
            Self::Content => (2, 5),
            Self::BadRequest => (4, 0),
            Self::Unauthorized => (4, 1),
            Self::BadOption => (4, 2),
            Self::NotFound => (4, 4),
            Self::MethodNotAllowed => (4, 5),
            Self::RequestEntityIncomplete => (4, 8),
            Self::Conflict => (4, 9),
            Self::RequestEntityTooLarge => (4, 13),
            Self::UnsupportedContentFormat => (4, 15),
            Self::InternalServerError => (5, 0),
        }
    }

    /// Check if this is a success code
    pub fn is_success(self) -> bool {
        matches!(self, Self::Created | Self::Changed | Self::Content)
    }
}

/// Query parameter 'c' (content) values
/// Controls how descendant nodes are processed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentParam {
    /// Return all descendant data nodes
    #[default]
    All,
    /// Return only configuration data nodes
    Config,
    /// Return only non-configuration data nodes
    Nonconfig,
}

impl ContentParam {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "a" => Some(Self::All),
            "c" => Some(Self::Config),
            "n" => Some(Self::Nonconfig),
            _ => None,
        }
    }
}

/// Query parameter 'd' (with-defaults) values
/// Controls how default values are processed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DefaultsParam {
    /// Report-all mode
    #[default]
    All,
    /// Trim mode - don't report defaults
    Trim,
}

impl DefaultsParam {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "a" => Some(Self::All),
            "t" => Some(Self::Trim),
            _ => None,
        }
    }
}

/// Parsed query parameters from a CORECONF request
#[derive(Debug, Clone, Default)]
pub struct QueryParams {
    pub content: ContentParam,
    pub defaults: DefaultsParam,
}

impl QueryParams {
    /// Parse query parameters from a query string
    pub fn parse(query: &str) -> Self {
        let mut params = Self::default();
        for part in query.split('&') {
            if let Some((key, value)) = part.split_once('=') {
                match key {
                    "c" => {
                        if let Some(c) = ContentParam::from_str(value) {
                            params.content = c;
                        }
                    }
                    "d" => {
                        if let Some(d) = DefaultsParam::from_str(value) {
                            params.defaults = d;
                        }
                    }
                    _ => {}
                }
            }
        }
        params
    }
}

/// Resource types for CORECONF discovery
pub mod resource_types {
    /// Datastore resource type
    pub const DATASTORE: &str = "core.c.ds";
    /// Event stream resource type
    pub const EVENT_STREAM: &str = "core.c.ev";
}

/// A CORECONF request (transport-agnostic)
#[derive(Debug, Clone)]
pub struct Request {
    /// The request method
    pub method: Method,
    /// CBOR-encoded payload
    pub payload: Vec<u8>,
    /// Content format of the payload
    pub content_format: Option<ContentFormat>,
    /// Parsed query parameters
    pub query: QueryParams,
}

impl Request {
    /// Create a new request
    pub fn new(method: Method) -> Self {
        Self {
            method,
            payload: Vec::new(),
            content_format: None,
            query: QueryParams::default(),
        }
    }

    /// Set the payload
    pub fn with_payload(mut self, payload: Vec<u8>, format: ContentFormat) -> Self {
        self.payload = payload;
        self.content_format = Some(format);
        self
    }

    /// Set query parameters
    pub fn with_query(mut self, query: QueryParams) -> Self {
        self.query = query;
        self
    }
}

/// A CORECONF response (transport-agnostic)
#[derive(Debug, Clone)]
pub struct Response {
    /// Response code
    pub code: ResponseCode,
    /// CBOR-encoded payload
    pub payload: Vec<u8>,
    /// Content format of the payload
    pub content_format: Option<ContentFormat>,
}

impl Response {
    /// Create a success response with content
    pub fn content(payload: Vec<u8>, format: ContentFormat) -> Self {
        Self {
            code: ResponseCode::Content,
            payload,
            content_format: Some(format),
        }
    }

    /// Create a changed response (for iPATCH)
    pub fn changed() -> Self {
        Self {
            code: ResponseCode::Changed,
            payload: Vec::new(),
            content_format: None,
        }
    }

    /// Create an error response
    pub fn error(code: ResponseCode, message: &str) -> Self {
        Self {
            code,
            payload: message.as_bytes().to_vec(),
            content_format: None,
        }
    }

    /// Create a not found error
    pub fn not_found(path: &str) -> Self {
        Self::error(
            ResponseCode::NotFound,
            &format!("Resource not found: {}", path),
        )
    }

    /// Create a method not allowed error
    pub fn method_not_allowed(method: Method) -> Self {
        Self::error(
            ResponseCode::MethodNotAllowed,
            &format!("Method {} not allowed", method),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_format_conversion() {
        assert_eq!(
            ContentFormat::from_u16(112),
            Some(ContentFormat::YangDataCbor)
        );
        assert_eq!(ContentFormat::YangInstancesCborSeq.as_u16(), 313);
    }

    #[test]
    fn test_response_code() {
        assert_eq!(ResponseCode::Content.to_code_pair(), (2, 5));
        assert!(ResponseCode::Changed.is_success());
        assert!(!ResponseCode::NotFound.is_success());
    }

    #[test]
    fn test_query_params_parse() {
        let params = QueryParams::parse("c=c&d=t");
        assert_eq!(params.content, ContentParam::Config);
        assert_eq!(params.defaults, DefaultsParam::Trim);
    }
}
