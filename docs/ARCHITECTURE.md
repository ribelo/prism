# Setu Architecture

## Overview

Setu is a universal AI model router that acts as an HTTP proxy daemon for routing AI requests between any client and any provider. It provides seamless translation between different AI provider APIs while maintaining a consistent interface for clients.

## Core Concept

Setu runs as a standalone binary providing both CLI and daemon functionality:
- **CLI Mode**: Configuration and management commands
- **Daemon Mode**: HTTP proxy server on localhost that clients connect to

```
┌─────────────┐    HTTP     ┌─────────────┐    Provider APIs    ┌─────────────┐
│   Clients   │────────────→│    Setu     │───────────────────→│  Providers  │
│             │             │   Daemon    │                    │             │
│ • Claude    │             │             │                    │ • OpenAI    │
│   Code      │             │ ┌─────────┐ │                    │ • Anthropic │
│ • Any HTTP  │             │ │ ai-ox   │ │                    │ • Gemini    │
│   Client    │             │ │ Router  │ │                    │ • Local     │
└─────────────┘             │ └─────────┘ │                    └─────────────┘
                            └─────────────┘
```

## Technical Stack

- **CLI Framework**: Clap for command-line interface
- **HTTP Server**: Axum for minimal, high-performance HTTP server
- **Configuration**: Figment + TOML for flexible configuration management
- **AI Integration**: ai-ox workspace (path dependency) for provider abstractions
- **Error Handling**: thiserror + backon + tokio for robust error management
- **XDG Compliance**: directories crate for proper config/cache/data directories
- **Observability**: Tracing crate for logging and monitoring
- **Architecture**: Stateless design for simplicity and reliability

## Core Components

### HTTP Proxy Server (Axum)
- **OpenAI API Compatibility**: `/v1/chat/completions`, `/v1/models`, etc.
- **Anthropic API Compatibility**: `/v1/messages`, `/v1/complete`, etc.
- **Smart Translation**: Transform requests/responses based on endpoint and target provider
- **Example Flow**: Client calls OpenAI endpoint → requesting Anthropic model → transform to Anthropic format → get Anthropic response → transform back to OpenAI format
- **Streaming Support**: Real-time response streaming for compatible providers

### ai-ox Integration
- **Provider Abstraction**: Leverages existing ai-ox provider implementations
- **Built-in Conversions**: Uses ai-ox's existing From/Into traits for format conversion
- **Unified ai-ox Format**: All requests convert to ai-ox common format, then to target provider
- **No Custom Translators**: Rely on ai-ox conversions like `From<AnthropicRequest> for OpenAIRequest`
- **Dogfooding**: Improve ai-ox through real-world usage in setu, adding missing conversions as needed

### Configuration System (Figment + TOML + XDG)
- **XDG Compliance**: Configuration in `$XDG_CONFIG_HOME/setu/` using directories crate
- **Environment Variables**: Provider credentials via environment variables
- **OAuth Support**: Future support for Claude Code and Google Gemini OAuth
- **Flexible Config**: Support for multiple config sources and formats
- **Provider Definitions**: Configuration of available AI providers
- **Routing Rules**: Simple name-based routing in Phase 1

### Routing Engine (Phase 1)
- **Name-Based Routing**: Route requests by explicit provider/model name
- **Stateless Design**: No session state between requests
- **Future-Proof**: Architecture ready for intelligent routing in later phases

## Project Structure

```
setu/
├── Cargo.toml              # Dependencies: clap, axum, figment, ai-ox, tracing, 
│                           # thiserror, backon, tokio, directories
├── src/
│   ├── main.rs             # CLI entry point (clap)
│   ├── server/             # HTTP proxy server (axum)
│   │   ├── mod.rs          # Server setup and request handling
│   │   └── routes.rs       # API route handlers for all endpoints
│   ├── router/             # Routing logic
│   │   ├── mod.rs          # Core routing engine
│   │   └── name_based.rs   # Phase 1: name-based routing
│   ├── config/             # Configuration management (XDG compliant)
│   │   ├── mod.rs          # Figment + directories integration
│   │   └── models.rs       # Configuration data structures
│   ├── error.rs            # Error types (thiserror)
│   └── lib.rs              # Library exports
└── docs/
    └── ARCHITECTURE.md     # This file
```

## XDG Directory Structure

```
$XDG_CONFIG_HOME/setu/     # ~/.config/setu/
├── setu.toml              # Main configuration
└── providers.toml         # Provider definitions

$XDG_DATA_HOME/setu/       # ~/.local/share/setu/
├── logs/                  # Application logs
└── cache/                 # Response cache (future)

$XDG_CACHE_HOME/setu/      # ~/.cache/setu/
└── temp/                  # Temporary files
```

## Configuration Example (TOML)

```toml
[daemon]
host = "127.0.0.1"
port = 3742
log_level = "info"

# Provider credentials via environment variables
[providers.openai]
type = "openai"
endpoint = "https://api.openai.com/v1"
# OPENAI_API_KEY environment variable
models = ["gpt-4", "gpt-3.5-turbo", "gpt-4-turbo"]

[providers.anthropic]
type = "anthropic"
endpoint = "https://api.anthropic.com"
# ANTHROPIC_API_KEY environment variable
# Future: OAuth support for Claude Code
models = ["claude-3-opus", "claude-3-sonnet", "claude-3-haiku"]

[providers.gemini]
type = "gemini"
endpoint = "https://generativelanguage.googleapis.com"
# GEMINI_API_KEY environment variable
# Future: OAuth support
models = ["gemini-pro", "gemini-pro-vision", "gemini-1.5-pro"]

[routing]
# Phase 1: Simple name-based routing
# Request to /v1/chat/completions?provider=anthropic routes to Anthropic
# Request to /v1/chat/completions?model=gpt-4 routes to OpenAI
default_provider = "openai"
```

## Protocol Translation Logic Using ai-ox

Translation leverages ai-ox's built-in conversion traits:

```
Client Request (OpenAI format)     Setu Processing              Provider Response
┌─────────────────────────┐       ┌──────────────────┐       ┌─────────────────────┐
│ POST /v1/chat/completions│──────→│1. Parse endpoint │──────→│ GET response from    │
│ model: "anthropic/opus"  │       │2. Detect target │       │ Anthropic via       │
│ messages: [...]          │       │   provider       │       │ ai-ox client        │
│ (OpenAI JSON format)     │       │3. Convert using  │       │                     │
└─────────────────────────┘       │   ai-ox From     │       │                     │
                                  │   traits         │       │                     │
                                  │4. Make request   │       │                     │
                                  │5. Convert back   │       │                     │
                                  └──────────────────┘       └─────────────────────┘
                                           │
                                           ▼
Client Response (OpenAI format)    ┌──────────────────┐
┌─────────────────────────┐       │ ai-ox handles    │
│ {                        │◀──────│ response format  │
│   "choices": [...],      │       │ conversion back  │
│   "model": "claude-3-opus"│       │ to OpenAI        │
│ }                        │       └──────────────────┘
└─────────────────────────┘

ai-ox Conversion Flow:
OpenAI Request → ai-ox::Request → Anthropic Request
Anthropic Response → ai-ox::Response → OpenAI Response
```

## Phase 1 Implementation Goals

1. **CLI Setup**: Basic clap-based command interface
2. **XDG Compliance**: Proper config directory structure using directories crate
3. **HTTP Server**: Axum-based localhost proxy with OpenAI/Anthropic endpoints
4. **ai-ox Integration**: Path dependency integration with existing providers
5. **Format Conversion**: Use ai-ox From/Into traits for request/response conversion
6. **Name-Based Routing**: Route by provider name or model name from request
7. **Error Handling**: Robust error handling with thiserror and backon
8. **Configuration**: Figment + TOML configuration with environment variable support
9. **Observability**: Tracing-based logging and monitoring

## Future Phases

- **Phase 2**: Intelligent routing (cost, latency, capabilities)
- **Phase 3**: Load balancing and failover
- **Phase 4**: Advanced analytics and monitoring
- **Phase 5**: Custom routing logic and plugins

## Development Philosophy

- **Make it work**: Focus on core functionality first
- **Make it right**: Clean architecture and code quality
- **Make it fast**: Performance optimization last
- **Dogfooding**: Use setu development to improve ai-ox