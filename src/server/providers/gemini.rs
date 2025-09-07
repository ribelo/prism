use axum::http::{HeaderMap, StatusCode};
use gemini_ox::Gemini;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::auth::google::GoogleOAuth;
use crate::config::Config;
use crate::error::SetuError;
use crate::router::name_based::RoutingDecision;
use crate::server::error_handling;

/// Create Gemini client with appropriate authentication
pub async fn create_gemini_client(config: Arc<Mutex<Config>>) -> Result<Gemini, SetuError> {
    // Try Claude Code OAuth first (highest priority)
    if let Ok(gemini_config) = GoogleOAuth::try_gemini_cli_credentials().await
        && let Some(oauth_token) = gemini_config.oauth_access_token
    {
        info!("🔐 Gemini → OAuth via Gemini CLI (subscription billing)");
        let client = Gemini::builder().oauth_token(&oauth_token).build();
        return Ok(client);
    }

    // Try setu config OAuth
    let config_guard = config.lock().await;
    if let Some(gemini_provider) = config_guard.providers.get("gemini")
        && let Some(oauth_token) = &gemini_provider.auth.oauth_access_token
    {
        info!("🔐 Gemini → OAuth via setu config (subscription billing)");
        let client = Gemini::builder().oauth_token(oauth_token).build();
        return Ok(client);
    }

    // Fallback to API key
    if let Ok(api_key) = std::env::var("GEMINI_API_KEY") {
        info!("🔐 Gemini → API key (pay-per-use billing)");
        let client = Gemini::builder().api_key(&api_key).build();
        return Ok(client);
    }

    Err(SetuError::Other(
        "No Gemini authentication available (OAuth or API key)".to_string(),
    ))
}

/// Handle Gemini requests (converted from OpenAI format)
pub async fn handle_gemini_request_from_openai(
    config: Arc<Mutex<Config>>,
    openai_request: openai_ox::request::ChatRequest,
    routing_decision: RoutingDecision,
    _headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    let gemini_client = match create_gemini_client(config).await {
        Ok(client) => client,
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to create Gemini client",
                &e,
            ));
        }
    };

    // Convert OpenAI → Anthropic → Gemini (using conversion chain)
    // First convert OpenAI to Anthropic format
    let anthropic_request = convert_openai_to_anthropic_request(openai_request)?;

    // Then convert Anthropic to Gemini
    let mut gemini_request =
        conversion_ox::anthropic_gemini::anthropic_to_gemini_request(anthropic_request);

    // Fix model name - use cleaned model without provider prefix
    gemini_request.model = routing_decision.model.clone();

    // Apply URL parameters if present
    if let Some(query_params) = routing_decision.query_params
        && let Some(thinking_config) =
            crate::server::parameter_mapping::create_gemini_thinking_config(&query_params)
    {
        gemini_request.generation_config = Some(gemini_ox::generate_content::GenerationConfig {
            thinking_config: Some(thinking_config),
            ..Default::default()
        });
    }

    if let Some(req_str) = crate::server::error_handling::prepare_gemini_request_log(&gemini_request) {
        tracing::debug!(target: "setu::request", "Outgoing Gemini (from OpenAI) request (detailed): {}", req_str);
    }

    // Send request to Gemini
    match gemini_request.send(&gemini_client).await {
        Ok(response) => {
            if let Some(resp_str) = crate::server::error_handling::prepare_response_log(&response) {
                tracing::debug!(target: "setu::response", "Gemini response: {}", resp_str);
            }
            // Convert Gemini response back to OpenAI format (via Anthropic)
            // This is a simplified conversion - full implementation would need proper format conversion
            let json_body = match serde_json::to_string(&response) {
                Ok(body) => body,
                Err(e) => {
                    return Err(error_handling::internal_error(
                        "Failed to serialize Gemini response",
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
        Err(e) => Err(error_handling::internal_error(
            "Gemini API request failed",
            &e,
        )),
    }
}

/// Handle Gemini requests (from Anthropic format)
pub async fn handle_gemini_request(
    config: Arc<Mutex<Config>>,
    anthropic_request: anthropic_ox::ChatRequest,
    routing_decision: RoutingDecision,
    _headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    let gemini_client = match create_gemini_client(config).await {
        Ok(client) => client,
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to create Gemini client",
                &e,
            ));
        }
    };

    // Convert Anthropic to Gemini format
    let mut gemini_request =
        conversion_ox::anthropic_gemini::anthropic_to_gemini_request(anthropic_request);

    // Fix model name - use cleaned model without provider prefix
    gemini_request.model = routing_decision.model.clone();

    // Apply URL parameters if present
    if let Some(query_params) = routing_decision.query_params
        && let Some(thinking_config) =
            crate::server::parameter_mapping::create_gemini_thinking_config(&query_params)
    {
        gemini_request.generation_config = Some(gemini_ox::generate_content::GenerationConfig {
            thinking_config: Some(thinking_config),
            ..Default::default()
        });
    }

    if let Some(req_str) = crate::server::error_handling::prepare_gemini_request_log(&gemini_request) {
        tracing::debug!(target: "setu::request", "Outgoing Gemini request (detailed): {}", req_str);
    }

    // Send request to Gemini
    match gemini_request.send(&gemini_client).await {
        Ok(gemini_response) => {
            // Convert Gemini response back to Anthropic format
            let anthropic_response =
                conversion_ox::anthropic_gemini::gemini_to_anthropic_response(gemini_response);

            // Serialize and return Anthropic-format response
            let json_body = match serde_json::to_string(&anthropic_response) {
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
        Err(e) => Err(error_handling::internal_error(
            "Gemini API request failed",
            &e,
        )),
    }
}

/// Handle Gemini → OpenRouter conversion chain
pub async fn handle_openrouter_from_gemini(
    config: Arc<Mutex<Config>>,
    gemini_request_value: serde_json::Value,
    model: &str,
    routing_decision: RoutingDecision,
    _headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    // Convert: Gemini JSON → Anthropic → OpenRouter (using simple conversion for now)
    let anthropic_request =
        convert_gemini_json_to_anthropic_request(gemini_request_value.clone(), model.to_string())?;

    let openrouter_request =
        match conversion_ox::anthropic_openrouter::anthropic_to_openrouter_request(
            anthropic_request,
        ) {
            Ok(mut req) => {
                // Fix the model name - use cleaned model without provider prefix
                req.model = routing_decision.model.clone();
                req
            }
            Err(e) => {
                return Err(error_handling::internal_error(
                    "Failed to convert Anthropic request to OpenRouter format",
                    &e,
                ));
            }
        };

    // Create OpenRouter client and send request
    let openrouter_client =
        match crate::server::providers::openrouter::create_openrouter_client(config).await {
            Ok(client) => client,
            Err(e) => {
                return Err(error_handling::internal_error(
                    "Failed to create OpenRouter client",
                    &e,
                ));
            }
        };

    // Apply URL parameters if present
    let final_request = if let Some(query_params) = routing_decision.query_params {
        let (updated_request, _) = crate::server::parameter_mapping::apply_openrouter_parameters(
            openrouter_request,
            &query_params,
        );
        updated_request
    } else {
        openrouter_request
    };

    if let Some(req_str) = crate::server::error_handling::prepare_openrouter_request_log(&final_request) {
        tracing::debug!(target: "setu::request", "Outgoing OpenRouter (from Gemini) request (detailed): {}", req_str);
    }

    // Send to OpenRouter
    match openrouter_client.send(&final_request).await {
        Ok(openrouter_response) => {
            // Convert: OpenRouter → Anthropic → Gemini
            let anthropic_response =
                match conversion_ox::anthropic_openrouter::openrouter_to_anthropic_response(
                    openrouter_response,
                ) {
                    Ok(resp) => resp,
                    Err(e) => {
                        return Err(error_handling::internal_error(
                            "Failed to convert OpenRouter response to Anthropic format",
                            &e,
                        ));
                    }
                };

            let gemini_response = convert_anthropic_to_gemini_response(anthropic_response);

            // Serialize and return Gemini-format response
            let json_body = match serde_json::to_string(&gemini_response) {
                Ok(body) => body,
                Err(e) => {
                    return Err(error_handling::internal_error(
                        "Failed to serialize Gemini response",
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
        Err(e) => Err(error_handling::internal_error(
            "OpenRouter API request failed",
            &e,
        )),
    }
}

/// Handle Gemini → Anthropic conversion
pub async fn handle_anthropic_from_gemini(
    config: Arc<Mutex<Config>>,
    gemini_request_value: serde_json::Value,
    model: &str,
    routing_decision: RoutingDecision,
    _headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    // Convert: Gemini JSON → Anthropic (using simple conversion for now)
    let mut anthropic_request =
        convert_gemini_json_to_anthropic_request(gemini_request_value.clone(), model.to_string())?;

    // Fix the model name - use cleaned model without provider prefix
    anthropic_request.model = routing_decision.model.clone();

    // Apply URL parameters if present
    if let Some(query_params) = routing_decision.query_params {
        anthropic_request = crate::server::parameter_mapping::apply_anthropic_parameters(
            anthropic_request,
            &query_params,
        );
    }

    // Create Anthropic client and send request
    let anthropic_client =
        match crate::server::providers::anthropic::create_anthropic_client(config, true).await {
            Ok(client) => client,
            Err(e) => {
                return Err(error_handling::internal_error(
                    "Failed to create Anthropic client",
                    &e,
                ));
            }
        };

    if let Some(req_str) = crate::server::error_handling::prepare_anthropic_request_log(&anthropic_request) {
        tracing::debug!(target: "setu::request", "Outgoing Anthropic (from Gemini) request: {}", req_str);
    }

    // Send to Anthropic
    match anthropic_client.send(&anthropic_request).await {
        Ok(anthropic_response) => {
            // Convert: Anthropic → Gemini
            let gemini_response = convert_anthropic_to_gemini_response(anthropic_response);

            // Serialize and return Gemini-format response
            let json_body = match serde_json::to_string(&gemini_response) {
                Ok(body) => body,
                Err(e) => {
                    return Err(error_handling::internal_error(
                        "Failed to serialize Gemini response",
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
        Err(e) => Err(error_handling::internal_error(
            "Anthropic API request failed",
            &e,
        )),
    }
}

/// Handle direct native Gemini requests
pub async fn handle_direct_gemini_request(
    config: Arc<Mutex<Config>>,
    gemini_request_value: serde_json::Value,
    model: &str,
    routing_decision: RoutingDecision,
    _headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    let gemini_client = match create_gemini_client(config).await {
        Ok(client) => client,
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to create Gemini client",
                &e,
            ));
        }
    };

    // Parse the JSON value into a proper Gemini request
    let mut gemini_request = parse_gemini_json_to_request(gemini_request_value, model.to_string())?;

    // Apply URL parameters if present
    if let Some(query_params) = routing_decision.query_params
        && let Some(thinking_config) =
            crate::server::parameter_mapping::create_gemini_thinking_config(&query_params)
    {
        gemini_request.generation_config = Some(gemini_ox::generate_content::GenerationConfig {
            thinking_config: Some(thinking_config),
            ..Default::default()
        });
    }

    if let Some(req_str) = crate::server::error_handling::prepare_gemini_request_log(&gemini_request) {
        tracing::debug!(target: "setu::request", "Outgoing Gemini (direct) request: {}", req_str);
    }

    // Send request to Gemini
    match gemini_request.send(&gemini_client).await {
        Ok(response) => {
            if let Some(resp_str) = crate::server::error_handling::prepare_response_log(&response) {
                tracing::debug!(target: "setu::response", "Gemini response: {}", resp_str);
            }
            // Return native Gemini format response
            let json_body = match serde_json::to_string(&response) {
                Ok(body) => body,
                Err(e) => {
                    return Err(error_handling::internal_error(
                        "Failed to serialize Gemini response",
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
        Err(e) => Err(error_handling::internal_error(
            "Gemini API request failed",
            &e,
        )),
    }
}

/// Parse JSON value into Gemini GenerateContentRequest
fn parse_gemini_json_to_request(
    json_value: serde_json::Value,
    model: String,
) -> Result<gemini_ox::generate_content::request::GenerateContentRequest, StatusCode> {
    use gemini_ox::content::{Content, Part, PartData, Role, Text};
    use gemini_ox::generate_content::request::GenerateContentRequest;

    // Extract contents array
    let contents_array = match json_value.get("contents").and_then(|v| v.as_array()) {
        Some(contents) => contents,
        None => {
            return Err(error_handling::bad_request(
                "Missing or invalid contents field",
                &"Contents field is required",
            ));
        }
    };

    // Convert JSON to Gemini contents
    let mut gemini_contents = Vec::new();
    for content_value in contents_array {
        if let (Some(role_str), Some(parts_array)) = (
            content_value.get("role").and_then(|v| v.as_str()),
            content_value.get("parts").and_then(|v| v.as_array()),
        ) {
            let role = match role_str {
                "user" => Role::User,
                "model" => Role::Model,
                _ => continue,
            };

            let mut parts = Vec::new();
            for part_value in parts_array {
                if let Some(text) = part_value.get("text").and_then(|v| v.as_str()) {
                    parts.push(Part::new(PartData::Text(Text::new(text))));
                }
                // Could add support for other part types (images, etc.) here
            }

            if !parts.is_empty() {
                gemini_contents.push(Content { role, parts });
            }
        }
    }

    if gemini_contents.is_empty() {
        return Err(error_handling::bad_request(
            "No valid contents found",
            &"At least one content item is required",
        ));
    }

    // Build the request
    let request = GenerateContentRequest::builder()
        .model(model)
        .content_list(gemini_contents)
        .build();

    Ok(request)
}

/// Simple Gemini JSON to Anthropic conversion
fn convert_gemini_json_to_anthropic_request(
    json_value: serde_json::Value,
    model: String,
) -> Result<anthropic_ox::ChatRequest, StatusCode> {
    // Extract contents array
    let contents_array = match json_value.get("contents").and_then(|v| v.as_array()) {
        Some(contents) => contents,
        None => {
            return Err(error_handling::bad_request(
                "Missing or invalid contents field",
                &"Contents field is required",
            ));
        }
    };

    // Convert to Anthropic messages
    let mut anthropic_messages = Vec::new();
    for content_value in contents_array {
        if let (Some(role_str), Some(parts_array)) = (
            content_value.get("role").and_then(|v| v.as_str()),
            content_value.get("parts").and_then(|v| v.as_array()),
        ) {
            let role = match role_str {
                "user" => anthropic_ox::message::Role::User,
                "model" => anthropic_ox::message::Role::Assistant,
                _ => continue,
            };

            let mut message_content = String::new();
            for part_value in parts_array {
                if let Some(text) = part_value.get("text").and_then(|v| v.as_str()) {
                    if !message_content.is_empty() {
                        message_content.push('\n');
                    }
                    message_content.push_str(text);
                }
            }

            if !message_content.is_empty() {
                anthropic_messages.push(anthropic_ox::message::Message {
                    role,
                    content: anthropic_ox::message::StringOrContents::String(message_content),
                });
            }
        }
    }

    if anthropic_messages.is_empty() {
        return Err(error_handling::bad_request(
            "No valid messages found",
            &"At least one message is required",
        ));
    }

    // Build Anthropic request with defaults
    Ok(anthropic_ox::ChatRequest::builder()
        .model(model)
        .max_tokens(1000) // Default max_tokens
        .messages(anthropic_ox::message::Messages(anthropic_messages))
        .build())
}

/// Simple Anthropic to Gemini response conversion
fn convert_anthropic_to_gemini_response(
    anthropic_response: anthropic_ox::ChatResponse,
) -> serde_json::Value {
    use serde_json::json;

    // Extract text content from Anthropic response
    let text_content = anthropic_response
        .content
        .into_iter()
        .filter_map(|content| match content {
            anthropic_ox::message::Content::Text(text) => Some(text.text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    let total_tokens = anthropic_response.usage.input_tokens.unwrap_or(0)
        + anthropic_response.usage.output_tokens.unwrap_or(0);

    // Build a simple Gemini-compatible response
    json!({
        "candidates": [{
            "content": {
                "parts": [{
                    "text": text_content
                }],
                "role": "model"
            },
            "finishReason": "STOP",
            "index": 0
        }],
        "usageMetadata": {
            "promptTokenCount": anthropic_response.usage.input_tokens.unwrap_or(0),
            "candidatesTokenCount": anthropic_response.usage.output_tokens.unwrap_or(0),
            "totalTokenCount": total_tokens
        }
    })
}

/// Convert OpenAI request to Anthropic format
fn convert_openai_to_anthropic_request(
    openai_request: openai_ox::request::ChatRequest,
) -> Result<anthropic_ox::ChatRequest, StatusCode> {
    // This is a simplified conversion - a full implementation would need proper format conversion
    let mut anthropic_messages = Vec::new();

    for message in openai_request.messages {
        match message.role {
            ai_ox_common::openai_format::MessageRole::System => {
                // System messages in OpenAI become system instruction in Anthropic
                // We'll handle this separately or convert to user message
                if let Some(content) = message.content {
                    anthropic_messages.push(anthropic_ox::message::Message {
                        role: anthropic_ox::message::Role::User,
                        content: anthropic_ox::message::StringOrContents::Contents(vec![
                            anthropic_ox::message::Content::Text(anthropic_ox::message::Text::new(
                                format!("System: {}", content),
                            )),
                        ]),
                    });
                }
            }
            ai_ox_common::openai_format::MessageRole::User => {
                if let Some(content) = message.content {
                    anthropic_messages.push(anthropic_ox::message::Message {
                        role: anthropic_ox::message::Role::User,
                        content: anthropic_ox::message::StringOrContents::Contents(vec![
                            anthropic_ox::message::Content::Text(anthropic_ox::message::Text::new(
                                content,
                            )),
                        ]),
                    });
                }
            }
            ai_ox_common::openai_format::MessageRole::Assistant => {
                if let Some(content) = message.content {
                    anthropic_messages.push(anthropic_ox::message::Message {
                        role: anthropic_ox::message::Role::Assistant,
                        content: anthropic_ox::message::StringOrContents::Contents(vec![
                            anthropic_ox::message::Content::Text(anthropic_ox::message::Text::new(
                                content,
                            )),
                        ]),
                    });
                }
            }
            ai_ox_common::openai_format::MessageRole::Tool => {
                // Skip tool messages for now - would need proper conversion
                continue;
            }
        }
    }

    Ok(anthropic_ox::ChatRequest::builder()
        .model(openai_request.model)
        .messages(anthropic_ox::message::Messages(anthropic_messages))
        .build())
}
