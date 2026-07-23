//! Generic projected keyed-list FETCH coverage.

use coreconf_model::CompositeModel;
use coreconf_model::instance_id::{
    InstancePath, PathComponent, decode_instances_with_model, encode_identifiers,
};
use coreconf_runtime::coap_types::{ContentFormat, Method, Request};
use coreconf_runtime::{Datastore, RequestHandler};
use serde_json::json;

const SID: &str = r#"{
    "module-name":"example",
    "module-revision":"2026-01-01",
    "item":[
        {"identifier":"example","sid":60000},
        {"identifier":"/example:devices","sid":60001},
        {"identifier":"/example:devices/device","sid":60002},
        {"identifier":"/example:devices/device/id","sid":60003,"type":"string"},
        {"identifier":"/example:devices/device/enabled","sid":60004,"type":"boolean"},
        {"identifier":"/example:devices/device/name","sid":60005,"type":"string"}
    ],
    "key-mapping":{"60002":[60003]}
}"#;

#[test]
fn projected_fetch_returns_requested_leaf_for_each_keyed_instance() {
    let model = CompositeModel::from_sid_strings(&[SID]).expect("model");
    let datastore = Datastore::with_data(
        coreconf_model::CoreconfModel::from_sid_str(SID).expect("model"),
        json!({
            "example:devices": {
                "device": [
                    {"id": "a", "enabled": true, "name": "first"},
                    {"id": "b", "enabled": false, "name": "second"}
                ]
            }
        }),
    );
    let mut handler = RequestHandler::new(datastore);
    let mut request_payload = Vec::new();
    ciborium::into_writer(&60004_i64, &mut request_payload).expect("identifier");
    let request = Request::new(Method::Fetch)
        .with_payload(request_payload, ContentFormat::YangIdentifiersCbor);
    let response = handler.handle(&request);
    assert!(response.code.is_success());
    let instances =
        decode_instances_with_model(&model, &response.payload).expect("projected instances");
    assert_eq!(instances.len(), 2);
    assert!(instances.iter().all(|instance| {
        instance
            .path
            .components
            .iter()
            .any(|component| matches!(component, PathComponent::KeyValue(_)))
    }));
    let mut by_key = instances
        .iter()
        .map(|instance| {
            let key = instance
                .path
                .components
                .iter()
                .find_map(|component| match component {
                    PathComponent::KeyValue(value) => value.as_str().map(str::to_owned),
                    PathComponent::SidDelta(_) => None,
                })
                .expect("projected key")
                .to_owned();
            (key, instance.value.clone())
        })
        .collect::<Vec<_>>();
    by_key.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        by_key,
        [
            ("a".to_owned(), Some(json!(true))),
            ("b".to_owned(), Some(json!(false)))
        ]
    );
    assert!(instances.iter().all(
        |instance| instance.value == Some(json!(true)) || instance.value == Some(json!(false))
    ));
    assert_eq!(
        model.get_sid("/example:devices/device/enabled"),
        Some(60004)
    );
}

#[test]
fn model_aware_instance_decoder_preserves_string_and_integer_list_keys() {
    let string_model = CompositeModel::from_sid_strings(&[SID]).expect("string-key model");
    let mut string_path = InstancePath::new();
    string_path.push_delta(60000);
    string_path.push_delta(1);
    string_path.push_delta(1);
    string_path.push_key(serde_json::json!("device-a"));
    string_path.push_delta(1);
    let string_payload = encode_identifiers(&[string_path.clone()]).expect("string identifier");
    let string_instances =
        decode_instances_with_model(&string_model, &encode_instance_map(&string_path))
            .expect("string instance");
    assert_eq!(string_instances[0].path, string_path);
    assert_eq!(string_payload, encode_identifier_sequence(&[string_path]));

    let integer_sid = r#"{
        "module-name":"integer-example",
        "module-revision":"2026-01-01",
        "item":[
            {"identifier":"integer-example","sid":62000},
            {"identifier":"/integer-example:devices","sid":62001},
            {"identifier":"/integer-example:devices/device","sid":62002},
            {"identifier":"/integer-example:devices/device/id","sid":62003,"type":"uint16"},
            {"identifier":"/integer-example:devices/device/enabled","sid":62004,"type":"boolean"}
        ],
        "key-mapping":{"62002":[62003]}
    }"#;
    let integer_model =
        CompositeModel::from_sid_strings(&[integer_sid]).expect("integer-key model");
    let mut integer_path = InstancePath::new();
    integer_path.push_delta(62000);
    integer_path.push_delta(1);
    integer_path.push_delta(1);
    integer_path.push_key(serde_json::json!(20));
    integer_path.push_delta(1);
    let integer_payload = encode_instance_map(&integer_path);
    let integer_instances =
        decode_instances_with_model(&integer_model, &integer_payload).expect("integer instance");
    assert_eq!(integer_instances[0].path, integer_path);
}

#[test]
fn encode_identifiers_explicit_key_fetch_keeps_normal_wire_shape() {
    let sid = r#"{
        "module-name":"integer-example",
        "module-revision":"2026-01-01",
        "item":[
            {"identifier":"integer-example","sid":62000},
            {"identifier":"/integer-example:devices","sid":62001},
            {"identifier":"/integer-example:devices/device","sid":62002},
            {"identifier":"/integer-example:devices/device/id","sid":62003,"type":"uint16"},
            {"identifier":"/integer-example:devices/device/enabled","sid":62004,"type":"boolean"}
        ],
        "key-mapping":{"62002":[62003]}
    }"#;
    let model = CompositeModel::from_sid_strings(&[sid]).expect("model");
    let datastore = Datastore::with_data(
        coreconf_model::CoreconfModel::from_sid_str(sid).expect("coreconf model"),
        json!({
            "integer-example:devices": {
                "device": [{"id": 20, "enabled": true}]
            }
        }),
    );
    let mut handler = RequestHandler::new(datastore);
    let mut path = InstancePath::new();
    path.push_delta(62002);
    path.push_key(serde_json::json!(20));
    let payload = encode_identifiers(&[path]).expect("explicit identifier");
    let response = handler.handle(
        &Request::new(Method::Fetch).with_payload(payload, ContentFormat::YangIdentifiersCbor),
    );
    assert!(response.code.is_success());
    let instances = decode_instances_with_model(&model, &response.payload).expect("response");
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].path.components.len(), 3);
    assert_eq!(
        instances[0].path.components[2],
        PathComponent::KeyValue(json!(20))
    );
    assert_eq!(instances[0].value, Some(json!({"1": 20, "2": true})));
}

fn encode_instance_map(path: &InstancePath) -> Vec<u8> {
    let key = coreconf_model::codec::json_to_cbor_value(
        &CompositeModel::from_sid_strings(&[SID]).expect("model"),
        &path.to_cbor_value(),
        0,
    );
    let map = ciborium::value::Value::Map(vec![(key, ciborium::value::Value::Bool(true))]);
    let mut bytes = Vec::new();
    ciborium::into_writer(&map, &mut bytes).expect("instance map");
    bytes
}

fn encode_identifier_sequence(paths: &[InstancePath]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for path in paths {
        ciborium::into_writer(&path.to_cbor_value(), &mut bytes).expect("identifier");
    }
    bytes
}
