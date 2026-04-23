use coreconf_schema::{CompiledSchemaBundle, NodeKind, SchemaModule, SchemaNode, YangScalarType};
use rust_coreconf::{CoreconfModel, Datastore};
use serde_json::json;
use std::collections::BTreeMap;

fn user_bundle() -> CompiledSchemaBundle {
    let mut nodes = BTreeMap::new();
    nodes.insert(
        "/demo:users".into(),
        SchemaNode {
            path: "/demo:users".into(),
            sid: Some(60001),
            kind: NodeKind::Container,
            yang_type: None,
            type_ref: None,
            keys: vec![],
            children: vec!["/demo:users/user".into()],
            must: vec![],
            when: None,
        },
    );
    nodes.insert(
        "/demo:users/user".into(),
        SchemaNode {
            path: "/demo:users/user".into(),
            sid: Some(60002),
            kind: NodeKind::List,
            yang_type: None,
            type_ref: None,
            keys: vec!["/demo:users/user/name".into()],
            children: vec!["/demo:users/user/name".into(), "/demo:users/user/role".into()],
            must: vec![],
            when: None,
        },
    );
    nodes.insert(
        "/demo:users/user/name".into(),
        SchemaNode {
            path: "/demo:users/user/name".into(),
            sid: Some(60003),
            kind: NodeKind::Leaf,
            yang_type: Some(YangScalarType::String),
            type_ref: None,
            keys: vec![],
            children: vec![],
            must: vec![],
            when: None,
        },
    );
    nodes.insert(
        "/demo:users/user/role".into(),
        SchemaNode {
            path: "/demo:users/user/role".into(),
            sid: Some(60004),
            kind: NodeKind::Leaf,
            yang_type: Some(YangScalarType::String),
            type_ref: None,
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
        typedefs: vec![],
        identities: vec![],
        nodes,
        operations: BTreeMap::new(),
    }
}

#[test]
fn sets_and_gets_list_leaf_by_path_expression() {
    let model = CoreconfModel::from_bundle(user_bundle()).unwrap();
    let mut ds = Datastore::new(model);
    ds.set_path_expr("/demo:users/user[name='obi']/role", json!("admin"))
        .unwrap();
    let value = ds.get_path_expr("/demo:users/user[name='obi']/role").unwrap();
    assert_eq!(value, Some(json!("admin")));
}

#[test]
fn enumerates_list_predicates() {
    let model = CoreconfModel::from_bundle(user_bundle()).unwrap();
    let mut ds = Datastore::new(model);
    ds.set_path_expr("/demo:users/user[name='obi']/role", json!("admin"))
        .unwrap();
    ds.set_path_expr("/demo:users/user[name='anakin']/role", json!("user"))
        .unwrap();
    let predicates = ds.predicates("/demo:users/user").unwrap();
    assert_eq!(predicates, vec!["[name='anakin']", "[name='obi']"]);
}
