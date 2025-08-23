use thiserror::Error;

pub type Result<T> = std::result::Result<T, SetuError>;

#[derive(Error, Debug)]
pub enum SetuError {
    #[error("Configuration error: {0}")]
    Config(#[from] figment::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP server error: {0}")]
    Http(#[from] axum::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("AI provider error: {0}")]
    Provider(String),

    #[error("Invalid model format: {0}")]
    InvalidModel(String),

    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Request parsing error: {0}")]
    RequestParsing(String),

    #[error("Translation error: {0}")]
    Translation(String),

    #[error("Other error: {0}")]
    Other(String),
}