//! Shared, dependency-free domain types for Infergen.
//!
//! This crate is the leaf of the dependency graph: it pulls in no external
//! crates so every other workspace member (and, by mirroring, every language
//! runtime) can depend on it freely. Concrete domain types (the event catalog
//! schema) land here in epic E1.1; for now it carries the schema version that
//! all readers/writers and generated runtimes must agree on.

/// Version of the on-disk catalog (`catalog.yaml`) schema.
///
/// Bump on any breaking change to the catalog format. The TypeScript runtime
/// mirrors this as `CATALOG_SCHEMA_VERSION` in `@infergen/runtime`; codegen
/// (E2.x) will assert the two stay in lockstep.
pub const CATALOG_SCHEMA_VERSION: u32 = 1;

/// The version string of this crate, sourced from Cargo at compile time.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
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
}
