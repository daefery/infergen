//! Infergen scan engine.
//!
//! This crate houses language parsers (E0.3), framework adapters (E0.4),
//! the heuristic namer (E1.2), and will house codegen (E2.x). E0.1 seeded
//! the shared error type and a version probe; E0.2 adds the project config
//! schema ([`config`]) and language/framework auto-detection ([`detect`]).

use std::path::PathBuf;

pub mod adapter;
pub mod catalog;
pub mod codegen;
pub mod config;
pub mod detect;
pub mod linter;
pub mod namer;
pub mod parser;
pub mod property;
pub mod provider;
pub mod review;

pub use adapter::django::DjangoAdapter;
pub use adapter::echo::EchoAdapter;
pub use adapter::fastapi::FastApiAdapter;
pub use adapter::flask::FlaskAdapter;
pub use adapter::gin::GinAdapter;
pub use adapter::nethttp::NetHttpAdapter;
pub use adapter::nextjs::NextjsAdapter;
pub use adapter::rails::RailsAdapter;
pub use adapter::{Adapter, EventKind, PropertyHint, ProposedEvent};
pub use catalog::{from_proposals, load_catalog, merge_proposals, rescan_merge, save_catalog};
pub use codegen::{CodegenConfig, GoCodegenConfig, RubyCodegenConfig, generate_go, generate_python, generate_ruby, generate_typescript};
pub use config::Config;
pub use detect::{DetectionResult, Framework, Language, detect};
pub use infergen_types::{
    Catalog, CatalogEntry, CatalogEventKind, EventProperty, EventProvenance, EventStatus,
    CATALOG_SCHEMA_VERSION,
};
pub use linter::{ConventionCase, LintRule, LintViolation, lint_catalog};
pub use namer::{NameResult, NameSignals, Namer};
pub use property::{enrich_hints, is_pii_property, type_from_name};
pub use review::{
    CatalogDiff, DiffEntry, EntryChange, approve, approve_all_proposed, diff_catalogs, ignore,
    remove_property, rename, set_description, upsert_property,
};
pub use parser::go::GoParser;
pub use parser::js::JsParser;
pub use parser::py::PyParser;
pub use parser::ruby::RubyParser;
pub use provider::{ProviderPlugin, ProviderRegistry, TrackEvent};
pub use parser::{Diagnostic, LanguageParser, ParsedFile};

/// Version of the on-disk project config (`infergen.config.*`) schema.
///
/// Distinct from [`CATALOG_SCHEMA_VERSION`] (the catalog file). Bump on any
/// breaking change to the config format.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// Errors produced by the scan engine.
///
/// Variants are added as subsystems land. `NotImplemented` is the placeholder
/// returned by stubs until their epic ships.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A subsystem exists in the API but is not yet implemented.
    #[error("not yet implemented: {0}")]
    NotImplemented(&'static str),

    /// I/O failure reading or writing a file.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// A config file could not be parsed (or serialized).
    #[error("failed to parse config at {}: {message}", path.display())]
    ConfigParse {
        /// Path of the offending config file.
        path: PathBuf,
        /// Human-readable parse/serialize error message.
        message: String,
    },

    /// No config file was found during discovery.
    #[error("no infergen config found in {}", dir.display())]
    ConfigNotFound {
        /// Directory that was searched.
        dir: PathBuf,
    },

    /// A config file already exists (e.g. `init` without `--force`).
    #[error("config already exists at {} (use --force to overwrite)", path.display())]
    ConfigExists {
        /// Path of the existing config file.
        path: PathBuf,
    },

    /// A file extension is not a supported config format.
    #[error("unsupported config format: {} (expected .json or .toml)", path.display())]
    UnsupportedFormat {
        /// Path with the unsupported extension.
        path: PathBuf,
    },

    /// A catalog file could not be parsed or serialized.
    #[error("failed to parse catalog at {}: {message}", path.display())]
    CatalogParse {
        /// Path of the offending catalog file.
        path: PathBuf,
        /// Human-readable parse/serialize error message.
        message: String,
    },

    /// An event ID was not found in the catalog.
    #[error("event not found: {id}")]
    EventNotFound {
        /// The ID that was looked up.
        id: String,
    },

    /// A proposed event name is invalid.
    #[error("invalid event name {name:?}: {reason}")]
    InvalidEventName {
        /// The name that failed validation.
        name: String,
        /// Human-readable reason.
        reason: String,
    },

    /// A provider plugin returned an error during a tracking call.
    #[error("provider {id:?} failed: {message}")]
    ProviderError {
        /// The offending provider's ID.
        id: String,
        /// Human-readable error from the provider.
        message: String,
    },
}

/// Convenience result type for scan-engine fallible operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The version string of the core engine, sourced from Cargo at compile time.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_cargo() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn not_implemented_error_formats() {
        let e = Error::NotImplemented("scan");
        assert_eq!(e.to_string(), "not yet implemented: scan");
    }

    #[test]
    fn reexports_schema_version() {
        assert_eq!(CATALOG_SCHEMA_VERSION, 1);
    }

    #[test]
    fn config_schema_version_is_one() {
        assert_eq!(CONFIG_SCHEMA_VERSION, 1);
    }

    #[test]
    fn config_exists_error_formats() {
        let e = Error::ConfigExists {
            path: PathBuf::from("a.json"),
        };
        assert!(e.to_string().contains("--force"));
    }

    #[test]
    fn catalog_parse_error_formats() {
        let e = Error::CatalogParse {
            path: PathBuf::from("catalog.yaml"),
            message: "unexpected key".into(),
        };
        assert!(e.to_string().contains("catalog.yaml"));
        assert!(e.to_string().contains("unexpected key"));
    }

    #[test]
    fn event_not_found_error_formats() {
        let e = Error::EventNotFound { id: "evt_abc123".into() };
        assert!(e.to_string().contains("evt_abc123"));
    }

    #[test]
    fn invalid_event_name_error_formats() {
        let e = Error::InvalidEventName { name: "".into(), reason: "empty".into() };
        assert!(e.to_string().contains("empty"));
    }

    #[test]
    fn provider_error_formats() {
        let e = Error::ProviderError { id: "posthog".into(), message: "timeout".into() };
        assert!(e.to_string().contains("posthog"));
        assert!(e.to_string().contains("timeout"));
    }
}
