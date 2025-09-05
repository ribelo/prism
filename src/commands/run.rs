use clap::Subcommand;
use tracing::{error, info};

use crate::process::ProcessManager;
use crate::{Result, SetuError};

#[derive(Subcommand, Debug)]
pub enum RunCommands {
    /// Run Claude Code with Setu as the backend
    Claude {
        /// Arguments to pass through to claude
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run Codex (OpenAI CLI) with Setu as the backend
    Codex {
        /// Arguments to pass through to codex
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Handle the run command by managing server and client processes
pub async fn handle_run_command(run_command: RunCommands) -> Result<()> {
    match run_command {
        RunCommands::Claude { args } => handle_claude_run(args).await,
        RunCommands::Codex { args } => handle_codex_run(args).await,
    }
}

async fn handle_claude_run(args: Vec<String>) -> Result<()> {
    info!("Starting Claude with Setu backend");

    // Check if claude is available
    if !is_command_available("claude") {
        return Err(SetuError::Other(
            "Claude CLI not found. Please install Claude Code first.\n\
             Visit https://claude.ai/code to get started."
                .to_string(),
        ));
    }

    let mut process_manager = ProcessManager::new();

    // Ensure server is running and get its URL
    let server_url = match process_manager.ensure_server_running().await {
        Ok(url) => {
            info!("Server ready at: {}", url);
            url
        }
        Err(e) => {
            error!("Failed to start server: {}", e);
            return Err(e);
        }
    };

    // Start Claude with the server URL
    match process_manager.start_client("claude", &args, &server_url) {
        Ok(()) => {
            info!("Claude started successfully");
        }
        Err(e) => {
            error!("Failed to start Claude: {}", e);
            return Err(e);
        }
    }

    // Wait for Claude to complete
    process_manager.wait_for_client()?;

    info!("Claude session completed");
    Ok(())
}

async fn handle_codex_run(args: Vec<String>) -> Result<()> {
    info!("Starting Codex with Setu backend");

    // Check if codex is available
    if !is_command_available("codex") {
        return Err(SetuError::Other(
            "Codex CLI not found. Please install Codex first.\n\
             Visit https://openai.com/codex or install via npm: npm i -g @openai/codex@native"
                .to_string(),
        ));
    }

    let mut process_manager = ProcessManager::new();

    // Ensure server is running and get its URL
    let server_url = match process_manager.ensure_server_running().await {
        Ok(url) => {
            info!("Server ready at: {}", url);
            url
        }
        Err(e) => {
            error!("Failed to start server: {}", e);
            return Err(e);
        }
    };

    // Start Codex with the server URL using OPENAI_BASE_URL
    match process_manager.start_client_with_env("codex", &args, &[("OPENAI_BASE_URL", &server_url)])
    {
        Ok(()) => {
            info!("Codex started successfully");
        }
        Err(e) => {
            error!("Failed to start Codex: {}", e);
            return Err(e);
        }
    }

    // Wait for Codex to complete
    process_manager.wait_for_client()?;

    info!("Codex session completed");
    Ok(())
}

/// Check if a command is available in the system PATH (cross-platform)
fn is_command_available(command: &str) -> bool {
    let which_command = if cfg!(windows) { "where" } else { "which" };

    std::process::Command::new(which_command)
        .arg(command)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_command_available() {
        // Test with a command that should always be available
        assert!(is_command_available("ls"));

        // Test with a command that probably doesn't exist
        assert!(!is_command_available("definitely_not_a_real_command_12345"));
    }
}
