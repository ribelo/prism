use thiserror::Error;

pub type Result<T> = std::result::Result<T, PrismError>;

#[derive(Error, Debug)]
pub enum PrismError {
    #[error("Configuration error: {0}")]
    Config(#[from] Box<figment::Error>),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP server error: {0}")]
    Http(#[from] axum::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP request error: {0}")]
    Request(#[from] reqwest::Error),

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
