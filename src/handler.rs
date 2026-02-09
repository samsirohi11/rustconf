//! CORECONF Request Handler
//!
//! Coap library-agnostic request handling for CORECONF operations.
//! This is the core of the library - plug into any CoAP server.

use crate::coap_types::{ContentFormat, Method, Request, Response, ResponseCode};
use crate::datastore::Datastore;
use crate::error::{CoreconfError, Result};
use crate::instance_id::{Instance, InstancePath, decode_instances, encode_instances};
use serde_json::Value;

/// Main CORECONF request handler
///
/// This handler processes CORECONF requests and returns responses.
///
/// # Example
/// ```ignore
/// let handler = RequestHandler::new(datastore);
/// let response = handler.handle(&request);
/// // Send response via your CoAP transport
/// ```
#[derive(Debug)]
pub struct RequestHandler {
    /// The datastore containing YANG data
    datastore: Datastore,
}

impl RequestHandler {
    /// Create a new request handler with the given datastore
    pub fn new(datastore: Datastore) -> Self {
        Self { datastore }
    }

    /// Get a reference to the datastore
    pub fn datastore(&self) -> &Datastore {
        &self.datastore
    }

    /// Get a mutable reference to the datastore
    pub fn datastore_mut(&mut self) -> &mut Datastore {
        &mut self.datastore
    }

    /// Handle an incoming CORECONF request
    pub fn handle(&mut self, request: &Request) -> Response {
        match request.method {
            Method::Get => self.handle_get(request),
            Method::Fetch => self.handle_fetch(request),
            Method::IPatch => self.handle_ipatch(request),
            Method::Post => self.handle_post(request),
        }
    }

    /// Handle GET request - retrieve full datastore
    fn handle_get(&self, _request: &Request) -> Response {
        match self.datastore.get_all_cbor() {
            Ok(cbor) => Response::content(cbor, ContentFormat::YangDataCbor),
            Err(e) => Response::error(ResponseCode::InternalServerError, &e.to_string()),
        }
    }

    /// Handle FETCH request - retrieve specific data nodes
    ///
    /// Request payload: application/yang-identifiers+cbor (SID sequence)
    /// Response payload: application/yang-instances+cbor-seq
    fn handle_fetch(&self, request: &Request) -> Response {
        // Validate content format
        if let Some(format) = request.content_format
            && format != ContentFormat::YangIdentifiersCbor
            && format != ContentFormat::YangDataCbor
        {
            return Response::error(
                ResponseCode::UnsupportedContentFormat,
                "expected yang-identifiers+cbor",
            );
        }

        // Empty payload = return all data
        if request.payload.is_empty() {
            return self.handle_get(request);
        }

        // Parse requested SIDs from payload
        match self.parse_fetch_request(&request.payload) {
            Ok(sids) => {
                let mut instances = Vec::with_capacity(sids.len());

                for sid in sids {
                    let mut path = InstancePath::new();
                    path.push_delta(sid);

                    match self.datastore.get_by_sid(sid) {
                        Ok(Some(value)) => {
                            instances.push(Instance::new(path, value));
                        }
                        Ok(None) => {
                            // Node not found, skip or return error
                        }
                        Err(_) => {
                            // SID not in model, skip
                        }
                    }
                }

                match encode_instances(&instances) {
                    Ok(cbor) => Response::content(cbor, ContentFormat::YangInstancesCborSeq),
                    Err(e) => Response::error(ResponseCode::InternalServerError, &e.to_string()),
                }
            }
            Err(e) => Response::error(ResponseCode::BadRequest, &e.to_string()),
        }
    }

    /// Parse FETCH request payload (CBOR sequence of SIDs)
    fn parse_fetch_request(&self, payload: &[u8]) -> Result<Vec<i64>> {
        let mut sids = Vec::new();
        let mut cursor = std::io::Cursor::new(payload);

        while (cursor.position() as usize) < payload.len() {
            let value: Value = ciborium::from_reader(&mut cursor)
                .map_err(|e| CoreconfError::CborDecode(e.to_string()))?;

            match value {
                Value::Number(n) => {
                    if let Some(sid) = n.as_i64() {
                        sids.push(sid);
                    }
                }
                Value::Array(arr) => {
                    // Instance identifier with keys
                    if let Some(first) = arr.first()
                        && let Some(sid) = first.as_i64()
                    {
                        sids.push(sid);
                    }
                }
                _ => {}
            }
        }

        Ok(sids)
    }

    /// Handle iPATCH request - modify data nodes
    ///
    /// Request payload: application/yang-instances+cbor-seq
    /// Each instance: {SID: value} or {SID: null} for delete
    fn handle_ipatch(&mut self, request: &Request) -> Response {
        // Validate content format
        if let Some(format) = request.content_format
            && format != ContentFormat::YangInstancesCborSeq
            && format != ContentFormat::YangDataCbor
        {
            return Response::error(
                ResponseCode::UnsupportedContentFormat,
                "expected yang-instances+cbor-seq",
            );
        }

        // Parse instances from payload
        match decode_instances(&request.payload) {
            Ok(instances) => {
                for instance in instances {
                    if let Some(sid) = instance.path.absolute_sid() {
                        let result = match instance.value {
                            Some(value) => self.datastore.set_by_sid(sid, value),
                            None => self.datastore.delete_by_sid(sid).map(|_| ()),
                        };

                        if let Err(e) = result {
                            return Response::error(ResponseCode::Conflict, &e.to_string());
                        }
                    }
                }
                Response::changed()
            }
            Err(e) => Response::error(ResponseCode::BadRequest, &e.to_string()),
        }
    }

    /// Handle POST request - invoke RPC or Action
    ///
    /// Request payload: application/yang-instances+cbor-seq with {SID: input}
    /// Response payload: application/yang-instances+cbor-seq with {SID: output}
    fn handle_post(&mut self, request: &Request) -> Response {
        // Validate content format
        if let Some(format) = request.content_format
            && format != ContentFormat::YangInstancesCborSeq
        {
            return Response::error(
                ResponseCode::UnsupportedContentFormat,
                "expected yang-instances+cbor-seq",
            );
        }

        // Parse RPC call from payload
        match decode_instances(&request.payload) {
            Ok(instances) => {
                // For now, just acknowledge the RPC
                // Actual RPC implementation would dispatch to registered handlers
                let mut results = Vec::new();

                for instance in &instances {
                    if let Some(sid) = instance.path.absolute_sid() {
                        // Check if this SID is an RPC in the model
                        if let Some(_identifier) =
                            self.datastore.model().sid_file.get_identifier(sid)
                        {
                            // Return null output (RPC completed with no output)
                            let mut result_path = InstancePath::new();
                            result_path.push_delta(sid);
                            results.push(Instance::delete(result_path)); // null = no output
                        } else {
                            return Response::not_found(&format!("RPC SID {}", sid));
                        }
                    }
                }

                match encode_instances(&results) {
                    Ok(cbor) => Response {
                        code: ResponseCode::Changed,
                        payload: cbor,
                        content_format: Some(ContentFormat::YangInstancesCborSeq),
                    },
                    Err(e) => Response::error(ResponseCode::InternalServerError, &e.to_string()),
                }
            }
            Err(e) => Response::error(ResponseCode::BadRequest, &e.to_string()),
        }
    }
}

/// RPC handler trait for custom RPC implementations
pub trait RpcHandler {
    /// Handle an RPC invocation
    fn handle(&self, input: Option<&Value>) -> Result<Option<Value>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coreconf::CoreconfModel;

    const SAMPLE_SID: &str = r#"{
        "assignment-range": [{"entry-point": 60000, "size": 10}],
        "module-name": "example-1",
        "module-revision": "unknown",
        "item": [
            {"namespace": "module", "identifier": "example-1", "sid": 60000},
            {"namespace": "data", "identifier": "/example-1:greeting", "sid": 60001},
            {"namespace": "data", "identifier": "/example-1:greeting/author", "sid": 60002, "type": "string"},
            {"namespace": "data", "identifier": "/example-1:greeting/message", "sid": 60003, "type": "string"}
        ],
        "key-mapping": {}
    }"#;

    fn create_handler() -> RequestHandler {
        let model = CoreconfModel::from_str(SAMPLE_SID).unwrap();
        let json = r#"{"example-1:greeting": {"author": "Obi", "message": "Hello!"}}"#;
        let datastore = Datastore::from_json(model, json).unwrap();
        RequestHandler::new(datastore)
    }

    #[test]
    fn test_handle_get() {
        let mut handler = create_handler();
        let request = Request::new(Method::Get);
        let response = handler.handle(&request);

        assert!(response.code.is_success());
        assert!(!response.payload.is_empty());
    }

    #[test]
    fn test_handle_ipatch() {
        let mut handler = create_handler();

        // Build iPATCH request to modify author
        let mut path = InstancePath::new();
        path.push_delta(60002);
        let instance = Instance::new(path, Value::String("Luke".into()));
        let payload = encode_instances(&[instance]).unwrap();

        let request =
            Request::new(Method::IPatch).with_payload(payload, ContentFormat::YangInstancesCborSeq);

        let response = handler.handle(&request);
        assert_eq!(response.code, ResponseCode::Changed);

        // Verify the change
        let value = handler.datastore().get_by_sid(60002).unwrap();
        assert_eq!(value, Some(Value::String("Luke".into())));
    }
}
