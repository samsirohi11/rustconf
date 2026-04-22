use coreconf_schema::{
    CompiledSchemaBundle, NodeKind, OperationSchema, SchemaModule, SchemaNode, YangScalarType,
};
use std::collections::BTreeMap;

#[test]
fn serializes_schema_bundle_roundtrip() {
    let mut nodes = BTreeMap::new();
    nodes.insert(
        "/demo:settings/enabled".into(),
        SchemaNode {
            path: "/demo:settings/enabled".into(),
            sid: Some(60002),
            kind: NodeKind::Leaf,
            yang_type: Some(YangScalarType::Boolean),
            keys: vec![],
            children: vec![],
            must: vec![],
            when: None,
        },
    );

    let bundle = CompiledSchemaBundle {
        format_version: 1,
        modules: vec![SchemaModule {
            name: "demo".into(),
            revision: "2026-04-22".into(),
        }],
        nodes,
        operations: BTreeMap::from([(
            "/demo:reset".into(),
            OperationSchema {
                path: "/demo:reset".into(),
                input: vec![],
                output: vec![],
            },
        )]),
    };

    let encoded = serde_json::to_string_pretty(&bundle).unwrap();
    let decoded: CompiledSchemaBundle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.nodes["/demo:settings/enabled"].sid, Some(60002));
    assert_eq!(decoded.operations["/demo:reset"].path, "/demo:reset");
}
