//! Integration tests using embedded SID data
//!
//! Note: Previous tests relied on external pycoreconf samples directory.
//! These self-contained tests ensure the library works standalone.

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

    // Encode to CORECONF
    let cbor = model.to_coreconf(SAMPLE_JSON).expect("Failed to encode");
    println!("CBOR hex: {}", hex::encode(&cbor));
    assert!(!cbor.is_empty());

    // Decode back
    let json = model.to_json(&cbor).expect("Failed to decode");
    println!("Decoded: {}", json);

    let decoded: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded["example-1:greeting"]["author"], "Obi");
    assert_eq!(decoded["example-1:greeting"]["message"], "Hello there!");
}

#[test]
fn test_handler_get() {
    let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
    let datastore = Datastore::from_json(model, SAMPLE_JSON).unwrap();
    let mut handler = RequestHandler::new(datastore);

    let request = Request::new(Method::Get);
    let response = handler.handle(&request);

    assert!(response.code.is_success());
    assert!(!response.payload.is_empty());
    println!("GET response: {} bytes", response.payload.len());
}

#[test]
fn test_handler_fetch() {
    let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
    let datastore = Datastore::from_json(model.clone(), SAMPLE_JSON).unwrap();
    let mut handler = RequestHandler::new(datastore);
    let builder = RequestBuilder::new(model);

    // Build FETCH request for author field (SID 60002)
    let payload = builder.build_fetch_sids(&[60002]).unwrap();
    let request =
        Request::new(Method::Fetch).with_payload(payload, ContentFormat::YangIdentifiersCbor);

    let response = handler.handle(&request);

    assert!(response.code.is_success());
    println!("FETCH response: {} bytes", response.payload.len());
}

#[test]
fn test_handler_ipatch() {
    let model: CoreconfModel = SAMPLE_SID.parse().unwrap();
    let datastore = Datastore::from_json(model.clone(), SAMPLE_JSON).unwrap();
    let mut handler = RequestHandler::new(datastore);
    let builder = RequestBuilder::new(model);

    // Modify author
    let payload = builder
        .build_ipatch_sids(&[(60002, Some(serde_json::json!("General Kenobi")))])
        .unwrap();

    let request =
        Request::new(Method::IPatch).with_payload(payload, ContentFormat::YangInstancesCborSeq);

    let response = handler.handle(&request);
    assert!(response.code.is_success());

    // Verify change
    let value = handler.datastore().get_by_sid(60002).unwrap();
    assert_eq!(value, Some(serde_json::json!("General Kenobi")));
}
