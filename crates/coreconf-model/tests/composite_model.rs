use coreconf_model::{CompositeModel, CoreconfError, SidFile, YangType};

#[test]
fn composite_model_resolves_multiple_sid_files() {
    let model = CompositeModel::from_sid_strings(&[
        r#"{"module-name":"example-a","module-revision":"2026-01-01","item":[
            {"identifier":"example-a","sid":60000},
            {"identifier":"/example-a:root","sid":60001}
        ],"key-mapping":{}}"#,
        r#"{"module-name":"example-b","module-revision":"2026-01-01","item":[
            {"identifier":"example-b","sid":61000},
            {"identifier":"/example-b:leaf","sid":61001,"type":"string"}
        ],"key-mapping":{}}"#,
    ])
    .unwrap();

    assert_eq!(model.get_sid("/example-a:root"), Some(60001));
    assert_eq!(model.get_sid("/example-b:leaf"), Some(61001));
}

#[test]
fn composite_model_exposes_canonical_schema_fields() {
    let model = CompositeModel::from_sid_strings(&[
        r#"{"module-name":"example-a","module-revision":"2026-01-01","item":[
            {"identifier":"example-a","sid":60000},
            {"identifier":"/example-a:root","sid":60001}
        ],"key-mapping":{}}"#,
        r#"{"module-name":"example-b","module-revision":"2026-01-01","item":[
            {"identifier":"example-b","sid":61000},
            {"identifier":"/example-b:list","sid":61001},
            {"identifier":"/example-b:list/id","sid":61002,"type":"uint32"}
        ],"key-mapping":{"61001":[61002]}}"#,
    ])
    .unwrap();

    assert_eq!(model.sid_files.len(), 2);
    assert_eq!(model.sids.get("/example-a:root"), Some(&60001));
    assert_eq!(
        model.ids.get(&61002).map(String::as_str),
        Some("/example-b:list/id")
    );
    assert_eq!(
        model.types.get("/example-b:list/id"),
        Some(&YangType::Uint32)
    );
    assert_eq!(model.key_mapping.get(&61001), Some(&vec![61002]));
}

#[test]
fn composite_model_rejects_identifier_collisions_across_sid_files() {
    let err = CompositeModel::from_sid_strings(&[
        r#"{"module-name":"example-a","module-revision":"2026-01-01","item":[
            {"identifier":"example-a","sid":60000},
            {"identifier":"/example-a:root","sid":60001}
        ],"key-mapping":{}}"#,
        r#"{"module-name":"example-b","module-revision":"2026-01-01","item":[
            {"identifier":"example-b","sid":61000},
            {"identifier":"/example-a:root","sid":61001}
        ],"key-mapping":{}}"#,
    ])
    .unwrap_err();

    match err {
        CoreconfError::InvalidSidFile(message) => {
            assert!(message.contains("identifier conflict"));
            assert!(message.contains("/example-a:root"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn composite_model_rejects_sid_collisions_across_sid_files() {
    let err = CompositeModel::from_sid_strings(&[
        r#"{"module-name":"example-a","module-revision":"2026-01-01","item":[
            {"identifier":"example-a","sid":60000},
            {"identifier":"/example-a:root","sid":60001}
        ],"key-mapping":{}}"#,
        r#"{"module-name":"example-b","module-revision":"2026-01-01","item":[
            {"identifier":"example-b","sid":61000},
            {"identifier":"/example-b:leaf","sid":60001}
        ],"key-mapping":{}}"#,
    ])
    .unwrap_err();

    match err {
        CoreconfError::InvalidSidFile(message) => {
            assert!(message.contains("SID conflict"));
            assert!(message.contains("60001"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn composite_model_rejects_type_conflicts_across_sid_files() {
    let err = CompositeModel::from_sid_strings(&[
        r#"{"module-name":"example-a","module-revision":"2026-01-01","item":[
            {"identifier":"example-a","sid":60000},
            {"identifier":"/example-a:leaf","sid":60001,"type":"string"}
        ],"key-mapping":{}}"#,
        r#"{"module-name":"example-b","module-revision":"2026-01-01","item":[
            {"identifier":"example-b","sid":61000},
            {"identifier":"/example-a:leaf","sid":60001,"type":"uint32"}
        ],"key-mapping":{}}"#,
    ])
    .unwrap_err();

    match err {
        CoreconfError::InvalidSidFile(message) => {
            assert!(message.contains("type conflict"));
            assert!(message.contains("/example-a:leaf"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn composite_model_rejects_key_mapping_conflicts_across_sid_files() {
    let err = CompositeModel::from_sid_strings(&[
        r#"{"module-name":"example-a","module-revision":"2026-01-01","item":[
            {"identifier":"example-a","sid":60000},
            {"identifier":"/example-a:list","sid":60001},
            {"identifier":"/example-a:list/id","sid":60002,"type":"uint32"}
        ],"key-mapping":{"60001":[60002]}}"#,
        r#"{"module-name":"example-b","module-revision":"2026-01-01","item":[
            {"identifier":"example-b","sid":61000},
            {"identifier":"/example-a:list","sid":60001},
            {"identifier":"/example-a:list/name","sid":60003,"type":"string"}
        ],"key-mapping":{"60001":[60003]}}"#,
    ])
    .unwrap_err();

    match err {
        CoreconfError::InvalidSidFile(message) => {
            assert!(message.contains("key-mapping conflict"));
            assert!(message.contains("60001"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn parses_real_m2m_sid_file() {
    let sid_path =
        std::path::Path::new("../pycoreconf/samples/datastore/coreconf-m2m@2026-03-29.sid");
    if !sid_path.exists() {
        // Skip if the pycoreconf checkout isn't available
        eprintln!("m2m SID file not found — skipping integration test");
        return;
    }

    let sid_file = SidFile::from_file(sid_path).unwrap();
    let model = CompositeModel::from_sid_files(vec![sid_file]).unwrap();

    // Verify identities are stored as module_name:identity_name
    assert_eq!(model.get_sid("coreconf-m2m:solar-radiation"), Some(100008));
    assert_eq!(model.get_sid("coreconf-m2m:air-temperature"), Some(100001));

    // Verify data nodes keep full paths
    assert_eq!(model.get_sid("/coreconf-m2m:transducers"), Some(100062));
    assert_eq!(
        model.get_sid("/coreconf-m2m:transducers/transducer"),
        Some(100063)
    );

    // Verify types
    assert_eq!(
        model.get_type("/coreconf-m2m:transducers/transducer/type"),
        Some(&YangType::Identityref)
    );
    assert_eq!(
        model.get_type("/coreconf-m2m:transducers/transducer/unit"),
        Some(&YangType::String)
    );
    assert_eq!(
        model.get_type("/coreconf-m2m:transducers/transducer/precision"),
        Some(&YangType::Uint8)
    );

    // Verify enums
    let encoding_type = model
        .get_type("/coreconf-m2m:transducers/transducer/notification-parameters/history/encoding");
    assert!(encoding_type.is_some());

    // Verify key mapping: transducer list has keys [type, id]
    let keys = model.get_keys(100063).unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], 100096); // type
    assert_eq!(keys[1], 100064); // id
}
