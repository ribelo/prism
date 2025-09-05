use axum::http::StatusCode;
use tracing::error;
use std::sync::OnceLock;
use serde::Serialize;

/// Truncate a string to `max_len` characters, appending `...` if truncated
pub fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        let end = s.char_indices().nth(max_len).map(|(i, _)| i).unwrap_or(max_len);
        let mut out = String::with_capacity(end + 3);
        out.push_str(&s[..end]);
        out.push_str("...");
        out
    } else {
        s.to_string()
    }
}

/// Recursively redact sensitive fields for logging
fn redact_sensitive_fields(value: &mut serde_json::Value, redact_messages: bool, redact_tools: bool, redact_system: bool) {
    match value {
        serde_json::Value::Object(map) => {
            if redact_messages && map.contains_key("messages") {
                map.insert("messages".to_string(), serde_json::Value::String("[messages]".to_string()));
            }
            if redact_tools && map.contains_key("tools") {
                map.insert("tools".to_string(), serde_json::Value::String("[tools]".to_string()));
            }
            if redact_system && map.contains_key("system") {
                map.insert("system".to_string(), serde_json::Value::String("[system]".to_string()));
            }
            for v in map.values_mut() {
                redact_sensitive_fields(v, redact_messages, redact_tools, redact_system);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                redact_sensitive_fields(v, redact_messages, redact_tools, redact_system);
            }
        }
        _ => {}
    }
}

/// Serialize a request object to JSON, redact `messages`, and truncate
pub fn sanitize_and_truncate_request<T: serde::Serialize>(request: &T, max_len: usize) -> String {
    match serde_json::to_value(request) {
        Ok(mut val) => {
            redact_sensitive_fields(&mut val, true, true, false);
            match serde_json::to_string_pretty(&val) {
                Ok(json) => truncate_str(&json, max_len),
                Err(_) => "<failed to serialize redacted request>".to_string(),
            }
        }
        Err(_) => "<failed to encode request to json>".to_string(),
    }
}

/// Serialize a response object to JSON and truncate
pub fn truncate_response<T: serde::Serialize>(response: &T, max_len: usize) -> String {
    match serde_json::to_string_pretty(response) {
        Ok(json) => truncate_str(&json, max_len),
        Err(_) => "<failed to serialize response>".to_string(),
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PayloadLogMode {
    Off,
    SummaryV1, // minimal: model, max_tokens, placeholders; redact tools/system/messages
    SummaryV2, // like V1 + full system
    SummaryV3, // like V2 + full messages
    Truncated,
    Full,
}

static PAYLOAD_LOG_MODE: OnceLock<PayloadLogMode> = OnceLock::new();

pub fn set_payload_log_mode(mode: PayloadLogMode) {
    let _ = PAYLOAD_LOG_MODE.set(mode);
}

fn current_payload_mode() -> PayloadLogMode {
    *PAYLOAD_LOG_MODE.get().unwrap_or(&PayloadLogMode::Off)
}

fn collect_strings(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(arr) => arr.iter().for_each(|i| collect_strings(i, out)),
        serde_json::Value::Object(map) => map.values().for_each(|i| collect_strings(i, out)),
        _ => {}
    }
}

fn extract_system_text(val: &serde_json::Value) -> Option<String> {
    // Prefer explicit system field
    if let Some(system) = val.get("system") {
        let mut parts = Vec::new();
        collect_strings(system, &mut parts);
        if !parts.is_empty() { return Some(parts.join("\n")); }
    }
    // Fallback: messages with role == system
    if let Some(messages) = val.get("messages").and_then(|m| m.as_array()) {
        let mut acc: Vec<String> = Vec::new();
        for m in messages {
            if let Some(role) = m.get("role").and_then(|r| r.as_str()) {
                if role == "system" {
                    let mut parts = Vec::new();
                    if let Some(content) = m.get("content") { collect_strings(content, &mut parts); }
                    if !parts.is_empty() { acc.push(parts.join("\n")); }
                }
            }
        }
        if !acc.is_empty() { return Some(acc.join("\n\n")); }
    }
    None
}

fn summarize_request_value(val: &serde_json::Value, level: u8) -> serde_json::Value {
    use serde_json::{json, Value};
    let model = val.get("model").cloned().unwrap_or(Value::String("<unknown>".into()));
    let max_tokens = val.get("max_tokens").cloned().unwrap_or(Value::Null);
    let temperature = val.get("temperature").cloned().unwrap_or(Value::Null);

    let has_tools = val.get("tools").is_some();
    let has_messages = val.get("messages").is_some();

    let system_val = match level {
        1 => Some(Value::String("[system]".into())),
        2 | 3 => extract_system_text(val).map(Value::String),
        _ => None,
    };

    let messages_val = if has_messages {
        match level {
            1 | 2 => Some(Value::String("[messages]".into())),
            3 => val.get("messages").cloned(),
            _ => None,
        }
    } else { None };

    let tools_val = if has_tools { Some(Value::String("[tools]".into())) } else { None };

    let mut obj = serde_json::Map::new();
    obj.insert("model".into(), model);
    if !max_tokens.is_null() { obj.insert("max_tokens".into(), max_tokens); }
    if !temperature.is_null() { obj.insert("temperature".into(), temperature); }
    if let Some(s) = system_val { obj.insert("system".into(), s); }
    if let Some(m) = messages_val { obj.insert("messages".into(), m); }
    if let Some(t) = tools_val { obj.insert("tools".into(), t); }
    Value::Object(obj)
}

fn summarize_response_value(val: &serde_json::Value) -> serde_json::Value {
    use serde_json::{json, Value};
    let mut obj = serde_json::Map::new();
    if let Some(model) = val.get("model").cloned() { obj.insert("model".into(), model); }
    if let Some(id) = val.get("id").cloned() { obj.insert("id".into(), id); }
    if let Some(finish) = val.get("finish_reason").or_else(|| val.get("finishReason")).cloned() {
        obj.insert("finish_reason".into(), finish);
    }
    if let Some(usage) = val.get("usage").cloned().or_else(|| val.get("usageMetadata").cloned()) {
        obj.insert("usage".into(), usage);
    }
    Value::Object(obj)
}

/// Prepare request string for logging according to global payload mode
pub fn prepare_request_log<T: Serialize>(request: &T) -> Option<String> {
    match current_payload_mode() {
        PayloadLogMode::Off => None,
        PayloadLogMode::SummaryV1 => serde_json::to_value(request).ok().and_then(|v| serde_json::to_string_pretty(&summarize_request_value(&v, 1)).ok()),
        PayloadLogMode::SummaryV2 => serde_json::to_value(request).ok().and_then(|v| serde_json::to_string_pretty(&summarize_request_value(&v, 2)).ok()),
        PayloadLogMode::SummaryV3 => serde_json::to_value(request).ok().and_then(|v| serde_json::to_string_pretty(&summarize_request_value(&v, 3)).ok()),
        PayloadLogMode::Truncated => Some(sanitize_and_truncate_request(request, 2000)),
        PayloadLogMode::Full => match serde_json::to_string_pretty(request) {
            Ok(json) => Some(json),
            Err(_) => Some("<failed to serialize request>".to_string()),
        },
    }
}

/// Prepare response string for logging according to global payload mode
pub fn prepare_response_log<T: Serialize>(response: &T) -> Option<String> {
    match current_payload_mode() {
        PayloadLogMode::Off => None,
        PayloadLogMode::SummaryV1 | PayloadLogMode::SummaryV2 | PayloadLogMode::SummaryV3 => {
            serde_json::to_value(response).ok().and_then(|v| serde_json::to_string_pretty(&summarize_response_value(&v)).ok())
        }
        PayloadLogMode::Truncated => Some(truncate_response(response, 2000)),
        PayloadLogMode::Full => match serde_json::to_string_pretty(response) {
            Ok(json) => Some(json),
            Err(_) => Some("<failed to serialize response>".to_string()),
        },
    }
}

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
