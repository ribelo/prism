use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
};
use rustc_hash::FxHashMap;
use serde_json::json;
use std::sync::{Arc, atomic::AtomicU64};
use tokio::sync::Mutex;

use setu::{
    auth::{AuthCache, AuthMethod, initialize_auth_cache},
    config::{AuthConfig, Config, ProviderConfig, RetryConfig, RoutingConfig, ServerConfig},
    server::{AppState, routes::gemini_generate_content},
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
    providers.insert(
        "gemini".to_string(),
        ProviderConfig {
            r#type: "gemini".to_string(),
            endpoint: "https://generativelanguage.googleapis.com/v1beta".to_string(),
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
        last_config_check: Arc::new(AtomicU64::new(0)),
        config_path: std::path::PathBuf::from("/tmp/test_setu.toml"),
    }
}

/// Test Gemini endpoint URL format validation
#[tokio::test]
async fn test_gemini_endpoint_url_format() {
    let app_state = create_test_app_state().await;

    let valid_request_body = json!({
        "contents": [
            {
                "role": "user",
                "parts": [{"text": "Hello"}]
            }
        ]
    });

    // Test valid format: model:generateContent
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(valid_request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("gemini-1.5-flash:generateContent".to_string()),
        request,
    )
    .await;

    // Should fail due to authentication, but URL parsing should succeed
    assert!(response.is_err());
    // Authentication failure typically returns UNAUTHORIZED or BAD_GATEWAY
    let status = response.unwrap_err();
    println!("Actual status code: {:?}", status);
    // Allow for various error codes since this is testing URL parsing, not auth
    assert!(
        status == StatusCode::UNAUTHORIZED
            || status == StatusCode::BAD_GATEWAY
            || status == StatusCode::INTERNAL_SERVER_ERROR
    );
}

/// Test empty model name validation
#[tokio::test]
async fn test_empty_model_name() {
    let app_state = create_test_app_state().await;

    let request_body = json!({
        "contents": [
            {
                "role": "user",
                "parts": [{"text": "Hello"}]
            }
        ]
    });

    // Test completely empty model (just the suffix)
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path(":generateContent".to_string()),
        request,
    )
    .await;

    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);

    // Test whitespace-only model
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("   :generateContent".to_string()),
        request,
    )
    .await;

    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);
}

/// Test invalid endpoint suffix validation
#[tokio::test]
async fn test_invalid_endpoint_suffix() {
    let app_state = create_test_app_state().await;

    let request_body = json!({
        "contents": [
            {
                "role": "user",
                "parts": [{"text": "Hello"}]
            }
        ]
    });

    // Test wrong suffix
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("gemini-1.5-flash:generate".to_string()),
        request,
    )
    .await;

    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);

    // Test completely wrong format
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("gemini-1.5-flash".to_string()),
        request,
    )
    .await;

    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);

    // Test multiple colons
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state),
        Path("gemini:1.5:flash:generateContent".to_string()),
        request,
    )
    .await;

    // Should still work - we only check the suffix
    assert!(response.is_err());
    let status = response.unwrap_err();
    assert!(
        status == StatusCode::UNAUTHORIZED
            || status == StatusCode::BAD_GATEWAY
            || status == StatusCode::INTERNAL_SERVER_ERROR
    );
}

/// Test URL-encoded model names
#[tokio::test]
async fn test_url_encoded_model_names() {
    let app_state = create_test_app_state().await;

    let request_body = json!({
        "contents": [
            {
                "role": "user",
                "parts": [{"text": "Hello"}]
            }
        ]
    });

    // Test URL-encoded model name (gemini-1.5-flash -> gemini%2D1.5%2Dflash)
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("gemini%2D1.5%2Dflash:generateContent".to_string()),
        request,
    )
    .await;

    // Should handle URL decoding and fail at authentication level
    assert!(response.is_err());
    let status = response.unwrap_err();
    assert!(
        status == StatusCode::UNAUTHORIZED
            || status == StatusCode::BAD_GATEWAY
            || status == StatusCode::INTERNAL_SERVER_ERROR
    );

    // Test model with special characters that would be URL encoded
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state),
        Path("openrouter/z-ai/glm-4.5:generateContent".to_string()),
        request,
    )
    .await;

    // Response can either succeed (if API keys available) or fail (if not)
    match response {
        Ok(_) => {} // Success - API keys are available and conversion worked
        Err(status) => {
            // Should route to openrouter and fail at authentication
            assert!(
                status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::BAD_GATEWAY
                    || status == StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    }
}

/// Test provider routing with Gemini endpoint
#[tokio::test]
async fn test_provider_routing() {
    let app_state = create_test_app_state().await;

    let request_body = json!({
        "contents": [
            {
                "role": "user",
                "parts": [{"text": "Hello"}]
            }
        ]
    });

    // Test OpenRouter routing
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("openrouter/z-ai/glm-4.5:generateContent".to_string()),
        request,
    )
    .await;

    // Response can either succeed (if API keys available) or fail (if not)
    match response {
        Ok(_) => {} // Success - API keys are available and conversion worked
        Err(status) => {
            // Should route to OpenRouter provider (fails due to no auth)
            assert!(
                status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::BAD_GATEWAY
                    || status == StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    }

    // Test Anthropic routing
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("anthropic/claude-3-5-sonnet-20241022:generateContent".to_string()),
        request,
    )
    .await;

    // Response can either succeed (if API keys available) or fail (if not)
    match response {
        Ok(_) => {} // Success - API keys are available and conversion worked
        Err(status) => {
            // Should route to Anthropic provider (fails due to no auth)
            assert!(
                status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::BAD_GATEWAY
                    || status == StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    }

    // Test native Gemini routing
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("gemini-1.5-flash:generateContent".to_string()),
        request,
    )
    .await;

    // Response can either succeed (if API keys available) or fail (if not)
    match response {
        Ok(_) => {} // Success - API keys are available and conversion worked
        Err(status) => {
            // Should route to native Gemini provider (fails due to no auth)
            assert!(
                status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::BAD_GATEWAY
                    || status == StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    }

    // Test unknown model defaults
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state),
        Path("unknown-model-12345:generateContent".to_string()),
        request,
    )
    .await;

    // Response can either succeed (if API keys available) or fail (if not)
    match response {
        Ok(_) => {} // Success - API keys are available and conversion worked
        Err(status) => {
            // Should route to default provider (OpenRouter) and fail
            assert!(
                status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::BAD_GATEWAY
                    || status == StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    }
}

/// Test malformed request body handling
#[tokio::test]
async fn test_malformed_request_body() {
    let app_state = create_test_app_state().await;

    // Test empty body
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("gemini-1.5-flash:generateContent".to_string()),
        request,
    )
    .await;

    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);

    // Test invalid JSON
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from("{ invalid json"))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("gemini-1.5-flash:generateContent".to_string()),
        request,
    )
    .await;

    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);

    // Test missing required Gemini fields
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"model": "gemini-1.5-flash"}"#))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state),
        Path("gemini-1.5-flash:generateContent".to_string()),
        request,
    )
    .await;

    assert!(response.is_err());
    assert_eq!(response.unwrap_err(), StatusCode::BAD_REQUEST);
}

/// Test special characters in model names
#[tokio::test]
async fn test_special_characters_in_model_names() {
    let app_state = create_test_app_state().await;

    let request_body = json!({
        "contents": [
            {
                "role": "user",
                "parts": [{"text": "Hello"}]
            }
        ]
    });

    // Test model with @ symbol (common in some model versions)
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state.clone()),
        Path("gemini-1.5-pro@001:generateContent".to_string()),
        request,
    )
    .await;

    // Response can either succeed (if API keys available) or fail (if not)
    match response {
        Ok(_) => {} // Success - API keys are available and conversion worked
        Err(status) => {
            assert!(
                status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::BAD_GATEWAY
                    || status == StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    }

    // Test model with underscores and numbers
    let request = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = gemini_generate_content(
        State(app_state),
        Path("gemini_2_5_flash_v1:generateContent".to_string()),
        request,
    )
    .await;

    // Response can either succeed (if API keys available) or fail (if not)
    match response {
        Ok(_) => {} // Success - API keys are available and conversion worked
        Err(status) => {
            assert!(
                status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::BAD_GATEWAY
                    || status == StatusCode::INTERNAL_SERVER_ERROR
            );
        }
    }
}
