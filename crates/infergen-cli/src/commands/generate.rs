//! `infergen generate` — TypeScript + Python SDK generation from approved catalog (E2.1/E5.1).

use anyhow::Context;
use infergen_core::{
    Catalog, CodegenConfig, EventStatus, generate_python, generate_typescript, load_catalog,
    detect::{Language, detect},
};

use crate::cli::GenerateArgs;

/// Run `infergen generate`.
pub fn run(args: GenerateArgs) -> anyhow::Result<()> {
    let catalog = if args.catalog.exists() {
        load_catalog(&args.catalog)
            .with_context(|| format!("reading catalog {}", args.catalog.display()))?
    } else {
        Catalog::default()
    };

    let config = CodegenConfig { include_proposed: args.include_proposed };
    let ts = generate_typescript(&catalog, &config);

    if args.check {
        let on_disk = if args.output.exists() {
            std::fs::read_to_string(&args.output)
                .with_context(|| format!("reading {}", args.output.display()))?
        } else {
            String::new()
        };
        if ts == on_disk {
            println!("infergen: {} is up to date", args.output.display());
            return Ok(());
        } else {
            anyhow::bail!(
                "infergen: {} is stale — run `infergen generate` to regenerate",
                args.output.display()
            );
        }
    }

    let generated_count = catalog
        .events
        .iter()
        .filter(|e| {
            e.status == EventStatus::Approved
                || (args.include_proposed && e.status == EventStatus::Proposed)
        })
        .count();

    if let Some(parent) = args.output.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating output directory {}", parent.display()))?;
    }

    std::fs::write(&args.output, &ts)
        .with_context(|| format!("writing {}", args.output.display()))?;

    println!("infergen: wrote {}  ({} events)", args.output.display(), generated_count);

    // Also generate Python SDK when Python is detected in cwd.
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(detected) = detect(&cwd) {
            if detected.languages.contains(&Language::Python) {
                let py_src = generate_python(&catalog, &config);
                let py_output = cwd.join("infergen_sdk.py");
                if let Err(e) = std::fs::write(&py_output, &py_src) {
                    eprintln!("infergen: warning: could not write Python SDK: {e}");
                } else {
                    println!("infergen: wrote {}  ({} events)", py_output.display(), generated_count);
                }
            }
        }
    }

    Ok(())
}
