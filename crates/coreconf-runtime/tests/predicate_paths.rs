use coreconf_model::CompositeModel;
use coreconf_runtime::{Datastore, PredicatePath};

#[test]
fn predicate_path_parse_extracts_canonical_path_and_predicates() {
    let parsed = PredicatePath::parse("/example:devices/device[id='rdc-1']/enabled").unwrap();

    assert_eq!(parsed.canonical_path, "/example:devices/device/enabled");
    assert_eq!(parsed.predicates, vec![("id".into(), "rdc-1".into())]);
}

#[test]
fn datastore_reads_and_writes_predicate_paths() {
    let model = CompositeModel::from_sid_strings(&[r#"{
        "module-name":"example",
        "module-revision":"2026-01-01",
        "item":[
            {"identifier":"example","sid":60000},
            {"identifier":"/example:devices","sid":60001},
            {"identifier":"/example:devices/device","sid":60002},
            {"identifier":"/example:devices/device/id","sid":60003,"type":"string"},
            {"identifier":"/example:devices/device/enabled","sid":60004,"type":"boolean"}
        ],
        "key-mapping":{"60002":[60003]}
    }"#])
    .unwrap();

    let mut datastore = Datastore::new_in_memory(model);
    datastore
        .set_path(
            "/example:devices/device[id='rdc-1']/enabled",
            serde_json::json!(true),
        )
        .unwrap();

    let value = datastore
        .get_path("/example:devices/device[id='rdc-1']/enabled")
        .unwrap();
    assert_eq!(value, Some(serde_json::json!(true)));
}

#[test]
fn datastore_preserves_single_key_predicate_matching() {
    let model = CompositeModel::from_sid_strings(&[r#"{
        "module-name":"example",
        "module-revision":"2026-01-01",
        "item":[
            {"identifier":"example","sid":60000},
            {"identifier":"/example:devices","sid":60001},
            {"identifier":"/example:devices/device","sid":60002},
            {"identifier":"/example:devices/device/id","sid":60003,"type":"string"},
            {"identifier":"/example:devices/device/enabled","sid":60004,"type":"boolean"}
        ],
        "key-mapping":{"60002":[60003]}
    }"#])
    .unwrap();

    let mut datastore = Datastore::new_in_memory(model);
    datastore
        .set_path(
            "/example:devices/device[/example:devices/device/id='rdc-1']/enabled",
            serde_json::json!(true),
        )
        .unwrap();

    let value = datastore
        .get_path("/example:devices/device[id='rdc-1']/enabled")
        .unwrap();
    assert_eq!(value, Some(serde_json::json!(true)));
}

#[test]
fn datastore_matches_multi_key_predicates_by_name() {
    let model = CompositeModel::from_sid_strings(&[r#"{
        "module-name":"example",
        "module-revision":"2026-01-01",
        "item":[
            {"identifier":"example","sid":60000},
            {"identifier":"/example:tenants","sid":60001},
            {"identifier":"/example:tenants/interface","sid":60002},
            {"identifier":"/example:tenants/interface/tenant","sid":60003,"type":"string"},
            {"identifier":"/example:tenants/interface/name","sid":60004,"type":"string"},
            {"identifier":"/example:tenants/interface/enabled","sid":60005,"type":"boolean"}
        ],
        "key-mapping":{"60002":[60003,60004]}
    }"#])
    .unwrap();

    let mut datastore = Datastore::new_in_memory(model);
    datastore
        .set_path(
            "/example:tenants/interface[tenant='tenant-a'][name='eth0']/enabled",
            serde_json::json!(true),
        )
        .unwrap();

    let value = datastore
        .get_path("/example:tenants/interface[name='eth0'][tenant='tenant-a']/enabled")
        .unwrap();
    assert_eq!(value, Some(serde_json::json!(true)));
}
