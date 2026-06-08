use std::fs;
use std::path::Path;

use infergen_core::{
    Catalog, CatalogEntry, CatalogEventKind, EventProperty, EventProvenance, EventStatus,
    CATALOG_SCHEMA_VERSION, cross_service_check, detect_monorepo, merge_package_catalogs,
    namespace_catalog,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write(dir: &Path, name: &str, contents: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, contents).unwrap();
}

fn make_entry(id: &str, name: &str, props: &[&str]) -> CatalogEntry {
    CatalogEntry {
        id: id.into(),
        name: name.into(),
        description: String::new(),
        status: EventStatus::Proposed,
        confidence: 0.9,
        kind: CatalogEventKind::PageView,
        provenance: vec![EventProvenance {
            source_path: "src/app.ts".into(),
            line: None,
            adapter: "nextjs".into(),
        }],
        properties: props
            .iter()
            .map(|n| EventProperty {
                name: n.to_string(),
                prop_type: Some("string".into()),
                required: true,
                pii: false,
            })
            .collect(),
        providers: vec![],
        package: None,
    }
}

fn make_catalog(entries: Vec<CatalogEntry>) -> Catalog {
    Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: entries }
}

// ---------------------------------------------------------------------------
// detect_monorepo — real tempdir trees
// ---------------------------------------------------------------------------

#[test]
fn detect_monorepo_real_npm_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "package.json",
        r#"{"workspaces":["packages/*"]}"#,
    );
    write(dir.path(), "packages/frontend/package.json", r#"{"name":"frontend"}"#);
    write(dir.path(), "packages/backend/package.json", r#"{"name":"backend"}"#);

    let layout = detect_monorepo(dir.path()).unwrap();
    assert_eq!(layout.packages.len(), 2);
    let names: Vec<&str> = layout.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"frontend"));
    assert!(names.contains(&"backend"));
}

#[test]
fn detect_monorepo_real_pnpm_workspace() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "pnpm-workspace.yaml", "packages:\n  - apps/*\n");
    write(dir.path(), "apps/web/package.json", r#"{"name":"web"}"#);
    write(dir.path(), "apps/api/package.json", r#"{"name":"api"}"#);

    let layout = detect_monorepo(dir.path()).unwrap();
    assert_eq!(layout.packages.len(), 2);
    let names: Vec<&str> = layout.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"web"));
    assert!(names.contains(&"api"));
}

#[test]
fn detect_monorepo_real_cargo_workspace() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\n",
    );
    write(dir.path(), "crates/a/Cargo.toml", "[package]\nname = \"crate-a\"\nversion = \"0.1.0\"\n");
    write(dir.path(), "crates/b/Cargo.toml", "[package]\nname = \"crate-b\"\nversion = \"0.1.0\"\n");

    let layout = detect_monorepo(dir.path()).unwrap();
    assert_eq!(layout.packages.len(), 2);
    let names: Vec<&str> = layout.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"crate-a"));
    assert!(names.contains(&"crate-b"));
}

#[test]
fn detect_monorepo_multi_manifest_fallback() {
    let dir = tempfile::tempdir().unwrap();
    // No root workspace config — each service has a go.mod in an immediate subdir.
    write(dir.path(), "gateway/go.mod", "module gateway\n");
    write(dir.path(), "worker/go.mod", "module worker\n");

    let layout = detect_monorepo(dir.path()).unwrap();
    assert_eq!(layout.packages.len(), 2);
}

#[test]
fn detect_monorepo_is_monorepo_true() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "package.json", r#"{"workspaces":["packages/*"]}"#);
    write(dir.path(), "packages/a/package.json", r#"{"name":"a"}"#);
    write(dir.path(), "packages/b/package.json", r#"{"name":"b"}"#);

    let layout = detect_monorepo(dir.path()).unwrap();
    assert!(layout.is_monorepo());
}

#[test]
fn detect_monorepo_single_package_is_monorepo_false() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "package.json", r#"{"name":"single-app"}"#);

    let layout = detect_monorepo(dir.path()).unwrap();
    assert!(!layout.is_monorepo());
}

// ---------------------------------------------------------------------------
// namespace_catalog and merge_package_catalogs — integration
// ---------------------------------------------------------------------------

#[test]
fn namespace_and_merge_integration() {
    let cat_a = make_catalog(vec![
        make_entry("evt_001", "page_viewed", &[]),
        make_entry("evt_002", "button_clicked", &["label"]),
        make_entry("evt_003", "form_submitted", &["form_id"]),
    ]);
    let cat_b = make_catalog(vec![
        make_entry("evt_004", "api_called", &["endpoint"]),
        make_entry("evt_005", "user_signed_in", &["email"]),
        make_entry("evt_006", "search_performed", &["query"]),
    ]);

    let merged = merge_package_catalogs(&[("frontend", &cat_a), ("backend", &cat_b)]);
    assert_eq!(merged.events.len(), 6);
    assert!(merged.events.iter().all(|e| e.package.is_some()));
}

#[test]
fn merge_preserves_event_count() {
    let make_n_entries = |prefix: &str, n: usize| {
        (0..n)
            .map(|i| make_entry(&format!("evt_{prefix}_{i:03}"), &format!("{prefix}_event_{i}"), &[]))
            .collect::<Vec<_>>()
    };

    let cat_a = make_catalog(make_n_entries("a", 4));
    let cat_b = make_catalog(make_n_entries("b", 4));
    let cat_c = make_catalog(make_n_entries("c", 4));

    let merged =
        merge_package_catalogs(&[("svc_a", &cat_a), ("svc_b", &cat_b), ("svc_c", &cat_c)]);
    assert_eq!(merged.events.len(), 12);
}

// ---------------------------------------------------------------------------
// cross_service_check — integration
// ---------------------------------------------------------------------------

#[test]
fn cross_service_check_integration_clean() {
    let cat_a = make_catalog(vec![make_entry("evt_001", "page_viewed", &[])]);
    let cat_b = make_catalog(vec![make_entry("evt_002", "user_signed_in", &["email"])]);
    let cat_c = make_catalog(vec![make_entry("evt_003", "api_called", &["endpoint"])]);

    let issues = cross_service_check(&[
        ("frontend", &cat_a),
        ("backend", &cat_b),
        ("gateway", &cat_c),
    ]);
    assert!(issues.is_empty());
}

#[test]
fn cross_service_check_integration_conflict() {
    // Same event name, different properties.
    let cat_a = make_catalog(vec![make_entry("evt_001", "user_signed_in", &["email"])]);
    let cat_b =
        make_catalog(vec![make_entry("evt_002", "user_signed_in", &["email", "user_id"])]);

    let issues = cross_service_check(&[("frontend", &cat_a), ("backend", &cat_b)]);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].event_name, "user_signed_in");
    assert_eq!(issues[0].kind, infergen_core::ConsistencyKind::PropertyConflict);
}

// ---------------------------------------------------------------------------
// CatalogEntry package field — YAML round-trip
// ---------------------------------------------------------------------------

#[test]
fn catalog_entry_package_field_yaml_roundtrip() {
    let mut entry = make_entry("evt_001", "page_viewed", &[]);
    entry.package = Some("frontend".into());
    let catalog = make_catalog(vec![entry]);

    let yaml = serde_yaml::to_string(&catalog).unwrap();
    let back: Catalog = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back.events[0].package, Some("frontend".into()));
}

#[test]
fn catalog_entry_none_package_yaml_omits_field() {
    let entry = make_entry("evt_001", "page_viewed", &[]);
    let catalog = make_catalog(vec![entry]);

    let yaml = serde_yaml::to_string(&catalog).unwrap();
    assert!(!yaml.contains("package:"), "package field must not appear in YAML when None");
}
