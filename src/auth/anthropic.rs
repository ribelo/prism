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

        let request_body = serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLIENT_ID,
        });

        let client = reqwest::Client::new();
        let response = client
            .post("https://console.anthropic.com/v1/oauth/token")
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| SetuError::Other(format!("Token refresh failed: {}", e)))?;

        if !response.status().is_success() {
            // Store the status before moving the response
            let status = response.status();

            // Try to get the error details from the response body
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error body".to_string());

            return Err(SetuError::Other(format!(
                "Token refresh failed with status: {} - Body: {}",
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

        auth_config.oauth_access_token = Some(token_response.access_token);
        auth_config.oauth_refresh_token = Some(token_response.refresh_token);
        auth_config.oauth_expires = Some(now + (token_response.expires_in * 1000));

        Ok(())
    }

    /// Validate that OAuth tokens are present and can be refreshed if needed
    /// Compares setu and Claude Code tokens, always choosing the newer one with full rationale
    pub async fn validate_auth_config(auth_config: &mut AuthConfig) -> Result<()> {
        let setu_token_info = analyze_token_source("setu config", auth_config);
        let claude_token_info = Self::try_claude_code_credentials()
            .map(|config| analyze_token_source("Claude Code", &config))
            .unwrap_or_else(|e| {
                tracing::debug!("Claude Code credentials unavailable: {}", e);
                TokenInfo {
                    source: "Claude Code".to_string(),
                    available: false,
                    expires_at: None,
                    is_expired: true,
                    age_description: "unavailable".to_string(),
                }
            });

        tracing::info!("🔍 Token Analysis:");
        tracing::info!("  📋 Setu config: {}", setu_token_info);
        tracing::info!("  🤖 Claude Code: {}", claude_token_info);

        // Choose the best token source
        let chosen_source = choose_best_token_source(&setu_token_info, &claude_token_info);
        tracing::info!("✅ Decision: {}", chosen_source);

        match chosen_source.source.as_str() {
            "Claude Code" => {
                if let Ok(claude_config) = Self::try_claude_code_credentials() {
                    *auth_config = claude_config;
                    return Ok(());
                }
            }
            "setu config" => {
                // Use existing setu tokens - validation continues below
            }
            "none" => {
                return Err(SetuError::Other(
                    "❌ No valid OAuth tokens found from any source\n\n\
                     🔧 To fix this issue, you have two options:\n\
                     1. Run: setu auth anthropic    (get fresh setu tokens)\n\
                     2. Start Claude Code first     (use Claude Code's tokens)\n\n\
                     💡 Setu will automatically use whichever tokens are newer.".to_string(),
                ));
            }
            _ => {
                // Both tokens are expired - fail startup with clear instructions
                if setu_token_info.is_expired && claude_token_info.is_expired {
                    return Err(SetuError::Other(format!(
                        "❌ All OAuth tokens are expired!\n\n\
                         📊 Token Status:\n\
                         • Setu config: {}\n\
                         • Claude Code: {}\n\n\
                         🔧 To fix this issue, you have two options:\n\
                         1. Run: setu auth anthropic    (get fresh setu tokens)\n\
                         2. Start Claude Code first     (refresh Claude Code's tokens)\n\n\
                         💡 Setu will automatically use whichever tokens are newer.",
                        setu_token_info,
                        claude_token_info
                    )));
                }
                
                return Err(SetuError::Other(format!(
                    "Unexpected token selection result: {}", chosen_source.source
                )));
            }
        }

        // Check if we have refresh token
        let _refresh_token = auth_config.oauth_refresh_token.as_ref().ok_or_else(|| {
            SetuError::Other(
                "No OAuth refresh token found. Please run 'setu auth anthropic' to authenticate."
                    .to_string(),
            )
        })?;

        // If access token is missing or expired, try to refresh
        if auth_config.oauth_access_token.is_none() || auth_config.is_token_expired() {
            tracing::info!("OAuth access token is missing or expired, attempting refresh...");
            Self::refresh_token(auth_config).await?;
        }

        // Verify we now have a valid access token
        if auth_config.oauth_access_token.is_none() {
            return Err(SetuError::Other(
                "No OAuth access token available after refresh attempt.".to_string(),
            ));
        }

        tracing::info!("Anthropic OAuth tokens validated successfully");
        Ok(())
    }

}
