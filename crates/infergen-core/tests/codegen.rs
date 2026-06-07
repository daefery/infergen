//! Integration tests for E2.2 TypeScript codegen.
//!
//! Tests the full pipeline: build catalog from proposals, approve events,
//! generate TypeScript, verify structure and content.

use infergen_core::{
    Catalog, CatalogEntry, CatalogEventKind, CodegenConfig, EventProperty, EventProvenance,
    EventStatus, approve, generate_typescript,
};
use infergen_types::CATALOG_SCHEMA_VERSION;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_catalog(entries: Vec<CatalogEntry>) -> Catalog {
    Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: entries }
}

fn make_entry(name: &str, status: EventStatus) -> CatalogEntry {
    CatalogEntry {
        id: format!("evt_{name}"),
        name: name.to_owned(),
        description: String::new(),
        status,
        confidence: 0.9,
        kind: CatalogEventKind::PageView,
        provenance: vec![EventProvenance {
            source_path: "src/index.tsx".into(),
            line: None,
            adapter: "nextjs".into(),
        }],
        properties: Vec::new(),
        providers: Vec::new(),
    }
}

fn make_prop(name: &str, t: Option<&str>, pii: bool) -> EventProperty {
    EventProperty { name: name.into(), prop_type: t.map(Into::into), required: false, pii }
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_approved_events_in_output() {
    let mut cat = make_catalog(vec![
        make_entry("page_viewed", EventStatus::Approved),
        make_entry("user_signed_in", EventStatus::Approved),
        make_entry("noise", EventStatus::Ignored),
        make_entry("maybe", EventStatus::Proposed),
    ]);
    // Approved is already set, but ensure ignore/proposed are excluded
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    assert!(ts.contains("page_viewed"), "approved event missing");
    assert!(ts.contains("user_signed_in"), "approved event missing");
    assert!(!ts.contains("\"noise\""), "ignored event present");
    assert!(!ts.contains("\"maybe\""), "proposed event present");
    // Silence unused mut warning
    let _ = &mut cat;
}

#[test]
fn full_pipeline_interface_has_correct_ts_types() {
    let mut entry = make_entry("api_called", EventStatus::Approved);
    entry.properties.push(make_prop("method", Some("string"), false));
    entry.properties.push(make_prop("count", Some("number"), false));
    entry.properties.push(make_prop("cached", Some("boolean"), false));
    let cat = make_catalog(vec![entry]);
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    assert!(ts.contains("method: string;"), "wrong string type");
    assert!(ts.contains("count: number;"), "wrong number type");
    assert!(ts.contains("cached: boolean;"), "wrong boolean type");
}

#[test]
fn full_pipeline_pii_flag_propagates() {
    let mut entry = make_entry("user_signed_in", EventStatus::Approved);
    entry.properties.push(make_prop("email", Some("string"), true));
    let cat = make_catalog(vec![entry]);
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    assert!(ts.contains("@pii"), "PII tag missing");
}

#[test]
fn full_pipeline_description_in_jsdoc() {
    let mut entry = make_entry("page_viewed", EventStatus::Approved);
    entry.description = "Fired on every page navigation.".into();
    let cat = make_catalog(vec![entry]);
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    assert!(ts.contains("Fired on every page navigation."), "description missing");
}

#[test]
fn full_pipeline_unknown_type_for_untyped_prop() {
    let mut entry = make_entry("page_viewed", EventStatus::Approved);
    entry.properties.push(make_prop("mystery", None, false));
    let cat = make_catalog(vec![entry]);
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    assert!(ts.contains("mystery: unknown;"), "unknown type missing");
}

#[test]
fn full_pipeline_include_proposed_flag() {
    let cat = make_catalog(vec![make_entry("proposed_event", EventStatus::Proposed)]);
    let config = CodegenConfig { include_proposed: true };
    let ts = generate_typescript(&cat, &config);
    assert!(ts.contains("proposed_event"), "proposed event missing with flag");
}

#[test]
fn full_pipeline_empty_properties_interface() {
    let cat = make_catalog(vec![make_entry("click_happened", EventStatus::Approved)]);
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    assert!(ts.contains("ClickHappenedProperties {}"), "empty interface missing");
}

#[test]
fn full_pipeline_track_object_has_all_events() {
    let cat = make_catalog(vec![
        make_entry("event_a", EventStatus::Approved),
        make_entry("event_b", EventStatus::Approved),
        make_entry("event_c", EventStatus::Approved),
    ]);
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    assert!(ts.contains("eventA: trackEventA"), "eventA missing from track");
    assert!(ts.contains("eventB: trackEventB"), "eventB missing from track");
    assert!(ts.contains("eventC: trackEventC"), "eventC missing from track");
}

#[test]
fn full_pipeline_output_contains_no_current_year() {
    // Verify idempotency: output must not contain 2026 or similar
    // (no timestamp that would change each run).
    let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved)]);
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    // The year 2026 should not appear as a timestamp in the output.
    assert!(!ts.contains("2026-"), "timestamp in output breaks idempotency");
}

#[test]
fn full_pipeline_properties_sorted_alphabetically() {
    let mut entry = make_entry("api_called", EventStatus::Approved);
    entry.properties.push(make_prop("zebra", Some("string"), false));
    entry.properties.push(make_prop("alpha", Some("string"), false));
    entry.properties.push(make_prop("mango", Some("string"), false));
    let cat = make_catalog(vec![entry]);
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    let alpha_pos = ts.find("alpha: string").unwrap();
    let mango_pos = ts.find("mango: string").unwrap();
    let zebra_pos = ts.find("zebra: string").unwrap();
    assert!(
        alpha_pos < mango_pos && mango_pos < zebra_pos,
        "properties not sorted: alpha={alpha_pos} mango={mango_pos} zebra={zebra_pos}"
    );
}

#[test]
fn full_pipeline_has_provider_interface() {
    let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved)]);
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    assert!(ts.contains("export interface Provider"), "Provider interface missing");
    assert!(ts.contains("export function configureInfergen"), "configureInfergen missing");
    assert!(ts.contains("let _providers: Provider[] = []"), "_providers missing");
}

#[test]
fn full_pipeline_track_fn_dispatches_via_providers() {
    let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved)]);
    let ts = generate_typescript(&cat, &CodegenConfig::default());
    assert!(
        ts.contains("_providers.forEach(p => p.track(\"page_viewed\", properties))"),
        "dispatch call missing\noutput:\n{ts}"
    );
}

#[test]
fn full_pipeline_empty_catalog_has_preamble() {
    let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
    assert!(ts.contains("export interface Provider"), "preamble missing for empty catalog");
    assert!(ts.contains("configureInfergen"), "configureInfergen missing for empty catalog");
}

#[test]
fn full_pipeline_approve_then_generate() {
    // Simulate the user journey: scan → approve → generate
    let mut cat = make_catalog(vec![
        make_entry("page_viewed", EventStatus::Proposed),
        make_entry("user_signed_in", EventStatus::Proposed),
    ]);

    // Approve only one event
    approve(&mut cat, "evt_page_viewed").unwrap();

    let ts = generate_typescript(&cat, &CodegenConfig::default());
    assert!(ts.contains("page_viewed"), "approved event missing");
    assert!(!ts.contains("user_signed_in"), "unapproved event present");
}
