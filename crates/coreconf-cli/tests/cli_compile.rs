use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn compiles_yang_to_bundle_and_sid() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("coreconf-compiler")
        .join("tests")
        .join("fixtures")
        .join("basic-module.yang");
    let output_dir = tempdir().unwrap();
    let bundle_out = output_dir.path().join("basic.bundle.json");
    let sid_out = output_dir.path().join("basic.sid.json");

    Command::cargo_bin("coreconf")
        .unwrap()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "--bundle-out",
            bundle_out.to_str().unwrap(),
            "--sid-out",
            sid_out.to_str().unwrap(),
        ])
        .assert()
        .success();
}
