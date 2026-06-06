//! Telemetra scan engine.
//!
//! This crate will house the language parsers (E0.3), framework adapters
//! (E0.4), the heuristic namer (E1.2), and codegen (E2.x). For the E0.1
//! scaffold it exposes only the shared error type and a version probe so the
//! CLI and tooling can link against a real, testable surface.

pub use telemetra_types::CATALOG_SCHEMA_VERSION;

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
}
