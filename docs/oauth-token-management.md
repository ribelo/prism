# OAuth Token Management Logic

## Overview

Setu implements comprehensive OAuth token management with the following priorities:

1. **Primary sources**: CLI tools (Claude CLI, Gemini CLI) - these have the freshest tokens
2. **Secondary source**: Setu configuration - user-managed tokens  
3. **No fallback**: If any OAuth tokens are expired, server startup fails with clear instructions

## Token Discovery Process

### 1. Token Source Discovery

For each provider (Anthropic, Gemini), setu checks two sources:

#### Anthropic OAuth Sources
- **Claude CLI**: `~/.config/claude/oauth.json` - Claude Code OAuth tokens
- **Setu Config**: `~/.config/setu/setu.toml` - User-configured OAuth tokens

#### Gemini OAuth Sources  
- **Gemini CLI**: `~/.gemini/oauth_creds.json` - Gemini CLI OAuth tokens
- **Setu Config**: `~/.config/setu/setu.toml` - User-configured OAuth tokens

### 2. Token Comparison Logic

When both sources have tokens, setu chooses the **newer token**:

```rust
fn choose_best_token_source(setu_info: &TokenInfo, cli_info: &TokenInfo) -> TokenDecision {
    match (setu_info.is_available, cli_info.is_available) {
        (true, true) => {
            // Both available - choose newer
            if cli_info.expires_at > setu_info.expires_at {
                cli_info.to_decision("CLI has newer tokens")
            } else {
                setu_info.to_decision("Setu config has newer tokens") 
            }
        }
        (true, false) => setu_info.to_decision("Only setu config available"),
        (false, true) => cli_info.to_decision("Only CLI available"),
        (false, false) => TokenDecision::none("No tokens found from any source"),
    }
}
```

### 3. Token Validation Rules

**Critical**: Setu enforces strict token validation at startup:

- ✅ **Valid tokens**: Server starts normally with OAuth authentication
- ❌ **Expired tokens**: Server startup **FAILS** with clear instructions  
- ❌ **No tokens**: Server startup **FAILS** with setup instructions
- ❌ **Invalid tokens**: Server startup **FAILS** with refresh instructions

**No API key fallback** - OAuth providers must have valid OAuth tokens.

## Implementation Details

### Startup Flow

1. **Load Configuration** - Read setu config from `~/.config/setu/setu.toml`
2. **Discover Tokens** - Check CLI tool credentials and setu config
3. **Compare & Choose** - Select the newest valid tokens
4. **Validate Expiration** - Ensure chosen tokens are not expired
5. **Cache Decision** - Store authentication method for request handling
6. **Start Server** - Only if all OAuth tokens are valid

### Error Cases & User Instructions

#### Expired Anthropic Tokens
```
ERROR setu::auth: Found expired Claude CLI OAuth tokens - startup will fail
Error: Anthropic OAuth tokens are expired!

Token Status:
  • Claude CLI: expired
  • Setu config: not configured

To fix this issue:
  1. Run: claude auth refresh    (refresh Claude CLI tokens)
  2. Run: setu auth anthropic   (get fresh setu tokens)

Setu will automatically use whichever tokens are newer.
```

#### Expired Gemini Tokens  
```
ERROR setu::auth: Found expired Gemini CLI OAuth tokens - startup will fail
Error: Gemini OAuth tokens are expired!

Token Status:
  • Gemini CLI: expired
  • Setu config: not configured

To fix this issue:
  1. Try: gemini -p "test"      (may trigger automatic refresh)
  2. Run: setu auth google      (copy CLI tokens to setu config)

If Gemini CLI refresh fails, you may need to re-authenticate with Google.
```

#### Mixed Providers
Setu validates **each provider independently**:
- Anthropic with expired tokens → startup fails (even if Gemini is valid)
- Gemini with expired tokens → startup fails (even if Anthropic is valid)
- Both must be valid for successful startup

### Token Refresh Strategy

**Anthropic**: Claude CLI tokens can often be refreshed automatically
**Gemini**: Gemini CLI may attempt automatic refresh, but often requires re-authentication

### Configuration Updates

When CLI tokens are newer than setu config tokens:
1. Setu automatically updates its configuration
2. The newer tokens are written to `~/.config/setu/setu.toml`
3. File permissions are set to `600` (owner read/write only)

## Logging

OAuth token analysis is logged at startup:

```
INFO setu::auth: Anthropic Token Analysis:
INFO setu::auth:   Claude CLI: valid, expires in 5h 23m (subscription billing)
INFO setu::auth:   Setu config: expired 2h ago (subscription billing)
INFO setu::auth: Anthropic Decision: Using Claude CLI (newer tokens)

INFO setu::auth: Gemini Token Analysis:  
INFO setu::auth:   Gemini CLI: expired 22h ago (pay-per-use billing)
INFO setu::auth:   Setu config: not found
ERROR setu::auth: Gemini Decision: Startup failed - no valid tokens
```

## Security Considerations

- OAuth tokens are never logged in full (only partial prefixes)
- Configuration files have restrictive permissions (600)
- Token expiration is checked proactively (10-minute buffer)
- No tokens are transmitted in logs or error messages