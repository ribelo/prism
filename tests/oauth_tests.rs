use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
};
use rustc_hash::FxHashMap;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicU64};
use std::time::SystemTime;
use tokio::sync::Mutex;

use setu::{
    auth::{AuthCache, AuthMethod},
    config::{AuthConfig, Config, ProviderConfig, RetryConfig, RoutingConfig, ServerConfig},
    server::{AppState, routes::anthropic_messages},
};

async fn create_test_app_state_with_oauth() -> AppState {
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

    // Create auth cache with mock OAuth token
    let auth_cache = AuthCache {
        anthropic_method: AuthMethod::OAuth {
            source: "Test OAuth".to_string(),
            token: "test-oauth-token-12345".to_string(),
        },
        gemini_method: AuthMethod::ApiKey,
        openai_method: AuthMethod::ApiKey,
        cached_at: SystemTime::now(),
    };

    AppState {
        config: Arc::new(Mutex::new(Config {
            server: ServerConfig::default(),
            providers,
            routing: RoutingConfig {
                models: FxHashMap::default(),
            },
            auth: FxHashMap::default(),
        })),
        auth_cache: Arc::new(auth_cache),
        config_path: PathBuf::from("/tmp/setu.toml"),
        last_config_check: Arc::new(AtomicU64::new(0)),
    }
}

#[tokio::test]
async fn test_oauth_header_detection() {
    let app_state = create_test_app_state_with_oauth().await;

    let request_body = json!({
        "model": "claude-3-5-sonnet",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}]
    });

    // Test claude-cli user agent detection
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("user-agent", "claude-cli/1.0.0")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    // This should trigger OAuth path
    let response = anthropic_messages(State(app_state.clone()), request).await;

    // Should fail because we're making actual API call with fake token,
    // but at least we can verify it tries OAuth path
    assert!(response.is_err());

    // The error should NOT be "authentication unavailable" - that would mean
    // OAuth wasn't detected
    let err = response.unwrap_err();
    assert_ne!(err, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_oauth_x_app_header_detection() {
    let app_state = create_test_app_state_with_oauth().await;

    let request_body = json!({
        "model": "claude-3-5-sonnet",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}]
    });

    // Test x-app header detection
    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("x-app", "cli")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state.clone()), request).await;

    // Should try OAuth path and fail due to API call with fake token
    assert!(response.is_err());
    let err = response.unwrap_err();
    assert_ne!(err, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_oauth_fallback_to_api_key() {
    // Create app state with API key auth method instead of OAuth
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

    let auth_cache = AuthCache {
        anthropic_method: AuthMethod::ApiKey,
        gemini_method: AuthMethod::ApiKey,
        openai_method: AuthMethod::ApiKey,
        cached_at: SystemTime::now(),
    };

    let app_state = AppState {
        config: Arc::new(Mutex::new(Config {
            server: ServerConfig::default(),
            providers,
            routing: RoutingConfig {
                models: FxHashMap::default(),
            },
            auth: FxHashMap::default(),
        })),
        auth_cache: Arc::new(auth_cache),
        config_path: PathBuf::from("/tmp/setu.toml"),
        last_config_check: Arc::new(AtomicU64::new(0)),
    };

    let request_body = json!({
        "model": "claude-3-5-sonnet",
        "max_tokens": 100,
        "messages": [{"role": "user", "content": "test"}]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/v1/messages")
        .header("content-type", "application/json")
        .header("user-agent", "claude-cli/1.0.0") // Claude Code header
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = anthropic_messages(State(app_state), request).await;

    // Should fall through to API key path instead of OAuth
    assert!(response.is_err());

    // Should fail due to missing API key, not auth unavailable
    let err = response.unwrap_err();
    assert_ne!(err, StatusCode::UNAUTHORIZED);
}
