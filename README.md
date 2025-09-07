# Prism

Universal AI model router - local HTTP proxy for AI providers.

## Why this exists

**Claude Code has the best TUI and developer experience.** Anthropic clearly loves Claude Code and built a beautiful interface for it. Problem: it only works with Anthropic models.

**Every other router sucks at OAuth.** You get XOR: use Anthropic models with OAuth *or* use other models with a router, but never both. That's stupid.

**Available routers have garbage model configs.** Sure, you can usually edit the whole response, but that's messy boilerplate hell. Prism gives you URL-like model parameters that actually fucking work: `openrouter/z-ai/glm-4.5:fireworks?temperature=0.7&max_tokens=2000&top_k=40`. Set all provider parameters through the model string. Map any alias to full model configs. Clean, concise, and gets shit done. No JSON wrestling, no config file hunting - just append your params and go. Worse is better.

**Every app can use every model.** OAuth support for Claude and Gemini. Simple proxy means any model works through any interface. Want GPT-5 in Claude Code? Done. Want to use Claude Max plan in any OpenAI-compatible client? Done. Want to use whatever the fuck CRUSH is? Done. No more vendor lock-in bullshit.

**This definitely violates every provider's TOS.** You should absolutely never use this. When (not if) they detect the proxy and block your account, don't come crying. I warned you. Use at your own risk and don't blame me when your $200/month Claude subscription gets nuked because you wanted to use it with some random AI editor that definitely looks suspicious in their logs.

## What it does

**Transparent request transformation.** Use Gemini models on Anthropic endpoints (`/v1/messages`), or DeepSeek models on OpenAI endpoints (`/v1/chat/completions`). Like locally-hosted OpenRouter.

**Three endpoints supported:**
- `/v1/chat/completions` expect OpenAI format
- `/v1/messages` expect Anthropic format
- `/v1beta/models/{model}:generateContent` expect Gemini format

**Model routing with fallbacks.** Request `{model}` get `["anthropic/claude-3-5-sonnet", "openai/gpt-4o"]` fallback chain.

**Custom endpoints.** Add any provider with custom endpoint: `chutes/model-name` → routes to Chutes API you add in `config.toml`.

**OAuth + API key auth.** Automatic token refresh. Works with Claude Code and Gemini CLI credentials. Did I mention this violates TOS? Good.

**Smart billing fallback.** Use free Gemini quota, automatically switch to API key billing when you hit rate limits. Same for Anthropic subscription → pay-per-use.

## How this was made

**This entire thing was coded by AI.** I didn't write a single fucking line of code myself. Just vibed with Claude and GPT until it worked. Pure dogfooding - using AI to build AI tooling. I heard that every time someone vibe-codes in Rust, a crab dies. So if you have mercy, don't do this.

**The ai-ox library underneath are "mostly" human work though.** So it's not complete AI slop, just the glue code that connects everything together and expose it as axum server.

**Probably full of outdated info and AI hallucinations everywhere.** But it works and was extensively tested by a human. Problem: that human is an ADHD autist and n=1, so your mileage may vary.

**Don't take this too seriously.** It's a weekend project that got out of hand. Works for what I need it to do. If it breaks your setup, that's a you problem.

## Testing Status

**What actually works:**
- ✅ **Claude Code** - Tested extensively. Everything works
- ✅ **Anthropic OAuth** - Tested and working. Token refresh works
- ✅ **Gemini OAuth** - Tested and working. Token refresh works
- ✅ **Direct API calls** - All three endpoints (`/v1/chat/completions`, `/v1/messages`, `/v1beta/models/{model}:generateContent`) tested and working

**What doesn't work:**
- ❌ **OpenAI/Codex OAuth** - Not functional. Requires system prompt mapping between different tool systems, which isn't implemented. Use API keys instead.

**What might work or might not:**
- ❓ **Any other AI editor/tool** - If it speaks OpenAI, Anthropic, or Gemini API format, it should work with API keys.

**Need feedback on:**
- Does it work with your favorite AI editor? Let me know what breaks

## Install

**Build from source** (only method currently):
```bash
git clone https://github.com/ribelo/prism.git
cd prism
cargo build --release
sudo cp target/release/prism /usr/local/bin/
```

## Quick start

1. **Configure providers**: `prism auth anthropic` (or `google` for Gemini). For OpenAI, set `OPENAI_API_KEY` environment variable.
2. **Use with Claude Code**: `prism run claude` (auto-starts server)
3. **Or start manually**: `prism start`

### Direct API usage

**OpenAI format:**
```bash
curl -X POST http://127.0.0.1:3742/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "anthropic/claude-...",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

**Anthropic format:**
```bash
curl -X POST http://127.0.0.1:3742/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openrouter/z-ai/glm-4.5",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

**Gemini format:**
```bash
curl -X POST http://127.0.0.1:3742/v1beta/models/anthropic/claude-3-5-sonnet-20241022:generateContent \
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

### Use with CLI tools
```bash
# Automatically starts server(if needed) and runs Claude Code with Prism backend
prism run claude

# Pass arguments to Claude Code
prism run claude --help

# For Codex CLI (OpenAI) - OAuth not working, but command exists
# Note: OAuth integration is disabled, only works with API keys
prism run codex "write a function"
prism run codex exec "list files"
```

### URL Parameter Support

Prism supports **all parameters** from every provider via URL query parameters. Nothing complex, just passing it by:

```bash
# Standard parameters (work across all providers)
curl -d '{"model": "anthropic/claude-5-epos?temperature=0.8&max_tokens=1500", ...}'

# Provider-specific parameters
curl -d '{"model": "openrouter/openai/gpt-4o?seed=42&frequency_penalty=0.5&top_k=50", ...}'

# Thinking/reasoning parameters

## Anthropic thinking
curl -d '{"model": "anthropic/claude-3-sonnet?think=2000", ...}'  # Anthropic thinking (token budget)

## OpenRouter reasoning (NEW - Updated implementation)
# Basic reasoning with structured configuration
curl -d '{"model": "openrouter/openai/gpt-4o?reasoning=true", ...}'  # Enable basic reasoning
curl -d '{"model": "openrouter/openai/gpt-4o?reasoning=true&effort=high", ...}'  # High depth reasoning
curl -d '{"model": "openrouter/openai/gpt-4o?reasoning=true&effort=medium", ...}'  # Medium depth reasoning
curl -d '{"model": "openrouter/openai/gpt-4o?reasoning=true&effort=low", ...}'  # Low depth reasoning
curl -d '{"model": "openrouter/openai/gpt-4o?reasoning=true&reasoning_max_tokens=2000", ...}'  # Set reasoning token budget
curl -d '{"model": "openrouter/openai/gpt-4o?reasoning=true&reasoning_exclude=true", ...}'  # Hide reasoning output from response

## Gemini thinking
curl -d '{"model": "gemini/gemini-2.0-flash-thinking?thoughts=true&think=1000", ...}'  # Gemini thinking
```

### Model mapping in Claude Code

Claude Code lets you specify models with `/model my-model`. You can either:

**Use full provider paths directly:**
```
/model openrouter/z-ai/glm-4.5:fireworks
/model anthropic/claude-3-5-sonnet?temperature=0.9&max_tokens=4000
```

**Or map custom names in config:**
```toml
[routing.models]
"best-fucking-model" = "openrouter/x-ai/grok-5-mechahitler"
"fast" = "openrouter/z-ai/glm-4.5:nitro"
"free" = "openrouter/santa/free-christmas-model"
"reasoning" = ["openrouter/openai/gpt-4o?reasoning=true&effort=high&reasoning_max_tokens=2000", "anthropic/claude-3-5-sonnet?think=2000", "gemini/gemini-2.0-flash-thinking?thoughts=true&think=1000"]
```

Then just use the alias:
```
/model best-fucking-model
/model fast
/model free
/model reasoning
```

**Works with `claude code` agent files too.** Drop a `.md` file with mapped model names and they'll resolve automatically. Just Works™.

```

## Configuration

Config file: `~/.config/prism/prism.toml`

### Basic setup

```toml
[server]
host = "127.0.0.1"
port = 3742

[routing]

# Model routing with fallback
[routing.models]
"claude-3.5-haiku" = ["openrouter/z-ai/glm-4.5", "openrouter/moonshotai/kimi-k2"]

[providers.anthropic]
type = "anthropic"
endpoint = "https://api.anthropic.com"
api_key_fallback = true
# Falls back to API key billing when OAuth quota is exhausted or rate-limited
fallback_on_errors = [429]

[providers.openai]
type = "openai"
endpoint = "https://api.openai.com"

# Custom endpoint example (anthropic-compatible)
[providers.chutes]
type = "openai"  # Use anthropic-compatible handling
endpoint = "https://api.chutes.ai/v1/foo/bar/baz"
api_key = "${CHUTES_API_KEY}"
```

### API keys

Set standard environment variables:
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-proj-..."
export GEMINI_API_KEY="AIza..."
```

Or use explicit interpolation in config:
```toml
[providers.anthropic]
api_key = "${MY_ANTHROPIC_API_KEY_WITH_SHITTY_NON_STANDARD_NAME}"
```

## Reasoning Parameters

OpenRouter supports advanced reasoning parameters for models that support it. These parameters control the depth and visibility of the model's reasoning process.

### Reasoning Parameter Reference

| Parameter | Values | Description |
|-----------|--------|-------------|
| `reasoning` | `true`, `false` | Enable/disable reasoning capability |
| `effort` | `high`, `medium`, `low` | Control reasoning depth - **does NOT affect provider preference sorting** |
| `reasoning_max_tokens` | any positive integer | Set specific token budget for reasoning process |
| `reasoning_exclude` | `true`, `false` | Hide reasoning output from final response |

### Examples

```bash
# Basic reasoning
curl -d '{"model": "openrouter/openai/gpt-4o?reasoning=true", ...}'

# Deep reasoning with specific token budget
curl -d '{"model": "openrouter/openai/gpt-4o?reasoning=true&effort=high&reasoning_max_tokens=4000", ...}'

# Reasoning without showing the process
curl -d '{"model": "openrouter/openai/gpt-4o?reasoning=true&reasoning_exclude=true", ...}'
```

### Migration from Old Syntax

**Old (deprecated):** `reasoning=true` boolean only
**New:** Structured reasoning configuration

The `effort` parameter now **explicitly controls reasoning depth** instead of incorrectly mapping to provider preferences (throughput vs price sorting). Use separate parameters for provider routing vs reasoning configuration.

## Commands

- `prism start` - Start HTTP server (manual start)
- `prism config` - Validate configuration
- `prism auth anthropic` - Setup Anthropic OAuth
- `prism auth openai` - Setup OpenAI OAuth (currently non-functional)
- `prism auth google` - Setup Gemini OAuth
- `prism diagnose` - Debug OAuth tokens
- `prism run claude [args]` - Auto-start server if needed + run Claude Code with Prism backend
- `prism run codex [args]` - Auto-start server if needed + run Codex CLI with Prism backend

## Features

### Model routing
```bash
# Direct provider routing
curl -d '{"model": "anthropic/claude-5-epos", ...}'

# Custom model mapping
curl -d '{"model": "haiku", ...}'  # Routes to claude-3-haiku

# Fallback chains
curl -d '{"model": "best", ...}'   # Tries sonnet, falls back to gpt-4o
```

### Authentication
- **OAuth**: Automatic token refresh for Anthropic and Gemini, shared with Claude Code/Gemini CLI. OpenAI OAuth is disabled due to system prompt compatibility issues. I mentioned about TOS?
- **API keys**: Environment variables or config file (required for OpenAI)
- **Fallback**: OAuth → API key on rate limits (429 errors) for supported providers

### Error handling
- **Retry policies**: Exponential backoff (3 attempts, 1s-30s delays)
- **Graceful shutdown**: SIGTERM handling
- **Clean logging**: Pretty console output + structured JSON logs to file

## Documentation

- [CONFIG_REFERENCE.md](CONFIG_REFERENCE.md) - Complete configuration options

## Development

**Quality checks**:
```bash
cargo check --all-targets && cargo clippy -- -D warnings && cargo fmt
```

**Tests**: `cargo test`

**Config location**: `~/.config/prism/prism.toml`
**Logs**: `~/.local/share/prism/logs/`

---

## Why Rust?

Because I know it. This is fucking small router with so much traffic that even my interpreter written in college can handle running on potato.

## Why own libraries?

Why own libraries to handle every provider and conversion? Because I like to have control of my system and application. We live in a world where it's faster to write something from scratch, but we shouldn't. Lips curse is nothing compared to what awaits us and this is my brick to that catastrophe. I failed this test.

---

Simple HTTP proxy. Does what it says. There will definitely be dragons here - code has tests but I don't have time to check if this works with every possible app out there. Works for me and Claude Code, that's what matters. If you report an issue, I might fix it when I feel like it. If you make a PR, I might accept it if it doesn't break my shit. Fork it if you want different behavior and save us both some headache.
