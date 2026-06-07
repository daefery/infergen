//! `infergen scan` — discover source files, run adapters, merge into catalog (E4.1).

use std::path::{Path, PathBuf};

use infergen_core::{
    Config, JsParser,
    NextjsAdapter,
    adapter::Adapter,
    catalog::{from_proposals, load_catalog, rescan_merge, save_catalog},
    parser::LanguageParser,
};

/// Walk `root` recursively and collect `.ts`, `.tsx`, `.js`, `.jsx` files.
///
/// Skips `node_modules`, `.git`, `dist`, `target`, `.next`. Output is sorted
/// for deterministic scan order.
fn collect_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(root, &mut files);
    files.sort();
    files
}

fn collect_files_recursive(dir: &Path, acc: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if path.is_dir() {
            if !matches!(
                name_str.as_ref(),
                "node_modules" | ".git" | "dist" | "target" | ".next"
            ) {
                collect_files_recursive(&path, acc);
            }
        } else if let Some(ext) = path.extension() {
            if matches!(ext.to_str(), Some("ts" | "tsx" | "js" | "jsx")) {
                acc.push(path);
            }
        }
    }
}

/// Run `infergen scan` in the current directory.
pub fn run() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;

    // Load config; tolerate missing config by using defaults
    let config = match Config::load_from_dir(&cwd) {
        Ok(c) => c,
        Err(infergen_core::Error::ConfigNotFound { .. }) => Config::default(),
        Err(e) => return Err(e.into()),
    };

    let source_files = collect_source_files(&cwd);
    println!("scan: {} files", source_files.len());

    let parser = JsParser;
    let mut all_proposals = Vec::new();

    for file_path in &source_files {
        let Ok(source) = std::fs::read_to_string(file_path) else { continue };
        let Ok(parsed) = parser.parse(file_path, &source) else { continue };
        let adapter = NextjsAdapter::new(&cwd);
        all_proposals.extend(adapter.analyze(&parsed));
    }

    let catalog_path = cwd.join(&config.catalog);

    match load_catalog(&catalog_path) {
        Ok(existing) => {
            let merged = rescan_merge(&existing, &all_proposals, &cwd);

            let new_count = merged
                .events
                .iter()
                .filter(|e| !existing.events.iter().any(|ex| ex.id == e.id))
                .count();
            let removed_count = existing
                .events
                .iter()
                .filter(|ex| !merged.events.iter().any(|me| me.id == ex.id))
                .count();
            let matched_count = merged.events.len().saturating_sub(new_count);

            println!("  {new_count} new events added");
            println!("  {removed_count} events removed (Proposed, no longer detected)");
            println!("  {matched_count} events matched (edits preserved)");

            save_catalog(&merged, &catalog_path)?;
        }
        Err(infergen_core::Error::Io(_)) => {
            // No catalog yet — fresh scan
            let cat = from_proposals(&all_proposals, &cwd);
            println!("  {} events proposed", cat.events.len());
            save_catalog(&cat, &catalog_path)?;
        }
        Err(e) => return Err(e.into()),
    }

    println!("catalog saved to {}", catalog_path.display());
    Ok(())
}
