use coreconf_model::instance_id::decode_instances;
use coreconf_model::{CoreconfError, Instance, InstancePath, Result};
use serde_json::Value;

use crate::coap_types::{ContentFormat, Method, Request, Response, ResponseCode};
use crate::datastore::Datastore;
use crate::operations::{OperationBinding, OperationRegistry};
use crate::path::PredicatePath;

pub struct RequestHandler {
    datastore: Datastore,
    operations: OperationRegistry,
}

impl RequestHandler {
    pub fn new(datastore: Datastore) -> Self {
        Self {
            datastore,
            operations: OperationRegistry::default(),
        }
    }

    pub fn with_operations(datastore: Datastore, operations: OperationRegistry) -> Self {
        Self {
            datastore,
            operations,
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

    pub fn handle(&mut self, request: &Request) -> Response {
        match request.method {
            Method::Get => self.handle_get(request),
            Method::Fetch => self.handle_fetch(request),
            Method::IPatch => self.handle_ipatch(request),
            Method::Post => self.handle_post(request),
        }
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
            Ok(Some(value)) => match encode_json_value(&value) {
                Ok(payload) => {
                    let filtered = apply_query_filters(&payload, &request.query);
                    Response::content(filtered, ContentFormat::YangDataCbor)
                }
                Err(error) => {
                    Response::error(ResponseCode::InternalServerError, &error.to_string())
                }
            },
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
            Ok(sids) => {
                let mut instances = Vec::with_capacity(sids.len());
                for sid in sids {
                    if let Some(identifier) = self.datastore.model().get_identifier(sid)
                        && let Ok(Some(value)) = self.datastore.get_path(identifier)
                    {
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
                Ok(()) => Response::changed(),
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
                    let Some(identifier) = self.datastore.model().get_identifier(sid) else {
                        return Response::error(
                            ResponseCode::Conflict,
                            &CoreconfError::IdentifierNotFound(sid).to_string(),
                        );
                    };
                    let identifier = identifier.to_string();
                    let result = match instance.value {
                        Some(value) => self.datastore.set_path(&identifier, value),
                        None => self.datastore.delete_path(&identifier).map(|_| ()),
                    };
                    if let Err(error) = result {
                        return Response::error(ResponseCode::Conflict, &error.to_string());
                    }
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
            Ok(Some(value)) => match encode_json_value(&value) {
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
            let identifier = self
                .datastore
                .model()
                .get_identifier(sid)
                .ok_or(CoreconfError::IdentifierNotFound(sid))?
                .to_string();
            last = self
                .operations
                .invoke(&identifier, instance.value.as_ref())?;
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
                Some(decode_json_value(payload)?)
            }
            Some(ContentFormat::YangInstancesCborSeq) => None,
            Some(_) => return Err(CoreconfError::UnsupportedContentFormat),
            None if payload.is_empty() => None,
            None => Some(decode_json_value(payload)?),
        };

        self.operations
            .invoke(&parsed.canonical_path, input.as_ref())
    }

    fn parse_fetch_request(&self, payload: &[u8]) -> Result<Vec<i64>> {
        let mut sids = Vec::new();
        let mut cursor = std::io::Cursor::new(payload);

        while (cursor.position() as usize) < payload.len() {
            let value: Value = ciborium::from_reader(&mut cursor)
                .map_err(|error| CoreconfError::CborDecode(error.to_string()))?;

            sids.push(parse_fetch_identifier(&value)?);
        }

        Ok(sids)
    }
}

fn parse_fetch_identifier(value: &Value) -> Result<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .ok_or_else(|| CoreconfError::TypeConversion("expected integer SID".into())),
        Value::Array(values) => parse_fetch_identifier_array(values),
        _ => Err(CoreconfError::TypeConversion(
            "invalid FETCH identifier format".into(),
        )),
    }
}

fn parse_fetch_identifier_array(values: &[Value]) -> Result<i64> {
    if values.is_empty() {
        return Err(CoreconfError::TypeConversion(
            "invalid FETCH identifier format".into(),
        ));
    }

    let mut absolute_sid = 0i64;
    let mut index = 0usize;

    while index < values.len() {
        let delta = values[index].as_i64().ok_or_else(|| {
            CoreconfError::TypeConversion("expected SID delta in FETCH identifier".into())
        })?;
        absolute_sid += delta;
        index += 1;

        while index < values.len() && values[index].as_i64().is_none() {
            if !is_supported_fetch_key_value(&values[index]) {
                return Err(CoreconfError::TypeConversion(
                    "unsupported key value in FETCH identifier".into(),
                ));
            }
            index += 1;
        }
    }

    Ok(absolute_sid)
}

fn is_supported_fetch_key_value(value: &Value) -> bool {
    matches!(value, Value::Bool(_) | Value::Number(_) | Value::String(_))
}

fn encode_json_value(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes)
        .map_err(|error| CoreconfError::CborEncode(error.to_string()))?;
    Ok(bytes)
}

fn decode_json_value(payload: &[u8]) -> Result<Value> {
    ciborium::from_reader(payload).map_err(|error| CoreconfError::CborDecode(error.to_string()))
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
