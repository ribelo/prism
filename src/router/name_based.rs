use crate::{config::Config, error::Result};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub provider: String,
    pub model: String,
    pub original_model: String,
    pub provider_preference: Option<String>, // New: stores "fireworks", "nitro", "floor", etc.
    pub query_params: Option<HashMap<String, String>>, // New: stores query parameters like "think=1000&effort=high"
}

/// Name-based router that routes requests based on model name format
pub struct NameBasedRouter {}

impl NameBasedRouter {
    pub fn new(_config: Config) -> Self {
        Self {}
    }

    pub fn new_with_default_provider(_default_provider: String) -> Self {
        Self {}
    }

    pub fn route_model(&self, model_name: &str) -> Result<RoutingDecision> {
        // Parse query parameters first (e.g., "model?think=1000&effort=high")
        let (model_part, query_params) = if let Some(query_pos) = model_name.find('?') {
            let model = &model_name[..query_pos];
            let query = &model_name[query_pos + 1..];
            
            // Parse query string into HashMap
            let params = if query.is_empty() {
                None
            } else {
                let mut map = HashMap::new();
                for param in query.split('&') {
                    if let Some(eq_pos) = param.find('=') {
                        let key = &param[..eq_pos];
                        let value = &param[eq_pos + 1..];
                        map.insert(key.to_string(), value.to_string());
                    } else {
                        // Handle key without value (e.g., "flag")
                        map.insert(param.to_string(), "true".to_string());
                    }
                }
                Some(map)
            };
            
            (model, params)
        } else {
            (model_name, None)
        };

        // Check if model name has provider/model format (e.g., "openrouter/z-ai/glm-4.5:fireworks")
        if let Some(slash_pos) = model_part.find('/') {
            let provider = &model_part[..slash_pos];
            let model_suffix = &model_part[slash_pos + 1..];

            // Parse provider preference (e.g., ":fireworks", ":nitro", ":floor")
            let (actual_model, provider_preference) = if let Some(colon_pos) = model_suffix.rfind(':') {
                let model = &model_suffix[..colon_pos];
                let preference = &model_suffix[colon_pos + 1..];
                (model.to_string(), Some(preference.to_string()))
            } else {
                (model_suffix.to_string(), None)
            };

            return Ok(RoutingDecision {
                provider: provider.to_string(),
                model: actual_model,
                original_model: model_name.to_string(),
                provider_preference,
                query_params,
            });
        }

        // Route based on model name prefix patterns for simple names
        let provider = self.infer_provider_from_model(model_part)?;

        Ok(RoutingDecision {
            provider,
            model: model_part.to_string(),
            original_model: model_name.to_string(),
            provider_preference: None,
            query_params,
        })
    }

    fn infer_provider_from_model(&self, model_name: &str) -> Result<String> {
        // Infer provider based on model name patterns
        if model_name.starts_with("claude") {
            Ok("anthropic".to_string())
        } else if model_name.starts_with("gpt") || model_name.starts_with("o1-") {
            Ok("openrouter".to_string())
        } else if model_name.starts_with("gemini") {
            Ok("gemini".to_string())
        } else {
            // Default to openrouter for unknown models
            Ok("openrouter".to_string())
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthConfig, Config, ProviderConfig, RoutingConfig, ServerConfig};
    use rustc_hash::FxHashMap;

    fn create_test_config() -> Config {
        let mut providers = FxHashMap::default();
        providers.insert(
            "openrouter".to_string(),
            ProviderConfig {
                r#type: "openrouter".to_string(),
                endpoint: "https://openrouter.ai/api/v1".to_string(),
                auth: AuthConfig {
                    oauth_access_token: None,
                    oauth_refresh_token: None,
                    oauth_expires: None,
                    project_id: None,
                },
                retry: crate::config::RetryConfig::default(),
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
                auth: AuthConfig {
                    oauth_access_token: None,
                    oauth_refresh_token: None,
                    oauth_expires: None,
                    project_id: None,
                },
                retry: crate::config::RetryConfig::default(),
                api_key: None,
                api_key_fallback: false,
                fallback_on_errors: vec![429],
            },
        );

        Config {
            server: ServerConfig::default(),
            providers,
            routing: RoutingConfig {
                models: FxHashMap::default(),
            },
            auth: FxHashMap::default(),
        }
    }

    #[test]
    fn test_route_by_model_prefix() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        let decision = router.route_model("gpt-4").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "gpt-4");
        assert_eq!(decision.provider_preference, None);

        let decision = router.route_model("claude-3-sonnet").unwrap();
        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3-sonnet");
        assert_eq!(decision.provider_preference, None);
    }

    #[test]
    fn test_route_default_provider() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        let decision = router.route_model("unknown-model").unwrap();
        assert_eq!(decision.provider, "openrouter"); // default
        assert_eq!(decision.model, "unknown-model");
        assert_eq!(decision.provider_preference, None);
    }

    #[test]
    fn test_route_with_provider_slash_format() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        // Test openrouter/model format
        let decision = router.route_model("openrouter/z-ai/glm-4.5").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "z-ai/glm-4.5");
        assert_eq!(decision.original_model, "openrouter/z-ai/glm-4.5");
        assert_eq!(decision.provider_preference, None);

        // Test anthropic/model format
        let decision = router.route_model("anthropic/claude-3-sonnet").unwrap();
        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3-sonnet");
        assert_eq!(decision.original_model, "anthropic/claude-3-sonnet");
        assert_eq!(decision.provider_preference, None);

        // Test simple provider/model format
        let decision = router.route_model("gemini/gemini-pro").unwrap();
        assert_eq!(decision.provider, "gemini");
        assert_eq!(decision.model, "gemini-pro");
        assert_eq!(decision.original_model, "gemini/gemini-pro");
        assert_eq!(decision.provider_preference, None);
    }

    #[test]
    fn test_route_openai_reasoning_models() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        // Test o1 series - should route to OpenRouter (all OpenAI models go through OpenRouter)
        let decision = router.route_model("o1-mini").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "o1-mini");
        assert_eq!(decision.provider_preference, None);

        let decision = router.route_model("o1-preview").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "o1-preview");
        assert_eq!(decision.provider_preference, None);

        // Test o3 series - should route to OpenRouter  
        let decision = router.route_model("o3-mini").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "o3-mini");
        assert_eq!(decision.provider_preference, None);

        // Test o4 series - should route to OpenRouter
        let decision = router.route_model("o4-mini").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "o4-mini");
        assert_eq!(decision.provider_preference, None);

        // Test regular GPT models - should route to OpenRouter
        let decision = router.route_model("gpt-4o").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "gpt-4o");
        assert_eq!(decision.provider_preference, None);

        // Test with provider prefix - explicit openai provider
        let decision = router.route_model("openai/o1-mini").unwrap();
        assert_eq!(decision.provider, "openai");
        assert_eq!(decision.model, "o1-mini");
        assert_eq!(decision.original_model, "openai/o1-mini");
        assert_eq!(decision.provider_preference, None);
    }

    #[test]
    fn test_provider_specific_routing() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        // Test specific provider routing
        let decision = router.route_model("openrouter/z-ai/glm-4.5:fireworks").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "z-ai/glm-4.5");
        assert_eq!(decision.original_model, "openrouter/z-ai/glm-4.5:fireworks");
        assert_eq!(decision.provider_preference, Some("fireworks".to_string()));

        // Test nitro preference (high throughput)
        let decision = router.route_model("openrouter/meta-llama/llama-3.1-8b:nitro").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "meta-llama/llama-3.1-8b");
        assert_eq!(decision.original_model, "openrouter/meta-llama/llama-3.1-8b:nitro");
        assert_eq!(decision.provider_preference, Some("nitro".to_string()));

        // Test floor preference (lowest price)
        let decision = router.route_model("openrouter/gpt-4o:floor").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "gpt-4o");
        assert_eq!(decision.original_model, "openrouter/gpt-4o:floor");
        assert_eq!(decision.provider_preference, Some("floor".to_string()));

        // Test together provider
        let decision = router.route_model("openrouter/mistralai/mixtral-8x7b:together").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "mistralai/mixtral-8x7b");
        assert_eq!(decision.original_model, "openrouter/mistralai/mixtral-8x7b:together");
        assert_eq!(decision.provider_preference, Some("together".to_string()));
    }

    #[test]
    fn test_complex_model_paths_with_providers() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        // Test model names with multiple slashes and provider preference
        let decision = router.route_model("openrouter/company/team/model-v2:groq").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "company/team/model-v2");
        assert_eq!(decision.original_model, "openrouter/company/team/model-v2:groq");
        assert_eq!(decision.provider_preference, Some("groq".to_string()));

        // Test model with colons in the actual model name (only last colon should be provider)
        let decision = router.route_model("openrouter/model:with:colons:deepinfra").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "model:with:colons");
        assert_eq!(decision.original_model, "openrouter/model:with:colons:deepinfra");
        assert_eq!(decision.provider_preference, Some("deepinfra".to_string()));

        // Test no provider preference (should still work)
        let decision = router.route_model("openrouter/complex/path/to/model").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "complex/path/to/model");
        assert_eq!(decision.original_model, "openrouter/complex/path/to/model");
        assert_eq!(decision.provider_preference, None);
    }

    #[test]
    fn test_anthropic_provider_routing() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        // Test Anthropic provider with preference (even though it might not be used)
        let decision = router.route_model("anthropic/claude-3.5-sonnet:anthropic").unwrap();
        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3.5-sonnet");
        assert_eq!(decision.original_model, "anthropic/claude-3.5-sonnet:anthropic");
        assert_eq!(decision.provider_preference, Some("anthropic".to_string()));
    }

    #[test]
    fn test_edge_cases_provider_routing() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        // Test empty provider preference (colon at end)
        let decision = router.route_model("openrouter/model-name:").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "model-name");
        assert_eq!(decision.original_model, "openrouter/model-name:");
        assert_eq!(decision.provider_preference, Some("".to_string()));
        assert_eq!(decision.query_params, None);

        // Test model name ending with colon but no preference
        let decision = router.route_model("openrouter/model:name").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "model");
        assert_eq!(decision.original_model, "openrouter/model:name");
        assert_eq!(decision.provider_preference, Some("name".to_string()));
        assert_eq!(decision.query_params, None);
    }

    #[test]
    fn test_query_string_parsing() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        // Test basic query string parsing
        let decision = router.route_model("anthropic/claude-3.5-sonnet?think=1000").unwrap();
        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3.5-sonnet");
        assert_eq!(decision.original_model, "anthropic/claude-3.5-sonnet?think=1000");
        assert_eq!(decision.provider_preference, None);
        let params = decision.query_params.unwrap();
        assert_eq!(params.get("think"), Some(&"1000".to_string()));

        // Test multiple query parameters
        let decision = router.route_model("openrouter/z-ai/glm-4.5:fireworks?think=2000&effort=high&reasoning=true").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "z-ai/glm-4.5");
        assert_eq!(decision.provider_preference, Some("fireworks".to_string()));
        let params = decision.query_params.unwrap();
        assert_eq!(params.get("think"), Some(&"2000".to_string()));
        assert_eq!(params.get("effort"), Some(&"high".to_string()));
        assert_eq!(params.get("reasoning"), Some(&"true".to_string()));

        // Test query parameter without value
        let decision = router.route_model("gemini/gemini-2.0-flash-thinking-exp?thoughts").unwrap();
        assert_eq!(decision.provider, "gemini");
        assert_eq!(decision.model, "gemini-2.0-flash-thinking-exp");
        let params = decision.query_params.unwrap();
        assert_eq!(params.get("thoughts"), Some(&"true".to_string()));

        // Test empty query string
        let decision = router.route_model("anthropic/claude-3-opus?").unwrap();
        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3-opus");
        assert_eq!(decision.query_params, None);
    }

    #[test]
    fn test_complex_model_names_with_query_params() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        // Test complex model path with provider preference and query params
        let decision = router.route_model("openrouter/company/team/model-v2:together?think=5000&thoughts=true").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "company/team/model-v2");
        assert_eq!(decision.provider_preference, Some("together".to_string()));
        let params = decision.query_params.unwrap();
        assert_eq!(params.get("think"), Some(&"5000".to_string()));
        assert_eq!(params.get("thoughts"), Some(&"true".to_string()));

        // Test model with colons in name plus query params
        let decision = router.route_model("openrouter/model:with:colons:deepinfra?effort=medium").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "model:with:colons");
        assert_eq!(decision.provider_preference, Some("deepinfra".to_string()));
        let params = decision.query_params.unwrap();
        assert_eq!(params.get("effort"), Some(&"medium".to_string()));
    }

    #[test]
    fn test_simple_models_with_query_params() {
        let config = create_test_config();
        let router = NameBasedRouter::new(config);

        // Test simple model names with query parameters
        let decision = router.route_model("claude-3-sonnet?think=3000").unwrap();
        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3-sonnet");
        assert_eq!(decision.provider_preference, None);
        let params = decision.query_params.unwrap();
        assert_eq!(params.get("think"), Some(&"3000".to_string()));

        // Test o1 models (routed to OpenRouter) with effort parameter
        let decision = router.route_model("o1-preview?effort=high").unwrap();
        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "o1-preview");
        let params = decision.query_params.unwrap();
        assert_eq!(params.get("effort"), Some(&"high".to_string()));

        // Test unknown model with query params (should use default)
        let decision = router.route_model("unknown-model?reasoning=true").unwrap();
        assert_eq!(decision.provider, "openrouter"); // default from config
        assert_eq!(decision.model, "unknown-model");
        let params = decision.query_params.unwrap();
        assert_eq!(params.get("reasoning"), Some(&"true".to_string()));
    }
}
