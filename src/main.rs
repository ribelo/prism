use clap::{Parser, Subcommand};
use setu::{Config, Result, SetuError};
use tracing::{info, error};

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

#[derive(Subcommand)]
enum AuthCommands {
    /// Authenticate with Anthropic using OAuth
    Anthropic,
    
    /// Authenticate with Google (not yet implemented)
    Google,
}

async fn handle_auth_command(auth_command: AuthCommands) -> Result<()> {
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
    use setu::auth::anthropic::AnthropicOAuth;
    use std::io::{self, Write};
    
    info!("Starting Anthropic OAuth authentication...");
    
    // Load existing config or create new one
    let mut config = Config::load().unwrap_or_default();
    
    // Generate authorization URL and get verifier
    let auth_result = AnthropicOAuth::create_authorization_url()?;
    
    println!("Anthropic OAuth Authentication");
    println!("==============================");
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
        println!("No authorization code provided");
        return Ok(());
    }
    
    // Exchange code for tokens
    println!("Exchanging authorization code for tokens...");
    
    match AnthropicOAuth::exchange_code_for_token(&auth_code, &auth_result.verifier).await {
        Ok(received_auth_config) => {
            // Save to config
            let mut provider_config = config.providers.get("anthropic").cloned()
                .unwrap_or_else(|| setu::config::ProviderConfig {
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize tracing
    init_tracing(cli.verbose)?;
    
    match cli.command {
        Commands::Start { host, port } => {
            start_server(host, port).await
        }
        Commands::Stop => {
            println!("Stop command not yet implemented");
            Ok(())
        }
        Commands::Status => {
            println!("Status command not yet implemented");
            Ok(())
        }
        Commands::Config => {
            validate_config().await
        }
        Commands::Auth { auth_command } => {
            handle_auth_command(auth_command).await
        }
        Commands::Diagnose => {
            diagnose_tokens().await
        }
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
    
    info!("Validating OAuth tokens...");
    
    // Check if we have anthropic provider configured
    if let Some(provider) = config.providers.get_mut("anthropic") {
        if provider.auth.oauth_refresh_token.is_some() {
            info!("Checking Anthropic OAuth tokens...");
            
            // Validate and potentially refresh tokens
            if let Err(e) = AnthropicOAuth::validate_auth_config(&mut provider.auth).await {
                return Err(SetuError::Other(format!("Anthropic OAuth validation failed: {}", e)));
            }
        } else {
            info!("No Anthropic OAuth tokens found (will only work with direct API keys)");
        }
    } else {
        info!("No Anthropic provider configured");
    }
    
    // Could add validation for other providers here in the future
    // if let Some(provider) = config.providers.get_mut("google") { ... }
    
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
            println!("   Token: {}...", &refresh_token[..std::cmp::min(20, refresh_token.len())]);
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
            println!("   Token: {}...", &access_token[..std::cmp::min(20, access_token.len())]);
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
                println!("Token expires in {} seconds ({} minutes)", seconds_left, seconds_left / 60);
            } else {
                let seconds_expired = (now - expires) / 1000;
                println!("Token expired {} seconds ago ({} minutes)", seconds_expired, seconds_expired / 60);
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
            println!("   New access token: {}...", 
                test_auth_config.oauth_access_token.as_ref()
                    .map(|t| &t[..std::cmp::min(20, t.len())])
                    .unwrap_or("missing"));
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
    println!("Config file: {:?}", Config::config_dir().unwrap_or_default().join("setu.toml"));
    println!("Providers configured: {}", config.providers.len());
    println!("Default provider: {}", config.routing.default_provider);
    
    Ok(())
}

fn init_tracing(verbose: bool) -> Result<()> {
    use tracing_subscriber::{
        fmt,
        layer::SubscriberExt,
        util::SubscriberInitExt,
        EnvFilter,
    };
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    
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
                eprintln!("Warning: Invalid log rotation '{}', using daily", config.server.log_rotation);
                Rotation::DAILY
            }
        };
        
        let file_appender = RollingFileAppender::new(
            rotation,
            &log_dir,
            &config.server.log_file_prefix
        );
        
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
