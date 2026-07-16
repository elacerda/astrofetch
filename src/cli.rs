use crate::setup_shell::SetupShell;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Validates width (1..=200)
fn validate_width(s: &str) -> Result<usize, String> {
    let val: usize = s
        .parse()
        .map_err(|_| format!("'{s}' is not a valid usize"))?;
    if !(1..=200).contains(&val) {
        return Err("width must be between 1 and 200".to_string());
    }
    Ok(val)
}

/// Validates height (1..=100)
fn validate_height(s: &str) -> Result<usize, String> {
    let val: usize = s
        .parse()
        .map_err(|_| format!("'{s}' is not a valid usize"))?;
    if !(1..=100).contains(&val) {
        return Err("height must be between 1 and 100".to_string());
    }
    Ok(val)
}

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

/// Layout choice for combining art and information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LayoutChoice {
    Auto,
    SideBySide,
    Stacked,
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
    #[arg(short, long, value_parser = validate_width)]
    pub width: Option<usize>,

    /// Height of the ASCII art area
    #[arg(long, value_parser = validate_height)]
    pub height: Option<usize>,

    /// Seed for deterministic art generation
    #[arg(short, long)]
    pub seed: Option<u64>,

    /// Disable ANSI color output
    #[arg(long)]
    pub no_color: bool,

    /// Print only the ASCII art
    #[arg(long)]
    #[clap(conflicts_with = "info_only")]
    pub logo_only: bool,

    /// Print only system information
    #[arg(long)]
    #[clap(conflicts_with = "logo_only")]
    pub info_only: bool,

    /// Print the compact field set
    #[arg(long)]
    pub compact: bool,

    /// Show per-filesystem disk usage details
    #[arg(long)]
    pub disk_details: bool,

    /// Layout for combining art and information
    #[arg(long, default_value = "auto", value_enum)]
    pub layout: LayoutChoice,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_default_values() {
        let args = Args::try_parse_from(["astrofetch"]).unwrap();
        assert_eq!(args.width, None);
        assert_eq!(args.height, None);
        assert_eq!(args.layout, LayoutChoice::Auto);
    }

    #[test]
    fn test_args_explicit_dimensions() {
        let args = Args::try_parse_from(["astrofetch", "--width", "60", "--height", "30"]).unwrap();
        assert_eq!(args.width, Some(60));
        assert_eq!(args.height, Some(30));
    }

    #[test]
    fn test_args_layout_auto() {
        let args = Args::try_parse_from(["astrofetch", "--layout", "auto"]).unwrap();
        assert_eq!(args.layout, LayoutChoice::Auto);
    }

    #[test]
    fn test_args_layout_side_by_side() {
        let args = Args::try_parse_from(["astrofetch", "--layout", "side-by-side"]).unwrap();
        assert_eq!(args.layout, LayoutChoice::SideBySide);
    }

    #[test]
    fn test_args_layout_stacked() {
        let args = Args::try_parse_from(["astrofetch", "--layout", "stacked"]).unwrap();
        assert_eq!(args.layout, LayoutChoice::Stacked);
    }

    #[test]
    fn test_args_zero_width() {
        let result = Args::try_parse_from(["astrofetch", "--width", "0"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_args_zero_height() {
        let result = Args::try_parse_from(["astrofetch", "--height", "0"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_args_width_above_200() {
        let result = Args::try_parse_from(["astrofetch", "--width", "201"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_args_height_above_100() {
        let result = Args::try_parse_from(["astrofetch", "--height", "101"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_args_logo_only_and_info_only_conflict() {
        let result = Args::try_parse_from(["astrofetch", "--logo-only", "--info-only"]);
        assert!(result.is_err());
    }
}
