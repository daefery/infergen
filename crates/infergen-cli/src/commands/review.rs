//! `infergen review` — event catalog review and diff workflow (E1.5).

use std::path::Path;

use anyhow::Context;
use infergen_core::{
    Catalog, CatalogEventKind, EntryChange, EventStatus, approve, diff_catalogs, ignore,
    load_catalog, rename, save_catalog, set_description,
};
use infergen_core::CATALOG_SCHEMA_VERSION;

use crate::cli::{ReviewAction, ReviewArgs};

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub fn run(args: ReviewArgs) -> anyhow::Result<()> {
    match args.action {
        ReviewAction::List { status } => list(&args.catalog, &status),
        ReviewAction::Approve { id } => mutate_catalog(&args.catalog, |cat| {
            let name = event_name(cat, &id);
            approve(cat, &id)?;
            println!("ok: {id} ({name}) → approved");
            Ok(())
        }),
        ReviewAction::Ignore { id } => mutate_catalog(&args.catalog, |cat| {
            let name = event_name(cat, &id);
            ignore(cat, &id)?;
            println!("ok: {id} ({name}) → ignored");
            Ok(())
        }),
        ReviewAction::Rename { id, new_name } => mutate_catalog(&args.catalog, |cat| {
            let old = event_name(cat, &id);
            rename(cat, &id, &new_name)?;
            println!("ok: renamed {old} → {new_name}");
            Ok(())
        }),
        ReviewAction::Describe { id, description } => mutate_catalog(&args.catalog, |cat| {
            set_description(cat, &id, &description)?;
            println!("ok: description set on {id}");
            Ok(())
        }),
        ReviewAction::Diff { proposed } => run_diff(&args.catalog, &proposed),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

fn list(catalog_path: &Path, status_filter: &str) -> anyhow::Result<()> {
    let filter = parse_status_filter(status_filter)?;
    let catalog = load_catalog_or_empty(catalog_path)?;

    let events: Vec<_> = catalog.events.iter().filter(|e| filter.matches(e.status)).collect();
    let proposed_count = catalog.events.iter().filter(|e| e.status == EventStatus::Proposed).count();
    let suffix = if proposed_count > 0 {
        format!(", {proposed_count} proposed")
    } else {
        String::new()
    };
    println!("Catalog: {}  ({} events{})", catalog_path.display(), catalog.events.len(), suffix);

    if events.is_empty() {
        println!("(no events match filter)");
        return Ok(());
    }
    for e in events {
        println!(
            "[{:8}] {}  {:<40}  ({})",
            status_str(e.status),
            e.id,
            e.name,
            kind_str(e.kind),
        );
    }
    Ok(())
}

fn run_diff(existing_path: &Path, proposed_path: &Path) -> anyhow::Result<()> {
    let existing = load_catalog_or_empty(existing_path)?;
    let proposed = load_catalog(proposed_path)
        .with_context(|| format!("reading proposed catalog {}", proposed_path.display()))?;
    let diff = diff_catalogs(&existing, &proposed);

    if diff.is_clean() {
        println!("catalog is up to date (no changes detected)");
        return Ok(());
    }

    println!("=== Catalog Diff ===");

    if !diff.added.is_empty() {
        println!("\nAdded ({}):", diff.added.len());
        for e in &diff.added {
            let src = e.provenance.first().map(|p| p.source_path.as_str()).unwrap_or("?");
            println!("  + {}  [{}]  {}", e.name, kind_str(e.kind), src);
        }
    }

    if !diff.removed.is_empty() {
        println!("\nRemoved ({}):", diff.removed.len());
        for e in &diff.removed {
            let src = e.provenance.first().map(|p| p.source_path.as_str()).unwrap_or("?");
            println!("  - {}  [{}]  {}", e.name, kind_str(e.kind), src);
        }
    }

    if !diff.modified.is_empty() {
        println!("\nModified ({}):", diff.modified.len());
        for de in &diff.modified {
            let src = de.existing.provenance.first().map(|p| p.source_path.as_str()).unwrap_or("?");
            println!("  ~ {}  [{}]  {}", de.existing.name, kind_str(de.existing.kind), src);
            for change in &de.changes {
                match change {
                    EntryChange::NameChanged { from, to } => {
                        println!("      name: {from} → {to}");
                    }
                    EntryChange::KindChanged { from, to } => {
                        println!("      kind: {} → {}", kind_str(*from), kind_str(*to));
                    }
                    EntryChange::PropertyAdded(p) => {
                        let t = p.prop_type.as_deref().unwrap_or("?");
                        println!("      + property: {} ({})", p.name, t);
                    }
                    EntryChange::PropertyRemoved(name) => {
                        println!("      - property: {name}");
                    }
                    EntryChange::PropertyChanged { name, from, to } => {
                        let ft = from.prop_type.as_deref().unwrap_or("?");
                        let tt = to.prop_type.as_deref().unwrap_or("?");
                        println!("      ~ property: {name}  {ft} → {tt}");
                    }
                }
            }
        }
    }

    println!("\nUnchanged: {} events", diff.unchanged.len());
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load a catalog, returning an empty one if the file doesn't exist.
fn load_catalog_or_empty(path: &Path) -> anyhow::Result<Catalog> {
    if path.exists() {
        load_catalog(path).with_context(|| format!("reading catalog {}", path.display()))
    } else {
        Ok(Catalog { schema_version: CATALOG_SCHEMA_VERSION, events: Vec::new() })
    }
}

/// Load, mutate, and save a catalog.
fn mutate_catalog<F>(path: &Path, f: F) -> anyhow::Result<()>
where
    F: FnOnce(&mut Catalog) -> anyhow::Result<()>,
{
    let mut catalog = load_catalog(path)
        .with_context(|| format!("reading catalog {}", path.display()))?;
    f(&mut catalog)?;
    save_catalog(&catalog, path)
        .with_context(|| format!("writing catalog {}", path.display()))?;
    Ok(())
}

/// Look up an event name by ID (returns the ID itself if not found — safe fallback).
fn event_name(catalog: &Catalog, id: &str) -> String {
    catalog
        .events
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| id.to_owned())
}

enum StatusFilter {
    All,
    Proposed,
    Approved,
    Ignored,
}

impl StatusFilter {
    fn matches(&self, s: EventStatus) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Proposed => s == EventStatus::Proposed,
            StatusFilter::Approved => s == EventStatus::Approved,
            StatusFilter::Ignored => s == EventStatus::Ignored,
        }
    }
}

fn parse_status_filter(s: &str) -> anyhow::Result<StatusFilter> {
    match s {
        "all" => Ok(StatusFilter::All),
        "proposed" => Ok(StatusFilter::Proposed),
        "approved" => Ok(StatusFilter::Approved),
        "ignored" => Ok(StatusFilter::Ignored),
        other => anyhow::bail!("unknown status filter {other:?}: expected all, proposed, approved, or ignored"),
    }
}

fn status_str(s: EventStatus) -> &'static str {
    match s {
        EventStatus::Proposed => "proposed",
        EventStatus::Approved => "approved",
        EventStatus::Ignored => "ignored",
    }
}

fn kind_str(k: CatalogEventKind) -> &'static str {
    match k {
        CatalogEventKind::PageView => "pageView",
        CatalogEventKind::ApiCall => "apiCall",
        CatalogEventKind::AuthEvent => "authEvent",
        CatalogEventKind::FormSubmit => "formSubmit",
        CatalogEventKind::Error => "error",
    }
}

