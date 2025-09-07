# Prism Configuration Reference

Config file: `~/.config/prism/prism.toml`

## Basic Prismp

```toml
[server]
host = "127.0.0.1"
port = 3742

[providers.anthropic]
type = "anthropic"
endpoint = "https://api.anthropic.com"

[providers.openai]  
type = "openai"
endpoint = "https://api.openai.com"

[providers.gemini]
type = "gemini" 
endpoint = "https://generativelanguage.googleapis.com"

[providers.openrouter]
type = "openrouter"
endpoint = "https://openrouter.ai/api/v1"
```

## API Keys

Set environment variables:
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-proj-..."  
export GEMINI_API_KEY="AIza..."
export OPENROUTER_API_KEY="sk-or-..."
```

Or in config:
```toml
[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"  # Environment variable
api_key = "sk-ant-hardcoded"      # Direct value
```

## OAuth

Authenticate once:
```bash
prism auth anthropic  # For Claude Code
prism auth google     # For Gemini CLI  
prism auth openai     # For Codex CLI
```

OAuth tokens stored automatically in config.

## Model Routing

```toml
[routing.models]
"fast" = "openrouter/z-ai/glm-4.5"
"smart" = "anthropic/claude-3-5-sonnet"
"cheap" = ["openrouter/gpt-4o-mini", "gemini/gemini-1.5-flash"]
```

Use with model name:
```bash
curl -d '{"model": "fast", "messages": [...]}'
curl -d '{"model": "cheap", "messages": [...]}'  # Tries first, falls back to second
```

## Custom Endpoints

```toml
[providers.custom]
type = "anthropic"  # Use Anthropic format
endpoint = "https://my-server.com/v1"
api_key = "my-key"
```

Use: `custom/any-model-name`

## API Key Fallback

```toml
[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
api_key_fallback = true           # Use API key if OAuth fails
fallback_on_errors = [429]        # On rate limit errors
```

Tries OAuth first, falls back to API key on 429 errors.

## Retry Settings

```toml
[providers.anthropic.retry]
max_retries = 3
initial_interval_ms = 1000
max_interval_ms = 30000
multiplier = 2.0
```

## Complete Example

```toml
[server]
host = "127.0.0.1"  
port = 3742

[routing.models]
"haiku" = "anthropic/claude-3-haiku"
"sonnet" = "anthropic/claude-3-5-sonnet"
"gpt4" = "openrouter/openai/gpt-4o"
"best" = ["anthropic/claude-3-5-sonnet", "openrouter/openai/gpt-4o"]

[providers.anthropic]
type = "anthropic"
endpoint = "https://api.anthropic.com"
api_key = "${ANTHROPIC_API_KEY}"
api_key_fallback = true
fallback_on_errors = [429]

[providers.openrouter]
type = "openrouter" 
endpoint = "https://openrouter.ai/api/v1"
api_key = "${OPENROUTER_API_KEY}"

[providers.local]
type = "anthropic"
endpoint = "http://localhost:8080"
api_key = "local-key"
```

## Environment Overrides

```bash
export PRISM_SERVER_HOST="0.0.0.0"
export PRISM_SERVER_PORT=8080
```

That's it. Everything else is optional.