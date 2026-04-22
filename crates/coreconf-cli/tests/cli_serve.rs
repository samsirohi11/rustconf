use assert_cmd::Command;

#[test]
fn prints_serve_help() {
    Command::cargo_bin("coreconf")
        .unwrap()
        .args(["serve", "--help"])
        .assert()
        .success();
}
