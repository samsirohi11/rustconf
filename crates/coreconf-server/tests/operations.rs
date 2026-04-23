use coreconf_compiler::compile_paths;
use coreconf_server::{
    CoreconfServer, NoopAuditSink, OperationRegistry, SqliteStore, StaticTokenAuthorizer,
};
use rust_coreconf::coap_types::{ContentFormat, Method, Request};
use rust_coreconf::{CoreconfModel, RequestBuilder};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn dispatches_registered_operation() {
    let mut registry = OperationRegistry::default();
    registry.register("/demo:reset", |input| {
        assert_eq!(input["username"], "obi");
        Ok(json!({"accepted": true}))
    });

    let output = registry
        .invoke("/demo:reset", json!({"username":"obi"}))
        .unwrap();
    assert_eq!(output["accepted"], true);
}

#[test]
fn dispatches_registered_post_operation_and_returns_output() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("coreconf-compiler")
        .join("tests")
        .join("fixtures");
    let bundle = compile_paths(&[
        fixtures.join("imported-types.yang"),
        fixtures.join("uses-rpc.yang"),
    ])
    .unwrap();
    let model = CoreconfModel::from_bundle(bundle.clone()).unwrap();
    let dir = tempdir().unwrap();
    let db = dir.path().join("ops.db");
    let store = SqliteStore::open(&db).unwrap();
    let mut server = CoreconfServer::from_bundle(
        bundle,
        store,
        StaticTokenAuthorizer::new([("cli-admin", "secret-token")]),
        NoopAuditSink::default(),
    )
    .unwrap();
    server.operations_mut().register("/uses-rpc:reset-user", |input| {
        assert_eq!(input["username"], "obi");
        Ok(json!({"accepted": true}))
    });

    let builder = RequestBuilder::new(model);
    let payload = builder
        .build_post("/uses-rpc:reset-user", Some(json!({"username":"obi"})))
        .unwrap();

    let response = server.handle(
        &Request::new(Method::Post)
            .with_actor("cli-admin")
            .with_auth_token("secret-token")
            .with_payload(payload, ContentFormat::YangInstancesCborSeq),
    );

    assert!(response.code.is_success());
    assert!(!response.payload.is_empty());
}
