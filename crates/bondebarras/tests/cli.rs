use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn help_lists_the_subcommands() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("scan"));
}

#[test]
fn version_is_reported() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("0.1.0"));
}
