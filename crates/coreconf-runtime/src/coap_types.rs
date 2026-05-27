/// CORECONF interface type — distinguishes management (`/c`) from streaming (`/s`).
///
/// Per draft-ietf-core-comi:
/// - `/c`  (management) handles GET, FETCH, iPATCH, POST on configuration/telemetry data.
/// - `/s`  (streaming) handles FETCH+Observe for time-series and event notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interface {
    Management,
    Streaming,
}

impl Interface {
    /// Recognise the standard CORECONF URI path segments.
    pub fn from_uri_segment(segment: &str) -> Option<Self> {
        match segment {
            "c" => Some(Self::Management),
            "s" => Some(Self::Streaming),
            _ => None,
        }
    }

    pub fn as_uri_segment(self) -> &'static str {
        match self {
            Self::Management => "c",
            Self::Streaming => "s",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ContentFormat {
    YangDataCbor = 140,
    YangIdentifiersCbor = 141,
    YangInstancesCborSeq = 143,
}

impl ContentFormat {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            140 => Some(Self::YangDataCbor),
            141 => Some(Self::YangIdentifiersCbor),
            143 => Some(Self::YangInstancesCborSeq),
            _ => None,
        }
    }

    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Fetch,
    Get,
    IPatch,
    Post,
    Delete,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Method::Fetch => f.write_str("FETCH"),
            Method::Get => f.write_str("GET"),
            Method::IPatch => f.write_str("iPATCH"),
            Method::Post => f.write_str("POST"),
            Method::Delete => f.write_str("DELETE"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseCode {
    Created,
    Changed,
    Content,
    BadRequest,
    Unauthorized,
    BadOption,
    NotFound,
    MethodNotAllowed,
    RequestEntityIncomplete,
    Conflict,
    RequestEntityTooLarge,
    UnsupportedContentFormat,
    InternalServerError,
}

impl std::fmt::Display for ResponseCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (class, detail) = self.to_code_pair();
        write!(f, "{class}.{detail:02}")
    }
}

impl ResponseCode {
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

    pub fn is_success(self) -> bool {
        matches!(self, Self::Created | Self::Changed | Self::Content)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentParam {
    #[default]
    All,
    Config,
    Nonconfig,
}

impl ContentParam {
    pub fn from_query_value(s: &str) -> Option<Self> {
        match s {
            "a" => Some(Self::All),
            "c" => Some(Self::Config),
            "n" => Some(Self::Nonconfig),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DefaultsParam {
    #[default]
    All,
    Trim,
}

impl DefaultsParam {
    pub fn from_query_value(s: &str) -> Option<Self> {
        match s {
            "a" => Some(Self::All),
            "t" => Some(Self::Trim),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct QueryParams {
    pub content: ContentParam,
    pub defaults: DefaultsParam,
}

impl QueryParams {
    pub fn parse(query: &str) -> Self {
        let mut params = Self::default();
        for part in query.split('&') {
            if let Some((key, value)) = part.split_once('=') {
                match key {
                    "c" => {
                        if let Some(content) = ContentParam::from_query_value(value) {
                            params.content = content;
                        }
                    }
                    "d" => {
                        if let Some(defaults) = DefaultsParam::from_query_value(value) {
                            params.defaults = defaults;
                        }
                    }
                    _ => {}
                }
            }
        }
        params
    }
}

pub mod resource_types {
    pub const DATASTORE: &str = "core.c.ds";
    pub const EVENT_STREAM: &str = "core.c.ev";
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub payload: Vec<u8>,
    pub content_format: Option<ContentFormat>,
    pub query: QueryParams,
    /// Which CORECONF interface this request targets (`/c` or `/s`).
    pub interface: Option<Interface>,
    /// CoAP Observe option: `Some(0)` = register, `Some(n)` = notification.
    pub observe: Option<u32>,
    /// CoAP token for matching requests to responses / observer state.
    pub token: Vec<u8>,
}

impl Request {
    pub fn new(method: Method) -> Self {
        Self {
            method,
            path: String::new(),
            payload: Vec::new(),
            content_format: None,
            query: QueryParams::default(),
            interface: None,
            observe: None,
            token: Vec::new(),
        }
    }

    pub fn with_token(mut self, token: Vec<u8>) -> Self {
        self.token = token;
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn with_payload(mut self, payload: Vec<u8>, format: ContentFormat) -> Self {
        self.payload = payload;
        self.content_format = Some(format);
        self
    }

    pub fn with_query(mut self, query: QueryParams) -> Self {
        self.query = query;
        self
    }

    pub fn with_interface(mut self, interface: Interface) -> Self {
        self.interface = Some(interface);
        self
    }

    pub fn with_observe(mut self, observe: u32) -> Self {
        self.observe = Some(observe);
        self
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub code: ResponseCode,
    pub payload: Vec<u8>,
    pub content_format: Option<ContentFormat>,
    /// CoAP Observe sequence number (present on notifications).
    pub observe: Option<u32>,
}

impl Response {
    pub fn content(payload: Vec<u8>, format: ContentFormat) -> Self {
        Self {
            code: ResponseCode::Content,
            payload,
            content_format: Some(format),
            observe: None,
        }
    }

    pub fn observe(payload: Vec<u8>, format: ContentFormat, sequence: u32) -> Self {
        Self {
            code: ResponseCode::Content,
            payload,
            content_format: Some(format),
            observe: Some(sequence),
        }
    }

    pub fn changed() -> Self {
        Self {
            code: ResponseCode::Changed,
            payload: Vec::new(),
            content_format: None,
            observe: None,
        }
    }

    pub fn error(code: ResponseCode, message: &str) -> Self {
        Self {
            code,
            payload: message.as_bytes().to_vec(),
            content_format: None,
            observe: None,
        }
    }

    pub fn not_found(path: &str) -> Self {
        Self::error(
            ResponseCode::NotFound,
            &format!("Resource not found: {path}"),
        )
    }

    pub fn method_not_allowed(method: Method) -> Self {
        Self::error(
            ResponseCode::MethodNotAllowed,
            &format!("Method {method} not allowed"),
        )
    }
}
