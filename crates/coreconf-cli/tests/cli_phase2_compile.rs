use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn compiles_yang_to_bundle_sid_tree_yang_and_yin() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("coreconf-compiler")
        .join("tests")
        .join("fixtures")
        .join("basic-module.yang");
    let output_dir = tempdir().unwrap();
    let bundle_out = output_dir.path().join("basic.bundle.json");
    let sid_out = output_dir.path().join("basic.sid.json");
    let tree_out = output_dir.path().join("basic.tree.txt");
    let yang_out = output_dir.path().join("basic.normalized.yang");
    let yin_out = output_dir.path().join("basic.normalized.yin");

    Command::cargo_bin("coreconf")
        .unwrap()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "--bundle-out",
            bundle_out.to_str().unwrap(),
            "--sid-out",
            sid_out.to_str().unwrap(),
            "--tree-out",
            tree_out.to_str().unwrap(),
            "--yang-out",
            yang_out.to_str().unwrap(),
            "--yin-out",
            yin_out.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(fs::read_to_string(tree_out).unwrap().contains("greeting"));
    assert!(fs::read_to_string(yang_out).unwrap().contains("module demo"));
    assert!(fs::read_to_string(yin_out).unwrap().contains("<module name=\"demo\">"));
}
