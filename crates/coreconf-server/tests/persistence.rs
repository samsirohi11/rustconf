use coreconf_schema::{CompiledSchemaBundle, SchemaModule};
use coreconf_server::{AuditEvent, SqliteStore, Store};
use serde_json::json;
use std::collections::BTreeMap;
use tempfile::tempdir;

fn minimal_bundle() -> CompiledSchemaBundle {
    CompiledSchemaBundle {
        format_version: 2,
        modules: vec![SchemaModule {
            name: "demo".into(),
            revision: "2026-04-23".into(),
        }],
        typedefs: vec![],
        identities: vec![],
        nodes: BTreeMap::new(),
        operations: BTreeMap::new(),
    }
}

#[test]
fn persists_bundle_snapshot_and_active_schema_version() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("coreconf.db");
    let mut store = SqliteStore::open(&db_path).unwrap();
    let bundle = minimal_bundle();

    store.write_bundle("demo@2026-04-23", &bundle).unwrap();
    store
        .write_snapshot("demo@2026-04-23", &json!({"demo:greeting":{"message":"hello"}}))
        .unwrap();
    store
        .set_active_schema_version("demo@2026-04-23")
        .unwrap();
    store
        .append_audit(AuditEvent::new("system", "write", "/demo:greeting/message"))
        .unwrap();

    let restored_bundle = store.read_bundle("demo@2026-04-23").unwrap().unwrap();
    let active = store.active_schema_version().unwrap();
    let snapshot = store.read_snapshot("demo@2026-04-23").unwrap().unwrap();
    let audit = store.read_audit().unwrap();

    assert_eq!(restored_bundle.modules[0].name, "demo");
    assert_eq!(active.as_deref(), Some("demo@2026-04-23"));
    assert_eq!(snapshot["demo:greeting"]["message"], "hello");
    assert_eq!(audit.len(), 1);
}
