use axum::http::{HeaderMap, StatusCode};
use openrouter_ox::OpenRouter;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::config::Config;
use crate::error::SetuError;
use crate::router::name_based::RoutingDecision;
use crate::server::error_handling;

/// OpenAI models endpoint - return simple model list
pub async fn openai_models() -> axum::response::Json<Value> {
    axum::response::Json(json!({
        "object": "list",
        "data": [
            {
                "id": "anthropic/claude-3-5-sonnet",
                "object": "model",
                "created": 1677610602,
                "owned_by": "setu-router"
            },
            {
                "id": "openrouter/openai/gpt-4o",
                "object": "model",
                "created": 1677610602,
                "owned_by": "setu-router"
            }
        ]
    }))
}

/// Create OpenRouter client with API key authentication
pub async fn create_openrouter_client(config: Arc<Mutex<Config>>) -> Result<OpenRouter, SetuError> {
    // Try OPENROUTER_API_KEY first (correct OpenRouter key)
    if let Ok(api_key) = std::env::var("OPENROUTER_API_KEY") {
        info!("🔐 OpenRouter → API key via OPENROUTER_API_KEY");
        return Ok(OpenRouter::builder().api_key(&api_key).build());
    }
    
    // Fallback to OPENAI_API_KEY for compatibility
    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        info!("🔐 OpenRouter → API key via OPENAI_API_KEY (fallback)");
        return Ok(OpenRouter::builder().api_key(&api_key).build());
    }

    // Try config file
    let config_guard = config.lock().await;
    if let Some(openai_provider) = config_guard.providers.get("openai")
        && let Some(api_key) = &openai_provider.api_key {
            info!("🔐 OpenRouter → API key via setu config");
            return Ok(OpenRouter::builder().api_key(api_key).build());
        }

    Err(SetuError::Other(
        "No OpenRouter API key found (OPENROUTER_API_KEY or OPENAI_API_KEY environment variable or setu config)".to_string(),
    ))
}

/// Handle OpenRouter requests from OpenAI format
pub async fn handle_openrouter_request_from_openai(
    config: Arc<Mutex<Config>>,
    openai_request: openai_ox::request::ChatRequest,
    routing_decision: RoutingDecision,
    _headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    let openrouter_client = match create_openrouter_client(config).await {
        Ok(client) => client,
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to create OpenRouter client",
                &e,
            ));
        }
    };

    // Convert OpenAI request to OpenRouter format
    // OpenRouter uses Anthropic-style content arrays while OpenAI uses strings
    let mut openrouter_messages = Vec::new();
    
    for message in openai_request.messages {
        let openrouter_message = match message.role {
            ai_ox_common::openai_format::MessageRole::System => {
                if let Some(content) = message.content {
                    openrouter_ox::message::Message::System(
                        openrouter_ox::message::SystemMessage::text(content)
                    )
                } else {
                    continue;
                }
            },
            ai_ox_common::openai_format::MessageRole::User => {
                if let Some(content) = message.content {
                    openrouter_ox::message::Message::User(
                        openrouter_ox::message::UserMessage::text(content)
                    )
                } else {
                    continue;
                }
            },
            ai_ox_common::openai_format::MessageRole::Assistant => {
                let mut assistant_msg = if let Some(content) = message.content {
                    openrouter_ox::message::AssistantMessage::text(content)
                } else {
                    openrouter_ox::message::AssistantMessage::new(Vec::<openrouter_ox::message::ContentPart>::new())
                };
                
                // Handle tool calls
                if let Some(tool_calls) = message.tool_calls {
                    assistant_msg.tool_calls = Some(tool_calls.into_iter().map(|tc| {
                        openrouter_ox::response::ToolCall {
                            index: None,
                            id: Some(tc.id),
                            type_field: tc.r#type,
                            function: openrouter_ox::response::FunctionCall {
                                name: Some(tc.function.name),
                                arguments: tc.function.arguments,
                            },
                        }
                    }).collect());
                }
                
                openrouter_ox::message::Message::Assistant(assistant_msg)
            },
            ai_ox_common::openai_format::MessageRole::Tool => {
                if let (Some(content), Some(tool_call_id)) = (message.content, message.tool_call_id) {
                    openrouter_ox::message::Message::Tool(
                        openrouter_ox::message::ToolMessage::with_name(
                            tool_call_id,
                            content,
                            message.name.unwrap_or_else(|| "unknown".to_string())
                        )
                    )
                } else {
                    continue;
                }
            },
        };
        openrouter_messages.push(openrouter_message);
    }
    
    // Build OpenRouter request - use routing decision model (without provider prefix)
    let mut openrouter_request = openrouter_ox::request::ChatRequest::new(
        routing_decision.model.clone(),
        openrouter_messages,
    );
    
    // Copy over other fields
    openrouter_request.temperature = openai_request.temperature.map(|t| t as f64);
    openrouter_request.max_tokens = openai_request.max_tokens;
    openrouter_request.top_p = openai_request.top_p.map(|tp| tp as f64);
    openrouter_request.stop = openai_request.stop;
    
    // Convert tools if present
    if let Some(tools) = openai_request.tools {
        openrouter_request.tools = Some(tools);
    }

    // Apply URL parameters to the request if present
    if let Some(query_params) = routing_decision.query_params {
        let (updated_request, _) = crate::server::parameter_mapping::apply_openrouter_parameters(openrouter_request, &query_params);
        openrouter_request = updated_request;
    }

    // Send request to OpenRouter
    match openrouter_client.send(&openrouter_request).await {
        Ok(response) => {
            // Serialize response to JSON and return (OpenRouter response format is OpenAI-compatible)
            let json_body = match serde_json::to_string(&response) {
                Ok(body) => body,
                Err(e) => {
                    return Err(error_handling::internal_error(
                        "Failed to serialize OpenRouter response",
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
            Err(error_handling::internal_error(
                "OpenRouter API request failed",
                &e,
            ))
        }
    }
}

/// Handle direct OpenRouter requests (from Anthropic format)
pub async fn handle_openrouter_request(
    config: Arc<Mutex<Config>>,
    anthropic_request: anthropic_ox::ChatRequest,
    routing_decision: RoutingDecision,
    _headers: HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    let openrouter_client = match create_openrouter_client(config).await {
        Ok(client) => client,
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to create OpenRouter client",
                &e,
            ));
        }
    };

    // Convert Anthropic request to OpenRouter format
    let mut openrouter_request = match conversion_ox::anthropic_openrouter::anthropic_to_openrouter_request(anthropic_request) {
        Ok(mut req) => {
            // Fix the model name - use cleaned model without provider prefix
            req.model = routing_decision.model.clone();
            req
        },
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to convert Anthropic request to OpenRouter format",
                &e,
            ));
        }
    };

    // Apply URL parameters to the request if present
    if let Some(query_params) = routing_decision.query_params {
        let (updated_request, _) = crate::server::parameter_mapping::apply_openrouter_parameters(openrouter_request, &query_params);
        openrouter_request = updated_request;
    }

    // Send request to OpenRouter
    match openrouter_client.send(&openrouter_request).await {
        Ok(openrouter_response) => {
            // Convert OpenRouter response back to Anthropic format
            let anthropic_response = match conversion_ox::anthropic_openrouter::openrouter_to_anthropic_response(openrouter_response) {
                Ok(resp) => resp,
                Err(e) => {
                    return Err(error_handling::internal_error(
                        "Failed to convert OpenRouter response to Anthropic format",
                        &e,
                    ));
                }
            };

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
        Err(e) => {
            Err(error_handling::internal_error(
                "OpenRouter API request failed",
                &e,
            ))
        }
    }
}