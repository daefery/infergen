//! Review workflow — event status mutations and two-catalog diff (E1.5).

use std::collections::HashMap;

use infergen_types::{Catalog, CatalogEntry, CatalogEventKind, EventProperty, EventStatus};

use crate::{Error, Result};

// ---------------------------------------------------------------------------
// Diff types
// ---------------------------------------------------------------------------

/// A change detected between an existing catalog entry and the proposed re-scan.
#[derive(Debug, Clone, PartialEq)]
pub enum EntryChange {
    /// The event name changed between existing and proposed.
    NameChanged {
        /// Previous name.
        from: String,
        /// Proposed name.
        to: String,
    },
    /// The event kind changed between existing and proposed.
    KindChanged {
        /// Previous kind.
        from: CatalogEventKind,
        /// Proposed kind.
        to: CatalogEventKind,
    },
    /// A property present in the proposed scan was absent from the existing catalog.
    PropertyAdded(EventProperty),
    /// A property present in the existing catalog was absent from the proposed scan.
    PropertyRemoved(String),
    /// A property with the same name differs in type or PII flag.
    PropertyChanged {
        /// Property name.
        name: String,
        /// Existing property value.
        from: EventProperty,
        /// Proposed property value.
        to: EventProperty,
    },
}

/// An entry present in both catalogs with at least one detected change.
#[derive(Debug, Clone)]
pub struct DiffEntry {
    /// Stable event ID.
    pub id: String,
    /// The entry as it exists in the current on-disk catalog.
    pub existing: CatalogEntry,
    /// The entry as the re-scan proposes it.
    pub proposed: CatalogEntry,
    /// All detected changes between `existing` and `proposed`.
    pub changes: Vec<EntryChange>,
}

/// Structured difference between an existing catalog and a fresh proposed catalog.
///
/// `removed` excludes `Ignored` entries — silently-excluded events do not
/// surface as "missing" noise on every re-scan.
#[derive(Debug, Default)]
pub struct CatalogDiff {
    /// Events in `proposed` absent from `existing` (new detections).
    pub added: Vec<CatalogEntry>,
    /// Events in `existing` (status != Ignored) absent from `proposed`.
    pub removed: Vec<CatalogEntry>,
    /// Events in both with at least one changed field.
    pub modified: Vec<DiffEntry>,
    /// Events in both with no detected change.
    pub unchanged: Vec<CatalogEntry>,
}

impl CatalogDiff {
    /// `true` if added, removed, and modified are all empty.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn find_entry_mut<'c>(catalog: &'c mut Catalog, id: &str) -> Result<&'c mut CatalogEntry> {
    catalog
        .events
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| Error::EventNotFound { id: id.to_owned() })
}

fn compute_changes(existing: &CatalogEntry, proposed: &CatalogEntry) -> Vec<EntryChange> {
    let mut changes = Vec::new();

    if existing.name != proposed.name {
        changes.push(EntryChange::NameChanged {
            from: existing.name.clone(),
            to: proposed.name.clone(),
        });
    }

    if existing.kind != proposed.kind {
        changes.push(EntryChange::KindChanged {
            from: existing.kind,
            to: proposed.kind,
        });
    }

    let existing_props: HashMap<&str, &EventProperty> =
        existing.properties.iter().map(|p| (p.name.as_str(), p)).collect();
    let proposed_props: HashMap<&str, &EventProperty> =
        proposed.properties.iter().map(|p| (p.name.as_str(), p)).collect();

    for (name, prop) in &proposed_props {
        if let Some(existing_prop) = existing_props.get(name) {
            if *existing_prop != *prop {
                changes.push(EntryChange::PropertyChanged {
                    name: name.to_string(),
                    from: (*existing_prop).clone(),
                    to: (*prop).clone(),
                });
            }
        } else {
            changes.push(EntryChange::PropertyAdded((*prop).clone()));
        }
    }

    for name in existing_props.keys() {
        if !proposed_props.contains_key(name) {
            changes.push(EntryChange::PropertyRemoved(name.to_string()));
        }
    }

    changes
}

// ---------------------------------------------------------------------------
// Mutation functions
// ---------------------------------------------------------------------------

/// Set the entry's status to `Approved`.
///
/// # Errors
/// Returns `EventNotFound` if `id` is not present.
pub fn approve(catalog: &mut Catalog, id: &str) -> Result<()> {
    find_entry_mut(catalog, id)?.status = EventStatus::Approved;
    Ok(())
}

/// Set the entry's status to `Ignored`.
///
/// # Errors
/// Returns `EventNotFound` if `id` is not present.
pub fn ignore(catalog: &mut Catalog, id: &str) -> Result<()> {
    find_entry_mut(catalog, id)?.status = EventStatus::Ignored;
    Ok(())
}

/// Rename an event.
///
/// # Errors
/// - `EventNotFound` if `id` is not present.
/// - `InvalidEventName` if `new_name` is empty after trimming.
pub fn rename(catalog: &mut Catalog, id: &str, new_name: &str) -> Result<()> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidEventName {
            name: new_name.to_owned(),
            reason: "empty".to_owned(),
        });
    }
    find_entry_mut(catalog, id)?.name = trimmed.to_owned();
    Ok(())
}

/// Set or replace the human-readable description on an event.
///
/// # Errors
/// Returns `EventNotFound` if `id` is not present.
pub fn set_description(catalog: &mut Catalog, id: &str, description: &str) -> Result<()> {
    find_entry_mut(catalog, id)?.description = description.to_owned();
    Ok(())
}

/// Insert or update a property on an event.
///
/// If a property with `prop.name` already exists it is replaced; otherwise
/// the property is appended.
///
/// # Errors
/// Returns `EventNotFound` if `event_id` is not present.
pub fn upsert_property(catalog: &mut Catalog, event_id: &str, prop: EventProperty) -> Result<()> {
    let entry = find_entry_mut(catalog, event_id)?;
    if let Some(existing) = entry.properties.iter_mut().find(|p| p.name == prop.name) {
        *existing = prop;
    } else {
        entry.properties.push(prop);
    }
    Ok(())
}

/// Remove a property by name from an event.
///
/// Idempotent — returns `Ok(())` if no property with `prop_name` exists.
///
/// # Errors
/// Returns `EventNotFound` if `event_id` is not present.
pub fn remove_property(catalog: &mut Catalog, event_id: &str, prop_name: &str) -> Result<()> {
    let entry = find_entry_mut(catalog, event_id)?;
    entry.properties.retain(|p| p.name != prop_name);
    Ok(())
}

// ---------------------------------------------------------------------------
// Diff
// ---------------------------------------------------------------------------

/// Compute the structured difference between `existing` and `proposed`.
///
/// `proposed` is typically built by `catalog::from_proposals`. `existing` is
/// the current on-disk catalog. Keyed on stable ID.
#[must_use]
pub fn diff_catalogs(existing: &Catalog, proposed: &Catalog) -> CatalogDiff {
    let existing_map: HashMap<&str, &CatalogEntry> =
        existing.events.iter().map(|e| (e.id.as_str(), e)).collect();
    let proposed_map: HashMap<&str, &CatalogEntry> =
        proposed.events.iter().map(|e| (e.id.as_str(), e)).collect();

    let mut diff = CatalogDiff::default();

    for (id, entry) in &proposed_map {
        if !existing_map.contains_key(id) {
            diff.added.push((*entry).clone());
        }
    }

    for (id, entry) in &existing_map {
        if !proposed_map.contains_key(id) && entry.status != EventStatus::Ignored {
            diff.removed.push((*entry).clone());
        }
    }

    for (id, existing_entry) in &existing_map {
        if let Some(proposed_entry) = proposed_map.get(id) {
            let changes = compute_changes(existing_entry, proposed_entry);
            if changes.is_empty() {
                diff.unchanged.push((*existing_entry).clone());
            } else {
                diff.modified.push(DiffEntry {
                    id: id.to_string(),
                    existing: (*existing_entry).clone(),
                    proposed: (*proposed_entry).clone(),
                    changes,
                });
            }
        }
    }

    diff.added.sort_by(|a, b| a.id.cmp(&b.id));
    diff.removed.sort_by(|a, b| a.id.cmp(&b.id));
    diff.modified.sort_by(|a, b| a.id.cmp(&b.id));
    diff.unchanged.sort_by(|a, b| a.id.cmp(&b.id));

    diff
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use infergen_types::{CatalogEventKind, EventProvenance, EventStatus};

    fn make_catalog(entries: Vec<CatalogEntry>) -> Catalog {
        use infergen_types::CATALOG_SCHEMA_VERSION;
        Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: entries }
    }

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
        }
    }

    // --- approve ---

    #[test]
    fn approve_sets_status() {
        let mut cat = make_catalog(vec![make_entry("evt_001", "page_viewed", EventStatus::Proposed)]);
        approve(&mut cat, "evt_001").unwrap();
        assert_eq!(cat.events[0].status, EventStatus::Approved);
    }

    #[test]
    fn approve_idempotent() {
        let mut cat = make_catalog(vec![make_entry("evt_001", "page_viewed", EventStatus::Approved)]);
        approve(&mut cat, "evt_001").unwrap();
        assert_eq!(cat.events[0].status, EventStatus::Approved);
    }

    #[test]
    fn approve_unknown_id_errors() {
        let mut cat = make_catalog(vec![]);
        let err = approve(&mut cat, "evt_missing").unwrap_err();
        assert!(matches!(err, Error::EventNotFound { .. }));
        assert!(err.to_string().contains("evt_missing"));
    }

    // --- ignore ---

    #[test]
    fn ignore_sets_status() {
        let mut cat = make_catalog(vec![make_entry("evt_002", "noise_event", EventStatus::Proposed)]);
        ignore(&mut cat, "evt_002").unwrap();
        assert_eq!(cat.events[0].status, EventStatus::Ignored);
    }

    // --- rename ---

    #[test]
    fn rename_changes_name() {
        let mut cat = make_catalog(vec![make_entry("evt_003", "old_name", EventStatus::Proposed)]);
        rename(&mut cat, "evt_003", "new_name").unwrap();
        assert_eq!(cat.events[0].name, "new_name");
    }

    #[test]
    fn rename_trims_whitespace() {
        let mut cat = make_catalog(vec![make_entry("evt_003", "old_name", EventStatus::Proposed)]);
        rename(&mut cat, "evt_003", "  trimmed  ").unwrap();
        assert_eq!(cat.events[0].name, "trimmed");
    }

    #[test]
    fn rename_empty_name_errors() {
        let mut cat = make_catalog(vec![make_entry("evt_003", "old_name", EventStatus::Proposed)]);
        let err = rename(&mut cat, "evt_003", "").unwrap_err();
        assert!(matches!(err, Error::InvalidEventName { .. }));
    }

    #[test]
    fn rename_whitespace_only_errors() {
        let mut cat = make_catalog(vec![make_entry("evt_003", "old_name", EventStatus::Proposed)]);
        let err = rename(&mut cat, "evt_003", "   ").unwrap_err();
        assert!(matches!(err, Error::InvalidEventName { .. }));
    }

    #[test]
    fn rename_unknown_id_errors() {
        let mut cat = make_catalog(vec![]);
        let err = rename(&mut cat, "evt_missing", "x").unwrap_err();
        assert!(matches!(err, Error::EventNotFound { .. }));
    }

    // --- set_description ---

    #[test]
    fn set_description_works() {
        let mut cat = make_catalog(vec![make_entry("evt_004", "page_viewed", EventStatus::Proposed)]);
        set_description(&mut cat, "evt_004", "Fires when the home page loads.").unwrap();
        assert_eq!(cat.events[0].description, "Fires when the home page loads.");
    }

    // --- upsert_property ---

    fn make_prop(name: &str, t: Option<&str>, pii: bool) -> EventProperty {
        EventProperty { name: name.into(), prop_type: t.map(Into::into), required: false, pii }
    }

    #[test]
    fn upsert_property_adds_new() {
        let mut cat = make_catalog(vec![make_entry("evt_005", "x", EventStatus::Proposed)]);
        upsert_property(&mut cat, "evt_005", make_prop("email", Some("string"), true)).unwrap();
        assert_eq!(cat.events[0].properties.len(), 1);
        assert_eq!(cat.events[0].properties[0].name, "email");
    }

    #[test]
    fn upsert_property_replaces_existing() {
        let mut cat = make_catalog(vec![make_entry("evt_005", "x", EventStatus::Proposed)]);
        upsert_property(&mut cat, "evt_005", make_prop("email", None, false)).unwrap();
        upsert_property(&mut cat, "evt_005", make_prop("email", Some("string"), true)).unwrap();
        assert_eq!(cat.events[0].properties.len(), 1);
        assert_eq!(cat.events[0].properties[0].prop_type, Some("string".into()));
        assert!(cat.events[0].properties[0].pii);
    }

    // --- remove_property ---

    #[test]
    fn remove_property_removes() {
        let mut cat = make_catalog(vec![make_entry("evt_006", "x", EventStatus::Proposed)]);
        upsert_property(&mut cat, "evt_006", make_prop("method", Some("string"), false)).unwrap();
        remove_property(&mut cat, "evt_006", "method").unwrap();
        assert!(cat.events[0].properties.is_empty());
    }

    #[test]
    fn remove_property_missing_is_ok() {
        let mut cat = make_catalog(vec![make_entry("evt_006", "x", EventStatus::Proposed)]);
        remove_property(&mut cat, "evt_006", "nonexistent").unwrap();
    }

    // --- diff_catalogs ---

    #[test]
    fn diff_empty_catalogs() {
        let existing = make_catalog(vec![]);
        let proposed = make_catalog(vec![]);
        let diff = diff_catalogs(&existing, &proposed);
        assert!(diff.is_clean());
    }

    #[test]
    fn diff_added_event() {
        let existing = make_catalog(vec![]);
        let proposed = make_catalog(vec![make_entry("evt_new", "new_event", EventStatus::Proposed)]);
        let diff = diff_catalogs(&existing, &proposed);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].id, "evt_new");
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_removed_non_ignored() {
        let existing = make_catalog(vec![make_entry("evt_old", "old_event", EventStatus::Approved)]);
        let proposed = make_catalog(vec![]);
        let diff = diff_catalogs(&existing, &proposed);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].id, "evt_old");
    }

    #[test]
    fn diff_ignored_not_removed() {
        let existing = make_catalog(vec![make_entry("evt_ign", "noise", EventStatus::Ignored)]);
        let proposed = make_catalog(vec![]);
        let diff = diff_catalogs(&existing, &proposed);
        assert!(diff.removed.is_empty());
        assert!(diff.is_clean());
    }

    #[test]
    fn diff_unchanged_entry() {
        let entry = make_entry("evt_same", "page_viewed", EventStatus::Approved);
        let mut proposed_entry = entry.clone();
        proposed_entry.status = EventStatus::Proposed;
        let existing = make_catalog(vec![entry]);
        let proposed = make_catalog(vec![proposed_entry]);
        let diff = diff_catalogs(&existing, &proposed);
        assert_eq!(diff.unchanged.len(), 1);
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn diff_name_changed() {
        let existing = make_catalog(vec![make_entry("evt_007", "page_viewed", EventStatus::Proposed)]);
        let mut renamed = make_entry("evt_007", "home_viewed", EventStatus::Proposed);
        renamed.id = "evt_007".into();
        let proposed = make_catalog(vec![renamed]);
        let diff = diff_catalogs(&existing, &proposed);
        assert_eq!(diff.modified.len(), 1);
        assert!(diff.modified[0].changes.iter().any(|c| matches!(c, EntryChange::NameChanged { .. })));
    }

    #[test]
    fn diff_property_added() {
        let existing = make_catalog(vec![make_entry("evt_008", "page_viewed", EventStatus::Proposed)]);
        let mut with_prop = make_entry("evt_008", "page_viewed", EventStatus::Proposed);
        with_prop.properties.push(make_prop("route", Some("string"), false));
        let proposed = make_catalog(vec![with_prop]);
        let diff = diff_catalogs(&existing, &proposed);
        assert_eq!(diff.modified.len(), 1);
        assert!(diff.modified[0].changes.iter().any(|c| matches!(c, EntryChange::PropertyAdded(_))));
    }

    #[test]
    fn diff_property_removed() {
        let mut with_prop = make_entry("evt_009", "page_viewed", EventStatus::Proposed);
        with_prop.properties.push(make_prop("route", Some("string"), false));
        let existing = make_catalog(vec![with_prop]);
        let proposed = make_catalog(vec![make_entry("evt_009", "page_viewed", EventStatus::Proposed)]);
        let diff = diff_catalogs(&existing, &proposed);
        assert_eq!(diff.modified.len(), 1);
        assert!(diff.modified[0].changes.iter().any(|c| matches!(c, EntryChange::PropertyRemoved(_))));
    }

    #[test]
    fn diff_property_type_changed() {
        let mut old = make_entry("evt_010", "page_viewed", EventStatus::Proposed);
        old.properties.push(make_prop("count", None, false));
        let mut new = make_entry("evt_010", "page_viewed", EventStatus::Proposed);
        new.properties.push(make_prop("count", Some("number"), false));
        let existing = make_catalog(vec![old]);
        let proposed = make_catalog(vec![new]);
        let diff = diff_catalogs(&existing, &proposed);
        assert_eq!(diff.modified.len(), 1);
        assert!(diff.modified[0].changes.iter().any(|c| matches!(c, EntryChange::PropertyChanged { .. })));
    }

    #[test]
    fn diff_is_clean_true_when_no_changes() {
        let diff = CatalogDiff::default();
        assert!(diff.is_clean());
    }
}
