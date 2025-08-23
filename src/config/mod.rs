use directories::ProjectDirs;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{Result, SetuError};

pub mod models;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_expires: Option<u64>,
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
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            oauth_access_token: None,
            oauth_refresh_token: None,
            oauth_expires: None,
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3742  // SETU on phone keypad: 7-3-8-5, but shifted to avoid common ports
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_provider() -> String {
    "openrouter".to_string()
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
            },
            auth: HashMap::new(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_dir = get_config_dir()?;
        let config_file = config_dir.join("setu.toml");

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
    let project_dirs = ProjectDirs::from("", "", "setu")
        .ok_or_else(|| SetuError::Config(figment::Error::from("Could not determine config directory")))?;
    
    let config_dir = project_dirs.config_dir();
    std::fs::create_dir_all(config_dir)?;
    
    Ok(config_dir.to_path_buf())
}

fn get_data_dir() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("", "", "setu")
        .ok_or_else(|| SetuError::Config(figment::Error::from("Could not determine data directory")))?;
    
    let data_dir = project_dirs.data_dir();
    std::fs::create_dir_all(data_dir)?;
    
    Ok(data_dir.to_path_buf())
}