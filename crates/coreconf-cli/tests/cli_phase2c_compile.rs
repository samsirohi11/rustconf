use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn compiles_yang_and_yin_inputs_and_emits_real_normalized_outputs() {
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("coreconf-compiler")
        .join("tests")
        .join("fixtures");
    let output_dir = tempdir().unwrap();
    let bundle_out = output_dir.path().join("phase2c.bundle.json");
    let sid_out = output_dir.path().join("phase2c.sid.json");
    let yang_out = output_dir.path().join("phase2c.normalized.yang");
    let yin_out = output_dir.path().join("phase2c.normalized.yin");

    Command::cargo_bin("coreconf")
        .unwrap()
        .args([
            "compile",
            fixture_dir.join("phase2c-service.yin").to_str().unwrap(),
            fixture_dir.join("phase2c-common.yang").to_str().unwrap(),
            "--bundle-out",
            bundle_out.to_str().unwrap(),
            "--sid-out",
            sid_out.to_str().unwrap(),
            "--yang-out",
            yang_out.to_str().unwrap(),
            "--yin-out",
            yin_out.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(fs::read_to_string(&yang_out).unwrap().contains("container service"));
    assert!(fs::read_to_string(&yang_out).unwrap().contains("leaf name"));
    assert!(fs::read_to_string(&yin_out).unwrap().contains("<container name=\"service\">"));
    assert!(fs::read_to_string(&yin_out).unwrap().contains("<leaf name=\"name\">"));
}
