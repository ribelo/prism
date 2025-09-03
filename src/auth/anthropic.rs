use base64::Engine;
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

use crate::auth::common::{analyze_token_source, choose_best_token_source, TokenInfo};
use crate::config::AuthConfig;
use crate::error::{Result, SetuError};

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

#[derive(Debug, Clone)]
pub struct AuthorizeResult {
    pub url: String,
    pub verifier: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    refresh_token: String,
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct ClaudeCodeOAuth {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: u64,
    scopes: Vec<String>,
    #[serde(rename = "subscriptionType")]
    #[allow(dead_code)]
    subscription_type: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeCodeCredentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: ClaudeCodeOAuth,
}

pub struct AnthropicOAuth;

impl AnthropicOAuth {
    /// Attempt to read Claude Code OAuth credentials from ~/.config/claude/.credentials.json
    pub fn try_claude_code_credentials() -> Result<AuthConfig> {
        let credentials_path = Self::get_claude_credentials_path()?;

        let contents = fs::read_to_string(&credentials_path).map_err(|e| {
            SetuError::Other(format!(
                "Failed to read Claude Code credentials from {}: {}",
                credentials_path.display(),
                e
            ))
        })?;

        let credentials: ClaudeCodeCredentials = serde_json::from_str(&contents).map_err(|e| {
            SetuError::Other(format!("Failed to parse Claude Code credentials: {}", e))
        })?;

        let oauth = &credentials.claude_ai_oauth;

        // Validate token hasn't expired
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| SetuError::Other(format!("Time error: {}", e)))?
            .as_millis() as u64;

        if oauth.expires_at <= now {
            return Err(SetuError::Other(
                "Claude Code OAuth token has expired".to_string(),
            ));
        }

        // Check for required scopes
        if !oauth.scopes.contains(&"user:inference".to_string()) {
            return Err(SetuError::Other(
                "Claude Code OAuth token missing required 'user:inference' scope".to_string(),
            ));
        }

        tracing::info!("Successfully loaded Claude Code OAuth credentials");

        Ok(AuthConfig {
            oauth_access_token: Some(oauth.access_token.clone()),
            oauth_refresh_token: Some(oauth.refresh_token.clone()),
            oauth_expires: Some(oauth.expires_at),
            project_id: None,
        })
    }

    fn get_claude_credentials_path() -> Result<PathBuf> {
        // Try to get home directory
        let home = std::env::var("HOME")
            .map_err(|_| SetuError::Other("HOME environment variable not set".to_string()))?;

        Ok(PathBuf::from(home)
            .join(".config")
            .join("claude")
            .join(".credentials.json"))
    }

    pub fn generate_pkce_pair() -> (String, String) {
        let mut rng = rand::thread_rng();
        let mut verifier_bytes = [0u8; 32];
        rng.fill(&mut verifier_bytes);
        let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier_bytes);

        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge_bytes = hasher.finalize();
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(challenge_bytes);

        (challenge, verifier)
    }

    pub fn create_authorization_url() -> Result<AuthorizeResult> {
        let (pkce_challenge, pkce_verifier) = Self::generate_pkce_pair();

        let mut url = Url::parse("https://claude.ai/oauth/authorize")
            .map_err(|e| SetuError::Other(format!("Failed to parse OAuth URL: {}", e)))?;

        url.query_pairs_mut()
            .append_pair("code", "true")
            .append_pair("client_id", CLIENT_ID)
            .append_pair("response_type", "code")
            .append_pair(
                "redirect_uri",
                "https://console.anthropic.com/oauth/code/callback",
            )
            .append_pair("scope", "org:create_api_key user:profile user:inference")
            .append_pair("code_challenge", &pkce_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &pkce_verifier);

        Ok(AuthorizeResult {
            url: url.to_string(),
            verifier: pkce_verifier,
        })
    }

    pub async fn exchange_code_for_token(code: &str, verifier: &str) -> Result<AuthConfig> {
        let splits: Vec<&str> = code.split('#').collect();

        let request_body = serde_json::json!({
            "code": splits[0],
            "state": splits.get(1).unwrap_or(&""),
            "grant_type": "authorization_code",
            "client_id": CLIENT_ID,
            "redirect_uri": "https://console.anthropic.com/oauth/code/callback",
            "code_verifier": verifier,
        });

        let client = reqwest::Client::new();
        let response = client
            .post("https://console.anthropic.com/v1/oauth/token")
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| SetuError::Other(format!("OAuth token exchange failed: {}", e)))?;

        if !response.status().is_success() {
            // Store the status before moving the response
            let status = response.status();

            // Try to get the error details from the response body
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error body".to_string());

            let error_msg = format!(
                "OAuth token exchange failed with status: {} - Body: {}",
                status, error_body
            );
            return Err(SetuError::Other(error_msg));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| SetuError::Other(format!("Failed to parse token response: {}", e)))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| SetuError::Other(format!("Time error: {}", e)))?
            .as_millis() as u64;

        Ok(AuthConfig {
            oauth_access_token: Some(token_response.access_token),
            oauth_refresh_token: Some(token_response.refresh_token),
            oauth_expires: Some(now + (token_response.expires_in * 1000)),
            project_id: None,
        })
    }

    pub async fn refresh_token(auth_config: &mut AuthConfig) -> Result<()> {
        let refresh_token = auth_config
            .oauth_refresh_token
            .as_ref()
            .ok_or_else(|| SetuError::Other("No refresh token available".to_string()))?;

        tracing::info!("Attempting to refresh Anthropic OAuth token");

        let request_body = serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLIENT_ID,
        });

        let client = reqwest::Client::new();
        let response = client
            .post("https://console.anthropic.com/v1/oauth/token")
            .header("Content-Type", "application/json")
            .header("anthropic-beta", 
                "oauth-2025-04-20,claude-code-20250219,interleaved-thinking-2025-05-14,fine-grained-tool-streaming-2025-05-14")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| SetuError::Other(format!("Token refresh request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error body".to_string());

            return Err(SetuError::Other(format!(
                "Token refresh failed with status {}: {}",
                status, error_body
            )));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| SetuError::Other(format!("Failed to parse refresh response: {}", e)))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| SetuError::Other(format!("Time error: {}", e)))?
            .as_millis() as u64;

        // Update auth config with new tokens
        auth_config.oauth_access_token = Some(token_response.access_token);
        auth_config.oauth_refresh_token = Some(token_response.refresh_token);
        auth_config.oauth_expires = Some(now + (token_response.expires_in * 1000));

        tracing::info!("Successfully refreshed Anthropic OAuth token");
        Ok(())
    }

    /// Check if token needs refresh and refresh if necessary
    /// Returns the current valid access token
    pub async fn get_valid_access_token(
        auth_config: &mut AuthConfig,
        persist_tokens: bool,
    ) -> Result<String> {
        // Check if token is expired or missing
        if auth_config.oauth_access_token.is_none() || auth_config.is_token_expired() {
            if auth_config.oauth_refresh_token.is_some() {
                tracing::debug!("Token expired, refreshing automatically");
                Self::refresh_token(auth_config).await?;
                
                if persist_tokens {
                    // Simple persistence without full config reload
                    if let Err(e) = Self::persist_tokens_to_config(auth_config).await {
                        tracing::warn!("Failed to persist refreshed tokens: {}", e);
                        // Continue anyway - in-memory tokens are still valid
                    }
                }
            } else {
                return Err(SetuError::Other("No refresh token available and access token is expired".to_string()));
            }
        }
        
        auth_config.oauth_access_token
            .clone()
            .ok_or_else(|| SetuError::Other("No access token available after refresh".to_string()))
    }

    /// Simple token persistence helper
    async fn persist_tokens_to_config(auth_config: &AuthConfig) -> Result<()> {
        use crate::Config;
        
        let mut config = Config::load()?;
        if let Some(provider) = config.providers.get_mut("anthropic") {
            provider.auth = auth_config.clone();
            config.save()?;
            tracing::debug!("Persisted refreshed tokens to config");
        }
        Ok(())
    }

    /// Validate OAuth tokens - use newest available, fail if none valid
    pub async fn validate_auth_config(auth_config: &mut AuthConfig) -> Result<()> {
        let setu_token_info = analyze_token_source("setu config", auth_config);
        let claude_token_info = Self::try_claude_code_credentials()
            .map(|config| analyze_token_source("Claude Code", &config))
            .unwrap_or_else(|_| TokenInfo::unavailable("Claude Code"));

        let chosen_source = choose_best_token_source(&setu_token_info, &claude_token_info);
        tracing::info!("Using {} tokens", chosen_source.source);

        match chosen_source.source.as_str() {
            "Claude Code" => {
                *auth_config = Self::try_claude_code_credentials()?;
            }
            "setu config" => {
                // Use existing setu tokens - validation continues below
            }
            _ => {
                return Err(SetuError::Other(
                    "No valid OAuth tokens available. Run 'setu auth anthropic' or start Claude Code first.".to_string(),
                ));
            }
        }

        // Ensure we have valid tokens
        if auth_config.oauth_refresh_token.is_none() {
            return Err(SetuError::Other(
                "No refresh token available. Run 'setu auth anthropic'.".to_string(),
            ));
        }

        if auth_config.oauth_access_token.is_none() || auth_config.is_token_expired() {
            Self::refresh_token(auth_config).await?;
        }

        if auth_config.oauth_access_token.is_none() {
            return Err(SetuError::Other(
                "Failed to obtain valid access token.".to_string(),
            ));
        }

        Ok(())
    }

}
