//! Data-collection manifest export (E7.4).
//!
//! Converts a [`Catalog`] into a [`Manifest`] — a privacy/compliance view of
//! what events are collected, what PII properties they carry, and which
//! destinations receive them. Supports JSON, YAML (via serde), and Markdown
//! (via [`render_markdown`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use infergen_types::{
    Catalog, CatalogEventKind, EventStatus, FlowKind,
};

/// Version of the manifest schema.
pub const MANIFEST_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Manifest types
// ---------------------------------------------------------------------------

/// Top-level manifest document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Schema version — always [`MANIFEST_VERSION`].
    pub manifest_version: u32,
    /// ISO 8601 / unix timestamp injected by the CLI layer. `None` in tests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    /// Aggregate counts for the manifest header.
    pub summary: ManifestSummary,
    /// All event entries, sorted by name for stable output.
    pub events: Vec<ManifestEvent>,
    /// PII properties aggregated across all events, sorted desc by event count.
    pub pii_inventory: Vec<PiiEntry>,
    /// Destinations referenced by events, sorted desc by event count.
    pub providers: Vec<ManifestProvider>,
    /// Detected funnels. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flows: Vec<ManifestFlow>,
}

/// Aggregate counts for the manifest header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestSummary {
    /// Total number of events across all statuses.
    pub total_events: usize,
    /// Events with [`EventStatus::Approved`].
    pub approved_events: usize,
    /// Events with [`EventStatus::Proposed`] (awaiting review).
    pub proposed_events: usize,
    /// Events with [`EventStatus::Ignored`].
    pub ignored_events: usize,
    /// Number of events that have at least one PII property.
    pub events_with_pii: usize,
    /// Total count of PII property instances across all events.
    pub total_pii_properties: usize,
    /// Distinct monorepo package names, sorted; empty in single-package projects.
    pub packages: Vec<String>,
    /// Distinct destination/provider IDs referenced by any event, sorted.
    pub destinations: Vec<String>,
}

/// One event entry in the manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEvent {
    /// Stable opaque ID, e.g. `"evt_0123456789abcdef"`.
    pub id: String,
    /// Current event name.
    pub name: String,
    /// camelCase kind string, e.g. `"authEvent"`.
    pub kind: String,
    /// `"approved"` | `"proposed"` | `"ignored"`.
    pub status: String,
    /// Human-written description. Omitted when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Configured destination provider IDs.
    #[serde(default)]
    pub providers: Vec<String>,
    /// Typed event properties.
    pub properties: Vec<ManifestProperty>,
    /// Source locations in `"path:line"` or `"path"` format.
    pub locations: Vec<String>,
    /// Monorepo package namespace. `None` in single-package projects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// IDs of flows this event participates in. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flow_ids: Vec<String>,
}

/// One typed property on a manifest event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestProperty {
    /// Property name, e.g. `"email"`.
    pub name: String,
    /// JS/TS type string, e.g. `"string"`, `"number"`. `None` when unresolved.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub prop_type: Option<String>,
    /// `true` if this property is required on every call site.
    pub required: bool,
    /// `true` if this property likely contains personally identifiable information.
    pub pii: bool,
}

/// A PII property aggregated across events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiiEntry {
    /// The PII property name, e.g. `"email"`.
    pub property_name: String,
    /// Number of distinct events that carry this property with `pii: true`.
    pub event_count: usize,
    /// Names of those events, sorted lexicographically.
    pub events: Vec<String>,
}

/// A destination/provider aggregated across events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestProvider {
    /// Provider ID, e.g. `"posthog"`.
    pub name: String,
    /// Number of events that reference this provider.
    pub event_count: usize,
}

/// A detected flow summarised for the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFlow {
    /// Stable flow ID, e.g. `"flow_0123456789abcdef"`.
    pub id: String,
    /// Human-readable funnel name, e.g. `"checkout"`.
    pub name: String,
    /// camelCase kind string, e.g. `"checkout"`.
    pub kind: String,
    /// Number of steps in this flow.
    pub step_count: usize,
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// Build a [`Manifest`] from a [`Catalog`].
///
/// `generated_at` is an opaque timestamp string injected by the caller (CLI)
/// so the core function stays deterministic in tests.
pub fn generate_manifest(catalog: &Catalog, generated_at: Option<String>) -> Manifest {
    let total_events = catalog.events.len();
    let approved_events = catalog.events.iter().filter(|e| e.status == EventStatus::Approved).count();
    let proposed_events = catalog.events.iter().filter(|e| e.status == EventStatus::Proposed).count();
    let ignored_events  = catalog.events.iter().filter(|e| e.status == EventStatus::Ignored).count();

    // pii_map: property_name → set of event names (deduplicates same-named property on same event)
    let mut pii_map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut events_with_pii = 0usize;
    let mut total_pii_properties = 0usize;

    let mut packages: BTreeSet<String> = BTreeSet::new();
    let mut provider_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut all_destinations: BTreeSet<String> = BTreeSet::new();

    let mut manifest_events: Vec<ManifestEvent> = Vec::new();

    for entry in &catalog.events {
        if let Some(pkg) = &entry.package {
            packages.insert(pkg.clone());
        }
        for p in &entry.providers {
            *provider_counts.entry(p.clone()).or_insert(0) += 1;
            all_destinations.insert(p.clone());
        }

        let mut has_pii = false;
        for prop in &entry.properties {
            if prop.pii {
                total_pii_properties += 1;
                has_pii = true;
                pii_map
                    .entry(prop.name.clone())
                    .or_default()
                    .insert(entry.name.clone());
            }
        }
        if has_pii {
            events_with_pii += 1;
        }

        let locations: Vec<String> = entry
            .provenance
            .iter()
            .map(|prov| match prov.line {
                Some(line) => format!("{}:{}", prov.source_path, line),
                None => prov.source_path.clone(),
            })
            .collect();

        manifest_events.push(ManifestEvent {
            id: entry.id.clone(),
            name: entry.name.clone(),
            kind: event_kind_str(entry.kind),
            status: event_status_str(entry.status),
            description: entry.description.clone(),
            providers: entry.providers.clone(),
            properties: entry
                .properties
                .iter()
                .map(|p| ManifestProperty {
                    name: p.name.clone(),
                    prop_type: p.prop_type.clone(),
                    required: p.required,
                    pii: p.pii,
                })
                .collect(),
            locations,
            package: entry.package.clone(),
            flow_ids: entry.flow_ids.clone(),
        });
    }

    // Stable output: sort events alphabetically by name.
    manifest_events.sort_by(|a, b| a.name.cmp(&b.name));

    // PII inventory: desc by event_count, then asc by property_name.
    let mut pii_inventory: Vec<PiiEntry> = pii_map
        .into_iter()
        .map(|(name, event_set)| {
            let mut events: Vec<String> = event_set.into_iter().collect();
            events.sort();
            let count = events.len();
            PiiEntry { property_name: name, event_count: count, events }
        })
        .collect();
    pii_inventory.sort_by(|a, b| {
        b.event_count
            .cmp(&a.event_count)
            .then(a.property_name.cmp(&b.property_name))
    });

    // Providers: desc by event_count, then asc by name.
    let mut providers: Vec<ManifestProvider> = provider_counts
        .into_iter()
        .map(|(name, count)| ManifestProvider { name, event_count: count })
        .collect();
    providers.sort_by(|a, b| {
        b.event_count.cmp(&a.event_count).then(a.name.cmp(&b.name))
    });

    let flows: Vec<ManifestFlow> = catalog
        .flows
        .iter()
        .map(|f| ManifestFlow {
            id: f.id.clone(),
            name: f.name.clone(),
            kind: flow_kind_str(&f.kind),
            step_count: f.steps.len(),
        })
        .collect();

    Manifest {
        manifest_version: MANIFEST_VERSION,
        generated_at,
        summary: ManifestSummary {
            total_events,
            approved_events,
            proposed_events,
            ignored_events,
            events_with_pii,
            total_pii_properties,
            packages: packages.into_iter().collect(),
            destinations: all_destinations.into_iter().collect(),
        },
        events: manifest_events,
        pii_inventory,
        providers,
        flows,
    }
}

// ---------------------------------------------------------------------------
// Markdown rendering
// ---------------------------------------------------------------------------

/// Render a [`Manifest`] as a Markdown compliance report.
///
/// Output is deterministic given the same manifest. Sections with no data
/// are omitted entirely.
pub fn render_markdown(manifest: &Manifest) -> String {
    let mut out = String::new();

    out.push_str("# Data-Collection Manifest\n\n");

    // Summary
    out.push_str("## Summary\n\n");
    out.push_str(&format!("- **Total events:** {}\n", manifest.summary.total_events));
    out.push_str(&format!(
        "- **Approved:** {} · **Proposed:** {} · **Ignored:** {}\n",
        manifest.summary.approved_events,
        manifest.summary.proposed_events,
        manifest.summary.ignored_events,
    ));
    out.push_str(&format!(
        "- **Events with PII:** {} ({} PII properties total)\n",
        manifest.summary.events_with_pii,
        manifest.summary.total_pii_properties,
    ));
    if !manifest.summary.destinations.is_empty() {
        out.push_str(&format!(
            "- **Destinations:** {}\n",
            manifest.summary.destinations.join(", ")
        ));
    }
    if !manifest.summary.packages.is_empty() {
        out.push_str(&format!(
            "- **Packages:** {}\n",
            manifest.summary.packages.join(", ")
        ));
    }
    out.push('\n');

    // Events
    if !manifest.events.is_empty() {
        out.push_str("## Events\n\n");
        for event in &manifest.events {
            out.push_str(&format!("### {} ({})\n\n", event.name, event.kind));
            let mut status_line = format!("**Status:** {}", event.status);
            if !event.providers.is_empty() {
                status_line.push_str(&format!(" | **Providers:** {}", event.providers.join(", ")));
            }
            out.push_str(&status_line);
            out.push_str("\n\n");
            if !event.description.is_empty() {
                out.push_str(&format!("{}\n\n", event.description));
            }
            if !event.properties.is_empty() {
                out.push_str("**Properties:**\n\n");
                for prop in &event.properties {
                    let type_str = prop.prop_type.as_deref().unwrap_or("unknown");
                    let req_tag = if prop.required { " _(required)_" } else { "" };
                    let pii_tag = if prop.pii { " — **PII**" } else { "" };
                    out.push_str(&format!("- `{}` ({}){}{}\n", prop.name, type_str, req_tag, pii_tag));
                }
                out.push('\n');
            }
            if !event.locations.is_empty() {
                out.push_str(&format!("_Source: {}_\n\n", event.locations.join(", ")));
            }
        }
    }

    // PII inventory
    if !manifest.pii_inventory.is_empty() {
        out.push_str("## PII Inventory\n\n");
        out.push_str("| Property | Events | Count |\n");
        out.push_str("|----------|--------|-------|\n");
        for entry in &manifest.pii_inventory {
            out.push_str(&format!(
                "| `{}` | {} | {} |\n",
                entry.property_name,
                entry.events.join(", "),
                entry.event_count,
            ));
        }
        out.push('\n');
    }

    // Destinations
    if !manifest.providers.is_empty() {
        out.push_str("## Destinations\n\n");
        out.push_str("| Provider | Events |\n");
        out.push_str("|----------|--------|\n");
        for provider in &manifest.providers {
            out.push_str(&format!("| `{}` | {} |\n", provider.name, provider.event_count));
        }
        out.push('\n');
    }

    // Flows
    if !manifest.flows.is_empty() {
        out.push_str("## Flows\n\n");
        for flow in &manifest.flows {
            out.push_str(&format!("### {} ({})\n\n", flow.name, flow.kind));
            out.push_str(&format!("Steps: {}\n\n", flow.step_count));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn event_kind_str(kind: CatalogEventKind) -> String {
    match kind {
        CatalogEventKind::PageView    => "pageView",
        CatalogEventKind::ApiCall     => "apiCall",
        CatalogEventKind::AuthEvent   => "authEvent",
        CatalogEventKind::FormSubmit  => "formSubmit",
        CatalogEventKind::ButtonClick => "buttonClick",
        CatalogEventKind::Search      => "search",
        CatalogEventKind::Error       => "error",
    }
    .to_string()
}

fn event_status_str(status: EventStatus) -> String {
    match status {
        EventStatus::Proposed => "proposed",
        EventStatus::Approved => "approved",
        EventStatus::Ignored  => "ignored",
    }
    .to_string()
}

fn flow_kind_str(kind: &FlowKind) -> String {
    match kind {
        FlowKind::Checkout   => "checkout",
        FlowKind::Onboarding => "onboarding",
        FlowKind::Auth       => "auth",
        FlowKind::Payment    => "payment",
        FlowKind::Search     => "search",
        FlowKind::Custom     => "custom",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use infergen_types::{
        Catalog, CatalogEntry, CatalogEventKind, EventProperty, EventProvenance, EventStatus,
        CATALOG_SCHEMA_VERSION,
    };

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

    #[test]
    fn manifest_version_is_one() {
        assert_eq!(MANIFEST_VERSION, 1);
    }

    #[test]
    fn empty_catalog_all_counts_zero() {
        let m = generate_manifest(&make_catalog(vec![]), None);
        assert_eq!(m.manifest_version, 1);
        assert_eq!(m.summary.total_events, 0);
        assert_eq!(m.summary.approved_events, 0);
        assert_eq!(m.summary.events_with_pii, 0);
        assert_eq!(m.summary.total_pii_properties, 0);
        assert!(m.events.is_empty());
        assert!(m.pii_inventory.is_empty());
        assert!(m.providers.is_empty());
        assert!(m.flows.is_empty());
    }

    #[test]
    fn single_approved_event() {
        let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved, CatalogEventKind::PageView)]);
        let m = generate_manifest(&cat, None);
        assert_eq!(m.summary.total_events, 1);
        assert_eq!(m.summary.approved_events, 1);
        assert_eq!(m.summary.proposed_events, 0);
        assert_eq!(m.events.len(), 1);
        assert_eq!(m.events[0].status, "approved");
        assert_eq!(m.events[0].kind, "pageView");
    }

    #[test]
    fn proposed_and_ignored_status_strings() {
        let cat = make_catalog(vec![
            make_entry("a", EventStatus::Proposed, CatalogEventKind::ApiCall),
            make_entry("b", EventStatus::Ignored, CatalogEventKind::Error),
        ]);
        let m = generate_manifest(&cat, None);
        let statuses: Vec<&str> = m.events.iter().map(|e| e.status.as_str()).collect();
        assert!(statuses.contains(&"proposed"));
        assert!(statuses.contains(&"ignored"));
        assert_eq!(m.summary.proposed_events, 1);
        assert_eq!(m.summary.ignored_events, 1);
    }

    #[test]
    fn pii_property_in_inventory() {
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
    fn pii_aggregates_across_events() {
        let mut e1 = make_entry("user_signed_in", EventStatus::Approved, CatalogEventKind::AuthEvent);
        e1.properties.push(EventProperty { name: "email".into(), prop_type: None, required: false, pii: true });
        let mut e2 = make_entry("user_updated", EventStatus::Approved, CatalogEventKind::FormSubmit);
        e2.properties.push(EventProperty { name: "email".into(), prop_type: None, required: false, pii: true });
        let m = generate_manifest(&make_catalog(vec![e1, e2]), None);
        assert_eq!(m.pii_inventory.len(), 1);
        assert_eq!(m.pii_inventory[0].event_count, 2);
        assert_eq!(m.summary.total_pii_properties, 2);
        assert_eq!(m.summary.events_with_pii, 2);
    }

    #[test]
    fn provider_counts_aggregated() {
        let mut e1 = make_entry("a", EventStatus::Approved, CatalogEventKind::PageView);
        e1.providers = vec!["posthog".into()];
        let mut e2 = make_entry("b", EventStatus::Approved, CatalogEventKind::PageView);
        e2.providers = vec!["posthog".into()];
        let mut e3 = make_entry("c", EventStatus::Approved, CatalogEventKind::PageView);
        e3.providers = vec!["posthog".into(), "segment".into()];
        let m = generate_manifest(&make_catalog(vec![e1, e2, e3]), None);
        assert_eq!(m.providers[0].name, "posthog");
        assert_eq!(m.providers[0].event_count, 3);
        assert_eq!(m.providers[1].name, "segment");
        assert_eq!(m.providers[1].event_count, 1);
    }

    #[test]
    fn location_with_line() {
        let mut entry = make_entry("auth_checked", EventStatus::Approved, CatalogEventKind::AuthEvent);
        entry.provenance.push(EventProvenance {
            source_path: "src/auth.ts".into(),
            line: Some(45),
            adapter: "nextjs".into(),
        });
        let m = generate_manifest(&make_catalog(vec![entry]), None);
        assert_eq!(m.events[0].locations, vec!["src/auth.ts:45"]);
    }

    #[test]
    fn location_without_line() {
        let mut entry = make_entry("auth_checked", EventStatus::Approved, CatalogEventKind::AuthEvent);
        entry.provenance.push(EventProvenance {
            source_path: "src/auth.ts".into(),
            line: None,
            adapter: "nextjs".into(),
        });
        let m = generate_manifest(&make_catalog(vec![entry]), None);
        assert_eq!(m.events[0].locations, vec!["src/auth.ts"]);
    }

    #[test]
    fn events_sorted_by_name() {
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

    #[test]
    fn packages_collected_and_sorted() {
        let mut e1 = make_entry("e1", EventStatus::Approved, CatalogEventKind::PageView);
        e1.package = Some("frontend".into());
        let mut e2 = make_entry("e2", EventStatus::Approved, CatalogEventKind::ApiCall);
        e2.package = Some("backend".into());
        let mut e3 = make_entry("e3", EventStatus::Approved, CatalogEventKind::ApiCall);
        e3.package = Some("frontend".into());
        let m = generate_manifest(&make_catalog(vec![e1, e2, e3]), None);
        assert_eq!(m.summary.packages, vec!["backend", "frontend"]);
    }

    #[test]
    fn destinations_in_summary() {
        let mut e1 = make_entry("a", EventStatus::Approved, CatalogEventKind::PageView);
        e1.providers = vec!["posthog".into()];
        let mut e2 = make_entry("b", EventStatus::Approved, CatalogEventKind::PageView);
        e2.providers = vec!["segment".into()];
        let m = generate_manifest(&make_catalog(vec![e1, e2]), None);
        assert!(m.summary.destinations.contains(&"posthog".to_string()));
        assert!(m.summary.destinations.contains(&"segment".to_string()));
    }

    #[test]
    fn generated_at_none_absent() {
        let m = generate_manifest(&make_catalog(vec![]), None);
        assert!(m.generated_at.is_none());
        let json = serde_json::to_string(&m).unwrap();
        assert!(!json.contains("generatedAt"));
    }

    #[test]
    fn generated_at_some_present() {
        let m = generate_manifest(&make_catalog(vec![]), Some("unix:1700000000".into()));
        assert_eq!(m.generated_at.as_deref(), Some("unix:1700000000"));
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("generatedAt"));
        assert!(json.contains("1700000000"));
    }

    #[test]
    fn render_markdown_contains_event_name() {
        let entry = make_entry("user_signed_in", EventStatus::Approved, CatalogEventKind::AuthEvent);
        let m = generate_manifest(&make_catalog(vec![entry]), None);
        let md = render_markdown(&m);
        assert!(md.contains("user_signed_in"));
    }

    #[test]
    fn render_markdown_marks_pii() {
        let mut entry = make_entry("user_signed_in", EventStatus::Approved, CatalogEventKind::AuthEvent);
        entry.properties.push(EventProperty {
            name: "email".into(),
            prop_type: Some("string".into()),
            required: true,
            pii: true,
        });
        let m = generate_manifest(&make_catalog(vec![entry]), None);
        let md = render_markdown(&m);
        assert!(md.contains("**PII**"), "markdown must highlight PII");
        assert!(md.contains("PII Inventory"));
    }

    #[test]
    fn render_markdown_empty_catalog_no_events_section() {
        let m = generate_manifest(&make_catalog(vec![]), None);
        let md = render_markdown(&m);
        assert!(md.contains("## Summary"));
        assert!(!md.contains("## Events"));
        assert!(!md.contains("## PII Inventory"));
        assert!(!md.contains("## Destinations"));
    }

    #[test]
    fn json_roundtrip() {
        let mut entry = make_entry("page_viewed", EventStatus::Approved, CatalogEventKind::PageView);
        entry.properties.push(EventProperty { name: "path".into(), prop_type: Some("string".into()), required: true, pii: false });
        let m = generate_manifest(&make_catalog(vec![entry]), None);
        let json = serde_json::to_string(&m).expect("serialize");
        let back: Manifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
    }
}
