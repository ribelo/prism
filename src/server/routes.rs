use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{Json, IntoResponse},
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;
use futures_util::StreamExt;

use crate::{
    auth::anthropic::AnthropicOAuth,
    config::Config,
    router::{NameBasedRouter, RoutingDecision},
};

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
    _request: Request,
) -> Result<Json<Value>, StatusCode> {
    info!("OpenAI chat completions → Not implemented");
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// NoOp handler for OpenAI models endpoint
pub async fn openai_models() -> Json<Value> {
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

/// Anthropic messages endpoint handler with Claude Code transformation
pub async fn anthropic_messages(
    State(config): State<Arc<Config>>,
    request: Request,
) -> Result<axum::response::Response, StatusCode> {

    let (parts, body) = request.into_parts();
    
    // Get request body as JSON
    let body_bytes = axum::body::to_bytes(body, 1024 * 1024).await
        .map_err(|e| {
            tracing::error!("Failed to read request body: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    // Parse request to check model
    let chat_request = match serde_json::from_slice::<ChatRequest>(&body_bytes) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!("Invalid ChatRequest JSON in request body: {}", e);
            
            // Try to parse as generic JSON for compacted logging
            match serde_json::from_slice::<Value>(&body_bytes) {
                Ok(json_value) => {
                    let compacted = compact_request_for_logging(&json_value);
                    tracing::error!("Failed request body (compacted): {}", 
                        serde_json::to_string_pretty(&compacted).unwrap_or_else(|_| "Failed to serialize".to_string()));
                }
                Err(_) => {
                    // If it's not even valid JSON, show first 500 chars
                    let body_str = String::from_utf8_lossy(&body_bytes);
                    let truncated = if body_str.len() > 500 {
                        format!("{}...", &body_str[..500])
                    } else {
                        body_str.to_string()
                    };
                    tracing::error!("Failed request body (not valid JSON): {}", truncated);
                }
            }
            
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Route the request based on model name
    let router = NameBasedRouter::new((*config).clone());
    let routing_decision = match router.route_model(&chat_request.model) {
        Ok(decision) => decision,
        Err(e) => {
            tracing::error!("Routing error for model {}: {}", chat_request.model, e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Check if this is a Claude Code request that needs transformation
    let is_claude_code = is_claude_code_request(&parts.headers);
    let has_api_key = has_api_key_auth(&parts.headers);

    // Handle OpenRouter routing
    if routing_decision.provider == "openrouter" {
        return handle_openrouter_request(config, chat_request, routing_decision, parts.headers).await;
    }
    
    if is_claude_code && has_api_key {
        info!("Claude Code → Anthropic OAuth: {}", chat_request.model);
        
        let chat_request = chat_request;

        // Don't modify system field - preserve original structure

        // Get OAuth token from provider config
        let mut auth_config = config.providers.get("anthropic")
            .map(|provider| provider.auth.clone())
            .ok_or_else(|| {
                tracing::error!("No OAuth credentials for anthropic provider");
                StatusCode::UNAUTHORIZED
            })?;

        // Refresh token if expired
        if auth_config.is_token_expired() {
            if let Err(e) = AnthropicOAuth::refresh_token(&mut auth_config).await {
                tracing::error!("Failed to refresh OAuth token: {}", e);
                return Err(StatusCode::UNAUTHORIZED);
            }
        }

        // Get OAuth access token
        let oauth_token = auth_config.oauth_access_token
            .ok_or_else(|| {
                tracing::error!("No OAuth access token available");
                StatusCode::UNAUTHORIZED
            })?;

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


        // Check if this is a streaming request
        let is_streaming = chat_request.stream.unwrap_or(false);

        if is_streaming {
            // Use stream method for streaming responses
            let mut stream = client.stream(&chat_request);
            let mut sse_response = String::new();
            
            while let Some(event_result) = stream.next().await {
                match event_result {
                    Ok(event) => {
                        let event_json = serde_json::to_string(&event)
                            .map_err(|e| {
                                tracing::error!("Failed to serialize stream event: {}", e);
                                StatusCode::INTERNAL_SERVER_ERROR
                            })?;
                        sse_response.push_str(&format!("data: {}\n\n", event_json));
                    }
                    Err(e) => {
                        tracing::error!("Stream event error: {}", e);
                        return Err(StatusCode::BAD_GATEWAY);
                    }
                }
            }
            
            // Add final done event
            sse_response.push_str("data: [DONE]\n\n");
            
            // Return SSE response
            use axum::response::Response;
            use axum::http::header;
            
            let response = Response::builder()
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header(header::CACHE_CONTROL, "no-cache")
                .header(header::CONNECTION, "keep-alive")
                .body(axum::body::Body::from(sse_response))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(response)
        } else {
            // Use send method for non-streaming responses  
            match client.send(&chat_request).await {
                Ok(response) => {
                    let response_json = serde_json::to_value(response)
                        .map_err(|e| {
                            tracing::error!("Failed to serialize response: {}", e);
                            StatusCode::INTERNAL_SERVER_ERROR
                        })?;
                    Ok(Json(response_json).into_response())
                },
                Err(e) => {
                    tracing::error!("Anthropic OAuth request failed: {}", e);
                    Err(StatusCode::BAD_GATEWAY)
                }
            }
        }
    } else {
        // Normal proxy passthrough for non-Claude Code requests
        info!("Direct → Anthropic: {}", chat_request.model);

        // Create client with API key from authorization header
        let mut client = if let Some(auth_header) = parts.headers.get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                if let Some(api_key) = auth_str.strip_prefix("Bearer ") {
                    Anthropic::builder().api_key(api_key).build()
                } else {
                    return Err(StatusCode::UNAUTHORIZED);
                }
            } else {
                return Err(StatusCode::UNAUTHORIZED);
            }
        } else {
            return Err(StatusCode::UNAUTHORIZED);
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
        
        // Check if streaming
        let is_streaming = chat_request.stream.unwrap_or(false);
        
        if is_streaming {
            // Handle streaming
            let mut stream = client.stream(&chat_request);
            let mut sse_response = String::new();
            
            while let Some(event_result) = stream.next().await {
                match event_result {
                    Ok(event) => {
                        let event_json = serde_json::to_string(&event)
                            .map_err(|e| {
                                tracing::error!("Failed to serialize stream event: {}", e);
                                StatusCode::INTERNAL_SERVER_ERROR
                            })?;
                        sse_response.push_str(&format!("data: {}\n\n", event_json));
                    }
                    Err(e) => {
                        tracing::error!("Stream event error: {}", e);
                        return Err(StatusCode::BAD_GATEWAY);
                    }
                }
            }
            
            sse_response.push_str("data: [DONE]\n\n");
            
            use axum::response::Response;
            use axum::http::header;
            
            let response = Response::builder()
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header(header::CACHE_CONTROL, "no-cache")
                .header(header::CONNECTION, "keep-alive")
                .body(axum::body::Body::from(sse_response))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(response)
        } else {
            // Handle non-streaming
            match client.send(&chat_request).await {
                Ok(response) => {
                    let response_json = serde_json::to_value(response)
                        .map_err(|e| {
                            tracing::error!("Failed to serialize response: {}", e);
                            StatusCode::INTERNAL_SERVER_ERROR
                        })?;
                    Ok(Json(response_json).into_response())
                },
                Err(e) => {
                    tracing::error!("Anthropic request failed: {}", e);
                    Err(StatusCode::BAD_GATEWAY)
                }
            }
        }
    }
}

/// Handle requests routed to OpenRouter
async fn handle_openrouter_request(
    _config: Arc<Config>,
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
        .map_err(|e| {
            tracing::error!("Failed to load OpenRouter credentials from environment: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if is_streaming {
        // Handle streaming request
        let mut stream = openrouter_client.stream(&openrouter_request);
        let mut converter = StreamConverter::new();
        let mut sse_response = String::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    // Convert OpenRouter chunk to Anthropic stream events
                    let anthropic_events = converter.convert_chunk(chunk);
                    
                    for event in anthropic_events {
                        let event_json = serde_json::to_string(&event)
                            .map_err(|e| {
                                tracing::error!("Failed to serialize stream event: {}", e);
                                StatusCode::INTERNAL_SERVER_ERROR
                            })?;
                        sse_response.push_str(&format!("data: {}\n\n", event_json));
                    }
                }
                Err(e) => {
                    tracing::error!("OpenRouter stream error: {}", e);
                    return Err(StatusCode::BAD_GATEWAY);
                }
            }
        }

        // Add final done event
        sse_response.push_str("data: [DONE]\n\n");

        // Return SSE response
        use axum::response::Response;
        use axum::http::header;

        let response = Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(axum::body::Body::from(sse_response))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(response)
    } else {
        // Handle non-streaming request
        match openrouter_client.send(&openrouter_request).await {
            Ok(openrouter_response) => {
                // Convert OpenRouter response to Anthropic response
                let anthropic_response: anthropic_ox::response::ChatResponse = openrouter_response.into();
                
                let response_json = serde_json::to_value(anthropic_response)
                    .map_err(|e| {
                        tracing::error!("Failed to serialize response: {}", e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
                
                Ok(Json(response_json).into_response())
            }
            Err(e) => {
                tracing::error!("OpenRouter request failed: {}", e);
                Err(StatusCode::BAD_GATEWAY)
            }
        }
    }
}