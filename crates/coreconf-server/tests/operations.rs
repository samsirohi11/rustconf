use coreconf_server::OperationRegistry;
use serde_json::json;

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
