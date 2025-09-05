use crate::server::error_handling;
use anthropic_ox::ChatRequest;
use axum::body::Body;
use axum::http::StatusCode;

/// Parse Anthropic message request body
pub async fn parse_chat_request(body: Body) -> Result<ChatRequest, StatusCode> {
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to read request body",
                &e,
            ));
        }
    };

    let body_text = match std::str::from_utf8(&body_bytes) {
        Ok(text) => text,
        Err(e) => {
            return Err(error_handling::bad_request(
                "Invalid UTF-8 in request body",
                &e,
            ));
        }
    };

    match serde_json::from_str::<ChatRequest>(body_text) {
        Ok(request) => Ok(request),
        Err(e) => Err(error_handling::bad_request(
            "Invalid JSON in request body",
            &e,
        )),
    }
}

/// Parse OpenAI chat request body (proper OpenAI format with string content)
pub async fn parse_openai_chat_request(
    body: Body,
) -> Result<openai_ox::request::ChatRequest, StatusCode> {
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to read request body",
                &e,
            ));
        }
    };

    let body_text = match std::str::from_utf8(&body_bytes) {
        Ok(text) => text,
        Err(e) => {
            return Err(error_handling::bad_request(
                "Invalid UTF-8 in request body",
                &e,
            ));
        }
    };

    match serde_json::from_str::<openai_ox::request::ChatRequest>(body_text) {
        Ok(request) => Ok(request),
        Err(e) => Err(error_handling::bad_request(
            "Invalid JSON in request body",
            &e,
        )),
    }
}

/// Parse Gemini generate content request body
pub async fn parse_gemini_request(body: Body) -> Result<serde_json::Value, StatusCode> {
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return Err(error_handling::internal_error(
                "Failed to read request body",
                &e,
            ));
        }
    };

    let body_text = match std::str::from_utf8(&body_bytes) {
        Ok(text) => text,
        Err(e) => {
            return Err(error_handling::bad_request(
                "Invalid UTF-8 in request body",
                &e,
            ));
        }
    };

    // Parse as JSON value first - we'll construct the proper struct in the handler
    match serde_json::from_str::<serde_json::Value>(body_text) {
        Ok(request) => Ok(request),
        Err(e) => Err(error_handling::bad_request(
            "Invalid JSON in request body",
            &e,
        )),
    }
}
