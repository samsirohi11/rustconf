use coreconf_compiler::{compile_paths, ValidationError};
use std::path::PathBuf;

#[test]
fn rejects_must_and_when_that_reference_missing_schema_nodes() {
    let result = compile_paths(&[PathBuf::from(
        "tests/fixtures/phase2c-invalid-constraints.yang",
    )]);

    assert!(matches!(
        result,
        Err(ValidationError::InvalidXPath(message))
        if message.contains("../missing")
    ));
}

#[test]
fn keeps_valid_must_and_when_expressions_on_lowered_nodes() {
    let bundle = compile_paths(&[PathBuf::from("tests/fixtures/phase2c-service.yang")]).unwrap();

    assert_eq!(
        bundle.nodes["/phase2c-service:service/endpoint/name"].must,
        vec!["string-length(.) >= 3".to_string()]
    );
    assert_eq!(
        bundle.nodes["/phase2c-service:service/endpoint/tls"].when.as_deref(),
        Some("./enabled = 'true'")
    );
}
