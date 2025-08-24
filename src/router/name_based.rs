use crate::{
    config::{models::ParsedModel, Config},
    error::{Result, SetuError},
};

/// Name-based router that routes requests based on model name format
pub struct NameBasedRouter {
    config: Config,
}

impl NameBasedRouter {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn route_model(&self, model_name: &str) -> Result<RoutingDecision> {
        // Try to parse our custom format first: provider/model
        match ParsedModel::parse(model_name) {
            Ok(parsed) => {
                // Validate that the provider exists in config
                if !self.config.providers.contains_key(&parsed.provider) {
                    return Err(SetuError::ProviderNotFound(parsed.provider.clone()));
                }

                Ok(RoutingDecision {
                    provider: parsed.provider,
                    model: parsed.model,
                    original_model: model_name.to_string(),
                })
            }
            Err(_) => {
                // If parsing fails, try to infer provider from model name
                let inferred_provider = self.infer_provider_from_model(model_name)?;
                
                Ok(RoutingDecision {
                    provider: inferred_provider,
                    model: model_name.to_string(),
                    original_model: model_name.to_string(),
                })
            }
        }
    }

    fn infer_provider_from_model(&self, model_name: &str) -> Result<String> {
        // Common model name patterns
        if model_name.starts_with("claude-") {
            return Ok("anthropic".to_string());
        }
        
        if model_name.starts_with("gemini-") {
            return Ok("gemini".to_string());
        }
        
        if model_name.starts_with("gpt-") || model_name.starts_with("text-") {
            return Ok("openrouter".to_string()); // Route OpenAI models through OpenRouter
        }

        // Default to configured default provider
        Ok(self.config.routing.default_provider.clone())
    }
}

#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub provider: String,
    pub model: String,
    pub original_model: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::config::{Config, ServerConfig, ProviderConfig, RoutingConfig, AuthConfig};

    fn create_test_config() -> Config {
        let mut providers = HashMap::new();
        providers.insert("openrouter".to_string(), ProviderConfig {
            r#type: "openrouter".to_string(),
            endpoint: "https://openrouter.ai/api/v1".to_string(),
            models: vec!["gpt-4".to_string()],
            auth: AuthConfig {
                oauth_access_token: None,
                oauth_refresh_token: None,
                oauth_expires: None,
            },
        });
        providers.insert("anthropic".to_string(), ProviderConfig {
            r#type: "anthropic".to_string(),
            endpoint: "https://api.anthropic.com".to_string(),
            models: vec!["claude-3-opus".to_string()],
            auth: AuthConfig {
                oauth_access_token: None,
                oauth_refresh_token: None,
                oauth_expires: None,
            },
        });

        Config {
            server: ServerConfig::default(),
            providers,
            routing: RoutingConfig {
                default_provider: "openrouter".to_string(),
            },
            auth: HashMap::new(),
        }
    }

    #[test]
    fn test_route_explicit_provider() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);
        
        let decision = router.route_model("anthropic/claude-3-opus").unwrap();
        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3-opus");
    }

    #[test]
    fn test_route_inferred_provider() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);
        
        let decision = router.route_model("gpt-4").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "gpt-4");
        
        let decision = router.route_model("claude-3-sonnet").unwrap();
        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3-sonnet");
    }

    #[test]
    fn test_route_unknown_provider() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);
        
        let result = router.route_model("nonexistent/model");
        assert!(result.is_err());
    }

    #[test]
    fn test_route_default_provider() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);
        
        let decision = router.route_model("unknown-model").unwrap();
        assert_eq!(decision.provider, "openrouter"); // default
        assert_eq!(decision.model, "unknown-model");
    }
}