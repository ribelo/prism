use crate::{
    error::Result,
    router::traits::{PrioritizedRouter, RouteRequest, Router, RouterPriority, RoutingDecision},
};
use async_trait::async_trait;

/// Model-based router that routes requests based on model name patterns and formats
pub struct ModelRouter {
    default_provider: String,
}

impl ModelRouter {
    pub fn new(default_provider: String) -> Self {
        Self { default_provider }
    }

    /// Parse provider/model format (e.g., "openrouter/z-ai/glm-4.5")
    fn parse_provider_model_format(&self, model_name: &str) -> Option<(String, String)> {
        if let Some(slash_pos) = model_name.find('/') {
            let provider = model_name[..slash_pos].to_string();
            let actual_model = model_name[slash_pos + 1..].to_string();
            Some((provider, actual_model))
        } else {
            None
        }
    }

    /// Infer provider from model name patterns
    fn infer_provider_from_pattern(&self, model_name: &str) -> String {
        // Common model name patterns
        if model_name.starts_with("claude-") {
            return "anthropic".to_string();
        }

        if model_name.starts_with("gemini-") {
            return "gemini".to_string();
        }

        if model_name.starts_with("gpt-") || model_name.starts_with("text-") {
            return "openrouter".to_string(); // Route OpenAI models through OpenRouter
        }

        // Default to configured default provider
        self.default_provider.clone()
    }
}

#[async_trait]
impl Router for ModelRouter {
    async fn route(&self, request: &RouteRequest) -> Result<RoutingDecision> {
        let model_name = &request.model;

        // First priority: Check if model name has provider/model format
        if let Some((provider, actual_model)) = self.parse_provider_model_format(model_name) {
            let reason = format!(
                "Explicit provider/model format: {}/{}",
                provider, actual_model
            );
            return Ok(
                RoutingDecision::new(provider, actual_model, model_name.clone())
                    .with_confidence(0.95)
                    .with_reason(reason),
            );
        }

        // Second priority: Check provider hint from request
        if let Some(provider_hint) = &request.provider_hint {
            return Ok(RoutingDecision::new(
                provider_hint.clone(),
                model_name.clone(),
                model_name.clone(),
            )
            .with_confidence(0.9)
            .with_reason(format!("Provider hint: {}", provider_hint)));
        }

        // Third priority: Infer from model name patterns
        let provider = self.infer_provider_from_pattern(model_name);
        let confidence = if provider != self.default_provider {
            0.8
        } else {
            0.5
        };
        let reason = if provider != self.default_provider {
            format!("Model name pattern match: {} -> {}", model_name, provider)
        } else {
            format!("Default provider fallback: {}", provider)
        };

        Ok(
            RoutingDecision::new(provider, model_name.clone(), model_name.clone())
                .with_confidence(confidence)
                .with_reason(reason),
        )
    }

    fn name(&self) -> &'static str {
        "ModelRouter"
    }

    async fn can_handle(&self, _request: &RouteRequest) -> bool {
        // ModelRouter can always provide a routing decision (via fallback)
        true
    }

    fn supported_providers(&self) -> Vec<String> {
        vec![
            "anthropic".to_string(),
            "openrouter".to_string(),
            "gemini".to_string(),
            self.default_provider.clone(),
        ]
    }
}

impl PrioritizedRouter for ModelRouter {
    fn priority(&self) -> RouterPriority {
        RouterPriority::Pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::traits::RouteRequest;

    fn create_test_router() -> ModelRouter {
        ModelRouter::new("openrouter".to_string())
    }

    #[tokio::test]
    async fn test_provider_model_format_routing() {
        let router = create_test_router();

        // Test openrouter/model format
        let request = RouteRequest::new("openrouter/z-ai/glm-4.5".to_string());
        let decision = router.route(&request).await.unwrap();

        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "z-ai/glm-4.5");
        assert_eq!(decision.original_model, "openrouter/z-ai/glm-4.5");
        assert_eq!(decision.confidence, 0.95);
        assert!(decision.reason.contains("Explicit provider/model format"));
    }

    #[tokio::test]
    async fn test_anthropic_model_format() {
        let router = create_test_router();

        let request = RouteRequest::new("anthropic/claude-3-sonnet".to_string());
        let decision = router.route(&request).await.unwrap();

        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3-sonnet");
        assert_eq!(decision.original_model, "anthropic/claude-3-sonnet");
    }

    #[tokio::test]
    async fn test_provider_hint_routing() {
        let router = create_test_router();

        let request =
            RouteRequest::new("some-model".to_string()).with_provider_hint("anthropic".to_string());
        let decision = router.route(&request).await.unwrap();

        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "some-model");
        assert_eq!(decision.confidence, 0.9);
        assert!(decision.reason.contains("Provider hint"));
    }

    #[tokio::test]
    async fn test_model_pattern_routing() {
        let router = create_test_router();

        // Test Claude model pattern
        let request = RouteRequest::new("claude-3-sonnet".to_string());
        let decision = router.route(&request).await.unwrap();

        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.model, "claude-3-sonnet");
        assert_eq!(decision.confidence, 0.8);
        assert!(decision.reason.contains("Model name pattern match"));

        // Test GPT model pattern
        let request = RouteRequest::new("gpt-4".to_string());
        let decision = router.route(&request).await.unwrap();

        assert_eq!(decision.provider, "openrouter");
        assert_eq!(decision.model, "gpt-4");
    }

    #[tokio::test]
    async fn test_default_provider_fallback() {
        let router = create_test_router();

        let request = RouteRequest::new("unknown-model".to_string());
        let decision = router.route(&request).await.unwrap();

        assert_eq!(decision.provider, "openrouter"); // default
        assert_eq!(decision.model, "unknown-model");
        assert_eq!(decision.confidence, 0.5);
        assert!(decision.reason.contains("Default provider fallback"));
    }

    #[tokio::test]
    async fn test_can_handle() {
        let router = create_test_router();
        let request = RouteRequest::new("any-model".to_string());

        assert!(router.can_handle(&request).await);
    }

    #[tokio::test]
    async fn test_supported_providers() {
        let router = create_test_router();
        let providers = router.supported_providers();

        assert!(providers.contains(&"anthropic".to_string()));
        assert!(providers.contains(&"openrouter".to_string()));
        assert!(providers.contains(&"gemini".to_string()));
    }
}
