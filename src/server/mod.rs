use axum::{
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use crate::{config::Config, error::Result};

pub mod routes;

pub struct SetuServer {
    config: Config,
}

impl SetuServer {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn start(&self) -> Result<()> {
        let config_state = Arc::new(self.config.clone());
        
        let app = Router::new()
            // OpenAI-compatible routes
            .route("/v1/chat/completions", post(routes::openai_chat_completions))
            .route("/v1/models", get(routes::openai_models))

            // Anthropic-compatible routes
            .route("/v1/messages", post(routes::anthropic_messages))

            // Health check
            .route("/health", get(health_check))

            // Add shared config state
            .with_state(config_state)

            // CORS and tracing middleware
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http());

        let addr = format!("{}:{}", self.config.server.host, self.config.server.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("Setu server starting on http://{}", addr);

        axum::serve(listener, app).await?;

        Ok(())
    }
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "setu",
        "version": env!("CARGO_PKG_VERSION")
    }))
}
