use coreconf_compiler::{compile_paths, ValidationError};
use std::path::PathBuf;

#[test]
fn resolves_imports_uses_and_rpc_shapes() {
    let bundle = compile_paths(&[
        PathBuf::from("tests/fixtures/imported-types.yang"),
        PathBuf::from("tests/fixtures/uses-rpc.yang"),
    ])
    .unwrap();

    assert!(bundle.nodes.contains_key("/uses-rpc:users/user/username"));
    assert!(bundle.operations.contains_key("/uses-rpc:reset-user"));
}

#[test]
fn reports_invalid_leafref_or_xpath() {
    let result = coreconf_compiler::validate_xpath("../missing[");
    assert!(matches!(result, Err(ValidationError::InvalidXPath(_))));
}
