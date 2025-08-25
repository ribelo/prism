use base64::Engine;
use rand::Rng;
use sha2::{Digest, Sha256};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

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

pub struct AnthropicOAuth;

impl AnthropicOAuth {
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
            .append_pair("redirect_uri", "https://console.anthropic.com/oauth/code/callback")
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
            let error_body = response.text().await
                .unwrap_or_else(|_| "Unable to read error body".to_string());
            
            let error_msg = format!("OAuth token exchange failed with status: {} - Body: {}", 
                status, error_body);
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
        })
    }

    pub async fn refresh_token(auth_config: &mut AuthConfig) -> Result<()> {
        let refresh_token = auth_config.oauth_refresh_token.as_ref()
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
            let error_body = response.text().await
                .unwrap_or_else(|_| "Unable to read error body".to_string());
            
            return Err(SetuError::Other(
                format!("Token refresh failed with status: {} - Body: {}", 
                    status, error_body)
            ));
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
    pub async fn validate_auth_config(auth_config: &mut AuthConfig) -> Result<()> {
        // Check if we have refresh token
        let _refresh_token = auth_config.oauth_refresh_token.as_ref()
            .ok_or_else(|| SetuError::Other("No OAuth refresh token found. Please run 'setu auth anthropic' to authenticate.".to_string()))?;

        // If access token is missing or expired, try to refresh
        if auth_config.oauth_access_token.is_none() || auth_config.is_token_expired() {
            tracing::info!("OAuth access token is missing or expired, attempting refresh...");
            Self::refresh_token(auth_config).await?;
        }

        // Verify we now have a valid access token
        if auth_config.oauth_access_token.is_none() {
            return Err(SetuError::Other("No OAuth access token available after refresh attempt.".to_string()));
        }

        tracing::info!("Anthropic OAuth tokens validated successfully");
        Ok(())
    }
}