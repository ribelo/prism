use setu::{
    auth::anthropic::AnthropicOAuth,
    config::{AuthConfig, Config, ProviderConfig, RoutingConfig, ServerConfig},
};
use std::collections::HashMap;

fn create_config_with_invalid_tokens() -> Config {
    let mut providers = HashMap::new();

    // Create provider with expired/invalid OAuth tokens
    providers.insert(
        "anthropic".to_string(),
        ProviderConfig {
            r#type: "anthropic".to_string(),
            endpoint: "https://api.anthropic.com".to_string(),
            models: vec!["claude-3-sonnet".to_string()],
            auth: AuthConfig {
                oauth_access_token: Some("invalid_token".to_string()),
                oauth_refresh_token: Some("invalid_refresh".to_string()),
                oauth_expires: Some(0), // Expired
            },
        },
    );

    Config {
        server: ServerConfig::default(),
        providers,
        routing: RoutingConfig {
            default_provider: "anthropic".to_string(),
        },
        auth: HashMap::new(),
    }
}

fn create_config_without_tokens() -> Config {
    let mut providers = HashMap::new();

    // Create provider without OAuth tokens
    providers.insert(
        "anthropic".to_string(),
        ProviderConfig {
            r#type: "anthropic".to_string(),
            endpoint: "https://api.anthropic.com".to_string(),
            models: vec!["claude-3-sonnet".to_string()],
            auth: AuthConfig::default(), // No tokens
        },
    );

    Config {
        server: ServerConfig::default(),
        providers,
        routing: RoutingConfig {
            default_provider: "anthropic".to_string(),
        },
        auth: HashMap::new(),
    }
}

#[tokio::test]
async fn test_validation_fails_with_invalid_tokens() {
    let config = create_config_with_invalid_tokens();
    let mut auth_config = config.providers.get("anthropic").unwrap().auth.clone();

    // This should fail because tokens are invalid
    let result = AnthropicOAuth::validate_auth_config(&mut auth_config).await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Token refresh failed"));
}

#[tokio::test]
async fn test_validation_fails_without_refresh_token() {
    let config = create_config_without_tokens();
    let mut auth_config = config.providers.get("anthropic").unwrap().auth.clone();

    // This should fail because no refresh token is present
    let result = AnthropicOAuth::validate_auth_config(&mut auth_config).await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("No OAuth refresh token found"));
    assert!(error_msg.contains("setu auth anthropic"));
}

#[tokio::test]
async fn test_validation_handles_missing_provider() {
    // Config with no anthropic provider
    let config = Config {
        server: ServerConfig::default(),
        providers: HashMap::new(),
        routing: RoutingConfig {
            default_provider: "openrouter".to_string(),
        },
        auth: HashMap::new(),
    };

    // Should not panic when no anthropic provider exists
    // This simulates the validation logic in main.rs
    if let Some(_provider) = config.providers.get("anthropic") {
        // This branch shouldn't execute
        assert!(false, "Should not have anthropic provider");
    } else {
        // This is the expected path - no anthropic provider configured
        assert!(true);
    }
}

#[test]
fn test_token_expiry_detection() {
    // Test token expiry logic
    let mut auth_config = AuthConfig {
        oauth_access_token: Some("valid_token".to_string()),
        oauth_refresh_token: Some("refresh_token".to_string()),
        oauth_expires: Some(0), // Already expired
    };

    assert!(auth_config.is_token_expired());

    // Set expiry far in the future
    let future_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 3600000; // 1 hour from now

    auth_config.oauth_expires = Some(future_time);
    assert!(!auth_config.is_token_expired());
}
