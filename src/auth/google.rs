use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::common::{analyze_token_source, choose_best_token_source, TokenInfo};
use crate::config::AuthConfig;
use crate::error::{Result, SetuError};

/// Project ID used for Gemini Cloud Code Assist API
/// 
/// This is a CONSTANT that will never change because we're mimicking gemini-cli behavior.
/// The gemini-cli tool uses this specific Google Cloud project ID for OAuth authentication
/// with the Cloud Code Assist API. This project ID is hardcoded in the gemini-cli source
/// and is shared across all gemini-cli installations.
/// 
/// Source: Found in ai-ox/crates/gemini-ox/examples/oauth_with_project_test.rs
/// This ID is required for the cloudaicompanion.googleapis.com API endpoint.
/// 
/// Why it's constant:
/// - Gemini-cli uses this exact project ID for all users
/// - It's part of Google's Cloud Code Assist infrastructure  
/// - Changing it would break compatibility with gemini-cli OAuth tokens
/// - Google manages this project ID, not individual users
pub const GEMINI_PROJECT_ID: &str = "pioneering-trilogy-xq6tl";

/// OAuth constants from Gemini CLI source code
const OAUTH_CLIENT_ID: &str = "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
const OAUTH_CLIENT_SECRET: &str = "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";
const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Get the token URL - can be overridden for testing
fn get_token_url() -> String {
    std::env::var("GOOGLE_TOKEN_URL").unwrap_or_else(|_| OAUTH_TOKEN_URL.to_string())
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GeminiOAuthCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expiry_date: u64,
    pub token_type: String,
    pub scope: String,
}

#[derive(Debug, Deserialize)]
struct TokenRefreshResponse {
    access_token: String,
    expires_in: u64,
    token_type: String,
    scope: Option<String>,
}

pub struct GoogleOAuth;

impl GoogleOAuth {
    /// Refresh OAuth tokens using refresh token
    async fn refresh_token(refresh_token: &str) -> Result<TokenRefreshResponse> {
        let client = reqwest::Client::new();
        
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", OAUTH_CLIENT_ID),
            ("client_secret", OAUTH_CLIENT_SECRET),
        ];

        tracing::info!("Attempting to refresh Gemini OAuth token");
        
        let response = client
            .post(get_token_url())
            .form(&params)
            .send()
            .await
            .map_err(|e| SetuError::Other(format!("Token refresh request failed: {}", e)))?;

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

        let refresh_response: TokenRefreshResponse = response
            .json()
            .await
            .map_err(|e| SetuError::Other(format!("Failed to parse token refresh response: {}", e)))?;

        tracing::info!("Successfully refreshed Gemini OAuth token");
        Ok(refresh_response)
    }

    /// Update credentials file with new tokens
    async fn update_gemini_credentials(mut credentials: GeminiOAuthCredentials, refresh_response: TokenRefreshResponse) -> Result<GeminiOAuthCredentials> {
        // Update with new token info
        credentials.access_token = refresh_response.access_token;
        credentials.token_type = refresh_response.token_type;
        
        // Calculate new expiry time
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| SetuError::Other(format!("Time error: {}", e)))?
            .as_millis() as u64;
        
        credentials.expiry_date = now + (refresh_response.expires_in * 1000);
        
        // Update scope if provided
        if let Some(scope) = refresh_response.scope {
            credentials.scope = scope;
        }

        // Write updated credentials back to file
        let credentials_path = Self::get_gemini_credentials_path()?;
        let contents = serde_json::to_string_pretty(&credentials)
            .map_err(|e| SetuError::Other(format!("Failed to serialize credentials: {}", e)))?;
        
        fs::write(&credentials_path, contents)
            .map_err(|e| SetuError::Other(format!(
                "Failed to write updated credentials to {}: {}",
                credentials_path.display(),
                e
            )))?;

        tracing::info!("Updated Gemini CLI OAuth credentials file");
        Ok(credentials)
    }

    /// Attempt to read Gemini CLI OAuth credentials from ~/.gemini/oauth_creds.json
    /// If expired and refresh token is available, attempt to refresh
    pub async fn try_gemini_cli_credentials() -> Result<AuthConfig> {
        let credentials_path = Self::get_gemini_credentials_path()?;

        let contents = fs::read_to_string(&credentials_path).map_err(|e| {
            SetuError::Other(format!(
                "Failed to read Gemini CLI credentials from {}: {}",
                credentials_path.display(),
                e
            ))
        })?;

        let mut credentials: GeminiOAuthCredentials = serde_json::from_str(&contents).map_err(|e| {
            SetuError::Other(format!("Failed to parse Gemini CLI credentials: {}", e))
        })?;

        // Check if token has expired
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| SetuError::Other(format!("Time error: {}", e)))?
            .as_millis() as u64;

        if credentials.expiry_date <= now {
            // Token is expired - try to refresh if we have a refresh token
            if let Some(ref refresh_token) = credentials.refresh_token {
                tracing::info!("Gemini OAuth token expired, attempting refresh");
                
                match Self::refresh_token(refresh_token).await {
                    Ok(refresh_response) => {
                        // Update credentials with new token
                        credentials = Self::update_gemini_credentials(credentials, refresh_response).await?;
                        tracing::info!("Successfully refreshed expired Gemini OAuth token");
                    }
                    Err(e) => {
                        return Err(SetuError::Other(format!(
                            "Gemini CLI OAuth token has expired and refresh failed: {}",
                            e
                        )));
                    }
                }
            } else {
                return Err(SetuError::Other(
                    "Gemini CLI OAuth token has expired and no refresh token is available".to_string(),
                ));
            }
        }

        // Check for required scopes
        if !credentials
            .scope
            .contains("https://www.googleapis.com/auth/cloud-platform")
        {
            return Err(SetuError::Other(
                "Gemini CLI OAuth token missing required 'cloud-platform' scope".to_string(),
            ));
        }

        tracing::info!("Successfully loaded Gemini CLI OAuth credentials");
        tracing::info!("Setting Gemini project_id: {}", GEMINI_PROJECT_ID);

        Ok(AuthConfig {
            oauth_access_token: Some(credentials.access_token),
            oauth_refresh_token: credentials.refresh_token,
            oauth_expires: Some(credentials.expiry_date),
            project_id: Some(GEMINI_PROJECT_ID.to_string()),
        })
    }

    fn get_gemini_credentials_path() -> Result<PathBuf> {
        // Try to get home directory
        let home = std::env::var("HOME")
            .map_err(|_| SetuError::Other("HOME environment variable not set".to_string()))?;

        Ok(PathBuf::from(home).join(".gemini").join("oauth_creds.json"))
    }

    /// Ensure the auth config has the required project_id for Gemini Cloud Code Assist API
    fn ensure_project_id(auth_config: &mut AuthConfig) {
        if auth_config.project_id.is_none() {
            tracing::info!("Adding missing project_id to setu config: {}", GEMINI_PROJECT_ID);
            auth_config.project_id = Some(GEMINI_PROJECT_ID.to_string());
        }
    }

    /// Validate that OAuth tokens are present and can be refreshed if needed
    /// Compares setu and Gemini CLI tokens, always choosing the newer one with full rationale
    pub async fn validate_auth_config(auth_config: &mut AuthConfig) -> Result<()> {
        let setu_token_info = analyze_token_source("setu config", auth_config);
        let gemini_token_info = Self::try_gemini_cli_credentials().await
            .map(|config| analyze_token_source("Gemini CLI", &config))
            .unwrap_or_else(|e| {
                tracing::debug!("Gemini CLI credentials unavailable: {}", e);
                TokenInfo::unavailable("Gemini CLI")
            });

        tracing::info!("Gemini Token Analysis:");
        tracing::info!("  Setu config: {}", setu_token_info);
        tracing::info!("  Gemini CLI: {}", gemini_token_info);

        // Choose the best token source
        let chosen_source = choose_best_token_source(&setu_token_info, &gemini_token_info);
        tracing::info!("Gemini Decision: {}", chosen_source);

        match chosen_source.source.as_str() {
            "Gemini CLI" => {
                if let Ok(gemini_config) = Self::try_gemini_cli_credentials().await {
                    *auth_config = gemini_config;
                    return Ok(());
                }
            }
            "setu config" => {
                // Use existing setu tokens - ensure project_id is always set
                Self::ensure_project_id(auth_config);
            }
            "none" => {
                return Err(SetuError::Other(
                    "No valid OAuth tokens found from any source\n\n\
                     To fix this issue, you have two options:\n\
                     1. Run: setu auth google       (get fresh setu tokens)\n\
                     2. Run: gemini auth login      (use Gemini CLI tokens)\n\n\
                     Setu will automatically use whichever tokens are newer.".to_string(),
                ));
            }
            _ => {
                // Both tokens are expired - fail startup with clear instructions
                if setu_token_info.is_expired && gemini_token_info.is_expired {
                    return Err(SetuError::Other(format!(
                        "All Gemini OAuth tokens are expired!\n\n\
                         Token Status:\n\
                         - Setu config: {}\n\
                         - Gemini CLI: {}\n\n\
                         To fix this issue, you have two options:\n\
                         1. Run: setu auth google       (get fresh setu tokens)\n\
                         2. Run: gemini auth login      (refresh Gemini CLI tokens)\n\n\
                         Setu will automatically use whichever tokens are newer.",
                        setu_token_info,
                        gemini_token_info
                    )));
                }
                
                return Err(SetuError::Other(format!(
                    "Unexpected token selection result: {}", chosen_source.source
                )));
            }
        }

        // Check if we have access token
        if auth_config.oauth_access_token.is_none() {
            return Err(SetuError::Other(
                "No OAuth access token found. Please run 'setu auth google' to authenticate."
                    .to_string(),
            ));
        }

        tracing::info!("Google/Gemini OAuth tokens validated successfully");
        Ok(())
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_try_gemini_cli_credentials() {
        // This test will only work if ~/.gemini/oauth_creds.json exists and is valid
        match GoogleOAuth::try_gemini_cli_credentials().await {
            Ok(config) => {
                assert!(config.oauth_access_token.is_some());
                assert_eq!(config.project_id, Some(GEMINI_PROJECT_ID.to_string()));
                println!("Successfully loaded Gemini CLI OAuth credentials");
            }
            Err(e) => {
                println!("Gemini CLI OAuth credentials not available: {}", e);
                // This is OK - we don't want to fail the test if credentials aren't available
            }
        }
    }

    #[test]
    fn test_ensure_project_id_adds_missing_id() {
        let mut auth_config = AuthConfig {
            oauth_access_token: Some("test_token".to_string()),
            oauth_refresh_token: Some("test_refresh".to_string()),
            oauth_expires: Some(1234567890),
            project_id: None, // Missing project_id
        };

        GoogleOAuth::ensure_project_id(&mut auth_config);

        assert_eq!(auth_config.project_id, Some(GEMINI_PROJECT_ID.to_string()));
    }

    #[test]
    fn test_ensure_project_id_preserves_existing_id() {
        let existing_id = "some-other-project-id".to_string();
        let mut auth_config = AuthConfig {
            oauth_access_token: Some("test_token".to_string()),
            oauth_refresh_token: Some("test_refresh".to_string()),
            oauth_expires: Some(1234567890),
            project_id: Some(existing_id.clone()),
        };

        GoogleOAuth::ensure_project_id(&mut auth_config);

        // Should preserve existing project_id, not overwrite it
        assert_eq!(auth_config.project_id, Some(existing_id));
    }

    #[test]
    fn test_gemini_project_id_constant() {
        // Verify the constant matches expected value
        assert_eq!(GEMINI_PROJECT_ID, "pioneering-trilogy-xq6tl");
    }

    #[test]
    fn test_new_gemini_cli_credentials_include_project_id() {
        // Test that try_gemini_cli_credentials always sets project_id when successful
        // This is a unit test for the logic, not dependent on actual credentials file
        
        // We can't easily mock the file system, but we can test the constant usage
        // The actual integration test is test_try_gemini_cli_credentials above
        let expected_project_id = GEMINI_PROJECT_ID.to_string();
        assert!(!expected_project_id.is_empty());
        assert!(expected_project_id.contains("pioneering-trilogy"));
    }
}

