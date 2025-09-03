use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;

use crate::router::name_based::NameBasedRouter;
use crate::server::error_handling;
use crate::server::providers::{anthropic, gemini, openrouter, parsing, auth};

/// Main OpenAI chat completions endpoint handler
pub async fn openai_chat_completions(
    State(app_state): State<crate::server::AppState>,
    request: Request,
) -> Result<axum::response::Response, StatusCode> {
    let (parts, body) = request.into_parts();

    // Parse OpenAI-format request body (proper OpenAI format with string content)
    let openai_request = parsing::parse_openai_chat_request(body).await?;

    // Route based on model name
    let router = NameBasedRouter::new(Default::default());
    let routing_decision = match router.route_model(&openai_request.model) {
        Ok(decision) => decision,
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
                parts.headers
            ).await
        }
        "gemini" | "google" => {
            gemini::handle_gemini_request_from_openai(
                app_state.config.clone(), 
                openai_request, 
                routing_decision, 
                parts.headers
            ).await
        }
        "anthropic" => {
            Err(error_handling::internal_error(
                "OpenAI → Anthropic conversion not yet implemented", 
                &"Direct Anthropic provider not supported from OpenAI endpoint yet"
            ))
        }
        provider_type => {
            Err(error_handling::internal_error(
                "Custom providers not yet supported from OpenAI endpoint",
                &format!("Provider type: {}", provider_type)
            ))
        }
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
    let (parts, body) = request.into_parts();

    // Parse Anthropic-format request body
    let anthropic_request = parsing::parse_chat_request(body).await?;

    // Route based on model name
    let router = NameBasedRouter::new(Default::default());
    let routing_decision = match router.route_model(&anthropic_request.model) {
        Ok(decision) => decision,
        Err(e) => {
            return Err(error_handling::bad_request(
                &format!("Routing error for model {}", anthropic_request.model),
                &e,
            ));
        }
    };

    // Route to appropriate provider
    match routing_decision.provider.as_str() {
        "anthropic" => {
            anthropic::handle_direct_anthropic_request(
                app_state.config.clone(),
                anthropic_request,
                routing_decision,
                parts.headers,
            ).await
        }
        "openrouter" => {
            openrouter::handle_openrouter_request(
                app_state.config.clone(),
                anthropic_request,
                routing_decision,
                parts.headers,
            ).await
        }
        "gemini" | "google" => {
            gemini::handle_gemini_request(
                app_state.config.clone(),
                anthropic_request,
                routing_decision,
                parts.headers,
            ).await
        }
        "oauth" => {
            auth::handle_oauth_request(
                app_state.config.clone(),
                routing_decision,
                parts.headers,
            ).await
        }
        provider_type => {
            Err(error_handling::internal_error(
                "Custom providers not yet supported from Anthropic endpoint",
                &format!("Provider type: {}", provider_type)
            ))
        }
    }
}

/// Main Gemini generateContent endpoint handler
pub async fn gemini_generate_content(
    State(app_state): State<crate::server::AppState>,
    Path(model_path): Path<String>,
    request: Request,
) -> Result<axum::response::Response, StatusCode> {
    let (parts, body) = request.into_parts();

    // Extract model from path like "gemini-1.5-flash:generateContent"
    let model = if let Some(model) = model_path.strip_suffix(":generateContent") {
        if model.trim().is_empty() {
            return Err(error_handling::bad_request(
                "Empty model name in Gemini endpoint", 
                &"Model name cannot be empty in URL path"
            ));
        }
        tracing::debug!("Extracted model from Gemini URL path: {}", model);
        model
    } else {
        return Err(error_handling::bad_request(
            "Invalid Gemini endpoint format", 
            &"Expected format: /v1beta/models/{model}:generateContent"
        ));
    };

    // Parse Gemini-format request body (no model field expected)
    let gemini_request_value = parsing::parse_gemini_request(body).await?;

    // Route based on model name
    let router = NameBasedRouter::new(Default::default());
    let routing_decision = match router.route_model(&model) {
        Ok(decision) => decision,
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
                &model,
                routing_decision,
                parts.headers,
            ).await
        }
        "openrouter" => {
            gemini::handle_openrouter_from_gemini(
                app_state.config.clone(),
                gemini_request_value.clone(),
                &model,
                routing_decision,
                parts.headers,
            ).await
        }
        "anthropic" => {
            gemini::handle_anthropic_from_gemini(
                app_state.config.clone(),
                gemini_request_value.clone(),
                &model,
                routing_decision,
                parts.headers,
            ).await
        }
        provider_type => {
            Err(error_handling::internal_error(
                "Custom providers not yet supported from Gemini endpoint",
                &format!("Provider type: {}", provider_type)
            ))
        }
    }
}