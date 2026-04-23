use coreconf_compiler::compile_paths;
use std::path::PathBuf;

#[test]
fn applies_augment_and_deviation_changes() {
    let bundle = compile_paths(&[
        PathBuf::from("tests/fixtures/phase2-types.yang"),
        PathBuf::from("tests/fixtures/phase2-device.yang"),
        PathBuf::from("tests/fixtures/phase2-overrides.yang"),
    ])
    .unwrap();

    assert!(bundle.nodes.contains_key("/phase2-device:inventory/device/firmware-version"));
    assert_eq!(
        bundle.nodes["/phase2-device:inventory/device/owner"].must,
        vec!["string-length(.) > 0".to_string()]
    );
}
