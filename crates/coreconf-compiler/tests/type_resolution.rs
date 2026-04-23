use coreconf_compiler::compile_paths;
use coreconf_schema::{ResolvedType, YangScalarType};
use std::path::PathBuf;

#[test]
fn resolves_typedef_identityref_and_leafref_metadata() {
    let bundle = compile_paths(&[
        PathBuf::from("tests/fixtures/phase2-types.yang"),
        PathBuf::from("tests/fixtures/phase2-device.yang"),
    ])
    .unwrap();

    assert_eq!(
        bundle.nodes["/phase2-device:inventory/device/name"].type_ref,
        Some(ResolvedType::Typedef {
            name: "phase2-types:device-name".into(),
            base: YangScalarType::String,
        })
    );

    assert_eq!(
        bundle.nodes["/phase2-device:inventory/device/role"].type_ref,
        Some(ResolvedType::IdentityRef {
            base: "phase2-types:device-role".into(),
            allowed: vec!["phase2-types:router".into(), "phase2-types:sensor".into()],
        })
    );

    assert_eq!(
        bundle.nodes["/phase2-device:inventory/device/owner-name-ref"].type_ref,
        Some(ResolvedType::LeafRef {
            target_path: "/phase2-device:inventory/device/owner".into(),
        })
    );
}
