use directories::ProjectDirs;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{Result, SetuError};

pub mod models;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub providers: FxHashMap<String, ProviderConfig>,
    pub routing: RoutingConfig,
    #[serde(default)]
    pub auth: FxHashMap<String, AuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_log_file_enabled")]
    pub log_file_enabled: bool,
    #[serde(default = "default_log_rotation")]
    pub log_rotation: String,
    #[serde(default)]
    pub log_dir: Option<String>,
    #[serde(default = "default_log_file_prefix")]
    pub log_file_prefix: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            log_level: default_log_level(),
            log_file_enabled: default_log_file_enabled(),
            log_rotation: default_log_rotation(),
            log_dir: None,
            log_file_prefix: default_log_file_prefix(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_initial_interval_ms")]
    pub initial_interval_ms: u64,
    #[serde(default = "default_max_interval_ms")]
    pub max_interval_ms: u64,
    #[serde(default = "default_multiplier")]
    pub multiplier: f32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            initial_interval_ms: default_initial_interval_ms(),
            max_interval_ms: default_max_interval_ms(),
            multiplier: default_multiplier(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub r#type: String,
    pub endpoint: String,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub retry: RetryConfig,
    /// API key for direct authentication (supports ${VAR} environment variable interpolation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Enable fallback to API key authentication when OAuth fails
    #[serde(default)]
    pub api_key_fallback: bool,
    /// HTTP error codes that trigger fallback authentication
    #[serde(default = "default_fallback_errors")]
    pub fallback_on_errors: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelRoute {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    /// Model-to-model routing with fallback support
    /// Maps model names to either a single model or array of fallback models
    /// Example: "haiku-3.5" = "openai/gpt-4o" or ["openai/gpt-4o", "openrouter/glm-4.5:fireworks"]
    #[serde(default)]
    pub models: FxHashMap<String, ModelRoute>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_expires: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

impl AuthConfig {
    pub fn is_token_expired(&self) -> bool {
        match self.oauth_expires {
            Some(expires) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                now >= expires
            }
            None => true, // If no expiry time, assume expired
        }
    }

    pub fn needs_refresh(&self) -> bool {
        match self.oauth_expires {
            Some(expires) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                // Refresh if token expires within 10 minutes (600,000 ms)
                (now + 600_000) >= expires
            }
            None => true, // If no expiry time, needs refresh
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3742 // SETU on phone keypad: 7-3-8-5, but shifted to avoid common ports
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_fallback_errors() -> Vec<u16> {
    vec![429] // Rate limit error
}

fn default_log_file_enabled() -> bool {
    true
}

fn default_log_rotation() -> String {
    "daily".to_string()
}

fn default_log_file_prefix() -> String {
    "setu".to_string()
}

fn default_max_retries() -> u32 {
    3
}

fn default_initial_interval_ms() -> u64 {
    1000 // 1 second
}

fn default_max_interval_ms() -> u64 {
    30000 // 30 seconds
}

fn default_multiplier() -> f32 {
    2.0
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            providers: FxHashMap::default(),
            routing: RoutingConfig {
                models: FxHashMap::default(),
            },
            auth: FxHashMap::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_dir = get_config_dir()?;
        let config_file = config_dir.join("setu.toml");

        // Create default config if file doesn't exist
        if !config_file.exists() {
            tracing::info!(
                "No config file found, creating default at: {:?}",
                config_file
            );
            let default_config = Self::default();
            default_config.save()?;
        }

        let config = Figment::new()
            .merge(Toml::file(&config_file))
            .merge(Env::prefixed("SETU_"))
            .extract()
            .map_err(|e| SetuError::Config(Box::new(e)))?;

        let mut config: Config = config;
        config.interpolate_api_keys();
        Ok(config)
    }

    /// Interpolate environment variables in API keys after loading config
    pub fn interpolate_api_keys(&mut self) {
        for provider in self.providers.values_mut() {
            if let Some(ref api_key) = provider.api_key {
                provider.api_key = Some(interpolate_env_vars(api_key));
            }
        }
    }

    pub fn config_dir() -> Result<PathBuf> {
        get_config_dir()
    }

    pub fn data_dir() -> Result<PathBuf> {
        get_data_dir()
    }

    pub fn log_dir(&self) -> Result<PathBuf> {
        if let Some(log_dir) = &self.server.log_dir {
            let path = PathBuf::from(log_dir);
            std::fs::create_dir_all(&path)?;
            Ok(path)
        } else {
            let data_dir = Self::data_dir()?;
            let log_dir = data_dir.join("logs");
            std::fs::create_dir_all(&log_dir)?;
            Ok(log_dir)
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_dir = get_config_dir()?;
        let config_file = config_dir.join("setu.toml");

        let toml_string = toml::to_string_pretty(self)
            .map_err(|e| SetuError::Other(format!("Failed to serialize config: {}", e)))?;

        // Write config file with restricted permissions (600 - owner read/write only)
        std::fs::write(&config_file, toml_string)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&config_file)?.permissions();
            perms.set_mode(0o600); // Read/write for owner only
            std::fs::set_permissions(&config_file, perms)?;
        }
        Ok(())
    }
}

fn get_config_dir() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("", "", "setu")
        .ok_or_else(|| SetuError::Other("Could not determine config directory".to_string()))?;

    let config_dir = project_dirs.config_dir();
    std::fs::create_dir_all(config_dir)?;

    Ok(config_dir.to_path_buf())
}

fn get_data_dir() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("", "", "setu")
        .ok_or_else(|| SetuError::Other("Could not determine data directory".to_string()))?;

    let data_dir = project_dirs.data_dir();
    std::fs::create_dir_all(data_dir)?;

    Ok(data_dir.to_path_buf())
}

/// Interpolate environment variables in a string
/// Supports ${VAR} syntax, e.g., "${ANTHROPIC_API_KEY}"
fn interpolate_env_vars(value: &str) -> String {
    let mut result = value.to_string();

    // Find all ${VAR} patterns
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let replacement = std::env::var(var_name).unwrap_or_default();
            result.replace_range(start..start + end + 1, &replacement);
        } else {
            break; // Malformed pattern, stop processing
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_env_vars_valid() {
        unsafe {
            std::env::set_var("TEST_VAR", "test_value");
            std::env::set_var("ANOTHER_VAR", "another_value");
        }

        assert_eq!(interpolate_env_vars("${TEST_VAR}"), "test_value");
        assert_eq!(
            interpolate_env_vars("prefix_${TEST_VAR}_suffix"),
            "prefix_test_value_suffix"
        );
        assert_eq!(
            interpolate_env_vars("${TEST_VAR}_${ANOTHER_VAR}"),
            "test_value_another_value"
        );

        unsafe {
            std::env::remove_var("TEST_VAR");
            std::env::remove_var("ANOTHER_VAR");
        }
    }

    #[test]
    fn test_interpolate_env_vars_missing() {
        assert_eq!(interpolate_env_vars("${NONEXISTENT_VAR}"), "");
        assert_eq!(
            interpolate_env_vars("prefix_${NONEXISTENT_VAR}_suffix"),
            "prefix__suffix"
        );
    }

    #[test]
    fn test_interpolate_env_vars_malformed() {
        // Test malformed patterns - should stop processing and return partial result
        assert_eq!(interpolate_env_vars("${BROKEN"), "${BROKEN");
        assert_eq!(interpolate_env_vars("${INCOMPLETE_VAR"), "${INCOMPLETE_VAR");
        assert_eq!(interpolate_env_vars("${}"), "");

        // Test mixed valid/invalid
        unsafe {
            std::env::set_var("VALID_VAR", "valid");
        }
        assert_eq!(
            interpolate_env_vars("${VALID_VAR}_${BROKEN"),
            "valid_${BROKEN"
        );
        unsafe {
            std::env::remove_var("VALID_VAR");
        }
    }

    #[test]
    fn test_interpolate_env_vars_empty() {
        assert_eq!(interpolate_env_vars(""), "");
        assert_eq!(interpolate_env_vars("no_vars"), "no_vars");
    }

    #[test]
    fn test_interpolate_env_vars_nested_patterns() {
        // Edge case: patterns that look nested - processes first ${VAR} and leaves rest
        assert_eq!(interpolate_env_vars("${${VAR}}"), "}"); // ${VAR} becomes empty, leaving }

        // More realistic nested-looking case - processes ${SUFFIX} first, leaving malformed pattern
        assert_eq!(interpolate_env_vars("${PREFIX_${SUFFIX}}"), "}"); // ${SUFFIX} becomes empty, leaving }
    }

    #[test]
    fn test_model_route_serialization() {
        use serde_json;

        // Test single model serialization
        let single_route = ModelRoute::Single("openai/gpt-4o".to_string());
        let json = serde_json::to_string(&single_route).unwrap();
        assert_eq!(json, "\"openai/gpt-4o\"");

        // Test deserialization back
        let deserialized: ModelRoute = serde_json::from_str(&json).unwrap();
        match deserialized {
            ModelRoute::Single(model) => assert_eq!(model, "openai/gpt-4o"),
            _ => panic!("Should be Single variant"),
        }

        // Test multiple model serialization
        let multi_route = ModelRoute::Multiple(vec![
            "openai/gpt-4o".to_string(),
            "anthropic/claude-3-5-sonnet".to_string(),
        ]);
        let json = serde_json::to_string(&multi_route).unwrap();
        assert_eq!(json, "[\"openai/gpt-4o\",\"anthropic/claude-3-5-sonnet\"]");

        // Test deserialization back
        let deserialized: ModelRoute = serde_json::from_str(&json).unwrap();
        match deserialized {
            ModelRoute::Multiple(models) => {
                assert_eq!(models.len(), 2);
                assert_eq!(models[0], "openai/gpt-4o");
                assert_eq!(models[1], "anthropic/claude-3-5-sonnet");
            }
            _ => panic!("Should be Multiple variant"),
        }
    }

    #[test]
    fn test_provider_config_with_api_key_fallback() {
        // Test ProviderConfig with all new fields
        let provider_config = ProviderConfig {
            r#type: "anthropic".to_string(),
            endpoint: "https://api.anthropic.com".to_string(),
            auth: AuthConfig::default(),
            retry: RetryConfig::default(),
            api_key: Some("${ANTHROPIC_API_KEY}".to_string()),
            api_key_fallback: true,
            fallback_on_errors: vec![429, 401],
        };

        // Test serialization
        let toml_string = toml::to_string(&provider_config).unwrap();
        assert!(toml_string.contains("api_key = \"${ANTHROPIC_API_KEY}\""));
        assert!(toml_string.contains("api_key_fallback = true"));
        assert!(toml_string.contains("fallback_on_errors = [429, 401]"));

        // Test deserialization
        let deserialized: ProviderConfig = toml::from_str(&toml_string).unwrap();
        assert_eq!(
            deserialized.api_key,
            Some("${ANTHROPIC_API_KEY}".to_string())
        );
        assert!(deserialized.api_key_fallback);
        assert_eq!(deserialized.fallback_on_errors, vec![429, 401]);
    }

    #[test]
    fn test_provider_config_defaults() {
        // Test that ProviderConfig defaults work correctly
        let minimal_config = r#"
            type = "anthropic"
            endpoint = "https://api.anthropic.com"
            models = ["claude-3-5-sonnet"]
        "#;

        let provider_config: ProviderConfig = toml::from_str(minimal_config).unwrap();

        // Test defaults
        assert_eq!(provider_config.api_key, None);
        assert!(!provider_config.api_key_fallback);
        assert_eq!(provider_config.fallback_on_errors, vec![429]); // default_fallback_errors()
        assert_eq!(provider_config.retry.max_retries, 3);
        assert_eq!(provider_config.retry.initial_interval_ms, 1000);
    }

    #[test]
    fn test_routing_config_serialization() {
        let mut models = FxHashMap::default();
        models.insert(
            "haiku".to_string(),
            ModelRoute::Single("openai/gpt-4o-mini".to_string()),
        );
        models.insert(
            "best".to_string(),
            ModelRoute::Multiple(vec![
                "openai/gpt-4o".to_string(),
                "anthropic/claude-3-5-sonnet".to_string(),
            ]),
        );

        let routing_config = RoutingConfig { models };

        // Test serialization
        let toml_string = toml::to_string(&routing_config).unwrap();
        assert!(toml_string.contains("haiku = \"openai/gpt-4o-mini\""));
        assert!(
            toml_string.contains("best = [\"openai/gpt-4o\", \"anthropic/claude-3-5-sonnet\"]")
        );

        // Test deserialization
        let deserialized: RoutingConfig = toml::from_str(&toml_string).unwrap();
        assert_eq!(deserialized.models.len(), 2);

        match deserialized.models.get("haiku").unwrap() {
            ModelRoute::Single(model) => assert_eq!(model, "openai/gpt-4o-mini"),
            _ => panic!("Should be Single variant"),
        }

        match deserialized.models.get("best").unwrap() {
            ModelRoute::Multiple(models) => {
                assert_eq!(models.len(), 2);
                assert_eq!(models[0], "openai/gpt-4o");
            }
            _ => panic!("Should be Multiple variant"),
        }
    }

    #[test]
    fn test_config_with_api_key_interpolation() {
        // Set test environment variable
        unsafe {
            std::env::set_var("TEST_ANTHROPIC_KEY", "sk-ant-test123");
        }

        // Create config with interpolation
        let mut providers = FxHashMap::default();
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                r#type: "anthropic".to_string(),
                endpoint: "https://api.anthropic.com".to_string(),
                auth: AuthConfig::default(),
                retry: RetryConfig::default(),
                api_key: Some("${TEST_ANTHROPIC_KEY}".to_string()),
                api_key_fallback: true,
                fallback_on_errors: vec![429],
            },
        );

        let mut config = Config {
            server: ServerConfig::default(),
            providers,
            routing: RoutingConfig {
                models: FxHashMap::default(),
            },
            auth: FxHashMap::default(),
        };

        // Test interpolation
        config.interpolate_api_keys();

        let anthropic_config = config.providers.get("anthropic").unwrap();
        assert_eq!(anthropic_config.api_key, Some("sk-ant-test123".to_string()));

        // Cleanup
        unsafe {
            std::env::remove_var("TEST_ANTHROPIC_KEY");
        }
    }

    #[test]
    fn test_config_loading_creates_default_on_missing() {
        // This test would require filesystem access, so we test the logic
        let default_config = Config::default();

        // Verify default structure
        assert!(default_config.providers.is_empty());
        assert!(default_config.routing.models.is_empty());
        assert!(default_config.auth.is_empty());
    }

    #[test]
    fn test_retry_config_defaults() {
        let default_retry = RetryConfig::default();

        assert_eq!(default_retry.max_retries, 3);
        assert_eq!(default_retry.initial_interval_ms, 1000);
        assert_eq!(default_retry.max_interval_ms, 30000);
        assert_eq!(default_retry.multiplier, 2.0);
    }

    #[test]
    fn test_auth_config_token_expiry() {
        // Test token expiry logic
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Test expired token
        let expired_config = AuthConfig {
            oauth_access_token: Some("token".to_string()),
            oauth_refresh_token: Some("refresh".to_string()),
            oauth_expires: Some(now - 1000), // 1 second ago
            project_id: None,
        };
        assert!(expired_config.is_token_expired());
        assert!(expired_config.needs_refresh());

        // Test valid token
        let valid_config = AuthConfig {
            oauth_access_token: Some("token".to_string()),
            oauth_refresh_token: Some("refresh".to_string()),
            oauth_expires: Some(now + 3_600_000), // 1 hour from now
            project_id: None,
        };
        assert!(!valid_config.is_token_expired());
        assert!(!valid_config.needs_refresh());

        // Test token that needs refresh soon (within 10 minutes)
        let refresh_soon_config = AuthConfig {
            oauth_access_token: Some("token".to_string()),
            oauth_refresh_token: Some("refresh".to_string()),
            oauth_expires: Some(now + 300_000), // 5 minutes from now
            project_id: None,
        };
        assert!(!refresh_soon_config.is_token_expired());
        assert!(refresh_soon_config.needs_refresh());
    }
}
