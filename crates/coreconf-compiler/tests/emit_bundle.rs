use coreconf_compiler::{compile_paths, emit_bundle_json, emit_sid_json, emit_tree, emit_yang};
use std::path::PathBuf;

#[test]
fn emits_bundle_sid_tree_and_normalized_yang() {
    let bundle = compile_paths(&[PathBuf::from("tests/fixtures/basic-module.yang")]).unwrap();
    let bundle_json = emit_bundle_json(&bundle).unwrap();
    let sid_json = emit_sid_json(&bundle).unwrap();
    let tree = emit_tree(&bundle);
    let yang = emit_yang(&bundle);

    assert!(bundle_json.contains("\"format_version\": 1"));
    assert!(sid_json.contains("\"module-name\": \"demo\""));
    assert!(tree.contains("greeting"));
    assert!(yang.contains("module demo"));
}
