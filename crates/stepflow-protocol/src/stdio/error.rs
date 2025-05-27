use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum StdioError {
    #[error("error spawning stepflow component process")]
    Spawn,
    #[error("error sending message")]
    Send,
    #[error("error receiving message")]
    Recv,
    #[error("received invalid message")]
    InvalidMessage,
    #[error("received invalid response")]
    InvalidResponse,
    #[error("components server error({code}): {message}")]
    ServerError {
        code: i64,
        message: String,
        data: Option<serde_json::Value>,
    },
    #[error("components server failed with exit code {exit_code:?}")]
    ServerFailure { exit_code: Option<i32> },
    #[error("error closing stepflow component process")]
    Close,
    #[error("invalid command: {}", .0.display())]
    InvalidCommand(PathBuf),
    #[error("error in receive loop")]
    RecvLoop,
    #[error("command not found: {0}")]
    MissingCommand(String),
}

pub type Result<T, E = error_stack::Report<StdioError>> = std::result::Result<T, E>;
