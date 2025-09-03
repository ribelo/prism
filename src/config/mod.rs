use directories::ProjectDirs;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{Result, SetuError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub providers: HashMap<String, ProviderConfig>,
    pub routing: RoutingConfig,
    #[serde(default)]
    pub auth: HashMap<String, AuthConfig>,
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
pub struct ProviderConfig {
    pub r#type: String,
    pub endpoint: String,
    pub models: Vec<String>,
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    #[serde(default = "default_provider")]
    pub default_provider: String,

    /// Routing strategy: "composite", "model", or "provider"
    #[serde(default = "default_routing_strategy")]
    pub strategy: String,

    /// Enable fallback routing when primary router fails
    #[serde(default = "default_enable_fallback")]
    pub enable_fallback: bool,

    /// Minimum confidence threshold for routing decisions (0.0 to 1.0)
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,

    /// Routing rules for model patterns
    #[serde(default)]
    pub rules: std::collections::HashMap<String, String>,

    /// Provider priorities (first = highest priority)
    #[serde(default)]
    pub provider_priorities: Vec<String>,

    /// Provider capabilities mapping
    #[serde(default)]
    pub provider_capabilities: std::collections::HashMap<String, Vec<String>>,

    /// Provider aliases
    #[serde(default)]
    pub provider_aliases: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            oauth_access_token: None,
            oauth_refresh_token: None,
            oauth_expires: None,
            project_id: None,
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

fn default_provider() -> String {
    "openrouter".to_string()
}

fn default_routing_strategy() -> String {
    "composite".to_string()
}

fn default_enable_fallback() -> bool {
    true
}

fn default_min_confidence() -> f64 {
    0.0
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

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            providers: HashMap::new(),
            routing: RoutingConfig {
                default_provider: default_provider(),
                strategy: default_routing_strategy(),
                enable_fallback: default_enable_fallback(),
                min_confidence: default_min_confidence(),
                rules: std::collections::HashMap::new(),
                provider_priorities: Vec::new(),
                provider_capabilities: std::collections::HashMap::new(),
                provider_aliases: std::collections::HashMap::new(),
            },
            auth: HashMap::new(),
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
            .extract()?;

        Ok(config)
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

        std::fs::write(&config_file, toml_string)?;
        Ok(())
    }
}

fn get_config_dir() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("", "", "setu").ok_or_else(|| {
        SetuError::Config(figment::Error::from("Could not determine config directory"))
    })?;

    let config_dir = project_dirs.config_dir();
    std::fs::create_dir_all(config_dir)?;

    Ok(config_dir.to_path_buf())
}

fn get_data_dir() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("", "", "setu").ok_or_else(|| {
        SetuError::Config(figment::Error::from("Could not determine data directory"))
    })?;

    let data_dir = project_dirs.data_dir();
    std::fs::create_dir_all(data_dir)?;

    Ok(data_dir.to_path_buf())
}
