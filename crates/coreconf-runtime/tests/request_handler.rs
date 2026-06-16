use std::sync::{Arc, Mutex};

use coreconf_model::CompositeModel;
use coreconf_runtime::coap_types::{ContentFormat, Method, Request, ResponseCode};
use coreconf_runtime::{Datastore, OperationBinding, OperationRegistry, RequestHandler};
use serde_json::json;

fn encode_value(value: &serde_json::Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).unwrap();
    bytes
}

fn encode_sid_value_at_path(
    model: &CompositeModel,
    canonical_path: &str,
    value: serde_json::Value,
) -> Vec<u8> {
    let sid_value = model
        .identifier_value_to_sid_value_at_path(value, canonical_path)
        .unwrap();
    encode_value(&sid_value)
}

fn decode_value(bytes: &[u8]) -> serde_json::Value {
    coreconf_model::codec::cbor_to_json_value(bytes).unwrap()
}

fn runtime_model() -> CompositeModel {
    CompositeModel::from_sid_strings(&[r#"{
        "module-name":"example",
        "module-revision":"2026-01-01",
        "item":[
            {"identifier":"example","sid":60000},
            {"identifier":"/example:devices","sid":60001},
            {"identifier":"/example:devices/device","sid":60002},
            {"identifier":"/example:devices/device/id","sid":60003,"type":"string"},
            {"identifier":"/example:devices/device/enabled","sid":60004,"type":"boolean"},
            {"identifier":"/example:devices/device/reset","sid":60005},
            {"identifier":"/example:settings","sid":60006},
            {"identifier":"/example:settings/enabled","sid":60007,"type":"boolean"}
        ],
        "key-mapping":{"60002":[60003]}
    }"#])
    .unwrap()
}

struct RecordingOperation {
    calls: Arc<Mutex<Vec<Option<serde_json::Value>>>>,
}

impl OperationBinding for RecordingOperation {
    fn canonical_path(&self) -> &str {
        "/example:devices/device/reset"
    }

    fn invoke(
        &self,
        input: Option<&serde_json::Value>,
    ) -> coreconf_model::Result<Option<serde_json::Value>> {
        self.calls.lock().unwrap().push(input.cloned());
        Ok(Some(json!({"status": "ok"})))
    }
}

#[test]
fn request_handler_applies_ipatch_to_predicate_path() {
    let datastore = Datastore::new_in_memory(runtime_model());
    let mut handler = RequestHandler::new(datastore);

    let request = Request::new(Method::IPatch)
        .with_path("/example:devices/device[id='rdc-1']/enabled")
        .with_payload(encode_value(&json!(true)), ContentFormat::YangDataCbor);

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::Changed);
    assert_eq!(
        handler
            .datastore()
            .get_path("/example:devices/device[id='rdc-1']/enabled")
            .unwrap(),
        Some(json!(true))
    );
}

#[test]
fn request_handler_decodes_path_ipatch_sid_object_before_storing() {
    let model = runtime_model();
    let datastore = Datastore::new_in_memory(model.clone());
    let mut handler = RequestHandler::new(datastore);

    let request = Request::new(Method::IPatch)
        .with_path("/example:devices/device[id='rdc-1']")
        .with_payload(
            encode_sid_value_at_path(&model, "/example:devices/device", json!({"enabled": true})),
            ContentFormat::YangDataCbor,
        );

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::Changed);
    assert_eq!(
        handler
            .datastore()
            .get_path("/example:devices/device[id='rdc-1']/enabled")
            .unwrap(),
        Some(json!(true))
    );
}

#[test]
fn request_handler_rejects_scalar_root_ipatch_payload() {
    let datastore = Datastore::new_in_memory(runtime_model());
    let mut handler = RequestHandler::new(datastore);

    let request = Request::new(Method::IPatch).with_payload(
        encode_value(&json!(true)),
        ContentFormat::YangInstancesCborSeq,
    );

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::BadRequest);
}

#[test]
fn request_handler_rejects_empty_root_ipatch_payload() {
    let datastore = Datastore::new_in_memory(runtime_model());
    let mut handler = RequestHandler::new(datastore);

    let request = Request::new(Method::IPatch).with_payload(
        encode_value(&json!({})),
        ContentFormat::YangInstancesCborSeq,
    );

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::BadRequest);
}

#[test]
fn request_handler_applies_valid_root_ipatch_instance() {
    let datastore = Datastore::new_in_memory(runtime_model());
    let mut handler = RequestHandler::new(datastore);

    let request = Request::new(Method::IPatch).with_payload(
        encode_value(&json!({"60007": true})),
        ContentFormat::YangInstancesCborSeq,
    );

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::Changed);
    assert_eq!(
        handler
            .datastore()
            .get_path("/example:settings/enabled")
            .unwrap(),
        Some(json!(true))
    );
}

#[test]
fn request_handler_dispatches_post_using_canonical_predicate_path() {
    let datastore = Datastore::new_in_memory(runtime_model());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut operations = OperationRegistry::default();
    operations.register(Box::new(RecordingOperation {
        calls: Arc::clone(&calls),
    }));
    let mut handler = RequestHandler::with_operations(datastore, operations);

    let request = Request::new(Method::Post)
        .with_path("/example:devices/device[id='rdc-1']/reset")
        .with_payload(
            encode_value(&json!({"reason": "test"})),
            ContentFormat::YangDataCbor,
        );

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::Content);
    assert_eq!(response.content_format, Some(ContentFormat::YangDataCbor));
    assert_eq!(decode_value(&response.payload), json!({"status": "ok"}));
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[Some(json!({"reason": "test"}))]
    );
}

#[test]
fn request_handler_rejects_malformed_fetch_payloads() {
    let datastore = Datastore::new_in_memory(runtime_model());
    let mut handler = RequestHandler::new(datastore);

    let payload = encode_value(&json!([60004, {"unexpected": true}]));

    let request =
        Request::new(Method::Fetch).with_payload(payload, ContentFormat::YangIdentifiersCbor);

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::BadRequest);
}

#[test]
fn request_handler_deletes_existing_path() {
    let datastore = Datastore::new_in_memory(runtime_model());
    let mut handler = RequestHandler::new(datastore);
    handler
        .datastore_mut()
        .set_path("/example:devices/device[id='rdc-1']/enabled", json!(true))
        .unwrap();

    let request =
        Request::new(Method::Delete).with_path("/example:devices/device[id='rdc-1']/enabled");

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::Changed);
    assert_eq!(
        handler
            .datastore()
            .get_path("/example:devices/device[id='rdc-1']/enabled")
            .unwrap(),
        None
    );
}

#[test]
fn request_handler_delete_missing_path_returns_not_found() {
    let datastore = Datastore::new_in_memory(runtime_model());
    let mut handler = RequestHandler::new(datastore);

    let request =
        Request::new(Method::Delete).with_path("/example:devices/device[id='rdc-1']/enabled");

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::NotFound);
}
