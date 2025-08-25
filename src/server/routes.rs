use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{Json, IntoResponse},
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;
use futures_util::StreamExt;

use crate::{
    config::Config,
    router::{NameBasedRouter, RoutingDecision},
};

/// Error handling helpers for consistent logging and status codes
mod error_handling {
    use axum::http::StatusCode;
    use tracing::error;

    /// Log error and return BAD_REQUEST
    pub fn bad_request(msg: &str, err: &impl std::fmt::Display) -> StatusCode {
        error!("{}: {}", msg, err);
        StatusCode::BAD_REQUEST
    }

    /// Log error and return UNAUTHORIZED
    pub fn unauthorized(msg: &str) -> StatusCode {
        error!("{}", msg);
        StatusCode::UNAUTHORIZED
    }

    /// Log error and return BAD_GATEWAY (for external API failures)
    pub fn bad_gateway(msg: &str, err: &impl std::fmt::Display) -> StatusCode {
        error!("{}: {}", msg, err);
        StatusCode::BAD_GATEWAY
    }

    /// Log error and return INTERNAL_SERVER_ERROR
    pub fn internal_error(msg: &str, err: &impl std::fmt::Display) -> StatusCode {
        error!("{}: {}", msg, err);
        StatusCode::INTERNAL_SERVER_ERROR
    }

}

// AI provider imports
use anthropic_ox::{Anthropic, ChatRequest};
use openrouter_ox::{OpenRouter, conversion::streaming::StreamConverter};

/// Check if request is from Claude Code CLI
fn is_claude_code_request(headers: &axum::http::HeaderMap) -> bool {
    // Check user-agent for Claude Code
    if let Some(user_agent) = headers.get("user-agent") {
        if let Ok(ua_str) = user_agent.to_str() {
            if ua_str.starts_with("claude-cli/") {
                return true;
            }
        }
    }
    
    // Check x-app header
    if let Some(x_app) = headers.get("x-app") {
        if let Ok(app_str) = x_app.to_str() {
            if app_str == "cli" {
                return true;
            }
        }
    }
    
    false
}

/// Check if authorization header contains an API key (starts with sk-)
fn has_api_key_auth(headers: &axum::http::HeaderMap) -> bool {
    if let Some(auth) = headers.get("authorization") {
        if let Ok(auth_str) = auth.to_str() {
            return auth_str.starts_with("Bearer sk-");
        }
    }
    false
}

/// Transform anthropic-beta header to include OAuth beta flag
fn transform_anthropic_beta_header(existing_beta: Option<&str>) -> String {
    match existing_beta {
        Some(beta) => format!("oauth-2025-04-20,{}", beta),
        None => "oauth-2025-04-20".to_string(),
    }
}

/// Compact a JSON request for logging by truncating long text values
/// Preserves structure while making logs readable
fn compact_request_for_logging(value: &Value) -> Value {
    match value {
        Value::String(s) => {
            if s.len() > 100 {
                Value::String(format!("{}...", &s[..97]))
            } else {
                value.clone()
            }
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(compact_request_for_logging).collect())
        }
        Value::Object(obj) => {
            let mut compacted = serde_json::Map::new();
            for (key, val) in obj {
                // Keep important structural fields intact
                match key.as_str() {
                    "model" | "role" | "type" | "id" | "name" => {
                        compacted.insert(key.clone(), val.clone());
                    }
                    _ => {
                        compacted.insert(key.clone(), compact_request_for_logging(val));
                    }
                }
            }
            Value::Object(compacted)
        }
        _ => value.clone(),
    }
}

/// OpenAI chat completions endpoint handler - not implemented for Claude Code
pub async fn openai_chat_completions(
    State(_config): State<Arc<Mutex<Config>>>,
    _request: Request,
) -> Result<Json<Value>, StatusCode> {
    info!("OpenAI chat completions → Not implemented");
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// NoOp handler for OpenAI models endpoint
pub async fn openai_models(
    State(_config): State<Arc<Mutex<Config>>>
) -> Json<Value> {
    info!("OpenAI models → Mock response");
    
    let mock_response = json!({
        "object": "list",
        "data": [{
            "id": "setu-noop",
            "object": "model",
            "created": chrono::Utc::now().timestamp(),
            "owned_by": "setu"
        }]
    });

    Json(mock_response)
}

/// Handle OAuth authentication for Claude Code requests
async fn handle_oauth_request(
    config: Arc<Mutex<Config>>,
    chat_request: ChatRequest,
    parts: axum::http::request::Parts,
) -> Result<axum::response::Response, StatusCode> {
    info!("Claude Code → Anthropic OAuth: {}", chat_request.model);

    // Extract OAuth token from provider config - get only what we need
    let oauth_token = {
        let config_guard = config.lock().await;
        
        let auth_config = config_guard.providers.get("anthropic")
            .map(|provider| &provider.auth)
            .ok_or_else(|| error_handling::unauthorized("No OAuth credentials for anthropic provider"))?;

        // Check if token is expired - if so, return clear error message
        if auth_config.is_token_expired() {
            return Err(error_handling::unauthorized("OAuth tokens expired or invalid. Run: setu auth anthropic"));
        }

        // Get OAuth access token and clone just the token string
        auth_config.oauth_access_token.as_ref()
            .ok_or_else(|| error_handling::unauthorized("OAuth tokens expired or invalid. Run: setu auth anthropic"))?
            .clone()
    }; // Lock is automatically dropped here

    // Create Anthropic client with OAuth and custom headers
    let mut client = Anthropic::builder()
        .oauth_token(oauth_token)
        .build();

    // Add required headers
    client = client.header("anthropic-client-id", "9d1c250a-e61b-44d9-88ed-5944d1962f5e");

    // Transform and add other headers
    for (name, value) in parts.headers.iter() {
        if let Ok(value_str) = value.to_str() {
            match name.as_str() {
                "anthropic-beta" => {
                    let transformed_beta = transform_anthropic_beta_header(Some(value_str));
                    client = client.header("anthropic-beta", transformed_beta);
                }
                "x-stainless-helper-method" => {
                    client = client.header("x-stainless-helper-method", value_str);
                }
                _ => {}
            }
        }
    }

    handle_anthropic_streaming(&client, &chat_request).await
}

/// Handle direct Anthropic API requests (non-Claude Code)
async fn handle_direct_anthropic_request(
    chat_request: ChatRequest,
    parts: axum::http::request::Parts,
) -> Result<axum::response::Response, StatusCode> {
    info!("Direct → Anthropic: {}", chat_request.model);

    // Create client with API key from authorization header
    let mut client = if let Some(auth_header) = parts.headers.get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(api_key) = auth_str.strip_prefix("Bearer ") {
                Anthropic::builder().api_key(api_key).build()
            } else {
                return Err(error_handling::unauthorized("Invalid authorization header format"));
            }
        } else {
            return Err(error_handling::unauthorized("Authorization header contains invalid characters"));
        }
    } else {
        return Err(error_handling::unauthorized("Missing authorization header"));
    };

    // Add custom headers from request
    for (name, value) in parts.headers.iter() {
        if let Ok(value_str) = value.to_str() {
            match name.as_str() {
                "anthropic-beta" | "x-stainless-helper-method" => {
                    client = client.header(name.as_str(), value_str);
                }
                _ => {}
            }
        }
    }
    
    handle_anthropic_streaming(&client, &chat_request).await
}

/// Handle streaming/non-streaming for Anthropic requests
async fn handle_anthropic_streaming(
    client: &Anthropic,
    chat_request: &ChatRequest,
) -> Result<axum::response::Response, StatusCode> {
    let is_streaming = chat_request.stream.unwrap_or(false);

    if is_streaming {
        // Use stream method for streaming responses
        let stream = client.stream(chat_request);
        
        // Create a streaming body that doesn't buffer everything in memory
        let event_stream = stream.map(|event_result| {
            match event_result {
                Ok(event) => {
                    match serde_json::to_string(&event) {
                        Ok(event_json) => Ok(format!("data: {}\n\n", event_json)),
                        Err(e) => {
                            error_handling::internal_error("Failed to serialize stream event", &e);
                            Err(std::io::Error::new(std::io::ErrorKind::Other, e))
                        }
                    }
                }
                Err(e) => {
                    error_handling::bad_gateway("Stream event error", &e);
                    Err(std::io::Error::new(std::io::ErrorKind::Other, e))
                }
            }
        })
        .chain(futures_util::stream::once(async { Ok("data: [DONE]\n\n".to_string()) }));
        
        // Convert to axum body stream
        let body = axum::body::Body::from_stream(event_stream);
        
        // Return SSE response
        use axum::response::Response;
        use axum::http::header;
        
        let response = Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(body)
            .map_err(|e| error_handling::internal_error("Failed to build streaming response", &e))?;
        Ok(response)
    } else {
        // Use send method for non-streaming responses  
        match client.send(chat_request).await {
            Ok(response) => {
                let response_json = serde_json::to_value(response)
                    .map_err(|e| error_handling::internal_error("Failed to serialize response", &e))?;
                Ok(Json(response_json).into_response())
            },
            Err(e) => {
                Err(error_handling::bad_gateway("Anthropic request failed", &e))
            }
        }
    }
}

/// Parse and validate request body
async fn parse_chat_request(body: axum::body::Body) -> Result<ChatRequest, StatusCode> {
    // Get request body as JSON
    let body_bytes = axum::body::to_bytes(body, 1024 * 1024).await
        .map_err(|e| error_handling::bad_request("Failed to read request body", &e))?;

    // Parse request to check model
    match serde_json::from_slice::<ChatRequest>(&body_bytes) {
        Ok(req) => Ok(req),
        Err(e) => {
            // Try to parse as generic JSON for compacted logging
            match serde_json::from_slice::<Value>(&body_bytes) {
                Ok(json_value) => {
                    let compacted = compact_request_for_logging(&json_value);
                    let compact_str = serde_json::to_string_pretty(&compacted)
                        .unwrap_or_else(|_| "Failed to serialize".to_string());
                    tracing::error!("Invalid ChatRequest JSON (compacted): {}", compact_str);
                }
                Err(_) => {
                    // If it's not even valid JSON, show first 500 chars
                    let body_str = String::from_utf8_lossy(&body_bytes);
                    let truncated = if body_str.len() > 500 {
                        format!("{}...", &body_str[..500])
                    } else {
                        body_str.to_string()
                    };
                    tracing::error!("Invalid request body (not valid JSON): {}", truncated);
                }
            }
            
            Err(error_handling::bad_request("Invalid ChatRequest JSON in request body", &e))
        }
    }
}

/// Anthropic messages endpoint handler with Claude Code transformation
pub async fn anthropic_messages(
    State(config): State<Arc<Mutex<Config>>>,
    request: Request,
) -> Result<axum::response::Response, StatusCode> {
    let (parts, body) = request.into_parts();
    
    // Parse request body
    let chat_request = parse_chat_request(body).await?;

    // Route the request based on model name - extract only default provider
    let default_provider = {
        let config_guard = config.lock().await;
        config_guard.routing.default_provider.clone()
    };
    let router = NameBasedRouter::new_with_default_provider(default_provider);
    let routing_decision = match router.route_model(&chat_request.model) {
        Ok(decision) => decision,
        Err(e) => {
            return Err(error_handling::bad_request(&format!("Routing error for model {}", chat_request.model), &e));
        }
    };

    // Handle OpenRouter routing
    if routing_decision.provider == "openrouter" {
        return handle_openrouter_request(config, chat_request, routing_decision, parts.headers).await;
    }

    // Check if this is a Claude Code request that needs transformation
    let is_claude_code = is_claude_code_request(&parts.headers);
    let has_api_key = has_api_key_auth(&parts.headers);
    
    if is_claude_code && has_api_key {
        handle_oauth_request(config, chat_request, parts).await
    } else {
        handle_direct_anthropic_request(chat_request, parts).await
    }
}

/// Handle requests routed to OpenRouter
async fn handle_openrouter_request(
    _config: Arc<Mutex<Config>>,
    anthropic_request: ChatRequest,
    routing_decision: RoutingDecision,
    _headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    info!("Claude Code → OpenRouter: {} ({})", anthropic_request.model, routing_decision.model);

    // Check if this is a streaming request (before converting)
    let is_streaming = anthropic_request.stream.unwrap_or(false);

    // Convert Anthropic request to OpenRouter request
    let mut openrouter_request: openrouter_ox::ChatRequest = anthropic_request.into();
    // Use the routed model name instead of the original
    openrouter_request.model = routing_decision.model;

    // Debug: Log the converted request
    tracing::debug!("OpenRouter request: {}", serde_json::to_string_pretty(&openrouter_request).unwrap_or_else(|_| "Failed to serialize".to_string()));

    // Create OpenRouter client from environment
    let openrouter_client = OpenRouter::load_from_env()
        .map_err(|e| error_handling::internal_error("Failed to load OpenRouter credentials from environment", &e))?;

    if is_streaming {
        // Handle streaming request without buffering
        let stream = openrouter_client.stream(&openrouter_request);
        let mut converter = StreamConverter::new();

        let event_stream = stream.map(move |chunk_result| {
            match chunk_result {
                Ok(chunk) => {
                    // Convert OpenRouter chunk to Anthropic stream events
                    let anthropic_events = converter.convert_chunk(chunk);
                    let mut result = String::new();
                    
                    for event in anthropic_events {
                        match serde_json::to_string(&event) {
                            Ok(event_json) => {
                                result.push_str(&format!("data: {}\n\n", event_json));
                            }
                            Err(e) => {
                                error_handling::internal_error("Failed to serialize OpenRouter stream event", &e);
                                return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
                            }
                        }
                    }
                    Ok(result)
                }
                Err(e) => {
                    error_handling::bad_gateway("OpenRouter stream error", &e);
                    Err(std::io::Error::new(std::io::ErrorKind::Other, e))
                }
            }
        })
        .chain(futures_util::stream::once(async { Ok("data: [DONE]\n\n".to_string()) }));

        let body = axum::body::Body::from_stream(event_stream);

        // Return SSE response
        use axum::response::Response;
        use axum::http::header;

        let response = Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(body)
            .map_err(|e| error_handling::internal_error("Failed to build streaming response", &e))?;
        Ok(response)
    } else {
        // Handle non-streaming request
        match openrouter_client.send(&openrouter_request).await {
            Ok(openrouter_response) => {
                // Convert OpenRouter response to Anthropic response
                let anthropic_response: anthropic_ox::response::ChatResponse = openrouter_response.into();
                
                let response_json = serde_json::to_value(anthropic_response)
                    .map_err(|e| error_handling::internal_error("Failed to serialize OpenRouter response", &e))?;
                
                Ok(Json(response_json).into_response())
            }
            Err(e) => {
                Err(error_handling::bad_gateway("OpenRouter request failed", &e))
            }
        }
    }
}