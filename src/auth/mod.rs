pub mod anthropic;

use crate::config::AuthConfig;
use crate::error::{Result, SetuError};
use std::time::{SystemTime, UNIX_EPOCH};

pub trait AuthProvider {
    fn is_oauth(&self) -> bool;
    fn get_auth_header(&self) -> Result<String>;
    fn is_token_expired(&self) -> bool;
}

impl AuthProvider for AuthConfig {
    fn is_oauth(&self) -> bool {
        self.oauth_access_token.is_some()
    }

    fn get_auth_header(&self) -> Result<String> {
        if let Some(oauth_token) = &self.oauth_access_token {
            Ok(format!("Bearer {}", oauth_token))
        } else {
            Err(SetuError::Config(figment::Error::from("No OAuth credentials found in config. API keys should be set via environment variables.")))
        }
    }

    fn is_token_expired(&self) -> bool {
        if let Some(expires) = self.oauth_expires {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            expires <= now
        } else {
            false // OAuth tokens don't expire without expires field
        }
    }
}