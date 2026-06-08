//! Integration tests for catalog I/O and proposal conversion.

use std::path::{Path, PathBuf};

use infergen_core::{
    Catalog, CatalogEventKind, EventStatus,
    JsParser,
    NextjsAdapter,
    adapter::{Adapter, EventKind, PropertyHint, ProposedEvent},
    catalog::{from_proposals, load_catalog, merge_proposals, rescan_merge, save_catalog},
    parser::LanguageParser,
};
use infergen_types::{CatalogEntry, EventProvenance};
use tempfile::tempdir;

fn make_proposal(name: &str, kind: EventKind, abs_path: &str, confidence: f32) -> ProposedEvent {
    ProposedEvent::new(name, kind, PathBuf::from(abs_path), confidence)
}

// ---------------------------------------------------------------------------
// from_proposals
// ---------------------------------------------------------------------------

#[test]
fn from_proposals_empty() {
    let catalog = from_proposals(&[], Path::new("/root"));
    assert!(catalog.events.is_empty());
    assert_eq!(catalog.schema_version, 1);
}

#[test]
fn from_proposals_single_entry() {
    let p = make_proposal("page_viewed", EventKind::PageView, "/root/pages/index.tsx", 0.9);
    let catalog = from_proposals(&[p], Path::new("/root"));
    assert_eq!(catalog.events.len(), 1);
    let e = &catalog.events[0];
    assert_eq!(e.name, "page_viewed");
    assert_eq!(e.kind, CatalogEventKind::PageView);
    assert_eq!(e.status, EventStatus::Proposed);
    assert!((e.confidence - 0.9).abs() < 0.001);
    assert!(e.id.starts_with("evt_"));
    assert_eq!(e.id.len(), 20);
}

#[test]
fn from_proposals_generates_stable_ids() {
    let p = make_proposal("user_signed_in", EventKind::AuthEvent, "/root/auth.ts", 0.85);
    let c1 = from_proposals(&[p.clone()], Path::new("/root"));
    let c2 = from_proposals(&[p], Path::new("/root"));
    assert_eq!(c1.events[0].id, c2.events[0].id);
}

#[test]
fn from_proposals_different_events_have_different_ids() {
    let p1 = make_proposal("page_viewed", EventKind::PageView, "/root/a.tsx", 0.9);
    let p2 = make_proposal("form_submitted", EventKind::FormSubmit, "/root/b.tsx", 0.7);
    let catalog = from_proposals(&[p1, p2], Path::new("/root"));
    assert_ne!(catalog.events[0].id, catalog.events[1].id);
}

#[test]
fn from_proposals_deduplicates_identical() {
    let p1 = make_proposal("x", EventKind::PageView, "/root/a.tsx", 0.9);
    let p2 = make_proposal("x", EventKind::PageView, "/root/a.tsx", 0.9);
    let catalog = from_proposals(&[p1, p2], Path::new("/root"));
    assert_eq!(catalog.events.len(), 1);
}

#[test]
fn from_proposals_converts_all_kinds() {
    let proposals = vec![
        make_proposal("pv", EventKind::PageView, "/r/a.tsx", 0.9),
        make_proposal("ac", EventKind::ApiCall, "/r/b.ts", 0.9),
        make_proposal("ae", EventKind::AuthEvent, "/r/c.ts", 0.9),
        make_proposal("fs", EventKind::FormSubmit, "/r/d.tsx", 0.9),
        make_proposal("er", EventKind::Error, "/r/e.tsx", 0.9),
    ];
    let catalog = from_proposals(&proposals, Path::new("/r"));
    let kinds: Vec<CatalogEventKind> = catalog.events.iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&CatalogEventKind::PageView));
    assert!(kinds.contains(&CatalogEventKind::ApiCall));
    assert!(kinds.contains(&CatalogEventKind::AuthEvent));
    assert!(kinds.contains(&CatalogEventKind::FormSubmit));
    assert!(kinds.contains(&CatalogEventKind::Error));
}

#[test]
fn from_proposals_converts_properties() {
    let mut p = make_proposal("user_signed_in", EventKind::AuthEvent, "/root/auth.ts", 0.85);
    p.properties.push(PropertyHint {
        name: "method".into(),
        type_hint: Some("string".into()),
        pii_hint: false,
    });
    p.properties.push(PropertyHint {
        name: "email".into(),
        type_hint: None,
        pii_hint: true,
    });
    let catalog = from_proposals(&[p], Path::new("/root"));
    let props = &catalog.events[0].properties;
    assert_eq!(props.len(), 2);
    assert_eq!(props[0].name, "method");
    assert_eq!(props[0].prop_type, Some("string".into()));
    assert!(!props[0].pii);
    assert!(props[1].pii);
    assert!(props[1].prop_type.is_none());
}

#[test]
fn from_proposals_strips_project_root_from_provenance() {
    let p = make_proposal("page_viewed", EventKind::PageView, "/project/pages/index.tsx", 0.9);
    let catalog = from_proposals(&[p], Path::new("/project"));
    assert_eq!(
        catalog.events[0].provenance[0].source_path,
        "pages/index.tsx"
    );
}

#[test]
fn from_proposals_sorted_by_id() {
    let proposals = vec![
        make_proposal("zzz", EventKind::PageView, "/r/c.tsx", 0.9),
        make_proposal("aaa", EventKind::ApiCall, "/r/a.ts", 0.9),
        make_proposal("mmm", EventKind::FormSubmit, "/r/b.tsx", 0.9),
    ];
    let catalog = from_proposals(&proposals, Path::new("/r"));
    let ids: Vec<&str> = catalog.events.iter().map(|e| e.id.as_str()).collect();
    let mut sorted_ids = ids.clone();
    sorted_ids.sort_unstable();
    assert_eq!(ids, sorted_ids, "events must be sorted by id");
}

// ---------------------------------------------------------------------------
// merge_proposals
// ---------------------------------------------------------------------------

#[test]
fn merge_adds_new_proposals() {
    let p1 = make_proposal("page_viewed", EventKind::PageView, "/r/a.tsx", 0.9);
    let mut catalog = from_proposals(&[p1], Path::new("/r"));
    assert_eq!(catalog.events.len(), 1);

    let p2 = make_proposal("form_submitted", EventKind::FormSubmit, "/r/b.tsx", 0.7);
    let p3 = make_proposal("user_signed_in", EventKind::AuthEvent, "/r/c.ts", 0.85);
    merge_proposals(&mut catalog, &[p2, p3], Path::new("/r"));
    assert_eq!(catalog.events.len(), 3);
}

#[test]
fn merge_skips_existing_ids() {
    let p = make_proposal("page_viewed", EventKind::PageView, "/r/a.tsx", 0.9);
    let mut catalog = from_proposals(&[p.clone()], Path::new("/r"));
    let original_len = catalog.events.len();
    merge_proposals(&mut catalog, &[p], Path::new("/r"));
    assert_eq!(catalog.events.len(), original_len);
}

#[test]
fn merge_preserves_approved_status() {
    let p = make_proposal("page_viewed", EventKind::PageView, "/r/a.tsx", 0.9);
    let mut catalog = from_proposals(&[p.clone()], Path::new("/r"));
    // Simulate human approval + rename
    catalog.events[0].status = EventStatus::Approved;
    catalog.events[0].name = "custom_page_view".into();

    merge_proposals(&mut catalog, &[p], Path::new("/r"));

    assert_eq!(catalog.events.len(), 1);
    assert_eq!(catalog.events[0].status, EventStatus::Approved);
    assert_eq!(catalog.events[0].name, "custom_page_view");
}

#[test]
fn merge_preserves_ignored_status() {
    let p = make_proposal("page_viewed", EventKind::PageView, "/r/a.tsx", 0.9);
    let mut catalog = from_proposals(&[p.clone()], Path::new("/r"));
    catalog.events[0].status = EventStatus::Ignored;

    merge_proposals(&mut catalog, &[p], Path::new("/r"));

    assert_eq!(catalog.events.len(), 1);
    assert_eq!(catalog.events[0].status, EventStatus::Ignored);
}

#[test]
fn merge_output_is_sorted() {
    let p1 = make_proposal("page_viewed", EventKind::PageView, "/r/a.tsx", 0.9);
    let mut catalog = from_proposals(&[p1], Path::new("/r"));
    let p2 = make_proposal("api_called", EventKind::ApiCall, "/r/b.ts", 0.9);
    merge_proposals(&mut catalog, &[p2], Path::new("/r"));

    let ids: Vec<&str> = catalog.events.iter().map(|e| e.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "merged events must remain sorted by id");
}

// ---------------------------------------------------------------------------
// save_catalog / load_catalog round-trip
// ---------------------------------------------------------------------------

#[test]
fn round_trip_empty_catalog() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".infergen/catalog.yaml");

    let catalog = Catalog::default();
    save_catalog(&catalog, &path).unwrap();
    let loaded = load_catalog(&path).unwrap();
    assert_eq!(loaded, catalog);
}

#[test]
fn round_trip_with_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".infergen/catalog.yaml");

    let proposals = vec![
        make_proposal("page_viewed", EventKind::PageView, "/r/pages/index.tsx", 0.9),
        make_proposal("form_submitted", EventKind::FormSubmit, "/r/form.tsx", 0.7),
    ];
    let catalog = from_proposals(&proposals, Path::new("/r"));
    save_catalog(&catalog, &path).unwrap();
    let loaded = load_catalog(&path).unwrap();
    assert_eq!(loaded.schema_version, catalog.schema_version);
    assert_eq!(loaded.events.len(), catalog.events.len());
    assert_eq!(loaded.events[0].id, catalog.events[0].id);
    assert_eq!(loaded.events[0].name, catalog.events[0].name);
    assert_eq!(loaded.events[0].kind, catalog.events[0].kind);
    assert_eq!(loaded.events[0].status, catalog.events[0].status);
}

#[test]
fn save_creates_parent_directories() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("deeply/nested/dir/catalog.yaml");
    let catalog = Catalog::default();
    save_catalog(&catalog, &path).unwrap();
    assert!(path.exists());
}

#[test]
fn save_produces_stable_order() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("catalog.yaml");

    let proposals = vec![
        make_proposal("zzz", EventKind::PageView, "/r/c.tsx", 0.9),
        make_proposal("aaa", EventKind::ApiCall, "/r/a.ts", 0.9),
    ];
    let catalog = from_proposals(&proposals, Path::new("/r"));
    save_catalog(&catalog, &path).unwrap();
    save_catalog(&catalog, &path).unwrap(); // second write must be identical

    let text1 = std::fs::read_to_string(&path).unwrap();
    save_catalog(&catalog, &path).unwrap();
    let text2 = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text1, text2, "repeated saves must produce identical output");
}

#[test]
fn load_nonexistent_returns_io_error() {
    let err = load_catalog(Path::new("/nonexistent/catalog.yaml")).unwrap_err();
    assert!(matches!(err, infergen_core::Error::Io(_)));
}

#[test]
fn load_malformed_yaml_returns_catalog_parse_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("catalog.yaml");
    std::fs::write(&path, "not: valid: yaml: :::\n  - broken").unwrap();
    let err = load_catalog(&path).unwrap_err();
    assert!(matches!(err, infergen_core::Error::CatalogParse { .. }));
}

#[test]
fn yaml_output_has_expected_keys() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("catalog.yaml");

    let p = make_proposal("page_viewed", EventKind::PageView, "/r/pages/index.tsx", 0.9);
    let catalog = from_proposals(&[p], Path::new("/r"));
    save_catalog(&catalog, &path).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("schemaVersion"), "missing schemaVersion key");
    assert!(text.contains("events"), "missing events key");
    assert!(text.contains("status"), "missing status key");
    assert!(text.contains("confidence"), "missing confidence key");
    assert!(text.contains("kind"), "missing kind key");
    assert!(text.contains("sourcePath"), "missing sourcePath key");
    assert!(text.contains("pageView"), "missing pageView kind value");
    assert!(text.contains("proposed"), "missing proposed status value");
}

// ---------------------------------------------------------------------------
// End-to-end: NextjsAdapter → from_proposals
// ---------------------------------------------------------------------------

#[test]
fn nextjs_adapter_proposals_convert_to_catalog() {
    let root = PathBuf::from("/project");
    let adapter = NextjsAdapter::new(&root);
    let source = "export default function AboutPage() { return null; }";
    let file_path = root.join("pages/about.tsx");
    let parsed = JsParser.parse(&file_path, source).unwrap();

    let proposals = adapter.analyze(&parsed);
    assert!(!proposals.is_empty(), "adapter should propose at least one event");

    let catalog = from_proposals(&proposals, &root);
    assert!(!catalog.events.is_empty());

    let page_views: Vec<_> = catalog
        .events
        .iter()
        .filter(|e| e.kind == CatalogEventKind::PageView)
        .collect();
    assert!(!page_views.is_empty(), "should have at least one PageView event");

    // Provenance path must be relative (not absolute)
    for event in &catalog.events {
        for prov in &event.provenance {
            assert!(
                !prov.source_path.starts_with('/'),
                "provenance path must be relative, got: {}",
                prov.source_path
            );
        }
    }
}

// ---------------------------------------------------------------------------
// rescan_merge integration tests
// ---------------------------------------------------------------------------

fn make_catalog_entry(id: &str, name: &str, status: EventStatus) -> CatalogEntry {
    CatalogEntry {
        id: id.to_owned(),
        name: name.to_owned(),
        description: String::new(),
        status,
        confidence: 0.9,
        kind: CatalogEventKind::PageView,
        provenance: vec![EventProvenance {
            source_path: "src/index.tsx".into(),
            line: None,
            adapter: String::new(),
        }],
        properties: Vec::new(),
        providers: Vec::new(),
        package: None,
        flow_ids: Vec::new(),
    }
}

fn make_catalog(entries: Vec<CatalogEntry>) -> Catalog {
    use infergen_types::CATALOG_SCHEMA_VERSION;
    Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: entries, flows: Vec::new() }
}

#[test]
fn rescan_merge_empty_existing_and_proposals() {
    let existing = make_catalog(vec![]);
    let result = rescan_merge(&existing, &[], std::path::Path::new("/root"));
    assert!(result.events.is_empty());
}

#[test]
fn rescan_merge_fresh_scan_all_proposed() {
    let existing = make_catalog(vec![]);
    let proposals = vec![
        make_proposal("page_viewed", EventKind::PageView, "/root/pages/index.tsx", 0.9),
        make_proposal("form_submitted", EventKind::FormSubmit, "/root/form.tsx", 0.7),
    ];
    let result = rescan_merge(&existing, &proposals, std::path::Path::new("/root"));
    assert_eq!(result.events.len(), 2);
    for e in &result.events {
        assert_eq!(e.status, EventStatus::Proposed, "fresh scan should be Proposed");
    }
}

#[test]
fn rescan_merge_preserved_name_edit() {
    let proposals = vec![make_proposal("page_viewed", EventKind::PageView, "/root/a.tsx", 0.9)];
    let fresh = from_proposals(&proposals, std::path::Path::new("/root"));
    let mut renamed = fresh.events[0].clone();
    renamed.name = "my_custom_view".to_string();
    let existing = make_catalog(vec![renamed]);

    let result = rescan_merge(&existing, &proposals, std::path::Path::new("/root"));
    assert_eq!(result.events[0].name, "my_custom_view");
}

#[test]
fn rescan_merge_preserved_approved_status() {
    let proposals = vec![make_proposal("page_viewed", EventKind::PageView, "/root/a.tsx", 0.9)];
    let fresh = from_proposals(&proposals, std::path::Path::new("/root"));
    let mut approved = fresh.events[0].clone();
    approved.status = EventStatus::Approved;
    let existing = make_catalog(vec![approved]);

    let result = rescan_merge(&existing, &proposals, std::path::Path::new("/root"));
    assert_eq!(result.events[0].status, EventStatus::Approved);
}

#[test]
fn rescan_merge_preserved_ignored_status() {
    let proposals = vec![make_proposal("page_viewed", EventKind::PageView, "/root/a.tsx", 0.9)];
    let fresh = from_proposals(&proposals, std::path::Path::new("/root"));
    let mut ignored = fresh.events[0].clone();
    ignored.status = EventStatus::Ignored;
    let existing = make_catalog(vec![ignored]);

    let result = rescan_merge(&existing, &proposals, std::path::Path::new("/root"));
    assert_eq!(result.events[0].status, EventStatus::Ignored);
}

#[test]
fn rescan_merge_preserved_description() {
    let proposals = vec![make_proposal("page_viewed", EventKind::PageView, "/root/a.tsx", 0.9)];
    let fresh = from_proposals(&proposals, std::path::Path::new("/root"));
    let mut desc = fresh.events[0].clone();
    desc.description = "User-written description.".to_string();
    let existing = make_catalog(vec![desc]);

    let result = rescan_merge(&existing, &proposals, std::path::Path::new("/root"));
    assert_eq!(result.events[0].description, "User-written description.");
}

#[test]
fn rescan_merge_preserved_properties() {
    use infergen_types::EventProperty;
    let proposals = vec![make_proposal("page_viewed", EventKind::PageView, "/root/a.tsx", 0.9)];
    let fresh = from_proposals(&proposals, std::path::Path::new("/root"));
    let mut with_prop = fresh.events[0].clone();
    with_prop.properties.push(EventProperty {
        name: "human_prop".into(),
        prop_type: Some("boolean".into()),
        required: true,
        pii: false,
    });
    let existing = make_catalog(vec![with_prop]);

    let result = rescan_merge(&existing, &proposals, std::path::Path::new("/root"));
    assert_eq!(result.events[0].properties.len(), 1);
    assert_eq!(result.events[0].properties[0].name, "human_prop");
}

#[test]
fn rescan_merge_drops_stale_proposed() {
    let existing = make_catalog(vec![
        make_catalog_entry("evt_stale0000000001", "stale_event", EventStatus::Proposed),
    ]);
    let result = rescan_merge(&existing, &[], std::path::Path::new("/root"));
    assert!(result.events.is_empty(), "stale Proposed must be removed");
}

#[test]
fn rescan_merge_keeps_approved_on_disappear() {
    let existing = make_catalog(vec![
        make_catalog_entry("evt_approved000001", "important_event", EventStatus::Approved),
    ]);
    let result = rescan_merge(&existing, &[], std::path::Path::new("/root"));
    assert_eq!(result.events.len(), 1);
    assert_eq!(result.events[0].name, "important_event");
}

#[test]
fn rescan_merge_keeps_ignored_on_disappear() {
    let existing = make_catalog(vec![
        make_catalog_entry("evt_ignored0000001", "noise_event", EventStatus::Ignored),
    ]);
    let result = rescan_merge(&existing, &[], std::path::Path::new("/root"));
    assert_eq!(result.events.len(), 1);
    assert_eq!(result.events[0].status, EventStatus::Ignored);
}

#[test]
fn rescan_merge_adds_new_detection() {
    let existing = make_catalog(vec![]);
    let proposals = vec![make_proposal("brand_new_event", EventKind::ApiCall, "/root/api.ts", 0.8)];
    let result = rescan_merge(&existing, &proposals, std::path::Path::new("/root"));
    assert_eq!(result.events.len(), 1);
    assert_eq!(result.events[0].name, "brand_new_event");
    assert_eq!(result.events[0].status, EventStatus::Proposed);
}

#[test]
fn rescan_merge_sorted_output() {
    let existing = make_catalog(vec![]);
    let proposals = vec![
        make_proposal("zzz_event", EventKind::PageView, "/root/c.tsx", 0.9),
        make_proposal("aaa_event", EventKind::ApiCall, "/root/a.ts", 0.9),
    ];
    let result = rescan_merge(&existing, &proposals, std::path::Path::new("/root"));
    let ids: Vec<&str> = result.events.iter().map(|e| e.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "rescan_merge must return events sorted by id");
}

#[test]
fn rescan_merge_full_scenario() {
    // Three existing events, various statuses
    let initial_proposals = vec![
        make_proposal("matched_event", EventKind::PageView, "/root/a.tsx", 0.9),
        make_proposal("stale_proposed", EventKind::ApiCall, "/root/b.ts", 0.8),
        make_proposal("gone_approved", EventKind::AuthEvent, "/root/c.ts", 0.85),
    ];
    let first = from_proposals(&initial_proposals, std::path::Path::new("/root"));

    // Approve one event
    let mut existing = first.clone();
    for e in &mut existing.events {
        if e.name == "gone_approved" {
            e.status = EventStatus::Approved;
        }
    }

    // Second scan: matched_event reappears, stale/approved are gone, new_event appears
    let second_proposals = vec![
        make_proposal("matched_event", EventKind::PageView, "/root/a.tsx", 0.9),
        make_proposal("brand_new", EventKind::FormSubmit, "/root/d.tsx", 0.7),
    ];
    let result = rescan_merge(&existing, &second_proposals, std::path::Path::new("/root"));

    assert!(result.events.iter().any(|e| e.name == "matched_event"), "matched must survive");
    assert!(!result.events.iter().any(|e| e.name == "stale_proposed"), "stale Proposed must be dropped");
    assert!(result.events.iter().any(|e| e.name == "gone_approved"), "Approved must be kept");
    assert!(result.events.iter().any(|e| e.name == "brand_new"), "new event must be added");
    assert_eq!(
        result.events.iter().find(|e| e.name == "brand_new").unwrap().status,
        EventStatus::Proposed
    );
}
