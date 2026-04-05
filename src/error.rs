use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("invalid config value for {key}: {value}")]
    InvalidConfigValue { key: &'static str, value: String },
    #[error("invalid boolean config value for {key}: {value}")]
    InvalidBooleanConfigValue { key: &'static str, value: String },
    #[error("startup validation failed for {key}: {message}")]
    StartupValidation {
        key: &'static str,
        message: String,
    },
}
