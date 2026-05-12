use std::net::{ToSocketAddrs, UdpSocket};
use std::time::Duration;

use coap_lite::block_handler::BlockValue;
use coap_lite::{
    CoapOption, ContentFormat as CoapContentFormat, MessageClass, MessageType, Packet, RequestType,
    ResponseType,
};
use coreconf_model::{CompositeModel, CoreconfError, Result};
use serde_json::Value;

use crate::coap_types::{
    ContentFormat, Interface, Method, QueryParams, Request, Response, ResponseCode,
};
use crate::request_handler::RequestHandler;

/// Maximum payload bytes per CoAP block to stay safely under the
/// 1152-byte default message size after adding headers and options.
const MAX_BLOCK_PAYLOAD: usize = 1024;

pub trait CoreconfClient {
    fn fetch_snapshot(&mut self) -> Result<Value>;
    fn apply_patch(&mut self, patch: &[(String, Option<Value>)]) -> Result<()>;
}

pub struct CoapLiteClient {
    socket: UdpSocket,
    endpoint: String,
    resource_path: String,
    model: CompositeModel,
    next_message_id: u16,
}

impl CoapLiteClient {
    pub fn connect(
        model: CompositeModel,
        endpoint: impl ToSocketAddrs,
        resource_path: impl Into<String>,
    ) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(Duration::from_secs(5)))?;
        socket.connect(endpoint)?;
        let endpoint = socket.peer_addr()?.to_string();

        Ok(Self {
            socket,
            endpoint,
            resource_path: resource_path.into(),
            model,
            next_message_id: 1,
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Send an iPATCH request split across multiple Block1 transfers (RFC 7959).
    fn send_blockwise_ipatch(&mut self, path: &str, payload: &[u8]) -> Result<()> {
        let blocks: Vec<&[u8]> = payload.chunks(MAX_BLOCK_PAYLOAD).collect();
        let total = blocks.len();

        for (i, chunk) in blocks.iter().enumerate() {
            let more = i + 1 < total;
            let block = BlockValue::new(i, more, MAX_BLOCK_PAYLOAD)
                .map_err(|e| invalid_data(e.to_string()))?;

            let response = self.send_coreconf_request_with_block(
                RequestType::IPatch,
                Some(path),
                chunk.to_vec(),
                Some(ContentFormat::YangDataCbor),
                block,
            )?;

            if more {
                if !matches!(
                    response.header.code,
                    MessageClass::Response(ResponseType::Continue)
                ) {
                    return Err(CoreconfError::ValidationError(format!(
                        "Block1: expected Continue (2.31) for block {}/{}, got {:?}",
                        i + 1,
                        total,
                        response.header.code
                    )));
                }
            } else {
                ensure_success(&response)?;
            }
        }
        Ok(())
    }

    fn send_coreconf_request_with_block(
        &mut self,
        method: RequestType,
        path: Option<&str>,
        payload: Vec<u8>,
        content_format: Option<ContentFormat>,
        block1: BlockValue,
    ) -> Result<Packet> {
        let mut packet = self.build_packet(method, path, payload, content_format);
        packet.add_option_as(CoapOption::Block1, block1);
        self.send_packet(packet)
    }

    fn send_coreconf_request(
        &mut self,
        method: RequestType,
        path: Option<&str>,
        payload: Vec<u8>,
        content_format: Option<ContentFormat>,
    ) -> Result<Packet> {
        let packet = self.build_packet(method, path, payload, content_format);
        self.send_packet(packet)
    }

    fn build_packet(
        &mut self,
        method: RequestType,
        path: Option<&str>,
        payload: Vec<u8>,
        content_format: Option<ContentFormat>,
    ) -> Packet {
        let mut packet = Packet::new();
        packet.header.message_id = self.next_message_id;
        self.next_message_id = self.next_message_id.wrapping_add(1);
        packet.header.code = MessageClass::Request(method);
        packet.header.set_type(MessageType::Confirmable);
        packet.set_token(vec![0xC0]);
        add_uri_path(&mut packet, &self.resource_path);
        if let Some(path) = path {
            add_uri_path(&mut packet, path);
        }

        if !payload.is_empty() {
            packet.payload = payload;
            if let Some(format) = content_format {
                packet.set_content_format(content_format_to_coap(format));
            }
        }
        packet
    }

    fn send_packet(&mut self, packet: Packet) -> Result<Packet> {
        let bytes = packet
            .to_bytes()
            .map_err(|error| invalid_data(error.to_string()))?;
        self.socket.send(&bytes)?;

        let mut buffer = [0u8; 1500];
        let len = self.socket.recv(&mut buffer)?;
        Packet::from_bytes(&buffer[..len]).map_err(|error| invalid_data(error.to_string()))
    }
}

impl CoreconfClient for CoapLiteClient {
    fn fetch_snapshot(&mut self) -> Result<Value> {
        let response = self.send_coreconf_request(RequestType::Get, None, Vec::new(), None)?;
        ensure_success(&response)?;
        let json = coreconf_model::decode_cbor_to_json(&self.model, &response.payload)?;
        serde_json::from_str(&json).map_err(CoreconfError::from)
    }

    fn apply_patch(&mut self, patch: &[(String, Option<Value>)]) -> Result<()> {
        for (path, value) in patch {
            let Some(value) = value else {
                return Err(CoreconfError::ValidationError(
                    "CoAP path delete is not supported by the reference adapter yet".into(),
                ));
            };

            let mut payload = Vec::new();
            ciborium::into_writer(value, &mut payload)
                .map_err(|error| CoreconfError::CborEncode(error.to_string()))?;

            if payload.len() <= MAX_BLOCK_PAYLOAD {
                let response = self.send_coreconf_request(
                    RequestType::IPatch,
                    Some(path),
                    payload,
                    Some(ContentFormat::YangDataCbor),
                )?;
                ensure_success(&response)?;
            } else {
                self.send_blockwise_ipatch(path, &payload)?;
            }
        }
        Ok(())
    }
}

pub struct CoapLiteServer {
    socket: UdpSocket,
    resource_path: String,
    handler: RequestHandler,
}

impl CoapLiteServer {
    pub fn bind(
        bind_addr: impl ToSocketAddrs,
        resource_path: impl Into<String>,
        handler: RequestHandler,
    ) -> Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind(bind_addr)?,
            resource_path: resource_path.into(),
            handler,
        })
    }

    pub fn local_addr(&self) -> Result<std::net::SocketAddr> {
        self.socket.local_addr().map_err(CoreconfError::from)
    }

    pub fn socket(&self) -> &UdpSocket {
        &self.socket
    }

    pub fn handler(&self) -> &RequestHandler {
        &self.handler
    }

    pub fn handler_mut(&mut self) -> &mut RequestHandler {
        &mut self.handler
    }

    pub fn handle_packet(&mut self, packet: &Packet) -> Packet {
        let request = packet_to_request(packet, &self.resource_path);
        let response = match request {
            Ok(request) => self.handler.handle(&request),
            Err(response) => response,
        };
        response_to_packet(packet, response)
    }

    pub fn serve_once(&mut self) -> Result<()> {
        let mut buffer = [0u8; 1500];
        let (len, peer) = self.socket.recv_from(&mut buffer)?;
        let packet =
            Packet::from_bytes(&buffer[..len]).map_err(|error| invalid_data(error.to_string()))?;
        let response = self.handle_packet(&packet);
        let bytes = response
            .to_bytes()
            .map_err(|error| invalid_data(error.to_string()))?;
        self.socket.send_to(&bytes, peer)?;
        Ok(())
    }
}

pub fn packet_to_request(
    packet: &Packet,
    resource_path: &str,
) -> std::result::Result<Request, Response> {
    let method = match packet.header.code {
        MessageClass::Request(RequestType::Get) => Method::Get,
        MessageClass::Request(RequestType::Post) => Method::Post,
        MessageClass::Request(RequestType::Fetch) => Method::Fetch,
        MessageClass::Request(RequestType::Patch) | MessageClass::Request(RequestType::IPatch) => {
            Method::IPatch
        }
        _ => {
            return Err(Response::method_not_allowed(Method::Get));
        }
    };

    let uri_path = uri_path(packet);
    let Some(remaining_path) = uri_path.strip_prefix(resource_path.trim_matches('/')) else {
        return Err(Response::not_found(&uri_path));
    };
    let remaining = remaining_path.trim_start_matches('/');

    // Extract CORECONF interface from the first URI path segment (`c` or `s`).
    //
    // When resource_path is empty, the raw URI path carries the interface
    // (e.g. "c" or "c/some/path").  When resource_path is already the
    // interface segment (e.g. "c"), remaining will be empty and the
    // interface is inferred from the resource_path itself.
    let (path, interface) = if remaining.is_empty() {
        // No sub-path — infer interface from resource_path (legacy compatibility).
        (
            String::new(),
            Interface::from_uri_segment(resource_path.trim_matches('/')),
        )
    } else if let Some((first, rest)) = remaining.split_once('/') {
        if let Some(iface) = Interface::from_uri_segment(first) {
            (rest.to_string(), Some(iface))
        } else {
            (remaining.to_string(), None)
        }
    } else if let Some(iface) = Interface::from_uri_segment(remaining) {
        (String::new(), Some(iface))
    } else {
        (remaining.to_string(), None)
    };

    let mut request = Request::new(method).with_path(if path.is_empty() {
        String::new()
    } else {
        format!("/{path}")
    });
    request.payload = packet.payload.clone();
    request.content_format = raw_content_format(packet)
        .and_then(|raw| content_format_from_raw(method, raw))
        .or_else(|| {
            packet
                .get_content_format()
                .and_then(|format| content_format_from_coap(method, format))
        })
        .or_else(|| default_content_format(method, &request.payload));
    request.query = uri_query(packet);

    if let Some(iface) = interface {
        request.interface = Some(iface);
    }

    // Parse CoAP Observe option.
    if let Some(Ok(observe_value)) = packet.get_observe_value() {
        request.observe = Some(observe_value);
    }

    Ok(request)
}

pub fn response_to_packet(request: &Packet, response: Response) -> Packet {
    let mut packet = Packet::new();
    packet.header.message_id = request.header.message_id;
    packet.set_token(request.get_token().to_vec());
    packet.header.code = response_code_to_coap(response.code);

    if !response.payload.is_empty() {
        packet.payload = response.payload;
        if let Some(format) = response.content_format {
            packet.set_content_format(content_format_to_coap(format));
        }
    }

    // Stamp CoAP Observe sequence number on notifications.
    if let Some(sequence) = response.observe {
        packet.set_observe_value(sequence);
    }

    packet
}

fn add_uri_path(packet: &mut Packet, path: &str) {
    for segment in path.trim_matches('/').split('/').filter(|s| !s.is_empty()) {
        packet.add_option(CoapOption::UriPath, segment.as_bytes().to_vec());
    }
}

fn uri_path(packet: &Packet) -> String {
    packet
        .get_option(CoapOption::UriPath)
        .map(|options| {
            options
                .iter()
                .filter_map(|value| std::str::from_utf8(value).ok())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

fn uri_query(packet: &Packet) -> QueryParams {
    let query = packet
        .get_option(CoapOption::UriQuery)
        .map(|options| {
            options
                .iter()
                .filter_map(|value| std::str::from_utf8(value).ok())
                .collect::<Vec<_>>()
                .join("&")
        })
        .unwrap_or_default();
    QueryParams::parse(&query)
}

fn default_content_format(method: Method, payload: &[u8]) -> Option<ContentFormat> {
    if payload.is_empty() {
        return None;
    }
    match method {
        Method::Fetch => Some(ContentFormat::YangIdentifiersCbor),
        Method::IPatch | Method::Post => Some(ContentFormat::YangDataCbor),
        Method::Get => None,
    }
}

/// Extract the raw content-format option value from a CoAP packet.
///
/// Reads the option bytes directly, avoiding the coap-lite `ContentFormat`
/// enum which lacks RFC-defined CORECONF formats (141, 142, 143).
fn raw_content_format(packet: &Packet) -> Option<u16> {
    let raw = packet.get_option(CoapOption::ContentFormat)?;
    let bytes = raw.front()?;
    if bytes.len() == 1 {
        Some(u16::from(bytes[0]))
    } else if bytes.len() >= 2 {
        Some(u16::from_be_bytes([bytes[0], bytes[1]]))
    } else {
        None
    }
}

/// Map a raw content-format number (RFC 9595 / IANA registry) to a `ContentFormat`.
///
/// Handles both the RFC-defined CORECONF formats used by aiocoap and the
/// coap-lite generic formats.  Prioritises raw numbers over the enum so that
/// clients sending RFC-compliant format numbers (e.g. 141 for
/// yang-identifiers+cbor) are recognised correctly.
fn content_format_from_raw(method: Method, raw: u16) -> Option<ContentFormat> {
    match (method, raw) {
        // RFC 9595 CORECONF content-formats
        (Method::Fetch, 141) => Some(ContentFormat::YangIdentifiersCbor),
        (_, 141) => Some(ContentFormat::YangDataCbor),
        (_, 142) | (_, 140) => Some(ContentFormat::YangDataCbor),
        (_, 143) | (_, 63) | (_, 271) => Some(ContentFormat::YangInstancesCborSeq),
        _ => None,
    }
}

fn content_format_to_coap(format: ContentFormat) -> CoapContentFormat {
    // Use RFC 9595 format numbers when available; fall back to coap-lite
    // generics for broader compatibility.
    match format {
        ContentFormat::YangDataCbor => CoapContentFormat::ApplicationYangDataCbor, // 142
        ContentFormat::YangIdentifiersCbor => CoapContentFormat::ApplicationCBOR, // 60 (no RFC-specific variant in coap-lite)
        ContentFormat::YangInstancesCborSeq => CoapContentFormat::ApplicationCborSeq, // 63
    }
}

fn content_format_from_coap(method: Method, format: CoapContentFormat) -> Option<ContentFormat> {
    match (method, format) {
        (Method::Fetch, CoapContentFormat::ApplicationCBOR) => {
            Some(ContentFormat::YangIdentifiersCbor)
        }
        (_, CoapContentFormat::ApplicationYangDataCbor)
        | (_, CoapContentFormat::ApplicationYangDataCborSid)
        | (_, CoapContentFormat::ApplicationYangDataCborName)
        | (_, CoapContentFormat::ApplicationCBOR) => Some(ContentFormat::YangDataCbor),
        (_, CoapContentFormat::ApplicationCborSeq) => Some(ContentFormat::YangInstancesCborSeq),
        _ => None,
    }
}

fn response_code_to_coap(code: ResponseCode) -> MessageClass {
    match code {
        ResponseCode::Created => MessageClass::Response(ResponseType::Created),
        ResponseCode::Changed => MessageClass::Response(ResponseType::Changed),
        ResponseCode::Content => MessageClass::Response(ResponseType::Content),
        ResponseCode::BadRequest => MessageClass::Response(ResponseType::BadRequest),
        ResponseCode::Unauthorized => MessageClass::Response(ResponseType::Unauthorized),
        ResponseCode::BadOption => MessageClass::Response(ResponseType::BadOption),
        ResponseCode::NotFound => MessageClass::Response(ResponseType::NotFound),
        ResponseCode::MethodNotAllowed => MessageClass::Response(ResponseType::MethodNotAllowed),
        ResponseCode::RequestEntityIncomplete => {
            MessageClass::Response(ResponseType::RequestEntityIncomplete)
        }
        ResponseCode::Conflict => MessageClass::Response(ResponseType::Conflict),
        ResponseCode::RequestEntityTooLarge => {
            MessageClass::Response(ResponseType::RequestEntityTooLarge)
        }
        ResponseCode::UnsupportedContentFormat => {
            MessageClass::Response(ResponseType::UnsupportedContentFormat)
        }
        ResponseCode::InternalServerError => {
            MessageClass::Response(ResponseType::InternalServerError)
        }
    }
}

fn ensure_success(packet: &Packet) -> Result<()> {
    if matches!(
        packet.header.code,
        MessageClass::Response(ResponseType::Created)
            | MessageClass::Response(ResponseType::Changed)
            | MessageClass::Response(ResponseType::Content)
    ) {
        return Ok(());
    }

    Err(CoreconfError::ValidationError(format!(
        "CoAP request failed with {:?}",
        packet.header.code
    )))
}

fn invalid_data(message: String) -> CoreconfError {
    CoreconfError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}
