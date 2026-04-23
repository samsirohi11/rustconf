use coreconf_compiler::compile_paths;
use std::path::PathBuf;

#[test]
fn compiles_basic_yin_module_into_bundle() {
    let bundle = compile_paths(&[PathBuf::from("tests/fixtures/phase2c-service.yin")]).unwrap();

    assert_eq!(bundle.modules[0].name, "phase2c-service");
    assert!(bundle.nodes.contains_key("/phase2c-service:service"));
    assert!(bundle.nodes.contains_key("/phase2c-service:service/name"));
}
