//! `infergen manifest` — data-collection manifest export (E7.4).

use anyhow::Context;
use infergen_core::{catalog::load_catalog, generate_manifest, render_markdown, Config};

use crate::cli::{ManifestArgs, ManifestFormat};

/// Run `infergen manifest`.
///
/// Loads the catalog, builds a [`infergen_core::Manifest`], and writes it to
/// stdout or `--output` in the requested format (JSON, YAML, or Markdown).
pub fn run(args: ManifestArgs) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;

    let config = match Config::load_from_dir(&cwd) {
        Ok(c) => c,
        Err(infergen_core::Error::ConfigNotFound { .. }) => Config::default(),
        Err(e) => return Err(e.into()),
    };

    let catalog_path = args.catalog.unwrap_or_else(|| cwd.join(&config.catalog));

    let catalog = load_catalog(&catalog_path).with_context(|| {
        format!(
            "manifest: catalog not found at {} — run `infergen scan` first",
            catalog_path.display()
        )
    })?;

    // Inject a unix-seconds timestamp from the CLI layer so the core function
    // stays deterministic in tests (no SystemTime calls in core).
    let generated_at = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Some(format!("unix:{secs}"))
    };

    let manifest = generate_manifest(&catalog, generated_at);

    let content = match args.format {
        ManifestFormat::Json => {
            let mut s = serde_json::to_string_pretty(&manifest)?;
            s.push('\n');
            s
        }
        ManifestFormat::Yaml => serde_yaml::to_string(&manifest)?,
        ManifestFormat::Markdown => render_markdown(&manifest),
    };

    match args.output {
        Some(ref path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            std::fs::write(path, &content)?;
            eprintln!(
                "manifest: written {} bytes to {}",
                content.len(),
                path.display()
            );
        }
        None => print!("{content}"),
    }

    Ok(())
}
