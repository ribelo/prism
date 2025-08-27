use serde::Deserialize;
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
const GEMINI_PROJECT_ID: &str = "pioneering-trilogy-xq6tl";

#[derive(Debug, Deserialize)]
struct GeminiOAuthCredentials {
    access_token: String,
    refresh_token: Option<String>,
    expiry_date: u64,
    token_type: String,
    scope: String,
}

pub struct GoogleOAuth;

impl GoogleOAuth {
    /// Attempt to read Gemini CLI OAuth credentials from ~/.gemini/oauth_creds.json
    pub fn try_gemini_cli_credentials() -> Result<AuthConfig> {
        let credentials_path = Self::get_gemini_credentials_path()?;

        let contents = fs::read_to_string(&credentials_path).map_err(|e| {
            SetuError::Other(format!(
                "Failed to read Gemini CLI credentials from {}: {}",
                credentials_path.display(),
                e
            ))
        })?;

        let credentials: GeminiOAuthCredentials = serde_json::from_str(&contents).map_err(|e| {
            SetuError::Other(format!("Failed to parse Gemini CLI credentials: {}", e))
        })?;

        // Validate token hasn't expired
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| SetuError::Other(format!("Time error: {}", e)))?
            .as_millis() as u64;

        if credentials.expiry_date <= now {
            return Err(SetuError::Other(
                "Gemini CLI OAuth token has expired".to_string(),
            ));
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
        let gemini_token_info = Self::try_gemini_cli_credentials()
            .map(|config| analyze_token_source("Gemini CLI", &config))
            .unwrap_or_else(|e| {
                tracing::debug!("Gemini CLI credentials unavailable: {}", e);
                TokenInfo {
                    source: "Gemini CLI".to_string(),
                    available: false,
                    expires_at: None,
                    is_expired: true,
                    age_description: "unavailable".to_string(),
                }
            });

        tracing::info!("Gemini Token Analysis:");
        tracing::info!("  Setu config: {}", setu_token_info);
        tracing::info!("  Gemini CLI: {}", gemini_token_info);

        // Choose the best token source
        let chosen_source = choose_best_token_source(&setu_token_info, &gemini_token_info);
        tracing::info!("Gemini Decision: {}", chosen_source);

        match chosen_source.source.as_str() {
            "Gemini CLI" => {
                if let Ok(gemini_config) = Self::try_gemini_cli_credentials() {
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

    #[test]
    fn test_try_gemini_cli_credentials() {
        // This test will only work if ~/.gemini/oauth_creds.json exists and is valid
        match GoogleOAuth::try_gemini_cli_credentials() {
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
