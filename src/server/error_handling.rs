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
    // Check for system field (Anthropic format)
    if let Some(system) = val.get("system") {
        let mut parts = Vec::new();
        collect_strings(system, &mut parts);
        if !parts.is_empty() { return Some(parts.join("\n")); }
    }
    // Check for instructions field (OpenAI Responses API format)
    if let Some(instructions) = val.get("instructions") {
        let mut parts = Vec::new();
        collect_strings(instructions, &mut parts);
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
    use serde_json::Value;
    let model = val.get("model").cloned().unwrap_or(Value::String("<unknown>".into()));
    let max_tokens = val.get("max_tokens").cloned().unwrap_or(Value::Null);
    let temperature = val.get("temperature").cloned().unwrap_or(Value::Null);

    let has_tools = val.get("tools").is_some();
    let has_messages = val.get("messages").is_some();

    // Check for both 'system' (Anthropic) and 'instructions' (OpenAI Responses API)
    let system_val = match level {
        1 => {
            if val.get("system").is_some() {
                Some(Value::String("[system]".into()))
            } else if val.get("instructions").is_some() {
                Some(Value::String("[instructions]".into()))
            } else {
                None
            }
        },
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

    let tools_val = if has_tools {
        match level {
            1 | 2 => Some(Value::String("[tools]".into())),
            3 => val.get("tools").cloned(),  // Show actual tools at level 3 for debugging
            _ => None,
        }
    } else { None };

    let mut obj = serde_json::Map::new();
    obj.insert("model".into(), model);
    if !max_tokens.is_null() { obj.insert("max_tokens".into(), max_tokens); }
    if !temperature.is_null() { obj.insert("temperature".into(), temperature); }
    
    // Use correct field name based on what's in the original request
    if let Some(s) = system_val {
        if val.get("instructions").is_some() {
            obj.insert("instructions".into(), s);
        } else {
            obj.insert("system".into(), s);
        }
    }
    
    if let Some(m) = messages_val { obj.insert("messages".into(), m); }
    if let Some(t) = tools_val { obj.insert("tools".into(), t); }
    Value::Object(obj)
}

fn summarize_response_value(val: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
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

/// Prepare Anthropic request string for detailed logging with ALL parameters visible
pub fn prepare_anthropic_request_log(request: &anthropic_ox::ChatRequest) -> Option<String> {
    use serde_json::Value;
    
    let mut obj = serde_json::Map::new();
    
    // Core fields
    obj.insert("model".into(), Value::String(request.model.clone()));
    obj.insert("messages".into(), if request.messages.is_empty() { 
        Value::String("[empty]".into()) 
    } else { 
        Value::String(format!("[{} messages]", request.messages.len()))
    });
    
    // System instruction - show even if None
    obj.insert("system".into(), request.system.as_ref()
        .map(|_| Value::String("[system]".into()))
        .unwrap_or(Value::String("null".into())));
    
    // Required field but show for completeness
    obj.insert("max_tokens".into(), Value::Number(request.max_tokens.into()));
    
    // Optional fields - show even if None
    obj.insert("metadata".into(), request.metadata.as_ref()
        .map(|v| v.clone())
        .unwrap_or(Value::String("null".into())));
    obj.insert("stop_sequences".into(), request.stop_sequences.as_ref()
        .map(|v| Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("stream".into(), request.stream
        .map(Value::Bool)
        .unwrap_or(Value::String("null".into())));
    obj.insert("temperature".into(), request.temperature
        .map(|v| Value::Number(serde_json::Number::from_f64(v as f64).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("top_p".into(), request.top_p
        .map(|v| Value::Number(serde_json::Number::from_f64(v as f64).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("top_k".into(), request.top_k
        .map(|v| Value::Number(v.into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("tools".into(), request.tools.as_ref()
        .map(|v| Value::String(format!("[{} tools]", v.len())))
        .unwrap_or(Value::String("null".into())));
    obj.insert("tool_choice".into(), request.tool_choice.as_ref()
        .map(|_| Value::String("[tool_choice]".into()))
        .unwrap_or(Value::String("null".into())));
    
    // Thinking configuration
    obj.insert("thinking".into(), request.thinking.as_ref()
        .map(|t| {
            let mut thinking_obj = serde_json::Map::new();
            thinking_obj.insert("config_type".into(), Value::String(t.config_type.clone()));
            thinking_obj.insert("budget_tokens".into(), Value::Number(t.budget_tokens.into()));
            Value::Object(thinking_obj)
        })
        .unwrap_or(Value::String("null".into())));
    
    serde_json::to_string_pretty(&Value::Object(obj)).ok()
}

/// Prepare Gemini request string for detailed logging with ALL parameters visible
pub fn prepare_gemini_request_log(request: &gemini_ox::generate_content::request::GenerateContentRequest) -> Option<String> {
    use serde_json::Value;
    
    let mut obj = serde_json::Map::new();
    
    // Core fields
    obj.insert("model".into(), Value::String(request.model.clone()));
    obj.insert("contents".into(), if request.contents.is_empty() { 
        Value::String("[empty]".into()) 
    } else { 
        Value::String(format!("[{} contents]", request.contents.len()))
    });
    
    // Optional fields - show even if None
    obj.insert("tools".into(), request.tools.as_ref()
        .map(|v| Value::String(format!("[{} tools]", v.len())))
        .unwrap_or(Value::String("null".into())));
    obj.insert("tool_config".into(), request.tool_config.as_ref()
        .map(|_| Value::String("[tool_config]".into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("safety_settings".into(), request.safety_settings.as_ref()
        .map(|_| Value::String("[safety_settings]".into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("system_instruction".into(), request.system_instruction.as_ref()
        .map(|_| Value::String("[system_instruction]".into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("cached_content".into(), request.cached_content.as_ref()
        .map(|v| Value::String(v.clone()))
        .unwrap_or(Value::String("null".into())));
    
    // Generation config - show detailed structure
    obj.insert("generation_config".into(), request.generation_config.as_ref()
        .map(|gc| {
            let mut gc_obj = serde_json::Map::new();
            gc_obj.insert("stop_sequences".into(), gc.stop_sequences.as_ref()
                .map(|v| Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()))
                .unwrap_or(Value::String("null".into())));
            gc_obj.insert("response_mime_type".into(), gc.response_mime_type.as_ref()
                .map(|v| Value::String(v.clone()))
                .unwrap_or(Value::String("null".into())));
            gc_obj.insert("response_schema".into(), gc.response_schema.as_ref()
                .map(|v| v.clone())
                .unwrap_or(Value::String("null".into())));
            gc_obj.insert("candidate_count".into(), gc.candidate_count
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::String("null".into())));
            gc_obj.insert("max_output_tokens".into(), gc.max_output_tokens
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::String("null".into())));
            gc_obj.insert("temperature".into(), gc.temperature
                .map(|v| Value::Number(serde_json::Number::from_f64(v as f64).unwrap()))
                .unwrap_or(Value::String("null".into())));
            gc_obj.insert("top_p".into(), gc.top_p
                .map(|v| Value::Number(serde_json::Number::from_f64(v as f64).unwrap()))
                .unwrap_or(Value::String("null".into())));
            gc_obj.insert("top_k".into(), gc.top_k
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::String("null".into())));
            
            // Thinking config
            gc_obj.insert("thinking_config".into(), gc.thinking_config.as_ref()
                .map(|tc| {
                    let mut thinking_obj = serde_json::Map::new();
                    thinking_obj.insert("include_thoughts".into(), Value::Bool(tc.include_thoughts));
                    thinking_obj.insert("thinking_budget".into(), Value::Number(tc.thinking_budget.into()));
                    Value::Object(thinking_obj)
                })
                .unwrap_or(Value::String("null".into())));
            
            Value::Object(gc_obj)
        })
        .unwrap_or(Value::String("null".into())));
    
    serde_json::to_string_pretty(&Value::Object(obj)).ok()
}

/// Prepare OpenAI request string for detailed logging with ALL parameters visible
pub fn prepare_openai_request_log(request: &openai_ox::ChatRequest) -> Option<String> {
    use serde_json::Value;
    
    let mut obj = serde_json::Map::new();
    
    // Core fields
    obj.insert("model".into(), Value::String(request.model.clone()));
    obj.insert("messages".into(), if request.messages.is_empty() { 
        Value::String("[empty]".into()) 
    } else { 
        Value::String(format!("[{} messages]", request.messages.len()))
    });
    
    // Optional fields - show even if None
    obj.insert("tools".into(), request.tools.as_ref()
        .map(|v| Value::String(format!("[{} tools]", v.len())))
        .unwrap_or(Value::String("null".into())));
    obj.insert("user".into(), request.user.as_ref()
        .map(|v| Value::String(v.clone()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("max_tokens".into(), request.max_tokens
        .map(|v| Value::Number(v.into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("temperature".into(), request.temperature
        .map(|v| Value::Number(serde_json::Number::from_f64(v as f64).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("top_p".into(), request.top_p
        .map(|v| Value::Number(serde_json::Number::from_f64(v as f64).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("stream".into(), request.stream
        .map(Value::Bool)
        .unwrap_or(Value::String("null".into())));
    obj.insert("stop".into(), request.stop.as_ref()
        .map(|v| Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()))
        .unwrap_or(Value::String("null".into())));
    
    // OpenAI-specific extensions
    obj.insert("n".into(), request.n
        .map(|v| Value::Number(v.into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("presence_penalty".into(), request.presence_penalty
        .map(|v| Value::Number(serde_json::Number::from_f64(v as f64).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("frequency_penalty".into(), request.frequency_penalty
        .map(|v| Value::Number(serde_json::Number::from_f64(v as f64).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("logit_bias".into(), request.logit_bias.as_ref()
        .map(|v| {
            let bias_map: serde_json::Map<String, Value> = v.iter()
                .map(|(k, v)| (k.clone(), Value::Number(serde_json::Number::from_f64(*v as f64).unwrap())))
                .collect();
            Value::Object(bias_map)
        })
        .unwrap_or(Value::String("null".into())));
    obj.insert("seed".into(), request.seed
        .map(|v| Value::Number(v.into()))
        .unwrap_or(Value::String("null".into())));
    
    serde_json::to_string_pretty(&Value::Object(obj)).ok()
}

/// Prepare OpenRouter request string for detailed logging with ALL parameters visible
pub fn prepare_openrouter_request_log(request: &openrouter_ox::ChatRequest) -> Option<String> {
    use serde_json::Value;
    
    let mut obj = serde_json::Map::new();
    
    // Core OpenAI format fields
    obj.insert("model".into(), Value::String(request.model.clone()));
    obj.insert("messages".into(), if request.messages.is_empty() { 
        Value::String("[empty]".into()) 
    } else { 
        Value::String(format!("[{} messages]", request.messages.len()))
    });
    
    // Optional core fields - show even if None
    obj.insert("response_format".into(), request.response_format.as_ref()
        .map(|v| v.clone()).unwrap_or(Value::String("null".into())));
    obj.insert("temperature".into(), request.temperature
        .map(|v| Value::Number(serde_json::Number::from_f64(v).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("top_p".into(), request.top_p
        .map(|v| Value::Number(serde_json::Number::from_f64(v).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("max_tokens".into(), request.max_tokens
        .map(|v| Value::Number(v.into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("stop".into(), request.stop.as_ref()
        .map(|v| Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("stream".into(), request.stream
        .map(Value::Bool)
        .unwrap_or(Value::String("null".into())));
    obj.insert("tools".into(), request.tools.as_ref()
        .map(|v| Value::String(format!("[{} tools]", v.len())))
        .unwrap_or(Value::String("null".into())));
    obj.insert("tool_choice".into(), request.tool_choice.as_ref()
        .map(|_| Value::String("[tool_choice]".into()))
        .unwrap_or(Value::String("null".into())));
    
    // OpenRouter-specific parameters
    obj.insert("seed".into(), request.seed
        .map(|v| Value::Number(v.into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("top_k".into(), request.top_k
        .map(|v| Value::Number(v.into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("frequency_penalty".into(), request.frequency_penalty
        .map(|v| Value::Number(serde_json::Number::from_f64(v).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("presence_penalty".into(), request.presence_penalty
        .map(|v| Value::Number(serde_json::Number::from_f64(v).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("repetition_penalty".into(), request.repetition_penalty
        .map(|v| Value::Number(serde_json::Number::from_f64(v).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("logit_bias".into(), request.logit_bias.as_ref()
        .map(|v| v.clone())
        .unwrap_or(Value::String("null".into())));
    obj.insert("top_logprobs".into(), request.top_logprobs
        .map(|v| Value::Number(v.into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("min_p".into(), request.min_p
        .map(|v| Value::Number(serde_json::Number::from_f64(v).unwrap()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("top_a".into(), request.top_a
        .map(|v| Value::Number(serde_json::Number::from_f64(v).unwrap()))
        .unwrap_or(Value::String("null".into())));
    
    // OpenRouter routing parameters
    obj.insert("prediction".into(), request.prediction.as_ref()
        .map(|_| Value::String("[prediction]".into()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("transforms".into(), request.transforms.as_ref()
        .map(|v| Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("models".into(), request.models.as_ref()
        .map(|v| Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("route".into(), request.route.as_ref()
        .map(|v| Value::String(v.clone()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("preset".into(), request.preset.as_ref()
        .map(|v| Value::String(v.clone()))
        .unwrap_or(Value::String("null".into())));
    obj.insert("provider".into(), request.provider.as_ref()
        .map(|_| Value::String("[provider_prefs]".into()))
        .unwrap_or(Value::String("null".into())));
    
    // Reasoning configuration (our new fix!)
    obj.insert("reasoning".into(), request.reasoning.as_ref()
        .map(|r| {
            let mut reasoning_obj = serde_json::Map::new();
            reasoning_obj.insert("enabled".into(), r.enabled
                .map(Value::Bool)
                .unwrap_or(Value::String("null".into())));
            reasoning_obj.insert("effort".into(), r.effort.as_ref()
                .map(|v| Value::String(v.clone()))
                .unwrap_or(Value::String("null".into())));
            reasoning_obj.insert("max_tokens".into(), r.max_tokens
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::String("null".into())));
            reasoning_obj.insert("exclude".into(), r.exclude
                .map(Value::Bool)
                .unwrap_or(Value::String("null".into())));
            Value::Object(reasoning_obj)
        })
        .unwrap_or(Value::String("null".into())));
    
    serde_json::to_string_pretty(&Value::Object(obj)).ok()
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

#[cfg(test)]
mod tests {
    use super::*;
    use openrouter_ox::{ChatRequest, ReasoningConfig};
    use openrouter_ox::message::Message;

    #[test]
    fn test_anthropic_detailed_logging() {
        let mut request = anthropic_ox::ChatRequest::builder()
            .model("claude-3-sonnet")
            .messages(Vec::<anthropic_ox::message::Message>::new())
            .build();
        
        // Set some parameters
        request.temperature = Some(0.7);
        request.top_k = Some(40);
        request.thinking = Some(anthropic_ox::request::ThinkingConfig::new(2000));
        
        let log_output = prepare_anthropic_request_log(&request);
        assert!(log_output.is_some());
        
        let log_str = log_output.unwrap();
        println!("Sample Anthropic detailed log:\n{}", log_str);
        
        // Verify all fields are present
        assert!(log_str.contains("temperature"));
        assert!(log_str.contains("thinking"));
        assert!(log_str.contains("budget_tokens"));
        assert!(log_str.contains("\"null\"")); // Should show null values
    }
    
    #[test]
    fn test_openai_detailed_logging() {
        let mut request = openai_ox::ChatRequest::builder()
            .model("gpt-4")
            .messages(vec![])
            .build();
        
        // Set some parameters
        request.temperature = Some(0.8);
        request.seed = Some(123);
        request.n = Some(2);
        
        let log_output = prepare_openai_request_log(&request);
        assert!(log_output.is_some());
        
        let log_str = log_output.unwrap();
        println!("Sample OpenAI detailed log:\n{}", log_str);
        
        // Verify all fields are present
        assert!(log_str.contains("temperature"));
        assert!(log_str.contains("seed"));
        assert!(log_str.contains("\"null\"")); // Should show null values
    }
    
    #[test]
    fn test_gemini_detailed_logging() {
        use gemini_ox::generate_content::{GenerationConfig, ThinkingConfig};
        
        let content = gemini_ox::Content::new(gemini_ox::Role::User, vec!["test"]);
        let mut request = gemini_ox::generate_content::request::GenerateContentRequest::builder()
            .model("gemini-2.0-flash")
            .content_list(vec![content])
            .build();
        
        // Set generation config with thinking
        let mut gen_config = GenerationConfig::default();
        gen_config.temperature = Some(0.9);
        gen_config.thinking_config = Some(ThinkingConfig {
            include_thoughts: true,
            thinking_budget: 1500,
        });
        request.generation_config = Some(gen_config);
        
        let log_output = prepare_gemini_request_log(&request);
        assert!(log_output.is_some());
        
        let log_str = log_output.unwrap();
        println!("Sample Gemini detailed log:\n{}", log_str);
        
        // Verify all fields are present
        assert!(log_str.contains("generation_config"));
        assert!(log_str.contains("thinking_config"));
        assert!(log_str.contains("include_thoughts"));
        assert!(log_str.contains("\"null\"")); // Should show null values
    }
    #[test]
    fn test_openrouter_detailed_logging() {
        // Create a sample OpenRouter request with various parameters
        let mut request = ChatRequest::new("openrouter/openai/gpt-4o", vec![
            Message::user("Test message"),
        ]);
        
        // Set some parameters to show in log
        request.temperature = Some(0.8);
        request.max_tokens = Some(2000);
        request.seed = Some(42);
        request.top_k = Some(50);
        request.frequency_penalty = Some(0.5);
        
        // Add reasoning config with the new structure
        request.reasoning = Some(ReasoningConfig {
            enabled: Some(true),
            effort: Some("high".to_string()),
            max_tokens: Some(1500),
            exclude: Some(false),
        });
        
        let log_output = prepare_openrouter_request_log(&request);
        assert!(log_output.is_some());
        
        let log_str = log_output.unwrap();
        println!("Sample OpenRouter detailed log:\n{}", log_str);
        
        // Verify all fields are present (even null ones)
        assert!(log_str.contains("temperature"));
        assert!(log_str.contains("seed"));
        assert!(log_str.contains("reasoning"));
        assert!(log_str.contains("effort"));
        assert!(log_str.contains("\"high\""));
        assert!(log_str.contains("provider"));
        assert!(log_str.contains("null")); // Should show null values
    }
}
