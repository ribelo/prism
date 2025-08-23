use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedModel {
    pub provider: String,
    pub model: String,
}

impl ParsedModel {
    pub fn parse(model_name: &str) -> crate::error::Result<Self> {
        // Parse format: provider/model-name
        let parts: Vec<&str> = model_name.splitn(2, '/').collect();
        
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(crate::error::SetuError::InvalidModel(
                format!("Model name must be in format 'provider/model', got: {}", model_name)
            ));
        }

        Ok(Self {
            provider: parts[0].to_string(),
            model: parts[1].to_string(),
        })
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_model() {
        let parsed = ParsedModel::parse("openrouter/openai/gpt-4o").unwrap();
        assert_eq!(parsed.provider, "openrouter");
        assert_eq!(parsed.model, "openai/gpt-4o");
    }

    #[test]
    fn test_parse_anthropic_model() {
        let parsed = ParsedModel::parse("anthropic/claude-3-sonnet").unwrap();
        assert_eq!(parsed.provider, "anthropic");
        assert_eq!(parsed.model, "claude-3-sonnet");
    }

    #[test]
    fn test_parse_invalid_model() {
        assert!(ParsedModel::parse("invalid-model").is_err());
        assert!(ParsedModel::parse("").is_err());
    }

    #[test]
    fn test_full_name() {
        let parsed = ParsedModel {
            provider: "anthropic".to_string(),
            model: "claude-3-opus".to_string(),
        };
        assert_eq!(parsed.full_name(), "anthropic/claude-3-opus");
    }
}