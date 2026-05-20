use coreconf_model::instance_id::decode_instances;
use coreconf_model::{CoreconfError, Instance, InstancePath, Result};
use serde_json::Value;

use std::collections::{HashMap, HashSet};

use crate::coap_types::{ContentFormat, Interface, Method, Request, Response, ResponseCode};
use crate::datastore::Datastore;
use crate::operations::{OperationBinding, OperationRegistry};
use crate::path::PredicatePath;

/// A registered CoAP observer identified by its token.
#[derive(Debug, Clone)]
pub struct Observer {
    pub token: Vec<u8>,
    /// Resource paths (or SID strings) this observer is watching.
    pub resources: HashSet<String>,
}

pub struct RequestHandler {
    datastore: Datastore,
    operations: OperationRegistry,
    /// Observe sequence counter (incremented on each notification).
    observe_sequence: u32,
    /// Registered observers keyed by token.
    observers: HashMap<Vec<u8>, Observer>,
    /// Resources that have changed since last notification.
    dirty_resources: HashSet<String>,
}

impl RequestHandler {
    pub fn new(datastore: Datastore) -> Self {
        Self {
            datastore,
            operations: OperationRegistry::default(),
            observe_sequence: 0,
            observers: HashMap::new(),
            dirty_resources: HashSet::new(),
        }
    }

    pub fn with_operations(datastore: Datastore, operations: OperationRegistry) -> Self {
        Self {
            datastore,
            operations,
            observe_sequence: 0,
            observers: HashMap::new(),
            dirty_resources: HashSet::new(),
        }
    }

    pub fn register_operation(&mut self, binding: Box<dyn OperationBinding>) {
        self.operations.register(binding);
    }

    pub fn datastore(&self) -> &Datastore {
        &self.datastore
    }

    pub fn datastore_mut(&mut self) -> &mut Datastore {
        &mut self.datastore
    }

    /// Register an observer (token → observed resources).
    ///
    /// Called automatically on FETCH+Observe.  The `resources` set
    /// identifies which data paths the observer cares about.
    pub fn register_observer(&mut self, token: Vec<u8>, resources: HashSet<String>) {
        self.observers
            .entry(token.clone())
            .and_modify(|obs| obs.resources.extend(resources.iter().cloned()))
            .or_insert(Observer { token, resources });
    }

    /// Remove an observer by token (e.g. on Observe=1 or connection close).
    pub fn deregister_observer(&mut self, token: &[u8]) {
        self.observers.remove(token);
    }

    /// Mark a resource path as changed, so registered observers will be
    /// notified on the next poll.  Converts identifier paths to SID strings
    /// so they match the SIDs observers registered via FETCH.
    pub fn mark_changed(&mut self, path: &str) {
        let parsed = match PredicatePath::parse(path) {
            Ok(p) => p,
            Err(_) => {
                self.dirty_resources.insert(path.to_string());
                return;
            }
        };
        let canonical = parsed.canonical_path.trim_start_matches('/');
        let segments: Vec<&str> = canonical.split('/').filter(|s| !s.is_empty()).collect();
        
        let mut found_any = false;
        for end in 1..=segments.len() {
            let candidate = format!("/{}", segments[..end].join("/"));
            if let Some(sid) = self.datastore.model().get_sid(&candidate) {
                self.dirty_resources.insert(sid.to_string());
                found_any = true;
            }
        }
        
        if !found_any {
            self.dirty_resources.insert(path.to_string());
        }
    }

    /// Collect pending notifications for a given observer token.
    ///
    /// Returns a list of (resource, sequence) pairs that need to be sent.
    /// The caller (CoAP transport) is responsible for encoding and sending
    /// the actual response packets.
    pub fn pending_notifications(&mut self, token: &[u8]) -> Vec<(String, u32)> {
        let Some(observer) = self.observers.get(token) else {
            return Vec::new();
        };

        let mut notifications = Vec::new();
        let dirty: Vec<String> = self
            .dirty_resources
            .iter()
            .filter(|r| observer.resources.contains(*r))
            .cloned()
            .collect();

        for resource in &dirty {
            let seq = self.observe_sequence;
            self.observe_sequence = self.observe_sequence.wrapping_add(1);
            notifications.push((resource.clone(), seq));
        }

        // Clear only the resources THIS observer was notified about.
        for resource in &dirty {
            // Only clear if no other observer is watching this resource.
            let still_watched = self
                .observers
                .values()
                .any(|obs| obs.token != token && obs.resources.contains(resource));
            if !still_watched {
                self.dirty_resources.remove(resource);
            }
        }

        notifications
    }

    pub fn handle(&mut self, request: &Request) -> Response {
        // Streaming interface (`/s`) only accepts FETCH+Observe.
        if request.interface == Some(Interface::Streaming) {
            return self.handle_streaming(request);
        }

        match request.method {
            Method::Get => self.handle_get(request),
            Method::Fetch => self.handle_fetch(request),
            Method::IPatch => self.handle_ipatch(request),
            Method::Post => self.handle_post(request),
        }
    }

    /// Handle a request on the streaming interface (`/s`).
    ///
    /// Only FETCH is permitted; observe is optional but typical for `/s`.
    fn handle_streaming(&mut self, request: &Request) -> Response {
        if request.method != Method::Fetch {
            return Response::error(
                ResponseCode::MethodNotAllowed,
                &format!("{} not allowed on streaming interface", request.method),
            );
        }

        let mut response = self.handle_fetch(request);

        // Register for observe if the client requested it (Observe=0).
        // Deregister on Observe=1 (client wants to stop).
        if let Some(observe_val) = request.observe {
            // Use the real CoAP token when available; fall back to \xc0
            // for programmatic/test usage where no token is set.
            let token = if request.token.is_empty() {
                b"\xc0".to_vec()
            } else {
                request.token.clone()
            };
            if observe_val == 0 {
                // Register: extract which SIDs/resources are being watched.
                if let Ok(identifiers) = self.parse_fetch_request(&request.payload) {
                    let resources: HashSet<String> = identifiers
                        .into_iter()
                        .map(|(sid, _keys)| sid.to_string())
                        .collect();
                    self.register_observer(token, resources);
                }
            } else if observe_val == 1 {
                self.deregister_observer(&token);
            }

            if response.code.is_success() {
                let seq = self.observe_sequence;
                self.observe_sequence = self.observe_sequence.wrapping_add(1);
                response.observe = Some(seq);
            }
        }

        response
    }

    fn handle_get(&self, request: &Request) -> Response {
        if request.path.is_empty() {
            return match self.datastore.get_all_cbor() {
                Ok(cbor) => {
                    let filtered = apply_query_filters(&cbor, &request.query);
                    Response::content(filtered, ContentFormat::YangDataCbor)
                }
                Err(error) => {
                    Response::error(ResponseCode::InternalServerError, &error.to_string())
                }
            };
        }

        match self.datastore.get_path(&request.path) {
            Ok(Some(value)) => {
                let parsed = match PredicatePath::parse(&request.path) {
                    Ok(p) => p,
                    Err(error) => return Response::error(ResponseCode::BadRequest, &error.to_string()),
                };
                let sid_val = match self.datastore.model().identifier_value_to_sid_value_at_path(value, &parsed.canonical_path) {
                    Ok(v) => v,
                    Err(error) => return Response::error(ResponseCode::InternalServerError, &error.to_string()),
                };
                match encode_json_value(self.datastore.model(), &sid_val) {
                    Ok(payload) => {
                        let filtered = apply_query_filters(&payload, &request.query);
                        Response::content(filtered, ContentFormat::YangDataCbor)
                    }
                    Err(error) => {
                        Response::error(ResponseCode::InternalServerError, &error.to_string())
                    }
                }
            }
            Ok(None) => Response::not_found(&request.path),
            Err(error) => Response::error(ResponseCode::BadRequest, &error.to_string()),
        }
    }

    fn handle_fetch(&self, request: &Request) -> Response {
        if !request.path.is_empty() {
            return self.handle_get(request);
        }

        if let Some(format) = request.content_format
            && format != ContentFormat::YangIdentifiersCbor
            && format != ContentFormat::YangDataCbor
        {
            return Response::error(
                ResponseCode::UnsupportedContentFormat,
                "expected yang-identifiers+cbor",
            );
        }

        if request.payload.is_empty() {
            return self.handle_get(request);
        }

        match self.parse_fetch_request(&request.payload) {
            Ok(identifiers) => {
                let mut instances = Vec::with_capacity(identifiers.len());
                for (sid, key_values) in identifiers {
                    let value = if key_values.is_empty() {
                        let identifier = self
                            .datastore
                            .model()
                            .get_identifier(sid)
                            .ok_or(CoreconfError::IdentifierNotFound(sid));
                        match identifier {
                            Ok(id) => self.datastore.get_path(id).ok().flatten(),
                            Err(_) => None,
                        }
                    } else {
                        // Build predicate path from instance ID with keys.
                        match self.datastore.create_xpath(sid, &key_values) {
                            Ok(xpath) => self.datastore.get_path(&xpath).ok().flatten(),
                            Err(_) => None,
                        }
                    };

                    if let Some(value) = value {
                        let mut path = InstancePath::new();
                        path.push_delta(sid);
                        instances.push(Instance::new(path, value));
                    }
                }

                match self.datastore.encode_instances(&instances) {
                    Ok(payload) => Response::content(payload, ContentFormat::YangInstancesCborSeq),
                    Err(error) => {
                        Response::error(ResponseCode::InternalServerError, &error.to_string())
                    }
                }
            }
            Err(error) => Response::error(ResponseCode::BadRequest, &error.to_string()),
        }
    }

    fn handle_ipatch(&mut self, request: &Request) -> Response {
        if !request.path.is_empty() {
            if request.content_format != Some(ContentFormat::YangDataCbor) {
                return Response::error(
                    ResponseCode::UnsupportedContentFormat,
                    "expected yang-data+cbor",
                );
            }

            return match decode_json_value(&request.payload)
                .and_then(|value| self.datastore.set_path(&request.path, value))
            {
                Ok(()) => {
                    self.mark_changed(&request.path);
                    Response::changed()
                }
                Err(error) => Response::error(ResponseCode::Conflict, &error.to_string()),
            };
        }

        if let Some(format) = request.content_format
            && format != ContentFormat::YangInstancesCborSeq
            && format != ContentFormat::YangDataCbor
        {
            return Response::error(
                ResponseCode::UnsupportedContentFormat,
                "expected yang-instances+cbor-seq",
            );
        }

        match decode_instances(&request.payload) {
            Ok(instances) => {
                for instance in instances {
                    let Some(sid) = instance.path.absolute_sid() else {
                        continue;
                    };
                    let mut keys = Vec::new();
                    for component in &instance.path.components {
                        if let coreconf_model::instance_id::PathComponent::KeyValue(val) = component {
                            keys.push(val.clone());
                        }
                    }
                    let xpath = match self.datastore.create_xpath(sid, &keys) {
                        Ok(xp) => xp,
                        Err(error) => return Response::error(ResponseCode::Conflict, &error.to_string()),
                    };
                    let parsed_xpath = match PredicatePath::parse(&xpath) {
                        Ok(p) => p,
                        Err(error) => return Response::error(ResponseCode::Conflict, &error.to_string()),
                    };
                    let converted_value = match instance.value {
                        Some(value) => {
                            match self.datastore.model().sid_value_to_identifier_value_at_path(value, &parsed_xpath.canonical_path) {
                                Ok(v) => Some(v),
                                Err(error) => return Response::error(ResponseCode::Conflict, &error.to_string()),
                            }
                        }
                        None => None,
                    };
                    let result = match converted_value {
                        Some(value) => self.datastore.set_path(&xpath, value),
                        None => self.datastore.delete_path(&xpath).map(|_| ()),
                    };
                    if let Err(error) = result {
                        return Response::error(ResponseCode::Conflict, &error.to_string());
                    }
                    self.mark_changed(&xpath);
                }
                Response::changed()
            }
            Err(error) => Response::error(ResponseCode::BadRequest, &error.to_string()),
        }
    }

    fn handle_post(&mut self, request: &Request) -> Response {
        let invocation = if !request.path.is_empty() {
            self.invoke_operation_path(
                &request.path,
                request.payload.as_slice(),
                request.content_format,
            )
        } else {
            self.handle_post_instances(request)
        };

        match invocation {
            Ok(Some(value)) => match encode_json_value(self.datastore.model(), &value) {
                Ok(payload) => Response::content(payload, ContentFormat::YangDataCbor),
                Err(error) => {
                    Response::error(ResponseCode::InternalServerError, &error.to_string())
                }
            },
            Ok(None) => Response::changed(),
            Err(error) => {
                if matches!(error, CoreconfError::ResourceNotFound(_)) {
                    Response::not_found(&error.to_string())
                } else {
                    Response::error(ResponseCode::BadRequest, &error.to_string())
                }
            }
        }
    }

    fn handle_post_instances(&self, request: &Request) -> Result<Option<Value>> {
        if let Some(format) = request.content_format
            && format != ContentFormat::YangInstancesCborSeq
        {
            return Err(CoreconfError::UnsupportedContentFormat);
        }

        let mut last = None;
        for instance in decode_instances(&request.payload)? {
            let Some(sid) = instance.path.absolute_sid() else {
                continue;
            };
            let mut keys = Vec::new();
            for component in &instance.path.components {
                if let coreconf_model::instance_id::PathComponent::KeyValue(val) = component {
                    keys.push(val.clone());
                }
            }
            let xpath = self.datastore.create_xpath(sid, &keys)?;
            let parsed_xpath = PredicatePath::parse(&xpath)?;
            let converted_value = match instance.value {
                Some(value) => {
                    Some(self.datastore.model().sid_value_to_identifier_value_at_path(value, &parsed_xpath.canonical_path)?)
                }
                None => None,
            };
            let result = self
                .operations
                .invoke(&xpath, converted_value.as_ref())?;
            last = match result {
                Some(val) => {
                    Some(self.datastore.model().identifier_value_to_sid_value_at_path(val, &parsed_xpath.canonical_path)?)
                }
                None => None,
            };
        }
        Ok(last)
    }

    fn invoke_operation_path(
        &self,
        path: &str,
        payload: &[u8],
        content_format: impl Into<Option<ContentFormat>>,
    ) -> Result<Option<Value>> {
        let parsed = PredicatePath::parse(path)?;
        let input = match content_format.into() {
            Some(ContentFormat::YangDataCbor) if !payload.is_empty() => {
                let sid_val = decode_json_value(payload)?;
                Some(self.datastore.model().sid_value_to_identifier_value_at_path(sid_val, &parsed.canonical_path)?)
            }
            Some(ContentFormat::YangInstancesCborSeq) => None,
            Some(_) => return Err(CoreconfError::UnsupportedContentFormat),
            None if payload.is_empty() => None,
            None => {
                let sid_val = decode_json_value(payload)?;
                Some(self.datastore.model().sid_value_to_identifier_value_at_path(sid_val, &parsed.canonical_path)?)
            }
        };

        let result = self.operations
            .invoke(&parsed.canonical_path, input.as_ref())?;
        match result {
            Some(val) => {
                let sid_val = self.datastore.model().identifier_value_to_sid_value_at_path(val, &parsed.canonical_path)?;
                Ok(Some(sid_val))
            }
            None => Ok(None),
        }
    }

    fn parse_fetch_request(&self, payload: &[u8]) -> Result<Vec<(i64, Vec<Value>)>> {
        let mut identifiers = Vec::new();
        let mut cursor = std::io::Cursor::new(payload);

        while (cursor.position() as usize) < payload.len() {
            let value: Value = ciborium::from_reader(&mut cursor)
                .map_err(|error| CoreconfError::CborDecode(error.to_string()))?;

            identifiers.push(parse_fetch_identifier(&value)?);
        }

        Ok(identifiers)
    }
}

/// Parse a FETCH identifier: `sid` (bare SID) or `[sid, key1, key2, ...]`
/// (instance ID with list-key values).
fn parse_fetch_identifier(value: &Value) -> Result<(i64, Vec<Value>)> {
    match value {
        Value::Number(number) => {
            let sid = number
                .as_i64()
                .ok_or_else(|| CoreconfError::TypeConversion("expected integer SID".into()))?;
            Ok((sid, Vec::new()))
        }
        Value::Array(values) => parse_fetch_identifier_array(values),
        _ => Err(CoreconfError::TypeConversion(
            "invalid FETCH identifier format".into(),
        )),
    }
}

fn parse_fetch_identifier_array(values: &[Value]) -> Result<(i64, Vec<Value>)> {
    if values.is_empty() {
        return Err(CoreconfError::TypeConversion(
            "invalid FETCH identifier format".into(),
        ));
    }

    // First element is always a SID delta.
    let delta = values[0].as_i64().ok_or_else(|| {
        CoreconfError::TypeConversion("expected SID delta in FETCH identifier".into())
    })?;
    let absolute_sid = delta;

    // Subsequent elements are key values (even if they happen to be integers —
    // identityref keys are encoded as integer SIDs in CBOR).
    let key_values: Vec<Value> = values[1..]
        .iter()
        .filter(|v| is_supported_fetch_key_value(v))
        .cloned()
        .collect();

    if key_values.len() != values.len() - 1 {
        return Err(CoreconfError::TypeConversion(
            "unsupported key value in FETCH identifier".into(),
        ));
    }

    Ok((absolute_sid, key_values))
}

fn is_supported_fetch_key_value(value: &Value) -> bool {
    matches!(value, Value::Bool(_) | Value::Number(_) | Value::String(_))
}

fn encode_json_value(model: &coreconf_model::CompositeModel, value: &Value) -> Result<Vec<u8>> {
    let ciborium_val = coreconf_model::codec::json_to_cbor_value(model, value, 0);
    let mut bytes = Vec::new();
    ciborium::into_writer(&ciborium_val, &mut bytes)
        .map_err(|error| CoreconfError::CborEncode(error.to_string()))?;
    Ok(bytes)
}

fn decode_json_value(payload: &[u8]) -> Result<Value> {
    coreconf_model::codec::cbor_to_json_value(payload)
}

/// Apply `c=` (content) and `d=` (defaults) query filters to a CBOR payload.
///
/// Currently returns the payload unchanged because:
/// - The datastore has a single schema tree (no candidate/running/startup split),
///   so all `c=` values behave identically.
/// - SID files do not carry leaf default values, so `d=t` (trim defaults) cannot
///   be applied yet.
///
/// When multi-datastore support and default-value tracking are added, this
/// function will perform the filtering as specified in RFC 9595 § 4.1.
fn apply_query_filters(payload: &[u8], _query: &crate::coap_types::QueryParams) -> Vec<u8> {
    payload.to_vec()
}
