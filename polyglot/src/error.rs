use thiserror::Error;

#[derive(Error, Debug)]
pub enum PolyglotError {
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("Execution timeout after {0}s")]
    Timeout(u64),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Compilation failed: {0}")]
    CompilationFailed(String),

    #[error("Output exceeds limit of {0} bytes")]
    OutputLimitExceeded(usize),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type PolyglotResult<T> = Result<T, PolyglotError>;
