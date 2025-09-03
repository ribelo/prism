use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::{
    config::Config,
    error::SetuError,
    router::{NameBasedRouter, name_based::LegacyRoutingDecision},
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
use anthropic_ox::{Anthropic, ChatRequest, request::ThinkingConfig as AnthropicThinkingConfig};
use gemini_ox::{Gemini, generate_content::ThinkingConfig as GeminiThinkingConfig};
use openrouter_ox::{OpenRouter, provider_preference::{ProviderPreferences, Sort}};
use std::collections::HashMap;

// OpenAI reasoning config - define locally since we don't have openai-ox in setu dependencies
#[derive(Debug, Clone)]
pub struct ReasoningConfig {
    pub effort: Option<String>,
    pub summary: Option<String>,
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
        Value::Array(arr) => Value::Array(arr.iter().map(compact_request_for_logging).collect()),
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

/// Parameter mapping functions for thinking/reasoning features
mod parameter_mapping {
    use super::*;

    /// Apply thinking parameters to Anthropic ChatRequest
    pub fn apply_anthropic_thinking_params(
        mut request: anthropic_ox::ChatRequest,
        query_params: &HashMap<String, String>,
    ) -> anthropic_ox::ChatRequest {
        if let Some(think_value) = query_params.get("think") {
            if let Ok(budget) = think_value.parse::<u32>() {
                request.thinking = Some(AnthropicThinkingConfig::new(budget));
            }
        }
        request
    }

    /// Apply reasoning parameters to OpenAI ReasoningConfig for o1 models
    pub fn create_openai_reasoning_config(query_params: &HashMap<String, String>) -> Option<ReasoningConfig> {
        if let Some(effort_value) = query_params.get("effort") {
            match effort_value.as_str() {
                "minimal" | "low" | "medium" | "high" => {
                    Some(ReasoningConfig {
                        effort: Some(effort_value.clone()),
                        summary: None,
                    })
                }
                _ => None,
            }
        } else {
            None
        }
    }

    /// Apply thinking parameters to Gemini ThinkingConfig
    pub fn create_gemini_thinking_config(query_params: &HashMap<String, String>) -> Option<GeminiThinkingConfig> {
        let mut has_thinking_params = false;
        let mut thinking_budget = 0;
        let mut include_thoughts = false;

        if let Some(think_value) = query_params.get("think") {
            if let Ok(budget) = think_value.parse::<i32>() {
                thinking_budget = budget;
                has_thinking_params = true;
            }
        }

        if let Some(thoughts_value) = query_params.get("thoughts") {
            include_thoughts = thoughts_value.parse::<bool>().unwrap_or(false);
            has_thinking_params = true;
        }

        if has_thinking_params {
            Some(GeminiThinkingConfig {
                include_thoughts,
                thinking_budget,
            })
        } else {
            None
        }
    }

    /// Apply reasoning and provider parameters to OpenRouter ProviderPreferences
    pub fn apply_openrouter_thinking_params(
        mut provider_prefs: ProviderPreferences,
        query_params: &HashMap<String, String>,
    ) -> (ProviderPreferences, Option<bool>) {
        let mut include_reasoning = None;

        // Handle reasoning flag
        if let Some(reasoning_value) = query_params.get("reasoning") {
            include_reasoning = Some(reasoning_value.parse::<bool>().unwrap_or(false));
        }

        // Handle thinking budget through max_price or other mechanisms if needed
        // OpenRouter doesn't have direct thinking budget, but can be extended later

        // Handle effort mapping to Sort preference
        if let Some(effort_value) = query_params.get("effort") {
            match effort_value.as_str() {
                "high" => provider_prefs.sort = Some(Sort::Throughput), // High effort = high throughput
                "low" | "minimal" => provider_prefs.sort = Some(Sort::Price), // Low effort = low price
                _ => {} // medium or other values don't change sort
            }
        }

        (provider_prefs, include_reasoning)
    }
}

/// OpenAI chat completions endpoint handler - not implemented for Claude Code
pub async fn openai_chat_completions(
    State(_app_state): State<crate::server::AppState>,
    _request: Request,
) -> Result<Json<Value>, StatusCode> {
    info!("OpenAI chat completions → Not implemented");
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// NoOp handler for OpenAI models endpoint
pub async fn openai_models(State(_app_state): State<crate::server::AppState>) -> Json<Value> {
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

/// Handle OAuth authentication with automatic token refresh on failure
async fn handle_oauth_request(
    auth_method: &crate::auth::AuthMethod,
    config: Arc<Mutex<crate::Config>>,
    chat_request: ChatRequest,
    routing_decision: LegacyRoutingDecision,
    parts: axum::http::request::Parts,
) -> Result<axum::response::Response, StatusCode> {

    use crate::auth::anthropic::AnthropicOAuth;

    // Try with cached token first
    let mut oauth_token = match auth_method {
        crate::auth::AuthMethod::OAuth { token, .. } => token.clone(),
        crate::auth::AuthMethod::ApiKey => {
            return Err(error_handling::unauthorized("OAuth method expected but API key method was cached"));
        }
    };

    // Attempt request with current token, refresh and retry if auth fails
    for attempt in 0..2 {
        let mut client = Anthropic::builder().oauth_token(&oauth_token).build();
        
        // Add required OAuth headers
        client = client.header("anthropic-client-id", "9d1c250a-e61b-44d9-88ed-5944d1962f5e");
        client = client.header("anthropic-beta", 
            "oauth-2025-04-20,claude-code-20250219,interleaved-thinking-2025-05-14,fine-grained-tool-streaming-2025-05-14");

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

        // Apply thinking parameters if present in query string
        let processed_request = if let Some(query_params) = &routing_decision.query_params {
            parameter_mapping::apply_anthropic_thinking_params(chat_request.clone(), query_params)
        } else {
            chat_request.clone()
        };

        // Attempt the request
        match handle_anthropic_streaming(&client, &processed_request).await {
            Ok(response) => return Ok(response),
            Err(status) if status == StatusCode::UNAUTHORIZED && attempt == 0 => {
                // Auth failed on first attempt - try to refresh token
                tracing::info!("OAuth request failed with 401, attempting token refresh");
                
                let token_refresh_result = {
                    let mut config_guard = config.lock().await;
                    if let Some(provider) = config_guard.providers.get_mut("anthropic") {
                        AnthropicOAuth::get_valid_access_token(&mut provider.auth, true).await
                    } else {
                        Err(SetuError::Other("No anthropic provider in config".to_string()))
                    }
                };

                match token_refresh_result {
                    Ok(new_token) => {
                        oauth_token = new_token;
                        tracing::info!("Token refreshed, retrying request");
                        continue; // Retry with new token
                    }
                    Err(e) => {
                        tracing::error!("Token refresh failed: {}", e);
                        return Err(status);
                    }
                }
            }
            Err(status) => return Err(status),
        }
    }

    // Should never reach here
    Err(StatusCode::INTERNAL_SERVER_ERROR)
}

/// Handle direct Anthropic API requests (non-Claude Code)
async fn handle_direct_anthropic_request(
    chat_request: ChatRequest,
    routing_decision: LegacyRoutingDecision,
    parts: axum::http::request::Parts,
) -> Result<axum::response::Response, StatusCode> {

    // Debug: Log all incoming headers to understand what Claude CLI sends
    tracing::debug!("Incoming headers: {:?}", parts.headers);

    // Create client with API key from authorization header or x-api-key header
    let api_key = if let Some(auth_header) = parts.headers.get("authorization") {
        tracing::debug!("Found Authorization header");
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(key) = auth_str.strip_prefix("Bearer ") {
                tracing::debug!("Using Bearer token from Authorization header");
                key.to_string()
            } else {
                return Err(error_handling::unauthorized(
                    "Invalid authorization header format - must be 'Bearer <api_key>'",
                ));
            }
        } else {
            return Err(error_handling::unauthorized(
                "Authorization header contains invalid characters",
            ));
        }
    } else if let Some(api_key_header) = parts.headers.get("x-api-key") {
        tracing::debug!("Found x-api-key header");
        if let Ok(key_str) = api_key_header.to_str() {
            tracing::debug!("Using API key from x-api-key header");
            key_str.to_string()
        } else {
            return Err(error_handling::unauthorized(
                "x-api-key header contains invalid characters",
            ));
        }
    } else {
        tracing::debug!("No authentication headers found");
        return Err(error_handling::unauthorized(
            "Missing authentication - provide either Authorization: Bearer <key> or x-api-key: <key> header",
        ));
    };

    let mut client = Anthropic::builder().api_key(api_key).build();

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

    // Apply thinking parameters if present in query string
    let processed_request = if let Some(query_params) = &routing_decision.query_params {
        parameter_mapping::apply_anthropic_thinking_params(chat_request, query_params)
    } else {
        chat_request
    };

    handle_anthropic_streaming(&client, &processed_request).await
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
        let event_stream = stream
            .map(|event_result| match event_result {
                Ok(event) => match serde_json::to_string(&event) {
                    Ok(event_json) => Ok(format!("data: {}\n\n", event_json)),
                    Err(e) => {
                        error_handling::internal_error("Failed to serialize stream event", &e);
                        Err(std::io::Error::new(std::io::ErrorKind::Other, e))
                    }
                },
                Err(e) => {
                    error_handling::bad_gateway("Stream event error", &e);
                    Err(std::io::Error::new(std::io::ErrorKind::Other, e))
                }
            })
            .chain(futures_util::stream::once(async {
                Ok("data: [DONE]\n\n".to_string())
            }));

        // Convert to axum body stream
        let body = axum::body::Body::from_stream(event_stream);

        // Return SSE response
        use axum::http::header;
        use axum::response::Response;

        let response = Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(body)
            .map_err(|e| {
                error_handling::internal_error("Failed to build streaming response", &e)
            })?;
        Ok(response)
    } else {
        // Use send method for non-streaming responses
        match client.send(chat_request).await {
            Ok(response) => {
                let response_json = serde_json::to_value(response).map_err(|e| {
                    error_handling::internal_error("Failed to serialize response", &e)
                })?;
                Ok(Json(response_json).into_response())
            }
            Err(e) => Err(error_handling::bad_gateway("Anthropic request failed", &e)),
        }
    }
}

/// Parse and validate request body
async fn parse_chat_request(body: axum::body::Body) -> Result<ChatRequest, StatusCode> {
    // Get request body as JSON
    let body_bytes = axum::body::to_bytes(body, 1024 * 1024)
        .await
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

            Err(error_handling::bad_request(
                "Invalid ChatRequest JSON in request body",
                &e,
            ))
        }
    }
}

/// Anthropic messages endpoint handler with Claude Code transformation
pub async fn anthropic_messages(
    State(app_state): State<crate::server::AppState>,
    request: Request,
) -> Result<axum::response::Response, StatusCode> {
    let (parts, body) = request.into_parts();

    // Parse request body
    let chat_request = parse_chat_request(body).await?;

    // Route the request based on model name - extract only default provider
    let default_provider = {
        let config_guard = app_state.config.lock().await;
        config_guard.routing.default_provider.clone()
    };
    let router = NameBasedRouter::new_with_default_provider(default_provider);
    let routing_decision = match router.route_model(&chat_request.model) {
        Ok(decision) => decision,
        Err(e) => {
            return Err(error_handling::bad_request(
                &format!("Routing error for model {}", chat_request.model),
                &e,
            ));
        }
    };

    // Handle OpenRouter routing (temporarily returns SERVICE_UNAVAILABLE)
    if routing_decision.provider == "openrouter" {
        return handle_openrouter_request(app_state.config.clone(), chat_request, routing_decision, parts.headers)
            .await;
    }

    // Handle Gemini routing
    if routing_decision.provider == "google" || routing_decision.provider == "gemini" {
        return handle_gemini_request(app_state.config.clone(), chat_request, routing_decision, parts.headers).await;
    }

    // Use cached authentication decision from startup
    let is_claude_code = is_claude_code_request(&parts.headers);
    
    match &app_state.auth_cache.anthropic_method {
        crate::auth::AuthMethod::OAuth { source, .. } => {
            if is_claude_code {
                info!("🔐 Claude Code → OAuth ({}, subscription billing) → {}", source, chat_request.model);
            } else {
                info!("🔐 Direct → OAuth ({}, subscription billing) → {}", source, chat_request.model); 
            }
            handle_oauth_request(&app_state.auth_cache.anthropic_method, app_state.config.clone(), chat_request, routing_decision, parts).await
        }
        crate::auth::AuthMethod::ApiKey => {
            info!("💳 Anthropic → API Key (pay-per-use billing) → {}", chat_request.model);
            handle_direct_anthropic_request(chat_request, routing_decision, parts).await
        }
    }
}

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



/// Map provider name strings to OpenRouter Provider enum values
fn map_provider_name(provider_name: &str) -> Option<openrouter_ox::provider_preference::Provider> {
    use openrouter_ox::provider_preference::Provider;
    
    match provider_name.to_lowercase().as_str() {
        "anthropic" => Some(Provider::Anthropic),
        "openai" => Some(Provider::OpenAI),
        "google" => Some(Provider::Google),
        "groq" => Some(Provider::Groq),
        "fireworks" => Some(Provider::Fireworks),
        "together" => Some(Provider::Together),
        "together2" => Some(Provider::Together2),
        "deepinfra" => Some(Provider::DeepInfra),
        "lepton" => Some(Provider::Lepton),
        "novita" => Some(Provider::Novita),
        "avian" => Some(Provider::Avian),
        "lambda" => Some(Provider::Lambda),
        "azure" => Some(Provider::Azure),
        "modal" => Some(Provider::Modal),
        "anyscale" => Some(Provider::AnyScale),
        "replicate" => Some(Provider::Replicate),
        "perplexity" => Some(Provider::Perplexity),
        "recursal" => Some(Provider::Recursal),
        "octoai" => Some(Provider::OctoAI),
        "deepseek" => Some(Provider::DeepSeek),
        "infermatic" => Some(Provider::Infermatic),
        "ai21" => Some(Provider::AI21),
        "featherless" => Some(Provider::Featherless),
        "inflection" => Some(Provider::Inflection),
        "xai" => Some(Provider::xAI),
        "cloudflare" => Some(Provider::Cloudflare),
        "minimax" => Some(Provider::Minimax),
        "nineteen" => Some(Provider::Nineteen),
        "z-ai" | "zai" => Some(Provider::ZAI),
        "01-ai" | "01ai" => Some(Provider::ZeroOneAI),
        "huggingface" => Some(Provider::HuggingFace),
        "mancer" => Some(Provider::Mancer),
        "mancer2" => Some(Provider::Mancer2),
        "hyperbolic" => Some(Provider::Hyperbolic),
        "hyperbolic2" => Some(Provider::Hyperbolic2),
        "lynn" => Some(Provider::Lynn),
        "lynn2" => Some(Provider::Lynn2),
        "reflection" => Some(Provider::Reflection),
        "aionlabs" => Some(Provider::AionLabs),
        "alibaba" => Some(Provider::Alibaba),
        "atlascloud" => Some(Provider::AtlasCloud),
        "atoma" => Some(Provider::Atoma),
        "baseten" => Some(Provider::BaseTen),
        "cerebras" => Some(Provider::Cerebras),
        "chutes" => Some(Provider::Chutes),
        "crofai" => Some(Provider::CrofAI),
        "crusoe" => Some(Provider::Crusoe),
        "enfer" => Some(Provider::Enfer),
        "friendli" => Some(Provider::Friendli),
        "gmicloud" => Some(Provider::GMICloud),
        "inception" => Some(Provider::Inception),
        "inferencenet" => Some(Provider::InferenceNet),
        "inocloud" => Some(Provider::InoCloud),
        "kluster" => Some(Provider::Kluster),
        "liquid" => Some(Provider::Liquid),
        "meta" => Some(Provider::Meta),
        "moonshot" | "moonshotai" => Some(Provider::MoonshotAI),
        "morph" => Some(Provider::Morph),
        "ncompass" => Some(Provider::NCompass),
        "nebius" => Some(Provider::Nebius),
        "nextbit" => Some(Provider::NextBit),
        "openinference" => Some(Provider::OpenInference),
        "parasail" => Some(Provider::Parasail),
        "phala" => Some(Provider::Phala),
        "siliconflow" => Some(Provider::SiliconFlow),
        "stealth" => Some(Provider::Stealth),
        "switchpoint" => Some(Provider::Switchpoint),
        "targon" => Some(Provider::Targon),
        "ubicloud" => Some(Provider::Ubicloud),
        "venice" => Some(Provider::Venice),
        "wandb" => Some(Provider::WandB),
        "cent-ml" | "centml" => Some(Provider::CentML),
        "sambanova" => Some(Provider::SambaNova),
        "sambanova2" => Some(Provider::SambaNova2),
        _ => None,
    }
}

/// Handle requests routed to OpenRouter
async fn handle_openrouter_request(
    config: Arc<Mutex<Config>>,
    anthropic_request: ChatRequest,
    routing_decision: LegacyRoutingDecision,
    _headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    info!("Anthropic → OpenRouter: {} → {}", anthropic_request.model, routing_decision.model);

    // Create OpenRouter client
    let client = match create_openrouter_client(config.clone()).await {
        Ok(client) => client,
        Err(e) => {
            return Err(error_handling::bad_gateway("Failed to create OpenRouter client", &e));
        }
    };

    // Update the model name to use the routed model (without provider prefix)
    let mut modified_request = anthropic_request;
    modified_request.model = routing_decision.model.clone();

    // Convert Anthropic request to OpenRouter format using explicit conversion
    let mut openrouter_request = match conversion_ox::anthropic_openrouter::anthropic_to_openrouter_request(modified_request) {
        Ok(req) => req,
        Err(e) => {
            return Err(error_handling::bad_request("Failed to convert request to OpenRouter format", &e));
        }
    };

    // Apply provider preferences and query parameters
    let mut provider_prefs = ProviderPreferences {
        allow_fallbacks: None,
        require_parameters: None,
        data_collection: None,
        order: None,
        only: None,
        ignore: None,
        quantizations: None,
        sort: None,
        max_price: None,
    };
    
    let mut include_reasoning: Option<bool> = None;
    let mut has_preferences = false;

    // Apply provider preference (e.g., :nitro, :floor, or specific provider)
    if let Some(ref preference) = routing_decision.provider_preference {
        match preference.as_str() {
            "nitro" => {
                provider_prefs.sort = Some(Sort::Throughput);
                info!("OpenRouter: Using :nitro preference (sort by throughput)");
                has_preferences = true;
            }
            "floor" => {
                provider_prefs.sort = Some(Sort::Price);
                info!("OpenRouter: Using :floor preference (sort by price)");
                has_preferences = true;
            }
            other => {
                // Map to specific provider
                if let Some(provider) = map_provider_name(other) {
                    provider_prefs.only = Some(vec![provider]);
                    info!("OpenRouter: Restricting to provider: {}", other);
                    has_preferences = true;
                } else {
                    tracing::warn!("OpenRouter: Unknown provider preference '{}', ignoring", other);
                }
            }
        }
    }

    // Apply query parameters (e.g., ?reasoning=true&effort=high)
    if let Some(query_params) = &routing_decision.query_params {
        let (modified_prefs, reasoning_flag) = parameter_mapping::apply_openrouter_thinking_params(provider_prefs, query_params);
        provider_prefs = modified_prefs;
        include_reasoning = reasoning_flag;
        
        if reasoning_flag.is_some() || provider_prefs.sort.is_some() {
            has_preferences = true;
            info!("OpenRouter: Applied query parameters: {:?}", query_params);
        }
    }

    // Set provider preferences if we configured anything
    if has_preferences {
        openrouter_request.provider = Some(provider_prefs);
    }
    
    // Set include_reasoning if specified
    if let Some(reasoning) = include_reasoning {
        openrouter_request.include_reasoning = Some(reasoning);
        info!("OpenRouter: Include reasoning = {}", reasoning);
    }

    let is_streaming = openrouter_request.stream.unwrap_or(false);

    if is_streaming {
        // Handle streaming request using OpenRouter client
        let stream = client.stream(&openrouter_request);

        // Convert OpenRouter stream to Anthropic format using the stream converter
        let mut converter = conversion_ox::anthropic_openrouter::streaming::AnthropicOpenRouterStreamConverter::new();
        let converted_stream = stream
            .map(move |chunk_result| match chunk_result {
                Ok(chunk) => {
                    let anthropic_events = converter.convert_chunk(chunk);
                    // Convert each event to SSE format
                    let events_json = anthropic_events
                        .into_iter()
                        .map(|event| match serde_json::to_string(&event) {
                            Ok(json) => Ok(format!("data: {}\n\n", json)),
                            Err(e) => {
                                error_handling::internal_error("Failed to serialize stream event", &e);
                                Err(std::io::Error::new(std::io::ErrorKind::Other, e))
                            }
                        })
                        .collect::<Result<Vec<_>, _>>();
                    
                    match events_json {
                        Ok(events) => Ok(events.join("")),
                        Err(e) => Err(e)
                    }
                },
                Err(e) => {
                    error_handling::bad_gateway("OpenRouter stream error", &e);
                    Err(std::io::Error::new(std::io::ErrorKind::Other, e))
                }
            })
            .chain(futures_util::stream::once(async {
                Ok("data: [DONE]\n\n".to_string())
            }));

        // Create streaming response
        let body = axum::body::Body::from_stream(converted_stream);
        
        use axum::http::header;
        use axum::response::Response;

        let response = Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(body)
            .map_err(|e| {
                error_handling::internal_error("Failed to build streaming response", &e)
            })?;
        Ok(response)
    } else {
        // Handle non-streaming request using OpenRouter client
        match client.send(&openrouter_request).await {
            Ok(openrouter_response) => {
                // Convert OpenRouter response to Anthropic format using explicit conversion
                let anthropic_response = match conversion_ox::anthropic_openrouter::openrouter_to_anthropic_response(openrouter_response) {
                    Ok(resp) => resp,
                    Err(e) => {
                        return Err(error_handling::internal_error("Failed to convert OpenRouter response", &e));
                    }
                };
                let response_json = serde_json::to_value(anthropic_response).map_err(|e| {
                    error_handling::internal_error("Failed to serialize response", &e)
                })?;
                Ok(Json(response_json).into_response())
            }
            Err(e) => Err(error_handling::bad_gateway("OpenRouter request failed", &e)),
        }
    }
}

/// Handle requests routed to Gemini with OAuth preference
async fn handle_gemini_request(
    config: Arc<Mutex<Config>>,
    anthropic_request: ChatRequest,
    routing_decision: LegacyRoutingDecision,
    _headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    info!("Anthropic → Gemini: {} → {}", anthropic_request.model, routing_decision.model);

    // Create Gemini client with OAuth preference
    let client = match create_gemini_client(config.clone()).await {
        Ok(client) => client,
        Err(e) => {
            return Err(error_handling::bad_gateway("Failed to create Gemini client", &e));
        }
    };

    // Update the model name to use the routed model (without provider prefix)
    let mut modified_request = anthropic_request;
    modified_request.model = routing_decision.model.clone();

    let is_streaming = modified_request.stream.unwrap_or(false);
    
    // Convert Anthropic request to Gemini format
    let mut gemini_request = conversion_ox::anthropic_gemini::anthropic_to_gemini_request(modified_request);
    
    // Apply thinking parameters from query string or use defaults for Gemini
    let mut generation_config = gemini_request.generation_config.unwrap_or_default();
    if let Some(query_params) = &routing_decision.query_params {
        // Use query parameters to configure thinking
        if let Some(thinking_config) = parameter_mapping::create_gemini_thinking_config(query_params) {
            generation_config.thinking_config = Some(thinking_config);
        } else {
            // No thinking params specified - use defaults to enable thinking content
            generation_config.thinking_config = Some(gemini_ox::generate_content::ThinkingConfig {
                include_thoughts: true,
                thinking_budget: -1,
            });
        }
    } else {
        // No query params - use defaults to enable thinking content  
        generation_config.thinking_config = Some(gemini_ox::generate_content::ThinkingConfig {
            include_thoughts: true,
            thinking_budget: -1,
        });
    }
    gemini_request.generation_config = Some(generation_config);

    if is_streaming {
        // Handle streaming request using Gemini request
        let stream = gemini_request.stream(&client);

        // Convert Gemini stream to Anthropic format
        let converted_stream = stream
            .map(|event_result| match event_result {
                Ok(gemini_event) => {
                    let anthropic_event = conversion_ox::anthropic_gemini::gemini_to_anthropic_response(gemini_event);
                    match serde_json::to_string(&anthropic_event) {
                        Ok(event_json) => Ok(format!("data: {}\n\n", event_json)),
                        Err(e) => {
                            error_handling::internal_error("Failed to serialize stream event", &e);
                            Err(std::io::Error::new(std::io::ErrorKind::Other, e))
                        }
                    }
                },
                Err(e) => {
                    error_handling::bad_gateway("Gemini stream event error", &e);
                    Err(std::io::Error::new(std::io::ErrorKind::Other, e))
                }
            })
            .chain(futures_util::stream::once(async {
                Ok("data: [DONE]\n\n".to_string())
            }));

        // Create streaming response
        let body = axum::body::Body::from_stream(converted_stream);
        
        use axum::http::header;
        use axum::response::Response;

        let response = Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(body)
            .map_err(|e| {
                error_handling::internal_error("Failed to build streaming response", &e)
            })?;
        Ok(response)
    } else {
        // Handle non-streaming request using Gemini request
        match gemini_request.send(&client).await {
            Ok(gemini_response) => {
                // Convert Gemini response to Anthropic format
                let anthropic_response = conversion_ox::anthropic_gemini::gemini_to_anthropic_response(gemini_response);
                let response_json = serde_json::to_value(anthropic_response).map_err(|e| {
                    error_handling::internal_error("Failed to serialize response", &e)
                })?;
                Ok(Json(response_json).into_response())
            }
            Err(e) => Err(error_handling::bad_gateway("Gemini request failed", &e)),
        }
    }
}

// All Gemini-related functions temporarily removed

/// Create OpenRouter client with optional provider config
async fn create_openrouter_client(config: Arc<Mutex<Config>>) -> Result<OpenRouter, SetuError> {
    // Check for API key first
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .map_err(|_| SetuError::Other("No OPENROUTER_API_KEY environment variable found".to_string()))?;

    info!("💳 OpenRouter → API Key (pay-per-use billing)");

    // Try to use provider config if available, otherwise use defaults
    let config_guard = config.lock().await;
    if let Some(openrouter_provider) = config_guard.providers.get("openrouter") {
        // Use configured endpoint if provided
        let endpoint = &openrouter_provider.endpoint;
        let client = OpenRouter::builder()
            .api_key(api_key)
            .base_url(endpoint)
            .build();
        Ok(client)
    } else {
        // Use default OpenRouter setup
        let client = OpenRouter::builder()
            .api_key(api_key)
            .build();
        Ok(client)
    }
}

/// Create Gemini client with optional provider config (OAuth preferred, API key fallback)
async fn create_gemini_client(config: Arc<Mutex<Config>>) -> Result<Gemini, SetuError> {
    use crate::auth::google::GoogleOAuth;

    // Try OAuth first (Gemini CLI, then setu config)
    
    // 1. Try Gemini CLI OAuth
    if let Ok(gemini_config) = GoogleOAuth::try_gemini_cli_credentials().await {
        if let Some(oauth_token) = gemini_config.oauth_access_token {
            info!("🔐 Gemini → OAuth via Gemini CLI (subscription billing)");
            let client = Gemini::builder()
                .oauth_token(oauth_token)
                .project_id("pioneering-trilogy-xq6tl") // Cloud Code Assist API project
                .build();
            return Ok(client);
        }
    }

    // 2. Try setu config OAuth (if provider configured)
    let config_guard = config.lock().await;
    if let Some(gemini_provider) = config_guard.providers.get("gemini") {
        if let Some(oauth_token) = &gemini_provider.auth.oauth_access_token {
            info!("🔐 Gemini → OAuth via setu config (subscription billing)");
            let client = Gemini::builder()
                .oauth_token(oauth_token.clone())
                .project_id("pioneering-trilogy-xq6tl") // Cloud Code Assist API project
                .build();
            return Ok(client);
        }
    }
    drop(config_guard); // Release lock

    // 3. Fall back to API key from environment
    match Gemini::load_from_env() {
        Ok(client) => {
            info!("💳 Gemini → API Key (pay-per-use billing)");
            Ok(client)
        }
        Err(_) => {
            Err(SetuError::Other(
                "No Gemini credentials found - set GEMINI_API_KEY/GOOGLE_AI_API_KEY environment variable or configure OAuth".to_string()
            ))
        }
    }
}
