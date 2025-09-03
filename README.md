# Setu

Universal AI model router - local HTTP proxy for AI providers.

## Why this exists

**Claude Code has the best TUI and developer experience.** Anthropic clearly loves Claude Code and built a beautiful interface for it. Problem: it only works with Anthropic models.

**Every other router sucks at OAuth.** You get XOR: use Anthropic models with OAuth *or* use other models with a router, but never both. That's stupid.

**Available routers have garbage model configs.** Setu gives you automatic full name resolving with URL-like parameters: `openrouter/z-ai/glm-4.5:fireworks?temperature=0.7&max_tokens=2000&top_k=40`

**Every app can use every model.** OAuth support for Claude, Codex, and Gemini. Simple proxy means any model works through any interface. Want GPT in Claude Code? Done. Want Claude in Codex? Done. Wan't to use CRUSH? Done. No more vendor lock-in bullshit. This definitely violate TOS and you should never use it, but you can. If they block your account, don't come crying.

## What it does

**Transparent request transformation.** Use Gemini models on Anthropic endpoints (`/v1/messages`), or DeepSeek models on OpenAI endpoints (`/v1/chat/completions`). Like locally-hosted OpenRouter.

**Three endpoints supported:**
- `/v1/chat/completions` (OpenAI format) - Routes to OpenRouter providers
- `/v1/messages` (Anthropic format) - Routes to all providers with format conversion
- `/v1beta/models/{model}:generateContent` (Gemini format) - Routes to all providers with format conversion

**Model routing with fallbacks.** Request `{model}` get `["anthropic/claude-3-5-sonnet", "openai/gpt-4o"]` fallback chain.

**Custom endpoints.** Add any provider with custom endpoint: `chutes/model-name` → routes to Chutes API you add in `config.toml`.

**OAuth + API key auth.** Automatic token refresh. Works with Claude Code, Codex CLI, Gemini CLI credentials. Did I mention this violates TOS? Good.

**Smart billing fallback.** Use free Gemini quota, automatically switch to API key billing when you hit rate limits. Same for Anthropic subscription → pay-per-use.

## How this was made

**This entire thing was coded by AI.** I didn't write a single fucking line of code myself. Just vibed with Claude and GPT until it worked.

**Every crab that dies is on me.** I heard that every time someone vibe-codes in Rust, a crab dies. So if you have mercy, don't do this.

**The ai-ox library underneath are mostly human work though.** So it's not complete AI slop, just the glue code that connects everything together and expose it as axum server.

**Probably full of outdated info and AI hallucinations everywhere.** But it works and was extensively tested by a human. Problem: that human is an ADHD autist and n=1, so your mileage may vary.

**Don't take this too seriously.** It's a "weekend" project that got out of hand. Works for what I need it to do. If it breaks your setup, that's a you problem.

## Testing Status

**What actually works:**
- ✅ **Claude Code** - Tested extensively. Everything works
- ✅ **Anthropic OAuth** - Tested and working. Token refresh works
- ✅ **Gemini OAuth** - Tested and working. Token refresh works
- ✅ **Direct API calls** - All three endpoints (`/v1/chat/completions`, `/v1/messages`, `/v1beta/models/{model}:generateContent`) tested and working

**What might work or might not:**
- ❓ **OpenAI OAuth** - Not tested because I don't have an OpenAI account. May work, may not. Try it and let me know
- ❓ **Any other AI editor/tool** - If it speaks OpenAI, Anthropic, or Gemini API format, it should work.

**Need feedback on:**
- Does Codex CLI OAuth work? I implemented it but can't test it
- Does it work with your favorite AI editor? Let me know what breaks

## Install

**Build from source** (only method currently):
```bash
git clone https://github.com/ribelo/setu.git
cd setu
cargo build --release
sudo cp target/release/setu /usr/local/bin/
```

## Quick start

1. **Configure providers**: `setu auth anthropic` (or `openai`, `google`)
2. **Use with Claude Code**: `setu run claude` (auto-starts server)
3. **Or start manually**: `setu start`

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
# Automatically starts server(if needed) and runs Claude Code with Setu backend
setu run claude

# Pass arguments to Claude Code
setu run claude --help

# Same for Codex CLI (OpenAI)
setu run codex "write a function"
setu run codex exec "list files"
```

### URL Parameter Support

Setu supports **all parameters** from every provider via URL query parameters. Nothing complex, just passing it by:

```bash
# Standard parameters (work across all providers)
curl -d '{"model": "anthropic/claude-5-epos?temperature=0.8&max_tokens=1500", ...}'

# Provider-specific parameters
curl -d '{"model": "openrouter/openai/gpt-4o?seed=42&frequency_penalty=0.5&top_k=50", ...}'

# Thinking/reasoning parameters
curl -d '{"model": "anthropic/claude-3-sonnet?think=2000", ...}'  # Anthropic thinking
curl -d '{"model": "openai/gpt-5?effort=high", ...}'      # OpenAI reasoning
curl -d '{"model": "gemini/gemini-2.5-pro?thoughts=true&think=1000", ...}'
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
"reasoning" = ["openai/gpt-5?effort=high", "anthropic/claude-5-epos?think_budget=1e6", "google/gemini-3.5-agi"]
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

Config file: `~/.config/setu/setu.toml`

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

## Commands

- `setu start` - Start HTTP server (manual start)
- `setu config` - Validate configuration
- `setu auth anthropic` - Setup Anthropic OAuth
- `setu auth openai` - Setup OpenAI OAuth
- `setu auth google` - Setup Gemini OAuth
- `setu diagnose` - Debug OAuth tokens
- `setu run claude [args]` - Auto-start server if needed + run Claude Code with Setu backend
- `setu run codex [args]` - Auto-start server if needed + run Codex CLI with Setu backend

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
- **OAuth**: Automatic token refresh, shared with Claude Code/Gemini CLI. I mentioned about TOS?
- **API keys**: Environment variables or config file
- **Fallback**: OAuth → API key on rate limits (429 errors)

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

**Config location**: `~/.config/setu/setu.toml`
**Logs**: `~/.local/share/setu/logs/`

---

## Why Rust?

Because I know it. This is fucking small router with so much traffic that even my interpreter written in college can handle running on potato.

## Why own libraries?

Why own libraries to handle every provider and conversion? Because I like to have control of my system and application. We live in a world where it's faster to write something from scratch, but we shouldn't. Lips curse is nothing compared to what awaits us and this is my brick to that catastrophe. I failed this test.

---

Simple HTTP proxy. Does what it says. There will definitely be dragons here - code has tests but I don't have time to check if this works with every possible app out there. Works for me and Claude Code, that's what matters. If you report an issue, I might fix it when I feel like it. If you make a PR, I might accept it if it doesn't break my shit. Fork it if you want different behavior and save us both some headache.
