use coreconf_schema::{CompiledSchemaBundle, NodeKind, SchemaModule, SchemaNode, YangScalarType};
use rust_coreconf::CoreconfModel;
use std::collections::BTreeMap;

fn sample_bundle() -> CompiledSchemaBundle {
    let mut nodes = BTreeMap::new();
    nodes.insert(
        "/demo:greeting".into(),
        SchemaNode {
            path: "/demo:greeting".into(),
            sid: Some(60001),
            kind: NodeKind::Container,
            yang_type: None,
            keys: vec![],
            children: vec!["/demo:greeting/message".into()],
            must: vec![],
            when: None,
        },
    );
    nodes.insert(
        "/demo:greeting/message".into(),
        SchemaNode {
            path: "/demo:greeting/message".into(),
            sid: Some(60002),
            kind: NodeKind::Leaf,
            yang_type: Some(YangScalarType::String),
            keys: vec![],
            children: vec![],
            must: vec![],
            when: None,
        },
    );

    CompiledSchemaBundle {
        format_version: 1,
        modules: vec![SchemaModule {
            name: "demo".into(),
            revision: "2026-04-22".into(),
        }],
        nodes,
        operations: BTreeMap::new(),
    }
}

#[test]
fn roundtrips_using_schema_bundle() {
    let model = CoreconfModel::from_bundle(sample_bundle()).unwrap();
    let cbor = model
        .to_coreconf(r#"{"demo:greeting":{"message":"hello"}}"#)
        .unwrap();
    let json = model.to_json(&cbor).unwrap();
    assert!(json.contains("\"message\":\"hello\""));
}
