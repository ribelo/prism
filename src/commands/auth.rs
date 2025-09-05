use clap::Subcommand;
use std::io::{self, Write};
use tracing::info;

use crate::{Config, Result, SetuError};

#[derive(Subcommand, Debug)]
pub enum AuthCommands {
    /// Authenticate with Anthropic using OAuth
    Anthropic,

    /// Authenticate with Google/Gemini using CLI credentials
    Google,

    /// Check OpenAI/codex CLI OAuth credentials
    Openai,
}

pub async fn handle_auth_command(auth_command: AuthCommands) -> Result<()> {
    match auth_command {
        AuthCommands::Anthropic => handle_anthropic_auth().await,
        AuthCommands::Google => handle_google_auth().await,
        AuthCommands::Openai => handle_openai_auth().await,
    }
}

async fn handle_anthropic_auth() -> Result<()> {
    use crate::auth::anthropic::AnthropicOAuth;

    info!("Starting Anthropic OAuth authentication...");

    // Load existing config or create new one
    let mut config = Config::load().unwrap_or_default();

    // Generate authorization URL and get verifier
    let auth_result = AnthropicOAuth::create_authorization_url()?;

    println!("Anthropic OAuth Authentication");
    println!("==================================");
    println!();
    println!("Please visit this URL to authorize Setu:");
    println!("{}", auth_result.url);
    println!();
    println!("After authorization, you'll be redirected to a localhost URL.");
    println!("Copy the 'code' parameter from the URL and paste it here:");
    print!("> ");
    io::stdout().flush().unwrap();

    // Read authorization code from user
    let mut auth_code = String::new();
    io::stdin()
        .read_line(&mut auth_code)
        .map_err(|e| SetuError::Other(format!("Failed to read input: {}", e)))?;
    let auth_code = auth_code.trim().to_string();

    if auth_code.is_empty() {
        println!("No authorization code provided");
        return Ok(());
    }

    // Exchange code for tokens
    println!("Exchanging authorization code for tokens...");

    match AnthropicOAuth::exchange_code_for_token(&auth_code, &auth_result.verifier).await {
        Ok(received_auth_config) => {
            // Save to config
            let mut provider_config =
                config
                    .providers
                    .get("anthropic")
                    .cloned()
                    .unwrap_or_else(|| crate::config::ProviderConfig {
                        r#type: "anthropic".to_string(),
                        endpoint: "https://api.anthropic.com".to_string(),
                        auth: received_auth_config.clone(),
                        retry: crate::config::RetryConfig::default(),
                        api_key: None,
                        api_key_fallback: false,
                        fallback_on_errors: vec![429],
                    });
            provider_config.auth = received_auth_config;
            config
                .providers
                .insert("anthropic".to_string(), provider_config);

            // Save config
            config.save()?;

            println!("Anthropic authentication successful!");
            println!("   Tokens have been saved to your configuration.");
            println!("   You can now use Anthropic models through Setu.");
        }
        Err(e) => {
            println!("Authentication failed: {}", e);
        }
    }

    Ok(())
}

async fn handle_google_auth() -> Result<()> {
    use crate::auth::google::GoogleOAuth;

    println!("Setting up Google/Gemini authentication...");

    // Load current config
    let mut config = Config::load().unwrap_or_default();

    // Try to read existing Gemini CLI credentials
    match GoogleOAuth::try_gemini_cli_credentials().await {
        Ok(auth_config) => {
            // Save to config
            let mut provider_config =
                config.providers.get("gemini").cloned().unwrap_or_else(|| {
                    crate::config::ProviderConfig {
                        r#type: "gemini".to_string(),
                        endpoint: "https://generativelanguage.googleapis.com".to_string(),
                        auth: auth_config.clone(),
                        retry: crate::config::RetryConfig::default(),
                        api_key: None,
                        api_key_fallback: false,
                        fallback_on_errors: vec![429],
                    }
                });
            provider_config.auth = auth_config;
            config
                .providers
                .insert("gemini".to_string(), provider_config);

            // Save config
            config.save()?;

            println!("Google/Gemini authentication successful!");
            println!("   OAuth tokens loaded from Gemini CLI credentials.");
            println!("   You can now use Gemini models through Setu.");
        }
        Err(e) => {
            println!("Could not load Gemini CLI OAuth credentials: {}", e);
            println!();
            println!("To use Gemini with OAuth:");
            println!("1. Install the Gemini CLI");
            println!("2. Run: gemini auth login");
            println!("3. Then retry: setu auth google");
            println!();
            println!(
                "Alternatively, set GEMINI_API_KEY environment variable to use API key authentication."
            );
        }
    }

    Ok(())
}

async fn handle_openai_auth() -> Result<()> {
    use crate::auth::openai::OpenAIOAuth;

    println!("Checking OpenAI/codex CLI authentication...");

    // Load current config
    let mut config = Config::load().unwrap_or_default();

    // Try to read existing codex CLI credentials
    match OpenAIOAuth::try_codex_cli_credentials().await {
        Ok(auth_config) => {
            // Save to config
            let mut provider_config =
                config.providers.get("openai").cloned().unwrap_or_else(|| {
                    crate::config::ProviderConfig {
                        r#type: "openai".to_string(),
                        endpoint: "https://api.openai.com".to_string(),
                        auth: auth_config.clone(),
                        retry: crate::config::RetryConfig::default(),
                        api_key: None,
                        api_key_fallback: false,
                        fallback_on_errors: vec![429],
                    }
                });
            provider_config.auth = auth_config;
            config
                .providers
                .insert("openai".to_string(), provider_config);

            // Save config
            config.save()?;

            println!("OpenAI authentication successful!");
            println!("   OAuth tokens loaded from codex CLI credentials.");
            println!("   You can now use OpenAI models through Setu.");

            // Show token refresh info
            println!();
            println!("Token Info:");
            println!("   • Tokens are automatically refreshed every 28 days");
            println!("   • Access token expires and refreshes as needed");
            println!("   • Shared with codex CLI (~/.codex/auth.json)");
        }
        Err(e) => {
            println!("Could not load codex CLI OAuth credentials: {}", e);
            println!();
            println!("To use OpenAI with OAuth:");
            println!("1. Install the codex CLI from OpenAI");
            println!("2. Run: codex auth login");
            println!("3. Then retry: setu auth openai");
            println!();
            println!(
                "Alternatively, set OPENAI_API_KEY environment variable to use API key authentication."
            );
            println!();
            println!(
                "Note: codex CLI provides OAuth tokens for ChatGPT Pro/Plus/Enterprise users."
            );
        }
    }

    Ok(())
}
