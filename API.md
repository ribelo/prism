# Setu API Documentation

This document provides details on how to interact with the Setu API. Setu acts as a proxy and router, so you can interact with it using a single, unified API, and it will intelligently route your requests to the appropriate backend provider.

## Base URL

All API endpoints are prefixed with the base URL where Setu is running. By default, this is:

```
http://127.0.0.1:3742
```

## Authentication

Setu handles authentication with the downstream AI providers. How you authenticate with Setu depends on the provider you are targeting.

### OpenAI-Compatible Endpoints

For the OpenAI-compatible endpoints (`/v1/chat/completions` and `/v1/models`), Setu's behavior depends on the model you specify in your request and the routing configuration.

-   **If routing to OpenRouter**: You must provide your OpenRouter API key in the `Authorization` header as a Bearer token. Setu will then use its own configured credentials for the downstream provider.
    ```
    Authorization: Bearer <YOUR_OPENROUTER_API_KEY>
    ```

-   **If routing to Anthropic (with API key)**: You must provide your Anthropic API key in the `x-api-key` header.
    ```
    x-api-key: <YOUR_ANTHROPIC_API_KEY>
    ```
-   **If routing to Anthropic (with OAuth)**: If you have configured Anthropic authentication using `setu auth anthropic`, Setu will automatically handle adding the OAuth access token to the request. You do not need to provide an `Authorization` header.

-   **If routing to Gemini (with API key)**: Setu will use the `GEMINI_API_KEY` environment variable on the server where Setu is running.

-   **If routing to Gemini (with OAuth)**: If you have configured Gemini authentication using `setu auth google`, Setu will automatically handle the authentication.

### Provider-Specific Endpoints

When calling a provider-specific endpoint like `/anthropic/v1/messages`, you should authenticate as you would with the provider's native API. Setu will pass through the authentication headers.

---

## Endpoints

### Chat Completions

This is the primary endpoint for interacting with AI models through Setu. It is compatible with the OpenAI Chat Completions API format, but it uses the Anthropic model for the request body.

-   **Endpoint**: `POST /v1/chat/completions`
-   **Method**: `POST`
-   **Content-Type**: `application/json`

#### Request Body

The request body should be a JSON object compatible with the Anthropic Messages API.

```json
{
  "model": "anthropic/claude-3-sonnet-20240229",
  "messages": [
    {
      "role": "user",
      "content": "Hello, world!"
    }
  ],
  "max_tokens": 1024,
  "stream": false
}
```

**Key Fields:**

-   `model` (string, required): The name of the model you want to use, in the format `provider/model-name`. Setu uses this to route the request.
-   `messages` (array, required): The conversation history.
-   `stream` (boolean, optional): If `true`, the response will be a server-sent event stream. Defaults to `false`.

#### Response (Non-Streaming)

If `stream` is `false`, the response will be a JSON object compatible with the Anthropic Messages API.

```json
{
  "id": "msg_01...",
  "type": "message",
  "role": "assistant",
  "model": "claude-3-sonnet-20240229",
  "content": [
    {
      "type": "text",
      "text": "Hello! It's nice to meet you."
    }
  ],
  "stop_reason": "end_turn",
  "stop_sequence": null,
  "usage": {
    "input_tokens": 8,
    "output_tokens": 13
  }
}
```

#### Response (Streaming)

If `stream` is `true`, the response will be a `text/event-stream` with server-sent events.

```
data: {"type": "message_start", "message": {"id": "msg_123...", ...}}

data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Hello!"}}

...

data: {"type": "message_delta", "delta": {"stop_reason": "end_turn", ...}}

data: [DONE]
```

#### `curl` Example (Non-Streaming)

```bash
curl -X POST http://127.0.0.1:3742/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "anthropic/claude-3-sonnet-20240229",
    "messages": [{"role": "user", "content": "Tell me a joke."}],
    "max_tokens": 100
  }'
```

#### `curl` Example (Streaming)

```bash
curl -N -X POST http://127.0.0.1:3742/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openrouter/openai/gpt-4o",
    "messages": [{"role": "user", "content": "Write a short story."}],
    "stream": true
  }'
```

### Models

This endpoint provides a list of available models. In the current version of Setu, it returns a mock response for compatibility purposes.

-   **Endpoint**: `GET /v1/models`
-   **Method**: `GET`

#### Response

```json
{
  "object": "list",
  "data": [
    {
      "id": "setu-noop",
      "object": "model",
      "created": 1677649963,
      "owned_by": "setu"
    }
  ]
}
```

---

## Error Handling

Setu returns standard HTTP status codes to indicate the success or failure of an API request.

| Status Code | Meaning | Description |
| --- | --- | --- |
| `200 OK` | Success | The request was successful. |
| `400 Bad Request` | Client Error | The request was malformed or invalid. Check the response body for details. |
| `401 Unauthorized` | Authentication Error | Your API key or OAuth token is invalid or missing. |
| `404 Not Found` | Not Found | The requested endpoint does not exist. |
| `500 Internal Server Error` | Server Error | Something went wrong on Setu's end. |
| `502 Bad Gateway` | Upstream Error | Setu received an error from the downstream provider (e.g., Anthropic, OpenRouter). |
| `501 Not Implemented` | Not Implemented | The requested functionality is not implemented. |

Error responses will include a JSON body with more details:

```json
{
  "error": {
    "message": "Invalid authorization header format",
    "type": "invalid_request_error",
    "param": null,
    "code": null
  }
}
```

## Rate Limiting

Setu itself does not currently implement rate limiting. However, you are still subject to the rate limits of the downstream providers you are using. Please consult the documentation for each provider for details on their rate limits.
