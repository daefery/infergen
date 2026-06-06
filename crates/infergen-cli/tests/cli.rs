//! Integration tests for the `infergen` binary.

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn prints_version_flag() {
    Command::cargo_bin("infergen")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn default_run_prints_scaffold_banner() {
    Command::cargo_bin("infergen")
        .unwrap()
        .assert()
        .success()
        .stdout(contains("scaffold ready"))
        .stdout(contains("core engine"));
}

#[test]
fn help_flag_succeeds() {
    Command::cargo_bin("infergen")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("infergen"));
}
