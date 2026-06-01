use crate::setup_shell::SetupShell;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Opções de modelo de arte ASCII.
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ArtModel {
    /// Choose randomly from the available models.
    Random,
    /// Elliptical galaxy.
    Elliptical,
    /// Spiral galaxy.
    Spiral,
    /// Stellar cluster.
    Cluster,
    /// Simple starfield.
    Starfield,
}

/// Argumentos de linha de comando do AstroFetch.
#[derive(Debug, Clone, Parser)]
#[command(name = "astrofetch")]
#[command(
    version,
    about = "Show system info beside procedural astrophysical ASCII art",
    long_about = "AstroFetch is a small screenFetch-inspired terminal app that replaces a static distro logo with procedural galaxies, clusters, and starfields."
)]
pub struct Args {
    /// Optional command to run instead of the default fetch display.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// ASCII art model to render
    #[arg(short, long, default_value = "random")]
    pub model: ArtModel,

    /// Width of the ASCII art area
    #[arg(short, long, default_value = "40")]
    pub width: usize,

    /// Height of the ASCII art area
    #[arg(long, default_value = "20")]
    pub height: usize,

    /// Seed for deterministic art generation
    #[arg(short, long)]
    pub seed: Option<u64>,

    /// Disable ANSI color output
    #[arg(long)]
    pub no_color: bool,

    /// Print only the ASCII art
    #[arg(long)]
    pub logo_only: bool,

    /// Print only system information
    #[arg(long)]
    pub info_only: bool,

    /// Print the compact field set
    #[arg(long)]
    pub compact: bool,

    /// Show per-filesystem disk usage details
    #[arg(long)]
    pub disk_details: bool,
}

/// Subcomandos explícitos do AstroFetch.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Safely add AstroFetch to a shell startup file.
    SetupShell(SetupShellArgs),
    /// Safely remove AstroFetch from a shell startup file.
    UninstallShell(UninstallShellArgs),
}

/// Argumentos para integração explícita com arquivos de inicialização de shell.
#[derive(Debug, Clone, clap::Args)]
pub struct SetupShellArgs {
    /// Shell startup file to update.
    #[arg(long, value_enum)]
    pub shell: Option<SetupShell>,

    /// Use `astrofetch --compact` in the managed startup block.
    #[arg(long)]
    pub compact: bool,

    /// Print the target and managed block without writing any files.
    #[arg(long)]
    pub dry_run: bool,

    /// Replace an existing managed AstroFetch block.
    #[arg(long)]
    pub force: bool,

    /// Advanced override for testing or manual setup against a specific file.
    #[arg(long)]
    pub target_path: Option<PathBuf>,
}

/// Arguments for explicitly removing shell startup integration.
#[derive(Debug, Clone, clap::Args)]
pub struct UninstallShellArgs {
    /// Shell startup file to update.
    #[arg(long, value_enum)]
    pub shell: Option<SetupShell>,

    /// Print what would be removed without writing any files.
    #[arg(long)]
    pub dry_run: bool,

    /// Advanced override for testing or manual setup against a specific file.
    #[arg(long)]
    pub target_path: Option<PathBuf>,
}
