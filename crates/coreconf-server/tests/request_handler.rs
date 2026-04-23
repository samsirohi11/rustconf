use coreconf_compiler::compile_paths;
use coreconf_server::{CoreconfServer, MemoryAuthorizer, NoopAuditSink, SqliteStore};
use rust_coreconf::coap_types::{ContentFormat, Method, Request};
use rust_coreconf::{CoreconfModel, RequestBuilder};
use tempfile::tempdir;

#[test]
fn ipatch_persists_across_server_restart() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("coreconf-compiler")
        .join("tests")
        .join("fixtures")
        .join("basic-module.yang");
    let bundle = compile_paths(&[fixture]).unwrap();
    let model = CoreconfModel::from_bundle(bundle.clone()).unwrap();
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("runtime.db");
    let store = SqliteStore::open(&db_path).unwrap();
    let mut server =
        CoreconfServer::from_bundle(bundle.clone(), store, MemoryAuthorizer::default(), NoopAuditSink::default())
            .unwrap();
    let builder = RequestBuilder::new(model.clone());
    let payload = builder
        .build_ipatch(&[("/demo:greeting/message", Some(serde_json::json!("hello")))])
        .unwrap();

    let response = server.handle(
        &Request::new(Method::IPatch)
            .with_actor("system")
            .with_payload(payload, ContentFormat::YangInstancesCborSeq),
    );
    assert!(response.code.is_success());

    let store = SqliteStore::open(&db_path).unwrap();
    let mut restarted =
        CoreconfServer::from_bundle(bundle, store, MemoryAuthorizer::default(), NoopAuditSink::default())
            .unwrap();
    let get = restarted.handle(&Request::new(Method::Get).with_actor("system"));
    let json = model.to_json(&get.payload).unwrap();
    assert!(json.contains("\"hello\""));
}

#[test]
fn handles_get_against_persisted_snapshot() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("server.db");
    let store = SqliteStore::open(&db_path).unwrap();
    let mut server =
        CoreconfServer::new(store, MemoryAuthorizer::default(), NoopAuditSink::default());
    let response = server.handle(&Request::new(Method::Get).with_actor("system"));
    assert!(response.code.is_success());
}
