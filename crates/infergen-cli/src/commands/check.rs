//! `infergen check` — CI drift detection (E4.2) with JSON output (E4.4).

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
use serde::Serialize;

use crate::cli::CheckArgs;

/// Machine-readable output of `infergen check --json`.
#[derive(Debug, Serialize)]
pub struct CheckReport {
    /// `true` when catalog is clean; `false` when any issue was found.
    pub ok: bool,
    /// Total number of issues across all tiers.
    pub issue_count: usize,
    /// Tier 1: events detected in source but absent from the catalog.
    pub new_untracked: Vec<EventRef>,
    /// Tier 2: events in the catalog with `Proposed` status (not yet reviewed).
    pub unreviewed: Vec<EventRef>,
    /// Tier 3: naming convention violations in the existing catalog.
    pub violations: Vec<ViolationRef>,
}

/// Minimal event reference: ID + name.
#[derive(Debug, Serialize)]
pub struct EventRef {
    /// Stable event ID (e.g. `evt_0123456789abcdef`).
    pub id: String,
    /// Human-readable event name.
    pub name: String,
}

/// A naming convention violation.
#[derive(Debug, Serialize)]
pub struct ViolationRef {
    /// Name of the event that violated the convention.
    pub event_name: String,
    /// Human-readable description of the violation.
    pub message: String,
    /// Suggested replacement name, if one can be derived.
    pub suggestion: Option<String>,
}

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
        issue_count += new_untracked.len();
    }
    if !unreviewed.is_empty() {
        issue_count += unreviewed.len();
    }
    if !violations.is_empty() {
        issue_count += violations.len();
    }

    // ── JSON output mode (E4.4) ─────────────────────────────────────────────
    if args.json {
        let report = CheckReport {
            ok: issue_count == 0,
            issue_count,
            new_untracked: new_untracked
                .iter()
                .map(|e| EventRef { id: e.id.clone(), name: e.name.clone() })
                .collect(),
            unreviewed: unreviewed
                .iter()
                .map(|e| EventRef { id: e.id.clone(), name: e.name.clone() })
                .collect(),
            violations: violations
                .iter()
                .map(|v| ViolationRef {
                    event_name: v.event_name.clone(),
                    message: v.message.clone(),
                    suggestion: v.suggestion.clone(),
                })
                .collect(),
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        if issue_count > 0 {
            // Exit non-zero; JSON stdout is the authoritative error output.
            anyhow::bail!("check failed: {} issue(s) — see JSON output above", issue_count);
        }
        return Ok(());
    }

    // ── Human-readable output mode ───────────────────────────────────────────
    if !new_untracked.is_empty() {
        println!(
            "check: {} new untracked moment(s) — run `infergen scan` to record:",
            new_untracked.len()
        );
        for e in &new_untracked {
            println!("  + {} ({})", e.name, e.id);
        }
    }

    if !unreviewed.is_empty() {
        println!(
            "check: {} unreviewed event(s) — run `infergen review` to approve or ignore:",
            unreviewed.len()
        );
        for e in &unreviewed {
            println!("  ? {} ({})", e.name, e.id);
        }
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
    }

    if issue_count > 0 {
        anyhow::bail!("check failed: {} issue(s) found", issue_count);
    }

    println!("check: OK \u{2014} catalog is clean");
    Ok(())
}
