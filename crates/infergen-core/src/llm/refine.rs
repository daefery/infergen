//! LLM catalog refinement (E6.1).
//!
//! `refine_catalog_with_config` is the main entry point called from the scan
//! command.  It filters low-confidence `Proposed` events, batches them, sends
//! each batch to the configured LLM backend, and applies validated suggestions
//! back to the catalog.
//!
//! All errors are soft failures — the catalog is never left in a partial state
//! if the LLM is unavailable or returns garbage.

use infergen_types::{Catalog, CatalogEventKind, EventStatus};

use crate::llm::{
    EventInput, LlmBackend, LlmError, PropertyInput, RefinementRequest, RefinementResponse,
};
use crate::llm::config::LlmConfig;

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Refine `catalog` using the provided `backend`.
///
/// Only `Proposed` events with `confidence < config.confidence_threshold` are
/// sent to the LLM.  `Approved` and `Ignored` events are never touched.
///
/// Returns the number of events that received at least one change.
pub fn refine_catalog(
    catalog: &mut Catalog,
    config: &LlmConfig,
    backend: &dyn LlmBackend,
) -> usize {
    let candidates: Vec<String> = catalog
        .events
        .iter()
        .filter(|e| {
            e.status == EventStatus::Proposed && e.confidence < config.confidence_threshold
        })
        .map(|e| e.id.clone())
        .collect();

    if candidates.is_empty() {
        return 0;
    }

    let context = project_context(catalog);
    let mut total_modified = 0usize;

    for chunk in candidates.chunks(config.batch_size) {
        let inputs: Vec<EventInput> = chunk
            .iter()
            .filter_map(|id| catalog.events.iter().find(|e| &e.id == id))
            .map(|e| EventInput {
                id: e.id.clone(),
                name: e.name.clone(),
                kind: kind_str(e.kind),
                confidence: e.confidence,
                source_paths: e.provenance.iter().map(|p| p.source_path.clone()).collect(),
                description: e.description.clone(),
                properties: e
                    .properties
                    .iter()
                    .map(|p| PropertyInput {
                        name: p.name.clone(),
                        prop_type: p.prop_type.clone(),
                        pii: p.pii,
                    })
                    .collect(),
            })
            .collect();

        let request = RefinementRequest { events: inputs, project_context: context.clone() };

        match backend.refine_batch(&request) {
            Ok(response) => {
                total_modified += apply_response(catalog, &response);
            }
            Err(e) => {
                eprintln!("infergen: LLM batch failed ({e}); continuing without refinement");
            }
        }
    }

    total_modified
}

/// Factory wrapper: builds the backend from config then calls `refine_catalog`.
///
/// Returns `Err` only if the backend cannot be constructed (e.g. missing API
/// key).  Batch-level errors are soft failures reported via stderr.
pub fn refine_catalog_with_config(
    catalog: &mut Catalog,
    config: &LlmConfig,
) -> Result<usize, LlmError> {
    let backend = crate::llm::provider::make_backend(config)?;
    Ok(refine_catalog(catalog, config, backend.as_ref()))
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Apply validated LLM suggestions to the catalog.  Returns changed count.
fn apply_response(catalog: &mut Catalog, response: &RefinementResponse) -> usize {
    let mut changed = 0usize;
    for output in &response.events {
        let Some(entry) = catalog
            .events
            .iter_mut()
            .find(|e| e.id == output.id && e.status == EventStatus::Proposed)
        else {
            continue;
        };

        let mut entry_changed = false;

        // Name update — only if valid snake_case and different from current.
        if let Some(name) = &output.name {
            let name = name.trim();
            if !name.is_empty() && is_valid_snake_case(name) && name != entry.name {
                entry.name = name.to_owned();
                entry_changed = true;
            }
        }

        // Description update — only fill empty descriptions.
        if entry.description.is_empty() {
            if let Some(desc) = &output.description {
                let desc = desc.trim();
                if !desc.is_empty() {
                    entry.description = desc.to_owned();
                    entry_changed = true;
                }
            }
        }

        // Property type update — only fill None types.
        for prop_out in &output.properties {
            if let Some(prop) = entry.properties.iter_mut().find(|p| p.name == prop_out.name) {
                if prop.prop_type.is_none() {
                    if let Some(t) = &prop_out.prop_type {
                        let t = t.trim().to_lowercase();
                        if matches!(t.as_str(), "string" | "number" | "boolean" | "object") {
                            prop.prop_type = Some(t);
                            entry_changed = true;
                        }
                    }
                }
            }
        }

        if entry_changed {
            changed += 1;
        }
    }
    changed
}

/// Derive a human-readable project context string from catalog metadata.
fn project_context(catalog: &Catalog) -> String {
    // Infer languages from provenance file extensions.
    let mut has_ts = false;
    let mut has_py = false;
    let mut has_go = false;
    let mut has_rb = false;
    for entry in &catalog.events {
        for prov in &entry.provenance {
            let p = prov.source_path.as_str();
            if p.ends_with(".ts") || p.ends_with(".tsx") {
                has_ts = true;
            }
            if p.ends_with(".py") {
                has_py = true;
            }
            if p.ends_with(".go") {
                has_go = true;
            }
            if p.ends_with(".rb") {
                has_rb = true;
            }
        }
    }
    let langs: Vec<&str> = [
        has_ts.then_some("TypeScript"),
        has_py.then_some("Python"),
        has_go.then_some("Go"),
        has_rb.then_some("Ruby"),
    ]
    .iter()
    .flatten()
    .copied()
    .collect();

    if langs.is_empty() {
        "web project".to_owned()
    } else {
        langs.join("/")
    }
}

/// Convert `CatalogEventKind` to the camelCase string used in the prompt.
fn kind_str(kind: CatalogEventKind) -> String {
    match kind {
        CatalogEventKind::PageView => "pageView",
        CatalogEventKind::ApiCall => "apiCall",
        CatalogEventKind::AuthEvent => "authEvent",
        CatalogEventKind::FormSubmit => "formSubmit",
        CatalogEventKind::ButtonClick => "buttonClick",
        CatalogEventKind::Search => "search",
        CatalogEventKind::Error => "error",
    }
    .to_owned()
}

/// Return `true` if `s` is a valid snake_case event name.
///
/// Rules: starts with lowercase letter, contains only lowercase letters,
/// digits, and underscores.
fn is_valid_snake_case(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use infergen_types::{
        Catalog, CatalogEntry, CatalogEventKind, EventProperty, EventProvenance, EventStatus,
        CATALOG_SCHEMA_VERSION,
    };

    use crate::llm::{EventOutput, LlmError, PropertyOutput, RefinementRequest, RefinementResponse};

    // --- Mock backend -------------------------------------------------------

    struct MockBackend(RefinementResponse);

    impl LlmBackend for MockBackend {
        fn refine_batch(&self, _req: &RefinementRequest) -> Result<RefinementResponse, LlmError> {
            Ok(self.0.clone())
        }
    }

    struct FailBackend;

    impl LlmBackend for FailBackend {
        fn refine_batch(&self, _req: &RefinementRequest) -> Result<RefinementResponse, LlmError> {
            Err(LlmError::Http("connection refused".into()))
        }
    }

    // --- Fixtures -----------------------------------------------------------

    fn make_entry(id: &str, name: &str, status: EventStatus, confidence: f64) -> CatalogEntry {
        CatalogEntry {
            id: id.to_owned(),
            name: name.to_owned(),
            description: String::new(),
            status,
            confidence,
            kind: CatalogEventKind::PageView,
            provenance: vec![EventProvenance {
                source_path: "src/pages/index.tsx".into(),
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

    fn make_config(threshold: f64) -> LlmConfig {
        LlmConfig {
            enabled: true,
            confidence_threshold: threshold,
            batch_size: 10,
            ..Default::default()
        }
    }

    fn single_event_response(id: &str, name: Option<&str>, desc: Option<&str>) -> RefinementResponse {
        RefinementResponse {
            events: vec![EventOutput {
                id: id.to_owned(),
                name: name.map(str::to_owned),
                description: desc.map(str::to_owned),
                properties: Vec::new(),
            }],
        }
    }

    // --- Tests --------------------------------------------------------------

    #[test]
    fn refine_renames_proposed_low_confidence() {
        let mut cat = make_catalog(vec![make_entry("evt_001", "page_page_viewed", EventStatus::Proposed, 0.5)]);
        let cfg = make_config(0.75);
        let resp = single_event_response("evt_001", Some("home_page_viewed"), None);
        let n = refine_catalog(&mut cat, &cfg, &MockBackend(resp));
        assert_eq!(n, 1);
        assert_eq!(cat.events[0].name, "home_page_viewed");
    }

    #[test]
    fn refine_skips_approved_events() {
        let mut cat = make_catalog(vec![make_entry("evt_001", "home_page_viewed", EventStatus::Approved, 0.5)]);
        let cfg = make_config(0.75);
        let resp = single_event_response("evt_001", Some("different_name"), None);
        let n = refine_catalog(&mut cat, &cfg, &MockBackend(resp));
        assert_eq!(n, 0);
        assert_eq!(cat.events[0].name, "home_page_viewed");
    }

    #[test]
    fn refine_skips_high_confidence() {
        let mut cat = make_catalog(vec![make_entry("evt_001", "home_page_viewed", EventStatus::Proposed, 0.9)]);
        let cfg = make_config(0.75);
        let resp = single_event_response("evt_001", Some("different_name"), None);
        let n = refine_catalog(&mut cat, &cfg, &MockBackend(resp));
        // high confidence → not sent → not in response → 0 changes
        assert_eq!(n, 0);
        assert_eq!(cat.events[0].name, "home_page_viewed");
    }

    #[test]
    fn refine_fills_empty_description() {
        let mut cat = make_catalog(vec![make_entry("evt_001", "home_page_viewed", EventStatus::Proposed, 0.5)]);
        let cfg = make_config(0.75);
        let resp = single_event_response("evt_001", None, Some("User viewed the home page"));
        refine_catalog(&mut cat, &cfg, &MockBackend(resp));
        assert_eq!(cat.events[0].description, "User viewed the home page");
    }

    #[test]
    fn refine_skips_nonempty_description() {
        let mut entry = make_entry("evt_001", "home_page_viewed", EventStatus::Proposed, 0.5);
        entry.description = "Existing description".to_owned();
        let mut cat = make_catalog(vec![entry]);
        let cfg = make_config(0.75);
        let resp = single_event_response("evt_001", None, Some("Different description"));
        refine_catalog(&mut cat, &cfg, &MockBackend(resp));
        assert_eq!(cat.events[0].description, "Existing description");
    }

    #[test]
    fn refine_infers_none_prop_type() {
        let mut entry = make_entry("evt_001", "form_submitted", EventStatus::Proposed, 0.5);
        entry.properties = vec![EventProperty {
            name: "email".into(),
            prop_type: None,
            required: false,
            pii: true,
        }];
        let mut cat = make_catalog(vec![entry]);
        let cfg = make_config(0.75);
        let resp = RefinementResponse {
            events: vec![EventOutput {
                id: "evt_001".into(),
                name: None,
                description: None,
                properties: vec![PropertyOutput {
                    name: "email".into(),
                    prop_type: Some("string".into()),
                }],
            }],
        };
        refine_catalog(&mut cat, &cfg, &MockBackend(resp));
        assert_eq!(cat.events[0].properties[0].prop_type, Some("string".into()));
    }

    #[test]
    fn refine_skips_known_prop_type() {
        let mut entry = make_entry("evt_001", "form_submitted", EventStatus::Proposed, 0.5);
        entry.properties = vec![EventProperty {
            name: "count".into(),
            prop_type: Some("number".into()),
            required: false,
            pii: false,
        }];
        let mut cat = make_catalog(vec![entry]);
        let cfg = make_config(0.75);
        let resp = RefinementResponse {
            events: vec![EventOutput {
                id: "evt_001".into(),
                name: None,
                description: None,
                properties: vec![PropertyOutput {
                    name: "count".into(),
                    prop_type: Some("string".into()), // LLM disagrees — should be ignored
                }],
            }],
        };
        refine_catalog(&mut cat, &cfg, &MockBackend(resp));
        assert_eq!(cat.events[0].properties[0].prop_type, Some("number".into()));
    }

    #[test]
    fn refine_rejects_invalid_name() {
        let mut cat = make_catalog(vec![make_entry("evt_001", "home_page_viewed", EventStatus::Proposed, 0.5)]);
        let cfg = make_config(0.75);
        // CamelCase is invalid
        let resp = single_event_response("evt_001", Some("HomePageViewed"), None);
        refine_catalog(&mut cat, &cfg, &MockBackend(resp));
        assert_eq!(cat.events[0].name, "home_page_viewed");
    }

    #[test]
    fn refine_backend_error_is_soft() {
        let mut cat = make_catalog(vec![make_entry("evt_001", "home_page_viewed", EventStatus::Proposed, 0.5)]);
        let cfg = make_config(0.75);
        let n = refine_catalog(&mut cat, &cfg, &FailBackend);
        assert_eq!(n, 0);
        assert_eq!(cat.events[0].name, "home_page_viewed");
    }

    #[test]
    fn refine_returns_count_of_modified() {
        let mut cat = make_catalog(vec![
            make_entry("evt_001", "page_page_viewed", EventStatus::Proposed, 0.5),
            make_entry("evt_002", "form_form_submitted", EventStatus::Proposed, 0.4),
        ]);
        let cfg = make_config(0.75);
        let resp = RefinementResponse {
            events: vec![
                EventOutput { id: "evt_001".into(), name: Some("home_page_viewed".into()), description: None, properties: Vec::new() },
                EventOutput { id: "evt_002".into(), name: Some("checkout_submitted".into()), description: None, properties: Vec::new() },
            ],
        };
        let n = refine_catalog(&mut cat, &cfg, &MockBackend(resp));
        assert_eq!(n, 2);
    }

    #[test]
    fn refine_empty_catalog_returns_zero() {
        let mut cat = make_catalog(vec![]);
        let cfg = make_config(0.75);
        let n = refine_catalog(&mut cat, &cfg, &FailBackend);
        assert_eq!(n, 0);
    }

    // --- is_valid_snake_case ------------------------------------------------

    #[test]
    fn valid_snake_case_names() {
        assert!(is_valid_snake_case("home_page_viewed"));
        assert!(is_valid_snake_case("user_signed_up"));
        assert!(is_valid_snake_case("api2_called"));
        assert!(is_valid_snake_case("a"));
    }

    #[test]
    fn invalid_snake_case_names() {
        assert!(!is_valid_snake_case("HomePageViewed")); // PascalCase
        assert!(!is_valid_snake_case("homePageViewed")); // camelCase
        assert!(!is_valid_snake_case("home-page-viewed")); // kebab
        assert!(!is_valid_snake_case("1home"));           // starts with digit
        assert!(!is_valid_snake_case(""));                // empty
        assert!(!is_valid_snake_case("home page viewed")); // spaces
    }
}
