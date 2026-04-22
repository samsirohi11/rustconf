use coreconf_compiler::compile_paths;
use std::path::PathBuf;

#[test]
fn loads_included_submodules_and_imports_together() {
    let bundle = compile_paths(&[
        PathBuf::from("tests/fixtures/phase2-types.yang"),
        PathBuf::from("tests/fixtures/phase2-device.yang"),
    ])
    .unwrap();

    assert!(bundle.nodes.contains_key("/phase2-device:inventory/device/serial"));
    assert!(bundle.nodes.contains_key("/phase2-device:inventory/device/owner"));
}
