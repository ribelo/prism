# Setu: The Universal AI Model Router

**Setu** (from the Sanskrit word for "bridge") is a provider- and client-agnostic intelligent routing middleware for AI model interactions. It acts as a local proxy server that can intelligently route requests to different AI providers, handle authentication, and provide a unified API for all your AI needs.

![Setu Architecture Diagram](docs/ARCHITECTURE.md)
*For a detailed look at the system design, see the [Architecture Document](docs/ARCHITECTURE.md).*

---

## Key Features

### 🚀 **Unified API Interface**
Interact with multiple AI providers (Anthropic Claude, OpenRouter, Google Gemini, etc.) through a single, OpenAI-compatible API. Switch between providers without changing your application code.

### 🧠 **Intelligent Routing**
Configure sophisticated routing rules based on:
- Model names and capabilities
- Provider priority and availability  
- Cost optimization strategies
- Performance characteristics
- Custom business logic

### 🔄 **Automatic Fallback & Resilience**
Built-in failover mechanisms ensure your applications stay running:
- Automatic retry with exponential backoff
- Provider health monitoring
- Circuit breaker patterns
- Graceful degradation strategies

### 🔐 **Enterprise-Grade Authentication**
Comprehensive authentication support:
- OAuth 2.0 for Anthropic and Google
- API key management with secure storage
- Automatic token refresh and rotation
- Multi-provider credential management

### ⚙️ **Centralized Configuration Management**
Single configuration file (`setu.toml`) manages:
- Provider endpoints and credentials
- Routing rules and priorities
- Rate limiting and quotas
- Logging and monitoring settings

### 🔧 **Developer-Friendly Design**
Built for production use:
- Hot configuration reloading
- Comprehensive logging and metrics
- Health check endpoints
- OpenAPI/Swagger documentation

### 🏠 **Privacy-First Architecture**
Local-first approach ensures:
- All requests stay on your infrastructure
- No external dependencies for core functionality
- Full control over data flow and logging
- Compliance with data residency requirements

---

## Installation and Setup

### Prerequisites

-   **Rust**: Setu is written in Rust. You'll need the Rust toolchain (2024 edition or later). You can install it from [rust-lang.org](https://www.rust-lang.org/tools/install).
-   **Cargo**: The Rust package manager, which is included with the Rust installation.
-   **Git**: To clone the repository.

### Installation Steps

#### Option 1: Build from Source (Recommended)

1.  **Clone the Repository**

    ```bash
    git clone https://github.com/ribelo/setu.git
    cd setu
    ```

2.  **Verify Prerequisites**

    Ensure you have the required Rust toolchain:
    ```bash
    rustc --version  # Should show 2024 edition or later
    cargo --version  # Should show 1.70+ 
    ```

3.  **Build the Application**

    Build the optimized release binary:
    ```bash
    cargo build --release
    ```

    This will create the binary at `target/release/setu`. The release build includes optimizations for production use.

4.  **Install System-Wide (Optional)**

    For easier access, install Setu in your system's `PATH`:

    **On macOS/Linux:**
    ```bash
    sudo cp target/release/setu /usr/local/bin/setu
    # Verify installation
    setu --version
    ```

    **On Windows:**
    ```powershell
    # Copy to a directory in your PATH, for example:
    copy target\release\setu.exe C:\Windows\System32\
    # Or create a dedicated directory
    mkdir C:\Tools
    copy target\release\setu.exe C:\Tools\
    # Add C:\Tools to your PATH environment variable
    ```

#### Option 2: Using Cargo Install

You can also install directly from the repository:

```bash
cargo install --git https://github.com/ribelo/setu.git --locked
```

#### Option 3: Docker (Coming Soon)

Docker images will be available on Docker Hub:

```bash
# This will be available in future releases
docker run -p 3742:3742 ribelo/setu:latest
```

### Verification

Verify your installation:

```bash
setu --version
setu --help
```

You should see the version information and available commands.

### Initial Configuration

The first time you run a Setu command, it will create a default configuration file at `~/.config/setu/setu.toml`. You will need to edit this file to add your AI providers and routing rules.

For a complete guide to all configuration options, see the [**Configuration Guide**](CONFIGURATION.md).

---

## Use Cases and Examples

Setu is designed for developers and organizations who need reliable, scalable AI model access. Here are common use cases:

### 🏢 **Enterprise AI Integration**
- **Multi-team organizations** where different teams prefer different AI providers
- **Cost optimization** by routing requests to the most cost-effective provider
- **Compliance requirements** that mandate specific providers for different data types
- **Risk mitigation** through provider diversification

### 🚀 **Application Development**
- **AI-powered applications** that need provider flexibility without code changes
- **Experimentation** with different models without refactoring
- **A/B testing** different AI providers for performance comparison
- **Development/staging/production** environments with different provider configurations

### 🔧 **Infrastructure & DevOps**
- **API gateway** for AI services across your infrastructure
- **Rate limiting and quota management** across multiple providers
- **Monitoring and logging** centralized AI usage
- **Load balancing** requests across provider endpoints

### 📊 **Research and Development**
- **Model comparison** studies across different providers
- **Performance benchmarking** with consistent interfaces
- **Cost analysis** across different AI services
- **Academic research** requiring provider diversity

---

## Getting Started: A Quick Example

Here's how to get Setu up and running in a few steps.

### 1. Configure a Provider

First, let's configure Setu to use OpenRouter, a service that provides access to many different models.

Open your `~/.config/setu/setu.toml` file and add the following under the `[providers]` section:

```toml
[providers.openrouter]
type = "openrouter"
endpoint = "https://openrouter.ai/api/v1"
models = ["openai/gpt-4o", "google/gemini-flash-1.5", "anthropic/claude-3-haiku"]
```

### 2. Set Your API Key

Setu loads API keys from environment variables for security. Set your OpenRouter API key:

```bash
export OPENROUTER_API_KEY="sk-or-..."
```

### 3. Start the Setu Server

Now, start the Setu proxy server in your terminal:

```bash
setu start
```

You should see output indicating that the server is running, probably on `127.0.0.1:3742`.

### 4. Make an API Request

Open a new terminal and use `curl` to make a request to the Setu server, asking for an OpenRouter model:

```bash
curl -X POST http://127.0.0.1:3742/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openrouter/openai/gpt-4o",
    "messages": [{"role": "user", "content": "What is the purpose of Setu?"}],
    "max_tokens": 200
  }'
```

Setu will receive this request, route it to OpenRouter, and stream the response back to you.

---

## CLI Usage

Setu provides a comprehensive command-line interface for managing your AI proxy server.

### Global Options

All commands support these global options:

```bash
setu [GLOBAL OPTIONS] <COMMAND> [COMMAND OPTIONS]

Global Options:
  -v, --verbose    Enable verbose logging output
  -h, --help       Show help information
  -V, --version    Show version information
```

### Server Management

#### `setu start` - Start the Proxy Server

Starts the HTTP proxy server that handles AI requests.

```bash
setu start [OPTIONS]

Options:
  --host <IP>      Override server host (default: 127.0.0.1)
  --port <PORT>    Override server port (default: 3742)
  --config <PATH>  Use custom configuration file
  --daemon         Run as background daemon (Linux/macOS)
```

**Examples:**
```bash
# Start with default settings
setu start

# Start on all interfaces
setu start --host 0.0.0.0

# Start on custom port with verbose logging
setu start --port 8080 --verbose

# Start with custom config file
setu start --config /etc/setu/production.toml
```

#### `setu stop` - Stop the Server

Gracefully stops a running Setu server.

```bash
setu stop [OPTIONS]

Options:
  --force    Force stop without waiting for active requests
  --timeout  Maximum wait time for graceful shutdown (default: 30s)
```

#### `setu status` - Check Server Status

Shows the current status of the Setu server and configured providers.

```bash
setu status [OPTIONS]

Options:
  --json    Output status in JSON format
  --health  Perform health checks on all providers
```

**Example output:**
```
Setu Server Status
==================
Server: Running (PID: 12345)
Host: 127.0.0.1:3742
Uptime: 2h 34m 12s
Active Connections: 5

Providers:
✓ anthropic (healthy, 45ms avg response)
✓ openrouter (healthy, 120ms avg response)
⚠ gemini (degraded, last error: rate limit)

Recent Activity:
- 1,234 requests processed (last hour)
- 98.5% success rate
- Average response time: 850ms
```

### Authentication Management

#### `setu auth` - Manage Provider Authentication

Handles OAuth 2.0 flows and API key management for various providers.

##### `setu auth anthropic` - Anthropic OAuth Setup

Initiates OAuth 2.0 flow for Anthropic Claude access.

```bash
setu auth anthropic [OPTIONS]

Options:
  --reauth          Force re-authentication even if tokens exist
  --scope <SCOPES>  Request specific OAuth scopes (comma-separated)
  --browser <CMD>   Use specific browser command for OAuth flow
```

**Process:**
1. Opens browser to Anthropic's OAuth consent page
2. User authorizes Setu application
3. Tokens are securely stored in `setu.toml`
4. Automatic token refresh is configured

##### `setu auth google` - Google OAuth Setup

Configures OAuth 2.0 for Google Gemini access.

```bash
setu auth google [OPTIONS]

Options:
  --project <ID>    Google Cloud Project ID for billing
  --reauth          Force re-authentication
  --service-account Use service account authentication instead
```

##### `setu auth validate` - Validate All Credentials

Checks the validity of all configured authentication credentials.

```bash
setu auth validate [OPTIONS]

Options:
  --provider <NAME>  Validate specific provider only
  --refresh          Attempt to refresh expired tokens
  --json            Output validation results in JSON format
```

### Configuration Management

#### `setu config` - Configuration Operations

Manages and validates Setu configuration.

```bash
setu config <SUBCOMMAND> [OPTIONS]

Subcommands:
  validate    Validate current configuration
  show        Display current configuration
  init        Initialize default configuration
  edit        Open configuration in editor
  import      Import configuration from file
  export      Export configuration to file
```

##### `setu config validate` - Validate Configuration

Checks configuration file for errors and validates provider settings.

```bash
setu config validate [OPTIONS]

Options:
  --fix             Attempt to fix common configuration issues
  --strict          Enable strict validation mode
  --config <PATH>   Validate specific configuration file
```

**Example output:**
```
Configuration Validation Results
================================
✓ Configuration file found: ~/.config/setu/setu.toml
✓ Syntax is valid
✓ All required sections present
⚠ Warning: Provider 'gemini' has no API key or OAuth token
✗ Error: Invalid model name 'gpt-5' in routing rules

Summary: 1 error, 1 warning found
```

##### `setu config show` - Display Configuration

Shows current configuration with sensitive data redacted.

```bash
setu config show [OPTIONS]

Options:
  --raw        Show configuration without redaction
  --json       Output in JSON format
  --section <NAME>  Show specific configuration section only
```

##### `setu config init` - Initialize Configuration

Creates a new configuration file with sensible defaults.

```bash
setu config init [OPTIONS]

Options:
  --overwrite      Overwrite existing configuration
  --template <T>   Use configuration template (basic, advanced, enterprise)
  --providers <P>  Include specific providers (comma-separated)
```

### Diagnostics and Troubleshooting

#### `setu diagnose` - System Diagnostics

Performs comprehensive system diagnostics and troubleshooting.

```bash
setu diagnose [OPTIONS]

Options:
  --provider <NAME>     Diagnose specific provider
  --network            Test network connectivity
  --permissions        Check file and directory permissions
  --json               Output diagnostics in JSON format
  --verbose            Include detailed debugging information
```

**Example output:**
```
Setu System Diagnostics
=======================
System Information:
- OS: Linux 5.15.0
- Architecture: x86_64
- Setu Version: 0.1.0

Configuration:
✓ Configuration file exists and is readable
✓ All providers have valid authentication
⚠ Warning: Log directory permissions may be too permissive

Network Connectivity:
✓ anthropic.ai (200ms)
✓ openrouter.ai (150ms)
✗ api.gemini.ai (timeout after 5000ms)

Provider Health:
✓ Anthropic: Healthy (last request: 2 minutes ago)
✓ OpenRouter: Healthy (last request: 30 seconds ago)
⚠ Gemini: No recent successful requests

Recommendations:
1. Check firewall settings for Gemini API access
2. Verify Gemini API key permissions
3. Consider updating log directory permissions: chmod 750 ~/.local/share/setu/logs/
```

### Utility Commands

#### `setu version` - Show Version Information

Displays detailed version and build information.

```bash
setu version [OPTIONS]

Options:
  --short     Show version number only
  --json      Output in JSON format
```

#### `setu help` - Show Help Information

Displays help for any command or subcommand.

```bash
setu help [COMMAND] [SUBCOMMAND]

Examples:
setu help auth          # Show auth command help
setu help auth anthropic # Show specific subcommand help
```

---

## API Documentation

Setu provides a comprehensive OpenAI-compatible REST API that serves as a universal interface for multiple AI providers. This design ensures maximum compatibility with existing tools, libraries, and applications.

### Core Endpoints

#### Chat Completions

**Endpoint:** `POST /v1/chat/completions`

The primary endpoint for AI conversations, compatible with OpenAI's Chat Completions API.

```bash
curl -X POST http://127.0.0.1:3742/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-setu-your-api-key" \
  -d '{
    "model": "anthropic/claude-3-sonnet",
    "messages": [
      {
        "role": "user",
        "content": "Explain quantum computing"
      }
    ],
    "max_tokens": 1000,
    "temperature": 0.7,
    "stream": false
  }'
```

**Response:**
```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1677858242,
  "model": "anthropic/claude-3-sonnet",
  "usage": {
    "prompt_tokens": 13,
    "completion_tokens": 7,
    "total_tokens": 20
  },
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": "Quantum computing is..."
      },
      "finish_reason": "stop",
      "index": 0
    }
  ]
}
```

#### Streaming Chat Completions

Enable real-time streaming responses by setting `"stream": true`:

```bash
curl -X POST http://127.0.0.1:3742/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openrouter/openai/gpt-4",
    "messages": [{"role": "user", "content": "Write a poem"}],
    "stream": true
  }'
```

**Streaming Response Format:**
```
data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677858242,"model":"openrouter/openai/gpt-4","choices":[{"delta":{"role":"assistant"},"index":0,"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677858242,"model":"openrouter/openai/gpt-4","choices":[{"delta":{"content":"In"},"index":0,"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677858242,"model":"openrouter/openai/gpt-4","choices":[{"delta":{"content":" the"},"index":0,"finish_reason":null}]}

data: [DONE]
```

### Model Management

#### List Available Models

**Endpoint:** `GET /v1/models`

Returns a list of all available models across configured providers.

```bash
curl http://127.0.0.1:3742/v1/models
```

**Response:**
```json
{
  "object": "list",
  "data": [
    {
      "id": "anthropic/claude-3-sonnet",
      "object": "model",
      "created": 1677858242,
      "owned_by": "anthropic",
      "provider": "anthropic",
      "capabilities": ["chat", "tools"],
      "context_length": 200000,
      "pricing": {
        "input": 0.003,
        "output": 0.015
      }
    },
    {
      "id": "openrouter/openai/gpt-4",
      "object": "model",
      "created": 1677858242,
      "owned_by": "openai",
      "provider": "openrouter",
      "capabilities": ["chat", "tools", "vision"],
      "context_length": 128000
    }
  ]
}
```

#### Get Model Details

**Endpoint:** `GET /v1/models/{model_id}`

Retrieve detailed information about a specific model.

```bash
curl http://127.0.0.1:3742/v1/models/anthropic/claude-3-sonnet
```

### Health and Status

#### Health Check

**Endpoint:** `GET /health`

Returns server health status and provider availability.

```bash
curl http://127.0.0.1:3742/health
```

**Response:**
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime": 3600,
  "providers": {
    "anthropic": {
      "status": "healthy",
      "last_check": "2024-01-15T10:30:00Z",
      "response_time_ms": 450
    },
    "openrouter": {
      "status": "healthy", 
      "last_check": "2024-01-15T10:30:00Z",
      "response_time_ms": 120
    }
  }
}
```

### Model Routing

Setu uses intelligent routing to determine which provider to use for each request. You can specify routing in several ways:

#### Provider Prefixes
```json
{
  "model": "anthropic/claude-3-sonnet",
  "messages": [...]
}
```

#### Explicit Routing Headers
```bash
curl -X POST http://127.0.0.1:3742/v1/chat/completions \
  -H "X-Setu-Provider: anthropic" \
  -H "X-Setu-Fallback: openrouter,gemini" \
  -d '{"model": "claude-3-sonnet", "messages": [...]}'
```

#### Load Balancing
```bash
curl -X POST http://127.0.0.1:3742/v1/chat/completions \
  -H "X-Setu-Strategy: round_robin" \
  -d '{"model": "gpt-4", "messages": [...]}'
```

### Error Handling

Setu provides detailed error responses that help identify and resolve issues quickly.

#### Common Error Responses

**Provider Unavailable (503):**
```json
{
  "error": {
    "message": "Provider anthropic is currently unavailable",
    "type": "provider_error", 
    "code": "provider_unavailable",
    "provider": "anthropic",
    "retry_after": 30
  }
}
```

**Invalid Model (400):**
```json
{
  "error": {
    "message": "Model 'gpt-5' not found in any configured provider",
    "type": "invalid_request_error",
    "code": "model_not_found",
    "available_models": ["anthropic/claude-3-sonnet", "openrouter/openai/gpt-4"]
  }
}
```

**Rate Limit Exceeded (429):**
```json
{
  "error": {
    "message": "Rate limit exceeded for provider openrouter",
    "type": "rate_limit_error",
    "code": "rate_limit_exceeded",
    "provider": "openrouter",
    "retry_after": 60,
    "limit": 100,
    "reset": 1677858300
  }
}
```

For complete API documentation including all endpoints, parameters, and response formats, see the dedicated [**API Documentation**](API.md).

---

## Troubleshooting

This section covers common issues and their solutions. For comprehensive diagnostics, use `setu diagnose`.

### Configuration Issues

#### Configuration File Not Found
```
Error: Configuration file not found at ~/.config/setu/setu.toml
```

**Solution:**
1. Initialize a new configuration: `setu config init`
2. Or create the directory: `mkdir -p ~/.config/setu/`
3. Verify XDG directories are properly set

#### Configuration Validation Failed
```
Error: Configuration validation failed: Invalid provider configuration
```

**Troubleshooting Steps:**
1. Run `setu config validate --verbose` for detailed error information
2. Check TOML syntax: `toml-lint ~/.config/setu/setu.toml`
3. Verify all required sections are present:
   ```toml
   [server]
   [routing]
   [providers]
   ```
4. Use `setu config init --template basic` to start with a working template

#### Permission Denied Errors
```
Error: Permission denied (os error 13)
```

**Common Causes and Solutions:**
1. **Log directory permissions:**
   ```bash
   chmod 755 ~/.local/share/setu/logs/
   chown $USER:$USER ~/.local/share/setu/logs/
   ```

2. **Configuration file permissions:**
   ```bash
   chmod 600 ~/.config/setu/setu.toml  # Secure but readable
   ```

3. **Port binding issues:**
   - Ports below 1024 require root privileges
   - Use ports 3742+ for non-root operation
   - Check if port is already in use: `lsof -i :3742`

### Authentication Problems

#### OAuth Token Validation Failed
```
Error: OAuth token validation failed: invalid_grant
```

**Resolution Steps:**
1. Clear existing tokens and re-authenticate:
   ```bash
   setu auth anthropic --reauth
   ```

2. Check token expiration:
   ```bash
   setu auth validate --verbose
   ```

3. Verify system clock synchronization (OAuth is time-sensitive):
   ```bash
   sudo ntpdate -s time.nist.gov  # Linux
   sudo sntp -sS time.apple.com   # macOS
   ```

4. Check firewall and proxy settings that might interfere with OAuth flows

#### API Key Issues
```
Error: 401 Unauthorized - Invalid API key
```

**Debugging Steps:**
1. Verify environment variables:
   ```bash
   echo $OPENROUTER_API_KEY
   echo $ANTHROPIC_API_KEY
   ```

2. Check key format and validity:
   - OpenRouter keys start with `sk-or-`
   - Anthropic keys start with `sk-ant-`
   - No extra whitespace or quotes

3. Test keys directly with provider APIs:
   ```bash
   curl -H "Authorization: Bearer $ANTHROPIC_API_KEY" \
        https://api.anthropic.com/v1/messages
   ```

### Network and Connectivity

#### Connection Refused
```
Error: Connection refused (os error 61)
```

**Troubleshooting:**
1. **Verify server is running:**
   ```bash
   ps aux | grep setu
   netstat -tlnp | grep 3742
   ```

2. **Check bind address:**
   - `127.0.0.1` only allows local connections
   - `0.0.0.0` allows external connections
   - Update in config or use: `setu start --host 0.0.0.0`

3. **Firewall configuration:**
   ```bash
   # Linux (iptables)
   sudo iptables -A INPUT -p tcp --dport 3742 -j ACCEPT
   
   # macOS
   # Add port to System Preferences > Security > Firewall
   
   # Windows
   netsh advfirewall firewall add rule name="Setu" dir=in action=allow protocol=TCP localport=3742
   ```

#### Provider Timeout Issues
```
Error: Provider request timeout after 30s
```

**Solutions:**
1. **Increase timeout in configuration:**
   ```toml
   [providers.anthropic]
   timeout_seconds = 60
   ```

2. **Network diagnostics:**
   ```bash
   ping api.anthropic.com
   curl -w "@curl-format.txt" -o /dev/null -s https://api.anthropic.com/
   ```

3. **Check proxy settings:**
   ```bash
   export HTTP_PROXY=http://proxy.company.com:8080
   export HTTPS_PROXY=https://proxy.company.com:8080
   ```

### Provider-Specific Issues

#### Anthropic Claude Errors
```
Error: 429 Too Many Requests - Rate limit exceeded
```

**Mitigation:**
1. Configure rate limiting in `setu.toml`:
   ```toml
   [providers.anthropic.rate_limit]
   requests_per_minute = 50
   burst_size = 10
   ```

2. Enable automatic retries:
   ```toml
   [providers.anthropic.retry]
   max_attempts = 3
   backoff_multiplier = 2
   ```

#### OpenRouter Issues
```
Error: Model not available on OpenRouter
```

**Resolution:**
1. Check available models: `curl https://openrouter.ai/api/v1/models`
2. Update model list in configuration
3. Use model aliases for compatibility:
   ```toml
   [routing.model_aliases]
   "gpt-4" = "openrouter/openai/gpt-4"
   ```

### Performance Issues

#### High Memory Usage
**Diagnosis:**
```bash
# Monitor Setu memory usage
top -p $(pgrep setu)
```

**Solutions:**
1. Adjust connection pool size:
   ```toml
   [server]
   max_connections = 100
   connection_pool_size = 10
   ```

2. Enable request/response streaming:
   ```toml
   [server]
   enable_streaming = true
   buffer_size = 8192
   ```

#### Slow Response Times
**Profiling:**
```bash
# Enable detailed request logging
RUST_LOG=debug setu start

# Analyze response times
tail -f ~/.local/share/setu/logs/requests.log | grep "response_time"
```

**Optimization:**
1. Use connection keep-alive
2. Enable HTTP/2 for providers that support it
3. Configure regional endpoints when available

### Log Analysis

#### Enable Debug Logging
```bash
RUST_LOG=setu=debug,tower_http=debug setu start --verbose
```

#### Log File Locations
- **Request logs:** `~/.local/share/setu/logs/requests.log`
- **Application logs:** `~/.local/share/setu/logs/setu.log`
- **Error logs:** `~/.local/share/setu/logs/errors.log`

#### Structured Log Analysis
```bash
# Filter by provider
jq 'select(.provider == "anthropic")' ~/.local/share/setu/logs/requests.log

# Find failed requests
jq 'select(.status_code >= 400)' ~/.local/share/setu/logs/requests.log

# Analyze response times
jq '.response_time_ms' ~/.local/share/setu/logs/requests.log | sort -n
```

### Getting Help

If you're still experiencing issues:

1. **Run comprehensive diagnostics:**
   ```bash
   setu diagnose --verbose > setu-diagnostics.txt
   ```

2. **Check for known issues:**
   - Visit the GitHub issues page
   - Check the documentation for your specific setup

3. **Create a support request** with:
   - Diagnostic output
   - Configuration file (with sensitive data redacted)
   - Relevant log excerpts
   - Steps to reproduce the issue

For urgent production issues, include your environment details, affected request volume, and business impact.

## Performance and Scaling

Setu is designed to be a lightweight and fast proxy. It uses asynchronous I/O (via Tokio) to handle many concurrent connections efficiently.

-   **Connection Pooling**: Setu reuses HTTP connections to downstream providers, reducing latency for subsequent requests.
-   **Streaming**: For requests that support it (`"stream": true`), Setu streams the response directly from the provider to the client without buffering the entire response in memory. This is highly efficient for large responses.
-   **Resource Usage**: Setu has a small memory footprint and low CPU usage when idle. When actively proxying requests, resource usage will scale with the number of concurrent connections.

For high-traffic environments, you can run multiple instances of Setu behind a load balancer. Since Setu is stateless (all configuration is in the `setu.toml` file), it is easy to scale horizontally.

---

## Development

### Code Quality

To ensure code quality, run the following checks before committing:

```bash
cargo check --all-targets && cargo clippy -- -D warnings && cargo fmt
```

### Running Tests

```bash
cargo test
```

## License

This project is currently under a TBD license.