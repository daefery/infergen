//! Subcommand dispatch and shared helpers.

pub mod init;
pub mod review;
pub mod stubs;

use crate::cli::Commands;

/// Run the selected subcommand.
pub fn run(command: Commands) -> anyhow::Result<()> {
    match command {
        Commands::Init(args) => init::run(args),
        Commands::Scan => stubs::not_implemented("scan", "E0.4"),
        Commands::Generate => stubs::not_implemented("generate", "E2.1"),
        Commands::Check => stubs::not_implemented("check", "E4.2"),
        Commands::Watch => stubs::not_implemented("watch", "E4.3"),
        Commands::Review(args) => review::run(args),
    }
}
