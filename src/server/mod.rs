use axum::{
    Router,
    response::Json,
    routing::{get, post},
};
use futures_util::FutureExt;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::SystemTime;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use crate::{
    auth::{AuthCache, anthropic::AnthropicOAuth},
    config::Config,
    error::Result,
};

pub mod error_handling;
pub mod parameter_mapping;
pub mod providers;
pub mod routes;

// Global timestamp for background task monitoring
static LAST_TOKEN_CHECK: AtomicU64 = AtomicU64::new(0);

/// Shared application state containing configuration and cached authentication
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Mutex<Config>>,
    pub auth_cache: Arc<AuthCache>,
    pub last_config_check: Arc<AtomicU64>,
    pub config_path: std::path::PathBuf,
}

pub struct SetuServer {
    config: Config,
    auth_cache: AuthCache,
}

impl SetuServer {
    pub fn new(config: Config, auth_cache: AuthCache) -> Self {
        Self { config, auth_cache }
    }

    pub async fn start(&self) -> Result<()> {
        let config_path = Config::config_dir()?.join("setu.toml");
        let app_state = AppState {
            config: Arc::new(Mutex::new(self.config.clone())),
            auth_cache: Arc::new(self.auth_cache.clone()),
            last_config_check: Arc::new(AtomicU64::new(0)),
            config_path,
        };

        let app = Router::new()
            // OpenAI-compatible routes
            .route(
                "/v1/chat/completions",
                post(routes::openai_chat_completions),
            )
            .route("/v1/models", get(routes::openai_models))
            // Anthropic-compatible routes
            .route("/v1/messages", post(routes::anthropic_messages))
            // Gemini-compatible routes
            .route(
                "/v1beta/models/{*model_path}",
                post(routes::gemini_generate_content),
            )
            // Health check
            .route("/health", get(health_check))
            // Add shared application state
            .with_state(app_state.clone())
            // CORS and tracing middleware
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http());

        let addr = format!("{}:{}", self.config.server.host, self.config.server.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("Setu server starting on http://{}", addr);

        // Spawn background token maintenance task with panic recovery
        tokio::spawn({
            let config = app_state.config.clone();
            async move {
                loop {
                    let result =
                        std::panic::AssertUnwindSafe(background_token_maintenance(config.clone()))
                            .catch_unwind()
                            .await;

                    match result {
                        Ok(()) => {
                            tracing::error!(
                                "Background token maintenance task ended unexpectedly, restarting"
                            );
                        }
                        Err(panic) => {
                            tracing::error!(
                                "Background token maintenance task panicked: {:?}, restarting",
                                panic
                            );
                        }
                    }

                    // Wait a bit before restarting to avoid rapid restart loops
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                }
            }
        });

        // Spawn SIGHUP handler for manual config reload
        #[cfg(unix)]
        {
            let sighup_config = app_state.config.clone();
            tokio::spawn(async move {
                handle_sighup(sighup_config).await;
            });
        }

        // Graceful shutdown handling
        let graceful = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal());

        graceful.await?;

        // Save any pending OAuth token refreshes
        let config_guard = app_state.config.lock().await;
        if let Err(e) = config_guard.save() {
            tracing::warn!("Failed to save config during shutdown: {}", e);
        }

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

/// Check if config file has changed and reload if needed
async fn check_and_reload_config(app_state: &AppState) {
    // Only check every 5 seconds to avoid too frequent checks
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let last_check = app_state.last_config_check.load(Ordering::Relaxed);
    if now - last_check < 5 {
        return;
    }

    app_state.last_config_check.store(now, Ordering::Relaxed);

    // Check if file has been modified
    if let Ok(metadata) = std::fs::metadata(&app_state.config_path)
        && let Ok(modified) = metadata.modified()
    {
        let modified_timestamp = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // If file was modified after our last check
        if modified_timestamp > last_check {
            // Try to reload config
            match Config::load() {
                Ok(new_config) => {
                    let mut config_guard = app_state.config.lock().await;
                    *config_guard = new_config;
                    tracing::info!("Config automatically reloaded due to file change");
                }
                Err(e) => {
                    tracing::error!("Failed to reload config: {}", e);
                }
            }
        }
    }
}

#[cfg(unix)]
async fn handle_sighup(config: Arc<Mutex<Config>>) {
    use tokio::signal::unix::{SignalKind, signal};

    let mut stream = match signal(SignalKind::hangup()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to install SIGHUP handler: {}", e);
            return;
        }
    };

    tracing::info!("SIGHUP handler installed - send SIGHUP to reload config");

    loop {
        stream.recv().await;
        tracing::info!("Received SIGHUP signal, reloading config...");

        // Reload config directly
        match Config::load() {
            Ok(new_config) => {
                let mut config_guard = config.lock().await;
                *config_guard = new_config;
                tracing::info!("Config reloaded via SIGHUP signal");
            }
            Err(e) => {
                tracing::error!("Failed to reload config on SIGHUP: {}", e);
            }
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down gracefully");
        },
        _ = terminate => {
            info!("Received SIGTERM, shutting down gracefully");
        }
    }
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
                        tracing::info!(
                            "Background token refresh: Successfully refreshed OAuth tokens"
                        );

                        // Clone config for I/O operation and release lock BEFORE file save
                        let config_to_save = config_guard.clone();
                        drop(config_guard); // Explicitly release lock

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
