use clap::{Parser, Subcommand};
use setu::commands::auth::AuthCommands;
use setu::{Config, Result};
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "setu")]
#[command(about = "Universal AI Model Router")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP proxy server
    Start {
        /// Override server host
        #[arg(long)]
        host: Option<String>,

        /// Override server port
        #[arg(long)]
        port: Option<u16>,
    },

    /// Stop the running server (placeholder)
    Stop,

    /// Check server status (placeholder)
    Status,

    /// Validate configuration
    Config,

    /// Manage authentication for AI providers
    Auth {
        #[command(subcommand)]
        auth_command: AuthCommands,
    },

    /// Diagnose OAuth token issues
    Diagnose,
}

async fn handle_auth_command(auth_command: AuthCommands) -> Result<()> {
    setu::commands::auth::handle_auth_command(auth_command).await
}


#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    init_tracing(cli.verbose)?;

    match cli.command {
        Commands::Start { host, port } => start_server(host, port).await,
        Commands::Stop => {
            println!("Stop command not yet implemented");
            Ok(())
        }
        Commands::Status => {
            println!("Status command not yet implemented");
            Ok(())
        }
        Commands::Config => validate_config().await,
        Commands::Auth { auth_command } => handle_auth_command(auth_command).await,
        Commands::Diagnose => diagnose_tokens().await,
    }
}

async fn start_server(host: Option<String>, port: Option<u16>) -> Result<()> {
    info!("Starting Setu server...");

    let mut config = Config::load()?;

    // Override config with CLI args if provided
    if let Some(host) = host {
        config.server.host = host;
    }
    if let Some(port) = port {
        config.server.port = port;
    }

    // Validate OAuth tokens before starting server
    if let Err(e) = validate_oauth_tokens(&mut config).await {
        error!("OAuth token validation failed: {}", e);
        println!();
        println!("To fix this issue:");
        println!("   1. Run: setu auth anthropic");
        println!("   2. Follow the OAuth flow to get fresh tokens");
        println!("   3. Try starting the server again");
        std::process::exit(1);
    }

    // Save config in case tokens were refreshed during validation
    config.save()?;

    let server = setu::server::SetuServer::new(config);
    server.start().await
}

async fn validate_oauth_tokens(config: &mut Config) -> Result<()> {
    use setu::auth::anthropic::AnthropicOAuth;
    use setu::auth::google::GoogleOAuth;

    info!("Validating OAuth tokens...");

    // Check if we have anthropic provider configured
    if let Some(provider) = config.providers.get_mut("anthropic") {
        info!("Checking Anthropic OAuth tokens...");

        // Always try to validate auth config - this will try Claude Code credentials if setu has none
        if let Err(e) = AnthropicOAuth::validate_auth_config(&mut provider.auth).await {
            info!(
                "Anthropic OAuth validation failed: {} (will only work with direct API keys)",
                e
            );
        }
    } else {
        info!("No Anthropic provider configured");
    }

    // Check if we have gemini provider configured
    if let Some(provider) = config.providers.get_mut("gemini") {
        info!("Checking Gemini OAuth tokens...");

        // Always try to validate auth config - this will try Gemini CLI credentials if setu has none
        if let Err(e) = GoogleOAuth::validate_auth_config(&mut provider.auth).await {
            info!(
                "Gemini OAuth validation failed: {} (will only work with direct API keys)",
                e
            );
        }
    } else {
        info!("No Gemini provider configured");
    }

    info!("Token validation complete");
    Ok(())
}

async fn validate_config() -> Result<()> {
    info!("Validating configuration...");

    match Config::load() {
        Ok(config) => {
            println!("Configuration is valid");
            println!("  Server: {}:{}", config.server.host, config.server.port);
            println!("  Providers: {}", config.providers.len());
            println!("  Default provider: {}", config.routing.default_provider);

            if let Ok(config_dir) = Config::config_dir() {
                println!("  Config directory: {}", config_dir.display());
            }

            Ok(())
        }
        Err(e) => {
            error!("Configuration validation failed: {}", e);
            Err(e)
        }
    }
}

async fn diagnose_tokens() -> Result<()> {
    use setu::auth::anthropic::AnthropicOAuth;
    use std::time::{SystemTime, UNIX_EPOCH};

    println!("Diagnosing OAuth Token Issues");
    println!("=============================");
    println!();

    // Load config
    let config = match Config::load() {
        Ok(config) => {
            println!("Configuration loaded successfully");
            config
        }
        Err(e) => {
            println!("Failed to load configuration: {}", e);
            return Err(e);
        }
    };

    // Check if anthropic provider exists
    let anthropic_provider = match config.providers.get("anthropic") {
        Some(provider) => {
            println!("Anthropic provider found in configuration");
            provider
        }
        None => {
            println!("No Anthropic provider found in configuration");
            println!("   Run 'setu auth anthropic' to set up OAuth");
            return Ok(());
        }
    };

    let auth_config = &anthropic_provider.auth;

    // Check refresh token
    match &auth_config.oauth_refresh_token {
        Some(refresh_token) => {
            println!("OAuth refresh token present");
            println!(
                "   Token: {}...",
                &refresh_token[..std::cmp::min(20, refresh_token.len())]
            );
        }
        None => {
            println!("No OAuth refresh token found");
            println!("   Run 'setu auth anthropic' to get tokens");
            return Ok(());
        }
    }

    // Check access token
    match &auth_config.oauth_access_token {
        Some(access_token) => {
            println!("OAuth access token present");
            println!(
                "   Token: {}...",
                &access_token[..std::cmp::min(20, access_token.len())]
            );
        }
        None => {
            println!("No OAuth access token found (will try to refresh)");
        }
    }

    // Check token expiration
    match auth_config.oauth_expires {
        Some(expires) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            if expires > now {
                let seconds_left = (expires - now) / 1000;
                println!(
                    "Token expires in {} seconds ({} minutes)",
                    seconds_left,
                    seconds_left / 60
                );
            } else {
                let seconds_expired = (now - expires) / 1000;
                println!(
                    "Token expired {} seconds ago ({} minutes)",
                    seconds_expired,
                    seconds_expired / 60
                );
            }
        }
        None => {
            println!("No expiration time set for token");
        }
    }

    println!();
    println!("Testing Token Refresh");
    println!("====================");

    // Try to refresh the token
    let mut test_auth_config = auth_config.clone();
    match AnthropicOAuth::refresh_token(&mut test_auth_config).await {
        Ok(()) => {
            println!("Token refresh successful!");
            println!(
                "   New access token: {}...",
                test_auth_config
                    .oauth_access_token
                    .as_ref()
                    .map(|t| &t[..std::cmp::min(20, t.len())])
                    .unwrap_or("missing")
            );
        }
        Err(e) => {
            println!("Token refresh failed: {}", e);
            println!();
            println!("Recommended Actions:");
            println!("1. Your refresh token has likely expired");
            println!("2. Run: setu auth anthropic");
            println!("3. Complete the OAuth flow to get fresh tokens");
            println!("4. Try starting the server again");
        }
    }

    println!();
    println!("Summary");
    println!("=======");
    println!(
        "Config file: {:?}",
        Config::config_dir().unwrap_or_default().join("setu.toml")
    );
    println!("Providers configured: {}", config.providers.len());
    println!("Default provider: {}", config.routing.default_provider);

    Ok(())
}

fn init_tracing(verbose: bool) -> Result<()> {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let filter = if verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    // Load config to get logging preferences
    let config = Config::load().unwrap_or_default();

    let registry = tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(filter);

    // Add file logging if enabled
    if config.server.log_file_enabled {
        let log_dir = config.log_dir()?;

        let rotation = match config.server.log_rotation.as_str() {
            "minutely" => Rotation::MINUTELY,
            "hourly" => Rotation::HOURLY,
            "daily" => Rotation::DAILY,
            "never" => Rotation::NEVER,
            _ => {
                eprintln!(
                    "Warning: Invalid log rotation '{}', using daily",
                    config.server.log_rotation
                );
                Rotation::DAILY
            }
        };

        let file_appender =
            RollingFileAppender::new(rotation, &log_dir, &config.server.log_file_prefix);

        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        registry
            .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
            .init();

        // Keep guard alive by leaking it (simple approach for now)
        std::mem::forget(_guard);
    } else {
        registry.init();
    }

    Ok(())
}
