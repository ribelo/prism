use axum::{
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use tokio::sync::Mutex;
use futures_util::FutureExt;
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use crate::{config::Config, error::Result, auth::anthropic::AnthropicOAuth};

pub mod routes;

// Global timestamp for background task monitoring
static LAST_TOKEN_CHECK: AtomicU64 = AtomicU64::new(0);

pub struct SetuServer {
    config: Config,
}

impl SetuServer {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn start(&self) -> Result<()> {
        let config_state = Arc::new(Mutex::new(self.config.clone()));
        
        let app = Router::new()
            // OpenAI-compatible routes
            .route("/v1/chat/completions", post(routes::openai_chat_completions))
            .route("/v1/models", get(routes::openai_models))

            // Anthropic-compatible routes
            .route("/v1/messages", post(routes::anthropic_messages))

            // Health check
            .route("/health", get(health_check))

            // Add shared config state
            .with_state(config_state.clone())

            // CORS and tracing middleware
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http());

        let addr = format!("{}:{}", self.config.server.host, self.config.server.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("Setu server starting on http://{}", addr);

        // Spawn background token maintenance task with panic recovery
        tokio::spawn({
            let config = config_state;
            async move {
                loop {
                    let result = std::panic::AssertUnwindSafe(background_token_maintenance(config.clone()))
                        .catch_unwind()
                        .await;
                    
                    match result {
                        Ok(()) => {
                            tracing::error!("Background token maintenance task ended unexpectedly, restarting");
                        }
                        Err(panic) => {
                            tracing::error!("Background token maintenance task panicked: {:?}, restarting", panic);
                        }
                    }
                    
                    // Wait a bit before restarting to avoid rapid restart loops
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                }
            }
        });

        axum::serve(listener, app).await?;

        Ok(())
    }
}

async fn health_check() -> Json<Value> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    let last_check = LAST_TOKEN_CHECK.load(Ordering::Relaxed);
    let token_task_healthy = last_check > 0 && (now - last_check) < 600; // Healthy if checked within 10 minutes
    
    Json(json!({
        "status": if token_task_healthy { "healthy" } else { "degraded" },
        "service": "setu", 
        "version": env!("CARGO_PKG_VERSION"),
        "background_token_task": {
            "healthy": token_task_healthy,
            "last_check": last_check,
            "seconds_since_last_check": if last_check > 0 { now - last_check } else { 0 }
        }
    }))
}

async fn background_token_maintenance(config: Arc<Mutex<Config>>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // 5 minutes
    
    loop {
        interval.tick().await;
        
        // Update monitoring timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        LAST_TOKEN_CHECK.store(now, Ordering::Relaxed);
        
        let mut config_guard = config.lock().await;
        
        // Check if we have an anthropic provider that needs refresh
        let needs_refresh = if let Some(provider) = config_guard.providers.get("anthropic") {
            provider.auth.oauth_refresh_token.is_some() && provider.auth.needs_refresh()
        } else {
            false
        };
        
        if needs_refresh {
            tracing::info!("Background token refresh: Token expires soon, attempting refresh");
            
            // Get mutable reference to auth config
            if let Some(provider) = config_guard.providers.get_mut("anthropic") {
                match AnthropicOAuth::refresh_token(&mut provider.auth).await {
                    Ok(()) => {
                        tracing::info!("Background token refresh: Successfully refreshed OAuth tokens");
                        
                        // Clone config for I/O operation and release lock BEFORE file save
                        let config_to_save = config_guard.clone();
                        drop(config_guard);  // Explicitly release lock
                        
                        // Persist refreshed tokens to config file WITHOUT holding lock
                        if let Err(e) = config_to_save.save() {
                            tracing::error!("Failed to save refreshed tokens to config: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Background token refresh failed: {}", e);
                        tracing::warn!("Tokens may need manual refresh via 'setu auth anthropic'");
                    }
                }
            }
        }
    }
}
