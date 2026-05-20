//! CORECONF protocol-level tests using the coreconf-m2m weather-station
//! SID model as a rich fixture with identityref keys, enumerations, nested
//! containers, and keyed lists.
//!
//! These tests validate the full CORECONF operations — GET, FETCH, iPATCH,
//! instance-ID encoding, predicate paths, `/c` vs `/s` interface routing,
//! CoAP Observe, and CBOR↔datastore roundtrips — on a realistic YANG model
//! without depending on any external device or Python library.
//!
//! The same operations work identically with any other YANG SID file.

use coreconf_model::{CompositeModel, CoreconfModel, SidFile};
use coreconf_runtime::Datastore;
use coreconf_runtime::RequestHandler;
use coreconf_runtime::coap_types::{ContentFormat, Interface, Method, Request};
use serde_json::json;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// shared test fixtures
// ---------------------------------------------------------------------------

static M2M_SID_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();

fn m2m_sid_path() -> &'static std::path::Path {
    M2M_SID_PATH.get_or_init(|| {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/coreconf-m2m@2026-03-29.sid")
    })
}

fn m2m_model() -> &'static CompositeModel {
    static MODEL: OnceLock<CompositeModel> = OnceLock::new();
    MODEL.get_or_init(|| {
        let sid_file = SidFile::from_file(m2m_sid_path()).expect("SID parse failed");
        CompositeModel::from_sid_files(vec![sid_file]).expect("composite build failed")
    })
}

fn m2m_coreconf_model() -> CoreconfModel {
    static MODEL: OnceLock<CoreconfModel> = OnceLock::new();
    MODEL
        .get_or_init(|| CoreconfModel::new(m2m_sid_path()).expect("load m2m model"))
        .clone()
}

/// Look up a SID from the model, panicking with a helpful message on failure.
fn sid(path: &str) -> i64 {
    m2m_model()
        .get_sid(path)
        .unwrap_or_else(|| panic!("SID not found for path: {path}"))
}

/// Build a FETCH payload as `[target_sid] + key_values` (RFC 9595 instance-ID format).
fn encode_fetch_instance(target_sid: i64, keys: &[i64]) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::into_writer(&target_sid, &mut buf).unwrap();
    for k in keys {
        ciborium::into_writer(&k, &mut buf).unwrap();
    }
    buf
}

/// Build a yang-identifiers+cbor payload with a list of bare SIDs.
fn encode_fetch_sids(sids: &[i64]) -> Vec<u8> {
    let mut buf = Vec::new();
    for sid in sids {
        ciborium::into_writer(&sid, &mut buf).unwrap();
    }
    buf
}

/// Pre-populated datastore with two transducers.
fn bootstrapped_datastore() -> Datastore {
    let mut ds = Datastore::new_in_memory(m2m_model().clone());

    ds.set_path(
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']",
        json!({
            "unit": "W/m2",
            "precision": 1,
            "quantity": {
                "value": 8500,
                "timestamp": 1700000000u64,
                "timestamp-source": 0,
                "statistics": {
                    "min": 8000i64,
                    "max": 9000i64,
                    "mean": 8500i64,
                    "median": 8500i64,
                    "stdev": 200u64,
                    "sample-count": 1000u64,
                },
            },
        }),
    )
    .unwrap();

    ds.set_path(
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:air-temperature'][id='0']",
        json!({
            "unit": "degC",
            "precision": 2,
            "quantity": {
                "value": 2350,
                "timestamp": 1700000000u64,
            },
        }),
    )
    .unwrap();

    ds
}

// ---------------------------------------------------------------------------
// from_cbor — decode CORECONF CBOR into a fresh datastore
// ---------------------------------------------------------------------------

#[test]
fn create_datastore_from_cbor_roundtrip() {
    let model = m2m_coreconf_model();

    let json_data = json!({
        "coreconf-m2m:transducers": {
            "transducer": [
                {
                    "type": "coreconf-m2m:solar-radiation",
                    "id": "1",
                    "unit": "W/m2",
                    "precision": 2,
                },
            ],
        },
    });
    let cbor = model
        .to_coreconf(&serde_json::to_string(&json_data).unwrap())
        .unwrap();

    let ds = Datastore::from_cbor(model, &cbor).unwrap();

    let preds = ds
        .predicates("/coreconf-m2m:transducers/transducer")
        .unwrap();
    assert_eq!(preds.len(), 1);
    assert!(preds[0].contains("solar-radiation"));
    assert!(preds[0].contains("id='1'"));

    let unit = ds
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='1']/unit",
        )
        .unwrap();
    assert_eq!(unit, Some(json!("W/m2")));
}

#[test]
fn from_cbor_handles_empty_response() {
    let model = m2m_coreconf_model();
    let empty_cbor = model.to_coreconf("{}").unwrap();
    let ds = Datastore::from_cbor(model, &empty_cbor).unwrap();

    assert!(
        ds.predicates("/coreconf-m2m:transducers/transducer")
            .unwrap()
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// Interface routing: /c (management) vs /s (streaming) + CoAP Observe
// ---------------------------------------------------------------------------

#[test]
fn streaming_interface_rejects_non_fetch_methods() {
    let datastore = Datastore::new_in_memory(m2m_model().clone());
    let mut handler = RequestHandler::new(datastore);

    let req = Request::new(Method::Get).with_interface(Interface::Streaming);
    let resp = handler.handle(&req);
    assert!(!resp.code.is_success());
}

#[test]
fn streaming_fetch_without_observe_still_succeeds() {
    let datastore = bootstrapped_datastore();
    let mut handler = RequestHandler::new(datastore);

    let precision_sid = sid("/coreconf-m2m:transducers/transducer/precision");
    let payload = encode_fetch_sids(&[precision_sid]);
    let req = Request::new(Method::Fetch)
        .with_interface(Interface::Streaming)
        .with_payload(payload, ContentFormat::YangIdentifiersCbor);

    let resp = handler.handle(&req);
    assert!(resp.code.is_success());
    assert!(resp.observe.is_none());
}

#[test]
fn streaming_fetch_with_observe_stamps_sequence() {
    let datastore = bootstrapped_datastore();
    let mut handler = RequestHandler::new(datastore);

    let precision_sid = sid("/coreconf-m2m:transducers/transducer/precision");
    let payload = encode_fetch_sids(&[precision_sid]);
    let req = Request::new(Method::Fetch)
        .with_interface(Interface::Streaming)
        .with_payload(payload, ContentFormat::YangIdentifiersCbor)
        .with_observe(0);

    let resp = handler.handle(&req);
    assert!(resp.code.is_success());
    assert_eq!(resp.observe, Some(0));

    let req2 = Request::new(Method::Fetch)
        .with_interface(Interface::Streaming)
        .with_payload(
            encode_fetch_sids(&[precision_sid]),
            ContentFormat::YangIdentifiersCbor,
        )
        .with_observe(0);

    let resp2 = handler.handle(&req2);
    assert_eq!(resp2.observe, Some(1));
}

#[test]
fn management_interface_passes_through_unchanged() {
    let datastore = bootstrapped_datastore();
    let mut handler = RequestHandler::new(datastore);

    let precision_sid = sid("/coreconf-m2m:transducers/transducer/precision");
    let payload = encode_fetch_sids(&[precision_sid]);
    let req = Request::new(Method::Fetch)
        .with_interface(Interface::Management)
        .with_payload(payload, ContentFormat::YangIdentifiersCbor);

    let resp = handler.handle(&req);
    assert!(resp.code.is_success());
}

// ---------------------------------------------------------------------------
// Predicate-path resolution (resolve_xpath / create_xpath)
// ---------------------------------------------------------------------------

#[test]
fn resolve_xpath_value_path() {
    let ds = bootstrapped_datastore();
    let (resolved_sid, keys) = ds
        .resolve_xpath(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/value",
        )
        .unwrap();

    let expected_sid = sid("/coreconf-m2m:transducers/transducer/quantity/value");
    assert_eq!(resolved_sid, expected_sid);
    assert_eq!(keys.len(), 2);
}

#[test]
fn resolve_xpath_statistics_path() {
    let ds = bootstrapped_datastore();
    let (resolved_sid, keys) = ds
        .resolve_xpath(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/statistics",
        )
        .unwrap();

    let expected_sid = sid("/coreconf-m2m:transducers/transducer/quantity/statistics");
    assert_eq!(resolved_sid, expected_sid);
    assert_eq!(keys.len(), 2);
}

#[test]
fn resolve_xpath_sensor_alert_path() {
    let ds = bootstrapped_datastore();
    let (resolved_sid, keys) = ds
        .resolve_xpath(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/sensor-alert",
        )
        .unwrap();

    let expected_sid =
        sid("/coreconf-m2m:transducers/transducer/notification-parameters/sensor-alert");
    assert_eq!(resolved_sid, expected_sid);
    assert_eq!(keys.len(), 2);
}

#[test]
fn resolve_xpath_history_path() {
    let ds = bootstrapped_datastore();
    let (resolved_sid, keys) = ds
        .resolve_xpath(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/history",
        )
        .unwrap();

    let expected_sid = sid("/coreconf-m2m:transducers/transducer/notification-parameters/history");
    assert_eq!(resolved_sid, expected_sid);
    assert_eq!(keys.len(), 2);
}

#[test]
fn create_xpath_roundtrip_all_paths() {
    let ds = bootstrapped_datastore();

    let paths = [
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/value",
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/statistics",
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/sensor-alert",
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/history",
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/history/encoding",
    ];

    for path in &paths {
        let (resolved_sid, keys) = ds.resolve_xpath(path).unwrap();
        let rebuilt = ds.create_xpath(resolved_sid, &keys).unwrap();
        assert!(!rebuilt.is_empty(), "empty rebuild for {path}");
    }
}

// ---------------------------------------------------------------------------
// FETCH with instance identifiers
// ---------------------------------------------------------------------------

#[test]
fn fetch_measurement_value_via_instance_id() {
    let ds = bootstrapped_datastore();
    let mut handler = RequestHandler::new(ds);

    let (target_sid, key_values) = handler
        .datastore()
        .resolve_xpath(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/value",
        )
        .unwrap();

    let key_sids: Vec<i64> = key_values.iter().filter_map(|v| v.as_i64()).collect();
    let payload = encode_fetch_instance(target_sid, &key_sids);

    let req = Request::new(Method::Fetch).with_payload(payload, ContentFormat::YangIdentifiersCbor);
    let resp = handler.handle(&req);
    assert!(resp.code.is_success(), "FETCH failed: {:?}", resp.code);
}

#[test]
fn fetch_statistics_via_instance_id() {
    let ds = bootstrapped_datastore();
    let mut handler = RequestHandler::new(ds);

    let (target_sid, key_values) = handler
        .datastore()
        .resolve_xpath(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/statistics",
        )
        .unwrap();

    let key_sids: Vec<i64> = key_values.iter().filter_map(|v| v.as_i64()).collect();
    let payload = encode_fetch_instance(target_sid, &key_sids);

    let req = Request::new(Method::Fetch).with_payload(payload, ContentFormat::YangIdentifiersCbor);
    let resp = handler.handle(&req);
    assert!(resp.code.is_success(), "FETCH failed: {:?}", resp.code);
}

// ---------------------------------------------------------------------------
// iPATCH — path-based data modification
// ---------------------------------------------------------------------------

#[test]
fn ipatch_sensor_alert_threshold() {
    let ds = bootstrapped_datastore();
    let mut handler = RequestHandler::new(ds);

    let req = Request::new(Method::IPatch)
        .with_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/sensor-alert",
        )
        .with_payload(
            encode_json_value(&json!({
                "active": true,
                "t-min": 2000i64,
                "t-max": 10000i64,
                "hysteresis": 5u64,
            })),
            ContentFormat::YangDataCbor,
        );

    let resp = handler.handle(&req);
    assert!(resp.code.is_success(), "iPATCH failed: {:?}", resp.code);

    let t_min = handler
        .datastore()
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/sensor-alert/t-min",
        )
        .unwrap();
    assert_eq!(t_min, Some(json!(2000i64)));
}

#[test]
fn ipatch_history_notification() {
    let ds = bootstrapped_datastore();
    let mut handler = RequestHandler::new(ds);

    let req = Request::new(Method::IPatch)
        .with_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/history",
        )
        .with_payload(
            encode_json_value(&json!({
                "active": true,
                "step": 120000u64,
                "max-samples": 30u64,
                "encoding": 1u64,
            })),
            ContentFormat::YangDataCbor,
        );

    let resp = handler.handle(&req);
    assert!(resp.code.is_success(), "iPATCH failed: {:?}", resp.code);

    let step = handler
        .datastore()
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/history/step",
        )
        .unwrap();
    assert_eq!(step, Some(json!(120000u64)));

    let encoding = handler
        .datastore()
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/history/encoding",
        )
        .unwrap();
    assert_eq!(encoding, Some(json!(1u64)));
}

// ---------------------------------------------------------------------------
// Streaming data-structures (resolved from the YANG model at runtime)
// ---------------------------------------------------------------------------

#[test]
fn resolve_streaming_time_series_path() {
    let ds = bootstrapped_datastore();
    let (resolved_sid, keys) = ds
        .resolve_xpath(
            "/coreconf-m2m:history/time-series[type='coreconf-m2m:solar-radiation'][id='0']",
        )
        .unwrap();

    let expected_sid = sid("/coreconf-m2m:history/time-series");
    assert_eq!(resolved_sid, expected_sid);
    assert_eq!(keys.len(), 2);
}

#[test]
fn resolve_streaming_sensor_alert_path() {
    let ds = bootstrapped_datastore();
    let (resolved_sid, keys) = ds
        .resolve_xpath(
            "/coreconf-m2m:sensor-alert/target[type='coreconf-m2m:solar-radiation'][id='0']",
        )
        .unwrap();

    let expected_sid = sid("/coreconf-m2m:sensor-alert/target");
    assert_eq!(resolved_sid, expected_sid);
    assert_eq!(keys.len(), 2);
}

#[test]
fn replace_from_cbor_notification_response() {
    let model = m2m_coreconf_model();

    let time_series_json = json!({
        "coreconf-m2m:history": {
            "time-series": [
                {
                    "type": "coreconf-m2m:air-temperature",
                    "id": "0",
                    "values": [2350i64, -5, 3, -2, 1],
                },
            ],
        },
    });

    let cbor = model
        .to_coreconf(&serde_json::to_string(&time_series_json).unwrap())
        .unwrap();

    let mut ds = Datastore::new_in_memory(m2m_model().clone());
    ds.replace_from_cbor(&cbor).unwrap();

    let preds = ds.predicates("/coreconf-m2m:history/time-series").unwrap();
    assert_eq!(preds.len(), 1);
    assert!(preds[0].contains("air-temperature"));
    assert!(preds[0].contains("id='0'"));

    let values = ds
        .get_path(
            "/coreconf-m2m:history/time-series[type='coreconf-m2m:air-temperature'][id='0']/values",
        )
        .unwrap();
    assert!(values.is_some());
}

// ---------------------------------------------------------------------------
// Full roundtrip: bootstrap (GET) → from_cbor → FETCH → Observe
// ---------------------------------------------------------------------------

#[test]
fn full_bootstrap_fetch_cycle() {
    let datastore = bootstrapped_datastore();
    let mut handler = RequestHandler::new(datastore);

    // Step 1: bootstrap — GET the entire datastore as CORECONF CBOR.
    let req = Request::new(Method::Get);
    let resp = handler.handle(&req);
    assert!(
        resp.code.is_success(),
        "GET bootstrap failed: {:?}",
        resp.code
    );
    assert!(!resp.payload.is_empty());

    // Reconstruct datastore from the GET response.
    let model = m2m_coreconf_model();
    let ds = Datastore::from_cbor(model, &resp.payload).unwrap();
    let preds = ds
        .predicates("/coreconf-m2m:transducers/transducer")
        .unwrap();
    assert!(!preds.is_empty(), "bootstrap should return transducers");

    // Step 2: fetch individual measurement via instance ID.
    let xpath = "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/value";
    let (target_sid, key_values) = ds.resolve_xpath(xpath).unwrap();
    let key_sids: Vec<i64> = key_values.iter().filter_map(|v| v.as_i64()).collect();
    let instance_payload = encode_fetch_instance(target_sid, &key_sids);

    let fetch_req = Request::new(Method::Fetch)
        .with_payload(instance_payload, ContentFormat::YangIdentifiersCbor);
    let fetch_resp = handler.handle(&fetch_req);
    assert!(
        fetch_resp.code.is_success(),
        "step 2 fetch failed: {:?}",
        fetch_resp.code
    );

    // Step 3: observe time-series on /s (FETCH+Observe).
    let ts_xpath = "/coreconf-m2m:history/time-series[type='coreconf-m2m:solar-radiation'][id='0']";
    let (ts_sid, ts_keys) = ds.resolve_xpath(ts_xpath).unwrap();
    let ts_key_sids: Vec<i64> = ts_keys.iter().filter_map(|v| v.as_i64()).collect();
    let obs_payload = encode_fetch_instance(ts_sid, &ts_key_sids);

    let obs_req = Request::new(Method::Fetch)
        .with_interface(Interface::Streaming)
        .with_payload(obs_payload, ContentFormat::YangIdentifiersCbor)
        .with_observe(0);

    let obs_resp = handler.handle(&obs_req);
    assert!(obs_resp.code.is_success());
    assert_eq!(obs_resp.observe, Some(0));
}

// ---------------------------------------------------------------------------
// Instance-ID FETCH with keyed-list navigation
// ---------------------------------------------------------------------------

#[test]
fn fetch_with_instance_id_navigates_list_entry() {
    let ds = bootstrapped_datastore();
    let mut handler = RequestHandler::new(ds);

    let xpath = "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/value";
    let (target_sid, key_values) = handler.datastore().resolve_xpath(xpath).unwrap();

    // Build CBOR array [target_sid, key1, key2] using ALL key values.
    let mut arr = Vec::new();
    arr.push(serde_json::Value::Number(target_sid.into()));
    for kv in &key_values {
        arr.push(kv.clone());
    }
    let arr_value = serde_json::Value::Array(arr);
    let mut payload = Vec::new();
    ciborium::into_writer(&arr_value, &mut payload).unwrap();

    let req = Request::new(Method::Fetch).with_payload(payload, ContentFormat::YangIdentifiersCbor);
    let resp = handler.handle(&req);
    assert!(resp.code.is_success());
    assert!(!resp.payload.is_empty(), "FETCH with keys returned empty");
}

#[test]
fn from_cbor_instance_seq_reconstructs_datastore() {
    let model = m2m_coreconf_model();

    let transducer_list = json!({
        "coreconf-m2m:transducers": {
            "transducer": [{
                "type": "coreconf-m2m:solar-radiation",
                "id": "0",
                "unit": "W/m2",
                "precision": 1,
            }]
        }
    });
    let value_cbor = model
        .to_coreconf(&serde_json::to_string(&transducer_list).unwrap())
        .unwrap();

    // Decode the CBOR value so we can embed it in the instance map.
    let decoded_value: serde_json::Value = coreconf_model::codec::cbor_to_json_value(value_cbor.as_slice()).unwrap();

    let root_sid = sid("/coreconf-m2m:transducers");
    let instance_map = serde_json::json!({ root_sid.to_string(): decoded_value });

    let mut seq = Vec::new();
    ciborium::into_writer(&instance_map, &mut seq).unwrap();

    let ds = Datastore::from_cbor_instance_seq(model, &seq).unwrap();

    let preds = ds
        .predicates("/coreconf-m2m:transducers/transducer")
        .unwrap();
    assert_eq!(preds.len(), 1);
    assert!(preds[0].contains("solar-radiation"));
}

// ---------------------------------------------------------------------------
// Observe lifecycle
// ---------------------------------------------------------------------------

#[test]
fn observe_register_and_notify() {
    let ds = bootstrapped_datastore();
    let mut handler = RequestHandler::new(ds);

    let token = b"\x01\x02\x03".to_vec();
    let mut resources = std::collections::HashSet::new();
    resources.insert(sid("/coreconf-m2m:transducers/transducer/quantity/value").to_string());

    handler.register_observer(token.clone(), resources);

    // Mark changed — should produce a notification.
    handler.mark_changed(&sid("/coreconf-m2m:transducers/transducer/quantity/value").to_string());

    let notifications = handler.pending_notifications(&token);
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].1, 0); // sequence starts at 0

    // Second poll with no new changes — should be empty.
    let notifications2 = handler.pending_notifications(&token);
    assert!(notifications2.is_empty());
}

#[test]
fn observe_deregister_stops_notifications() {
    let ds = bootstrapped_datastore();
    let mut handler = RequestHandler::new(ds);

    let token = b"\xaa".to_vec();
    let mut resources = std::collections::HashSet::new();
    resources.insert("100080".to_string());

    handler.register_observer(token.clone(), resources);
    handler.mark_changed("100080");

    // Deregister.
    handler.deregister_observer(&token);

    let notifications = handler.pending_notifications(&token);
    assert!(notifications.is_empty());
}

#[test]
fn observe_multiple_resources() {
    let ds = bootstrapped_datastore();
    let mut handler = RequestHandler::new(ds);

    let token = b"\xbb".to_vec();
    let mut resources = std::collections::HashSet::new();
    resources.insert("sid_a".to_string());
    resources.insert("sid_b".to_string());

    handler.register_observer(token.clone(), resources);
    handler.mark_changed("sid_a");
    handler.mark_changed("sid_b");

    let notifications = handler.pending_notifications(&token);
    assert_eq!(notifications.len(), 2);
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn encode_json_value(value: &serde_json::Value) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).unwrap();
    buf
}
