//! Shared domain types for Infergen.
//!
//! This crate is the leaf of the dependency graph. Every workspace member and
//! language runtime can depend on it freely. The catalog schema types added in
//! E1.1 require `serde` for serialization; `serde_yaml` I/O lives in
//! `infergen-core` to keep this crate thin.

use serde::{Deserialize, Serialize};

/// Version of the on-disk catalog (`catalog.yaml`) schema.
///
/// Bump on any breaking change to the catalog format.
pub const CATALOG_SCHEMA_VERSION: u32 = 1;

/// The version string of this crate, sourced from Cargo at compile time.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ---------------------------------------------------------------------------
// Catalog domain types (E1.1)
// ---------------------------------------------------------------------------

/// Review status of a catalog event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventStatus {
    /// Auto-proposed by the scan engine; awaiting human review.
    Proposed,
    /// Reviewed and accepted by a human.
    Approved,
    /// Reviewed and explicitly excluded (false positive or unwanted).
    Ignored,
}

/// Broad category of a catalog event.
///
/// Mirrors `infergen_core::adapter::EventKind`; defined here so codegen
/// (E2.x) and language runtimes can read event kind without depending on the
/// scan engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CatalogEventKind {
    /// User navigated to a page or screen.
    PageView,
    /// An API endpoint was called.
    ApiCall,
    /// An authentication action (login, logout, signup, session).
    AuthEvent,
    /// A form was submitted.
    FormSubmit,
    /// A button or clickable element was clicked.
    ButtonClick,
    /// A search query was issued (search input / search handler).
    Search,
    /// An unhandled error or error boundary triggered.
    Error,
}

/// Source location that triggered a catalog event proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventProvenance {
    /// Path relative to the project root, e.g. `"src/auth.ts"`.
    pub source_path: String,
    /// Source line number, if the adapter can pinpoint one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Adapter that produced this proposal, e.g. `"nextjs"`. Empty until E1.2.
    #[serde(default)]
    pub adapter: String,
}

/// A single typed property on a catalog event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventProperty {
    /// Property name, e.g. `"method"`.
    pub name: String,
    /// JS/TS type string, e.g. `"string"`, `"number"`, `"boolean"`. `None`
    /// when the adapter could not infer a type.
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prop_type: Option<String>,
    /// `true` if this property is required on every call site.
    pub required: bool,
    /// `true` if this property likely contains personally identifiable
    /// information (email, name, phone, address).
    pub pii: bool,
}

/// A single event entry in the catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    /// Stable opaque ID in the form `evt_{016hex}`. Survives renames (E4.1).
    pub id: String,
    /// Current event name. May be edited by the reviewer.
    pub name: String,
    /// Human-written description. Empty until the reviewer fills it in.
    #[serde(default)]
    pub description: String,
    /// Review status.
    pub status: EventStatus,
    /// Adapter confidence at proposal time, `0.0`–`1.0`.
    pub confidence: f64,
    /// Category of tracking moment.
    pub kind: CatalogEventKind,
    /// Source locations that triggered this proposal.
    pub provenance: Vec<EventProvenance>,
    /// Typed event properties.
    #[serde(default)]
    pub properties: Vec<EventProperty>,
    /// Configured destination provider IDs, e.g. `["posthog"]`. Populated
    /// post-E3.1 when the user adds providers.
    #[serde(default)]
    pub providers: Vec<String>,
    /// Package namespace in a monorepo, e.g. `"frontend"`.
    ///
    /// `None` in single-package projects. Set by
    /// `infergen_core::monorepo::namespace_catalog` when merging per-package
    /// catalogs. Omitted from serialized YAML when `None` so existing
    /// single-package catalog files are unaffected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

/// Top-level catalog document — the contents of `.infergen/catalog.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    /// Schema version (always [`CATALOG_SCHEMA_VERSION`]). Readers can reject
    /// catalogs with an unsupported version.
    pub schema_version: u32,
    /// All event entries, sorted by `id` for stable diffs.
    pub events: Vec<CatalogEntry>,
}

impl Default for Catalog {
    fn default() -> Self {
        Catalog {
            schema_version: CATALOG_SCHEMA_VERSION,
            events: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_is_one() {
        assert_eq!(CATALOG_SCHEMA_VERSION, 1);
    }

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn catalog_default_has_schema_version_one() {
        let c = Catalog::default();
        assert_eq!(c.schema_version, CATALOG_SCHEMA_VERSION);
        assert!(c.events.is_empty());
    }

    #[test]
    fn event_property_defaults() {
        let p = EventProperty {
            name: "email".into(),
            prop_type: None,
            required: false,
            pii: true,
        };
        assert!(p.prop_type.is_none());
        assert!(p.pii);
    }

    #[test]
    fn event_provenance_line_is_optional() {
        let prov = EventProvenance {
            source_path: "src/auth.ts".into(),
            line: None,
            adapter: String::new(),
        };
        assert!(prov.line.is_none());
    }

    #[test]
    fn catalog_entry_description_defaults_empty() {
        let entry = CatalogEntry {
            id: "evt_abc".into(),
            name: "page_viewed".into(),
            description: String::new(),
            status: EventStatus::Proposed,
            confidence: 0.9,
            kind: CatalogEventKind::PageView,
            provenance: Vec::new(),
            properties: Vec::new(),
            providers: Vec::new(),
            package: None,
        };
        assert!(entry.description.is_empty());
    }

    fn make_entry(name: &str) -> CatalogEntry {
        CatalogEntry {
            id: format!("evt_{name}"),
            name: name.into(),
            description: String::new(),
            status: EventStatus::Proposed,
            confidence: 0.9,
            kind: CatalogEventKind::PageView,
            provenance: Vec::new(),
            properties: Vec::new(),
            providers: Vec::new(),
            package: None,
        }
    }

    #[test]
    fn catalog_entry_package_defaults_none() {
        let entry = make_entry("page_viewed");
        assert!(entry.package.is_none());
    }

    #[test]
    fn catalog_entry_package_some_roundtrips_yaml() {
        let mut entry = make_entry("user_signed_in");
        entry.package = Some("frontend".into());
        let catalog = Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: vec![entry] };
        let yaml = serde_yaml::to_string(&catalog).unwrap();
        let back: Catalog = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.events[0].package, Some("frontend".into()));
    }

    #[test]
    fn catalog_entry_without_package_yaml_omits_field() {
        let entry = make_entry("api_called");
        let catalog = Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: vec![entry] };
        let yaml = serde_yaml::to_string(&catalog).unwrap();
        assert!(!yaml.contains("package:"), "package field should not appear when None");
    }

    #[test]
    fn catalog_entry_existing_yaml_deserializes_without_package() {
        let yaml = r#"schemaVersion: 1
events:
  - id: evt_abc
    name: page_viewed
    description: ""
    status: proposed
    confidence: 0.9
    kind: pageView
    provenance: []
"#;
        let catalog: Catalog = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(catalog.events[0].package, None);
    }

    #[test]
    fn event_status_variants_are_distinct() {
        assert_ne!(EventStatus::Proposed, EventStatus::Approved);
        assert_ne!(EventStatus::Approved, EventStatus::Ignored);
    }

    #[test]
    fn catalog_event_kind_variants_are_distinct() {
        assert_ne!(CatalogEventKind::PageView, CatalogEventKind::ApiCall);
        assert_ne!(CatalogEventKind::AuthEvent, CatalogEventKind::FormSubmit);
    }
}
