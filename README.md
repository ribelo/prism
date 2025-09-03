# Setu: The Universal AI Model Router

**Setu** (from the Sanskrit word for "bridge") is a provider- and client-agnostic intelligent routing middleware for AI model interactions. It acts as a local proxy server that can intelligently route requests to different AI providers, handle authentication, and provide a unified API for all your AI needs.

![Setu Architecture Diagram](docs/ARCHITECTURE.md)
*For a detailed look at the system design, see the [Architecture Document](docs/ARCHITECTURE.md).*

---

## Key Features

-   **Unified API**: Interact with multiple AI providers (Anthropic, OpenRouter, Gemini, etc.) through a single, OpenAI-compatible API.
-   **Intelligent Routing**: Configure routing rules based on model names, provider priority, or other criteria.
-   **Automatic Fallback**: If a request to one provider fails, Setu can automatically retry with the next provider in your priority list.
-   **OAuth 2.0 Management**: Setu handles the complexities of OAuth 2.0 for providers like Anthropic and Gemini, automatically refreshing tokens for you.
-   **Centralized Configuration**: Manage all your provider settings, API keys, and routing rules in a single `setu.toml` file.
-   **Extensible**: Designed to be easily extended with new providers and routing strategies.
-   **Local First**: Runs as a local server, keeping your configurations and request metadata on your machine.

---

## Installation and Setup

### Prerequisites

-   **Rust**: Setu is written in Rust. You'll need the Rust toolchain (2024 edition or later). You can install it from [rust-lang.org](https://www.rust-lang.org/tools/install).
-   **Cargo**: The Rust package manager, which is included with the Rust installation.
-   **Git**: To clone the repository.

### Installation Steps

1.  **Clone the Repository**

    ```bash
    git clone https://github.com/ribelo/setu.git
    cd setu
    ```

2.  **Build the Application**

    Build the Setu binary using Cargo.

    ```bash
    cargo build --release
    ```

    The optimized binary will be located at `target/release/setu`.

3.  **Install the Binary (Optional)**

    For easier access, you can copy the binary to a location in your system's `PATH`.

    ```bash
    cp target/release/setu /usr/local/bin/setu
    ```

    Now you can run Setu from anywhere using the `setu` command.

### Initial Configuration

The first time you run a Setu command, it will create a default configuration file at `~/.config/setu/setu.toml`. You will need to edit this file to add your AI providers and routing rules.

For a complete guide to all configuration options, see the [**Configuration Guide**](CONFIGURATION.md).

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

Setu is controlled via a command-line interface.

### `setu start`

Starts the HTTP proxy server.

```bash
setu start [--host <IP>] [--port <PORT>]
```

-   `--host`: Override the host address from the config file.
-   `--port`: Override the port from the config file.

### `setu auth`

Manages authentication for providers that use OAuth 2.0.

#### `setu auth anthropic`

Initiates the OAuth 2.0 flow for Anthropic. This will open a browser window for you to authorize Setu. The resulting tokens will be stored securely in your `setu.toml`.

```bash
setu auth anthropic
```

#### `setu auth google`

Initiates the OAuth 2.0 flow for Google (for Gemini).

```bash
setu auth google
```

### `setu config`

Validates your current configuration file.

```bash
setu config
```

If the configuration is valid, it will print a summary. If not, it will show an error.

---

## API Documentation

Setu provides an OpenAI-compatible API endpoint for chat completions. You can use it with any tool or library that is compatible with the OpenAI API.

For detailed information on the API endpoints, request/response formats, authentication, and error handling, please see the [**API Documentation**](API.md).

---

## Troubleshooting

-   **"Configuration validation failed"**: Run `setu config` to get more details about what's wrong with your `setu.toml` file.
-   **"OAuth token validation failed"**: Your OAuth tokens may have expired or been revoked. Run `setu auth <provider>` again to get new tokens.
-   **401 Unauthorized**: Make sure you have set the correct API key as an environment variable (e.g., `OPENROUTER_API_KEY`) or that your OAuth tokens are valid.
-   **502 Bad Gateway**: This means Setu successfully sent a request to the downstream provider, but the provider returned an error. Check the Setu logs for more details about the error from the provider.
-   **Connection Refused**: Make sure the Setu server is running (`setu start`). Check the host and port you are trying to connect to.

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