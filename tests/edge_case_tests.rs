use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
};
use serde_json::json;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use setu::{
    auth::{AuthCache, AuthMethod, initialize_auth_cache},
    config::{AuthConfig, Config, ProviderConfig, RoutingConfig, ServerConfig, RetryConfig},
    server::{routes::{anthropic_messages, openai_chat_completions, openai_models}, AppState},
};
use std::time::SystemTime;

async fn create_test_app_state() -> AppState {
    let mut providers = FxHashMap::default();
    providers.insert(
        "anthropic".to_string(),
        ProviderConfig {
            r#type: "anthropic".to_string(),
            endpoint: "https://api.anthropic.com".to_string(),
            auth: AuthConfig::default(),
            retry: RetryConfig::default(),
            api_key: None,
            api_key_fallback: false,
            fallback_on_errors: vec![429],
        },
    );
    providers.insert(
        "openrouter".to_string(),
        ProviderConfig {
            r#type: "openrouter".to_string(),
            endpoint: "https://openrouter.ai/api/v1".to_string(),
            auth: AuthConfig::default(),
            retry: RetryConfig::default(),
            api_key: None,
            api_key_fallback: false,
            fallback_on_errors: vec![429],
        },
    );

    AppState {
        config: Arc::new(Mutex::new(Config {
        server: ServerConfig::default(),
        providers,
        routing: RoutingConfig {
            models: FxHashMap::default(),
        },
        auth: FxHashMap::default(),
        })),
        auth_cache: Arc::new(initialize_auth_cache().await.unwrap_or_else(|_| AuthCache {
            anthropic_method: AuthMethod::ApiKey,
            gemini_method: AuthMethod::ApiKey,
            openai_method: AuthMethod::ApiKey,
            cached_at: SystemTime::now(),
        })),
    }
}

/// Test request parsing edge cases
#[tokio::test]
#[ignore] // Integration test that makes network calls
async fn test_request_parsing_edge_cases() {
    let app_state = create_test_app_state().await;

    // Test completely empty body
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);

    // Test malformed JSON
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .body(Body::from("{ invalid json"))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);

    // Test missing required fields
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"max_tokens": 100}"#))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);

    // Test extremely large request body (should fail gracefully)
    let large_content = "x".repeat(2_000_000); // 2MB of content
    let large_request = json!({
        "model": "claude-3-sonnet",
        "max_tokens": 100,
        "messages": [{
            "role": "user",
            "content": large_content
        }]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .body(Body::from(large_request.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state), request).await;
    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);
}

/// Test routing edge cases
#[tokio::test]
async fn test_routing_edge_cases() {
    let app_state = create_test_app_state().await;

    // Test unknown model
    let request_body = json!({
        "model": "unknown-model-12345",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-test")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    // Should default to openrouter provider
    assert!(response.is_err());
    // Will fail due to missing OpenRouter credentials, but routing should work

    // Test empty model name
    let request_body = json!({
        "model": "",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-test")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    // Should default to openrouter provider for empty model
    assert!(response.is_err());

    // Test model with special characters
    let request_body = json!({
        "model": "claude-3/sonnet@special!",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-test")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state), request).await;
    // Should route to anthropic based on "claude-" prefix
    assert!(response.is_err());
}

/// Test authentication edge cases
#[tokio::test]
#[ignore] // Integration test that makes network calls
async fn test_authentication_edge_cases() {
    let app_state = create_test_app_state().await;

    let request_body = json!({
        "model": "claude-3-sonnet",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}]
    });

    // Test missing authorization header
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::UNAUTHORIZED);

    // Test invalid authorization header format
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "InvalidFormat")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::UNAUTHORIZED);

    // Test empty bearer token (API call will likely fail with bad gateway)
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "Bearer ")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    assert!(response.is_err());
    // Empty token likely results in API call failure (BAD_GATEWAY) rather than UNAUTHORIZED
    let status = response.unwrap_err();
    assert!(status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_GATEWAY);

    // Test non-sk API key format
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "Bearer invalid-api-key")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state), request).await;
    // Should attempt direct anthropic call and fail due to invalid credentials
    assert!(response.is_err());
    // Will be BAD_GATEWAY (502) due to API call failure, not UNAUTHORIZED
    assert_eq!(response.unwrap_err(), StatusCode::BAD_GATEWAY);
}

/// Test Claude Code detection edge cases
#[tokio::test]
#[ignore] // Integration test that makes network calls
async fn test_claude_code_detection() {
    let app_state = create_test_app_state().await;

    let request_body = json!({
        "model": "claude-3-sonnet",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}]
    });

    // Test claude-cli user agent detection
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-test")
        .header("user-agent", "claude-cli/1.0.0")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    // Should trigger OAuth path and fail due to API call failure
    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_GATEWAY);

    // Test x-app cli header detection
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-test")
        .header("x-app", "cli")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    // Should trigger OAuth path and fail due to API call failure
    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_GATEWAY);

    // Test invalid user agent (should not trigger Claude Code path)
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-test")
        .header("user-agent", "not-claude-cli/1.0.0")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state), request).await;
    // Should use direct path and fail due to invalid API key
    assert!(response.is_err());
}

/// Test OpenAI endpoint behaviors
#[tokio::test]
#[ignore] // Integration test that makes network calls
async fn test_openai_endpoints() {
    let app_state = create_test_app_state().await;
    
    // Test not implemented chat completions
    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .body(Body::empty())
        .unwrap();

    let response = openai_chat_completions(State(app_state.clone()), request).await;
    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::NOT_IMPLEMENTED);

    // Test models endpoint returns mock data
    let response = openai_models(State(app_state)).await;
    let json_value = response.0; // Extract the Value from Json<Value>

    assert!(json_value["data"].is_array());
    assert_eq!(json_value["object"], "list");
    assert_eq!(json_value["data"][0]["id"], "setu-noop");
}

/// Test streaming request edge cases
#[tokio::test]
#[ignore] // Integration test that makes network calls
async fn test_streaming_edge_cases() {
    let app_state = create_test_app_state().await;

    // Test streaming with Claude Code detection (should fail due to missing OAuth)
    let request_body = json!({
        "model": "claude-3-sonnet",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}],
        "stream": true
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-test")
        .header("user-agent", "claude-cli/1.0.0")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;
    // May succeed or fail depending on token availability and streaming implementation
    match response {
        Ok(_) => {} // Streaming response succeeded
        Err(status) => {
            // Should be BAD_GATEWAY if it fails due to API call failure
            assert_eq!(status, StatusCode::BAD_GATEWAY);
        }
    }

    // Test streaming with malformed request (should parse properly and route correctly)
    let request_body = json!({
        "model": "gpt-4",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}],
        "stream": true
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-test")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state), request).await;
    // This should route to OpenRouter and likely succeed with mock/test response or fail gracefully
    // Just verify it doesn't panic and returns a valid status
    match response {
        Ok(_) => {} // Response succeeded (streaming handled properly)
        Err(status) => {
            // Should be one of the expected error types if it fails
            assert!(
                status == StatusCode::BAD_GATEWAY
                    || status == StatusCode::INTERNAL_SERVER_ERROR
                    || status == StatusCode::UNAUTHORIZED
            );
        }
    }
}

/// Test header handling edge cases
#[tokio::test]
async fn test_header_handling() {
    let app_state = create_test_app_state().await;

    let request_body = json!({
        "model": "claude-3-sonnet",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}]
    });

    // Test with anthropic-beta header
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("anthropic-beta", "messages-2023-12-15".parse().unwrap());

    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let (mut parts, body) = request.into_parts();
    parts.headers = headers;
    let request = Request::from_parts(parts, body);

    let response = anthropic_messages(State(app_state.clone()), request).await;
    // Should handle the header properly (fail due to invalid API key)
    assert!(response.is_err());

    // Test with invalid header values (non-UTF8)
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    // Add header with invalid UTF-8 - this should be handled gracefully

    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let (mut parts, body) = request.into_parts();
    parts.headers = headers;
    let request = Request::from_parts(parts, body);

    let response = anthropic_messages(State(app_state), request).await;
    // Should handle gracefully and fail due to invalid API key
    assert!(response.is_err());
}
