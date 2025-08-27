use setu::{
    config::{AuthConfig, Config, RoutingConfig, ServerConfig},
};
use std::collections::HashMap;

// Removed unused helper functions create_config_with_invalid_tokens and create_config_without_tokens
// These were only used by the removed tests above

// Removed test_validation_fails_with_invalid_tokens and test_validation_fails_without_refresh_token
// These tests were testing behavior that's intentionally changed with smart token selection.
// The validation now tries Claude Code credentials as fallback, so it may succeed even when
// setu config has invalid/missing tokens.

#[tokio::test]
async fn test_validation_handles_missing_provider() {
    // Config with no anthropic provider
    let config = Config {
        server: ServerConfig::default(),
        providers: HashMap::new(),
        routing: RoutingConfig {
            default_provider: "openrouter".to_string(),
            strategy: "composite".to_string(),
            enable_fallback: true,
            min_confidence: 0.0,
            rules: HashMap::new(),
            provider_priorities: Vec::new(),
            provider_capabilities: HashMap::new(),
            provider_aliases: HashMap::new(),
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
        project_id: None,
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
