//! `infergen view` — generate an offline catalog web viewer (E7.1).
//!
//! Reads `.infergen/catalog.yaml`, embeds all catalog data as JSON in a
//! self-contained HTML file, and opens it in the default browser.

use std::path::Path;

use anyhow::Context;
use infergen_core::{Catalog, load_catalog};

use crate::cli::ViewArgs;

/// HTML template with `__CATALOG_JSON__` placeholder.
const TEMPLATE: &str = include_str!("view.html");

/// Run `infergen view`.
pub fn run(args: ViewArgs) -> anyhow::Result<()> {
    // Load catalog. Missing catalog → generate empty viewer (useful before first scan).
    let catalog = match load_catalog(&args.catalog) {
        Ok(c) => c,
        Err(infergen_core::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "note: catalog not found at {} — generating empty viewer",
                args.catalog.display()
            );
            Catalog::default()
        }
        Err(e) => return Err(e.into()),
    };

    // Serialize catalog to compact JSON for embedding.
    let catalog_json =
        serde_json::to_string(&catalog).context("serialize catalog to JSON")?;

    // Inject JSON into template.
    let html = TEMPLATE.replace("__CATALOG_JSON__", &catalog_json);

    // Determine output path: explicit --output, or catalog-viewer.html next to catalog.
    let output: std::path::PathBuf = match &args.output {
        Some(p) => p.clone(),
        None => args
            .catalog
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("catalog-viewer.html"),
    };

    // Create parent directories if needed.
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }

    // Write HTML.
    std::fs::write(&output, html)
        .with_context(|| format!("write viewer to {}", output.display()))?;

    println!("catalog viewer → {}", output.display());

    // Open in browser unless suppressed.
    if !args.no_open {
        if let Err(e) = open::that(&output) {
            eprintln!("note: could not open browser automatically: {e}");
            eprintln!("      open manually: {}", output.display());
        }
    } else {
        println!("open in browser: {}", output.display());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use infergen_core::{
        Catalog, CatalogEntry, CatalogEventKind, EventStatus, CATALOG_SCHEMA_VERSION,
    };
    use tempfile::tempdir;

    use super::*;

    fn make_catalog(entries: Vec<CatalogEntry>) -> Catalog {
        Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: entries }
    }

    fn make_entry(name: &str, status: EventStatus) -> CatalogEntry {
        CatalogEntry {
            id: format!("evt_{:016x}", name.len()),
            name: name.to_owned(),
            description: String::new(),
            status,
            confidence: 0.9,
            kind: CatalogEventKind::PageView,
            provenance: Vec::new(),
            properties: Vec::new(),
            providers: Vec::new(),
            package: None,
        }
    }

    fn run_view(catalog: &Catalog, dir: &std::path::Path) -> PathBuf {
        // Write catalog yaml
        let cat_path = dir.join(".infergen/catalog.yaml");
        infergen_core::catalog::save_catalog(catalog, &cat_path).unwrap();

        let output = dir.join("catalog-viewer.html");
        let args = ViewArgs {
            catalog: cat_path,
            output: Some(output.clone()),
            no_open: true,
        };
        run(args).unwrap();
        output
    }

    #[test]
    fn view_writes_html_file() {
        let dir = tempdir().unwrap();
        let cat = make_catalog(vec![]);
        let out = run_view(&cat, dir.path());
        assert!(out.exists(), "HTML file should be written");
    }

    #[test]
    fn view_embeds_catalog_json() {
        let dir = tempdir().unwrap();
        let cat = make_catalog(vec![make_entry("user_signed_up", EventStatus::Approved)]);
        let out = run_view(&cat, dir.path());
        let html = std::fs::read_to_string(&out).unwrap();
        assert!(html.contains("user_signed_up"), "event name should appear in HTML");
        assert!(html.contains("approved"), "status should appear in HTML");
    }

    #[test]
    fn view_empty_catalog_no_panic() {
        let dir = tempdir().unwrap();
        let cat = make_catalog(vec![]);
        let out = run_view(&cat, dir.path());
        let html = std::fs::read_to_string(&out).unwrap();
        assert!(html.contains("CATALOG"), "template scaffold should be present");
    }

    #[test]
    fn view_custom_output_path() {
        let dir = tempdir().unwrap();
        let cat = make_catalog(vec![]);
        let custom_out = dir.path().join("custom/my-viewer.html");
        let cat_path = dir.path().join(".infergen/catalog.yaml");
        infergen_core::catalog::save_catalog(&cat, &cat_path).unwrap();
        let args = ViewArgs {
            catalog: cat_path,
            output: Some(custom_out.clone()),
            no_open: true,
        };
        run(args).unwrap();
        assert!(custom_out.exists(), "should write to custom path");
    }

    #[test]
    fn html_contains_event_names() {
        let dir = tempdir().unwrap();
        let cat = make_catalog(vec![
            make_entry("home_page_viewed", EventStatus::Approved),
            make_entry("checkout_submitted", EventStatus::Proposed),
            make_entry("old_noise", EventStatus::Ignored),
        ]);
        let out = run_view(&cat, dir.path());
        let html = std::fs::read_to_string(&out).unwrap();
        assert!(html.contains("home_page_viewed"));
        assert!(html.contains("checkout_submitted"));
        assert!(html.contains("old_noise"));
    }

    #[test]
    fn html_valid_json_placeholder_replaced() {
        let dir = tempdir().unwrap();
        let cat = make_catalog(vec![make_entry("test_event", EventStatus::Proposed)]);
        let out = run_view(&cat, dir.path());
        let html = std::fs::read_to_string(&out).unwrap();
        // Placeholder must not appear in final output
        assert!(!html.contains("__CATALOG_JSON__"), "placeholder should be replaced");
        // JSON opening must appear
        assert!(html.contains("\"schemaVersion\""), "JSON should be embedded");
    }

    #[test]
    fn view_missing_catalog_generates_empty_viewer() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("viewer.html");
        let args = ViewArgs {
            catalog: dir.path().join(".infergen/catalog.yaml"), // doesn't exist
            output: Some(output.clone()),
            no_open: true,
        };
        run(args).unwrap(); // should not error
        let html = std::fs::read_to_string(&output).unwrap();
        assert!(html.contains("CATALOG"), "should produce valid HTML");
    }
}
