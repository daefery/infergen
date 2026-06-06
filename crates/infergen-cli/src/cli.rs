//! Command-line interface definition for the `infergen` binary.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Infergen — scan code, infer a typed analytics catalog, generate a
/// type-safe, multi-provider SDK.
#[derive(Debug, Parser)]
#[command(name = "infergen", version, about, long_about = None)]
pub struct Cli {
    /// Subcommand to run. When omitted, prints a status banner.
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Detect languages/frameworks and write an `infergen.config.*`.
    Init(InitArgs),
    /// Scan source and propose an event catalog. (Lands in E0.4.)
    Scan,
    /// Generate a typed SDK from the catalog. (Lands in E2.1.)
    Generate,
    /// CI check: fail on drift / untracked moments. (Lands in E4.2.)
    Check,
    /// Watch files and re-scan on change. (Lands in E4.3.)
    Watch,
}

/// Arguments for `infergen init`.
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Project directory to initialize (default: current directory).
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,
    /// Config file format to write.
    #[arg(long, value_enum, default_value_t = InitFormat::Json)]
    pub format: InitFormat,
    /// Overwrite an existing config file.
    #[arg(long)]
    pub force: bool,
}

/// On-disk format selectable by `init --format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InitFormat {
    /// JSON (`infergen.config.json`).
    Json,
    /// TOML (`infergen.config.toml`).
    Toml,
}
