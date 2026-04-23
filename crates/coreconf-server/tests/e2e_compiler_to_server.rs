use coreconf_compiler::compile_paths;
use coreconf_server::{CoreconfServer, MemoryAuthorizer, NoopAuditSink, SqliteStore};
use rust_coreconf::coap_types::{Method, Request};
use tempfile::tempdir;

#[test]
fn serves_compiled_bundle_end_to_end() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("coreconf-compiler")
        .join("tests")
        .join("fixtures")
        .join("basic-module.yang");
    let bundle = compile_paths(&[fixture]).unwrap();
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    let store = SqliteStore::open(&db_path).unwrap();
    let mut server = CoreconfServer::from_bundle(
        bundle,
        store,
        MemoryAuthorizer::default(),
        NoopAuditSink::default(),
    )
    .unwrap();
    let response = server.handle(&Request::new(Method::Get));
    assert!(response.code.is_success());
}
