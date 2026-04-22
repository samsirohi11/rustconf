use coreconf_schema::{
    CompiledSchemaBundle, IdentitySchema, NodeKind, ResolvedType, SchemaModule, SchemaNode,
    TypedefSchema, YangScalarType,
};
use std::collections::BTreeMap;

#[test]
fn roundtrips_resolved_type_metadata() {
    let mut nodes = BTreeMap::new();
    nodes.insert(
        "/device:inventory/device/role".into(),
        SchemaNode {
            path: "/device:inventory/device/role".into(),
            sid: Some(61004),
            kind: NodeKind::Leaf,
            yang_type: Some(YangScalarType::IdentityRef),
            type_ref: Some(ResolvedType::IdentityRef {
                base: "phase2-types:device-role".into(),
                allowed: vec!["phase2-types:router".into(), "phase2-types:sensor".into()],
            }),
            keys: vec![],
            children: vec![],
            must: vec![],
            when: None,
        },
    );

    let bundle = CompiledSchemaBundle {
        format_version: 2,
        modules: vec![SchemaModule {
            name: "phase2-device".into(),
            revision: "2026-04-22".into(),
        }],
        typedefs: vec![TypedefSchema {
            module: "phase2-types".into(),
            name: "device-name".into(),
            base: YangScalarType::String,
        }],
        identities: vec![IdentitySchema {
            module: "phase2-types".into(),
            name: "device-role".into(),
            base: None,
        }],
        nodes,
        operations: BTreeMap::new(),
    };

    let json = serde_json::to_string_pretty(&bundle).unwrap();
    let decoded: CompiledSchemaBundle = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.format_version, 2);
    assert_eq!(decoded.typedefs[0].name, "device-name");
    assert_eq!(
        decoded.nodes["/device:inventory/device/role"].type_ref,
        Some(ResolvedType::IdentityRef {
            base: "phase2-types:device-role".into(),
            allowed: vec!["phase2-types:router".into(), "phase2-types:sensor".into()],
        })
    );
}
