use axum::http::{HeaderMap, StatusCode};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::config::Config;
use crate::server::error_handling;

/// Check if request is from Claude Code by examining headers
pub fn is_claude_code_request(headers: &HeaderMap) -> bool {
    if let Some(user_agent) = headers.get("user-agent")
        && let Ok(ua_str) = user_agent.to_str()
        && ua_str.starts_with("claude-cli/") {
        return true;
    }

    if let Some(x_app) = headers.get("x-app")
        && let Ok(app_str) = x_app.to_str()
        && app_str == "cli" {
        return true;
    }

    false
}

/// Handle OAuth authentication requests
pub async fn handle_oauth_request(
    config: Arc<Mutex<Config>>,
    routing_decision: crate::router::name_based::RoutingDecision,
    headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    let provider_config = {
        let config_guard = config.lock().await;
        config_guard.providers.get(&routing_decision.provider).cloned()
    };

    match provider_config {
        Some(_provider) => {
            let _is_claude_code = is_claude_code_request(&headers);
            
            // For now, return an error indicating OAuth is not fully implemented
            Err(error_handling::internal_error(
                "OAuth authentication not yet fully implemented",
                &"Direct provider authentication required"
            ))
        }
        None => {
            Err(error_handling::bad_request(
                &format!("Provider '{}' not configured", routing_decision.provider),
                &crate::error::SetuError::ProviderNotFound(routing_decision.provider.clone()),
            ))
        }
    }
}