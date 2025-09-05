use crate::config::{Config, ModelRoute};
use crate::error::{Result, SetuError};
use crate::router::name_based::{NameBasedRouter, RoutingDecision};
use std::collections::HashSet;

/// Model-to-model router with fallback chain support
pub struct ModelRouter {
    config: Config,
    name_based_router: NameBasedRouter,
}

impl ModelRouter {
    pub fn new(config: Config) -> Self {
        let name_based_router = NameBasedRouter::new(config.clone());
        Self {
            config,
            name_based_router,
        }
    }

    /// Recursively resolve model mappings, preventing infinite loops
    fn resolve_model_mapping(
        &self,
        model_name: &str,
        visited: &mut HashSet<String>,
    ) -> Result<Vec<String>> {
        // Prevent infinite recursion
        if visited.contains(model_name) {
            tracing::warn!(
                "Circular model mapping detected for '{}', breaking cycle",
                model_name
            );
            return Ok(vec![model_name.to_string()]);
        }
        visited.insert(model_name.to_string());

        // Check if we have an explicit model mapping
        if let Some(model_route) = self.config.routing.models.get(model_name) {
            let mapped_models = match model_route {
                ModelRoute::Single(model) => vec![model.clone()],
                ModelRoute::Multiple(models) => models.clone(),
            };

            // Recursively resolve each mapped model
            let mut resolved = Vec::new();
            for mapped_model in mapped_models {
                // Check if the mapped model itself has a mapping (recursive resolution)
                let sub_resolved = self.resolve_model_mapping(&mapped_model, visited)?;
                resolved.extend(sub_resolved);
            }
            return Ok(resolved);
        }

        // No mapping found, return the model as-is
        Ok(vec![model_name.to_string()])
    }

    /// Route a model name, checking for explicit mappings first, then falling back to name-based routing
    pub fn route_model(&self, model_name: &str) -> Result<Vec<RoutingDecision>> {
        // Recursively resolve model mappings
        let mut visited = HashSet::new();
        let resolved_models = self.resolve_model_mapping(model_name, &mut visited)?;

        // If we got back the same model name, it means there was no mapping
        if resolved_models.len() == 1 && resolved_models[0] == model_name {
            // No explicit mapping found, use name-based routing as-is
            let decision = self.name_based_router.route_model(model_name)?;
            return Ok(vec![decision]);
        }

        // Convert each resolved model to a routing decision
        let mut routing_decisions = Vec::new();
        for resolved_model in resolved_models {
            // Route each resolved model through the name-based router
            match self.name_based_router.route_model(&resolved_model) {
                Ok(mut decision) => {
                    // Override the model name with the target model
                    decision.model = self.extract_model_name(&resolved_model);
                    decision.original_model = model_name.to_string();
                    routing_decisions.push(decision);
                }
                Err(e) => {
                    tracing::warn!("Failed to route resolved model '{}': {}", resolved_model, e);
                    continue;
                }
            }
        }

        if routing_decisions.is_empty() {
            return Err(SetuError::Other(format!(
                "All fallback routes failed for model '{}'",
                model_name
            )));
        }

        Ok(routing_decisions)
    }

    /// Extract the actual model name from a provider/model string
    /// Examples: "openai/gpt-4o" -> "gpt-4o", "anthropic/claude-3" -> "claude-3"
    fn extract_model_name(&self, model_spec: &str) -> String {
        if let Some(slash_pos) = model_spec.find('/') {
            let after_slash = &model_spec[slash_pos + 1..];
            // Handle provider preferences like ":fireworks"
            if let Some(colon_pos) = after_slash.rfind(':') {
                after_slash[..colon_pos].to_string()
            } else {
                after_slash.to_string()
            }
        } else {
            model_spec.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthConfig, ProviderConfig, RetryConfig, RoutingConfig, ServerConfig};
    use rustc_hash::FxHashMap;

    fn create_test_config_with_model_routing() -> Config {
        let mut providers = FxHashMap::default();
        providers.insert(
            "openai".to_string(),
            ProviderConfig {
                r#type: "openai".to_string(),
                endpoint: "https://api.openai.com".to_string(),
                auth: AuthConfig::default(),
                retry: RetryConfig::default(),
                api_key: None,
                api_key_fallback: false,
                fallback_on_errors: vec![429],
            },
        );
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                r#type: "anthropic".to_string(),
                endpoint: "https://api.anthropic.com".to_string(),
                auth: AuthConfig::default(),
                retry: RetryConfig::default(),
                api_key: None,
                api_key_fallback: false,
                fallback_on_errors: vec![429],
            },
        );

        let mut model_routes = FxHashMap::default();
        model_routes.insert(
            "haiku-3.5".to_string(),
            ModelRoute::Single("openai/gpt-4o".to_string()),
        );
        model_routes.insert(
            "claude-3".to_string(),
            ModelRoute::Multiple(vec![
                "openai/gpt-4o".to_string(),
                "anthropic/claude-3-5-sonnet".to_string(),
            ]),
        );

        Config {
            server: ServerConfig::default(),
            providers,
            routing: RoutingConfig {
                models: model_routes,
            },
            auth: FxHashMap::default(),
        }
    }

    #[test]
    fn test_single_model_mapping() {
        let config = create_test_config_with_model_routing();
        let router = ModelRouter::new(config);

        let decisions = router.route_model("haiku-3.5").unwrap();
        assert_eq!(decisions.len(), 1);

        let decision = &decisions[0];
        assert_eq!(decision.provider, "openai");
        assert_eq!(decision.model, "gpt-4o");
        assert_eq!(decision.original_model, "haiku-3.5");
    }

    #[test]
    fn test_multiple_model_fallback() {
        let config = create_test_config_with_model_routing();
        let router = ModelRouter::new(config);

        let decisions = router.route_model("claude-3").unwrap();
        assert_eq!(decisions.len(), 2);

        // First fallback: openai/gpt-4o
        let decision1 = &decisions[0];
        assert_eq!(decision1.provider, "openai");
        assert_eq!(decision1.model, "gpt-4o");
        assert_eq!(decision1.original_model, "claude-3");

        // Second fallback: anthropic/claude-3-5-sonnet
        let decision2 = &decisions[1];
        assert_eq!(decision2.provider, "anthropic");
        assert_eq!(decision2.model, "claude-3-5-sonnet");
        assert_eq!(decision2.original_model, "claude-3");
    }

    #[test]
    fn test_unmapped_model_passthrough() {
        let config = create_test_config_with_model_routing();
        let router = ModelRouter::new(config);

        // Test a model that's not in our mapping - should use name-based routing
        let decisions = router.route_model("gpt-4").unwrap();
        assert_eq!(decisions.len(), 1);

        let decision = &decisions[0];
        assert_eq!(decision.provider, "openrouter"); // Based on name-based routing (gpt-* -> openrouter)
        assert_eq!(decision.model, "gpt-4");
        assert_eq!(decision.original_model, "gpt-4");
    }

    #[test]
    fn test_extract_model_name() {
        let config = create_test_config_with_model_routing();
        let router = ModelRouter::new(config);

        assert_eq!(router.extract_model_name("openai/gpt-4o"), "gpt-4o");
        assert_eq!(
            router.extract_model_name("anthropic/claude-3-5-sonnet"),
            "claude-3-5-sonnet"
        );
        assert_eq!(
            router.extract_model_name("openrouter/z-ai/glm-4.5:fireworks"),
            "z-ai/glm-4.5"
        );
        assert_eq!(router.extract_model_name("simple-model"), "simple-model");
    }

    #[test]
    fn test_recursive_model_resolution() {
        let mut model_routes = FxHashMap::default();
        // Create a chain: foo -> bar -> baz -> openrouter/x-ai/grok
        model_routes.insert("foo".to_string(), ModelRoute::Single("bar".to_string()));
        model_routes.insert("bar".to_string(), ModelRoute::Single("baz".to_string()));
        model_routes.insert(
            "baz".to_string(),
            ModelRoute::Single("openrouter/x-ai/grok-code-fast-1".to_string()),
        );

        let mut providers = FxHashMap::default();
        providers.insert(
            "openrouter".to_string(),
            ProviderConfig {
                r#type: "openrouter".to_string(),
                endpoint: "https://openrouter.ai/api/v1".to_string(),
                auth: AuthConfig::default(),
                retry: RetryConfig::default(),
                api_key: None,
                api_key_fallback: false,
                fallback_on_errors: vec![429],
            },
        );

        let config = Config {
            server: ServerConfig::default(),
            providers,
            routing: RoutingConfig {
                models: model_routes,
            },
            auth: FxHashMap::default(),
        };

        let router = ModelRouter::new(config);

        // Test recursive resolution
        let decisions = router.route_model("foo").unwrap();
        assert_eq!(decisions.len(), 1);

        let decision = &decisions[0];
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "x-ai/grok-code-fast-1");
        assert_eq!(decision.original_model, "foo");
    }

    #[test]
    fn test_circular_reference_handling() {
        let mut model_routes = FxHashMap::default();
        // Create a circular reference: foo -> bar -> foo
        model_routes.insert("foo".to_string(), ModelRoute::Single("bar".to_string()));
        model_routes.insert("bar".to_string(), ModelRoute::Single("foo".to_string()));

        let mut providers = FxHashMap::default();
        providers.insert(
            "openrouter".to_string(),
            ProviderConfig {
                r#type: "openrouter".to_string(),
                endpoint: "https://openrouter.ai/api/v1".to_string(),
                auth: AuthConfig::default(),
                retry: RetryConfig::default(),
                api_key: None,
                api_key_fallback: false,
                fallback_on_errors: vec![429],
            },
        );

        let config = Config {
            server: ServerConfig::default(),
            providers,
            routing: RoutingConfig {
                models: model_routes,
            },
            auth: FxHashMap::default(),
        };

        let router = ModelRouter::new(config);

        // Should handle circular reference gracefully
        let decisions = router.route_model("foo").unwrap();
        assert_eq!(decisions.len(), 1);

        // Since it detected a cycle, it should fall back to name-based routing
        let decision = &decisions[0];
        assert_eq!(decision.provider, "openrouter"); // default provider
        assert_eq!(decision.model, "foo");
        assert_eq!(decision.original_model, "foo");
    }
}
