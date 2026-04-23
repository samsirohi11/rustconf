use coreconf_compiler::compile_paths;
use std::path::PathBuf;

#[test]
fn expands_imported_groupings_and_applies_uses_refine_and_augment() {
    let bundle = compile_paths(&[PathBuf::from("tests/fixtures/phase2c-service.yang")]).unwrap();

    assert!(bundle.nodes.contains_key("/phase2c-service:service/endpoint/name"));
    assert_eq!(
        bundle.nodes["/phase2c-service:service/endpoint/name"].must,
        vec!["string-length(.) >= 3".to_string()]
    );
    assert_eq!(
        bundle.nodes["/phase2c-service:service/endpoint/tls"].when.as_deref(),
        Some("./enabled = 'true'")
    );
    assert!(bundle
        .nodes
        .contains_key("/phase2c-service:service/endpoint/tls/mode"));
}
