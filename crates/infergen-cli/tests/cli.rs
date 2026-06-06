//! Integration tests for the `infergen` binary.

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::tempdir;

fn infergen() -> Command {
    Command::cargo_bin("infergen").unwrap()
}

#[test]
fn prints_version_flag() {
    infergen()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_lists_all_subcommands() {
    infergen()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("init"))
        .stdout(contains("scan"))
        .stdout(contains("generate"))
        .stdout(contains("check"))
        .stdout(contains("watch"));
}

#[test]
fn no_args_prints_banner() {
    infergen()
        .assert()
        .success()
        .stdout(contains("core engine"))
        .stdout(contains("config schema"));
}

#[test]
fn scan_stub_not_implemented() {
    infergen()
        .arg("scan")
        .assert()
        .success()
        .stdout(contains("not yet implemented"))
        .stdout(contains("E0.4"));
}

#[test]
fn generate_stub_not_implemented() {
    infergen()
        .arg("generate")
        .assert()
        .success()
        .stdout(contains("not yet implemented"))
        .stdout(contains("E2.1"));
}

#[test]
fn init_writes_config() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();
    let path = dir.path().join("infergen.config.json");
    assert!(path.is_file());
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("schemaVersion"));
}

#[test]
fn init_detects_nextjs() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"next":"14"}}"#,
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();
    let contents = std::fs::read_to_string(dir.path().join("infergen.config.json")).unwrap();
    assert!(contents.contains("next-js"));
    assert!(contents.contains("react"));
}

#[test]
fn init_refuses_existing_without_force() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();
    infergen()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

#[test]
fn init_force_overwrites() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();
    infergen()
        .current_dir(dir.path())
        .args(["init", "--force"])
        .assert()
        .success();
}

#[test]
fn init_toml_format() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["init", "--format", "toml"])
        .assert()
        .success();
    let path = dir.path().join("infergen.config.toml");
    assert!(path.is_file());
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("schemaVersion"));
}
