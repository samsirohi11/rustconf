use rust_coreconf::coap_types::{ContentFormat, Method, Request};
use rust_coreconf::{CoreconfModel, Datastore, RequestBuilder, RequestHandler};

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

const SAMPLE_JSON: &str = r#"{"example-1:greeting": {"author": "Obi", "message": "Hello there!"}}"#;

#[test]
fn test_coreconf_roundtrip() {
    let model: CoreconfModel = SAMPLE_SID.parse().expect("Failed to parse SID");
    let cbor = model.to_coreconf(SAMPLE_JSON).expect("Failed to encode");
    let json = model.to_json(&cbor).expect("Failed to decode");
    let decoded: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded["example-1:greeting"]["author"], "Obi");
}

#[test]
fn test_handler_get() {
    let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
    let datastore = Datastore::from_json(model, SAMPLE_JSON).unwrap();
    let mut handler = RequestHandler::new(datastore);
    let response = handler.handle(&Request::new(Method::Get));
    assert!(response.code.is_success());
    assert!(!response.payload.is_empty());
}

#[test]
fn test_handler_fetch() {
    let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
    let datastore = Datastore::from_json(model.clone(), SAMPLE_JSON).unwrap();
    let mut handler = RequestHandler::new(datastore);
    let builder = RequestBuilder::new(model);
    let payload = builder.build_fetch_sids(&[60002]).unwrap();
    let request =
        Request::new(Method::Fetch).with_payload(payload, ContentFormat::YangIdentifiersCbor);
    let response = handler.handle(&request);
    assert!(response.code.is_success());
}
