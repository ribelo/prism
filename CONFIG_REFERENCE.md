# Setu Configuration Reference

This document describes all supported configuration options in Setu's simplified configuration system.

## Configuration Location

- **Config file**: `~/.config/setu/setu.toml`
- **Log directory**: `~/.local/share/setu/logs/` (or custom path)
- **File permissions**: 600 (owner read/write only) for security

## Configuration Sources

Setu uses Figment for configuration, supporting multiple sources in order of precedence:

1. **Environment variables** (highest priority) - `SETU_` prefixed for config overrides
2. **API keys** - Standard environment variables (ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY)
3. **TOML file** - `~/.config/setu/setu.toml`
4. **Default values** (lowest priority)

## Complete Configuration Structure

```toml
[server]
host = "127.0.0.1"
port = 3742
log_level = "info"
log_file_enabled = true
log_rotation = "daily"
log_dir = "/custom/log/path"  # Optional, defaults to ~/.local/share/setu/logs/
log_file_prefix = "setu"

[routing]

# Model-to-model routing with fallback support
[routing.models]
"small-model" = "openai/gpt-4o"                                                  # Single model routing
"anthropic/claude-sonnet-4" = ["openai/gpt-4o", "openrouter/glm-4.5:fireworks"]  # Multiple fallback models

[providers.anthropic]
type = "anthropic"
endpoint = "https://api.anthropic.com"
api_key = "${ANTHROPIC_API_KEY}"  # Environment variable interpolation
api_key_fallback = true           # Enable API key fallback on OAuth failure
fallback_on_errors = [429]        # HTTP errors that trigger fallback

[providers.anthropic.auth]
oauth_access_token = "token_value"
oauth_refresh_token = "refresh_value"
oauth_expires = 1234567890  # Unix timestamp in milliseconds
project_id = "project_id"

[providers.anthropic.retry]
max_retries = 3
initial_interval_ms = 1000
max_interval_ms = 30000
multiplier = 2.0

[providers.openai]
type = "openai"
endpoint = "https://api.openai.com"
api_key = "${OPENAI_API_KEY}"
api_key_fallback = false
fallback_on_errors = [429]

[providers.gemini]
type = "gemini"
endpoint = "https://generativelanguage.googleapis.com"
api_key = "${GEMINI_API_KEY}"
api_key_fallback = true
fallback_on_errors = [429]

# Global auth section (alternative to provider-specific auth)
[auth.anthropic]
oauth_access_token = "token_value"
oauth_refresh_token = "refresh_value"
oauth_expires = 1234567890
project_id = "project_id"
```

## Custom Endpoint Examples

### Custom Server (Chutes via Anthropic-compatible API)
```toml
[providers.chutes]
type = "anthropic"  # Use anthropic-compatible handling (works via /v1/messages endpoint)
endpoint = "https://api.chutes.ai/v1"
api_key = "${CHUTES_API_KEY}"
api_key_fallback = false

[providers.chutes.retry]
max_retries = 3
initial_interval_ms = 1000
max_interval_ms = 30000
multiplier = 2.0
```

**Usage**: `chutes/some-model-name` → Routes to `https://api.chutes.ai/v1` (via `/v1/messages`)

### Multiple Custom Providers
```toml
[providers.local-llm]
type = "anthropic"
endpoint = "http://localhost:8080"
api_key = "local-key"

[providers.custom-ai]
type = "anthropic" 
endpoint = "https://my-ai-server.com/v1"
api_key = "${CUSTOM_AI_KEY}"
```

**Usage**:
- `local-llm/claude-3-sonnet` → Routes to localhost:8080
- `custom-ai/gpt-4` → Routes to my-ai-server.com

## Configuration Sections

### Server Configuration

Controls HTTP server behavior and logging.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `host` | String | `"127.0.0.1"` | Server bind address |
| `port` | u16 | `3742` | Server port (SETU on phone keypad) |
| `log_level` | String | `"info"` | Logging level (trace, debug, info, warn, error) |
| `log_file_enabled` | Boolean | `true` | Enable file logging |
| `log_rotation` | String | `"daily"` | Log rotation strategy |
| `log_dir` | String | None | Custom log directory path |
| `log_file_prefix` | String | `"setu"` | Log file name prefix |

**Environment variables**:
- `SETU_SERVER_HOST` - Override server host
- `SETU_SERVER_PORT` - Override server port
- `SETU_SERVER_LOG_LEVEL` - Override logging level

### Routing Configuration

**Simplified routing system** - removes the over-engineered complexity of the old system.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `models` | HashMap | `{}` | Model-to-model routing with fallback chains |

**Model routing format**:
- **Single model**: `"haiku-3.5" = "openai/gpt-4o"`
- **Fallback chain**: `"claude-3" = ["openai/gpt-4o", "openrouter/glm-4.5:fireworks"]`
- **Unmapped models**: Pass through to existing provider inference

**Environment variables**:

### Provider Configuration

Each provider has its own configuration section with API key fallback support.

| Option | Type | Required | Description |
|--------|------|----------|-------------|
| `type` | String | Yes | Provider type identifier |
| `endpoint` | String | Yes | API endpoint URL |
| `auth` | AuthConfig | No | OAuth authentication configuration |
| `retry` | RetryConfig | No | Retry policy configuration |
| `api_key` | String | No | API key with environment variable interpolation |
| `api_key_fallback` | Boolean | No | Enable OAuth→API key fallback |
| `fallback_on_errors` | Array | No | HTTP errors triggering fallback |

**Supported provider types**: `anthropic`, `openai`, `gemini`, `openrouter`

### Authentication Configuration

OAuth and API key authentication for providers.

| Option | Type | Description |
|--------|------|-------------|
| `oauth_access_token` | String | OAuth access token |
| `oauth_refresh_token` | String | OAuth refresh token |
| `oauth_expires` | u64 | Token expiration (Unix timestamp in milliseconds) |
| `project_id` | String | Provider project identifier |

**Token management**:
- Tokens auto-refresh 10 minutes before expiration
- Secure storage with 600 file permissions
- Shared with CLI tools (Anthropic, codex CLI, Gemini CLI)

### API Key Configuration

**Environment variable interpolation** with `${VAR}` syntax:

```toml
api_key = "${ANTHROPIC_API_KEY}"    # Interpolate from environment
api_key = "sk-ant-hardcoded-key"    # Explicit value
api_key = null                      # No API key configured
```

**API Key Fallback**:
- Set `api_key_fallback = true` to enable OAuth→API key fallback
- Configure `fallback_on_errors = [429, 401]` for specific HTTP errors
- API keys are tried when OAuth fails with configured errors

### Retry Configuration

Exponential backoff retry policies for API calls.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `max_retries` | u32 | `3` | Maximum retry attempts |
| `initial_interval_ms` | u64 | `1000` | Initial retry delay (milliseconds) |
| `max_interval_ms` | u64 | `30000` | Maximum retry delay (milliseconds) |
| `multiplier` | f32 | `2.0` | Backoff multiplier |

**Retry behavior**:
- Only applies to non-streaming requests
- Uses exponential backoff with jitter
- Logs retry attempts and failures

## Supported API Endpoints

Setu supports three different API endpoint formats with automatic request/response conversion between providers:

### OpenAI Format
- **Endpoint**: `/v1/chat/completions`
- **Content-Type**: `application/json`
- **Request Format**: OpenAI chat completion format with `messages` array
- **Usage**: Compatible with OpenAI API clients
- **Provider Support**: Routes to OpenRouter by default

### Anthropic Format  
- **Endpoint**: `/v1/messages`
- **Content-Type**: `application/json` 
- **Request Format**: Anthropic messages format with `messages` array and `max_tokens`
- **Usage**: Compatible with Claude Code and Anthropic API clients
- **Provider Support**: Routes to all providers with format conversion

### Gemini Format
- **Endpoint**: `/v1beta/models/{model}:generateContent`
- **Content-Type**: `application/json`
- **Request Format**: Google Gemini format with `contents` array containing `role` and `parts`
- **Usage**: Compatible with Google AI Studio and Gemini API clients
- **Provider Support**: Routes to all providers with format conversion
- **Model Parameter**: Extracted from URL path (e.g., `/v1beta/models/anthropic/claude-3-5-sonnet:generateContent`)

**Example Gemini API Call**:
```bash
curl -X POST http://127.0.0.1:3742/v1beta/models/openrouter/z-ai/glm-4.5:generateContent \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [
      {
        "role": "user",
        "parts": [{"text": "Hello"}]
      }
    ]
  }'
```

## Model-to-Model Routing Examples

### Basic Model Mapping
```toml
[routing.models]
"haiku" = "openai/gpt-4o-mini"
"opus" = "anthropic/claude-3-opus"
"gemini" = "gemini/gemini-1.5-pro"
```

### Fallback Chains
```toml
[routing.models]
# Try GPT-4o first, then fallback to Anthropic if it fails
"best-model" = ["openai/gpt-4o", "anthropic/claude-3-5-sonnet"]

# OpenRouter fallbacks with provider preferences
"cheap-model" = [
  "openrouter/gpt-4o-mini:floor",    # Cheapest price
  "openrouter/claude-3-haiku:nitro", # High throughput
  "gemini/gemini-1.5-flash"          # Final fallback
]
```

### Provider-Specific Routing
```toml
[routing.models]
"production" = "anthropic/claude-3-5-sonnet:anthropic"
"development" = "openrouter/gpt-4o:together"
"testing" = ["gemini/gemini-1.5-flash", "openai/gpt-4o-mini"]
```

## API Key Fallback Examples

### Anthropic with Fallback
```toml
[providers.anthropic]
type = "anthropic"
endpoint = "https://api.anthropic.com"
models = ["claude-3-5-sonnet-20241022"]
api_key = "${ANTHROPIC_API_KEY}"
api_key_fallback = true
fallback_on_errors = [429, 401, 403]  # Rate limit and auth errors

[providers.anthropic.auth]
oauth_access_token = "token_from_claude_code_cli"
# ... other OAuth fields
```

**Behavior**:
1. Try OAuth first (subscription billing)
2. On 429/401/403 errors, fallback to API key (pay-per-use)
3. Return aggregated error if both fail

### Gemini Free Tier + API Key
```toml
[providers.gemini]
type = "gemini"
endpoint = "https://generativelanguage.googleapis.com"
models = ["gemini-1.5-flash"]
api_key = "${GEMINI_API_KEY}"
api_key_fallback = true
fallback_on_errors = [429]  # Free tier quota exceeded

[providers.gemini.auth]
oauth_access_token = "token_from_gemini_cli"
# ... other OAuth fields
```

## Environment Variables

### Standard API Keys
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-proj-..."
export GEMINI_API_KEY="AIza..."
```

### Configuration Override
```bash
export SETU_SERVER_HOST="0.0.0.0"
export SETU_SERVER_PORT=8080
```

## Migration from Old System

**Removed complexity** (no longer supported):
- ~~`strategy`~~ - Always uses intelligent routing
- ~~`enable_fallback`~~ - Fallbacks defined per-model
- ~~`min_confidence`~~ - No confidence scoring needed
- ~~`rules`~~ - Replaced by `models` mapping
- ~~`provider_priorities`~~ - Define order in fallback arrays
- ~~`provider_capabilities`~~ - Auto-detected from provider types
- ~~`provider_aliases`~~ - Use explicit model mapping instead

**New features**:
- Model-to-model routing with fallback chains
- API key fallback on OAuth failure
- Environment variable interpolation
- Simplified configuration structure

## Default Provider Configurations

### Anthropic
- **Endpoint**: `https://api.anthropic.com`
- **Auth**: OAuth via Claude Code CLI + API key fallback

### OpenAI
- **Endpoint**: `https://api.openai.com`
- **Auth**: OAuth via codex CLI + API key fallback

### Gemini
- **Endpoint**: `https://generativelanguage.googleapis.com/v1beta`
- **Auth**: OAuth via Gemini CLI + API key fallback
- **API Format**: `/v1beta/models/{model}:generateContent` (Google's native format)

### OpenRouter
- **Endpoint**: `https://openrouter.ai/api/v1`
- **Auth**: API key only (no OAuth)

### Custom Endpoints
Custom providers can be configured with any name and endpoint:
- **Example**: `chutes/some-model-name` → routes to custom server via `/v1/messages` endpoint
- **Auth**: API key authentication
- **Note**: Currently works via Anthropic-compatible API format

## Configuration Validation

Setu validates configuration on startup:
- Model routing targets must reference valid providers
- API keys are interpolated and validated if present
- Provider endpoints must be valid URLs
- Retry parameters must be positive
- Fallback chains cannot be circular

## Security Features

- Config file permissions restricted to owner (600)
- OAuth tokens stored securely with auto-refresh
- API keys loaded from environment variables
- Sensitive values excluded from logs
- Request/response data truncated in error logs
