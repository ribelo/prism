use crate::{
    config::Config,
    router::{CompositeRouterBuilder, ModelRouter, ProviderRouter, RouteRequest, Router},
};

/// Router factory for creating routers based on configuration
pub struct RouterFactory;

impl RouterFactory {
    /// Create a router based on the configuration
    pub fn create_router(config: &Config) -> Box<dyn Router> {
        let available_providers: Vec<String> = if config.routing.provider_priorities.is_empty() {
            config.providers.keys().cloned().collect()
        } else {
            config.routing.provider_priorities.clone()
        };
        let default_provider = config.routing.default_provider.clone();

        match config.routing.strategy.as_str() {
            "model" => Box::new(ModelRouter::new(default_provider)),
            "provider" => {
                let mut provider_router = ProviderRouter::new(available_providers);

                if !config.routing.provider_aliases.is_empty() {
                    provider_router =
                        provider_router.with_aliases(config.routing.provider_aliases.clone());
                }

                if !config.routing.provider_capabilities.is_empty() {
                    provider_router = provider_router
                        .with_capabilities(config.routing.provider_capabilities.clone());
                }

                Box::new(provider_router)
            }
            "composite" | _ => {
                let builder =
                    CompositeRouterBuilder::standard(available_providers, default_provider)
                        .with_fallback(config.routing.enable_fallback)
                        .with_min_confidence(config.routing.min_confidence);

                // Could add custom routers here based on additional config

                Box::new(builder.build())
            }
        }
    }

    /// Create a simple model router for backward compatibility
    pub fn create_model_router(default_provider: String) -> Box<dyn Router> {
        Box::new(ModelRouter::new(default_provider))
    }

    /// Helper to convert model name to RouteRequest
    pub fn model_to_request(model_name: &str) -> RouteRequest {
        RouteRequest::new(model_name.to_string())
    }
}

/// Adapter to use new Router with legacy RoutingDecision
pub struct RouterAdapter {
    router: Box<dyn Router>,
}

impl RouterAdapter {
    pub fn new(router: Box<dyn Router>) -> Self {
        Self { router }
    }

    /// Route a model name and return a legacy RoutingDecision
    pub async fn route_model(
        &self,
        model_name: &str,
    ) -> crate::error::Result<crate::router::name_based::LegacyRoutingDecision> {
        let request = RouterFactory::model_to_request(model_name);
        let decision = self.router.route(&request).await?;

        Ok(crate::router::name_based::LegacyRoutingDecision {
            provider: decision.provider,
            model: decision.model,
            original_model: decision.original_model,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthConfig, ProviderConfig, RoutingConfig, ServerConfig};
    use std::collections::HashMap;

    fn create_test_config() -> Config {
        let mut providers = HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                r#type: "anthropic".to_string(),
                endpoint: "https://api.anthropic.com".to_string(),
                models: vec!["claude-3-sonnet".to_string()],
                auth: AuthConfig::default(),
            },
        );
        providers.insert(
            "openrouter".to_string(),
            ProviderConfig {
                r#type: "openrouter".to_string(),
                endpoint: "https://openrouter.ai/api/v1".to_string(),
                models: vec!["gpt-4".to_string()],
                auth: AuthConfig::default(),
            },
        );

        Config {
            server: ServerConfig::default(),
            providers,
            routing: RoutingConfig {
                default_provider: "openrouter".to_string(),
                strategy: "composite".to_string(),
                enable_fallback: true,
                min_confidence: 0.0,
                rules: HashMap::new(),
                provider_priorities: Vec::new(),
                provider_capabilities: HashMap::new(),
                provider_aliases: HashMap::new(),
            },
            auth: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_router_factory() {
        let config = create_test_config();
        let router = RouterFactory::create_router(&config);

        // Test routing with the created router
        let request = RouterFactory::model_to_request("claude-3-sonnet");
        let decision = router.route(&request).await.unwrap();

        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3-sonnet");
    }

    #[tokio::test]
    async fn test_router_adapter() {
        let config = create_test_config();
        let router = RouterFactory::create_router(&config);
        let adapter = RouterAdapter::new(router);

        // Test adapter compatibility with legacy interface
        let decision = adapter
            .route_model("openrouter/z-ai/glm-4.5")
            .await
            .unwrap();

        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "z-ai/glm-4.5");
        assert_eq!(decision.original_model, "openrouter/z-ai/glm-4.5");
    }

    #[tokio::test]
    async fn test_model_router_creation() {
        let router = RouterFactory::create_model_router("anthropic".to_string());

        let request = RouterFactory::model_to_request("claude-3-sonnet");
        let decision = router.route(&request).await.unwrap();

        assert_eq!(decision.provider, "anthropic");
    }
}
