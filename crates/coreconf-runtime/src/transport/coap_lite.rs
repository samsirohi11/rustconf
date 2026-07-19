use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

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
const BLOCK1_TRANSFER_TTL: Duration = Duration::from_secs(30);

pub trait CoreconfClient {
    fn fetch_snapshot(&mut self) -> Result<Value>;
    fn fetch_path(&mut self, _path: &str) -> Result<Option<Value>> {
        Err(CoreconfError::ValidationError(
            "path GET is not supported by this client".into(),
        ))
    }
    fn apply_patch(&mut self, patch: &[(String, Option<Value>)]) -> Result<()>;
    fn discover(&mut self, _query: Option<&str>) -> Result<String> {
        Err(CoreconfError::ValidationError(
            "discovery is not supported by this client".into(),
        ))
    }
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

    fn send_discovery_request(&mut self, query: Option<&str>) -> Result<Packet> {
        let mut packet = Packet::new();
        packet.header.message_id = self.next_message_id;
        self.next_message_id = self.next_message_id.wrapping_add(1);
        packet.header.code = MessageClass::Request(RequestType::Get);
        packet.header.set_type(MessageType::Confirmable);
        packet.set_token(vec![0xC0]);
        add_uri_path(&mut packet, "/.well-known/core");
        if let Some(query) = query.filter(|q| !q.is_empty()) {
            packet.add_option(CoapOption::UriQuery, query.as_bytes().to_vec());
        }
        self.send_packet(packet)
    }
}

impl CoreconfClient for CoapLiteClient {
    fn discover(&mut self, query: Option<&str>) -> Result<String> {
        let response = self.send_discovery_request(query)?;
        ensure_success(&response)?;
        String::from_utf8(response.payload).map_err(|error| invalid_data(error.to_string()))
    }

    fn fetch_snapshot(&mut self) -> Result<Value> {
        let response = self.send_coreconf_request(RequestType::Get, None, Vec::new(), None)?;
        ensure_success(&response)?;
        let json = coreconf_model::decode_cbor_to_json(&self.model, &response.payload)?;
        serde_json::from_str(&json).map_err(CoreconfError::from)
    }

    fn fetch_path(&mut self, path: &str) -> Result<Option<Value>> {
        let response =
            self.send_coreconf_request(RequestType::Get, Some(path), Vec::new(), None)?;
        if matches!(
            response.header.code,
            MessageClass::Response(ResponseType::NotFound)
        ) {
            return Ok(None);
        }
        ensure_success(&response)?;
        let sid_value = coreconf_model::codec::cbor_to_json_value(&response.payload)?;
        let parsed = crate::PredicatePath::parse(path)?;
        self.model
            .sid_value_to_identifier_value_at_path(sid_value, &parsed.canonical_path)
            .map(Some)
    }

    fn apply_patch(&mut self, patch: &[(String, Option<Value>)]) -> Result<()> {
        for (path, value) in patch {
            let Some(value) = value else {
                let response =
                    self.send_coreconf_request(RequestType::Delete, Some(path), Vec::new(), None)?;
                ensure_success(&response)?;
                continue;
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
    block1_transfers: HashMap<Block1Key, PendingBlock1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Block1Key {
    peer: SocketAddr,
    token: Vec<u8>,
    uri_path: String,
}

#[derive(Debug)]
struct PendingBlock1 {
    payload: Vec<u8>,
    next_num: u16,
    updated_at: Instant,
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
            block1_transfers: HashMap::new(),
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

        let packet = match self.reassemble_block1(packet, peer) {
            Block1Outcome::Complete(packet) => packet,
            Block1Outcome::Respond(response) => return response,
        };

        let request = packet_to_request(&packet, &self.resource_path);
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
        response_to_packet(&packet, response)
    }

    fn reassemble_block1(&mut self, packet: &Packet, peer: SocketAddr) -> Block1Outcome {
        self.expire_block1_transfers();
        let Some(block1) = packet
            .get_first_option_as::<BlockValue>(CoapOption::Block1)
            .and_then(|value| value.ok())
        else {
            return Block1Outcome::Complete(packet.clone());
        };

        let key = Block1Key {
            peer,
            token: packet.get_token().to_vec(),
            uri_path: uri_path(packet),
        };

        if block1.num == 0 {
            let pending = PendingBlock1 {
                payload: packet.payload.clone(),
                next_num: 1,
                updated_at: Instant::now(),
            };

            if block1.more {
                self.block1_transfers.insert(key, pending);
                return Block1Outcome::Respond(block1_response(
                    packet,
                    ResponseType::Continue,
                    block1,
                ));
            }

            let mut complete = packet.clone();
            complete.clear_option(CoapOption::Block1);
            return Block1Outcome::Complete(complete);
        }

        let Some(pending) = self.block1_transfers.get_mut(&key) else {
            return Block1Outcome::Respond(block1_response(
                packet,
                ResponseType::RequestEntityIncomplete,
                block1,
            ));
        };

        if pending.next_num != block1.num {
            self.block1_transfers.remove(&key);
            return Block1Outcome::Respond(block1_response(
                packet,
                ResponseType::RequestEntityIncomplete,
                block1,
            ));
        }

        pending.payload.extend_from_slice(&packet.payload);
        pending.next_num = pending.next_num.saturating_add(1);
        pending.updated_at = Instant::now();

        if block1.more {
            return Block1Outcome::Respond(block1_response(packet, ResponseType::Continue, block1));
        }

        let Some(pending) = self.block1_transfers.remove(&key) else {
            return Block1Outcome::Respond(block1_response(
                packet,
                ResponseType::RequestEntityIncomplete,
                block1,
            ));
        };
        let mut complete = packet.clone();
        complete.payload = pending.payload;
        complete.clear_option(CoapOption::Block1);
        Block1Outcome::Complete(complete)
    }

    fn expire_block1_transfers(&mut self) {
        let now = Instant::now();
        self.block1_transfers
            .retain(|_, pending| now.duration_since(pending.updated_at) <= BLOCK1_TRANSFER_TTL);
    }

    /// Build a CoRE Link Format response for `/.well-known/core`.
    fn well_known_core_response(&self, request: &Packet) -> Packet {
        let (management_path, streaming_path) = advertised_paths(&self.resource_path);
        let links = format!(
            "</{management_path}>;rt=\"core.c.ds\";ct=112,</{streaming_path}>;rt=\"core.c.ev\";ct=141;obs"
        );
        let mut packet = Packet::new();
        packet.header.message_id = request.header.message_id;
        packet.header.set_type(immediate_response_type(request));
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
                // Re-fetch the full datastore as CORECONF/SID CBOR.
                let notification_payload = self.handler.datastore().get_all_cbor();

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

enum Block1Outcome {
    Complete(Packet),
    Respond(Packet),
}

fn block1_response(request: &Packet, code: ResponseType, block1: BlockValue) -> Packet {
    let mut packet = Packet::new();
    packet.header.message_id = request.header.message_id;
    packet.header.set_type(immediate_response_type(request));
    packet.set_token(request.get_token().to_vec());
    packet.header.code = MessageClass::Response(code);
    packet.add_option_as(CoapOption::Block1, block1);
    packet
}

pub fn packet_to_request(
    packet: &Packet,
    resource_path: &str,
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
    let segments: Vec<&str> = uri_path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let resource_segments = resource_segments(resource_path);
    let Some((interface, consumed)) = route_coreconf_path(&segments, &resource_segments) else {
        let path = if uri_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{uri_path}")
        };
        return Err(Response::not_found(&path));
    };

    let path = path_from_segments(&segments, consumed);
    let mut request =
        Request::new(method).with_path(if path.is_empty() { String::new() } else { path });
    request.payload = packet.payload.clone();
    request.raw_content_format = raw_content_format(packet);
    request.content_format = match request.raw_content_format {
        Some(raw) => content_format_from_raw(method, raw),
        None => packet
            .get_content_format()
            .and_then(|format| content_format_from_coap(method, format))
            .or_else(|| default_content_format(method, &request.payload)),
    };
    request.query = uri_query(packet);

    request.interface = Some(interface);

    // Parse CoAP Observe option.
    if let Some(Ok(observe_value)) = packet.get_observe_value() {
        request.observe = Some(observe_value);
    }

    // Extract CoAP token for observer tracking.
    request.token = packet.get_token().to_vec();

    Ok(request)
}

fn path_from_segments(segments: &[&str], consumed: usize) -> String {
    let remaining = &segments[consumed..];
    if remaining.is_empty() {
        String::new()
    } else {
        format!("/{}", remaining.join("/"))
    }
}

fn resource_segments(path: &str) -> Vec<&str> {
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        vec!["c"]
    } else {
        segments
    }
}

fn route_coreconf_path(segments: &[&str], resource: &[&str]) -> Option<(Interface, usize)> {
    if resource == ["c"] {
        match segments.first().copied() {
            Some("c") => return Some((Interface::Management, 1)),
            Some("s") => return Some((Interface::Streaming, 1)),
            _ => return None,
        }
    }

    if segments.len() < resource.len() || !segments.starts_with(resource) {
        return None;
    }

    let mut consumed = resource.len();
    let interface = if segments.get(consumed).copied() == Some("s") {
        consumed += 1;
        Interface::Streaming
    } else {
        Interface::Management
    };
    Some((interface, consumed))
}

fn advertised_paths(resource_path: &str) -> (String, String) {
    let management_path = resource_segments(resource_path).join("/");
    let streaming_path = if management_path == "c" {
        "s".to_string()
    } else {
        format!("{management_path}/s")
    };
    (management_path, streaming_path)
}

pub fn response_to_packet(request: &Packet, response: Response) -> Packet {
    let mut packet = Packet::new();
    packet.header.message_id = request.header.message_id;
    packet.header.set_type(immediate_response_type(request));
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

fn immediate_response_type(request: &Packet) -> MessageType {
    match request.header.get_type() {
        MessageType::Confirmable => MessageType::Acknowledgement,
        MessageType::NonConfirmable => MessageType::NonConfirmable,
        // Only CON and NON packets are CoAP requests.  Preserve the existing
        // default type for malformed input rather than expanding server
        // handling beyond valid request packets.
        MessageType::Acknowledgement | MessageType::Reset => MessageType::Confirmable,
    }
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
    if bytes.is_empty() {
        Some(0)
    } else if bytes.len() == 1 {
        Some(u16::from(bytes[0]))
    } else if bytes.len() == 2 {
        Some(u16::from_be_bytes([bytes[0], bytes[1]]))
    } else {
        None
    }
}

/// Map a raw content-format number to a known semantic `ContentFormat`.
///
/// Known CORECONF values retain their method-specific interpretation.  An
/// unknown raw value deliberately remains semantically unrecognised.
fn content_format_from_raw(method: Method, raw: u16) -> Option<ContentFormat> {
    match (method, raw) {
        (Method::Fetch, 60) => Some(ContentFormat::YangIdentifiersCbor),
        (_, 60) => Some(ContentFormat::YangDataCbor),
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
        ContentFormat::YangDataCbor => CoapContentFormat::ApplicationYangDataCborSid, // 140
        ContentFormat::YangIdentifiersCbor => CoapContentFormat::ApplicationCBOR,     // 60
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

    fn request_packet(method: RequestType, path: &str) -> Packet {
        let mut packet = Packet::new();
        packet.header.code = MessageClass::Request(method);
        add_uri_path(&mut packet, path);
        packet
    }

    fn blockwise_ipatch_packet(path: &str, payload: Vec<u8>, num: usize, more: bool) -> Packet {
        let mut packet = request_packet(RequestType::IPatch, path);
        packet.set_token(vec![0x44]);
        packet.payload = payload;
        packet.set_content_format(content_format_to_coap(ContentFormat::YangDataCbor));
        packet.add_option_as(CoapOption::Block1, BlockValue::new(num, more, 16).unwrap());
        packet
    }

    fn small_model() -> CompositeModel {
        CompositeModel::from_sid_strings(&[r#"{
            "module-name":"example",
            "module-revision":"2026-01-01",
            "item":[
                {"identifier":"example","sid":60000},
                {"identifier":"/example:settings","sid":60001},
                {"identifier":"/example:settings/text","sid":60002,"type":"string"}
            ],
            "key-mapping":{}
        }"#])
        .unwrap()
    }

    #[test]
    fn unknown_raw_content_format_is_preserved_without_semantic_default() {
        let mut packet = request_packet(RequestType::IPatch, "/c/example:settings");
        packet.payload = vec![0xa0];
        packet.add_option(CoapOption::ContentFormat, vec![0xf1, 0x23]);

        let request = packet_to_request(&packet, "c").unwrap();

        assert_eq!(request.raw_content_format, Some(0xf123));
        assert_eq!(request.content_format, None);
    }

    #[test]
    fn explicit_raw_coap_enum_value_does_not_use_enum_fallback() {
        let mut packet = request_packet(RequestType::IPatch, "/c/example:settings");
        packet.payload = vec![0xa0];
        packet.add_option(CoapOption::ContentFormat, vec![0x01, 0x54]);

        let request = packet_to_request(&packet, "c").unwrap();

        assert_eq!(request.raw_content_format, Some(340));
        assert_eq!(request.content_format, None);
    }

    #[test]
    fn raw_application_cbor_keeps_method_specific_semantics() {
        for (method, expected) in [
            (RequestType::Fetch, ContentFormat::YangIdentifiersCbor),
            (RequestType::IPatch, ContentFormat::YangDataCbor),
            (RequestType::Post, ContentFormat::YangDataCbor),
        ] {
            let mut packet = request_packet(method, "/c");
            packet.payload = vec![0xa0];
            packet.add_option(CoapOption::ContentFormat, vec![60]);

            let request = packet_to_request(&packet, "c").unwrap();

            assert_eq!(request.raw_content_format, Some(60));
            assert_eq!(request.content_format, Some(expected));
        }
    }

    #[test]
    fn standard_content_format_preserves_raw_value_and_semantics() {
        let mut packet = request_packet(RequestType::Fetch, "/c");
        packet.payload = vec![0xa0];
        packet.add_option(CoapOption::ContentFormat, vec![0x8d]);

        let request = packet_to_request(&packet, "c").unwrap();

        assert_eq!(request.raw_content_format, Some(141));
        assert_eq!(
            request.content_format,
            Some(ContentFormat::YangIdentifiersCbor)
        );
    }

    #[test]
    fn request_payload_builder_records_standard_raw_content_format() {
        let request =
            Request::new(Method::IPatch).with_payload(vec![0xa0], ContentFormat::YangDataCbor);

        assert_eq!(request.raw_content_format, Some(142));
        assert_eq!(request.content_format, Some(ContentFormat::YangDataCbor));
    }

    #[test]
    fn unknown_raw_root_ipatch_cannot_reach_handler() {
        let handler = RequestHandler::new(crate::Datastore::new_in_memory(small_model()));
        let mut server = CoapLiteServer::bind("127.0.0.1:0", "c", handler).unwrap();
        let peer: SocketAddr = "127.0.0.1:56831".parse().unwrap();
        let mut packet = request_packet(RequestType::IPatch, "/c");
        packet.payload = {
            let mut payload = Vec::new();
            ciborium::into_writer(&serde_json::json!({"60002": "changed"}), &mut payload).unwrap();
            payload
        };
        packet.add_option(CoapOption::ContentFormat, vec![0xf1, 0x23]);

        let response = server.handle_packet(&packet, peer);

        assert!(matches!(
            response.header.code,
            MessageClass::Response(ResponseType::UnsupportedContentFormat)
        ));
        assert_eq!(
            server
                .handler()
                .datastore()
                .get_path("/example:settings/text")
                .unwrap(),
            None
        );
    }

    #[test]
    fn packet_to_request_rejects_unknown_coreconf_root() {
        let packet = request_packet(RequestType::Get, "/foo");

        let error = packet_to_request(&packet, "c").unwrap_err();

        assert_eq!(error.code, ResponseCode::NotFound);
    }

    #[test]
    fn packet_to_request_maps_default_management_root() {
        let packet = request_packet(RequestType::Get, "/c/example:settings");

        let request = packet_to_request(&packet, "c").unwrap();

        assert_eq!(request.interface, Some(Interface::Management));
        assert_eq!(request.path, "/example:settings");
    }

    #[test]
    fn packet_to_request_maps_default_streaming_root() {
        let packet = request_packet(RequestType::Fetch, "/s/events");

        let request = packet_to_request(&packet, "c").unwrap();

        assert_eq!(request.interface, Some(Interface::Streaming));
        assert_eq!(request.path, "/events");
    }

    #[test]
    fn packet_to_request_honors_custom_management_root() {
        let packet = request_packet(RequestType::Get, "/mgmt/example:settings");

        let request = packet_to_request(&packet, "mgmt").unwrap();

        assert_eq!(request.interface, Some(Interface::Management));
        assert_eq!(request.path, "/example:settings");
    }

    #[test]
    fn well_known_core_advertises_custom_resource_path() {
        let handler = RequestHandler::new(crate::Datastore::new_in_memory(small_model()));
        let server = CoapLiteServer::bind("127.0.0.1:0", "mgmt", handler).unwrap();
        let request = request_packet(RequestType::Get, "/.well-known/core");

        let response = server.well_known_core_response(&request);
        let payload = String::from_utf8(response.payload).unwrap();

        assert!(payload.contains("</mgmt>;rt=\"core.c.ds\""));
        assert!(payload.contains("</mgmt/s>;rt=\"core.c.ev\""));
    }

    #[test]
    fn handle_packet_reassembles_block1_ipatch_before_dispatch() {
        let handler = RequestHandler::new(crate::Datastore::new_in_memory(small_model()));
        let mut server = CoapLiteServer::bind("127.0.0.1:0", "c", handler).unwrap();
        let peer: SocketAddr = "127.0.0.1:56830".parse().unwrap();
        let value = serde_json::json!("abcdefghijklmnopqrstuvwxyz");
        let mut payload = Vec::new();
        ciborium::into_writer(&value, &mut payload).unwrap();
        let (first, second) = payload.split_at(16);

        let first_response = server.handle_packet(
            &blockwise_ipatch_packet("/c/example:settings/text", first.to_vec(), 0, true),
            peer,
        );
        assert!(matches!(
            first_response.header.code,
            MessageClass::Response(ResponseType::Continue)
        ));

        let second_response = server.handle_packet(
            &blockwise_ipatch_packet("/c/example:settings/text", second.to_vec(), 1, false),
            peer,
        );
        assert!(matches!(
            second_response.header.code,
            MessageClass::Response(ResponseType::Changed)
        ));
        assert_eq!(
            server
                .handler()
                .datastore()
                .get_path("/example:settings/text")
                .unwrap(),
            Some(value)
        );
    }

    fn assert_handle_packet_wire_response(
        request_type: MessageType,
        expected_response_type: MessageType,
        message_id: u16,
        token: Vec<u8>,
        expected_wire_header: &[u8],
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let handler = RequestHandler::new(crate::Datastore::new_in_memory(small_model()));
        let mut server = CoapLiteServer::bind("127.0.0.1:0", "c", handler)?;
        let peer: SocketAddr = "127.0.0.1:56832".parse()?;
        let mut request = request_packet(RequestType::Get, "/c");
        request.header.message_id = message_id;
        request.header.set_type(request_type);
        request.set_token(token.clone());

        let request_wire = request.to_bytes()?;
        let received_request = Packet::from_bytes(&request_wire)?;
        let response = server.handle_packet(&received_request, peer);

        assert_eq!(
            response.header.code,
            MessageClass::Response(ResponseType::Content)
        );
        assert_eq!(response.header.get_type(), expected_response_type);
        assert_eq!(response.header.message_id, message_id);
        assert_eq!(response.get_token(), token.as_slice());

        let response_wire = response.to_bytes()?;
        assert_eq!(
            &response_wire[..expected_wire_header.len()],
            expected_wire_header
        );

        let received_response = Packet::from_bytes(&response_wire)?;
        assert_eq!(
            received_response.header.code,
            MessageClass::Response(ResponseType::Content)
        );
        assert_eq!(received_response.header.get_type(), expected_response_type);
        assert_eq!(received_response.header.message_id, message_id);
        assert_eq!(received_response.get_token(), token.as_slice());
        assert_eq!(received_response.to_bytes()?, response_wire);
        Ok(())
    }

    #[test]
    fn handle_packet_serializes_acknowledgement_for_confirmable_request()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_handle_packet_wire_response(
            MessageType::Confirmable,
            MessageType::Acknowledgement,
            0x1234,
            vec![0xa1, 0xb2],
            &[0x62, 0x45, 0x12, 0x34, 0xa1, 0xb2],
        )
    }

    #[test]
    fn handle_packet_serializes_nonconfirmable_for_nonconfirmable_request()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_handle_packet_wire_response(
            MessageType::NonConfirmable,
            MessageType::NonConfirmable,
            0xabcd,
            vec![0xde, 0xad, 0xbe],
            &[0x53, 0x45, 0xab, 0xcd, 0xde, 0xad, 0xbe],
        )
    }
}
