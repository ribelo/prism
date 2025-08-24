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

## Testing Approach

- Unit tests in `tests/` directory
- Test individual configs with `cargo test --test <test_name>`
- Integration tests for routing and API transformation logic