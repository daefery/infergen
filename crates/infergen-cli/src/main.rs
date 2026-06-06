//! Infergen CLI entrypoint.
//!
//! Subcommands (`init`, `scan`, `generate`, `check`, `watch`) arrive in epic
//! E0.2. For E0.1 this binary proves the build graph: it links the core engine,
//! parses `--version`/`--help`, and prints a scaffold banner.

use clap::Parser;

/// Infergen — scan code, infer a typed analytics catalog, generate a
/// type-safe, multi-provider SDK.
#[derive(Debug, Parser)]
#[command(name = "infergen", version, about, long_about = None)]
struct Cli {
    // Subcommands land in E0.2.
}

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    println!("infergen {}", env!("CARGO_PKG_VERSION"));
    println!("core engine {}", infergen_core::version());
    println!("catalog schema v{}", infergen_core::CATALOG_SCHEMA_VERSION);
    println!("scaffold ready — commands land in E0.2");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
