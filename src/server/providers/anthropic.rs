use anthropic_ox::{Anthropic, ChatRequest};
use axum::http::{HeaderMap, StatusCode};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::auth::anthropic::AnthropicOAuth;
use crate::config::Config;
use crate::error::SetuError;
use crate::router::name_based::RoutingDecision;
use crate::server::error_handling;

/// Handle direct Anthropic requests using OAuth or API key
pub async fn handle_direct_anthropic_request(
    config: Arc<Mutex<Config>>,
    mut anthropic_request: ChatRequest,
    routing_decision: RoutingDecision,
    headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    let is_claude_code = super::auth::is_claude_code_request(&headers);

    let anthropic_client = match create_anthropic_client(config, is_claude_code).await {
        Ok(client) => client,
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to create Anthropic client",
                &e,
            ));
        }
    };

    // Apply URL parameters to the request if present
    if let Some(query_params) = routing_decision.query_params {
        anthropic_request = crate::server::parameter_mapping::apply_anthropic_parameters(
            anthropic_request,
            &query_params,
        );
    }

    if let Some(req_str) = crate::server::error_handling::prepare_anthropic_request_log(&anthropic_request) {
        tracing::debug!(target: "setu::request", "Outgoing Anthropic request (detailed): {}", req_str);
    }

    // Ensure request model matches routed model for Anthropic provider
    if !routing_decision.model.is_empty() {
        if let Ok(mut val) = serde_json::to_value(&anthropic_request) {
            val["model"] = serde_json::Value::String(routing_decision.model.clone());
            if let Ok(updated) = serde_json::from_value::<ChatRequest>(val) {
                anthropic_request = updated;
            }
        }
    }

    // Send request to Anthropic (simplified for now)
    match anthropic_client.send(&anthropic_request).await {
        Ok(response) => {
            if let Some(resp_str) = crate::server::error_handling::prepare_response_log(&response) {
                tracing::debug!(target: "setu::response", "Anthropic response: {}", resp_str);
            }

            // Convert response to JSON and return
            let json_body = match serde_json::to_string(&response) {
                Ok(body) => body,
                Err(e) => {
                    return Err(error_handling::internal_error(
                        "Failed to serialize Anthropic response",
                        &e,
                    ));
                }
            };

            Ok(axum::response::Response::builder()
                .status(200)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(json_body))
                .unwrap())
        }
        Err(e) => {
            let compacted_request =
                super::super::error_handling::compact_request_for_logging(&anthropic_request);
            tracing::error!(
                "Failed streaming request (compacted): {}",
                compacted_request
            );

            Err(error_handling::internal_error(
                "Anthropic API request failed",
                &e,
            ))
        }
    }
}

/// Create Anthropic client with OAuth or API key authentication
pub async fn create_anthropic_client(
    config: Arc<Mutex<Config>>,
    prefer_oauth: bool,
) -> Result<Anthropic, SetuError> {
    if prefer_oauth {
        // Try OAuth authentication first for Claude Code
        let mut config_guard = config.lock().await;
        if let Some(anthropic_provider) = config_guard.providers.get_mut("anthropic") {
            match AnthropicOAuth::validate_auth_config(&mut anthropic_provider.auth).await {
                Ok(()) => {
                    if let Ok(access_token) =
                        AnthropicOAuth::get_valid_access_token(&mut anthropic_provider.auth, false)
                            .await
                    {
                        info!("🔐 Anthropic → OAuth (subscription billing)");
                        return Ok(Anthropic::builder().oauth_token(&access_token).build());
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "OAuth authentication failed, falling back to API key: {}",
                        e
                    );
                }
            }
        }
    }

    // Fallback to API key authentication
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        info!("🔐 Anthropic → API key (pay-per-use billing)");
        return Ok(Anthropic::builder().api_key(&api_key).build());
    }

    let config_guard = config.lock().await;
    if let Some(anthropic_provider) = config_guard.providers.get("anthropic")
        && let Some(api_key) = &anthropic_provider.api_key
    {
        info!("🔐 Anthropic → API key via setu config (pay-per-use billing)");
        return Ok(Anthropic::builder().api_key(api_key).build());
    }

    Err(SetuError::Other(
        "No Anthropic authentication available (OAuth or API key)".to_string(),
    ))
}
