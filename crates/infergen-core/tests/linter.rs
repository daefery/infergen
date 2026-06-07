//! Integration tests for the E1.3 naming convention linter.
//!
//! Tests verify the full pipeline: catalog entries → lint_catalog → violations.
//! Some tests build proposals through the NextjsAdapter to confirm real-world
//! names pass default snake_case linting end-to-end.

use infergen_core::{
    LintRule, lint_catalog,
    config::NamingConfig,
    from_proposals,
};
use infergen_types::{Catalog, CatalogEntry, CatalogEventKind, EventProvenance, EventStatus};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_entry(id: &str, name: &str, status: EventStatus) -> CatalogEntry {
    CatalogEntry {
        id: id.to_string(),
        name: name.to_string(),
        description: String::new(),
        status,
        kind: CatalogEventKind::PageView,
        confidence: 0.9,
        properties: vec![],
        providers: vec![],
        provenance: vec![EventProvenance {
            source_path: "src/page.tsx".into(),
            line: None,
            adapter: "nextjs".into(),
        }],
    }
}

fn make_catalog(entries: Vec<CatalogEntry>) -> Catalog {
    Catalog {
        schema_version: 1,
        events: entries,
    }
}

fn snake_naming() -> NamingConfig {
    NamingConfig::default()
}

fn camel_naming() -> NamingConfig {
    NamingConfig {
        case: "camelCase".into(),
        ..Default::default()
    }
}

fn pascal_naming() -> NamingConfig {
    NamingConfig {
        case: "PascalCase".into(),
        ..Default::default()
    }
}

// ── nextjs pipeline → lint ────────────────────────────────────────────────────

#[test]
fn nextjs_catalog_passes_snake_lint() {
    // Proposals from the NextjsAdapter already produce snake_case names, so
    // a catalog built from them should have zero lint violations.
    use infergen_core::{Adapter, JsParser, NextjsAdapter, LanguageParser};
    use std::path::{Path, PathBuf};

    let src = r#"
        export default function LoginPage() {
            async function handleLoginSubmit(e) {
                e.preventDefault();
            }
            return <form onSubmit={handleLoginSubmit}><button type="submit">Login</button></form>;
        }
    "#;

    let path = PathBuf::from("src/app/login/page.tsx");
    let parsed = JsParser.parse(&path, src).expect("parse ok");

    let root = Path::new("/proj");
    let adapter = NextjsAdapter::new(root);
    let proposals = adapter.analyze(&parsed);

    let catalog = from_proposals(&proposals, root);
    let violations = lint_catalog(&catalog, &snake_naming());
    assert!(
        violations.is_empty(),
        "NextjsAdapter proposals should produce valid snake_case names; got: {violations:#?}"
    );
}

// ── CaseViolation ─────────────────────────────────────────────────────────────

#[test]
fn bad_name_produces_case_violation() {
    let catalog = make_catalog(vec![make_entry("evt_001", "UserSignedIn", EventStatus::Proposed)]);
    let violations = lint_catalog(&catalog, &snake_naming());
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].rule, LintRule::CaseViolation);
}

#[test]
fn bad_name_suggestion_is_correct() {
    let catalog = make_catalog(vec![make_entry("evt_001", "UserSignedIn", EventStatus::Proposed)]);
    let violations = lint_catalog(&catalog, &snake_naming());
    assert_eq!(violations[0].suggestion.as_deref(), Some("user_signed_in"));
}

#[test]
fn camel_config_flags_snake_names() {
    let catalog =
        make_catalog(vec![make_entry("evt_001", "user_signed_in", EventStatus::Proposed)]);
    let violations = lint_catalog(&catalog, &camel_naming());
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].rule, LintRule::CaseViolation);
    assert_eq!(violations[0].suggestion.as_deref(), Some("userSignedIn"));
}

#[test]
fn pascal_config_flags_snake_names() {
    let catalog =
        make_catalog(vec![make_entry("evt_001", "user_signed_in", EventStatus::Proposed)]);
    let violations = lint_catalog(&catalog, &pascal_naming());
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].rule, LintRule::CaseViolation);
    assert_eq!(violations[0].suggestion.as_deref(), Some("UserSignedIn"));
}

// ── Ignored entries ───────────────────────────────────────────────────────────

#[test]
fn ignored_entry_never_linted() {
    let catalog = make_catalog(vec![make_entry("evt_001", "BADNAME", EventStatus::Ignored)]);
    let violations = lint_catalog(&catalog, &snake_naming());
    assert!(violations.is_empty());
}

// ── Structural violations ─────────────────────────────────────────────────────

#[test]
fn consecutive_underscores_caught() {
    let catalog =
        make_catalog(vec![make_entry("evt_001", "user__signed", EventStatus::Proposed)]);
    let violations = lint_catalog(&catalog, &snake_naming());
    let rules: Vec<_> = violations.iter().map(|v| &v.rule).collect();
    assert!(rules.contains(&&LintRule::ConsecutiveUnderscores));
}

#[test]
fn empty_name_rule_fires() {
    let catalog = make_catalog(vec![make_entry("evt_001", "", EventStatus::Proposed)]);
    let violations = lint_catalog(&catalog, &snake_naming());
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].rule, LintRule::EmptyName);
    assert_eq!(violations[0].suggestion, None);
}

#[test]
fn multiple_violations_per_entry() {
    // "__BadName_": ConsecutiveUnderscores + LeadingOrTrailingUnderscore + CaseViolation
    let catalog = make_catalog(vec![make_entry("evt_001", "__BadName_", EventStatus::Proposed)]);
    let violations = lint_catalog(&catalog, &snake_naming());
    let rules: Vec<_> = violations.iter().map(|v| &v.rule).collect();
    assert!(rules.contains(&&LintRule::ConsecutiveUnderscores));
    assert!(rules.contains(&&LintRule::LeadingOrTrailingUnderscore));
    assert!(rules.contains(&&LintRule::CaseViolation));
    assert_eq!(violations.len(), 3);
}

// ── Metadata correctness ──────────────────────────────────────────────────────

#[test]
fn lint_result_event_id_matches() {
    let catalog = make_catalog(vec![make_entry("evt_abc123", "BadName", EventStatus::Proposed)]);
    let violations = lint_catalog(&catalog, &snake_naming());
    assert!(!violations.is_empty());
    assert_eq!(violations[0].event_id, "evt_abc123");
}

#[test]
fn multiple_entries_violations_independent() {
    let catalog = make_catalog(vec![
        make_entry("evt_001", "valid_name", EventStatus::Proposed),
        make_entry("evt_002", "BadName", EventStatus::Proposed),
    ]);
    let violations = lint_catalog(&catalog, &snake_naming());
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].event_id, "evt_002");
}
