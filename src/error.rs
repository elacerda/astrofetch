use std::fmt;

/// Recoverable AstroFetch errors.
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    /// Failed to collect system information.
    System(String),
    /// Failed to generate ASCII art.
    Render(String),
    /// CLI error (invalid arguments, etc.).
    Cli(String),
    /// IO error.
    Io(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::System(msg) => write!(f, "failed to collect system information: {}", msg),
            AppError::Render(msg) => write!(f, "failed to render art: {}", msg),
            AppError::Cli(msg) => write!(f, "CLI error: {}", msg),
            AppError::Io(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}
