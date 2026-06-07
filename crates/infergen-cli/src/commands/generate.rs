//! `infergen generate` — TypeScript SDK generation from approved catalog (E2.1 + E2.2).

use anyhow::Context;
use infergen_core::{
    Catalog, CodegenConfig, EventStatus, generate_typescript, load_catalog,
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
    Ok(())
}
