# Prism

Universal AI model router - local HTTP proxy for AI providers.

## What It Is

**Prism is a local HTTP proxy that routes AI requests to any provider.** Use Claude Code with GPT-5. Use Gemini in any OpenAI client. Use whatever model you want, wherever you want.

**Three supported API formats:**
- `/v1/chat/completions` - OpenAI format
- `/v1/messages` - Anthropic format
- `/v1beta/models/{model}:generateContent` - Gemini format

**Transparent request transformation.** Request in any format, route to any provider. Like locally-hosted OpenRouter but with OAuth support.

## Why This Exists

### The Problems

**Claude Code has the best TUI and developer experience.** Anthropic clearly loves Claude and built a beautiful interface for it. Problem: it only works with Anthropic models by default.

**Every other router sucks at OAuth.** You get XOR: use Anthropic models with OAuth *or* use other models with a router, but never both. That's stupid.

**Available routers have garbage model configs.** Sure, you can usually edit the whole response, but that's messy boilerplate hell.

### The Solution

**Every app can use every model.** OAuth support for Claude and Gemini. Simple proxy means any model works through any interface. Want GPT-5 in Claude Code? Done. Want to use Claude Max plan in any OpenAI-compatible client? Done. Want to use whatever the fuck CRUSH is? Done. No more vendor lock-in bullshit.

**Clean URL-like model parameters.** Prism gives you URL-like model parameters that actually fucking work: `openrouter/z-ai/glm-4.5:fireworks?temperature=0.7&max_tokens=2000&top_k=40`. Set all provider parameters through the model string. Map any alias to full model configs. Clean, concise, and gets shit done. No JSON wrestling, no config file hunting - just append your params and go. Worse is better.

**Smart billing fallback.** Use free Gemini quota, automatically switch to API key billing when you hit rate limits. Same for Anthropic subscription → pay-per-use.

### Important Warning

**This definitely violates every provider's TOS.** You should absolutely never use this. When (not if) they detect the proxy and block your account, don't come crying. I warned you. Use at your own risk and don't blame me when your $200/month Claude subscription gets nuked because you wanted to use it with some random AI editor that definitely looks suspicious in their logs.

## Quick Start

### Install

**Build from source**
```bash
git clone https://github.com/ribelo/prism.git
cd prism
cargo build --release
sudo cp target/release/prism /usr/local/bin/
```

### Basic Setup

1. **Configure providers**: `prism auth anthropic` (or `google` for Gemini). For OpenAI, set `OPENAI_API_KEY` environment variable.
2. **Use with Claude Code**: `prism run claude` (auto-starts server or reuse existing one)
3. **Or start manually**: `prism start`

### CLI Commands

- `prism start` - Start HTTP server (manual start)
- `prism config` - Validate configuration
- `prism auth anthropic` - Setup Anthropic OAuth
- `prism auth openai` - Setup OpenAI OAuth (currently non-functional)
- `prism auth google` - Setup Gemini OAuth
- `prism diagnose` - Debug OAuth tokens
- `prism run claude [args]` - Auto-start server if needed + run Claude Code with Prism backend

## Usage Examples

### Direct API Usage

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

### Use With CLI Tools

```bash
# Automatically starts server(if needed) and runs Claude Code with Prism backend
prism run claude

# Pass arguments to Claude Code
prism run claude --help
```

## Model Configuration

### URL Parameter Support

Prism supports **all parameters** from every provider via URL query parameters:

```bash
# Standard parameters (work across all providers)
curl -d '{"model": "anthropic/claude-5-epos?temperature=0.8&max_tokens=1500", ...}'

# Provider-specific parameters
curl -d '{"model": "openrouter/openai/gpt-4o?seed=42&frequency_penalty=0.5&top_k=50", ...}'

# Thinking/reasoning parameters
## Anthropic thinking
curl -d '{"model": "anthropic/claude-4-sonnet?think=2000", ...}'  # Anthropic thinking (token budget)

## OpenRouter reasoning
curl -d '{"model": "openrouter/openai/gpt-5?reasoning=true&effort=high", ...}'  # High depth reasoning

## Gemini thinking
curl -d '{"model": "gemini/gemini-2.5-pro?thoughts=true&think=1000", ...}'  # Gemini thinking
```

### Model Mapping in Claude Code

Claude Code lets you specify models
 with `/model my-model`. You can either:

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
"reasoning" = ["openrouter/openai/gpt-4o?reasoning=true&effort=high&reasoning_max_tokens=2000", "anthropic/claude-5-epos?think=1e6", "gemini/gemini-3.14-ultra?thoughts=true&think=1000"]
```

Then just use the alias:
```
/model best-fucking-model
/model fast
/model free
/model reasoning
```

### Model Directive Comments

**Override model routing directly in system prompts** using HTML comments in the first non-empty line:

```
<!-- openrouter/x-ai/grok-code-fast-1 -->
<!-- gemini/gemini-2.5-flash -->
<!-- openai/gpt-5 -->
```

**How it works:**
- Directive MUST be in the first non-empty line
- Overrides default model routing completely
- Supports all URL parameters: `<!-- openrouter/openai/gpt-4o?temperature=0.7&max_tokens=2000 -->`
- Supports provider preferences: `<!-- openrouter/moonshotai/kimi-k2:groq -->`

**
Practical usage:**
```
---
claude agent ymlfrontmatter you already have
---
<!-- openrouter/x-ai/grok-code-fast-1 -->
You are a fast coder...
```

**Why this exists:** Agents need different models for different tasks. GPT-5 excels at debugging, Kimi owns document writing, GLM-4.5 dominates search tasks. Directive comments let you route to the best model for each job without touching configs. You can keep agents compatible with standard claude code.

**Why not YAML frontmatter:** Claude Code has this weird behavior where it replaces any unknown model in YAML frontmatter with Sonnet, but you can set any model you want from the terminal. So you can `/model whatever-fucking-model` in the CLI just fine, but put that same model in your agent's YAML frontmatter and Claude Code goes "nope, replacing with Sonnet." Directive comments bypass this nonsense entirely - they override the model after Claude Code has already done its weird replacement dance.

### Strategic Model Selection: Beyond Sonnet

Maximize your Claude Max subscription value by using Sonnet as your daily driver, then strategically deploy OpenRouter/Gemini/Openai models through directive comments when you need GPT-5's debugging expertise, Kimi's document mastery, or GLM-4.5's search capabilities - because any models are actually cheaper than Anthropic's api pricing (even GPT-5 costs less than Sonnet), so mixing them in actually saves money while giving you access to specialized capabilities, but only if you are able to use all claude max tokens possible, or you want something better than sonnet without burning your subscription on Opus.

**Example workflow:**
```bash
# Main development work - use your Claude Max tokens
/model anthropic/claude-5-eops

# Hit a nasty bug - switch to the best debugger
<!-- openrouter/openai/gpt-6-agi -->

# Need docs written - use free Kimi
<!-- openrouter/moonshot/kimi-k2 -->

# Need file search - use free GLM
<!-- openrouter/z-ai/glm-4.5 -->
```

## Configuration

Config file: `~/.config/prism/prism.toml`

### Basic Configuration

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

### API Keys

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

## Advanced Features

### Model Routing

```bash
# Direct provider routing
curl -d '{"model": "anthropic/claude-5-epos", ...}'

# Custom model mapping
curl -d '{"model": "haiku", ...}'  # Routes to claude-3-haiku

# Fallback chains
curl -d '{"model": "best", ...}'   # Tries sonnet, falls back to gpt-4o
```

### Authentication Features

- **OAuth**: Automatic token refresh for Anthropic and Gemini, shared with Claude Code/Gemini CLI. OpenAI OAuth is disabled due to system prompt compatibility issues. I mentioned about TOS?
- **API keys**: Environment variables or config file (required for OpenAI)
- **Fallback**: OAuth → API key on rate limits (429 errors) for supported providers

### Error Handling

- **Retry policies**: Exponential backoff (3 attempts, 1s-30s delays)
- **Graceful shutdown**: SIGTERM handling
- **Clean logging**: Pretty console output + structured JSON logs to file

## Testing Status

### What Actually Works
- ✅ **Claude Code** - Tested extensively. Everything works
- ✅ **Anthropic OAuth** - Tested and working. Token refresh works
- ✅ **Gemini OAuth** - Tested and working. Token refresh works
- ✅ **Direct API calls** - All three endpoints (`/v1/chat/completions`, `/v1/messages`, `/v1beta/models/{model}:generateContent`) tested and working

### What Doesn't Work
- ❌ **OpenAI/Codex OAuth** - Implemented but not functional. Oauth requires Codex system prompt, so also mapping between different tool systems, which isn't implemented. Use API keys instead.

### What Might Work or Might Not
- ❓ **Any other AI editor/tool** - If it speaks OpenAI, Anthropic, or Gemini API format, it should work.

### Need Feedback On
- Does it work with your favorite AI editor? Let me know what breaks

## Development

### Quality Checks
```bash
cargo check --all-targets && cargo clippy -- -D warnings && cargo fmt
```

### Tests
```bash
cargo test
```

### File Locations
- **Config**: `~/.config/prism/prism.toml`
- **Logs**: `~/.local/share/prism/logs/`

### Documentation
- [CONFIG_REFERENCE.md](CONFIG_REFERENCE.md) - Complete configuration options

## About This Project

### How This Was Made

**This entire thing was coded by AI.** I didn't write a single fucking line of code myself. Just vibed with Claude and GPT until it worked. Pure dogfooding - using AI to build AI tooling. I heard that every time someone vibe-codes in Rust, a crab dies and i can confirm. So if you have mercy, don't ever do this. Feris will thank you.

**The ai-ox library underneath are "mostly" human work though.** So it's not complete AI slop, just the glue code that connects everything together and expose it as axum server.

**Probably full of outdated info and AI hallucinations everywhere.** But it works and was extensively tested by a human. Problem: that human is an ADHD autist and n=1, so your mileage may vary.

**Don't take this too seriously.** It's a weekend project that got out of hand. Works for what I need it to do. If it breaks your setup, that's a you problem.

### Why Rust?

Because I know it. This is fucking small router with so much traffic that even my interpreter written in college can handle running on potato. It's not another "rewrite it in rust" case. Every available claude router is already fast enough.

### Why Own Libraries?

Why own libraries to handle every provider and conversion? Because I like to have control of my important hobby application. We live in a world where it's faster to write something from scratch, but we shouldn't. Lips curse is nothing compared to what awaits us and this is my brick to that catastrophe. I failed this test. Sorry.

---

Simple HTTP proxy. Does what it says. There will definitely be dragons here - code has tests but I don't have time to check if this works with every possible app out there. Works for me and Claude Code, that's what matters. If you report an issue, I might fix it when I feel like it. If you make a PR, I might accept it if it doesn't break my shit. Fork it if you want different behavior and save us both some headache.
