use setu::config::models::ParsedModel;

#[test]
fn test_parse_valid_model_formats() {
    // Test various valid model formats
    let test_cases = vec![
        ("openrouter/openai/gpt-4o", "openrouter", "openai/gpt-4o"),
        ("anthropic/claude-3-sonnet", "anthropic", "claude-3-sonnet"),
        ("gemini/gemini-pro", "gemini", "gemini-pro"),
        ("local/llama-2-70b", "local", "llama-2-70b"),
    ];

    for (input, expected_provider, expected_model) in test_cases {
        let parsed = ParsedModel::parse(input).unwrap();
        assert_eq!(parsed.provider, expected_provider);
        assert_eq!(parsed.model, expected_model);
        assert_eq!(parsed.full_name(), input);
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
    ];

    for invalid_input in invalid_cases {
        assert!(ParsedModel::parse(invalid_input).is_err(), 
                "Expected error for input: {}", invalid_input);
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