use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::common::{analyze_token_source, choose_best_token_source};
use crate::config::AuthConfig;
use crate::error::{Result, SetuError};

const OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

#[derive(Debug, Deserialize, Serialize)]
struct CodexAuthJson {
    tokens: Option<CodexTokenData>,
    last_refresh: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexTokenData {
    id_token: String,
    access_token: String,
    refresh_token: String,
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenRefreshResponse {
    id_token: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct TokenRefreshRequest {
    client_id: String,
    grant_type: String,
    refresh_token: String,
    scope: String,
}

pub struct OpenAIOAuth;

impl OpenAIOAuth {
    /// Load codex CLI auth file
    fn load_codex_auth() -> Result<CodexAuthJson> {
        let home = std::env::var("HOME")
            .map_err(|_| SetuError::Other("HOME environment variable not set".to_string()))?;
        let codex_home = std::env::var("CODEX_HOME").unwrap_or_else(|_| format!("{}/.codex", home));
        let auth_path = PathBuf::from(codex_home).join("auth.json");

        let contents = fs::read_to_string(&auth_path)?;
        let auth_json: CodexAuthJson = serde_json::from_str(&contents)?;
        Ok(auth_json)
    }

    /// Save codex CLI auth file
    fn save_codex_auth(auth_json: &CodexAuthJson) -> Result<()> {
        let home = std::env::var("HOME")
            .map_err(|_| SetuError::Other("HOME environment variable not set".to_string()))?;
        let codex_home = std::env::var("CODEX_HOME").unwrap_or_else(|_| format!("{}/.codex", home));
        let auth_path = PathBuf::from(codex_home).join("auth.json");

        let contents = serde_json::to_string_pretty(auth_json)?;
        fs::write(auth_path, contents)?;
        Ok(())
    }

    /// Check if token needs refresh (28 days old)
    fn needs_refresh(auth_json: &CodexAuthJson) -> bool {
        auth_json
            .last_refresh
            .as_ref()
            .and_then(|lr| chrono::DateTime::parse_from_rfc3339(lr).ok())
            .map(|lr| {
                let now = chrono::Utc::now();
                let age = now.signed_duration_since(lr).num_days();
                age >= 28
            })
            .unwrap_or(true)
    }

    /// Refresh OAuth tokens
    async fn refresh_token(refresh_token: &str) -> Result<TokenRefreshResponse> {
        let client = reqwest::Client::new();
        let request = TokenRefreshRequest {
            client_id: OAUTH_CLIENT_ID.to_string(),
            grant_type: "refresh_token".to_string(),
            refresh_token: refresh_token.to_string(),
            scope: "openid profile email".to_string(),
        };

        let response = client
            .post(OAUTH_TOKEN_URL)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SetuError::Other(format!(
                "Token refresh failed with status {}: {}",
                status, error_text
            )));
        }

        let refresh_response: TokenRefreshResponse = response.json().await?;
        Ok(refresh_response)
    }

    /// Try to load OpenAI OAuth credentials from codex CLI
    pub async fn try_codex_cli_credentials() -> Result<AuthConfig> {
        let mut auth_json = Self::load_codex_auth()?;
        let tokens = auth_json.tokens.clone().ok_or_else(|| {
            SetuError::Other("No OAuth tokens found in codex CLI auth file".to_string())
        })?;

        // Refresh if needed
        if Self::needs_refresh(&auth_json) {
            tracing::info!("Refreshing OpenAI OAuth token");
            match Self::refresh_token(&tokens.refresh_token).await {
                Ok(refresh_response) => {
                    // Create updated tokens
                    let updated_tokens = CodexTokenData {
                        id_token: refresh_response.id_token,
                        access_token: refresh_response.access_token.unwrap_or(tokens.access_token),
                        refresh_token: refresh_response
                            .refresh_token
                            .unwrap_or(tokens.refresh_token),
                        account_id: tokens.account_id,
                    };

                    auth_json.tokens = Some(updated_tokens);
                    auth_json.last_refresh = Some(chrono::Utc::now().to_rfc3339());
                    Self::save_codex_auth(&auth_json)?;
                    tracing::info!("Successfully refreshed OpenAI OAuth token");
                }
                Err(e) => {
                    tracing::warn!("Failed to refresh OpenAI OAuth token: {}", e);
                    // Continue with existing token
                }
            }
        }

        // Get final tokens
        let final_tokens = auth_json.tokens.unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Ok(AuthConfig {
            oauth_access_token: Some(final_tokens.access_token),
            oauth_refresh_token: Some(final_tokens.refresh_token),
            oauth_expires: Some(now + (28 * 24 * 60 * 60 * 1000)), // 28 days
            project_id: final_tokens.account_id,
        })
    }

    /// Validate and refresh OpenAI OAuth config, choosing the best available tokens
    pub async fn validate_auth_config(auth_config: &mut AuthConfig) -> Result<()> {
        let setu_token_info = analyze_token_source("setu config", auth_config);
        let codex_token_info = Self::try_codex_cli_credentials()
            .await
            .map(|config| analyze_token_source("codex CLI", &config))
            .unwrap_or_else(|e| {
                tracing::debug!("Codex CLI credentials unavailable: {}", e);
                crate::auth::common::TokenInfo::unavailable("codex CLI")
            });

        // Choose the best token source
        let chosen_source = choose_best_token_source(&setu_token_info, &codex_token_info);
        tracing::info!("Using OpenAI OAuth tokens: {}", chosen_source);

        match chosen_source.source.as_str() {
            "codex CLI" => {
                if let Ok(codex_config) = Self::try_codex_cli_credentials().await {
                    *auth_config = codex_config;
                }
            }
            "setu config" => {
                // Current config is best - validate it's not expired
                if setu_token_info.is_expired {
                    return Err(SetuError::Other(
                        "Setu OpenAI OAuth token has expired and no refresh token is available"
                            .to_string(),
                    ));
                }
            }
            _ => {
                return Err(SetuError::Other(
                    "No valid OpenAI OAuth tokens available from setu config or codex CLI"
                        .to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Get a valid access token, refreshing if necessary
    pub async fn get_valid_access_token(auth_config: &mut AuthConfig) -> Result<String> {
        // Validate and potentially refresh the config
        Self::validate_auth_config(auth_config).await?;

        auth_config
            .oauth_access_token
            .clone()
            .ok_or_else(|| SetuError::Other("No OpenAI OAuth access token available".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_refresh_no_last_refresh() {
        let auth_json = CodexAuthJson {
            tokens: None,
            last_refresh: None,
        };
        assert!(OpenAIOAuth::needs_refresh(&auth_json));
    }

    #[test]
    fn test_needs_refresh_old_token() {
        let old_time = chrono::Utc::now() - chrono::Duration::days(30);
        let auth_json = CodexAuthJson {
            tokens: None,
            last_refresh: Some(old_time.to_rfc3339()),
        };
        assert!(OpenAIOAuth::needs_refresh(&auth_json));
    }

    #[test]
    fn test_needs_refresh_fresh_token() {
        let recent_time = chrono::Utc::now() - chrono::Duration::days(1);
        let auth_json = CodexAuthJson {
            tokens: None,
            last_refresh: Some(recent_time.to_rfc3339()),
        };
        assert!(!OpenAIOAuth::needs_refresh(&auth_json));
    }
}
