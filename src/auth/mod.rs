pub mod anthropic;
pub mod common;
pub mod google;
pub mod openai;

use crate::config::AuthConfig;
use crate::error::{Result, PrismError};
use std::time::{SystemTime, UNIX_EPOCH};

/// Cached authentication method to avoid checking tokens on every request
#[derive(Debug, Clone)]
pub struct AuthCache {
    pub anthropic_method: AuthMethod,
    pub gemini_method: AuthMethod,
    pub openai_method: AuthMethod,
    pub cached_at: SystemTime,
}

/// Authentication method determined at startup
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// OAuth authentication with token source and actual token
    OAuth { source: String, token: String },
    /// API key authentication (no OAuth tokens available)
    ApiKey,
    /// Provider unavailable (no tokens found or expired)
    Unavailable { reason: String },
}

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
            Err(PrismError::Other(
                "No OAuth credentials found in config. API keys should be set via environment variables.".to_string(),
            ))
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

/// Initialize authentication cache by checking OAuth tokens once at startup
pub async fn initialize_auth_cache() -> Result<AuthCache> {
    use tracing::info;

    info!("Checking OAuth token availability...");

    let cached_at = SystemTime::now();

    // Check Anthropic tokens - fail startup if expired
    let anthropic_method = determine_anthropic_auth_method()?;

    // Check Gemini tokens - fail startup if expired
    let gemini_method = determine_gemini_auth_method().await?;

    // Check OpenAI tokens - don't fail startup if unavailable
    let openai_method = determine_openai_auth_method().await?;

    info!("Authentication methods cached at startup");

    Ok(AuthCache {
        anthropic_method,
        gemini_method,
        openai_method,
        cached_at,
    })
}

/// Determine the best Anthropic authentication method
fn determine_anthropic_auth_method() -> Result<AuthMethod> {
    use crate::auth::anthropic::AnthropicOAuth;
    use crate::auth::common::analyze_token_source;
    use tracing::info;

    // Try to load Claude CLI tokens
    let claude_cli_result = AnthropicOAuth::try_claude_code_credentials();

    if let Ok(claude_config) = claude_cli_result {
        let claude_info = analyze_token_source("Claude CLI", &claude_config);

        if !claude_info.is_expired {
            info!(
                "Found valid Claude CLI OAuth tokens ({})",
                claude_info.age_description
            );

            if let Some(token) = claude_config.oauth_access_token {
                return Ok(AuthMethod::OAuth {
                    source: "Claude CLI".to_string(),
                    token,
                });
            }
        } else {
            use tracing::error;
            error!("Found expired Claude CLI OAuth tokens - startup will fail");
            // Return clean error for expired tokens - no fallback to API key
            return Err(PrismError::Other(
                "Anthropic OAuth tokens are expired!\n\n\
                 Token Status:\n\
                   • Claude CLI: expired\n\
                   • Setu config: not configured\n\n\
                 To fix this issue:\n\
                   1. Run: claude auth refresh    (refresh Claude CLI tokens)\n\
                   2. Run: setu auth anthropic   (get fresh setu tokens)\n\n\
                 Setu will automatically use whichever tokens are newer."
                    .to_string(),
            ));
        }
    } else {
        info!("No Claude CLI OAuth tokens found");
    }

    // No OAuth tokens found anywhere - this is an error for OAuth providers
    Err(PrismError::Other(
        "No Anthropic OAuth tokens found!\n\n\
         Token Status:\n\
           • Claude CLI: not found\n\
           • Setu config: not configured\n\n\
         To fix this issue:\n\
           1. Run: claude auth refresh    (if Claude CLI is installed)\n\
           2. Run: setu auth anthropic   (get fresh setu tokens)\n\n\
         Setu will automatically use whichever tokens are newer."
            .to_string(),
    ))
}

/// Determine the best Gemini authentication method  
async fn determine_gemini_auth_method() -> Result<AuthMethod> {
    use crate::auth::google::GoogleOAuth;
    use tracing::info;

    // Try to load Gemini CLI tokens
    let gemini_cli_result = GoogleOAuth::try_gemini_cli_credentials().await;

    if let Ok(gemini_config) = gemini_cli_result {
        // Tokens are valid and not expired
        info!("Found valid Gemini CLI OAuth tokens");

        if let Some(token) = gemini_config.oauth_access_token {
            return Ok(AuthMethod::OAuth {
                source: "Gemini CLI".to_string(),
                token,
            });
        }
    } else {
        // Check if it's a specific expiration error
        if let Err(ref error) = gemini_cli_result {
            let error_str = error.to_string();
            if error_str.contains("expired") {
                use tracing::error;
                error!("Found expired Gemini CLI OAuth tokens - startup will fail");
                // Return clean error for expired tokens - no fallback to API key
                return Err(PrismError::Other(
                    "Gemini OAuth tokens are expired!\n\n\
                     Token Status:\n\
                       • Gemini CLI: expired\n\
                       • Setu config: not configured\n\n\
                     To fix this issue:\n\
                       1. Try: gemini -p \"test\"      (may trigger automatic refresh)\n\
                       2. Run: setu auth google      (copy CLI tokens to prism config)\n\n\
                     If Gemini CLI refresh fails, you may need to re-authenticate with Google."
                        .to_string(),
                ));
            } else {
                info!("No Gemini CLI OAuth tokens found");
            }
        }
    }

    // No OAuth tokens found anywhere - this is an error for OAuth providers
    Err(PrismError::Other(
        "No Gemini OAuth tokens found!\n\n\
         Token Status:\n\
           • Gemini CLI: not found\n\
           • Setu config: not configured\n\n\
         To fix this issue:\n\
           1. Try: gemini -p \"test\"      (may trigger authentication)\n\
           2. Run: setu auth google      (set up OAuth tokens)\n\n\
         If Gemini CLI is not installed, you may need to set up Google OAuth."
            .to_string(),
    ))
}

/// Determine the best OpenAI authentication method  
async fn determine_openai_auth_method() -> Result<AuthMethod> {
    use crate::auth::openai::OpenAIOAuth;
    use tracing::info;

    // Try to load codex CLI tokens
    let codex_cli_result = OpenAIOAuth::try_codex_cli_credentials().await;

    if let Ok(openai_config) = codex_cli_result {
        // Tokens are valid and not expired
        info!("Found valid codex CLI OAuth tokens");

        if let Some(token) = openai_config.oauth_access_token {
            return Ok(AuthMethod::OAuth {
                source: "codex CLI".to_string(),
                token,
            });
        }
    } else {
        // Check if it's a specific expiration error
        if let Err(ref error) = codex_cli_result {
            let error_str = error.to_string();
            if error_str.contains("expired") {
                use tracing::warn;
                warn!("Found expired codex CLI OAuth tokens - will continue without OpenAI");
                // For OpenAI, we don't fail startup - it's optional
                return Ok(AuthMethod::Unavailable {
                    reason: "OAuth tokens expired".to_string(),
                });
            } else {
                info!("No codex CLI OAuth tokens found - OpenAI unavailable");
            }
        }
    }

    // No OAuth tokens found - OpenAI is simply unavailable (not an error)
    Ok(AuthMethod::Unavailable {
        reason: "No OAuth tokens found".to_string(),
    })
}
