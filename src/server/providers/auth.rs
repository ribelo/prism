use crate::config::Config;
use crate::server::error_handling;
use axum::http::{HeaderMap, StatusCode};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Check if request is from Claude Code by examining headers
pub fn is_claude_code_request(headers: &HeaderMap) -> bool {
    if let Some(user_agent) = headers.get("user-agent")
        && let Ok(ua_str) = user_agent.to_str()
        && ua_str.starts_with("claude-cli/")
    {
        return true;
    }

    if let Some(x_app) = headers.get("x-app")
        && let Ok(app_str) = x_app.to_str()
        && app_str == "cli"
    {
        return true;
    }

    false
}

/// Handle OAuth authentication with automatic token refresh on failure
pub async fn handle_oauth_request(
    auth_method: &crate::auth::AuthMethod,
    config: Arc<Mutex<Config>>,
    chat_request: anthropic_ox::ChatRequest,
    routing_decision: crate::router::name_based::RoutingDecision,
    parts: axum::http::request::Parts,
) -> Result<axum::response::Response, StatusCode> {
    use crate::auth::anthropic::AnthropicOAuth;
    use anthropic_ox::Anthropic;

    // Try with cached token first
    let mut oauth_token = match auth_method {
        crate::auth::AuthMethod::OAuth { token, .. } => token.clone(),
        crate::auth::AuthMethod::ApiKey => {
            return Err(error_handling::unauthorized(
                "OAuth method expected but API key method was cached",
            ));
        }
        crate::auth::AuthMethod::Unavailable { reason } => {
            return Err(error_handling::unauthorized(&format!(
                "OAuth method unavailable: {}",
                reason
            )));
        }
    };

    // Attempt request with current token, refresh and retry if auth fails
    for attempt in 0..2 {
        let mut client = Anthropic::builder().oauth_token(&oauth_token).build();

        // Add required OAuth headers
        client = client.header(
            "anthropic-client-id",
            "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
        );
        client = client.header("anthropic-beta", 
            "oauth-2025-04-20,claude-code-20250219,interleaved-thinking-2025-05-14,fine-grained-tool-streaming-2025-05-14");

        tracing::debug!(
            "OAuth request with token: {}...",
            &oauth_token[..std::cmp::min(oauth_token.len(), 20)]
        );

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

        // Apply URL parameters to the request if present
        let mut modified_request = chat_request.clone();
        if let Some(query_params) = &routing_decision.query_params {
            modified_request = crate::server::parameter_mapping::apply_anthropic_parameters(
                modified_request,
                query_params,
            );
        }

        // Check if this is a streaming request
        let is_streaming = modified_request.stream.unwrap_or(false);

        if is_streaming {
            // Verbose: log sanitized, truncated request
            if let Some(req_str) = crate::server::error_handling::prepare_request_log(&modified_request) {
                tracing::debug!(target: "setu::request", "Outgoing Anthropic OAuth (stream) request: {}", req_str);
            }
            // Handle streaming request
            use axum::body::Body;
            use futures_util::StreamExt;

            let stream = client.stream(&modified_request);
            let sse_stream = stream.map(|result| {
                match result {
                    Ok(event) => {
                        // Convert StreamEvent to SSE format
                        let json_data = match serde_json::to_string(&event) {
                            Ok(json) => json,
                            Err(_) => {
                                return Ok::<String, String>(
                                    "data: {\"error\": \"serialization failed\"}\n\n".to_string(),
                                );
                            }
                        };
                        Ok(format!("data: {}\n\n", json_data))
                    }
                    Err(e) => {
                        let error_json =
                            format!("{{\"error\": \"{}\"}}", e.to_string().replace("\"", "\\\""));
                        Ok(format!("data: {}\n\n", error_json))
                    }
                }
            });

            // Convert to Body stream
            let body_stream = sse_stream.map(|item| match item {
                Ok(data) => Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(data)),
                Err(_) => Ok(axum::body::Bytes::from(
                    "data: {\"error\": \"stream error\"}\n\n",
                )),
            });

            return Ok(axum::response::Response::builder()
                .status(200)
                .header("content-type", "text/event-stream")
                .header("cache-control", "no-cache")
                .header("connection", "keep-alive")
                .body(Body::from_stream(body_stream))
                .unwrap());
        } else {
            // Verbose: log sanitized, truncated request
            if let Some(req_str) = crate::server::error_handling::prepare_request_log(&modified_request) {
                tracing::debug!(target: "setu::request", "Outgoing Anthropic OAuth request: {}", req_str);
            }
            // Handle non-streaming request
            match client.send(&modified_request).await {
                Ok(response) => {
                    // Verbose: log truncated response
                    if let Some(resp_str) = crate::server::error_handling::prepare_response_log(&response) {
                        tracing::debug!(target: "setu::response", "Anthropic OAuth response: {}", resp_str);
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

                    return Ok(axum::response::Response::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(json_body))
                        .unwrap());
                }
                Err(e) => {
                    let error_str = e.to_string();
                    tracing::error!("OAuth request error: {}", error_str);

                    // Check if this is an auth error that might be resolved by refreshing
                    if attempt == 0
                        && (error_str.contains("401") || error_str.contains("authentication"))
                    {
                        tracing::warn!(
                            "OAuth request failed with auth error, attempting token refresh"
                        );

                        // Try to refresh token from prism config if available
                        let config_guard = config.lock().await;
                        if let Some(anthropic_provider) = config_guard.providers.get("anthropic") {
                            let mut auth_config = anthropic_provider.auth.clone();
                            drop(config_guard); // Release lock before async operation

                            match AnthropicOAuth::refresh_token(&mut auth_config).await {
                                Ok(()) => {
                                    if let Some(new_token) = auth_config.oauth_access_token {
                                        oauth_token = new_token;
                                        tracing::info!(
                                            "Successfully refreshed OAuth token, retrying request"
                                        );
                                        continue; // Retry with new token
                                    }
                                }
                                Err(refresh_err) => {
                                    tracing::error!(
                                        "Failed to refresh OAuth token: {}",
                                        refresh_err
                                    );
                                }
                            }
                        }
                    }

                    // Log the error and return failure
                    let compacted_request =
                        crate::server::error_handling::compact_request_for_logging(
                            &modified_request,
                        );
                    tracing::error!("Failed OAuth request (compacted): {}", compacted_request);

                    return Err(error_handling::internal_error(
                        "Anthropic OAuth request failed",
                        &e,
                    ));
                }
            }
        }
    }

    // Should not reach here
    Err(error_handling::internal_error(
        "OAuth request failed after retries",
        &"Maximum retry attempts exceeded",
    ))
}

/// Transform anthropic-beta header to include OAuth beta flag
fn transform_anthropic_beta_header(existing_beta: Option<&str>) -> String {
    match existing_beta {
        Some(beta) => format!("oauth-2025-04-20,{}", beta),
        None => "oauth-2025-04-20".to_string(),
    }
}
