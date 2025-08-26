use clap::{Parser, Subcommand};
use setu::{Config, Result, SetuError, commands::auth::{handle_auth_command, AuthCommands}};
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
    
    let server = setu::server::SetuServer::new(config);
    server.start().await
}

async fn validate_config() -> Result<()> {
    info!("Validating configuration...");
    
    match Config::load() {
        Ok(config) => {
            println!("✓ Configuration is valid");
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
