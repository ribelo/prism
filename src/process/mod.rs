use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, error, info};

use crate::{Config, Result, SetuError};

// Configuration constants - extracted from magic numbers
const SERVER_PROBE_TIMEOUT_SECS: u64 = 2;
const SERVER_READY_MAX_ATTEMPTS: u32 = 20;
const SERVER_READY_RETRY_INTERVAL_MS: u64 = 500;

/// Check if Setu server is running by attempting to connect and verify health
pub async fn is_server_running() -> Result<Option<String>> {
    let config = Config::load()?;
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let server_url = format!("http://{}", addr);

    debug!("Checking if server is running on {}", addr);

    // First, try to connect to the port
    match timeout(
        Duration::from_secs(SERVER_PROBE_TIMEOUT_SECS),
        TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(_)) => {
            debug!("Port {} is open, checking if it's a Setu server", addr);

            // Verify it's actually a Setu server by checking the health endpoint
            match check_server_health(&server_url).await {
                Ok(true) => {
                    debug!("Confirmed Setu server is running on {}", addr);
                    Ok(Some(server_url))
                }
                Ok(false) => {
                    debug!("Port {} is occupied by a non-Setu service", addr);
                    Err(SetuError::Other(format!(
                        "Port {} is occupied by another service. Please stop it or use a different port.",
                        config.server.port
                    )))
                }
                Err(_) => {
                    debug!("Health check failed, assuming server is starting up");
                    // If health check fails, it might be starting up, so we return None
                    // to trigger server startup which will fail if port is truly occupied
                    Ok(None)
                }
            }
        }
        Ok(Err(_)) | Err(_) => {
            debug!("Server not running on {}", addr);
            Ok(None)
        }
    }
}

/// Check if the server responds correctly to health endpoint
async fn check_server_health(server_url: &str) -> Result<bool> {
    let client = reqwest::Client::new();
    let health_url = format!("{}/health", server_url);

    match timeout(
        Duration::from_secs(SERVER_PROBE_TIMEOUT_SECS),
        client.get(&health_url).send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            if response.status().is_success() {
                // Try to parse the response to verify it's a Setu server
                match response.json::<serde_json::Value>().await {
                    Ok(json) => {
                        let is_setu = json
                            .get("service")
                            .and_then(|s| s.as_str())
                            .map(|s| s == "setu")
                            .unwrap_or(false);
                        Ok(is_setu)
                    }
                    Err(_) => Ok(false),
                }
            } else {
                Ok(false)
            }
        }
        Ok(Err(_)) | Err(_) => Ok(false),
    }
}

/// Spawn the Setu server in the background
pub fn spawn_server_background() -> Result<Child> {
    info!("Starting Setu server in background");

    // Get the current executable path so we can spawn another instance of ourselves
    let current_exe = std::env::current_exe()
        .map_err(|e| SetuError::Other(format!("Failed to get current executable path: {}", e)))?;

    let child = Command::new(current_exe)
        .args(["start"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| SetuError::Other(format!("Failed to start server: {}", e)))?;

    Ok(child)
}

/// Wait for server to become ready by polling the connection
pub async fn wait_for_server(max_attempts: u32) -> Result<String> {
    debug!(
        "Waiting for server to become ready (max {} attempts)",
        max_attempts
    );

    for attempt in 1..=max_attempts {
        if let Some(server_url) = is_server_running().await? {
            info!("Server ready at {} (attempt {})", server_url, attempt);
            return Ok(server_url);
        }

        debug!(
            "Server not ready yet (attempt {}), waiting {}ms",
            attempt, SERVER_READY_RETRY_INTERVAL_MS
        );
        tokio::time::sleep(Duration::from_millis(SERVER_READY_RETRY_INTERVAL_MS)).await;
    }

    Err(SetuError::Other(
        "Server failed to start within timeout".to_string(),
    ))
}

/// Spawn a client process with environment variables set
pub fn spawn_client_process(command: &str, args: &[String], server_url: &str) -> Result<Child> {
    info!("Starting {} with server URL: {}", command, server_url);

    let child = Command::new(command)
        .args(args)
        .env("ANTHROPIC_BASE_URL", server_url)
        .spawn()
        .map_err(|e| SetuError::Other(format!("Failed to start {}: {}", command, e)))?;

    Ok(child)
}

/// Spawn a client process with custom environment variables
pub fn spawn_client_process_with_env(
    command: &str,
    args: &[String],
    env_vars: &[(&str, &str)],
) -> Result<Child> {
    info!("Starting {} with custom environment", command);

    let mut cmd = Command::new(command);
    cmd.args(args);

    // Set all provided environment variables
    for (key, value) in env_vars {
        info!("Setting {}={}", key, value);
        cmd.env(key, value);
    }

    let child = cmd
        .spawn()
        .map_err(|e| SetuError::Other(format!("Failed to start {}: {}", command, e)))?;

    Ok(child)
}

/// Process management for coordinating server and client lifecycle
pub struct ProcessManager {
    server_child: Option<Child>,
    client_child: Option<Child>,
    server_was_already_running: bool,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            server_child: None,
            client_child: None,
            server_was_already_running: false,
        }
    }

    /// Start server if needed and return the server URL
    pub async fn ensure_server_running(&mut self) -> Result<String> {
        // Check if server is already running
        if let Some(server_url) = is_server_running().await? {
            self.server_was_already_running = true;
            return Ok(server_url);
        }

        // Start server in background
        let server_child = spawn_server_background()?;
        self.server_child = Some(server_child);
        self.server_was_already_running = false;

        // Wait for server to be ready
        wait_for_server(SERVER_READY_MAX_ATTEMPTS).await
    }

    /// Start client process
    pub fn start_client(&mut self, command: &str, args: &[String], server_url: &str) -> Result<()> {
        let client_child = spawn_client_process(command, args, server_url)?;
        self.client_child = Some(client_child);
        Ok(())
    }

    /// Start client process with custom environment variables
    pub fn start_client_with_env(
        &mut self,
        command: &str,
        args: &[String],
        env_vars: &[(&str, &str)],
    ) -> Result<()> {
        let client_child = spawn_client_process_with_env(command, args, env_vars)?;
        self.client_child = Some(client_child);
        Ok(())
    }

    /// Wait for client to complete and handle cleanup
    pub fn wait_for_client(&mut self) -> Result<()> {
        if let Some(mut client) = self.client_child.take() {
            match client.wait() {
                Ok(status) => {
                    debug!("Client process exited with status: {}", status);
                }
                Err(e) => {
                    error!("Error waiting for client process: {}", e);
                }
            }
        }

        // Optionally stop the server we started (for now, keep it running)
        self.cleanup_if_needed();

        Ok(())
    }

    fn cleanup_if_needed(&mut self) {
        // For now, we keep the server running even after client exits
        // This provides better user experience for subsequent runs

        if let Some(server) = self.server_child.take()
            && !self.server_was_already_running
        {
            info!("Keeping server running for future use");
            // We could optionally kill it here, but leaving it running is better UX
            std::mem::forget(server); // Don't wait for it or kill it
        }
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        // Ensure cleanup happens even if wait_for_client wasn't called
        self.cleanup_if_needed();
    }
}
