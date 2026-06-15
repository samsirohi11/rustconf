use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
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
    SCHC_MANAGEMENT_CONTENT_FORMAT,
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
    /// Maps observer token → peer address for unsolicited notifications.
    observer_peers: HashMap<Vec<u8>, SocketAddr>,
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
            observer_peers: HashMap::new(),
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

    /// Handle a packet from a known peer.  Associates the peer with any
    /// observer registration so unsolicited notifications can be sent back.
    pub fn handle_packet(&mut self, packet: &Packet, peer: SocketAddr) -> Packet {
        // ── /.well-known/core resource discovery ──────────────────────
        let uri = uri_path(packet);
        if uri.trim_start_matches('/') == ".well-known/core" {
            return self.well_known_core_response(packet);
        }

        let request = packet_to_request(packet, &self.resource_path);
        let response = match request {
            Ok(ref req) => {
                // Track peer for observer registration (Observe=0 on /s).
                if req.observe == Some(0) && !req.token.is_empty() {
                    self.observer_peers.insert(req.token.clone(), peer);
                }
                // Remove on Observe=1 (client deregisters).
                if req.observe == Some(1) && !req.token.is_empty() {
                    self.observer_peers.remove(&req.token);
                }
                self.handler.handle(req)
            }
            Err(response) => response,
        };
        response_to_packet(packet, response)
    }

    /// Build a CoRE Link Format response for `/.well-known/core`.
    fn well_known_core_response(&self, request: &Packet) -> Packet {
        let links = "</c>;rt=\"core.c.ds\";ct=112,</s>;rt=\"core.c.ev\";ct=141;obs".to_string();
        let mut packet = Packet::new();
        packet.header.message_id = request.header.message_id;
        packet.set_token(request.get_token().to_vec());
        packet.header.code = MessageClass::Response(ResponseType::Content);
        packet.payload = links.into_bytes();
        packet.set_content_format(CoapContentFormat::TextPlain);
        packet
    }

    /// Send pending notifications to all registered observers.
    /// Call after each request so observers receive push updates promptly.
    pub fn flush_pending_notifications(&mut self) {
        // Snapshot peer list — pending_notifications takes &mut self.
        let peers: Vec<(Vec<u8>, SocketAddr)> = self
            .observer_peers
            .iter()
            .map(|(t, p)| (t.clone(), *p))
            .collect();

        for (token, peer) in &peers {
            let pending = self.handler.pending_notifications(token);
            for (_resource, sequence) in pending {
                // Re-fetch the full datastore as the notification payload
                // (simplest correct approach — observers get the current state).
                let value = self.handler.datastore().get_all();
                let notification_payload = encode_notification(&value);

                if let Ok(payload) = notification_payload {
                    let response =
                        Response::observe(payload, ContentFormat::YangDataCbor, sequence);
                    // Build a non-confirmable notification packet.
                    let mut packet = Packet::new();
                    packet.header.message_id = 0;
                    packet.header.set_type(MessageType::NonConfirmable);
                    packet.set_token(token.clone());
                    packet.header.code = response_code_to_coap(response.code);
                    if !response.payload.is_empty() {
                        packet.payload = response.payload;
                        if let Some(format) = response.content_format {
                            packet.set_content_format(content_format_to_coap(format));
                        }
                    }
                    if let Some(seq) = response.observe {
                        packet.set_observe_value(seq);
                    }
                    if let Ok(bytes) = packet.to_bytes() {
                        let _ = self.socket.send_to(&bytes, *peer);
                    }
                }
            }
        }
    }

    pub fn serve_once(&mut self) -> Result<()> {
        let mut buffer = [0u8; 1500];
        let (len, peer) = self.socket.recv_from(&mut buffer)?;
        let packet =
            Packet::from_bytes(&buffer[..len]).map_err(|error| invalid_data(error.to_string()))?;
        let response = self.handle_packet(&packet, peer);
        let bytes = response
            .to_bytes()
            .map_err(|error| invalid_data(error.to_string()))?;
        self.socket.send_to(&bytes, peer)?;
        self.flush_pending_notifications();
        Ok(())
    }
}

/// Encode a JSON value as CBOR for an observe notification payload.
fn encode_notification(value: &serde_json::Value) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes)
        .map_err(|error| CoreconfError::CborEncode(error.to_string()))?;
    Ok(bytes)
}

pub fn packet_to_request(
    packet: &Packet,
    _resource_path: &str,
) -> std::result::Result<Request, Response> {
    let method = match packet.header.code {
        MessageClass::Request(RequestType::Get) => Method::Get,
        MessageClass::Request(RequestType::Post) => Method::Post,
        MessageClass::Request(RequestType::Delete) => Method::Delete,
        MessageClass::Request(RequestType::Fetch) => Method::Fetch,
        MessageClass::Request(RequestType::Patch) | MessageClass::Request(RequestType::IPatch) => {
            Method::IPatch
        }
        _ => {
            return Err(Response::method_not_allowed(Method::Get));
        }
    };

    let uri_path = uri_path(packet);
    let trimmed = uri_path.trim_start_matches('/');

    // Detect CORECONF interface from the first URI path segment (`c` or `s`).
    // This works regardless of the configured resource_path — both /c and /s
    // are always served.
    let (remaining, interface) = if let Some((first, rest)) = trimmed.split_once('/') {
        if let Some(iface) = Interface::from_uri_segment(first) {
            (rest.to_string(), Some(iface))
        } else {
            (trimmed.to_string(), None)
        }
    } else if let Some(iface) = Interface::from_uri_segment(trimmed) {
        (String::new(), Some(iface))
    } else {
        (trimmed.to_string(), None)
    };

    let path = if remaining.is_empty() {
        String::new()
    } else {
        format!("/{remaining}")
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

    // Extract CoAP token for observer tracking.
    request.token = packet.get_token().to_vec();

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
        Method::Get | Method::Delete => None,
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
        // SCHC CORECONF M-rules use a dedicated management payload
        // content-format. Preserve the method semantics expected by the
        // runtime handlers.
        (Method::Fetch, SCHC_MANAGEMENT_CONTENT_FORMAT) => Some(ContentFormat::YangIdentifiersCbor),
        (Method::IPatch, SCHC_MANAGEMENT_CONTENT_FORMAT) => Some(ContentFormat::YangDataCbor),
        (Method::Post, SCHC_MANAGEMENT_CONTENT_FORMAT) => Some(ContentFormat::YangInstancesCborSeq),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_schc_management_content_format_by_method() {
        assert_eq!(
            content_format_from_raw(Method::Fetch, SCHC_MANAGEMENT_CONTENT_FORMAT),
            Some(ContentFormat::YangIdentifiersCbor)
        );
        assert_eq!(
            content_format_from_raw(Method::IPatch, SCHC_MANAGEMENT_CONTENT_FORMAT),
            Some(ContentFormat::YangDataCbor)
        );
        assert_eq!(
            content_format_from_raw(Method::Post, SCHC_MANAGEMENT_CONTENT_FORMAT),
            Some(ContentFormat::YangInstancesCborSeq)
        );
    }
}
