use crate::{
    config::Config,
    error::Result,
};

/// Name-based router that routes requests based on model name format
pub struct NameBasedRouter {
    default_provider: String,
}

impl NameBasedRouter {
    pub fn new(config: Config) -> Self {
        Self {
            default_provider: config.routing.default_provider.clone(),
        }
    }
    
    pub fn new_with_default_provider(default_provider: String) -> Self {
        Self { default_provider }
    }

    pub fn route_model(&self, model_name: &str) -> Result<RoutingDecision> {
        // Route based on model name prefix patterns
        let provider = self.infer_provider_from_model(model_name)?;
        
        Ok(RoutingDecision {
            provider,
            model: model_name.to_string(),
            original_model: model_name.to_string(),
        })
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
        Ok(self.default_provider.clone())
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
    fn test_route_by_model_prefix() {
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
    fn test_route_default_provider() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);
        
        let decision = router.route_model("unknown-model").unwrap();
        assert_eq!(decision.provider, "openrouter"); // default
        assert_eq!(decision.model, "unknown-model");
    }
}