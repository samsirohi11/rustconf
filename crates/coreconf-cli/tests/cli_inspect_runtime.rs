use assert_cmd::Command;
use predicates::str::contains;
use tempfile::tempdir;

#[test]
fn inspect_reports_active_schema_and_audit_count() {
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

    Command::cargo_bin("coreconf")
        .unwrap()
        .args([
            "inspect",
            "--bundle",
            bundle.to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("active schema version"))
        .stdout(contains("audit events:"));
}
