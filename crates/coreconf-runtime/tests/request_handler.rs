use std::sync::{Arc, Mutex};

use coreconf_model::CompositeModel;
use coreconf_runtime::coap_types::{ContentFormat, Method, Request, ResponseCode};
use coreconf_runtime::{
    Backend, Datastore, OperationBinding, OperationRegistry, RequestHandler, TransactionContext,
    TransactionParticipant,
};
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

fn binary_runtime_model() -> CompositeModel {
    CompositeModel::from_sid_strings(&[r#"{
        "module-name":"binary-example",
        "module-revision":"2026-01-01",
        "item":[
            {"identifier":"binary-example","sid":61000},
            {"identifier":"/binary-example:config","sid":61001},
            {"identifier":"/binary-example:config/profile","sid":61002},
            {"identifier":"/binary-example:config/profile/name","sid":61003,"type":"string"},
            {"identifier":"/binary-example:config/profile/blob","sid":61004,"type":"binary"},
            {"identifier":"/binary-example:config/enabled","sid":61005,"type":"boolean"}
        ],
        "key-mapping":{}
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
fn request_handler_root_ipatch_stores_binary_leaf_in_identifier_form() {
    let mut handler = RequestHandler::new(Datastore::new_in_memory(binary_runtime_model()));
    let request = Request::new(Method::IPatch).with_payload(
        encode_value(&json!({
            "61001": {
                "1": {"1": "primary", "2": [6, 7]},
                "4": true
            }
        })),
        ContentFormat::YangInstancesCborSeq,
    );

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::Changed);
    assert_eq!(
        handler
            .datastore()
            .get_path("/binary-example:config")
            .unwrap(),
        Some(json!({
            "profile": {"name": "primary", "blob": "Bgc="},
            "enabled": true
        }))
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

#[derive(Clone)]
struct RecordingBackend {
    tree: serde_json::Value,
    replacements: Arc<Mutex<Vec<serde_json::Value>>>,
    fail: bool,
}

impl Backend for RecordingBackend {
    fn read_tree(&self) -> serde_json::Value {
        self.tree.clone()
    }

    fn replace_tree(&mut self, next: serde_json::Value) -> coreconf_model::Result<()> {
        self.replacements.lock().unwrap().push(next.clone());
        if self.fail {
            return Err(coreconf_model::CoreconfError::Io(std::io::Error::other(
                "publication failed",
            )));
        }
        self.tree = next;
        Ok(())
    }
}

struct RecordingParticipant {
    events: Arc<Mutex<Vec<String>>>,
    reject: bool,
}

impl TransactionParticipant for RecordingParticipant {
    fn pre_commit(&self, context: &TransactionContext<'_>) -> coreconf_model::Result<()> {
        self.events.lock().unwrap().push(format!(
            "validate:{:?}:{:?}:{:?}:{:?}",
            context.request().method,
            context.previous_tree(),
            context.candidate_tree(),
            context.changed_paths()
        ));
        if self.reject {
            Err(coreconf_model::CoreconfError::ValidationError(
                "candidate rejected".into(),
            ))
        } else {
            Ok(())
        }
    }

    fn post_commit(&self, context: &TransactionContext<'_>) {
        self.events.lock().unwrap().push(format!(
            "post:{:?}:{:?}:{:?}:{:?}",
            context.request().method,
            context.previous_tree(),
            context.candidate_tree(),
            context.changed_paths()
        ));
    }
}

fn root_ipatch_request(payload: Vec<u8>) -> Request {
    Request::new(Method::IPatch).with_payload(payload, ContentFormat::YangInstancesCborSeq)
}

fn unknown_raw_request(method: Method, payload: Vec<u8>) -> Request {
    let mut request = Request::new(method);
    request.payload = payload;
    request.raw_content_format = Some(0xf123);
    request
}

fn root_ipatch_payload(values: &[serde_json::Value]) -> Vec<u8> {
    let mut payload = Vec::new();
    for value in values {
        payload.extend(encode_value(value));
    }
    payload
}

#[test]
fn unknown_raw_root_ipatch_is_rejected_before_commit() {
    let mut handler = RequestHandler::new(Datastore::new_in_memory(runtime_model()));
    let request = unknown_raw_request(
        Method::IPatch,
        root_ipatch_payload(&[json!({"60007": true})]),
    );

    let response = handler.handle(&request);

    assert_eq!(response.code, ResponseCode::UnsupportedContentFormat);
    assert_eq!(handler.datastore().get_all(), json!({}));
}

#[test]
fn unknown_raw_fetch_and_post_are_rejected_before_dispatch() {
    let mut handler = RequestHandler::new(Datastore::new_in_memory(runtime_model()));

    let fetch = handler.handle(&unknown_raw_request(
        Method::Fetch,
        encode_value(&json!(60007)),
    ));
    assert_eq!(fetch.code, ResponseCode::UnsupportedContentFormat);

    let post = handler.handle(&unknown_raw_request(
        Method::Post,
        root_ipatch_payload(&[json!({"60005": null})]),
    ));
    assert_eq!(post.code, ResponseCode::UnsupportedContentFormat);
    assert_eq!(handler.datastore().get_all(), json!({}));
}

#[test]
fn root_ipatch_success_publishes_complete_candidate_and_notifies_in_order() {
    let replacements = Arc::new(Mutex::new(Vec::new()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let backend = RecordingBackend {
        tree: json!({}),
        replacements: Arc::clone(&replacements),
        fail: false,
    };
    let mut handler = RequestHandler::new(Datastore::with_backend(runtime_model(), backend));
    handler.register_transaction_participant(Box::new(RecordingParticipant {
        events: Arc::clone(&events),
        reject: false,
    }));
    handler.register_transaction_participant(Box::new(RecordingParticipant {
        events: Arc::clone(&events),
        reject: false,
    }));

    let response = handler.handle(&root_ipatch_request(root_ipatch_payload(&[
        json!({"60007": true}),
        json!({"60007": false}),
    ])));

    assert_eq!(response.code, ResponseCode::Changed);
    assert_eq!(replacements.lock().unwrap().len(), 1);
    assert_eq!(
        handler
            .datastore()
            .get_path("/example:settings/enabled")
            .unwrap(),
        Some(json!(false))
    );
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 4);
    assert!(events[0].starts_with("validate:IPatch:"));
    assert!(events[1].starts_with("validate:IPatch:"));
    assert!(events[2].starts_with("post:IPatch:"));
    assert!(events[3].starts_with("post:IPatch:"));
    assert!(events[2].contains("false"));
    assert_eq!(events[2].matches("/example:settings/enabled").count(), 1);
}

#[test]
fn root_ipatch_late_edit_is_atomic_and_does_not_dirty_observers() {
    let mut handler = RequestHandler::new(Datastore::new_in_memory(runtime_model()));
    handler.register_observer(vec![1], ["60007".to_string()].into_iter().collect());

    let response = handler.handle(&root_ipatch_request(root_ipatch_payload(&[
        json!({"60007": true}),
        json!({"69999": false}),
    ])));

    assert_eq!(response.code, ResponseCode::Conflict);
    assert_eq!(handler.datastore().get_all(), json!({}));
    assert!(handler.pending_notifications(&[1]).is_empty());
}

#[test]
fn root_ipatch_validator_rejection_does_not_publish_or_notify() {
    let replacements = Arc::new(Mutex::new(Vec::new()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let backend = RecordingBackend {
        tree: json!({}),
        replacements: Arc::clone(&replacements),
        fail: false,
    };
    let mut handler = RequestHandler::new(Datastore::with_backend(runtime_model(), backend));
    handler.register_observer(vec![2], ["60007".to_string()].into_iter().collect());
    handler.register_transaction_participant(Box::new(RecordingParticipant {
        events: Arc::clone(&events),
        reject: true,
    }));

    let response = handler.handle(&root_ipatch_request(root_ipatch_payload(&[
        json!({"60007": true}),
    ])));

    assert_eq!(response.code, ResponseCode::Conflict);
    assert!(replacements.lock().unwrap().is_empty());
    assert_eq!(handler.datastore().get_all(), json!({}));
    assert!(handler.pending_notifications(&[2]).is_empty());
    assert_eq!(events.lock().unwrap().len(), 1);
}

#[test]
fn root_ipatch_publication_failure_has_no_post_commit_or_dirty_resources() {
    let replacements = Arc::new(Mutex::new(Vec::new()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let backend = RecordingBackend {
        tree: json!({}),
        replacements: Arc::clone(&replacements),
        fail: true,
    };
    let mut handler = RequestHandler::new(Datastore::with_backend(runtime_model(), backend));
    handler.register_observer(vec![3], ["60007".to_string()].into_iter().collect());
    handler.register_transaction_participant(Box::new(RecordingParticipant {
        events: Arc::clone(&events),
        reject: false,
    }));

    let response = handler.handle(&root_ipatch_request(root_ipatch_payload(&[
        json!({"60007": true}),
    ])));

    assert_eq!(response.code, ResponseCode::InternalServerError);
    assert_eq!(replacements.lock().unwrap().len(), 1);
    assert_eq!(events.lock().unwrap().len(), 1);
    assert!(handler.pending_notifications(&[3]).is_empty());
}
