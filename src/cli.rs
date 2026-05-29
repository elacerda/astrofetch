use clap::Parser;

/// Opções de modelo de arte ASCII.
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ArtModel {
    /// Escolhe aleatoriamente entre os modelos disponíveis.
    Random,
    /// Galáxia elíptica.
    Elliptical,
    /// Galáxia espiral.
    Spiral,
    /// Aglomerado estelar.
    Cluster,
    /// Campo de estrelas simples.
    Starfield,
}

/// Argumentos de linha de comando do AstroFetch.
#[derive(Debug, Clone, Parser)]
#[command(name = "astrofetch")]
#[command(about = "Um app de terminal que mostra informações do sistema ao lado de arte ASCII astrofísica", long_about = None)]
pub struct Args {
    /// Modelo de arte ASCII (random, elliptical, spiral, cluster, starfield)
    #[arg(short, long, default_value = "random")]
    pub model: ArtModel,

    /// Largura da arte ASCII (padrão: 40)
    #[arg(short, long, default_value = "40")]
    pub width: usize,

    /// Altura da arte ASCII (padrão: 20)
    #[arg(long, default_value = "20")]
    pub height: usize,

    /// Seed para geração determinística
    #[arg(short, long)]
    pub seed: Option<u64>,

    /// Desativa cores ANSI
    #[arg(long)]
    pub no_color: bool,

    /// Imprime apenas a arte ASCII
    #[arg(long)]
    pub logo_only: bool,

    /// Imprime apenas informações do sistema
    #[arg(long)]
    pub info_only: bool,

    /// Modo compacto (menos campos)
    #[arg(long)]
    pub compact: bool,
}
