//! Infergen CLI entrypoint.

mod cli;
mod commands;

use clap::Parser;

use cli::Cli;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(command) => commands::run(command),
        None => {
            print_banner();
            Ok(())
        }
    }
}

/// Print the no-subcommand status banner.
fn print_banner() {
    println!("infergen {}", env!("CARGO_PKG_VERSION"));
    println!("core engine {}", infergen_core::version());
    println!("catalog schema v{}", infergen_core::CATALOG_SCHEMA_VERSION);
    println!("config schema v{}", infergen_core::CONFIG_SCHEMA_VERSION);
    println!("run `infergen --help` to see commands");
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
