use coreconf_compiler::parse_module;

#[test]
fn parses_basic_module_tree() {
    let input = include_str!("fixtures/basic-module.yang");
    let module = parse_module(input).unwrap();
    assert_eq!(module.name, "demo");
    assert_eq!(module.children[0].keyword, "namespace");
    assert_eq!(module.children[2].keyword, "container");
    assert_eq!(module.children[2].children[0].keyword, "leaf");
    assert_eq!(
        module.children[2].children[0].argument.as_deref(),
        Some("message")
    );
}
