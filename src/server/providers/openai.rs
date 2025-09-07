use axum::http::{HeaderMap, StatusCode};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::auth::openai::OpenAIOAuth;
use crate::config::Config;
use crate::error::SetuError;
use crate::router::name_based::RoutingDecision;
use crate::server::error_handling;

enum OpenAIAuth {
    OAuth(String),
    ApiKey(String),
}

/// Replace instructions with simple OpenAI-compatible instruction
/// OpenAI Responses API expects simple, clean instructions without Claude-specific content
fn sanitize_instructions_for_openai(_text: String) -> String {
    // Always use simple, clean instruction like codex-openai-proxy does
    // The OpenAI Responses API rejects complex Claude-specific instructions
    "You are a helpful AI assistant. Provide clear, accurate, and concise responses to user questions and requests.".to_string()
}

/// Resolve OpenAI auth using OAuth (codex/setu) or API key
async fn resolve_openai_auth(config: Arc<Mutex<Config>>) -> Result<OpenAIAuth, SetuError> {
    // Temporarily skip OAuth - go straight to API key
    // TODO: Re-enable OAuth after fixing Responses API issues
    
    // Primary: Environment API key
    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        tracing::info!("🔐 OpenAI → API key via OPENAI_API_KEY");
        return Ok(OpenAIAuth::ApiKey(api_key));
    }

    // Fallback: Config API key
    let cfg = config.lock().await;
    if let Some(provider) = cfg.providers.get("openai")
        && let Some(api_key) = &provider.api_key
    {
        tracing::info!("🔐 OpenAI → API key via setu config");
        return Ok(OpenAIAuth::ApiKey(api_key.clone()));
    }
    drop(cfg);

    Err(SetuError::Other(
        "No OpenAI credentials available (OAuth or API key)".to_string(),
    ))
}

fn openai_base_url(config: &Config) -> String {
    if let Some(provider) = config.providers.get("openai") {
        provider.endpoint.clone()
    } else {
        "https://api.openai.com".to_string()
    }
}

fn auth_header_value(auth: &OpenAIAuth) -> String {
    match auth {
        OpenAIAuth::OAuth(t) | OpenAIAuth::ApiKey(t) => format!("Bearer {}", t),
    }
}

/// Send OpenAI-format request directly to OpenAI
pub async fn handle_openai_request_from_openai(
    config: Arc<Mutex<Config>>,
    openai_request: openai_ox::request::ChatRequest,
    routing_decision: RoutingDecision,
    _headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    // Prepare HTTP
    let auth = match resolve_openai_auth(config.clone()).await {
        Ok(a) => a,
        Err(e) => return Err(error_handling::unauthorized(&e.to_string())),
    };
    let cfg = config.lock().await;
    let base = openai_base_url(&cfg);
    drop(cfg);

    let url = format!("{}/v1/chat/completions", base.trim_end_matches('/'));
    if let Some(req_str) = crate::server::error_handling::prepare_openai_request_log(&openai_request) {
        tracing::debug!(target = "setu::request", "Outgoing OpenAI request (detailed): {}", req_str);
    }

    let client = reqwest::Client::new();
    let resp = match client
        .post(&url)
        .header("Authorization", auth_header_value(&auth))
        .header("Content-Type", "application/json")
        .json(&openai_request)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return Err(error_handling::bad_gateway("OpenAI request failed", &e)),
    };

    let status = resp.status().as_u16();
    let text = match resp.text().await {
        Ok(t) => t,
        Err(e) => return Err(error_handling::bad_gateway("Failed to read OpenAI response", &e)),
    };

    if let Ok(val) = serde_json::from_str::<Value>(&text) {
        if let Some(resp_str) = crate::server::error_handling::prepare_response_log(&val) {
            tracing::debug!(target = "setu::response", "OpenAI response: {}", resp_str);
        }
    }

    Ok(axum::response::Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(text))
        .unwrap())
}

/// Send Anthropic-format request by converting to OpenAI via ai-ox and calling OpenAI API
pub async fn handle_openai_request_from_anthropic(
    config: Arc<Mutex<Config>>,
    anthropic_request: anthropic_ox::ChatRequest,
    routing_decision: RoutingDecision,
    _headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    // Prepare auth
    let auth = match resolve_openai_auth(config.clone()).await {
        Ok(a) => a,
        Err(e) => return Err(error_handling::unauthorized(&e.to_string())),
    };
    // Pass through original request without instruction sanitization (acting as proxy)
    // Convert Anthropic → OpenAI Responses API using ai-ox
    let mut responses_req = match conversion_ox::anthropic_openai::anthropic_to_openai_responses_request(
        anthropic_request,
    ) {
        Ok(req) => req,
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to convert Anthropic request to OpenAI Responses format",
                &e,
            ))
        }
    };
    responses_req.model = routing_decision.model.clone();
    
    if let Some(req_str) = crate::server::error_handling::prepare_request_log(&responses_req) {
        tracing::debug!(target = "setu::request", "Outgoing OpenAI Responses (from Anthropic) request (detailed): {}", req_str);
    }

    // Build OpenAI client targeting appropriate base URL
    let client = match &auth {
        OpenAIAuth::OAuth(token) => {
            // ChatGPT Responses endpoint (kept for future use)
            openai_ox::OpenAI::builder()
                .api_key(token.clone())
                .base_url("https://chatgpt.com/backend-api/codex")
                .build()
        }
        OpenAIAuth::ApiKey(key) => {
            // Platform Responses endpoint
            let cfg = config.lock().await;
            let base = openai_base_url(&cfg);
            drop(cfg);
            openai_ox::OpenAI::builder()
                .api_key(key.clone())
                .base_url(format!("{}/v1", base.trim_end_matches('/')))
                .build()
        }
    };

    // Send via Responses API
    match client.send_responses(&responses_req).await {
        Ok(resp) => {
            if let Some(resp_str) = crate::server::error_handling::prepare_response_log(&resp) {
                tracing::debug!(target = "setu::response", "OpenAI Responses response: {}", resp_str);
            }
            let json_body = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
            Ok(axum::response::Response::builder()
                .status(200)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(json_body))
                .unwrap())
        }
        Err(e) => {
            tracing::error!("OpenAI Responses API error details: {:?}", e);
            // Try to make a direct HTTP call to get more details
            let cfg = config.lock().await;
            let base = openai_base_url(&cfg);
            drop(cfg);
            
            let url = format!("{}/v1/responses", base.trim_end_matches('/'));
            let client = reqwest::Client::new();
            
            let resp = client
                .post(&url)
                .header("Authorization", auth_header_value(&auth))
                .header("OpenAI-Beta", "responses=experimental")
                .header("Content-Type", "application/json")
                .json(&responses_req)
                .send()
                .await;
                
            if let Ok(r) = resp {
                let status = r.status();
                if let Ok(text) = r.text().await {
                    tracing::error!("Raw OpenAI response (status {}): {}", status, text);
                }
            }
            
            Err(error_handling::bad_gateway("OpenAI Responses API request failed", &e))
        }
    }
}
