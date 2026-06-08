//! Framework adapter abstraction.
//!
//! An [`Adapter`] inspects a [`ParsedFile`] and returns [`ProposedEvent`]s —
//! candidate analytics tracking moments.  Adapters are stateless, infallible,
//! and framework-specific.  The output feeds the catalog (E1.1) after human
//! review.

use std::path::PathBuf;

use crate::{detect::Framework, parser::ParsedFile};

pub mod nextjs;

/// Broad category of a proposed tracking moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// User navigated to a page / screen.
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

/// A candidate event property hinted by the adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyHint {
    /// Suggested property name (snake_case preferred; E1.3 normalises).
    pub name: String,
    /// TS/JS type hint, e.g. `"string"`, `"number"`, `"boolean"`. `None` if
    /// the adapter cannot infer a type.
    pub type_hint: Option<String>,
    /// `true` if this property likely contains personally identifiable
    /// information (email, name, phone, address). Conservative: false when
    /// uncertain.
    pub pii_hint: bool,
}

/// A candidate analytics event proposed by a framework adapter.
///
/// This is **not** a catalog entry — it is a pre-review proposal. The catalog
/// schema (E1.1) defines the persistent format after a human approves/edits.
#[derive(Debug, Clone)]
pub struct ProposedEvent {
    /// Heuristic event name. Not yet convention-enforced (E1.3). Reviewers may
    /// rename freely.
    pub name: String,
    /// Category of tracking moment.
    pub kind: EventKind,
    /// Absolute path of the source file that triggered this proposal.
    pub source_path: PathBuf,
    /// Adapter confidence, `0.0` (guess) – `1.0` (certain).
    /// Path-based detection: 0.9. AST-based import detection: 0.85.
    /// Name-heuristic detection: 0.7.
    pub confidence: f32,
    /// Candidate event properties.
    pub properties: Vec<PropertyHint>,
    /// Name of the adapter that produced this proposal, e.g. `"nextjs"`.
    /// Defaults to `""` when not set. Flows to `EventProvenance.adapter` in
    /// the catalog (E1.1+E1.2).
    pub adapter: String,
}

impl ProposedEvent {
    /// Convenience constructor for the common case.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        kind: EventKind,
        source_path: impl Into<PathBuf>,
        confidence: f32,
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            source_path: source_path.into(),
            confidence,
            properties: Vec::new(),
            adapter: String::new(),
        }
    }

    /// Add a property hint and return `self` for chaining.
    #[must_use]
    pub fn with_prop(mut self, name: impl Into<String>, type_hint: Option<&str>) -> Self {
        self.properties.push(PropertyHint {
            name: name.into(),
            type_hint: type_hint.map(str::to_owned),
            pii_hint: false,
        });
        self
    }

    /// Set the adapter attribution and return `self` for chaining.
    #[must_use]
    pub fn with_adapter(mut self, adapter: impl Into<String>) -> Self {
        self.adapter = adapter.into();
        self
    }
}

/// Contract for framework adapters.
///
/// Adapters are **infallible**: they return an empty `Vec` when nothing is
/// detected rather than propagating errors. I/O (reading files from disk)
/// happens outside the adapter — it receives an already-parsed [`ParsedFile`].
pub trait Adapter {
    /// Analyse `file` and return zero or more proposed events.
    fn analyze(&self, file: &ParsedFile) -> Vec<ProposedEvent>;
    /// The framework this adapter targets.
    fn framework(&self) -> Framework;
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn proposed_event_new_has_no_properties() {
        let e = ProposedEvent::new("page_viewed", EventKind::PageView, "app.ts", 0.9);
        assert!(e.properties.is_empty());
        assert_eq!(e.name, "page_viewed");
        assert_eq!(e.confidence, 0.9_f32);
    }

    #[test]
    fn proposed_event_with_prop_chains() {
        let e = ProposedEvent::new("page_viewed", EventKind::PageView, "app.ts", 0.9)
            .with_prop("route", Some("string"))
            .with_prop("referrer", None);
        assert_eq!(e.properties.len(), 2);
        assert_eq!(e.properties[0].name, "route");
        assert_eq!(e.properties[0].type_hint.as_deref(), Some("string"));
        assert_eq!(e.properties[1].name, "referrer");
        assert!(e.properties[1].type_hint.is_none());
    }

    #[test]
    fn property_hint_pii_defaults_false() {
        let h = PropertyHint {
            name: "email".into(),
            type_hint: Some("string".into()),
            pii_hint: false,
        };
        assert!(!h.pii_hint);
    }

    /// Adapter must be object-safe so Vec<Box<dyn Adapter>> is possible.
    #[test]
    fn adapter_is_object_safe() {
        fn _accepts(_: &dyn Adapter) {}
    }

    #[test]
    fn source_path_roundtrips() {
        let e = ProposedEvent::new(
            "x",
            EventKind::ApiCall,
            PathBuf::from("pages/api/foo.ts"),
            0.9,
        );
        assert_eq!(e.source_path, PathBuf::from("pages/api/foo.ts"));
    }

    #[test]
    fn proposed_event_adapter_defaults_empty() {
        let e = ProposedEvent::new("page_viewed", EventKind::PageView, "app.ts", 0.9);
        assert_eq!(e.adapter, "");
    }

    #[test]
    fn with_adapter_sets_name() {
        let e = ProposedEvent::new("page_viewed", EventKind::PageView, "app.ts", 0.9)
            .with_adapter("nextjs");
        assert_eq!(e.adapter, "nextjs");
    }

    #[test]
    fn with_adapter_chains_with_with_prop() {
        let e = ProposedEvent::new("page_viewed", EventKind::PageView, "app.ts", 0.9)
            .with_prop("route", Some("string"))
            .with_adapter("nextjs");
        assert_eq!(e.adapter, "nextjs");
        assert_eq!(e.properties.len(), 1);
    }
}
