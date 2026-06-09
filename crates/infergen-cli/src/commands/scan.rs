//! `infergen scan` — discover source files, run adapters, merge into catalog (E4.1/E5.1/E8.1).

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use infergen_core::{
    CacheEntry, Config, DjangoAdapter, EventKind, FastApiAdapter, FlaskAdapter, FeedbackStore,
    FlowDetector, JsParser, NextjsAdapter, PyParser, ScanCache,
    adapter::Adapter,
    cache::{cache_path, file_mtime, fnv1a_hash, load_cache, normalize_path, save_cache},
    catalog::{assign_flows, from_proposals, load_catalog, rescan_merge, save_catalog},
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

    // Set up incremental scan cache (E8.1).
    let catalog_path = cwd.join(&config.catalog);
    let cache_file_path = cache_path(&catalog_path);
    let mut cache = load_cache(&cache_file_path);
    let mut cache_dirty = false;

    // --- JS/TS scan (Next.js adapter, parallelised E8.1) ---------------------
    let js_files = collect_js_files(&cwd);
    if !js_files.is_empty() {
        println!("scan: {} JS/TS files", js_files.len());
        let adapter = NextjsAdapter::new(&cwd);

        // Phase 1 (sequential): check mtime → split into cache-hits and stale.
        let mut js_cached: Vec<infergen_core::adapter::ProposedEvent> = Vec::new();
        let mut js_stale: Vec<(PathBuf, String)> = Vec::new();

        for file_path in &js_files {
            let rel = normalize_path(file_path, &cwd);
            let mtime = file_mtime(file_path).unwrap_or(0);
            if let Some(entry) = cache.get(&rel) {
                if mtime == entry.modified_secs {
                    js_cached.extend(entry.proposals.iter().cloned());
                    continue;
                }
            }
            js_stale.push((file_path.clone(), rel));
        }

        // Phase 2 (parallel): re-parse stale files.
        let js_fresh: Vec<(String, u64, u64, Vec<infergen_core::adapter::ProposedEvent>)> =
            js_stale
                .par_iter()
                .map(|(file_path, rel)| {
                    let Ok(source) = std::fs::read_to_string(file_path) else {
                        return (rel.clone(), 0u64, 0u64, vec![]);
                    };
                    let content_hash = fnv1a_hash(source.as_bytes());

                    // Secondary hit: mtime changed but content identical (e.g. `touch`).
                    if let Some(cached) = cache.get(rel.as_str()) {
                        if content_hash == cached.content_hash {
                            let mtime = file_mtime(file_path).unwrap_or(0);
                            return (rel.clone(), mtime, content_hash, cached.proposals.clone());
                        }
                    }

                    // Full re-parse + analyze.
                    let Ok(parsed) = JsParser.parse(file_path, &source) else {
                        return (rel.clone(), file_mtime(file_path).unwrap_or(0), content_hash, vec![]);
                    };
                    let proposals = adapter.analyze(&parsed);
                    let mtime = file_mtime(file_path).unwrap_or(0);
                    (rel.clone(), mtime, content_hash, proposals)
                })
                .collect();

        // Phase 3 (sequential): update cache, merge proposals.
        for (rel, mtime, content_hash, proposals) in js_fresh {
            if content_hash != 0 {
                cache.insert(
                    rel,
                    CacheEntry {
                        modified_secs: mtime,
                        content_hash,
                        proposals: proposals.clone(),
                    },
                );
                cache_dirty = true;
            }
            all_proposals.extend(proposals);
        }
        all_proposals.extend(js_cached);
    }

    // --- Python scan (parallelised E8.1) -------------------------------------
    if detected.languages.contains(&Language::Python) {
        let py_files = collect_py_files(&cwd);
        if !py_files.is_empty() {
            println!("scan: {} Python files", py_files.len());

            // Resolve which adapter to use; each rayon thread creates its own instance.
            let py_framework = if detected.frameworks.contains(&Framework::FastApi) {
                Framework::FastApi
            } else if detected.frameworks.contains(&Framework::Django) {
                Framework::Django
            } else if detected.frameworks.contains(&Framework::Flask) {
                Framework::Flask
            } else {
                Framework::FastApi
            };

            // Phase 1: mtime cache check.
            let mut py_cached: Vec<infergen_core::adapter::ProposedEvent> = Vec::new();
            let mut py_stale: Vec<(PathBuf, String)> = Vec::new();

            for file_path in &py_files {
                let rel = normalize_path(file_path, &cwd);
                let mtime = file_mtime(file_path).unwrap_or(0);
                if let Some(entry) = cache.get(&rel) {
                    if mtime == entry.modified_secs {
                        py_cached.extend(entry.proposals.iter().cloned());
                        continue;
                    }
                }
                py_stale.push((file_path.clone(), rel));
            }

            // Phase 2: parallel re-parse. Each thread creates its own adapter
            // instance (cheap — just a PathBuf clone).
            let py_fresh: Vec<(String, u64, u64, Vec<infergen_core::adapter::ProposedEvent>)> =
                py_stale
                    .par_iter()
                    .map(|(file_path, rel)| {
                        let Ok(source) = std::fs::read_to_string(file_path) else {
                            return (rel.clone(), 0u64, 0u64, vec![]);
                        };
                        let content_hash = fnv1a_hash(source.as_bytes());

                        if let Some(cached) = cache.get(rel.as_str()) {
                            if content_hash == cached.content_hash {
                                let mtime = file_mtime(file_path).unwrap_or(0);
                                return (rel.clone(), mtime, content_hash, cached.proposals.clone());
                            }
                        }

                        let py_adapter: Box<dyn Adapter> = match py_framework {
                            Framework::FastApi => Box::new(FastApiAdapter::new(&cwd)),
                            Framework::Django => Box::new(DjangoAdapter::new(&cwd)),
                            Framework::Flask => Box::new(FlaskAdapter::new(&cwd)),
                            _ => Box::new(FastApiAdapter::new(&cwd)),
                        };
                        let Ok(parsed) = PyParser.parse(file_path, &source) else {
                            return (
                                rel.clone(),
                                file_mtime(file_path).unwrap_or(0),
                                content_hash,
                                vec![],
                            );
                        };
                        let proposals = py_adapter.analyze(&parsed);
                        let mtime = file_mtime(file_path).unwrap_or(0);
                        (rel.clone(), mtime, content_hash, proposals)
                    })
                    .collect();

            // Phase 3: update cache, merge proposals.
            for (rel, mtime, content_hash, proposals) in py_fresh {
                if content_hash != 0 {
                    cache.insert(
                        rel,
                        CacheEntry {
                            modified_secs: mtime,
                            content_hash,
                            proposals: proposals.clone(),
                        },
                    );
                    cache_dirty = true;
                }
                all_proposals.extend(proposals);
            }
            all_proposals.extend(py_cached);
        }
    }

    // Persist updated cache entries (E8.1).
    if cache_dirty {
        if let Err(e) = save_cache(&cache, &cache_file_path) {
            eprintln!("scan: warning — could not save cache: {e}");
        }
    }

    // Apply quality-loop feedback: confidence multipliers + name hints (E6.3).
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

    // E6.2 — detect multi-step funnels across quality-adjusted proposals.
    let flow_detector = FlowDetector::new();
    let detected_flows = flow_detector.detect(&all_proposals);
    if !detected_flows.is_empty() {
        println!("scan: {} flow(s) detected", detected_flows.len());
        for f in &detected_flows {
            println!(
                "  [{kind}] {name} ({steps} steps, {conf:.0}% confidence)",
                kind = format!("{:?}", f.kind).to_lowercase(),
                name = f.name,
                steps = f.steps.len(),
                conf = f64::from(f.confidence) * 100.0,
            );
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
            if !detected_flows.is_empty() {
                let mut cat = load_catalog(&catalog_path)?;
                assign_flows(&mut cat, &detected_flows, &all_proposals, &cwd);
                save_catalog(&cat, &catalog_path)?;
            }
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
            if !detected_flows.is_empty() {
                let mut cat = load_catalog(&catalog_path)?;
                assign_flows(&mut cat, &detected_flows, &all_proposals, &cwd);
                save_catalog(&cat, &catalog_path)?;
            }
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
