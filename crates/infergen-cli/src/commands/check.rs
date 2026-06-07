//! `infergen check` — CI drift detection (E4.2).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Context;
use infergen_core::{
    Config, EventStatus, JsParser, NextjsAdapter,
    adapter::Adapter,
    catalog::{from_proposals, load_catalog},
    lint_catalog,
    parser::LanguageParser,
};

use crate::cli::CheckArgs;

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

/// Run `infergen check` — CI drift detection.
///
/// Checks three tiers in order and reports all issues before failing:
/// 1. New untracked moments (scan detects events absent from the catalog)
/// 2. Unreviewed events (existing catalog has `Proposed` entries)
/// 3. Naming convention violations in the existing catalog
///
/// Exits non-zero if any issue is found. Never writes to disk.
pub fn run(args: CheckArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;

    let config = match Config::load_from_dir(&cwd) {
        Ok(c) => c,
        Err(infergen_core::Error::ConfigNotFound { .. }) => Config::default(),
        Err(e) => return Err(e.into()),
    };

    let catalog_path = args.catalog.unwrap_or_else(|| cwd.join(&config.catalog));

    let existing = load_catalog(&catalog_path).with_context(|| {
        format!(
            "check: catalog not found at {} — run `infergen scan` first",
            catalog_path.display()
        )
    })?;

    // Scan source files (read-only — no catalog write)
    let source_files = collect_source_files(&cwd);
    let parser = JsParser;
    let mut all_proposals = Vec::new();
    for file_path in &source_files {
        let Ok(source) = std::fs::read_to_string(file_path) else { continue };
        let Ok(parsed) = parser.parse(file_path, &source) else { continue };
        let adapter = NextjsAdapter::new(&cwd);
        all_proposals.extend(adapter.analyze(&parsed));
    }

    // Tier 1: new untracked moments
    let proposal_catalog = from_proposals(&all_proposals, &cwd);
    let existing_ids: HashSet<&str> = existing.events.iter().map(|e| e.id.as_str()).collect();
    let new_untracked: Vec<_> = proposal_catalog
        .events
        .iter()
        .filter(|e| !existing_ids.contains(e.id.as_str()))
        .collect();

    // Tier 2: unreviewed events in existing catalog
    let unreviewed: Vec<_> = existing
        .events
        .iter()
        .filter(|e| e.status == EventStatus::Proposed)
        .collect();

    // Tier 3: naming convention violations in existing catalog
    let violations = lint_catalog(&existing, &config.naming);

    let mut issue_count = 0;

    if !new_untracked.is_empty() {
        println!(
            "check: {} new untracked moment(s) — run `infergen scan` to record:",
            new_untracked.len()
        );
        for e in &new_untracked {
            println!("  + {} ({})", e.name, e.id);
        }
        issue_count += new_untracked.len();
    }

    if !unreviewed.is_empty() {
        println!(
            "check: {} unreviewed event(s) — run `infergen review` to approve or ignore:",
            unreviewed.len()
        );
        for e in &unreviewed {
            println!("  ? {} ({})", e.name, e.id);
        }
        issue_count += unreviewed.len();
    }

    if !violations.is_empty() {
        println!("check: {} naming convention violation(s):", violations.len());
        for v in &violations {
            if let Some(ref s) = v.suggestion {
                println!("  ! {} \u{2192} {}", v.event_name, s);
            } else {
                println!("  ! {}: {}", v.event_name, v.message);
            }
        }
        issue_count += violations.len();
    }

    if issue_count > 0 {
        anyhow::bail!("check failed: {} issue(s) found", issue_count);
    }

    println!("check: OK \u{2014} catalog is clean");
    Ok(())
}
