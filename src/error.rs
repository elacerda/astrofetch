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
    /// The `--animate` intro was interrupted by Ctrl+C.
    ///
    /// The process exits with conventional interrupted semantics (exit code
    /// 130) without printing an error message.
    Interrupted,
    /// The `--animate` intro failed.
    Animation(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::System(msg) => write!(f, "failed to collect system information: {}", msg),
            AppError::Render(msg) => write!(f, "failed to render art: {}", msg),
            AppError::Cli(msg) => write!(f, "CLI error: {}", msg),
            AppError::Io(msg) => write!(f, "IO error: {}", msg),
            AppError::Interrupted => write!(f, "interrupted (Ctrl+C)"),
            AppError::Animation(msg) => write!(f, "animation failed: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}
