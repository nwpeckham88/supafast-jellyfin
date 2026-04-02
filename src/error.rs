use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid path: {0}")]
    InvalidPath(String),
}
