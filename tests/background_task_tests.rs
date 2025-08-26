use setu::{
    Config,
    config::{AuthConfig, ProviderConfig, RoutingConfig, ServerConfig},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[test]
fn test_token_needs_refresh_logic() {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Test token that expires within 10 minutes should need refresh
    let soon_expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 300_000; // 5 minutes from now

    let auth_config = AuthConfig {
        oauth_access_token: Some("token".to_string()),
        oauth_refresh_token: Some("refresh".to_string()),
        oauth_expires: Some(soon_expiry),
    };

    assert!(auth_config.needs_refresh());

    // Test token that expires in more than 10 minutes should not need refresh
    let future_expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 900_000; // 15 minutes from now

    let auth_config_future = AuthConfig {
        oauth_access_token: Some("token".to_string()),
        oauth_refresh_token: Some("refresh".to_string()),
        oauth_expires: Some(future_expiry),
    };

    assert!(!auth_config_future.needs_refresh());
}

#[test]
fn test_token_expiry_detection() {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Test expired token
    let expired_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        - 1000; // 1 second ago

    let auth_config = AuthConfig {
        oauth_access_token: Some("token".to_string()),
        oauth_refresh_token: Some("refresh".to_string()),
        oauth_expires: Some(expired_time),
    };

    assert!(auth_config.is_token_expired());

    // Test valid token
    let future_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 3600_000; // 1 hour from now

    let auth_config_valid = AuthConfig {
        oauth_access_token: Some("token".to_string()),
        oauth_refresh_token: Some("refresh".to_string()),
        oauth_expires: Some(future_time),
    };

    assert!(!auth_config_valid.is_token_expired());
}

fn create_test_config() -> Config {
    let mut providers = HashMap::new();
    providers.insert(
        "anthropic".to_string(),
        ProviderConfig {
            r#type: "anthropic".to_string(),
            endpoint: "https://api.anthropic.com".to_string(),
            models: vec!["claude-3-sonnet".to_string()],
            auth: AuthConfig {
                oauth_access_token: Some("test_token".to_string()),
                oauth_refresh_token: Some("test_refresh".to_string()),
                oauth_expires: Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64
                        + 300_000, // 5 minutes from now
                ),
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

#[tokio::test]
async fn test_background_task_config_structure() {
    // Test that our config structure supports the background task logic
    let config = create_test_config();
    let config_arc = Arc::new(Mutex::new(config));

    let config_guard = config_arc.lock().await;

    // Verify we can access anthropic provider
    let provider = config_guard.providers.get("anthropic");
    assert!(provider.is_some());

    let provider = provider.unwrap();
    assert!(provider.auth.oauth_refresh_token.is_some());
    assert!(provider.auth.needs_refresh()); // Should need refresh since expires in 5 minutes
}
