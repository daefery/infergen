//! Command-line interface definition for the `infergen` binary.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

// Default catalog path used by review sub-commands.
pub const DEFAULT_CATALOG: &str = ".infergen/catalog.yaml";

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
    /// Generate a TypeScript SDK from the approved catalog.
    Generate(GenerateArgs),
    /// Generate a SQL schema (CREATE TABLE + typed views) from the approved catalog.
    Schema(SchemaArgs),
    /// CI check: fail on drift / untracked moments. (Lands in E4.2.)
    Check,
    /// Watch files and re-scan on change. (Lands in E4.3.)
    Watch,
    /// Review and annotate the event catalog.
    Review(ReviewArgs),
}

/// Arguments for `infergen generate`.
#[derive(Debug, Args)]
pub struct GenerateArgs {
    /// Path to the catalog file.
    #[arg(long, default_value = DEFAULT_CATALOG)]
    pub catalog: PathBuf,
    /// Output TypeScript file path.
    #[arg(long, default_value = "infergen.generated.ts")]
    pub output: PathBuf,
    /// Also generate code for Proposed events (in addition to Approved).
    #[arg(long)]
    pub include_proposed: bool,
    /// Check whether the output file is up to date; exit non-zero if stale. Does not write.
    #[arg(long)]
    pub check: bool,
}

/// Arguments for `infergen schema`.
#[derive(Debug, Args)]
pub struct SchemaArgs {
    /// SQL dialect to generate.
    #[arg(long, value_enum, default_value_t = SchemaDialect::Postgres)]
    pub dialect: SchemaDialect,
    /// Path to the catalog file.
    #[arg(long, default_value = DEFAULT_CATALOG)]
    pub catalog: PathBuf,
    /// Output file path. Prints to stdout when omitted.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// SQL dialect for `infergen schema`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SchemaDialect {
    /// PostgreSQL — JSONB, BIGSERIAL, TIMESTAMPTZ.
    Postgres,
    /// MySQL / MariaDB — JSON, BIGINT UNSIGNED AUTO_INCREMENT, DATETIME.
    Mysql,
    /// SQLite — TEXT properties column, INTEGER AUTOINCREMENT.
    Sqlite,
}

/// Arguments for `infergen review`.
#[derive(Debug, Args)]
pub struct ReviewArgs {
    /// Sub-command to run.
    #[command(subcommand)]
    pub action: ReviewAction,
    /// Path to the catalog file.
    #[arg(long, global = true, default_value = DEFAULT_CATALOG)]
    pub catalog: PathBuf,
}

/// `infergen review` sub-commands.
#[derive(Debug, Subcommand)]
pub enum ReviewAction {
    /// List catalog events (filter by --status).
    List {
        /// Show only events with this status: `proposed`, `approved`, `ignored`, or `all`.
        #[arg(long, default_value = "all")]
        status: String,
    },
    /// Approve an event by stable ID.
    Approve {
        /// Stable event ID (e.g. `evt_0123456789abcdef`).
        id: String,
    },
    /// Ignore an event by stable ID (mark as false positive).
    Ignore {
        /// Stable event ID.
        id: String,
    },
    /// Rename an event.
    Rename {
        /// Stable event ID.
        id: String,
        /// New event name.
        new_name: String,
    },
    /// Set the human-readable description of an event.
    Describe {
        /// Stable event ID.
        id: String,
        /// Description text.
        description: String,
    },
    /// Show the diff between a proposed catalog and the existing catalog.
    Diff {
        /// Path to the proposed catalog (output of `infergen scan`).
        proposed: PathBuf,
    },
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
