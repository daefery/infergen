//! Integration tests for E1.5 review workflow.
//!
//! Verifies that mutations persist through YAML round-trips and that diff
//! works with realistic catalog files.

use infergen_core::{
    Catalog, CatalogEntry, CatalogEventKind, EntryChange, EventProperty, EventProvenance,
    EventStatus, approve, diff_catalogs, ignore, remove_property, rename, set_description,
    upsert_property, load_catalog, merge_proposals, save_catalog,
};
use infergen_core::adapter::{EventKind, ProposedEvent};
use infergen_types::CATALOG_SCHEMA_VERSION;
use std::path::PathBuf;
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_entry(id: &str, name: &str, status: EventStatus) -> CatalogEntry {
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
            adapter: "nextjs".into(),
        }],
        properties: Vec::new(),
        providers: Vec::new(),
        package: None,
    }
}

fn make_catalog(entries: Vec<CatalogEntry>) -> Catalog {
    Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: entries }
}

fn make_proposal(name: &str, kind: EventKind, path: &str) -> ProposedEvent {
    ProposedEvent::new(name, kind, PathBuf::from(path), 0.9)
}

// ---------------------------------------------------------------------------
// Round-trip persistence tests
// ---------------------------------------------------------------------------

#[test]
fn approve_persists_through_yaml_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("catalog.yaml");
    let mut cat = make_catalog(vec![make_entry("evt_001", "page_viewed", EventStatus::Proposed)]);
    approve(&mut cat, "evt_001").unwrap();
    save_catalog(&cat, &path).unwrap();
    let loaded = load_catalog(&path).unwrap();
    assert_eq!(loaded.events[0].status, EventStatus::Approved);
}

#[test]
fn ignore_persists_through_yaml_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("catalog.yaml");
    let mut cat = make_catalog(vec![make_entry("evt_002", "noise", EventStatus::Proposed)]);
    ignore(&mut cat, "evt_002").unwrap();
    save_catalog(&cat, &path).unwrap();
    let loaded = load_catalog(&path).unwrap();
    assert_eq!(loaded.events[0].status, EventStatus::Ignored);
}

#[test]
fn rename_persists_through_yaml_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("catalog.yaml");
    let mut cat = make_catalog(vec![make_entry("evt_003", "old_name", EventStatus::Proposed)]);
    rename(&mut cat, "evt_003", "new_name").unwrap();
    save_catalog(&cat, &path).unwrap();
    let loaded = load_catalog(&path).unwrap();
    assert_eq!(loaded.events[0].name, "new_name");
}

#[test]
fn set_description_persists_through_yaml_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("catalog.yaml");
    let mut cat = make_catalog(vec![make_entry("evt_004", "page_viewed", EventStatus::Proposed)]);
    set_description(&mut cat, "evt_004", "Fires on every page navigation.").unwrap();
    save_catalog(&cat, &path).unwrap();
    let loaded = load_catalog(&path).unwrap();
    assert_eq!(loaded.events[0].description, "Fires on every page navigation.");
}

#[test]
fn upsert_property_persists_through_yaml_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("catalog.yaml");
    let mut cat = make_catalog(vec![make_entry("evt_005", "user_signed_in", EventStatus::Proposed)]);
    upsert_property(&mut cat, "evt_005", EventProperty {
        name: "method".into(),
        prop_type: Some("string".into()),
        required: true,
        pii: false,
    }).unwrap();
    save_catalog(&cat, &path).unwrap();
    let loaded = load_catalog(&path).unwrap();
    let prop = &loaded.events[0].properties[0];
    assert_eq!(prop.name, "method");
    assert_eq!(prop.prop_type.as_deref(), Some("string"));
    assert!(prop.required);
    assert!(!prop.pii);
}

#[test]
fn remove_property_persists_through_yaml_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("catalog.yaml");
    let mut cat = make_catalog(vec![make_entry("evt_006", "user_signed_in", EventStatus::Proposed)]);
    upsert_property(&mut cat, "evt_006", EventProperty {
        name: "method".into(),
        prop_type: Some("string".into()),
        required: false,
        pii: false,
    }).unwrap();
    remove_property(&mut cat, "evt_006", "method").unwrap();
    save_catalog(&cat, &path).unwrap();
    let loaded = load_catalog(&path).unwrap();
    assert!(loaded.events[0].properties.is_empty());
}

// ---------------------------------------------------------------------------
// Diff integration tests
// ---------------------------------------------------------------------------

#[test]
fn diff_fresh_vs_existing_catalog() {
    // existing: one approved (evt_007) + one ignored (evt_ign)
    // proposed: evt_007 still present, evt_ign gone, evt_new added
    let mut approved = make_entry("evt_007", "page_viewed", EventStatus::Approved);
    approved.id = "evt_007".into();
    let mut ignored = make_entry("evt_ign", "noise", EventStatus::Ignored);
    ignored.id = "evt_ign".into();
    let existing = make_catalog(vec![approved.clone(), ignored.clone()]);

    let mut new_event = make_entry("evt_new", "user_signed_up", EventStatus::Proposed);
    new_event.id = "evt_new".into();
    let mut proposed_approved = approved.clone();
    proposed_approved.status = EventStatus::Proposed;
    let proposed = make_catalog(vec![proposed_approved, new_event]);

    let diff = diff_catalogs(&existing, &proposed);
    assert_eq!(diff.added.len(), 1, "one new event");
    assert_eq!(diff.added[0].id, "evt_new");
    assert!(diff.removed.is_empty(), "ignored entry not in removed");
    assert_eq!(diff.unchanged.len(), 1, "approved entry unchanged");
}

#[test]
fn diff_shows_renamed_entry_as_modified() {
    let existing = make_catalog(vec![make_entry("evt_008", "page_viewed", EventStatus::Approved)]);
    let mut renamed = make_entry("evt_008", "home_viewed", EventStatus::Proposed);
    renamed.id = "evt_008".into();
    let proposed = make_catalog(vec![renamed]);
    let diff = diff_catalogs(&existing, &proposed);
    assert_eq!(diff.modified.len(), 1);
    let name_change = diff.modified[0].changes.iter().find(|c| matches!(c, EntryChange::NameChanged { .. }));
    assert!(name_change.is_some(), "NameChanged detected");
    if let Some(EntryChange::NameChanged { from, to }) = name_change {
        assert_eq!(from, "page_viewed");
        assert_eq!(to, "home_viewed");
    }
}

#[test]
fn approve_then_merge_preserves_status() {
    // Approve an event in the catalog, then merge the same proposals again.
    // The approved entry must remain approved (merge_proposals never overwrites).
    let dir = tempdir().unwrap();
    let root = dir.path();

    let proposals = vec![
        make_proposal("page_viewed", EventKind::PageView, "/proj/pages/index.tsx"),
    ];
    let mut cat = infergen_core::from_proposals(&proposals, root);
    assert_eq!(cat.events[0].status, EventStatus::Proposed);

    // Approve.
    let id = cat.events[0].id.clone();
    approve(&mut cat, &id).unwrap();
    assert_eq!(cat.events[0].status, EventStatus::Approved);

    // Re-merge same proposals — should not touch the approved entry.
    merge_proposals(&mut cat, &proposals, root);
    assert_eq!(cat.events.len(), 1, "no duplicate");
    assert_eq!(cat.events[0].status, EventStatus::Approved, "status preserved");
}
