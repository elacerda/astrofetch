use std::fmt;

/// Erros recuperáveis do AstroFetch.
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    /// Erro ao coletar informações do sistema
    System(String),
    /// Erro ao gerar arte ASCII
    Render(String),
    /// Erro de CLI (argumentos inválidos, etc.)
    Cli(String),
    /// Erro de IO
    Io(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::System(msg) => write!(f, "Erro ao coletar informações do sistema: {}", msg),
            AppError::Render(msg) => write!(f, "Erro ao renderizar arte: {}", msg),
            AppError::Cli(msg) => write!(f, "Erro de CLI: {}", msg),
            AppError::Io(msg) => write!(f, "Erro de IO: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}
