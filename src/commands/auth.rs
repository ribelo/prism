use clap::Subcommand;
use crate::{Config, Result, SetuError};
use tracing::info;

#[derive(Subcommand, Debug)]
pub enum AuthCommands {
    /// Authenticate with Anthropic using OAuth
    Anthropic,

    /// Authenticate with Google (not yet implemented)
    Google,
}

pub async fn handle_auth_command(auth_command: AuthCommands) -> Result<()> {
    match auth_command {
        AuthCommands::Anthropic => {
            handle_anthropic_auth().await
        }
        AuthCommands::Google => {
            println!("Google authentication not yet implemented");
            Ok(())
        }
    }
}

async fn handle_anthropic_auth() -> Result<()> {
    use crate::auth::anthropic::AnthropicOAuth;
    use std::io::{self, Write};

    info!("Starting Anthropic OAuth authentication...");

    // Load existing config or create new one
    let mut config = Config::load().unwrap_or_default();

    // Generate authorization URL and get verifier
    let auth_result = AnthropicOAuth::create_authorization_url()?;

    println!("🔐 Anthropic OAuth Authentication");
    println!("================================");
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
    io::stdin().read_line(&mut auth_code)
        .map_err(|e| SetuError::Other(format!("Failed to read input: {}", e)))?;
    let auth_code = auth_code.trim().to_string();

    if auth_code.is_empty() {
        println!("❌ No authorization code provided");
        return Ok(());
    }

    // Exchange code for tokens
    println!("🔄 Exchanging authorization code for tokens...");

    match AnthropicOAuth::exchange_code_for_token(&auth_code, &auth_result.verifier).await {
        Ok(received_auth_config) => {
            // Save to config
            let mut provider_config = config.providers.get("anthropic").cloned()
                .unwrap_or_else(|| crate::config::ProviderConfig {
                    r#type: "anthropic".to_string(),
                    endpoint: "https://api.anthropic.com".to_string(),
                    models: vec![
                        "claude-3-5-sonnet-20241022".to_string(),
                        "claude-3-haiku-20240307".to_string(),
                        "claude-3-opus-20240229".to_string(),
                    ],
                    auth: received_auth_config.clone(),
                });
            provider_config.auth = received_auth_config;
            config.providers.insert("anthropic".to_string(), provider_config);

            // Save config
            config.save()?;

            println!("✅ Anthropic authentication successful!");
            println!("   Tokens have been saved to your configuration.");
            println!("   You can now use Anthropic models through Setu.");
        }
        Err(e) => {
            println!("❌ Authentication failed: {}", e);
        }
    }

    Ok(())
}
