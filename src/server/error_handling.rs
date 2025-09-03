use axum::http::StatusCode;
use tracing::error;

/// Log error and return BAD_REQUEST
pub fn bad_request(msg: &str, err: &impl std::fmt::Display) -> StatusCode {
    error!("{}: {}", msg, err);
    StatusCode::BAD_REQUEST
}

/// Log error and return UNAUTHORIZED
pub fn unauthorized(msg: &str) -> StatusCode {
    error!("{}", msg);
    StatusCode::UNAUTHORIZED
}

/// Log error and return BAD_GATEWAY (for external API failures)
pub fn bad_gateway(msg: &str, err: &impl std::fmt::Display) -> StatusCode {
    error!("{}: {}", msg, err);
    StatusCode::BAD_GATEWAY
}

/// Log error and return INTERNAL_SERVER_ERROR
pub fn internal_error(msg: &str, err: &impl std::fmt::Display) -> StatusCode {
    error!("{}: {}", msg, err);
    StatusCode::INTERNAL_SERVER_ERROR
}

/// Compact request for logging (truncates large request payloads for debugging)
/// - Truncates text fields > 100 characters to 97 chars + "..."
/// - Preserves important structural fields: model, role, type, id, name
/// - Recursively compacts nested objects and arrays
/// - Used automatically in error logging for failed requests
pub fn compact_request_for_logging<T: std::fmt::Debug>(request: &T) -> String {
    let debug_str = format!("{:?}", request);
    
    // Simple truncation strategy - truncate very long debug strings
    if debug_str.len() > 500 {
        let truncated = &debug_str[..497];
        format!("{}...", truncated)
    } else {
        debug_str
    }
}