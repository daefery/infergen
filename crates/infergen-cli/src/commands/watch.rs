//! `infergen watch` — live re-scan and regenerate on source change (E4.3).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::Watcher;

use infergen_core::{
    Config, JsParser, NextjsAdapter,
    adapter::Adapter,
    catalog::{from_proposals, load_catalog, rescan_merge, save_catalog},
    codegen::{CodegenConfig, generate_typescript},
    parser::LanguageParser,
};

use crate::cli::WatchArgs;

/// Run `infergen watch`.
///
/// Performs an initial scan+generate cycle, then (unless `--once`) sets up a
/// file watcher and re-runs the cycle on every relevant source change.
///
/// Exits on `--once` after the first cycle, or on SIGINT (Ctrl+C).
pub fn run(args: WatchArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;

    println!("watch: starting — initial scan");
    run_cycle(&cwd, &args.output)?;

    if args.once {
        return Ok(());
    }

    println!("watch: watching for changes (Ctrl+C to stop)");

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::RecommendedWatcher::new(tx, notify::Config::default())?;
    watcher.watch(&cwd, notify::RecursiveMode::Recursive)?;

    let debounce = Duration::from_millis(300);
    let mut last_change: Option<Instant> = None;

    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(event)) => {
                if event.paths.iter().any(|p| is_source_file(p)) {
                    last_change = Some(Instant::now());
                }
            }
            Ok(Err(e)) => eprintln!("watch: fs error: {e}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if let Some(t) = last_change {
            if t.elapsed() >= debounce {
                last_change = None;
                println!("watch: change detected — re-scanning");
                if let Err(e) = run_cycle(&cwd, &args.output) {
                    eprintln!("watch: cycle error: {e}");
                }
            }
        }
    }

    Ok(())
}

/// Run one scan+generate cycle: scan source files, update catalog, generate SDK.
fn run_cycle(cwd: &Path, output: &Path) -> anyhow::Result<()> {
    let config = match Config::load_from_dir(cwd) {
        Ok(c) => c,
        Err(infergen_core::Error::ConfigNotFound { .. }) => Config::default(),
        Err(e) => return Err(e.into()),
    };

    // Scan source files
    let source_files = collect_source_files(cwd);
    println!("watch: {} source file(s)", source_files.len());
    let parser = JsParser;
    let mut all_proposals = Vec::new();
    for file_path in &source_files {
        let Ok(source) = std::fs::read_to_string(file_path) else { continue };
        let Ok(parsed) = parser.parse(file_path, &source) else { continue };
        let adapter = NextjsAdapter::new(cwd);
        all_proposals.extend(adapter.analyze(&parsed));
    }

    // Merge into catalog (or create fresh)
    let catalog_path = cwd.join(&config.catalog);
    let merged = match load_catalog(&catalog_path) {
        Ok(existing) => rescan_merge(&existing, &all_proposals, cwd),
        Err(infergen_core::Error::Io(_)) => from_proposals(&all_proposals, cwd),
        Err(e) => return Err(e.into()),
    };
    save_catalog(&merged, &catalog_path)?;
    println!("watch: catalog updated ({} events)", merged.events.len());

    // Generate TypeScript SDK
    let ts = generate_typescript(&merged, &CodegenConfig::default());
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, &ts)?;
    println!("watch: SDK written to {}", output.display());

    Ok(())
}

/// Return `true` for `.ts`, `.tsx`, `.js`, `.jsx` files outside ignored directories.
fn is_source_file(path: &Path) -> bool {
    let has_source_ext = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx" | "js" | "jsx")
    );
    let in_ignored = path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("node_modules" | ".git" | "dist" | "target" | ".next")
        )
    });
    has_source_ext && !in_ignored
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_source_file_tsx() {
        assert!(is_source_file(Path::new("pages/index.tsx")));
    }

    #[test]
    fn is_source_file_ts() {
        assert!(is_source_file(Path::new("src/auth.ts")));
    }

    #[test]
    fn is_source_file_js() {
        assert!(is_source_file(Path::new("lib/utils.js")));
    }

    #[test]
    fn is_source_file_jsx() {
        assert!(is_source_file(Path::new("components/Button.jsx")));
    }

    #[test]
    fn is_source_file_rejects_non_source() {
        assert!(!is_source_file(Path::new("catalog.yaml")));
        assert!(!is_source_file(Path::new("README.md")));
        assert!(!is_source_file(Path::new("package.json")));
    }

    #[test]
    fn is_source_file_rejects_node_modules() {
        assert!(!is_source_file(Path::new("node_modules/react/index.js")));
    }

    #[test]
    fn is_source_file_rejects_git() {
        assert!(!is_source_file(Path::new(".git/hooks/pre-commit.ts")));
    }

    #[test]
    fn is_source_file_rejects_target() {
        assert!(!is_source_file(Path::new("target/debug/build/foo.ts")));
    }
}
