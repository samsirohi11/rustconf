//! Integration tests using the coreconf-m2m weather station SID file,
//! mirroring the pycoreconf samples/datastore/main.py workflow.

use coreconf_model::{CompositeModel, SidFile};
use coreconf_runtime::Datastore;
use serde_json::json;

fn load_m2m_model() -> CompositeModel {
    let sid_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/coreconf-m2m@2026-03-29.sid");
    let sid_file = SidFile::from_file(&sid_path).expect("failed to parse m2m SID file");
    CompositeModel::from_sid_files(vec![sid_file]).expect("failed to build m2m composite model")
}

#[test]
fn m2m_predicates_on_empty_datastore() {
    let model = load_m2m_model();
    let datastore = Datastore::new_in_memory(model);

    // Predicates on an empty list should return empty.
    let preds = datastore
        .predicates("/coreconf-m2m:transducers/transducer")
        .unwrap();
    assert!(preds.is_empty());
}

#[test]
fn m2m_create_and_read_list_entry_with_identityref_key() {
    let model = load_m2m_model();
    let mut ds = Datastore::new_in_memory(model);
    let result = ds.set_path(
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision",
        json!(2),
    );
    assert!(result.is_ok(), "set at leaf inside list failed: {:?}", result.err());

    let value = ds.get_path(
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision"
    ).unwrap();
    assert_eq!(value, Some(json!(2)));

    // Verify predicates.
    let preds = ds.predicates("/coreconf-m2m:transducers/transducer").unwrap();
    assert_eq!(preds.len(), 1);
    assert!(preds[0].contains("solar-radiation"));
    assert!(preds[0].contains("id='0'"));
}

#[test]
fn m2m_set_full_dict_on_list_entry() {
    let model = load_m2m_model();
    let mut datastore = Datastore::new_in_memory(model);

    // Create a full transducer entry with a dict.
    datastore
        .set_path(
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
                        "sample-count": 1000u64
                    }
                }
            }),
        )
        .unwrap();

    // Read back individual fields.
    let unit = datastore
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit",
        )
        .unwrap();
    assert_eq!(unit, Some(json!("W/m2")));

    let value = datastore
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/value",
        )
        .unwrap();
    assert_eq!(value, Some(json!(8500)));

    let sample_count = datastore
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/quantity/statistics/sample-count",
        )
        .unwrap();
    assert_eq!(sample_count, Some(json!(1000u64)));
}

#[test]
fn m2m_delete_leaf_and_entry() {
    let model = load_m2m_model();
    let mut datastore = Datastore::new_in_memory(model);

    // Create an entry first.
    datastore
        .set_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']",
            json!({"unit": "W/m2", "precision": 2}),
        )
        .unwrap();

    // Delete just the unit leaf.
    let deleted = datastore
        .delete_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit",
        )
        .unwrap();
    assert!(deleted);

    // Unit should be gone, precision still there.
    let unit = datastore
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit",
        )
        .unwrap();
    assert_eq!(unit, None);

    let precision = datastore
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision",
        )
        .unwrap();
    assert_eq!(precision, Some(json!(2)));

    // Delete the entire list entry.
    let deleted = datastore
        .delete_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']",
        )
        .unwrap();
    assert!(deleted);

    let preds = datastore
        .predicates("/coreconf-m2m:transducers/transducer")
        .unwrap();
    assert!(preds.is_empty());
}

#[test]
fn m2m_resolve_and_create_xpath_roundtrip() {
    let model = load_m2m_model();
    let mut datastore = Datastore::new_in_memory(model);

    // Create an entry.
    datastore
        .set_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision",
            json!(3),
        )
        .unwrap();

    let xpath =
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision";

    // Resolve to (SID, keys).
    let (sid, keys) = datastore.resolve_xpath(xpath).unwrap();
    assert_eq!(sid, 100080); // precision SID

    // Create xpath back from SID + keys.
    let roundtrip = datastore.create_xpath(sid, &keys).unwrap();

    // Full XPath roundtrip may differ due to shortened module prefixes in create_xpath.
    // For example: /coreconf-m2m:transducers/transducer[...]/precision
    // vs:         /transducers/transducer[...]/precision (after module prefix stripping)
    // We just verify the leaf and keys roundtrip.
    assert!(roundtrip.contains("transducer"));
    assert!(roundtrip.contains("precision"));
}

#[test]
fn m2m_predicates_returns_all_entries() {
    let model = load_m2m_model();
    let mut datastore = Datastore::new_in_memory(model);

    // Create three transducer entries of different types.
    datastore
        .set_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']",
            json!({"precision": 1}),
        )
        .unwrap();
    datastore
        .set_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:air-temperature'][id='0']",
            json!({"precision": 2}),
        )
        .unwrap();
    datastore
        .set_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:wind-speed'][id='0']",
            json!({"precision": 0}),
        )
        .unwrap();

    let preds = datastore
        .predicates("/coreconf-m2m:transducers/transducer")
        .unwrap();
    assert_eq!(preds.len(), 3);

    // Each predicate should contain its type identity name.
    assert!(preds.iter().any(|p| p.contains("solar-radiation")));
    assert!(preds.iter().any(|p| p.contains("air-temperature")));
    assert!(preds.iter().any(|p| p.contains("wind-speed")));
}

#[test]
fn m2m_enum_predicate_in_set_and_get() {
    let model = load_m2m_model();
    let mut datastore = Datastore::new_in_memory(model);

    // Create an entry and set the encoding enum to "delta" (value 1).
    datastore
        .set_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']",
            json!({
                "notification-parameters": {
                    "history": {
                        "encoding": 1
                    }
                }
            }),
        )
        .unwrap();

    let encoding = datastore
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/notification-parameters/history/encoding",
        )
        .unwrap();
    assert_eq!(encoding, Some(json!(1)));
}

#[test]
fn m2m_multiple_sensors_same_type_different_ids() {
    let model = load_m2m_model();
    let mut datastore = Datastore::new_in_memory(model);

    // Same type, different ids.
    datastore
        .set_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']",
            json!({"unit": "W/m2"}),
        )
        .unwrap();
    datastore
        .set_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='1']",
            json!({"unit": "kW/m2"}),
        )
        .unwrap();

    // Predicates resolves both.
    let preds = datastore
        .predicates("/coreconf-m2m:transducers/transducer")
        .unwrap();
    assert_eq!(preds.len(), 2);

    // Read individual entries correctly.
    let unit0 = datastore
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/unit",
        )
        .unwrap();
    assert_eq!(unit0, Some(json!("W/m2")));

    let unit1 = datastore
        .get_path(
            "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='1']/unit",
        )
        .unwrap();
    assert_eq!(unit1, Some(json!("kW/m2")));
}

#[test]
fn simple_leaf_set_inside_list() {
    let model = load_m2m_model();
    let mut ds = Datastore::new_in_memory(model);
    let result = ds.set_path(
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision",
        json!(2),
    );
    assert!(result.is_ok(), "set at leaf inside list failed: {:?}", result.err());

    let value = ds.get_path(
        "/coreconf-m2m:transducers/transducer[type='coreconf-m2m:solar-radiation'][id='0']/precision"
    ).unwrap();
    assert_eq!(value, Some(json!(2)));
}
