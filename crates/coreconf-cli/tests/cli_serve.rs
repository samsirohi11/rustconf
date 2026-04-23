use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn prints_serve_help() {
    Command::cargo_bin("coreconf")
        .unwrap()
        .args(["serve", "--help"])
        .assert()
        .success();
}

#[test]
fn serve_bootstraps_sqlite_runtime_db() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("coreconf-compiler")
        .join("tests")
        .join("fixtures")
        .join("basic-module.yang");
    let dir = tempdir().unwrap();
    let bundle = dir.path().join("basic.bundle.json");
    let sid = dir.path().join("basic.sid.json");
    let db = dir.path().join("runtime.db");

    Command::cargo_bin("coreconf")
        .unwrap()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "--bundle-out",
            bundle.to_str().unwrap(),
            "--sid-out",
            sid.to_str().unwrap(),
        ])
        .assert()
        .success();

    Command::cargo_bin("coreconf")
        .unwrap()
        .args([
            "serve",
            "--bundle",
            bundle.to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--seed-json",
            "{\"demo:greeting\":{\"message\":\"hello\"}}",
        ])
        .assert()
        .success();

    assert!(db.exists());
}
