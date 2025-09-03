# Setu Configuration Guide

This guide provides a comprehensive overview of how to configure Setu using the `setu.toml` file.

## Configuration File Location

Setu follows the XDG Base Directory Specification. The configuration file is located at:

-   **Linux/macOS**: `~/.config/setu/setu.toml`
-   **Windows**: `C:\Users\<YourUser>\AppData\Roaming\setu\config\setu.toml`

If the configuration file does not exist, Setu will create a default one when it runs for the first time.

## Environment Variables

All configuration options can be overridden using environment variables. The variables must be prefixed with `SETU_`, and use `_` as a separator for nested keys.

For example, to override the server port, you can set the `SETU_SERVER_PORT` environment variable:

```bash
export SETU_SERVER_PORT=8080
setu start
```

## Top-Level Structure

The `setu.toml` file has three main sections: `server`, `routing`, and `providers`.

```toml
# Main server configuration
[server]
host = "127.0.0.1"
port = 3742

# Routing strategy and rules
[routing]
default_provider = "openrouter"
strategy = "composite"

# Provider-specific configurations
[providers]
  [providers.openrouter]
  type = "openrouter"
  endpoint = "https://openrouter.ai/api/v1"
  models = ["openai/gpt-4o", "google/gemini-flash-1.5"]
  # auth is configured separately, see below
```

---

## `[server]` Section

This section configures the HTTP proxy server.

| Key               | Type    | Default         | Environment Variable            | Description                                                                                             |
| ----------------- | ------- | --------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `host`            | String  | `"127.0.0.1"`   | `SETU_SERVER_HOST`              | The IP address the server will bind to.                                                                 |
| `port`            | Integer | `3742`          | `SETU_SERVER_PORT`              | The port the server will listen on.                                                                     |
| `log_level`       | String  | `"info"`        | `SETU_SERVER_LOG_LEVEL`         | The logging level. Can be `trace`, `debug`, `info`, `warn`, or `error`.                                   |
| `log_file_enabled`| Boolean | `true`          | `SETU_SERVER_LOG_FILE_ENABLED`  | Enables or disables logging to a file.                                                                  |
| `log_rotation`    | String  | `"daily"`       | `SETU_SERVER_LOG_ROTATION`      | The rotation strategy for log files. Can be `minutely`, `hourly`, `daily`, or `never`.                  |
| `log_dir`         | String  | (data dir)/logs | `SETU_SERVER_LOG_DIR`           | The directory to store log files in. Defaults to the XDG data directory.                                |
| `log_file_prefix` | String  | `"setu"`        | `SETU_SERVER_LOG_FILE_PREFIX`   | The prefix for log file names.                                                                          |

### Example

```toml
[server]
host = "0.0.0.0"
port = 8000
log_level = "debug"
log_file_enabled = true
log_rotation = "hourly"
```

---

## `[routing]` Section

This section controls how incoming requests are routed to different providers.

| Key                   | Type           | Default       | Environment Variable                | Description                                                              |
| --------------------- | -------------- | ------------- | ----------------------------------- | ------------------------------------------------------------------------ |
| `default_provider`    | String         | `"openrouter"`| `SETU_ROUTING_DEFAULT_PROVIDER`     | The provider to use if no other routing rule matches.                    |
| `strategy`            | String         | `"composite"` | `SETU_ROUTING_STRATEGY`             | The routing strategy. Can be `composite`, `model`, or `provider`.        |
| `enable_fallback`     | Boolean        | `true`        | `SETU_ROUTING_ENABLE_FALLBACK`      | If `true`, Setu will try other providers if the primary one fails.       |
| `min_confidence`      | Float          | `0.0`         | `SETU_ROUTING_MIN_CONFIDENCE`       | The minimum confidence score required for a routing decision (future use). |
| `rules`               | Table          | `{}`          | (not settable via env)              | A map of model name patterns to provider names.                          |
| `provider_priorities` | Array of String| `[]`          | (not settable via env)              | A list of provider names in order of priority for fallback routing.      |
| `provider_aliases`    | Table          | `{}`          | (not settable via env)              | A map of provider aliases to provider names.                             |

### Example

```toml
[routing]
default_provider = "anthropic"
strategy = "model"
enable_fallback = true
provider_priorities = ["anthropic", "openrouter", "gemini"]

[routing.rules]
"openai/*" = "openrouter"
"google/*" = "gemini"
"claude-*" = "anthropic"

[routing.provider_aliases]
"claude" = "anthropic"
"google" = "gemini"
```

---

## `[providers]` Section

This section defines the configuration for each AI provider you want to use. Each provider has its own sub-table, e.g., `[providers.anthropic]`.

| Key        | Type          | Required | Description                                                                   |
| ---------- | ------------- | -------- | ----------------------------------------------------------------------------- |
| `type`     | String        | Yes      | The type of the provider. Must match a supported provider type in Setu.       |
| `endpoint` | String        | Yes      | The base URL for the provider's API.                                          |
| `models`   | Array of String | Yes      | A list of model names that this provider supports.                            |
| `auth`     | Table         | No       | Authentication configuration for this provider. See the `[auth]` section below. |

### Example

```toml
[providers]

  # Configuration for Anthropic
  [providers.anthropic]
  type = "anthropic"
  endpoint = "https://api.anthropic.com/v1"
  models = ["claude-3-opus-20240229", "claude-3-sonnet-20240229"]

  # Configuration for OpenRouter
  [providers.openrouter]
  type = "openrouter"
  endpoint = "https://openrouter.ai/api/v1"
  models = ["openai/gpt-4o", "google/gemini-pro-1.5"]

  # Configuration for Google Gemini
  [providers.gemini]
  type = "gemini"
  endpoint = "https://generativelanguage.googleapis.com/v1beta"
  models = ["models/gemini-1.5-pro-latest"]
```

---

## `[auth]` Section

Authentication is configured on a per-provider basis within the `[providers.provider_name.auth]` table. Setu primarily uses OAuth 2.0 for providers that support it, and manages tokens for you.

You typically do not need to edit this section manually. Instead, use the `setu auth` command to perform the OAuth flow, which will automatically populate these fields.

| Key                   | Type   | Description                                                                                                   |
| --------------------- | ------ | ------------------------------------------------------------------------------------------------------------- |
| `oauth_access_token`  | String | The OAuth access token. This is short-lived and automatically refreshed.                                      |
| `oauth_refresh_token` | String | The OAuth refresh token. This is long-lived and used to get new access tokens. **Treat this like a password.** |
| `oauth_expires`       | Integer| The timestamp (in milliseconds) when the access token expires.                                                |
| `project_id`          | String | The Google Cloud project ID, required for Gemini authentication.                                              |

For providers that use simple API keys (like OpenRouter), you should set the corresponding environment variable (e.g., `OPENROUTER_API_KEY`).

### Security Best Practices

-   **Do not commit `setu.toml` to version control**, especially if it contains OAuth refresh tokens or other secrets.
-   **Use environment variables for API keys** whenever possible, rather than hardcoding them in the configuration file.
-   **Keep your OAuth refresh tokens secure.** If a refresh token is compromised, anyone can generate access tokens for your account.
-   **Ensure the permissions on your `setu.toml` file are restrictive.** Set it to be readable only by your user account (e.g., `chmod 600 ~/.config/setu/setu.toml`).
-   Run `setu auth <provider>` again to refresh your tokens if you suspect they have been compromised.
