use crate::{
    error::Result,
    router::traits::{
        AlternativeProvider, PrioritizedRouter, RouteRequest, Router, RouterPriority,
        RoutingDecision,
    },
};
use async_trait::async_trait;
use std::collections::HashMap;

/// Provider-based router that routes based on explicit provider requests
pub struct ProviderRouter {
    /// Map of provider aliases to canonical provider names
    provider_aliases: HashMap<String, String>,

    /// List of available providers in priority order
    available_providers: Vec<String>,

    /// Provider capabilities mapping
    provider_capabilities: HashMap<String, Vec<String>>,
}

impl ProviderRouter {
    pub fn new(available_providers: Vec<String>) -> Self {
        Self {
            provider_aliases: Self::default_provider_aliases(),
            available_providers,
            provider_capabilities: Self::default_provider_capabilities(),
        }
    }

    pub fn with_aliases(mut self, aliases: HashMap<String, String>) -> Self {
        self.provider_aliases.extend(aliases);
        self
    }

    pub fn with_capabilities(mut self, capabilities: HashMap<String, Vec<String>>) -> Self {
        self.provider_capabilities = capabilities;
        self
    }

    fn default_provider_aliases() -> HashMap<String, String> {
        let mut aliases = HashMap::new();

        // Common aliases
        aliases.insert("openai".to_string(), "openrouter".to_string()); // Route OpenAI through OpenRouter
        aliases.insert("gpt".to_string(), "openrouter".to_string());
        aliases.insert("claude".to_string(), "anthropic".to_string());
        aliases.insert("google".to_string(), "gemini".to_string());

        aliases
    }

    fn default_provider_capabilities() -> HashMap<String, Vec<String>> {
        let mut capabilities = HashMap::new();

        capabilities.insert(
            "anthropic".to_string(),
            vec![
                "streaming".to_string(),
                "tools".to_string(),
                "system_messages".to_string(),
                "json_schema".to_string(),
            ],
        );

        capabilities.insert(
            "openrouter".to_string(),
            vec![
                "streaming".to_string(),
                "tools".to_string(),
                "multiple_models".to_string(),
            ],
        );

        capabilities.insert(
            "gemini".to_string(),
            vec![
                "streaming".to_string(),
                "vision".to_string(),
                "tools".to_string(),
            ],
        );

        capabilities
    }

    /// Resolve provider alias to canonical name
    fn resolve_provider(&self, provider: &str) -> String {
        self.provider_aliases
            .get(provider)
            .cloned()
            .unwrap_or_else(|| provider.to_string())
    }

    /// Check if provider supports required capabilities
    fn supports_capabilities(&self, provider: &str, required: &[String]) -> bool {
        if required.is_empty() {
            return true;
        }

        let provider_caps = self
            .provider_capabilities
            .get(provider)
            .map(|caps| caps.as_slice())
            .unwrap_or(&[]);

        required.iter().all(|req| provider_caps.contains(req))
    }

    /// Find alternative providers that support the required capabilities
    fn find_alternatives(
        &self,
        required_capabilities: &[String],
        exclude: &str,
    ) -> Vec<AlternativeProvider> {
        let mut alternatives = Vec::new();

        for provider in &self.available_providers {
            if provider == exclude {
                continue;
            }

            if self.supports_capabilities(provider, required_capabilities) {
                let confidence = if required_capabilities.is_empty() {
                    0.8
                } else {
                    0.9
                };
                let reason = if required_capabilities.is_empty() {
                    "General provider alternative".to_string()
                } else {
                    format!(
                        "Supports required capabilities: {:?}",
                        required_capabilities
                    )
                };

                alternatives.push(AlternativeProvider::new(
                    provider.clone(),
                    format!("{}/{{model}}", provider), // Placeholder for model transformation
                    confidence,
                    reason,
                ));
            }
        }

        // Sort by confidence (descending)
        alternatives.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        alternatives
    }
}

#[async_trait]
impl Router for ProviderRouter {
    async fn route(&self, request: &RouteRequest) -> Result<RoutingDecision> {
        // Only handle requests with explicit provider hints
        let provider_hint = match &request.provider_hint {
            Some(hint) => hint,
            None => {
                return Err(crate::error::SetuError::Other(
                    "ProviderRouter requires explicit provider hint".to_string(),
                ));
            }
        };

        // Resolve provider alias
        let canonical_provider = self.resolve_provider(provider_hint);

        // Check if provider is available
        if !self.available_providers.contains(&canonical_provider) {
            return Err(crate::error::SetuError::Other(format!(
                "Provider '{}' is not available",
                canonical_provider
            )));
        }

        // Check capability requirements
        if !self.supports_capabilities(&canonical_provider, &request.capabilities) {
            return Err(crate::error::SetuError::Other(format!(
                "Provider '{}' does not support required capabilities: {:?}",
                canonical_provider, request.capabilities
            )));
        }

        // Find alternatives
        let alternatives = self.find_alternatives(&request.capabilities, &canonical_provider);

        let confidence = if provider_hint == &canonical_provider {
            1.0
        } else {
            0.95
        };
        let reason = if provider_hint == &canonical_provider {
            format!("Explicit provider request: {}", canonical_provider)
        } else {
            format!(
                "Provider alias resolved: {} -> {}",
                provider_hint, canonical_provider
            )
        };

        Ok(RoutingDecision::new(
            canonical_provider,
            request.model.clone(),
            request.model.clone(),
        )
        .with_confidence(confidence)
        .with_reason(reason)
        .with_alternatives(alternatives))
    }

    fn name(&self) -> &'static str {
        "ProviderRouter"
    }

    async fn can_handle(&self, request: &RouteRequest) -> bool {
        // Only handle requests with provider hints
        request.provider_hint.is_some()
    }

    fn supported_providers(&self) -> Vec<String> {
        self.available_providers.clone()
    }
}

impl PrioritizedRouter for ProviderRouter {
    fn priority(&self) -> RouterPriority {
        RouterPriority::Explicit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::traits::RouteRequest;

    fn create_test_router() -> ProviderRouter {
        ProviderRouter::new(vec![
            "anthropic".to_string(),
            "openrouter".to_string(),
            "gemini".to_string(),
        ])
    }

    #[tokio::test]
    async fn test_explicit_provider_routing() {
        let router = create_test_router();

        let request = RouteRequest::new("claude-3-sonnet".to_string())
            .with_provider_hint("anthropic".to_string());

        let decision = router.route(&request).await.unwrap();

        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3-sonnet");
        assert_eq!(decision.confidence, 1.0);
        assert!(decision.reason.contains("Explicit provider request"));
    }

    #[tokio::test]
    async fn test_provider_alias_resolution() {
        let router = create_test_router();

        let request =
            RouteRequest::new("gpt-4".to_string()).with_provider_hint("openai".to_string()); // Alias for openrouter

        let decision = router.route(&request).await.unwrap();

        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.confidence, 0.95);
        assert!(
            decision
                .reason
                .contains("Provider alias resolved: openai -> openrouter")
        );
    }

    #[tokio::test]
    async fn test_capability_requirements() {
        let router = create_test_router();

        let request = RouteRequest::new("some-model".to_string())
            .with_provider_hint("anthropic".to_string())
            .with_capabilities(vec!["streaming".to_string(), "tools".to_string()]);

        let decision = router.route(&request).await.unwrap();
        assert_eq!(decision.provider, "anthropic");

        // Test unsupported capability
        let request = RouteRequest::new("some-model".to_string())
            .with_provider_hint("anthropic".to_string())
            .with_capabilities(vec!["unsupported_capability".to_string()]);

        assert!(router.route(&request).await.is_err());
    }

    #[tokio::test]
    async fn test_unavailable_provider() {
        let router = create_test_router();

        let request = RouteRequest::new("some-model".to_string())
            .with_provider_hint("unavailable_provider".to_string());

        assert!(router.route(&request).await.is_err());
    }

    #[tokio::test]
    async fn test_alternatives_generation() {
        let router = create_test_router();

        let request = RouteRequest::new("some-model".to_string())
            .with_provider_hint("anthropic".to_string())
            .with_capabilities(vec!["streaming".to_string()]);

        let decision = router.route(&request).await.unwrap();

        // Should have alternatives that support streaming
        assert!(!decision.alternatives.is_empty());

        // Alternatives should not include the primary provider
        assert!(
            !decision
                .alternatives
                .iter()
                .any(|alt| alt.provider == "anthropic")
        );
    }

    #[tokio::test]
    async fn test_can_handle() {
        let router = create_test_router();

        // Should handle requests with provider hints
        let request_with_hint =
            RouteRequest::new("model".to_string()).with_provider_hint("anthropic".to_string());
        assert!(router.can_handle(&request_with_hint).await);

        // Should not handle requests without provider hints
        let request_without_hint = RouteRequest::new("model".to_string());
        assert!(!router.can_handle(&request_without_hint).await);
    }

    #[tokio::test]
    async fn test_no_provider_hint_error() {
        let router = create_test_router();

        let request = RouteRequest::new("some-model".to_string());
        let result = router.route(&request).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("requires explicit provider hint")
        );
    }
}
