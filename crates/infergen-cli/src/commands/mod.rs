//! Subcommand dispatch and shared helpers.

pub mod check;
pub mod generate;
pub mod init;
pub mod manifest;
pub mod review;
pub mod scan;
pub mod watch;

use crate::cli::Commands;

/// Run the selected subcommand.
pub fn run(command: Commands) -> anyhow::Result<()> {
    match command {
        Commands::Init(args) => init::run(args),
        Commands::Scan => scan::run(),
        Commands::Generate(args) => generate::run(args),
        Commands::Check(args) => check::run(args),
        Commands::Watch(args) => watch::run(args),
        Commands::Review(args) => review::run(args),
        Commands::Manifest(args) => manifest::run(args),
    }
}
