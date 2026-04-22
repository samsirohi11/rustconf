use coreconf_server::{AuditEvent, SqliteStore, Store};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn persists_snapshots_and_audit_events() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("coreconf.db");
    let mut store = SqliteStore::open(&db_path).unwrap();

    store
        .write_snapshot("schema-v1", &json!({"demo:greeting":{"message":"hello"}}))
        .unwrap();
    store
        .append_audit(AuditEvent::new("system", "write", "/demo:greeting/message"))
        .unwrap();

    let snapshot = store.read_snapshot("schema-v1").unwrap().unwrap();
    let audit = store.read_audit().unwrap();

    assert_eq!(snapshot["demo:greeting"]["message"], "hello");
    assert_eq!(audit.len(), 1);
}
