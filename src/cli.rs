use clap::Parser;

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
}
