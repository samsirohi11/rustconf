use coreconf_server::{CoreconfServer, MemoryAuthorizer, NoopAuditSink, SqliteStore};
use rust_coreconf::coap_types::{Method, Request};
use tempfile::tempdir;

#[test]
fn handles_get_against_persisted_snapshot() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("server.db");
    let store = SqliteStore::open(&db_path).unwrap();
    let mut server =
        CoreconfServer::new(store, MemoryAuthorizer::default(), NoopAuditSink::default());
    let response = server.handle(&Request::new(Method::Get));
    assert!(response.code.is_success());
}
