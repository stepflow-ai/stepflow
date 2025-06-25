use thiserror::Error;

#[derive(Error, Debug)]
pub enum BuiltinError {
    #[error("invalid input")]
    InvalidInput,
    #[error("internal error")]
    Internal,
    #[error("missing value ({0:?}) or environment variable ({1:?})")]
    MissingValueOrEnv(&'static str, &'static str),
    #[error("openai error: {0}")]
    OpenAI(String),
}

pub type Result<T, E = error_stack::Report<BuiltinError>> = std::result::Result<T, E>;
