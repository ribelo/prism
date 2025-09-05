use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;

use crate::router::model_router::ModelRouter;
use regex::Regex;
use crate::server::error_handling;
use crate::server::providers::{anthropic, auth, gemini, openrouter, parsing};

/// Main OpenAI chat completions endpoint handler
pub async fn openai_chat_completions(
    State(app_state): State<crate::server::AppState>,
    request: Request,
) -> Result<axum::response::Response, StatusCode> {
    // Check for config changes
    crate::server::check_and_reload_config(&app_state).await;

    let (parts, body) = request.into_parts();

    // Parse OpenAI-format request body (proper OpenAI format with string content)
    let openai_request = parsing::parse_openai_chat_request(body).await?;
    if let Some(in_str) = crate::server::error_handling::prepare_request_log(&openai_request) {
        tracing::debug!(target: "setu::incoming", "Incoming OpenAI chat request: {}", in_str);
    }

    // Route based on model name
    let config = app_state.config.lock().await.clone();
    let router = ModelRouter::new(config);
    let routing_decision = match router.route_model(&openai_request.model) {
        Ok(decisions) => {
            // Use the first routing decision (primary route)
            decisions.into_iter().next().ok_or_else(|| {
                error_handling::internal_error(
                    "No routing decisions available",
                    &"Failed to get routing decision",
                )
            })?
        }
        Err(e) => {
            return Err(error_handling::bad_request(
                &format!("Routing error for model {}", openai_request.model),
                &e,
            ));
        }
    };

    // Route to appropriate provider
    match routing_decision.provider.as_str() {
        "openrouter" => {
            openrouter::handle_openrouter_request_from_openai(
                app_state.config.clone(),
                openai_request,
                routing_decision,
                parts.headers,
            )
            .await
        }
        "gemini" | "google" => {
            gemini::handle_gemini_request_from_openai(
                app_state.config.clone(),
                openai_request,
                routing_decision,
                parts.headers,
            )
            .await
        }
        "anthropic" => Err(error_handling::internal_error(
            "OpenAI → Anthropic conversion not yet implemented",
            &"Direct Anthropic provider not supported from OpenAI endpoint yet",
        )),
        provider_type => Err(error_handling::internal_error(
            "Custom providers not yet supported from OpenAI endpoint",
            &format!("Provider type: {}", provider_type),
        )),
    }
}

/// OpenAI-compatible models endpoint (returns OpenRouter models)
pub async fn openai_models(State(_app_state): State<crate::server::AppState>) -> Json<Value> {
    openrouter::openai_models().await
}

/// Main Anthropic messages endpoint handler
pub async fn anthropic_messages(
    State(app_state): State<crate::server::AppState>,
    request: Request,
) -> Result<axum::response::Response, StatusCode> {
    // Check for config changes
    crate::server::check_and_reload_config(&app_state).await;

    let (parts, body) = request.into_parts();

    // Parse Anthropic-format request body
    let anthropic_request = parsing::parse_chat_request(body).await?;
    // Inspect Anthropic system message for explicit model ID hints
    {
        fn collect_strings(v: &serde_json::Value, out: &mut Vec<String>) {
            match v {
                serde_json::Value::String(s) => out.push(s.clone()),
                serde_json::Value::Array(arr) => {
                    for item in arr { collect_strings(item, out); }
                }
                serde_json::Value::Object(map) => {
                    for val in map.values() { collect_strings(val, out); }
                }
                _ => {}
            }
        }

        if let Ok(val) = serde_json::to_value(&anthropic_request) {
            if let Some(system_val) = val.get("system") {
                let mut texts = Vec::new();
                collect_strings(system_val, &mut texts);

                let re = Regex::new(r"The exact model ID is\s+([A-Za-z0-9._:/-]+)").unwrap();
                for text in texts {
                    for caps in re.captures_iter(&text) {
                        let model_id = &caps[1];
                        tracing::info!("The exact model ID is {}", model_id);
                    }
                }
            }
        }
    }
    if let Some(in_str) = crate::server::error_handling::prepare_request_log(&anthropic_request) {
        tracing::debug!(target: "setu::incoming", "Incoming Anthropic messages request: {}", in_str);
    }

    // Route based on model name
    let config = app_state.config.lock().await.clone();
    let router = ModelRouter::new(config);
    let routing_decision = match router.route_model(&anthropic_request.model) {
        Ok(decisions) => {
            // Use the first routing decision (primary route)
            decisions.into_iter().next().ok_or_else(|| {
                error_handling::internal_error(
                    "No routing decisions available",
                    &"Failed to get routing decision",
                )
            })?
        }
        Err(e) => {
            return Err(error_handling::bad_request(
                &format!("Routing error for model {}", anthropic_request.model),
                &e,
            ));
        }
    };


    // Check cached authentication FIRST for Anthropic provider
    if routing_decision.provider == "anthropic" {
        let is_claude_code = auth::is_claude_code_request(&parts.headers);

        match &app_state.auth_cache.anthropic_method {
            crate::auth::AuthMethod::OAuth { source, .. } => {
                if is_claude_code {
                    tracing::info!(
                        "🔐 Claude Code → OAuth ({}, subscription billing) → {}",
                        source,
                        anthropic_request.model
                    );
                } else {
                    tracing::info!(
                        "🔐 Direct → OAuth ({}, subscription billing) → {}",
                        source,
                        anthropic_request.model
                    );
                }
                return auth::handle_oauth_request(
                    &app_state.auth_cache.anthropic_method,
                    app_state.config.clone(),
                    anthropic_request,
                    routing_decision,
                    parts,
                )
                .await;
            }
            crate::auth::AuthMethod::ApiKey => {
                tracing::info!(
                    "💳 Anthropic → API Key (pay-per-use billing) → {}",
                    anthropic_request.model
                );
                // Fall through to regular provider routing
            }
            crate::auth::AuthMethod::Unavailable { reason } => {
                tracing::error!("Anthropic authentication unavailable: {}", reason);
                return Err(error_handling::unauthorized(&format!(
                    "Anthropic authentication unavailable: {}",
                    reason
                )));
            }
        }
    }

    // Route to appropriate provider
    match routing_decision.provider.as_str() {
        "anthropic" => {
            anthropic::handle_direct_anthropic_request(
                app_state.config.clone(),
                anthropic_request,
                routing_decision,
                parts.headers,
            )
            .await
        }
        "openrouter" => {
            openrouter::handle_openrouter_request(
                app_state.config.clone(),
                anthropic_request,
                routing_decision,
                parts.headers,
            )
            .await
        }
        "gemini" | "google" => {
            gemini::handle_gemini_request(
                app_state.config.clone(),
                anthropic_request,
                routing_decision,
                parts.headers,
            )
            .await
        }
        provider_type => Err(error_handling::internal_error(
            "Custom providers not yet supported from Anthropic endpoint",
            &format!("Provider type: {}", provider_type),
        )),
    }
}

/// Main Gemini generateContent endpoint handler
pub async fn gemini_generate_content(
    State(app_state): State<crate::server::AppState>,
    Path(model_path): Path<String>,
    request: Request,
) -> Result<axum::response::Response, StatusCode> {
    // Check for config changes
    crate::server::check_and_reload_config(&app_state).await;

    let (parts, body) = request.into_parts();

    // Extract model from path like "gemini-1.5-flash:generateContent"
    let model = if let Some(model) = model_path.strip_suffix(":generateContent") {
        if model.trim().is_empty() {
            return Err(error_handling::bad_request(
                "Empty model name in Gemini endpoint",
                &"Model name cannot be empty in URL path",
            ));
        }
        tracing::debug!("Extracted model from Gemini URL path: {}", model);
        model
    } else {
        return Err(error_handling::bad_request(
            "Invalid Gemini endpoint format",
            &"Expected format: /v1beta/models/{model}:generateContent",
        ));
    };

    // Parse Gemini-format request body (no model field expected)
    let gemini_request_value = parsing::parse_gemini_request(body).await?;
    if let Some(in_str) = crate::server::error_handling::prepare_request_log(&gemini_request_value) {
        tracing::debug!(target: "setu::incoming", "Incoming Gemini generateContent request: {}", in_str);
    }

    // Route based on model name
    let config = app_state.config.lock().await.clone();
    let router = ModelRouter::new(config);
    let routing_decision = match router.route_model(model) {
        Ok(decisions) => {
            // Use the first routing decision (primary route)
            decisions.into_iter().next().ok_or_else(|| {
                error_handling::internal_error(
                    "No routing decisions available",
                    &"Failed to get routing decision",
                )
            })?
        }
        Err(e) => {
            return Err(error_handling::bad_request(
                &format!("Routing error for model {}", model),
                &e,
            ));
        }
    };

    // Route to appropriate provider
    match routing_decision.provider.as_str() {
        "gemini" | "google" => {
            gemini::handle_direct_gemini_request(
                app_state.config.clone(),
                gemini_request_value,
                model,
                routing_decision,
                parts.headers,
            )
            .await
        }
        "openrouter" => {
            gemini::handle_openrouter_from_gemini(
                app_state.config.clone(),
                gemini_request_value.clone(),
                model,
                routing_decision,
                parts.headers,
            )
            .await
        }
        "anthropic" => {
            gemini::handle_anthropic_from_gemini(
                app_state.config.clone(),
                gemini_request_value.clone(),
                model,
                routing_decision,
                parts.headers,
            )
            .await
        }
        provider_type => Err(error_handling::internal_error(
            "Custom providers not yet supported from Gemini endpoint",
            &format!("Provider type: {}", provider_type),
        )),
    }
}
