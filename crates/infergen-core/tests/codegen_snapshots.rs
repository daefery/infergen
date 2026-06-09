//! Codegen snapshot tests (E8.2).
//!
//! Captures the full output of each code generator against a fixed catalog
//! fixture (`tests/fixtures/catalog/e2e_catalog.yaml`).  Any future change to
//! codegen output will cause these tests to fail, surfacing the diff for
//! review.  Update with `cargo insta review` after intentional changes.

use std::path::Path;

use infergen_core::{
    Catalog, CodegenConfig, GoCodegenConfig, RubyCodegenConfig, generate_go, generate_python,
    generate_ruby, generate_typescript,
};

// ---------------------------------------------------------------------------
// Catalog fixture
// ---------------------------------------------------------------------------

fn load_e2e_catalog() -> Catalog {
    let yaml = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/catalog/e2e_catalog.yaml"),
    )
    .expect("e2e_catalog.yaml fixture not found");
    serde_yaml::from_str(&yaml).expect("e2e_catalog.yaml is not valid catalog YAML")
}

// ---------------------------------------------------------------------------
// Snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn typescript_codegen_full_snapshot() {
    let catalog = load_e2e_catalog();
    let ts = generate_typescript(&catalog, &CodegenConfig::default());
    insta::assert_snapshot!(ts);
}

#[test]
fn go_codegen_full_snapshot() {
    let catalog = load_e2e_catalog();
    let go_src = generate_go(&catalog, &GoCodegenConfig::default());
    insta::assert_snapshot!(go_src);
}

#[test]
fn python_codegen_full_snapshot() {
    let catalog = load_e2e_catalog();
    let py = generate_python(&catalog, &CodegenConfig::default());
    insta::assert_snapshot!(py);
}

#[test]
fn ruby_codegen_full_snapshot() {
    let catalog = load_e2e_catalog();
    let rb = generate_ruby(&catalog, &RubyCodegenConfig::default());
    insta::assert_snapshot!(rb);
}

// ---------------------------------------------------------------------------
// Determinism test — no snapshots needed, just equality
// ---------------------------------------------------------------------------

#[test]
fn typescript_codegen_is_deterministic() {
    let catalog = load_e2e_catalog();
    let config = CodegenConfig::default();
    let ts1 = generate_typescript(&catalog, &config);
    let ts2 = generate_typescript(&catalog, &config);
    assert_eq!(ts1, ts2, "TypeScript codegen must be deterministic (no timestamps/random IDs)");
}

#[test]
fn go_codegen_is_deterministic() {
    let catalog = load_e2e_catalog();
    let config = GoCodegenConfig::default();
    let go1 = generate_go(&catalog, &config);
    let go2 = generate_go(&catalog, &config);
    assert_eq!(go1, go2, "Go codegen must be deterministic");
}

#[test]
fn python_codegen_is_deterministic() {
    let catalog = load_e2e_catalog();
    let config = CodegenConfig::default();
    let py1 = generate_python(&catalog, &config);
    let py2 = generate_python(&catalog, &config);
    assert_eq!(py1, py2, "Python codegen must be deterministic");
}

#[test]
fn ruby_codegen_is_deterministic() {
    let catalog = load_e2e_catalog();
    let config = RubyCodegenConfig::default();
    let rb1 = generate_ruby(&catalog, &config);
    let rb2 = generate_ruby(&catalog, &config);
    assert_eq!(rb1, rb2, "Ruby codegen must be deterministic");
}

// ---------------------------------------------------------------------------
// Structural correctness — approved events present, ignored absent
// ---------------------------------------------------------------------------

#[test]
fn typescript_snapshot_contains_approved_events() {
    let catalog = load_e2e_catalog();
    let ts = generate_typescript(&catalog, &CodegenConfig::default());
    assert!(ts.contains("page_viewed"), "page_viewed should be in TS output");
    assert!(ts.contains("user_signed_in"), "user_signed_in should be in TS output");
    assert!(ts.contains("product_added_to_cart"), "product_added_to_cart should be in TS output");
    assert!(ts.contains("checkout_completed"), "checkout_completed should be in TS output");
    assert!(!ts.contains("noise_event"), "ignored noise_event must not be in TS output");
}

#[test]
fn go_snapshot_contains_approved_events() {
    let catalog = load_e2e_catalog();
    let go_src = generate_go(&catalog, &GoCodegenConfig::default());
    assert!(go_src.contains("PageViewed") || go_src.contains("page_viewed"),
        "page_viewed event should appear in Go output");
    assert!(!go_src.contains("NoiseEvent") && !go_src.contains("noise_event"),
        "ignored noise_event must not appear in Go output");
}

#[test]
fn python_snapshot_contains_approved_events() {
    let catalog = load_e2e_catalog();
    let py = generate_python(&catalog, &CodegenConfig::default());
    assert!(py.contains("page_viewed"), "page_viewed should be in Python output");
    assert!(!py.contains("noise_event"), "ignored noise_event must not be in Python output");
}

#[test]
fn ruby_snapshot_contains_approved_events() {
    let catalog = load_e2e_catalog();
    let rb = generate_ruby(&catalog, &RubyCodegenConfig::default());
    assert!(rb.contains("page_viewed") || rb.contains("PageViewed"),
        "page_viewed should be in Ruby output");
    assert!(!rb.contains("noise_event") && !rb.contains("NoiseEvent"),
        "ignored noise_event must not be in Ruby output");
}
