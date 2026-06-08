//! `infergen scan` — discover source files, run adapters, merge into catalog (E4.1/E5.1).

use std::path::{Path, PathBuf};

use infergen_core::{
    Config, DjangoAdapter, EventKind, FastApiAdapter, FlaskAdapter, FeedbackStore, JsParser,
    NextjsAdapter, PyParser,
    adapter::Adapter,
    catalog::{from_proposals, load_catalog, rescan_merge, save_catalog},
    detect::{Framework, Language, detect},
    parser::LanguageParser,
    quality_path,
    refine_catalog_with_config,
};

/// Walk `root` recursively and collect JS/TS files.
///
/// Skips `node_modules`, `.git`, `dist`, `target`, `.next`. Output is sorted
/// for deterministic scan order.
fn collect_js_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(root, &mut files, |ext| {
        matches!(ext, "ts" | "tsx" | "js" | "jsx")
    });
    files.sort();
    files
}

/// Walk `root` recursively and collect `.py` files.
///
/// Skips `__pycache__`, `venv`, `.venv`, `env`, `build`, `dist`, `target`,
/// `.git`, `node_modules`, `site-packages`.
fn collect_py_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(root, &mut files, |ext| ext == "py");
    files.sort();
    files
}

fn collect_files_recursive<F>(dir: &Path, acc: &mut Vec<PathBuf>, keep_ext: F)
where
    F: Fn(&str) -> bool + Copy,
{
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if path.is_dir() {
            if !matches!(
                name_str.as_ref(),
                "node_modules"
                    | ".git"
                    | "dist"
                    | "target"
                    | ".next"
                    | "__pycache__"
                    | "venv"
                    | ".venv"
                    | "env"
                    | "build"
                    | "site-packages"
            ) {
                collect_files_recursive(&path, acc, keep_ext);
            }
        } else if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                if keep_ext(ext_str) {
                    acc.push(path);
                }
            }
        }
    }
}

/// Run `infergen scan` in the current directory.
pub fn run() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;

    // Load config; tolerate missing config by using defaults.
    let config = match Config::load_from_dir(&cwd) {
        Ok(c) => c,
        Err(infergen_core::Error::ConfigNotFound { .. }) => Config::default(),
        Err(e) => return Err(e.into()),
    };

    // Detect languages + frameworks for adapter dispatch.
    let detected = detect(&cwd).unwrap_or_default();

    let mut all_proposals = Vec::new();

    // --- JS/TS scan (Next.js adapter) ----------------------------------------
    let js_files = collect_js_files(&cwd);
    if !js_files.is_empty() {
        println!("scan: {} JS/TS files", js_files.len());
        let parser = JsParser;
        for file_path in &js_files {
            let Ok(source) = std::fs::read_to_string(file_path) else { continue };
            let Ok(parsed) = parser.parse(file_path, &source) else { continue };
            let adapter = NextjsAdapter::new(&cwd);
            all_proposals.extend(adapter.analyze(&parsed));
        }
    }

    // --- Python scan ---------------------------------------------------------
    if detected.languages.contains(&Language::Python) {
        let py_files = collect_py_files(&cwd);
        if !py_files.is_empty() {
            println!("scan: {} Python files", py_files.len());
            let py_adapter: Box<dyn Adapter> =
                if detected.frameworks.contains(&Framework::FastApi) {
                    Box::new(FastApiAdapter::new(&cwd))
                } else if detected.frameworks.contains(&Framework::Django) {
                    Box::new(DjangoAdapter::new(&cwd))
                } else if detected.frameworks.contains(&Framework::Flask) {
                    Box::new(FlaskAdapter::new(&cwd))
                } else {
                    // Generic Python: use FastAPI adapter as widest net.
                    Box::new(FastApiAdapter::new(&cwd))
                };

            let py_parser = PyParser;
            for file_path in &py_files {
                let Ok(source) = std::fs::read_to_string(file_path) else { continue };
                let Ok(parsed) = py_parser.parse(file_path, &source) else { continue };
                all_proposals.extend(py_adapter.analyze(&parsed));
            }
        }
    }

    // Apply quality-loop feedback: confidence multipliers + name hints (E6.3).
    let catalog_path = cwd.join(&config.catalog);
    let feedback = FeedbackStore::load(&quality_path(&catalog_path)).unwrap_or_default();

    for proposal in &mut all_proposals {
        if !proposal.adapter.is_empty() {
            let k = proposal_kind_str(proposal.kind);
            let m = feedback.confidence_multiplier(&proposal.adapter, &k);
            proposal.confidence = (proposal.confidence * m as f32).clamp(0.0, 1.0);
        }
    }

    for proposal in &mut all_proposals {
        if !proposal.adapter.is_empty() {
            let rel = proposal
                .source_path
                .strip_prefix(&cwd)
                .unwrap_or(&proposal.source_path)
                .to_string_lossy();
            let k = proposal_kind_str(proposal.kind);
            if let Some(name) = feedback.name_hint(&proposal.adapter, &k, &rel) {
                proposal.name = name;
            }
        }
    }

    match load_catalog(&catalog_path) {
        Ok(existing) => {
            let mut merged = rescan_merge(&existing, &all_proposals, &cwd);

            // Optional LLM refinement pass (E6.1).
            if let Some(llm_cfg) = &config.llm {
                if llm_cfg.enabled {
                    match refine_catalog_with_config(&mut merged, llm_cfg) {
                        Ok(n) if n > 0 => println!("  {n} events refined by LLM"),
                        Ok(_) => {}
                        Err(e) => eprintln!("  LLM pass skipped: {e}"),
                    }
                }
            }

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
            // No catalog yet — fresh scan.
            let mut cat = from_proposals(&all_proposals, &cwd);

            // Optional LLM refinement pass (E6.1).
            if let Some(llm_cfg) = &config.llm {
                if llm_cfg.enabled {
                    match refine_catalog_with_config(&mut cat, llm_cfg) {
                        Ok(n) if n > 0 => println!("  {n} events refined by LLM"),
                        Ok(_) => {}
                        Err(e) => eprintln!("  LLM pass skipped: {e}"),
                    }
                }
            }

            println!("  {} events proposed", cat.events.len());
            save_catalog(&cat, &catalog_path)?;
        }
        Err(e) => return Err(e.into()),
    }

    println!("catalog saved to {}", catalog_path.display());
    Ok(())
}

/// Translate a proposal [`EventKind`] to the camelCase string used in feedback entries.
fn proposal_kind_str(kind: EventKind) -> String {
    match kind {
        EventKind::PageView => "pageView",
        EventKind::ApiCall => "apiCall",
        EventKind::AuthEvent => "authEvent",
        EventKind::FormSubmit => "formSubmit",
        EventKind::ButtonClick => "buttonClick",
        EventKind::Search => "search",
        EventKind::Error => "error",
    }
    .to_owned()
}
