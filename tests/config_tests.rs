use setu::config::{AuthConfig, Config, ProviderConfig, RoutingConfig, ServerConfig};
use std::collections::HashMap;

#[test]
fn test_default_config() {
    let config = Config::default();
    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.server.port, 3742);
    assert_eq!(config.server.log_level, "info");
    assert_eq!(config.routing.default_provider, "openrouter");
    assert!(config.providers.is_empty());
}

#[test]
fn test_config_with_providers() {
    let mut providers = HashMap::new();
    providers.insert(
        "test_provider".to_string(),
        ProviderConfig {
            r#type: "openrouter".to_string(),
            endpoint: "https://test.com".to_string(),
            models: vec!["test-model".to_string()],
            auth: AuthConfig::default(),
        },
    );

    let config = Config {
        server: ServerConfig {
            host: "localhost".to_string(),
            port: 9000,
            log_level: "debug".to_string(),
            log_file_enabled: true,
            log_rotation: "daily".to_string(),
            log_dir: None,
            log_file_prefix: "setu".to_string(),
        },
        providers,
        routing: RoutingConfig {
            default_provider: "test_provider".to_string(),
        },
        auth: HashMap::new(),
    };

    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 9000);
    assert_eq!(config.providers.len(), 1);
    assert!(config.providers.contains_key("test_provider"));
}

#[test]
fn test_config_serialization() {
    let config = Config::default();
    let serialized = serde_json::to_string(&config).unwrap();
    assert!(serialized.contains("server"));
    assert!(serialized.contains("providers"));
    assert!(serialized.contains("routing"));
}
