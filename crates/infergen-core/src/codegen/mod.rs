//! TypeScript code generation from an approved event catalog (E2.1 + E2.2 + E3.3).
//!
//! `generate_typescript` produces a deterministic, idempotent TypeScript source
//! file: one typed interface + one named function per approved event, a `track`
//! namespace object for autocomplete-friendly use, and `EventName` union type.
//! JSDoc comments carry event descriptions and `@pii` tags on sensitive props.
//! E3.3 adds `DeliveryEngine` + `withDelivery` to every generated file.

mod consent;
mod delivery;
pub mod go_gen;
mod python;
use consent::write_consent_module;
use delivery::write_delivery_engine;
pub use go_gen::{GoCodegenConfig, generate_go};
pub use python::generate_python;

use infergen_types::{CatalogEntry, EventProperty, EventStatus};

use crate::{Catalog, CATALOG_SCHEMA_VERSION};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Configuration for TypeScript code generation.
#[derive(Debug, Clone, Default)]
pub struct CodegenConfig {
    /// Also generate code for `Proposed` events in addition to `Approved`.
    ///
    /// Default: `false` — only approved events are emitted.
    pub include_proposed: bool,
}


/// Generate a TypeScript SDK source string from an approved catalog.
///
/// Only `Approved` entries are included by default. `Ignored` entries are
/// always excluded. Set `config.include_proposed = true` to also include
/// `Proposed` entries (useful during active development).
///
/// Output is deterministic: events sorted alphabetically, no timestamps.
///
/// # Examples
/// ```rust
/// use infergen_core::codegen::{CodegenConfig, generate_typescript};
/// use infergen_core::Catalog;
/// let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
/// assert!(ts.contains("EventName"));
/// ```
#[must_use]
pub fn generate_typescript(catalog: &Catalog, config: &CodegenConfig) -> String {
    let mut events: Vec<&CatalogEntry> = catalog
        .events
        .iter()
        .filter(|e| {
            e.status == EventStatus::Approved
                || (config.include_proposed && e.status == EventStatus::Proposed)
        })
        .collect();
    events.sort_by(|a, b| a.name.cmp(&b.name));

    let mut out = String::new();
    write_header(&mut out);
    write_runtime_preamble(&mut out);
    write_delivery_engine(&mut out);
    write_consent_module(&mut out);
    out.push_str(&build_event_name_type(&events));

    for entry in &events {
        write_event_section(&mut out, entry);
    }

    write_track_object(&mut out, &events);
    out
}

// ---------------------------------------------------------------------------
// Private helpers — name conversion
// ---------------------------------------------------------------------------

/// Convert any identifier to PascalCase.
///
/// `"user_signed_in"` → `"UserSignedIn"`, `"pageViewed"` → `"Pageviewed"` etc.
fn to_pascal_case(name: &str) -> String {
    crate::namer::split_identifier(name)
        .into_iter()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}

/// Convert any identifier to camelCase.
///
/// `"user_signed_in"` → `"userSignedIn"`.
fn to_camel_case(name: &str) -> String {
    let parts = crate::namer::split_identifier(name);
    parts
        .iter()
        .enumerate()
        .map(|(i, w)| {
            if i == 0 {
                w.clone()
            } else {
                let mut chars = w.chars();
                match chars.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().to_string() + chars.as_str(),
                }
            }
        })
        .collect()
}

/// Map an `EventProperty.prop_type` value to a TypeScript type string.
///
/// Recognises `"string"`, `"number"`, `"boolean"`. Everything else — including
/// `None` — maps to `"unknown"`.
fn ts_type(prop_type: Option<&str>) -> &'static str {
    match prop_type {
        Some("string") => "string",
        Some("number") => "number",
        Some("boolean") => "boolean",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Private helpers — output builders
// ---------------------------------------------------------------------------

fn write_runtime_preamble(out: &mut String) {
    out.push('\n');
    out.push_str("/** A provider that receives analytics events. */\n");
    out.push_str("export interface Provider {\n");
    out.push_str("  /** Unique provider identifier, e.g. \"posthog\". */\n");
    out.push_str("  id: string;\n");
    out.push_str("  /** Send a single analytics event. */\n");
    out.push_str("  track(eventName: string, properties: Record<string, unknown>): void;\n");
    out.push_str("  /** Flush buffered events (optional). */\n");
    out.push_str("  flush?(): void;\n");
    out.push_str("  /** Shut down the provider (optional). */\n");
    out.push_str("  shutdown?(): void;\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("/** Runtime configuration. Pass to `configureInfergen` at app startup. */\n");
    out.push_str("export interface InfergenConfig {\n");
    out.push_str("  /** Analytics providers that receive every `track` call. */\n");
    out.push_str("  providers: Provider[];\n");
    out.push_str("  /** Initial consent state. Default: \"unknown\" (permissive until explicitly set). */\n");
    out.push_str("  consent?: ConsentState;\n");
    out.push_str("  /** Initial region tag for provider routing (e.g. \"eu\", \"us\"). */\n");
    out.push_str("  region?: string;\n");
    out.push_str("  /** Initial per-property redaction hook. */\n");
    out.push_str("  redactFn?: RedactFn;\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("/** @internal Registered providers — populated by `configureInfergen`. */\n");
    out.push_str("let _providers: Provider[] = [];\n");
    out.push('\n');
    out.push_str("/** Configure the Infergen runtime. Call once before any track calls. */\n");
    out.push_str("export function configureInfergen(config: InfergenConfig): void {\n");
    out.push_str("  _providers = config.providers;\n");
    out.push_str("  if (config.consent !== undefined) setConsent(config.consent);\n");
    out.push_str("  if (config.region !== undefined) setRegion(config.region);\n");
    out.push_str("  if (config.redactFn !== undefined) setRedactFn(config.redactFn);\n");
    out.push_str("}\n");
}

fn write_header(out: &mut String) {
    out.push_str("// Auto-generated by Infergen. Do not edit — run `infergen generate` to regenerate.\n");
    out.push_str(&format!(
        "// infergen-core v{} · catalog schema v{}\n",
        env!("CARGO_PKG_VERSION"),
        CATALOG_SCHEMA_VERSION,
    ));
}

fn build_event_name_type(events: &[&CatalogEntry]) -> String {
    let mut s = String::new();
    s.push('\n');
    s.push_str("/** All tracked event names. */\n");
    if events.is_empty() {
        s.push_str("export type EventName = never;\n");
    } else {
        s.push_str("export type EventName =\n");
        for (i, entry) in events.iter().enumerate() {
            if i == events.len() - 1 {
                s.push_str(&format!("  | \"{}\";\n", entry.name));
            } else {
                s.push_str(&format!("  | \"{}\"\n", entry.name));
            }
        }
    }
    s
}

fn write_section_divider(out: &mut String, name: &str) {
    let prefix = format!("// ─── {} ", name);
    let bar_len = 80usize.saturating_sub(prefix.len());
    let bar = "─".repeat(bar_len.max(2));
    out.push('\n');
    out.push_str(&format!("{}{}\n", prefix, bar));
    out.push('\n');
}

fn write_property_jsdoc(out: &mut String, prop: &EventProperty) {
    let t = ts_type(prop.prop_type.as_deref());
    if prop.pii {
        out.push_str("  /**\n");
        out.push_str(&format!("   * @type {{{t}}}\n"));
        out.push_str("   * @pii Contains personally identifiable information — handle with care.\n");
        out.push_str("   */\n");
    } else {
        out.push_str(&format!("  /** @type {{{t}}} */\n"));
    }
}

fn write_interface(out: &mut String, entry: &CatalogEntry) {
    let pascal = to_pascal_case(&entry.name);
    out.push_str(&format!("/** Properties for the {} event. */\n", entry.name));
    let mut sorted_props: Vec<&EventProperty> = entry.properties.iter().collect();
    sorted_props.sort_by(|a, b| a.name.cmp(&b.name));
    if sorted_props.is_empty() {
        // `Record<string, never>` is the lint-clean empty-object type; an empty
        // `interface {}` trips @typescript-eslint/no-empty-object-type.
        out.push_str(&format!(
            "export type {pascal}Properties = Record<string, never>;\n"
        ));
    } else {
        out.push_str(&format!("export interface {}Properties {{\n", pascal));
        for prop in &sorted_props {
            write_property_jsdoc(out, prop);
            let t = ts_type(prop.prop_type.as_deref());
            out.push_str(&format!("  {}: {};\n", prop.name, t));
        }
        out.push_str("}\n");
    }
}

fn write_function(out: &mut String, entry: &CatalogEntry) {
    let pascal = to_pascal_case(&entry.name);
    out.push('\n');
    if entry.description.is_empty() {
        out.push_str(&format!("/** Track a {} event. */\n", entry.name));
    } else {
        out.push_str("/**\n");
        out.push_str(&format!(" * Track a {} event.\n", entry.name));
        out.push_str(" *\n");
        out.push_str(&format!(" * {}\n", entry.description));
        out.push_str(" */\n");
    }
    out.push_str(&format!(
        "export function track{pascal}(properties: {pascal}Properties): void {{\n"
    ));
    out.push_str(&format!(
        "  _dispatch(\"{}\", properties);\n",
        entry.name
    ));
    out.push_str("}\n");
}

fn write_event_section(out: &mut String, entry: &CatalogEntry) {
    write_section_divider(out, &entry.name);
    write_interface(out, entry);
    write_function(out, entry);
}

fn write_track_object(out: &mut String, events: &[&CatalogEntry]) {
    out.push('\n');
    if events.is_empty() {
        out.push_str("/**\n");
        out.push_str(" * Track any event by name with fully typed properties.\n");
        out.push_str(" * No events are currently approved in the catalog.\n");
        out.push_str(" */\n");
        out.push_str("export const track = {} as const;\n");
    } else {
        out.push_str("/**\n");
        out.push_str(" * Track any event by name with fully typed properties.\n");
        out.push_str(" *\n");
        out.push_str(" * @example\n");
        // Use first event's camelCase for the example
        let first_camel = to_camel_case(&events[0].name);
        out.push_str(&format!(" * track.{}({{ ... }});\n", first_camel));
        out.push_str(" */\n");
        out.push_str("export const track = {\n");
        for entry in events {
            let camel = to_camel_case(&entry.name);
            let pascal = to_pascal_case(&entry.name);
            out.push_str(&format!("  {camel}: track{pascal},\n"));
        }
        out.push_str("} as const;\n");
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use infergen_types::{
        CatalogEntry, CatalogEventKind, EventProperty, EventProvenance, EventStatus,
        CATALOG_SCHEMA_VERSION,
    };

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
                adapter: String::new(),
            }],
            properties: Vec::new(),
            providers: Vec::new(),
        }
    }

    fn make_prop(name: &str, t: Option<&str>, pii: bool) -> EventProperty {
        EventProperty { name: name.into(), prop_type: t.map(Into::into), required: false, pii }
    }

    // --- name helpers ---

    #[test]
    fn to_pascal_case_single_word() {
        assert_eq!(to_pascal_case("submit"), "Submit");
    }

    #[test]
    fn to_pascal_case_snake_case() {
        assert_eq!(to_pascal_case("user_signed_in"), "UserSignedIn");
    }

    #[test]
    fn to_pascal_case_single_char() {
        assert_eq!(to_pascal_case("a"), "A");
    }

    #[test]
    fn to_camel_case_single_word() {
        assert_eq!(to_camel_case("submit"), "submit");
    }

    #[test]
    fn to_camel_case_snake_case() {
        assert_eq!(to_camel_case("user_signed_in"), "userSignedIn");
    }

    #[test]
    fn to_camel_case_single_char() {
        assert_eq!(to_camel_case("a"), "a");
    }

    // --- ts_type ---

    #[test]
    fn ts_type_string() {
        assert_eq!(ts_type(Some("string")), "string");
    }

    #[test]
    fn ts_type_number() {
        assert_eq!(ts_type(Some("number")), "number");
    }

    #[test]
    fn ts_type_boolean() {
        assert_eq!(ts_type(Some("boolean")), "boolean");
    }

    #[test]
    fn ts_type_none_is_unknown() {
        assert_eq!(ts_type(None), "unknown");
    }

    #[test]
    fn ts_type_unrecognised_is_unknown() {
        assert_eq!(ts_type(Some("custom")), "unknown");
    }

    // --- generate_typescript ---

    #[test]
    fn generate_empty_catalog_has_event_name_never() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("EventName = never"), "output:\n{ts}");
    }

    #[test]
    fn generate_empty_catalog_has_empty_track_object() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("track = {} as const"), "output:\n{ts}");
    }

    #[test]
    fn generate_single_event_interface_present() {
        let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved)]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(ts.contains("PageViewedProperties"), "output:\n{ts}");
    }

    #[test]
    fn generate_single_event_function_present() {
        let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved)]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(ts.contains("trackPageViewed"), "output:\n{ts}");
    }

    #[test]
    fn generate_single_event_track_object_key() {
        let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved)]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(ts.contains("pageViewed: trackPageViewed"), "output:\n{ts}");
    }

    #[test]
    fn generate_pii_property_has_pii_tag() {
        let mut entry = make_entry("user_signed_in", EventStatus::Approved);
        entry.properties.push(make_prop("email", Some("string"), true));
        let cat = make_catalog(vec![entry]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(ts.contains("@pii"), "output:\n{ts}");
    }

    #[test]
    fn generate_non_pii_no_pii_tag() {
        let mut entry = make_entry("page_viewed", EventStatus::Approved);
        entry.properties.push(make_prop("route", Some("string"), false));
        let cat = make_catalog(vec![entry]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(!ts.contains("@pii"), "output:\n{ts}");
    }

    #[test]
    fn generate_description_in_jsdoc() {
        let mut entry = make_entry("page_viewed", EventStatus::Approved);
        entry.description = "Fires on every page navigation.".into();
        let cat = make_catalog(vec![entry]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(ts.contains("Fires on every page navigation."), "output:\n{ts}");
    }

    #[test]
    fn generate_empty_description_uses_single_line_jsdoc() {
        let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved)]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(ts.contains("/** Track a page_viewed event. */"), "output:\n{ts}");
    }

    #[test]
    fn generate_ignores_ignored_events() {
        let cat = make_catalog(vec![make_entry("noise_event", EventStatus::Ignored)]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(!ts.contains("noise_event"), "output:\n{ts}");
    }

    #[test]
    fn generate_ignores_proposed_by_default() {
        let cat = make_catalog(vec![make_entry("maybe_event", EventStatus::Proposed)]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(!ts.contains("maybe_event"), "output:\n{ts}");
    }

    #[test]
    fn generate_includes_proposed_with_flag() {
        let cat = make_catalog(vec![make_entry("maybe_event", EventStatus::Proposed)]);
        let config = CodegenConfig { include_proposed: true };
        let ts = generate_typescript(&cat, &config);
        assert!(ts.contains("maybe_event"), "output:\n{ts}");
    }

    #[test]
    fn generate_events_sorted_alphabetically() {
        let cat = make_catalog(vec![
            make_entry("z_event", EventStatus::Approved),
            make_entry("a_event", EventStatus::Approved),
        ]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        let a_pos = ts.find("a_event").unwrap();
        let z_pos = ts.find("z_event").unwrap();
        assert!(a_pos < z_pos, "a_event should appear before z_event");
    }

    #[test]
    fn generate_event_name_union_lists_all() {
        let cat = make_catalog(vec![
            make_entry("page_viewed", EventStatus::Approved),
            make_entry("user_signed_in", EventStatus::Approved),
        ]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(ts.contains("\"page_viewed\""), "output:\n{ts}");
        assert!(ts.contains("\"user_signed_in\""), "output:\n{ts}");
    }

    #[test]
    fn generate_deterministic() {
        let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved)]);
        let config = CodegenConfig::default();
        let ts1 = generate_typescript(&cat, &config);
        let ts2 = generate_typescript(&cat, &config);
        assert_eq!(ts1, ts2);
    }

    #[test]
    fn generate_empty_props_uses_record_never() {
        let cat = make_catalog(vec![make_entry("click_happened", EventStatus::Approved)]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(
            ts.contains("export type ClickHappenedProperties = Record<string, never>;"),
            "output:\n{ts}"
        );
        // Must not emit a lint-tripping empty interface.
        assert!(!ts.contains("ClickHappenedProperties {}"), "output:\n{ts}");
    }

    #[test]
    fn generate_typed_property_in_interface() {
        let mut entry = make_entry("api_called", EventStatus::Approved);
        entry.properties.push(make_prop("method", Some("string"), false));
        entry.properties.push(make_prop("count", Some("number"), false));
        entry.properties.push(make_prop("cached", Some("boolean"), false));
        let cat = make_catalog(vec![entry]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(ts.contains("method: string;"), "output:\n{ts}");
        assert!(ts.contains("count: number;"), "output:\n{ts}");
        assert!(ts.contains("cached: boolean;"), "output:\n{ts}");
    }

    #[test]
    fn generate_unknown_type_for_untyped_prop() {
        let mut entry = make_entry("page_viewed", EventStatus::Approved);
        entry.properties.push(make_prop("mystery", None, false));
        let cat = make_catalog(vec![entry]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(ts.contains("mystery: unknown;"), "output:\n{ts}");
    }

    #[test]
    fn generate_properties_sorted_alphabetically() {
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
            "properties not sorted: alpha={alpha_pos} mango={mango_pos} zebra={zebra_pos}\noutput:\n{ts}"
        );
    }

    #[test]
    fn generate_has_provider_interface() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("export interface Provider"), "output:\n{ts}");
        assert!(ts.contains("export interface InfergenConfig"), "output:\n{ts}");
    }

    #[test]
    fn generate_has_configure_infergen() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("export function configureInfergen"), "output:\n{ts}");
        assert!(ts.contains("let _providers: Provider[] = []"), "output:\n{ts}");
    }

    #[test]
    fn generate_track_fn_dispatches_providers() {
        let cat = make_catalog(vec![make_entry("page_viewed", EventStatus::Approved)]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(
            ts.contains("_dispatch(\"page_viewed\", properties)"),
            "dispatch call missing\noutput:\n{ts}"
        );
    }

    #[test]
    fn generate_has_set_consent() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("export function setConsent"), "setConsent missing");
    }

    #[test]
    fn generate_has_opt_out() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("export function optOut"), "optOut missing");
    }

    #[test]
    fn generate_has_opt_in() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("export function optIn"), "optIn missing");
    }

    #[test]
    fn generate_has_set_region() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("export function setRegion"), "setRegion missing");
    }

    #[test]
    fn generate_has_set_redact_fn() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("export function setRedactFn"), "setRedactFn missing");
    }

    #[test]
    fn generate_has_dispatch() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("export function _dispatch"), "_dispatch missing");
    }

    #[test]
    fn generate_track_fn_uses_dispatch() {
        let cat = make_catalog(vec![make_entry("user_signed_in", EventStatus::Approved)]);
        let ts = generate_typescript(&cat, &CodegenConfig::default());
        assert!(ts.contains("_dispatch("), "track fn must use _dispatch");
        assert!(!ts.contains("_providers.forEach"), "_providers.forEach must not appear");
    }

    #[test]
    fn generate_infergen_config_has_consent_field() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("consent?: ConsentState"), "consent field missing");
    }

    #[test]
    fn generate_infergen_config_has_region_field() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("region?: string"), "region field missing");
    }

    #[test]
    fn generate_infergen_config_has_redact_fn_field() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("redactFn?: RedactFn"), "redactFn field missing");
    }

    #[test]
    fn generate_configure_infergen_calls_set_consent() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("setConsent(config.consent)"), "setConsent call missing");
    }

    #[test]
    fn generate_configure_infergen_calls_set_region() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("setRegion(config.region)"), "setRegion call missing");
    }

    #[test]
    fn generate_configure_infergen_calls_set_redact_fn() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("setRedactFn(config.redactFn)"), "setRedactFn call missing");
    }

    #[test]
    fn generate_empty_catalog_has_preamble() {
        let ts = generate_typescript(&Catalog::default(), &CodegenConfig::default());
        assert!(ts.contains("export interface Provider"), "preamble missing\noutput:\n{ts}");
        assert!(ts.contains("configureInfergen"), "configureInfergen missing\noutput:\n{ts}");
    }
}
