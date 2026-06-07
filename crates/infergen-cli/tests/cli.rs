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
        .stdout(contains("watch"))
        .stdout(contains("review"));
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
fn generate_empty_catalog_succeeds() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", "out.ts"])
        .assert()
        .success()
        .stdout(contains("out.ts"));
    let ts = std::fs::read_to_string(dir.path().join("out.ts")).unwrap();
    assert!(ts.contains("EventName = never"), "empty catalog should have EventName = never");
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

// ---------------------------------------------------------------------------
// generate sub-command tests
// ---------------------------------------------------------------------------

#[test]
fn generate_writes_output_file() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_gen0000000000001", "page_viewed", "approved"),
    ).unwrap();
    let out = dir.path().join("sdk.ts");
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", out.to_str().unwrap()])
        .assert()
        .success();
    assert!(out.exists(), "output file not created");
    let ts = std::fs::read_to_string(&out).unwrap();
    assert!(ts.contains("page_viewed"), "event missing from generated SDK");
}

#[test]
fn generate_default_output_infergen_generated_ts() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_gen0000000000002", "home_viewed", "approved"),
    ).unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("generate")
        .assert()
        .success();
    let out = dir.path().join("infergen.generated.ts");
    assert!(out.exists(), "infergen.generated.ts not created");
}

#[test]
fn generate_reports_event_count() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    // Two approved events
    let yaml = format!(
        "schemaVersion: 1\nevents:\n{}\n{}",
        minimal_event_yaml("evt_gen0000000000003", "event_a", "approved"),
        minimal_event_yaml("evt_gen0000000000004", "event_b", "approved"),
    );
    std::fs::write(catalog_dir.join("catalog.yaml"), &yaml).unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", "out.ts"])
        .assert()
        .success()
        .stdout(contains("2 events"));
}

// ---------------------------------------------------------------------------
// generate --check tests
// ---------------------------------------------------------------------------

#[test]
fn generate_check_up_to_date_exits_zero() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("sdk.ts");
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", out.to_str().unwrap()])
        .assert()
        .success();
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", out.to_str().unwrap(), "--check"])
        .assert()
        .success()
        .stdout(contains("up to date"));
}

#[test]
fn generate_check_missing_file_exits_nonzero() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", "sdk.ts", "--check"])
        .assert()
        .failure()
        .stderr(contains("stale"));
}

#[test]
fn generate_check_stale_file_exits_nonzero() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("sdk.ts");
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", out.to_str().unwrap()])
        .assert()
        .success();
    std::fs::write(&out, "// stale content\n").unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", out.to_str().unwrap(), "--check"])
        .assert()
        .failure()
        .stderr(contains("stale"));
}

#[test]
fn generate_check_does_not_write_file() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("sdk.ts");
    let sentinel = "// sentinel content\n";
    std::fs::write(&out, sentinel).unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", out.to_str().unwrap(), "--check"])
        .assert()
        .failure();
    let contents = std::fs::read_to_string(&out).unwrap();
    assert_eq!(contents, sentinel, "--check must not overwrite the file");
}

// ---------------------------------------------------------------------------
// generate with provider config tests
// ---------------------------------------------------------------------------

#[test]
fn generate_with_posthog_config_emits_adapter() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("infergen.config.json"),
        r#"{"providers":[{"name":"posthog"}]}"#,
    ).unwrap();
    let out = dir.path().join("sdk.ts");
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", out.to_str().unwrap()])
        .assert()
        .success();
    let ts = std::fs::read_to_string(&out).unwrap();
    assert!(ts.contains("PostHogProvider"), "PostHogProvider not generated");
    assert!(ts.contains("us.i.posthog.com"), "PostHog endpoint missing");
}

#[test]
fn generate_without_config_no_adapter_section() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("sdk.ts");
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", out.to_str().unwrap()])
        .assert()
        .success();
    let ts = std::fs::read_to_string(&out).unwrap();
    assert!(!ts.contains("PostHogProvider"), "unexpected adapter without config");
    assert!(!ts.contains("Provider Adapters"), "unexpected adapter section without config");
}

/// Single-event YAML block for multi-event fixture building.
fn minimal_event_yaml(id: &str, name: &str, status: &str) -> String {
    format!(
        r#"  - id: "{id}"
    name: "{name}"
    description: ""
    status: {status}
    confidence: 0.9
    kind: pageView
    provenance:
      - sourcePath: "src/index.tsx"
        adapter: "nextjs"
    properties: []
    providers: []
"#
    )
}

// ---------------------------------------------------------------------------
// review sub-command tests
// ---------------------------------------------------------------------------

/// Minimal well-formed catalog.yaml fixture for CLI tests.
fn minimal_catalog_yaml(event_id: &str, name: &str, status: &str) -> String {
    format!(
        r#"schemaVersion: 1
events:
  - id: "{event_id}"
    name: "{name}"
    description: ""
    status: {status}
    confidence: 0.9
    kind: pageView
    provenance:
      - sourcePath: "src/index.tsx"
        adapter: "nextjs"
    properties: []
    providers: []
"#
    )
}

#[test]
fn review_list_empty_catalog() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["review", "list"])
        .assert()
        .success()
        .stdout(contains("0 events"));
}

#[test]
fn review_approve_sets_status() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    let catalog_path = catalog_dir.join("catalog.yaml");
    std::fs::write(&catalog_path, minimal_catalog_yaml("evt_aabbccddeeff0011", "page_viewed", "proposed")).unwrap();

    infergen()
        .current_dir(dir.path())
        .args(["review", "approve", "evt_aabbccddeeff0011"])
        .assert()
        .success()
        .stdout(contains("approved"));

    let contents = std::fs::read_to_string(&catalog_path).unwrap();
    assert!(contents.contains("approved"), "catalog should contain approved status");
}

#[test]
fn review_ignore_sets_status() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    let catalog_path = catalog_dir.join("catalog.yaml");
    std::fs::write(&catalog_path, minimal_catalog_yaml("evt_1122334455667788", "noise", "proposed")).unwrap();

    infergen()
        .current_dir(dir.path())
        .args(["review", "ignore", "evt_1122334455667788"])
        .assert()
        .success()
        .stdout(contains("ignored"));

    let contents = std::fs::read_to_string(&catalog_path).unwrap();
    assert!(contents.contains("ignored"));
}

#[test]
fn review_rename_changes_name() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    let catalog_path = catalog_dir.join("catalog.yaml");
    std::fs::write(&catalog_path, minimal_catalog_yaml("evt_aabbccddeeff0022", "old_name", "proposed")).unwrap();

    infergen()
        .current_dir(dir.path())
        .args(["review", "rename", "evt_aabbccddeeff0022", "new_name"])
        .assert()
        .success()
        .stdout(contains("new_name"));

    let contents = std::fs::read_to_string(&catalog_path).unwrap();
    assert!(contents.contains("new_name"));
}

#[test]
fn review_describe_sets_description() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    let catalog_path = catalog_dir.join("catalog.yaml");
    std::fs::write(&catalog_path, minimal_catalog_yaml("evt_aabbccddeeff0033", "page_viewed", "proposed")).unwrap();

    infergen()
        .current_dir(dir.path())
        .args(["review", "describe", "evt_aabbccddeeff0033", "my description"])
        .assert()
        .success()
        .stdout(contains("description set"));

    let contents = std::fs::read_to_string(&catalog_path).unwrap();
    assert!(contents.contains("my description"));
}

#[test]
fn review_diff_shows_added_event() {
    let dir = tempdir().unwrap();

    // existing: empty catalog
    let existing_path = dir.path().join("existing.yaml");
    std::fs::write(&existing_path, "schemaVersion: 1\nevents: []\n").unwrap();

    // proposed: one new event
    let proposed_path = dir.path().join("proposed.yaml");
    std::fs::write(
        &proposed_path,
        minimal_catalog_yaml("evt_aabbccddeeff0044", "user_signed_up", "proposed"),
    ).unwrap();

    infergen()
        .current_dir(dir.path())
        .args([
            "review",
            "--catalog",
            existing_path.to_str().unwrap(),
            "diff",
            proposed_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("Added (1)"));
}

#[test]
fn review_unknown_id_exits_nonzero() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    let catalog_path = catalog_dir.join("catalog.yaml");
    std::fs::write(&catalog_path, "schemaVersion: 1\nevents: []\n").unwrap();

    infergen()
        .current_dir(dir.path())
        .args(["review", "approve", "evt_0000000000000000"])
        .assert()
        .failure();
}
