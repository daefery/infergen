//! Catalog I/O — YAML read/write and proposal conversion.
//!
//! Stable ID scheme: FNV-1a 64-bit over `"{name}:{source_path}:{kind}"`,
//! implemented inline (no extra crate). Produces `evt_{016hex}`.

use std::collections::HashSet;
use std::path::Path;

use infergen_types::{
    Catalog, CatalogEntry, CatalogEventKind, EventProperty, EventProvenance, EventStatus,
    CATALOG_SCHEMA_VERSION,
};

use crate::{Error, ProposedEvent, Result, adapter::EventKind};

// ---------------------------------------------------------------------------
// Stable ID generation
// ---------------------------------------------------------------------------

/// FNV-1a 64-bit hash over `"{name}:{source_path}:{kind}"`.
///
/// Deterministic and portable across platforms and Rust versions.
fn generate_stable_id(name: &str, source_path: &str, kind: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for byte in format!("{name}:{source_path}:{kind}").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("evt_{hash:016x}")
}

// ---------------------------------------------------------------------------
// Type conversions
// ---------------------------------------------------------------------------

fn event_kind_to_catalog(kind: EventKind) -> CatalogEventKind {
    match kind {
        EventKind::PageView => CatalogEventKind::PageView,
        EventKind::ApiCall => CatalogEventKind::ApiCall,
        EventKind::AuthEvent => CatalogEventKind::AuthEvent,
        EventKind::FormSubmit => CatalogEventKind::FormSubmit,
        EventKind::ButtonClick => CatalogEventKind::ButtonClick,
        EventKind::Search => CatalogEventKind::Search,
        EventKind::Error => CatalogEventKind::Error,
    }
}

fn kind_to_str(kind: EventKind) -> &'static str {
    match kind {
        EventKind::PageView => "pageView",
        EventKind::ApiCall => "apiCall",
        EventKind::AuthEvent => "authEvent",
        EventKind::FormSubmit => "formSubmit",
        EventKind::ButtonClick => "buttonClick",
        EventKind::Search => "search",
        EventKind::Error => "error",
    }
}

fn proposal_to_entry(proposal: &ProposedEvent, project_root: &Path) -> CatalogEntry {
    let rel_path = proposal
        .source_path
        .strip_prefix(project_root)
        .unwrap_or(&proposal.source_path)
        .to_string_lossy()
        .to_string();

    let id = generate_stable_id(&proposal.name, &rel_path, kind_to_str(proposal.kind));

    let provenance = vec![EventProvenance {
        source_path: rel_path,
        line: None,
        adapter: proposal.adapter.clone(),
    }];

    let properties = proposal
        .properties
        .iter()
        .map(|h| EventProperty {
            name: h.name.clone(),
            prop_type: h.type_hint.clone(),
            required: false,
            pii: h.pii_hint,
        })
        .collect();

    CatalogEntry {
        id,
        name: proposal.name.clone(),
        description: String::new(),
        status: EventStatus::Proposed,
        confidence: f64::from(proposal.confidence),
        kind: event_kind_to_catalog(proposal.kind),
        provenance,
        properties,
        providers: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build a fresh [`Catalog`] from a slice of proposals.
///
/// All entries are `Proposed`. Events are sorted by ID for stable diffs.
/// Proposals producing the same (name, source path, kind) are deduplicated.
#[must_use]
pub fn from_proposals(proposals: &[ProposedEvent], project_root: &Path) -> Catalog {
    let mut events: Vec<CatalogEntry> = proposals
        .iter()
        .map(|p| proposal_to_entry(p, project_root))
        .collect();

    events.sort_by(|a, b| a.id.cmp(&b.id));
    events.dedup_by(|a, b| a.id == b.id);

    Catalog {
        schema_version: CATALOG_SCHEMA_VERSION,
        events,
    }
}

/// Merge new proposals into an existing catalog without clobbering manual edits.
///
/// Only proposals whose stable ID is not already present are appended.
/// Existing entries (including `Approved` / `Ignored` status and human edits
/// to name, description, properties, providers) are never modified.
/// The events list is re-sorted by ID after merge.
pub fn merge_proposals(
    catalog: &mut Catalog,
    proposals: &[ProposedEvent],
    project_root: &Path,
) {
    let existing_ids: HashSet<&str> = catalog.events.iter().map(|e| e.id.as_str()).collect();

    let new_entries: Vec<CatalogEntry> = proposals
        .iter()
        .map(|p| proposal_to_entry(p, project_root))
        .filter(|e| !existing_ids.contains(e.id.as_str()))
        .collect();

    catalog.events.extend(new_entries);
    catalog.events.sort_by(|a, b| a.id.cmp(&b.id));
}

/// Load a [`Catalog`] from a YAML file.
///
/// # Errors
/// - [`Error::Io`] if the file cannot be read.
/// - [`Error::CatalogParse`] if the YAML is malformed or does not match the
///   [`Catalog`] schema.
pub fn load_catalog(path: &Path) -> Result<Catalog> {
    let text = std::fs::read_to_string(path).map_err(Error::Io)?;
    serde_yaml::from_str(&text).map_err(|e| Error::CatalogParse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// Serialize a [`Catalog`] to YAML and write it to `path`.
///
/// Events are sorted by ID before writing for stable diffs. Parent directories
/// are created as needed.
///
/// # Errors
/// - [`Error::CatalogParse`] if serialization fails (should not occur for
///   well-formed catalogs).
/// - [`Error::Io`] if the file or parent directories cannot be created/written.
pub fn save_catalog(catalog: &Catalog, path: &Path) -> Result<()> {
    let mut sorted = catalog.clone();
    sorted.events.sort_by(|a, b| a.id.cmp(&b.id));

    let text = serde_yaml::to_string(&sorted).map_err(|e| Error::CatalogParse {
        path: path.to_path_buf(),
        message: format!("serialize: {e}"),
    })?;

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(Error::Io)?;
    }
    std::fs::write(path, text).map_err(Error::Io)?;
    Ok(())
}

/// Merge fresh scan proposals into an existing catalog using three-way merge semantics.
///
/// ## Merge rules (keyed on stable ID)
///
/// | Scenario | Action |
/// |---|---|
/// | ID in proposals only (new detection) | Add as `Proposed` |
/// | ID in both existing and proposed | Keep existing entry verbatim (all human edits preserved) |
/// | ID in existing only, status `Proposed` | Remove — unreviewed event no longer detected |
/// | ID in existing only, status `Approved` or `Ignored` | Keep — human reviewed |
///
/// The returned [`Catalog`] is sorted by ID for stable diffs.
#[must_use]
pub fn rescan_merge(
    existing: &Catalog,
    proposals: &[ProposedEvent],
    project_root: &Path,
) -> Catalog {
    let proposed_catalog = from_proposals(proposals, project_root);

    let existing_ids: HashSet<&str> = existing.events.iter().map(|e| e.id.as_str()).collect();
    let proposed_ids: HashSet<&str> =
        proposed_catalog.events.iter().map(|e| e.id.as_str()).collect();

    let mut merged: Vec<CatalogEntry> = Vec::new();

    // Keep existing entries that match or are reviewed
    for entry in &existing.events {
        if proposed_ids.contains(entry.id.as_str()) {
            // Matched — keep existing verbatim (preserve all human edits)
            merged.push(entry.clone());
        } else {
            // Disappeared from scan
            match entry.status {
                EventStatus::Proposed => {} // drop — unreviewed, no longer detected
                EventStatus::Approved | EventStatus::Ignored => {
                    merged.push(entry.clone()); // keep — human explicitly reviewed
                }
            }
        }
    }

    // Add new proposals (IDs absent from existing)
    for entry in proposed_catalog.events {
        if !existing_ids.contains(entry.id.as_str()) {
            merged.push(entry);
        }
    }

    merged.sort_by(|a, b| a.id.cmp(&b.id));

    Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: merged }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{EventKind, PropertyHint, ProposedEvent};
    use std::path::PathBuf;

    fn make_proposal(name: &str, kind: EventKind, path: &str, confidence: f32) -> ProposedEvent {
        ProposedEvent::new(name, kind, PathBuf::from(path), confidence)
    }

    #[test]
    fn generate_stable_id_is_deterministic() {
        let id1 = generate_stable_id("page_viewed", "src/pages/index.tsx", "pageView");
        let id2 = generate_stable_id("page_viewed", "src/pages/index.tsx", "pageView");
        assert_eq!(id1, id2);
    }

    #[test]
    fn generate_stable_id_has_evt_prefix() {
        let id = generate_stable_id("x", "y", "z");
        assert!(id.starts_with("evt_"));
        assert_eq!(id.len(), 20); // "evt_" (4) + 16 hex digits
    }

    #[test]
    fn generate_stable_id_differs_on_name_change() {
        let id1 = generate_stable_id("page_viewed", "src/index.tsx", "pageView");
        let id2 = generate_stable_id("home_viewed", "src/index.tsx", "pageView");
        assert_ne!(id1, id2);
    }

    #[test]
    fn proposal_to_entry_strips_project_root() {
        let root = PathBuf::from("/project");
        let proposal = make_proposal(
            "page_viewed",
            EventKind::PageView,
            "/project/pages/index.tsx",
            0.9,
        );
        let entry = proposal_to_entry(&proposal, &root);
        assert_eq!(entry.provenance[0].source_path, "pages/index.tsx");
    }

    #[test]
    fn proposal_to_entry_fallback_on_unrooted_path() {
        let root = PathBuf::from("/other");
        let proposal = make_proposal(
            "page_viewed",
            EventKind::PageView,
            "/project/pages/index.tsx",
            0.9,
        );
        let entry = proposal_to_entry(&proposal, &root);
        // strip_prefix fails → full path stored
        assert!(entry.provenance[0].source_path.contains("index.tsx"));
    }

    #[test]
    fn kind_mapping_covers_all_variants() {
        assert_eq!(
            event_kind_to_catalog(EventKind::PageView),
            CatalogEventKind::PageView
        );
        assert_eq!(
            event_kind_to_catalog(EventKind::ApiCall),
            CatalogEventKind::ApiCall
        );
        assert_eq!(
            event_kind_to_catalog(EventKind::AuthEvent),
            CatalogEventKind::AuthEvent
        );
        assert_eq!(
            event_kind_to_catalog(EventKind::FormSubmit),
            CatalogEventKind::FormSubmit
        );
        assert_eq!(
            event_kind_to_catalog(EventKind::Error),
            CatalogEventKind::Error
        );
    }

    #[test]
    fn from_proposals_empty_input() {
        let catalog = from_proposals(&[], Path::new("/root"));
        assert!(catalog.events.is_empty());
        assert_eq!(catalog.schema_version, CATALOG_SCHEMA_VERSION);
    }

    #[test]
    fn from_proposals_status_is_proposed() {
        let p = make_proposal("x", EventKind::PageView, "/root/a.tsx", 0.9);
        let catalog = from_proposals(&[p], Path::new("/root"));
        assert_eq!(catalog.events[0].status, EventStatus::Proposed);
    }

    #[test]
    fn from_proposals_deduplicates_identical() {
        let p1 = make_proposal("x", EventKind::PageView, "/root/a.tsx", 0.9);
        let p2 = make_proposal("x", EventKind::PageView, "/root/a.tsx", 0.9);
        let catalog = from_proposals(&[p1, p2], Path::new("/root"));
        assert_eq!(catalog.events.len(), 1);
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
        assert_eq!(props[0].name, "method");
        assert_eq!(props[0].prop_type, Some("string".into()));
        assert!(!props[0].pii);
        assert_eq!(props[1].name, "email");
        assert!(props[1].prop_type.is_none());
        assert!(props[1].pii);
    }

    #[test]
    fn proposal_to_entry_adapter_propagates() {
        let root = PathBuf::from("/project");
        let mut proposal = make_proposal(
            "user_signed_in",
            EventKind::AuthEvent,
            "/project/src/auth.ts",
            0.85,
        );
        proposal.adapter = "nextjs".to_owned();
        let entry = proposal_to_entry(&proposal, &root);
        assert_eq!(entry.provenance[0].adapter, "nextjs");
    }

    #[test]
    fn proposal_to_entry_empty_adapter_propagates() {
        let root = PathBuf::from("/project");
        let proposal = make_proposal("page_viewed", EventKind::PageView, "/project/pages/index.tsx", 0.9);
        // adapter defaults to ""
        let entry = proposal_to_entry(&proposal, &root);
        assert_eq!(entry.provenance[0].adapter, "");
    }

    #[test]
    fn from_proposals_stable_sort_by_id() {
        // Three proposals — sort is by FNV hash of (name:path:kind)
        let proposals = vec![
            make_proposal("zzz_event", EventKind::PageView, "/root/c.tsx", 0.9),
            make_proposal("aaa_event", EventKind::ApiCall, "/root/a.ts", 0.9),
            make_proposal("mmm_event", EventKind::FormSubmit, "/root/b.tsx", 0.9),
        ];
        let catalog = from_proposals(&proposals, Path::new("/root"));
        let ids: Vec<&str> = catalog.events.iter().map(|e| e.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "events must be sorted by id");
    }

    // ---------------------------------------------------------------------------
    // rescan_merge tests
    // ---------------------------------------------------------------------------

    fn make_entry_with_id(id: &str, name: &str, status: EventStatus) -> CatalogEntry {
        use infergen_types::{CatalogEventKind, EventProvenance};
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
        }
    }

    fn make_catalog_with(entries: Vec<CatalogEntry>) -> Catalog {
        Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: entries }
    }

    #[test]
    fn rescan_merge_empty_both_returns_empty() {
        let existing = make_catalog_with(vec![]);
        let result = rescan_merge(&existing, &[], Path::new("/root"));
        assert!(result.events.is_empty());
    }

    #[test]
    fn rescan_merge_empty_existing_adds_all_as_proposed() {
        let existing = make_catalog_with(vec![]);
        let proposals = vec![
            make_proposal("page_viewed", EventKind::PageView, "/root/index.tsx", 0.9),
        ];
        let result = rescan_merge(&existing, &proposals, Path::new("/root"));
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].status, EventStatus::Proposed);
    }

    #[test]
    fn rescan_merge_matched_id_preserves_name() {
        let proposals = vec![
            make_proposal("page_viewed", EventKind::PageView, "/root/index.tsx", 0.9),
        ];
        // Build catalog to get the stable ID
        let fresh = from_proposals(&proposals, Path::new("/root"));
        let id = fresh.events[0].id.clone();

        // Human renamed it
        let mut renamed = fresh.events[0].clone();
        renamed.name = "custom_page_view".to_string();
        let existing = make_catalog_with(vec![renamed]);

        let result = rescan_merge(&existing, &proposals, Path::new("/root"));
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].name, "custom_page_view");
        assert_eq!(result.events[0].id, id);
    }

    #[test]
    fn rescan_merge_matched_id_preserves_approved_status() {
        let proposals = vec![
            make_proposal("page_viewed", EventKind::PageView, "/root/index.tsx", 0.9),
        ];
        let fresh = from_proposals(&proposals, Path::new("/root"));
        let mut approved = fresh.events[0].clone();
        approved.status = EventStatus::Approved;
        let existing = make_catalog_with(vec![approved]);

        let result = rescan_merge(&existing, &proposals, Path::new("/root"));
        assert_eq!(result.events[0].status, EventStatus::Approved);
    }

    #[test]
    fn rescan_merge_matched_id_preserves_ignored_status() {
        let proposals = vec![
            make_proposal("page_viewed", EventKind::PageView, "/root/index.tsx", 0.9),
        ];
        let fresh = from_proposals(&proposals, Path::new("/root"));
        let mut ignored = fresh.events[0].clone();
        ignored.status = EventStatus::Ignored;
        let existing = make_catalog_with(vec![ignored]);

        let result = rescan_merge(&existing, &proposals, Path::new("/root"));
        assert_eq!(result.events[0].status, EventStatus::Ignored);
    }

    #[test]
    fn rescan_merge_matched_id_preserves_description() {
        let proposals = vec![
            make_proposal("page_viewed", EventKind::PageView, "/root/index.tsx", 0.9),
        ];
        let fresh = from_proposals(&proposals, Path::new("/root"));
        let mut described = fresh.events[0].clone();
        described.description = "Human description.".to_string();
        let existing = make_catalog_with(vec![described]);

        let result = rescan_merge(&existing, &proposals, Path::new("/root"));
        assert_eq!(result.events[0].description, "Human description.");
    }

    #[test]
    fn rescan_merge_matched_id_preserves_properties() {
        use infergen_types::EventProperty;
        let proposals = vec![
            make_proposal("page_viewed", EventKind::PageView, "/root/index.tsx", 0.9),
        ];
        let fresh = from_proposals(&proposals, Path::new("/root"));
        let mut with_prop = fresh.events[0].clone();
        with_prop.properties.push(EventProperty {
            name: "custom_prop".into(),
            prop_type: Some("string".into()),
            required: true,
            pii: false,
        });
        let existing = make_catalog_with(vec![with_prop]);

        let result = rescan_merge(&existing, &proposals, Path::new("/root"));
        assert_eq!(result.events[0].properties.len(), 1);
        assert_eq!(result.events[0].properties[0].name, "custom_prop");
    }

    #[test]
    fn rescan_merge_disappeared_proposed_is_removed() {
        // Existing has a Proposed event that is NOT in new proposals
        let existing = make_catalog_with(vec![
            make_entry_with_id("evt_stale0000000000", "stale_event", EventStatus::Proposed),
        ]);
        // No proposals → stale Proposed should be removed
        let result = rescan_merge(&existing, &[], Path::new("/root"));
        assert!(result.events.is_empty(), "stale Proposed event must be removed");
    }

    #[test]
    fn rescan_merge_disappeared_approved_is_kept() {
        let existing = make_catalog_with(vec![
            make_entry_with_id("evt_approved000000", "important_event", EventStatus::Approved),
        ]);
        let result = rescan_merge(&existing, &[], Path::new("/root"));
        assert_eq!(result.events.len(), 1, "Approved event must survive rescan");
        assert_eq!(result.events[0].name, "important_event");
    }

    #[test]
    fn rescan_merge_disappeared_ignored_is_kept() {
        let existing = make_catalog_with(vec![
            make_entry_with_id("evt_ignored0000000", "noise_event", EventStatus::Ignored),
        ]);
        let result = rescan_merge(&existing, &[], Path::new("/root"));
        assert_eq!(result.events.len(), 1, "Ignored event must survive rescan");
    }

    #[test]
    fn rescan_merge_new_proposal_added() {
        let existing = make_catalog_with(vec![]);
        let proposals = vec![
            make_proposal("new_event", EventKind::ApiCall, "/root/api.ts", 0.8),
        ];
        let result = rescan_merge(&existing, &proposals, Path::new("/root"));
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].name, "new_event");
        assert_eq!(result.events[0].status, EventStatus::Proposed);
    }

    #[test]
    fn rescan_merge_result_sorted_by_id() {
        let existing = make_catalog_with(vec![]);
        let proposals = vec![
            make_proposal("zzz", EventKind::PageView, "/root/c.tsx", 0.9),
            make_proposal("aaa", EventKind::ApiCall, "/root/a.ts", 0.9),
            make_proposal("mmm", EventKind::FormSubmit, "/root/b.tsx", 0.9),
        ];
        let result = rescan_merge(&existing, &proposals, Path::new("/root"));
        let ids: Vec<&str> = result.events.iter().map(|e| e.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "rescan_merge output must be sorted by id");
    }

    #[test]
    fn rescan_merge_combination_all_three_scenarios() {
        let proposals_first = vec![
            make_proposal("matched_event", EventKind::PageView, "/root/a.tsx", 0.9),
            make_proposal("will_disappear_proposed", EventKind::ApiCall, "/root/b.ts", 0.8),
            make_proposal("will_disappear_approved", EventKind::AuthEvent, "/root/c.ts", 0.85),
        ];
        let first_catalog = from_proposals(&proposals_first, Path::new("/root"));

        // Set up existing catalog with various statuses
        let mut existing = first_catalog.clone();
        // will_disappear_approved → Approved
        for e in &mut existing.events {
            if e.name == "will_disappear_approved" {
                e.status = EventStatus::Approved;
            }
        }

        // Second scan: matched_event still present, will_disappear_* gone, new_event added
        let proposals_second = vec![
            make_proposal("matched_event", EventKind::PageView, "/root/a.tsx", 0.9),
            make_proposal("new_event", EventKind::FormSubmit, "/root/d.tsx", 0.7),
        ];

        let result = rescan_merge(&existing, &proposals_second, Path::new("/root"));

        // matched_event: present in result
        assert!(result.events.iter().any(|e| e.name == "matched_event"), "matched_event must be kept");
        // will_disappear_proposed: removed (was Proposed, not in scan)
        assert!(!result.events.iter().any(|e| e.name == "will_disappear_proposed"), "stale Proposed must be removed");
        // will_disappear_approved: kept (was Approved)
        assert!(result.events.iter().any(|e| e.name == "will_disappear_approved"), "Approved must be kept");
        // new_event: added
        assert!(result.events.iter().any(|e| e.name == "new_event"), "new_event must be added");
        assert_eq!(result.events.iter().filter(|e| e.name == "new_event").next().unwrap().status, EventStatus::Proposed);
    }
}
