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

// E4.1 — scan integration tests

#[test]
fn scan_empty_dir_creates_catalog() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("scan")
        .assert()
        .success()
        .stdout(contains("catalog saved"));
    assert!(dir.path().join(".infergen/catalog.yaml").exists());
}

#[test]
fn scan_nextjs_page_proposes_event() {
    let dir = tempdir().unwrap();
    let pages_dir = dir.path().join("pages");
    std::fs::create_dir_all(&pages_dir).unwrap();
    std::fs::write(
        pages_dir.join("index.tsx"),
        "export default function HomePage() { return null; }",
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("scan")
        .assert()
        .success();
    let catalog_path = dir.path().join(".infergen/catalog.yaml");
    assert!(catalog_path.exists());
    let contents = std::fs::read_to_string(&catalog_path).unwrap();
    assert!(contents.contains("pageView"), "expected pageView kind in catalog");
}

#[test]
fn scan_rescan_preserves_approved_event() {
    let dir = tempdir().unwrap();
    let pages_dir = dir.path().join("pages");
    std::fs::create_dir_all(&pages_dir).unwrap();
    std::fs::write(
        pages_dir.join("index.tsx"),
        "export default function HomePage() { return null; }",
    )
    .unwrap();
    // First scan
    infergen().current_dir(dir.path()).arg("scan").assert().success();

    let catalog_path = dir.path().join(".infergen/catalog.yaml");
    let yaml = std::fs::read_to_string(&catalog_path).unwrap();

    // Extract event ID — serde_yaml may or may not quote the value
    let id_start = yaml.find("evt_").unwrap_or_else(|| {
        panic!("no evt_ id found in catalog:\n{yaml}")
    });
    let id_slice = &yaml[id_start..];
    let id_end = id_slice.find(|c: char| c.is_whitespace() || c == '"').unwrap_or(id_slice.len());
    let event_id = id_slice[..id_end].to_string();

    // Approve the event
    infergen()
        .current_dir(dir.path())
        .args(["review", "approve", &event_id])
        .assert()
        .success();

    // Second scan — approved event must survive
    infergen().current_dir(dir.path()).arg("scan").assert().success();
    let after = std::fs::read_to_string(&catalog_path).unwrap();
    assert!(after.contains("approved"), "approved event must survive rescan");
    assert!(after.contains(&event_id), "approved event id must survive rescan");
}

#[test]
fn scan_rescan_removes_stale_proposed_event() {
    let dir = tempdir().unwrap();
    // Seed catalog with a Proposed event that won't be re-detected
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_stale0000000000", "stale_event", "proposed"),
    )
    .unwrap();
    // Scan empty dir — stale Proposed must be removed
    infergen().current_dir(dir.path()).arg("scan").assert().success();
    let after = std::fs::read_to_string(catalog_dir.join("catalog.yaml")).unwrap();
    assert!(!after.contains("stale_event"), "stale Proposed event must be removed");
}

#[test]
fn scan_rescan_keeps_approved_when_no_source_match() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_kept0000000000", "important_event", "approved"),
    )
    .unwrap();
    // Scan empty dir — Approved event must be kept
    infergen().current_dir(dir.path()).arg("scan").assert().success();
    let after = std::fs::read_to_string(catalog_dir.join("catalog.yaml")).unwrap();
    assert!(after.contains("important_event"), "approved event must survive");
    assert!(after.contains("approved"), "status must remain approved");
}

// ── E4.3 watch integration tests ────────────────────────────────────────────

#[test]
fn watch_once_empty_dir_creates_catalog_and_sdk() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["watch", "--once"])
        .assert()
        .success();
    assert!(dir.path().join(".infergen/catalog.yaml").exists(), "catalog must be created");
    assert!(dir.path().join("infergen.generated.ts").exists(), "SDK must be generated");
}

#[test]
fn watch_once_nextjs_page_updates_catalog() {
    let dir = tempdir().unwrap();
    let pages_dir = dir.path().join("pages");
    std::fs::create_dir_all(&pages_dir).unwrap();
    std::fs::write(
        pages_dir.join("index.tsx"),
        "export default function HomePage() { return null; }",
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["watch", "--once"])
        .assert()
        .success();
    let catalog = std::fs::read_to_string(dir.path().join(".infergen/catalog.yaml")).unwrap();
    assert!(catalog.contains("pageView"), "scan must detect page view event");
}

#[test]
fn watch_once_generates_typescript_preamble() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["watch", "--once"])
        .assert()
        .success();
    let ts = std::fs::read_to_string(dir.path().join("infergen.generated.ts")).unwrap();
    assert!(ts.contains("export interface Provider"), "SDK must have Provider interface");
    assert!(ts.contains("configureInfergen"), "SDK must have configureInfergen");
}

#[test]
fn watch_once_custom_output_path() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["watch", "--once", "--output", "sdk/analytics.ts"])
        .assert()
        .success();
    assert!(dir.path().join("sdk/analytics.ts").exists(), "SDK must be at custom path");
}

#[test]
fn watch_once_prints_status_output() {
    let dir = tempdir().unwrap();
    let output = infergen()
        .current_dir(dir.path())
        .args(["watch", "--once"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains("watch:"), "must print watch: prefix");
    assert!(!stdout.contains("watching for changes"), "--once must not enter watch loop");
}

/// Live watch test: spawns the watcher, creates a file, verifies re-scan fires.
/// Marked #[ignore] — uses real file watching and timing; run manually:
/// `cargo test -p infergen-cli watch_live -- --ignored`
#[test]
#[ignore]
fn watch_live_detects_new_source_file() {
    use assert_cmd::cargo::cargo_bin;
    use std::time::Duration;

    let dir = tempdir().unwrap();
    let mut child = std::process::Command::new(cargo_bin("infergen"))
        .args(["watch"])
        .current_dir(dir.path())
        .spawn()
        .unwrap();

    // Allow initial scan to complete
    std::thread::sleep(Duration::from_millis(800));

    // Create a Next.js page — watcher should detect and re-scan
    let pages_dir = dir.path().join("pages");
    std::fs::create_dir_all(&pages_dir).unwrap();
    std::fs::write(
        pages_dir.join("about.tsx"),
        "export default function AboutPage() { return null; }",
    )
    .unwrap();

    // Wait for debounce (300ms) + re-scan time
    std::thread::sleep(Duration::from_millis(2000));
    child.kill().ok();

    let catalog = std::fs::read_to_string(dir.path().join(".infergen/catalog.yaml")).unwrap();
    assert!(catalog.contains("pageView"), "re-scan should detect about page event");
}

// ── E4.2 check integration tests ────────────────────────────────────────────

#[test]
fn check_no_catalog_fails() {
    let dir = tempdir().unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("check")
        .assert()
        .failure()
        .stderr(contains("infergen scan"));
}

#[test]
fn check_clean_approved_catalog_succeeds() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_clean0000000000", "page_viewed", "approved"),
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("check")
        .assert()
        .success()
        .stdout(contains("OK"));
}

#[test]
fn check_proposed_event_fails() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_prop0000000000", "page_viewed", "proposed"),
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("check")
        .assert()
        .failure()
        .stdout(contains("unreviewed"));
}

#[test]
fn check_new_untracked_moment_fails() {
    let dir = tempdir().unwrap();
    // Seed catalog with an approved event whose ID won't match the scanned Next.js page event
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_0000000000000001", "some_other_event", "approved"),
    )
    .unwrap();
    // Add a Next.js page — scan will detect a new event not in the catalog
    let pages_dir = dir.path().join("pages");
    std::fs::create_dir_all(&pages_dir).unwrap();
    std::fs::write(
        pages_dir.join("index.tsx"),
        "export default function HomePage() { return null; }",
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("check")
        .assert()
        .failure()
        .stdout(contains("untracked"));
}

#[test]
fn check_convention_violation_fails() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    // "pageViewed" violates the default snake_case convention
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_conv0000000000", "pageViewed", "approved"),
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("check")
        .assert()
        .failure()
        .stdout(contains("violation"));
}

#[test]
fn check_output_lists_unreviewed_with_question_mark() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_prop1111111111", "signup_clicked", "proposed"),
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .arg("check")
        .assert()
        .failure()
        .stdout(contains("?"));
}

#[test]
fn check_custom_catalog_path() {
    let dir = tempdir().unwrap();
    let custom_catalog = dir.path().join("my_catalog.yaml");
    std::fs::write(
        &custom_catalog,
        minimal_catalog_yaml("evt_custom000000000", "home_viewed", "approved"),
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["check", "--catalog", custom_catalog.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("OK"));
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

// ── E4.4 check --json integration tests ─────────────────────────────────────

#[test]
fn check_json_clean_catalog_outputs_ok_true() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_json0000000001", "page_viewed", "approved"),
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["check", "--json"])
        .assert()
        .success()
        .stdout(contains(r#""ok": true"#))
        .stdout(contains(r#""issue_count": 0"#));
}

#[test]
fn check_json_proposed_event_outputs_ok_false() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_json0000000002", "signup_clicked", "proposed"),
    )
    .unwrap();
    infergen()
        .current_dir(dir.path())
        .args(["check", "--json"])
        .assert()
        .failure()
        .stdout(contains(r#""ok": false"#))
        .stdout(contains(r#""unreviewed""#))
        .stdout(contains("signup_clicked"));
}

#[test]
fn check_json_output_is_valid_json() {
    let dir = tempdir().unwrap();
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_json0000000003", "page_viewed", "approved"),
    )
    .unwrap();
    let output = infergen()
        .current_dir(dir.path())
        .args(["check", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&output);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert!(parsed.get("ok").is_some(), "JSON must have 'ok' field");
    assert!(parsed.get("issue_count").is_some(), "JSON must have 'issue_count' field");
    assert!(parsed.get("new_untracked").is_some(), "JSON must have 'new_untracked' field");
    assert!(parsed.get("unreviewed").is_some(), "JSON must have 'unreviewed' field");
    assert!(parsed.get("violations").is_some(), "JSON must have 'violations' field");
}

#[test]
fn check_json_new_untracked_moment_contains_event_name() {
    let dir = tempdir().unwrap();
    // Catalog has an approved event that won't match the scanned Next.js page
    let catalog_dir = dir.path().join(".infergen");
    std::fs::create_dir_all(&catalog_dir).unwrap();
    std::fs::write(
        catalog_dir.join("catalog.yaml"),
        minimal_catalog_yaml("evt_json0000000004", "some_other_event", "approved"),
    )
    .unwrap();
    // Add Next.js page — scan will detect a new untracked event
    let pages_dir = dir.path().join("pages");
    std::fs::create_dir_all(&pages_dir).unwrap();
    std::fs::write(
        pages_dir.join("index.tsx"),
        "export default function HomePage() { return null; }",
    )
    .unwrap();
    let output = infergen()
        .current_dir(dir.path())
        .args(["check", "--json"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains(r#""ok": false"#), "must be not ok");
    assert!(stdout.contains(r#""new_untracked""#), "must list new_untracked");
    // The scanned event has a name derived from the Next.js page
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("must be valid JSON");
    let untracked = parsed["new_untracked"].as_array().expect("new_untracked must be array");
    assert!(!untracked.is_empty(), "must have at least one new untracked event");
    assert!(untracked[0].get("id").is_some(), "untracked event must have id");
    assert!(untracked[0].get("name").is_some(), "untracked event must have name");
}

// ---------------------------------------------------------------------------
// E3.3 Delivery Engine CLI tests
// ---------------------------------------------------------------------------

#[test]
fn generate_output_contains_delivery_engine() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("sdk.ts");
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", out.to_str().unwrap()])
        .assert()
        .success();
    let ts = std::fs::read_to_string(&out).unwrap();
    assert!(ts.contains("DeliveryEngine"), "DeliveryEngine missing from generated SDK");
}

#[test]
fn generate_output_contains_with_delivery() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("sdk.ts");
    infergen()
        .current_dir(dir.path())
        .args(["generate", "--output", out.to_str().unwrap()])
        .assert()
        .success();
    let ts = std::fs::read_to_string(&out).unwrap();
    assert!(ts.contains("withDelivery"), "withDelivery missing from generated SDK");
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
