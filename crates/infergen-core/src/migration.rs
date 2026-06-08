//! Catalog schema migration — version compatibility and upgrade path (E8.3).
//!
//! Every [`load_catalog`](crate::catalog::load_catalog) call passes through
//! [`migrate_catalog`] before returning to the caller. The function:
//!
//! 1. Rejects catalogs whose `schema_version` exceeds what this binary supports
//!    (the user must upgrade Infergen to read such a catalog).
//! 2. Applies any pending migrations for catalogs written by older versions.
//! 3. As of E8.3 only schema version 1 exists, so the migration chain is a no-op.
//!
//! ## Adding a future migration (example: v1 → v2)
//!
//! 1. Bump `CATALOG_SCHEMA_VERSION` in `infergen-types` to `2`.
//! 2. Add a `fn migrate_v1_to_v2(catalog: Catalog) -> Result<Catalog>` here.
//! 3. Add `1 => migrate_v1_to_v2(catalog)` to the match arm in `migrate_catalog`.

use std::path::Path;

use infergen_types::{Catalog, CATALOG_SCHEMA_VERSION};

use crate::{Error, Result};

/// Validate `catalog`'s schema version and apply any pending migrations.
///
/// Returns the (possibly upgraded) catalog on success, or [`Error::CatalogParse`]
/// when the version is unrecognised or too new for this binary to handle.
///
/// # Errors
/// - `Error::CatalogParse` — version is `0` (never shipped), greater than
///   [`CATALOG_SCHEMA_VERSION`], or any other unrecognised value.
pub fn migrate_catalog(path: &Path, catalog: Catalog) -> Result<Catalog> {
    if catalog.schema_version > CATALOG_SCHEMA_VERSION {
        return Err(Error::CatalogParse {
            path: path.to_path_buf(),
            message: format!(
                "catalog schema version {} is newer than this infergen binary supports \
                 (max {CATALOG_SCHEMA_VERSION}); upgrade infergen to read this catalog",
                catalog.schema_version,
            ),
        });
    }

    match catalog.schema_version {
        // Current version — no migration needed.
        1 => Ok(catalog),
        // version 0 was never released; treat as invalid.
        unknown => Err(Error::CatalogParse {
            path: path.to_path_buf(),
            message: format!("unknown catalog schema version {unknown}; expected {CATALOG_SCHEMA_VERSION}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use infergen_types::{Catalog, CATALOG_SCHEMA_VERSION};

    use super::*;

    fn path() -> PathBuf {
        PathBuf::from("test_catalog.yaml")
    }

    fn make_catalog(version: u32) -> Catalog {
        Catalog {
            schema_version: version,
            events: Vec::new(),
            flows: Vec::new(),
        }
    }

    #[test]
    fn migrate_catalog_v1_passthrough() {
        let cat = make_catalog(1);
        let result = migrate_catalog(&path(), cat.clone()).unwrap();
        assert_eq!(result.schema_version, 1);
    }

    #[test]
    fn migrate_catalog_too_new_errors() {
        let cat = make_catalog(99);
        let err = migrate_catalog(&path(), cat).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("newer than this infergen binary"), "message: {msg}");
        assert!(msg.contains("99"), "message: {msg}");
    }

    #[test]
    fn migrate_catalog_zero_errors() {
        let cat = make_catalog(0);
        let err = migrate_catalog(&path(), cat).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown catalog schema version"), "message: {msg}");
    }

    #[test]
    fn migrate_catalog_preserves_events() {
        use infergen_types::{CatalogEntry, CatalogEventKind, EventStatus};
        let entry = CatalogEntry {
            id: "evt_test".into(),
            name: "page_viewed".into(),
            description: String::new(),
            status: EventStatus::Proposed,
            confidence: 0.9,
            kind: CatalogEventKind::PageView,
            provenance: Vec::new(),
            properties: Vec::new(),
            providers: Vec::new(),
            package: None,
            flow_ids: Vec::new(),
        };
        let cat = Catalog {
            schema_version: 1,
            events: vec![entry],
            flows: Vec::new(),
        };
        let result = migrate_catalog(&path(), cat).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].name, "page_viewed");
    }

    #[test]
    fn migrate_catalog_current_version_is_valid() {
        let cat = make_catalog(CATALOG_SCHEMA_VERSION);
        assert!(migrate_catalog(&path(), cat).is_ok());
    }
}
