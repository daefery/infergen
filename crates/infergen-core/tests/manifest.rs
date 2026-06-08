//! Integration tests for E7.4 — data-collection manifest export.

use infergen_core::{
    generate_manifest, render_markdown, Manifest, MANIFEST_VERSION,
    Catalog, CatalogEntry, CatalogEventKind, EventProperty, EventProvenance, EventStatus,
    CATALOG_SCHEMA_VERSION,
};
use infergen_types::{EventFlow, FlowKind, FlowStep};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn make_entry(name: &str, status: EventStatus, kind: CatalogEventKind) -> CatalogEntry {
    CatalogEntry {
        id: format!("evt_{name}"),
        name: name.into(),
        description: String::new(),
        status,
        confidence: 0.9,
        kind,
        provenance: Vec::new(),
        properties: Vec::new(),
        providers: Vec::new(),
        package: None,
        flow_ids: Vec::new(),
    }
}

fn make_catalog(events: Vec<CatalogEntry>) -> Catalog {
    Catalog { schema_version: CATALOG_SCHEMA_VERSION, events, flows: Vec::new() }
}

// ---------------------------------------------------------------------------
// MANIFEST_VERSION
// ---------------------------------------------------------------------------

#[test]
fn manifest_version_constant_is_one() {
    assert_eq!(MANIFEST_VERSION, 1);
}

// ---------------------------------------------------------------------------
// Summary counts
// ---------------------------------------------------------------------------

#[test]
fn empty_catalog_all_counts_zero() {
    let m = generate_manifest(&make_catalog(vec![]), None);
    assert_eq!(m.manifest_version, 1);
    assert_eq!(m.summary.total_events, 0);
    assert_eq!(m.summary.approved_events, 0);
    assert_eq!(m.summary.proposed_events, 0);
    assert_eq!(m.summary.ignored_events, 0);
    assert_eq!(m.summary.events_with_pii, 0);
    assert_eq!(m.summary.total_pii_properties, 0);
    assert!(m.summary.packages.is_empty());
    assert!(m.summary.destinations.is_empty());
    assert!(m.events.is_empty());
    assert!(m.pii_inventory.is_empty());
    assert!(m.providers.is_empty());
    assert!(m.flows.is_empty());
}

#[test]
fn status_counts_are_correct() {
    let entries = vec![
        make_entry("a", EventStatus::Approved, CatalogEventKind::PageView),
        make_entry("b", EventStatus::Approved, CatalogEventKind::PageView),
        make_entry("c", EventStatus::Proposed, CatalogEventKind::ApiCall),
        make_entry("d", EventStatus::Ignored, CatalogEventKind::Error),
    ];
    let m = generate_manifest(&make_catalog(entries), None);
    assert_eq!(m.summary.total_events, 4);
    assert_eq!(m.summary.approved_events, 2);
    assert_eq!(m.summary.proposed_events, 1);
    assert_eq!(m.summary.ignored_events, 1);
}

// ---------------------------------------------------------------------------
// Event status & kind strings
// ---------------------------------------------------------------------------

#[test]
fn approved_event_status_string() {
    let m = generate_manifest(
        &make_catalog(vec![make_entry("x", EventStatus::Approved, CatalogEventKind::PageView)]),
        None,
    );
    assert_eq!(m.events[0].status, "approved");
}

#[test]
fn proposed_event_status_string() {
    let m = generate_manifest(
        &make_catalog(vec![make_entry("x", EventStatus::Proposed, CatalogEventKind::ApiCall)]),
        None,
    );
    assert_eq!(m.events[0].status, "proposed");
}

#[test]
fn ignored_event_status_string() {
    let m = generate_manifest(
        &make_catalog(vec![make_entry("x", EventStatus::Ignored, CatalogEventKind::Error)]),
        None,
    );
    assert_eq!(m.events[0].status, "ignored");
}

#[test]
fn all_kind_strings_are_camel_case() {
    use CatalogEventKind::*;
    let pairs = [
        (PageView, "pageView"),
        (ApiCall, "apiCall"),
        (AuthEvent, "authEvent"),
        (FormSubmit, "formSubmit"),
        (ButtonClick, "buttonClick"),
        (Search, "search"),
        (Error, "error"),
    ];
    for (i, (kind, expected)) in pairs.iter().enumerate() {
        let entry = make_entry(&format!("e{i}"), EventStatus::Approved, *kind);
        let m = generate_manifest(&make_catalog(vec![entry]), None);
        assert_eq!(m.events[0].kind, *expected, "kind mismatch for {:?}", kind);
    }
}

// ---------------------------------------------------------------------------
// Events sorted by name
// ---------------------------------------------------------------------------

#[test]
fn events_sorted_alphabetically() {
    let entries = vec![
        make_entry("z_event", EventStatus::Approved, CatalogEventKind::PageView),
        make_entry("a_event", EventStatus::Approved, CatalogEventKind::PageView),
        make_entry("m_event", EventStatus::Approved, CatalogEventKind::PageView),
    ];
    let m = generate_manifest(&make_catalog(entries), None);
    assert_eq!(m.events[0].name, "a_event");
    assert_eq!(m.events[1].name, "m_event");
    assert_eq!(m.events[2].name, "z_event");
}

// ---------------------------------------------------------------------------
// PII inventory
// ---------------------------------------------------------------------------

#[test]
fn single_pii_property_in_inventory() {
    let mut entry = make_entry("user_signed_in", EventStatus::Approved, CatalogEventKind::AuthEvent);
    entry.properties.push(EventProperty {
        name: "email".into(),
        prop_type: Some("string".into()),
        required: true,
        pii: true,
    });
    let m = generate_manifest(&make_catalog(vec![entry]), None);
    assert_eq!(m.summary.events_with_pii, 1);
    assert_eq!(m.summary.total_pii_properties, 1);
    assert_eq!(m.pii_inventory.len(), 1);
    assert_eq!(m.pii_inventory[0].property_name, "email");
    assert_eq!(m.pii_inventory[0].event_count, 1);
    assert_eq!(m.pii_inventory[0].events, vec!["user_signed_in"]);
}

#[test]
fn pii_aggregated_across_two_events() {
    let mut e1 = make_entry("user_signed_in", EventStatus::Approved, CatalogEventKind::AuthEvent);
    e1.properties.push(EventProperty { name: "email".into(), prop_type: None, required: false, pii: true });
    let mut e2 = make_entry("user_updated", EventStatus::Approved, CatalogEventKind::FormSubmit);
    e2.properties.push(EventProperty { name: "email".into(), prop_type: None, required: false, pii: true });
    let m = generate_manifest(&make_catalog(vec![e1, e2]), None);
    assert_eq!(m.pii_inventory.len(), 1);
    assert_eq!(m.pii_inventory[0].event_count, 2);
    assert_eq!(m.summary.total_pii_properties, 2);
    assert_eq!(m.summary.events_with_pii, 2);
    // events list should be sorted
    let events = &m.pii_inventory[0].events;
    assert_eq!(events, &vec!["user_signed_in", "user_updated"]);
}

#[test]
fn non_pii_property_not_in_inventory() {
    let mut entry = make_entry("page_viewed", EventStatus::Approved, CatalogEventKind::PageView);
    entry.properties.push(EventProperty {
        name: "path".into(),
        prop_type: Some("string".into()),
        required: true,
        pii: false,
    });
    let m = generate_manifest(&make_catalog(vec![entry]), None);
    assert!(m.pii_inventory.is_empty());
    assert_eq!(m.summary.events_with_pii, 0);
    assert_eq!(m.summary.total_pii_properties, 0);
}

#[test]
fn pii_inventory_sorted_desc_by_count() {
    // "email" appears in 2 events, "phone" appears in 1 → email should come first.
    let mut e1 = make_entry("a", EventStatus::Approved, CatalogEventKind::AuthEvent);
    e1.properties.push(EventProperty { name: "email".into(), prop_type: None, required: false, pii: true });
    e1.properties.push(EventProperty { name: "phone".into(), prop_type: None, required: false, pii: true });
    let mut e2 = make_entry("b", EventStatus::Approved, CatalogEventKind::FormSubmit);
    e2.properties.push(EventProperty { name: "email".into(), prop_type: None, required: false, pii: true });
    let m = generate_manifest(&make_catalog(vec![e1, e2]), None);
    assert_eq!(m.pii_inventory[0].property_name, "email");
    assert_eq!(m.pii_inventory[0].event_count, 2);
    assert_eq!(m.pii_inventory[1].property_name, "phone");
    assert_eq!(m.pii_inventory[1].event_count, 1);
}

// ---------------------------------------------------------------------------
// Provider aggregation
// ---------------------------------------------------------------------------

#[test]
fn provider_counts_across_events() {
    let mut e1 = make_entry("a", EventStatus::Approved, CatalogEventKind::PageView);
    e1.providers = vec!["posthog".into()];
    let mut e2 = make_entry("b", EventStatus::Approved, CatalogEventKind::PageView);
    e2.providers = vec!["posthog".into()];
    let mut e3 = make_entry("c", EventStatus::Approved, CatalogEventKind::PageView);
    e3.providers = vec!["posthog".into(), "segment".into()];
    let m = generate_manifest(&make_catalog(vec![e1, e2, e3]), None);
    // posthog: 3, segment: 1 — posthog first (higher count)
    assert_eq!(m.providers[0].name, "posthog");
    assert_eq!(m.providers[0].event_count, 3);
    assert_eq!(m.providers[1].name, "segment");
    assert_eq!(m.providers[1].event_count, 1);
}

#[test]
fn event_with_no_providers_not_in_aggregate() {
    let entry = make_entry("unrouted", EventStatus::Approved, CatalogEventKind::ApiCall);
    let m = generate_manifest(&make_catalog(vec![entry]), None);
    assert!(m.providers.is_empty());
    assert!(m.summary.destinations.is_empty());
}

// ---------------------------------------------------------------------------
// Locations
// ---------------------------------------------------------------------------

#[test]
fn location_with_line_number() {
    let mut entry = make_entry("auth_event", EventStatus::Approved, CatalogEventKind::AuthEvent);
    entry.provenance.push(EventProvenance {
        source_path: "src/auth.ts".into(),
        line: Some(45),
        adapter: "nextjs".into(),
    });
    let m = generate_manifest(&make_catalog(vec![entry]), None);
    assert_eq!(m.events[0].locations, vec!["src/auth.ts:45"]);
}

#[test]
fn location_without_line_number() {
    let mut entry = make_entry("auth_event", EventStatus::Approved, CatalogEventKind::AuthEvent);
    entry.provenance.push(EventProvenance {
        source_path: "src/auth.ts".into(),
        line: None,
        adapter: "nextjs".into(),
    });
    let m = generate_manifest(&make_catalog(vec![entry]), None);
    assert_eq!(m.events[0].locations, vec!["src/auth.ts"]);
}

// ---------------------------------------------------------------------------
// Packages
// ---------------------------------------------------------------------------

#[test]
fn packages_deduplicated_and_sorted() {
    let mut e1 = make_entry("e1", EventStatus::Approved, CatalogEventKind::PageView);
    e1.package = Some("frontend".into());
    let mut e2 = make_entry("e2", EventStatus::Approved, CatalogEventKind::ApiCall);
    e2.package = Some("backend".into());
    let mut e3 = make_entry("e3", EventStatus::Approved, CatalogEventKind::ApiCall);
    e3.package = Some("frontend".into());
    let m = generate_manifest(&make_catalog(vec![e1, e2, e3]), None);
    assert_eq!(m.summary.packages, vec!["backend", "frontend"]);
}

// ---------------------------------------------------------------------------
// Flows
// ---------------------------------------------------------------------------

#[test]
fn flow_step_count_in_manifest() {
    let flow = EventFlow {
        id: "flow_abc".into(),
        name: "checkout".into(),
        kind: FlowKind::Checkout,
        description: String::new(),
        steps: vec![
            FlowStep { event_id: "evt_a".into(), step_index: 0, optional: false },
            FlowStep { event_id: "evt_b".into(), step_index: 1, optional: false },
            FlowStep { event_id: "evt_c".into(), step_index: 2, optional: false },
        ],
        confidence: 0.85,
    };
    let cat = Catalog {
        schema_version: CATALOG_SCHEMA_VERSION,
        events: Vec::new(),
        flows: vec![flow],
    };
    let m = generate_manifest(&cat, None);
    assert_eq!(m.flows.len(), 1);
    assert_eq!(m.flows[0].name, "checkout");
    assert_eq!(m.flows[0].kind, "checkout");
    assert_eq!(m.flows[0].step_count, 3);
}

// ---------------------------------------------------------------------------
// generated_at
// ---------------------------------------------------------------------------

#[test]
fn generated_at_none_omitted_from_json() {
    let m = generate_manifest(&make_catalog(vec![]), None);
    assert!(m.generated_at.is_none());
    let json = serde_json::to_string(&m).unwrap();
    assert!(!json.contains("generatedAt"));
}

#[test]
fn generated_at_some_present_in_json() {
    let m = generate_manifest(&make_catalog(vec![]), Some("unix:1700000000".into()));
    let json = serde_json::to_string(&m).unwrap();
    assert!(json.contains("generatedAt"));
    assert!(json.contains("1700000000"));
}

// ---------------------------------------------------------------------------
// JSON roundtrip
// ---------------------------------------------------------------------------

#[test]
fn json_roundtrip() {
    let mut entry = make_entry("page_viewed", EventStatus::Approved, CatalogEventKind::PageView);
    entry.properties.push(EventProperty {
        name: "path".into(),
        prop_type: Some("string".into()),
        required: true,
        pii: false,
    });
    entry.providers = vec!["posthog".into()];
    let m = generate_manifest(&make_catalog(vec![entry]), None);
    let json = serde_json::to_string(&m).expect("serialize");
    let back: Manifest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(m, back);
}

// ---------------------------------------------------------------------------
// render_markdown
// ---------------------------------------------------------------------------

#[test]
fn render_markdown_has_summary_section() {
    let m = generate_manifest(&make_catalog(vec![]), None);
    let md = render_markdown(&m);
    assert!(md.contains("# Data-Collection Manifest"));
    assert!(md.contains("## Summary"));
}

#[test]
fn render_markdown_empty_catalog_no_events_or_pii_section() {
    let m = generate_manifest(&make_catalog(vec![]), None);
    let md = render_markdown(&m);
    assert!(!md.contains("## Events"));
    assert!(!md.contains("## PII Inventory"));
    assert!(!md.contains("## Destinations"));
    assert!(!md.contains("## Flows"));
}

#[test]
fn render_markdown_contains_event_name() {
    let entry = make_entry("user_signed_in", EventStatus::Approved, CatalogEventKind::AuthEvent);
    let m = generate_manifest(&make_catalog(vec![entry]), None);
    let md = render_markdown(&m);
    assert!(md.contains("user_signed_in"));
    assert!(md.contains("## Events"));
}

#[test]
fn render_markdown_marks_pii_property() {
    let mut entry = make_entry("user_signed_in", EventStatus::Approved, CatalogEventKind::AuthEvent);
    entry.properties.push(EventProperty {
        name: "email".into(),
        prop_type: Some("string".into()),
        required: true,
        pii: true,
    });
    let m = generate_manifest(&make_catalog(vec![entry]), None);
    let md = render_markdown(&m);
    assert!(md.contains("**PII**"), "PII properties must be highlighted");
    assert!(md.contains("PII Inventory"), "PII Inventory section must appear");
}

#[test]
fn render_markdown_non_pii_has_no_pii_inventory() {
    let mut entry = make_entry("page_viewed", EventStatus::Approved, CatalogEventKind::PageView);
    entry.properties.push(EventProperty {
        name: "path".into(),
        prop_type: Some("string".into()),
        required: true,
        pii: false,
    });
    let m = generate_manifest(&make_catalog(vec![entry]), None);
    let md = render_markdown(&m);
    assert!(!md.contains("PII Inventory"));
}

#[test]
fn render_markdown_includes_destinations_section_when_providers_exist() {
    let mut entry = make_entry("page_viewed", EventStatus::Approved, CatalogEventKind::PageView);
    entry.providers = vec!["posthog".into()];
    let m = generate_manifest(&make_catalog(vec![entry]), None);
    let md = render_markdown(&m);
    assert!(md.contains("## Destinations"));
    assert!(md.contains("posthog"));
}
