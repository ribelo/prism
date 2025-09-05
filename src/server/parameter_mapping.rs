use std::collections::HashMap;
use std::str::FromStr;

/// Generic parameter applicator - no more copy-paste bullshit
fn apply_param<T: FromStr>(target: &mut Option<T>, params: &HashMap<String, String>, key: &str) {
    if let Some(value_str) = params.get(key)
        && let Ok(value) = value_str.parse::<T>()
    {
        *target = Some(value);
    }
}

/// Apply parameters to Anthropic ChatRequest
pub fn apply_anthropic_parameters(
    mut request: anthropic_ox::ChatRequest,
    query_params: &HashMap<String, String>,
) -> anthropic_ox::ChatRequest {
    // Standard parameters
    apply_param(&mut request.temperature, query_params, "temperature");
    apply_param(&mut request.top_p, query_params, "top_p");
    apply_param(&mut request.top_k, query_params, "top_k");

    // Max tokens is required field, handle specially
    if let Some(max_tokens_str) = query_params.get("max_tokens")
        && let Ok(value) = max_tokens_str.parse::<u32>()
    {
        request.max_tokens = value;
    }

    // Thinking parameter - standardize on "think"
    if let Some(think_str) = query_params.get("think")
        && let Ok(budget) = think_str.parse::<u32>()
    {
        request.thinking = Some(anthropic_ox::request::ThinkingConfig::new(budget));
    }

    request
}

/// Apply parameters to OpenRouter ChatRequest
pub fn apply_openrouter_parameters(
    mut request: openrouter_ox::ChatRequest,
    query_params: &HashMap<String, String>,
) -> (openrouter_ox::ChatRequest, Option<bool>) {
    // Standard parameters
    apply_param(&mut request.temperature, query_params, "temperature");
    apply_param(&mut request.max_tokens, query_params, "max_tokens");
    apply_param(&mut request.top_p, query_params, "top_p");

    // OpenRouter-specific parameters
    apply_param(&mut request.seed, query_params, "seed");
    apply_param(
        &mut request.frequency_penalty,
        query_params,
        "frequency_penalty",
    );
    apply_param(
        &mut request.presence_penalty,
        query_params,
        "presence_penalty",
    );
    apply_param(
        &mut request.repetition_penalty,
        query_params,
        "repetition_penalty",
    );
    apply_param(&mut request.top_k, query_params, "top_k");
    apply_param(&mut request.min_p, query_params, "min_p");
    apply_param(&mut request.top_a, query_params, "top_a");
    apply_param(&mut request.top_logprobs, query_params, "top_logprobs");

    // Reasoning flag
    let reasoning_flag = query_params
        .get("reasoning")
        .and_then(|v| v.parse::<bool>().ok());

    if let Some(reasoning) = reasoning_flag {
        request.include_reasoning = Some(reasoning);
    }

    (request, reasoning_flag)
}

/// Apply provider preferences from query parameters
pub fn apply_openrouter_provider_params(
    mut provider_prefs: openrouter_ox::provider_preference::ProviderPreferences,
    query_params: &HashMap<String, String>,
) -> openrouter_ox::provider_preference::ProviderPreferences {
    // Effort mapping
    if let Some(effort) = query_params.get("effort") {
        provider_prefs.sort = match effort.as_str() {
            "high" => Some(openrouter_ox::provider_preference::Sort::Throughput),
            "low" => Some(openrouter_ox::provider_preference::Sort::Price),
            _ => None,
        };
    }

    // Direct sort parameter
    if let Some(sort) = query_params.get("sort") {
        provider_prefs.sort = match sort.as_str() {
            "price" => Some(openrouter_ox::provider_preference::Sort::Price),
            "throughput" => Some(openrouter_ox::provider_preference::Sort::Throughput),
            _ => None,
        };
    }

    // Quantization mapping
    if let Some(quant_str) = query_params.get("quantization") {
        let quant_type = match quant_str.to_lowercase().as_str() {
            "int4" => openrouter_ox::provider_preference::Quantization::Int4,
            "int8" => openrouter_ox::provider_preference::Quantization::Int8,
            "fp4" => openrouter_ox::provider_preference::Quantization::Fp4,
            "fp6" => openrouter_ox::provider_preference::Quantization::Fp6,
            "fp8" => openrouter_ox::provider_preference::Quantization::Fp8,
            "fp16" => openrouter_ox::provider_preference::Quantization::Fp16,
            "bf16" => openrouter_ox::provider_preference::Quantization::Bf16,
            "fp32" => openrouter_ox::provider_preference::Quantization::Fp32,
            _ => openrouter_ox::provider_preference::Quantization::Unknown,
        };
        provider_prefs.quantizations = Some(vec![quant_type]);
    }

    provider_prefs
}

/// Create Gemini thinking config from query parameters
pub fn create_gemini_thinking_config(
    query_params: &HashMap<String, String>,
) -> Option<gemini_ox::generate_content::ThinkingConfig> {
    let thinking_budget = query_params
        .get("think")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);

    let include_thoughts = query_params
        .get("thoughts")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    if thinking_budget > 0 || include_thoughts {
        Some(gemini_ox::generate_content::ThinkingConfig {
            include_thoughts,
            thinking_budget,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_anthropic_parameter_mapping() {
        let mut params = HashMap::new();
        params.insert("temperature".to_string(), "0.7".to_string());
        params.insert("max_tokens".to_string(), "2000".to_string());
        params.insert("top_k".to_string(), "40".to_string());
        params.insert("think".to_string(), "1500".to_string());

        let request = anthropic_ox::ChatRequest::builder()
            .model("claude-3-sonnet")
            .messages(Vec::<anthropic_ox::message::Message>::new())
            .build();

        let modified_request = apply_anthropic_parameters(request, &params);

        assert_eq!(modified_request.temperature, Some(0.7));
        assert_eq!(modified_request.max_tokens, 2000);
        assert_eq!(modified_request.top_k, Some(40));
        assert!(modified_request.thinking.is_some());
        assert_eq!(modified_request.thinking.unwrap().budget_tokens, 1500);
    }

    #[test]
    fn test_openrouter_parameter_mapping() {
        let mut params = HashMap::new();
        params.insert("temperature".to_string(), "0.8".to_string());
        params.insert("seed".to_string(), "42".to_string());
        params.insert("reasoning".to_string(), "true".to_string());

        let request = openrouter_ox::ChatRequest::new("gpt-4", vec![]);
        let (modified_request, reasoning_flag) = apply_openrouter_parameters(request, &params);

        assert_eq!(modified_request.temperature, Some(0.8));
        assert_eq!(modified_request.seed, Some(42));
        assert_eq!(reasoning_flag, Some(true));
    }

    #[test]
    fn test_gemini_thinking_config() {
        let mut params = HashMap::new();
        params.insert("think".to_string(), "2000".to_string());
        params.insert("thoughts".to_string(), "true".to_string());

        let config = create_gemini_thinking_config(&params);

        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.thinking_budget, 2000);
        assert!(config.include_thoughts);
    }

    #[test]
    fn test_provider_preferences() {
        let mut params = HashMap::new();
        params.insert("effort".to_string(), "high".to_string());
        params.insert("quantization".to_string(), "int8".to_string());

        let prefs = openrouter_ox::provider_preference::ProviderPreferences {
            allow_fallbacks: None,
            require_parameters: None,
            data_collection: None,
            order: None,
            only: None,
            ignore: None,
            quantizations: None,
            sort: None,
            max_price: None,
        };

        let modified_prefs = apply_openrouter_provider_params(prefs, &params);
        assert_eq!(
            modified_prefs.sort,
            Some(openrouter_ox::provider_preference::Sort::Throughput)
        );
        assert_eq!(
            modified_prefs.quantizations,
            Some(vec![openrouter_ox::provider_preference::Quantization::Int8])
        );
    }

    #[test]
    fn test_invalid_parameters_ignored() {
        let mut params = HashMap::new();
        params.insert("temperature".to_string(), "invalid".to_string());
        params.insert("max_tokens".to_string(), "not_a_number".to_string());

        let request = anthropic_ox::ChatRequest::builder()
            .model("claude-3-sonnet")
            .messages(Vec::<anthropic_ox::message::Message>::new())
            .build();

        let modified_request = apply_anthropic_parameters(request, &params);

        // Invalid values should be ignored, defaults preserved
        assert_eq!(modified_request.temperature, None);
        assert_eq!(modified_request.max_tokens, 4096); // Default from builder
    }
}
