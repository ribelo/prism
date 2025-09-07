use prism::config::models::ParsedModel;

#[test]
fn test_parse_valid_model_formats() {
    // Test various valid model formats
    let test_cases = vec![
        ("openrouter/openai/gpt-4o", "openrouter", "openai/gpt-4o"),
        ("anthropic/claude-3-sonnet", "anthropic", "claude-3-sonnet"),
        ("gemini/gemini-pro", "gemini", "gemini-pro"),
        ("local/llama-2-70b", "local", "llama-2-70b"),
        (
            "provider/complex-model-name-v2",
            "provider",
            "complex-model-name-v2",
        ),
    ];

    for (input, expected_provider, expected_model) in test_cases {
        let parsed = ParsedModel::parse(input).unwrap();
        assert_eq!(
            parsed.provider, expected_provider,
            "Failed for input: {}",
            input
        );
        assert_eq!(parsed.model, expected_model, "Failed for input: {}", input);
        assert_eq!(parsed.full_name(), input, "Failed for input: {}", input);
    }
}

#[test]
fn test_parse_invalid_model_formats() {
    let invalid_cases = vec![
        "just-a-model-name",
        "",
        "/",
        "provider/",
        "/model",
        "//",
        "provider//model",
    ];

    for invalid_input in invalid_cases {
        let result = ParsedModel::parse(invalid_input);
        assert!(
            result.is_err(),
            "Expected error for input: '{}'",
            invalid_input
        );

        // Verify the error message mentions the format requirement
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("provider/model"),
            "Error message should mention format requirement for: '{}'",
            invalid_input
        );
    }
}

#[test]
fn test_parsed_model_full_name() {
    let parsed = ParsedModel {
        provider: "test_provider".to_string(),
        model: "test_model".to_string(),
    };
    assert_eq!(parsed.full_name(), "test_provider/test_model");
}

#[test]
fn test_parse_complex_model_names() {
    // Test more complex model names that might appear in real usage
    let complex_cases = vec![
        (
            "openrouter/anthropic/claude-3-5-sonnet-20241022",
            "openrouter",
            "anthropic/claude-3-5-sonnet-20241022",
        ),
        (
            "huggingface/microsoft/DialoGPT-medium",
            "huggingface",
            "microsoft/DialoGPT-medium",
        ),
        (
            "custom/org/model-v1.2.3-fine-tuned",
            "custom",
            "org/model-v1.2.3-fine-tuned",
        ),
    ];

    for (input, expected_provider, expected_model) in complex_cases {
        let parsed = ParsedModel::parse(input).unwrap();
        assert_eq!(
            parsed.provider, expected_provider,
            "Failed for complex input: {}",
            input
        );
        assert_eq!(
            parsed.model, expected_model,
            "Failed for complex input: {}",
            input
        );
    }
}

#[test]
fn test_parse_with_whitespace_trimming() {
    // Test that whitespace is properly trimmed
    let whitespace_cases = vec![
        (" anthropic/claude-3-sonnet", "anthropic", "claude-3-sonnet"),
        ("gemini/gemini-pro ", "gemini", "gemini-pro"),
        (" openrouter/gpt-4 ", "openrouter", "gpt-4"),
        ("  local/model  ", "local", "model"),
    ];

    for (input, expected_provider, expected_model) in whitespace_cases {
        let parsed = ParsedModel::parse(input).unwrap();
        assert_eq!(
            parsed.provider, expected_provider,
            "Failed for whitespace input: {}",
            input
        );
        assert_eq!(
            parsed.model, expected_model,
            "Failed for whitespace input: {}",
            input
        );
    }
}
