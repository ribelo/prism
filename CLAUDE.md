# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Setu is a universal AI model router - an HTTP proxy daemon that provides provider and client agnostic intelligent routing middleware for AI model interactions. It acts as a local proxy server that translates between different AI provider APIs (OpenAI, Anthropic, Gemini, etc.) while maintaining a consistent interface for clients.

## Development Commands

### Building
```bash
cargo build                # Build debug version
cargo build --release      # Build optimized release
```

### Running
```bash
cargo run -- start         # Start the HTTP proxy server
cargo run -- auth anthropic # Authenticate with Anthropic OAuth
cargo run -- config        # Validate configuration
```

### Testing & Quality Checks
```bash
cargo test                                              # Run all tests
cargo test --test config_tests                        # Run specific test file
cargo check --all-targets                             # Quick compilation check
cargo clippy -- -D warnings                          # Lint with all warnings as errors
cargo fmt                                             # Format code

# Combined quality check (run before committing)
cargo check --all-targets && cargo clippy -- -D warnings && cargo fmt
```

### Nix Development Environment
```bash
nix develop               # Enter dev shell with nightly Rust and dependencies
```

## Architecture

### Core Components

1. **HTTP Proxy Server** (`src/server/`)
   - Axum-based server handling OpenAI/Anthropic API endpoints
   - Routes requests to appropriate providers based on model/provider name
   - Transforms requests/responses between different API formats

2. **Routing Engine** (`src/router/`)
   - Name-based routing in Phase 1 (route by provider/model name)
   - Stateless request handling

3. **Configuration** (`src/config/`)
   - XDG-compliant configuration using Figment + TOML
   - Config location: `~/.config/setu/setu.toml`
   - Environment variable support for API keys

4. **Authentication** (`src/auth/`)
   - OAuth support for Anthropic (Claude Code)
   - Token management and refresh logic

5. **AI Provider Integration**
   - Uses ai-ox workspace (path dependency at `../ai-ox/`)
   - Leverages ai-ox's From/Into traits for format conversion
   - All requests flow: Client Format → ai-ox::Request → Provider Format

### Request Flow

```
Client (OpenAI format) → Setu Server → ai-ox conversion → Provider (Anthropic/etc)
                            ↑                                    ↓
                         Response ← ai-ox conversion ← Provider Response
```

## Key Dependencies

- **ai-ox**: Path dependency at `../ai-ox/` - provides provider abstractions and format conversions
- **anthropic-ox**: Path dependency at `../ai-ox/crates/anthropic-ox` - Anthropic provider implementation
- **openrouter-ox**: Path dependency at `../ai-ox/crates/openrouter-ox` - OpenRouter provider implementation

## Configuration Structure

Configuration files are stored in XDG directories:
- Config: `~/.config/setu/setu.toml`
- Logs: `~/.local/share/setu/logs/`
- Cache: `~/.cache/setu/` (future)

## API Endpoints

The server provides compatibility with both OpenAI and Anthropic API formats:
- OpenAI: `/v1/chat/completions`, `/v1/models`, etc.
- Anthropic: `/v1/messages`, `/v1/complete`, etc.

## Environment Variables

Provider API keys can be set via environment variables:
- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`
- `GEMINI_API_KEY`

## Error Handling

Uses `thiserror` for error types and `backon` for retry logic. All errors flow through `SetuError` type defined in `src/error.rs`.

## Testing with Claude CLI

Setu can be tested using the Claude CLI tool by setting the base URL to point to the local setu server:

```bash
# Start setu server
cargo run -- start

# Test with Claude CLI (in another terminal)
ANTHROPIC_BASE_URL=http://localhost:3742 claude -p "hello"
```

This allows testing the full request flow through setu's routing system while using Claude Code's OAuth credentials automatically.

### Claude CLI Model Parameter Limitation

**Important**: The Claude CLI `--model` parameter does not work with custom `ANTHROPIC_BASE_URL`. When using a custom base URL, Claude CLI ignores the `--model` parameter and sends its own default model names (e.g., `claude-3-5-haiku-20241022`, `claude-sonnet-4-20250514`).

```bash
# This will NOT work as expected - model parameter is ignored
ANTHROPIC_BASE_URL=http://localhost:3742 claude --model openrouter/z-ai/glm-4.5 -p "test"

# Claude CLI will still send claude-3-5-haiku-20241022 instead of openrouter/z-ai/glm-4.5
```

To test different model routing through setu, you would need to:
1. Use a different client that respects custom models with custom base URLs
2. Test setu's routing logic directly via HTTP requests
3. Configure setu's routing rules to map Claude's default models to desired providers

## Testing Approach

- Unit tests in `tests/` directory
- Test individual configs with `cargo test --test <test_name>`
- Integration tests for routing and API transformation logic

## Code Quality Standards

### Display vs String Methods
- **User-facing output**: Always implement `Display` trait instead of custom `to_string()` methods
- **Error messages**: Use `thiserror::Error` which automatically provides `Display`
- **Logging output**: Prefer `{}` (Display) over `{:?}` (Debug) for user-readable messages
- **Examples**:
  ```rust
  // Good: Implement Display trait
  impl fmt::Display for TokenInfo {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
          write!(f, "{} - {}", self.source, self.status)
      }
  }
  
  // Use in logging
  tracing::info!("Token status: {}", token_info);  // Display
  tracing::debug!("Token debug: {:?}", token_info); // Debug
  ```

### Serialization Standards
- **JSON responses**: Always implement `Serialize` trait for API responses
- **Configuration**: Use both `Serialize` and `Deserialize` for config structs
- **Internal data transfer**: Implement `Serialize` for data that crosses boundaries
- **Examples**:
  ```rust
  // API response types
  #[derive(Debug, Serialize)]
  pub struct ApiResponse {
      pub status: String,
      pub data: Value,
  }
  
  // Configuration types
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Config {
      pub server: ServerConfig,
      pub providers: HashMap<String, Provider>,
  }
  ```

### Pattern Summary
- **Display**: User messages, logs, CLI output, error descriptions
- **Serialize**: JSON APIs, config files, data persistence
- **Debug**: Development debugging, internal logging only