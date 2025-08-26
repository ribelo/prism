use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request context for routing decisions
#[derive(Debug, Clone)]
pub struct RouteRequest {
    /// The model name from the request (e.g., "claude-3-sonnet", "openrouter/z-ai/glm-4.5")
    pub model: String,

    /// Optional explicit provider hint (from headers, params, etc.)
    pub provider_hint: Option<String>,

    /// Required capabilities (streaming, tools, vision, etc.)
    pub capabilities: Vec<String>,

    /// Additional request metadata for routing decisions
    pub metadata: HashMap<String, String>,
}

impl RouteRequest {
    pub fn new(model: String) -> Self {
        Self {
            model,
            provider_hint: None,
            capabilities: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_provider_hint(mut self, provider: String) -> Self {
        self.provider_hint = Some(provider);
        self
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Enhanced routing decision with metadata and alternatives
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// The target provider to route to
    pub provider: String,

    /// The model name to send to the provider (may be transformed)
    pub model: String,

    /// The original model name from the request
    pub original_model: String,

    /// Confidence score for this routing decision (0.0 to 1.0)
    pub confidence: f64,

    /// Reason for this routing decision (for debugging/logging)
    pub reason: String,

    /// Alternative providers that could handle this request (for fallback)
    pub alternatives: Vec<AlternativeProvider>,

    /// Provider-specific transformations or hints needed
    pub transformations: HashMap<String, String>,
}

impl RoutingDecision {
    pub fn new(provider: String, model: String, original_model: String) -> Self {
        Self {
            provider,
            model,
            original_model,
            confidence: 1.0,
            reason: "Default routing".to_string(),
            alternatives: Vec::new(),
            transformations: HashMap::new(),
        }
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence;
        self
    }

    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = reason;
        self
    }

    pub fn with_alternatives(mut self, alternatives: Vec<AlternativeProvider>) -> Self {
        self.alternatives = alternatives;
        self
    }

    pub fn with_transformation(mut self, key: String, value: String) -> Self {
        self.transformations.insert(key, value);
        self
    }
}

/// Alternative provider for fallback routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeProvider {
    pub provider: String,
    pub model: String,
    pub confidence: f64,
    pub reason: String,
}

impl AlternativeProvider {
    pub fn new(provider: String, model: String, confidence: f64, reason: String) -> Self {
        Self {
            provider,
            model,
            confidence,
            reason,
        }
    }
}

/// Core router trait - all routers must implement this interface
#[async_trait::async_trait]
pub trait Router: Send + Sync {
    /// Route a request to determine the target provider and model
    async fn route(&self, request: &RouteRequest) -> Result<RoutingDecision>;

    /// Get the name/type of this router (for debugging/logging)
    fn name(&self) -> &'static str;

    /// Check if this router can handle the given request
    async fn can_handle(&self, request: &RouteRequest) -> bool {
        // Default implementation: try to route and see if it succeeds
        self.route(request).await.is_ok()
    }

    /// Get supported providers by this router
    fn supported_providers(&self) -> Vec<String> {
        // Default implementation: empty list (router will determine dynamically)
        Vec::new()
    }
}

/// Router priority for composite routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouterPriority {
    /// Highest priority - explicit provider requests
    Explicit = 0,
    /// High priority - provider/model format parsing
    Structured = 1,
    /// Medium priority - model name pattern matching  
    Pattern = 2,
    /// Low priority - fallback/default routing
    Default = 3,
}

/// Trait for routers that can be prioritized in composite routing
pub trait PrioritizedRouter: Router {
    /// Get the priority of this router
    fn priority(&self) -> RouterPriority;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_request_builder() {
        let request = RouteRequest::new("claude-3-sonnet".to_string())
            .with_provider_hint("anthropic".to_string())
            .with_capabilities(vec!["streaming".to_string(), "tools".to_string()])
            .with_metadata("region".to_string(), "us-east-1".to_string());

        assert_eq!(request.model, "claude-3-sonnet");
        assert_eq!(request.provider_hint, Some("anthropic".to_string()));
        assert_eq!(request.capabilities, vec!["streaming", "tools"]);
        assert_eq!(
            request.metadata.get("region"),
            Some(&"us-east-1".to_string())
        );
    }

    #[test]
    fn test_routing_decision_builder() {
        let decision = RoutingDecision::new(
            "anthropic".to_string(),
            "claude-3-sonnet".to_string(),
            "claude-3-sonnet".to_string(),
        )
        .with_confidence(0.95)
        .with_reason("Model name pattern match".to_string())
        .with_transformation("endpoint".to_string(), "messages".to_string());

        assert_eq!(decision.provider, "anthropic");
        assert_eq!(decision.confidence, 0.95);
        assert_eq!(decision.reason, "Model name pattern match");
        assert_eq!(
            decision.transformations.get("endpoint"),
            Some(&"messages".to_string())
        );
    }

    #[test]
    fn test_alternative_provider() {
        let alt = AlternativeProvider::new(
            "openrouter".to_string(),
            "anthropic/claude-3-sonnet".to_string(),
            0.8,
            "Available through proxy".to_string(),
        );

        assert_eq!(alt.provider, "openrouter");
        assert_eq!(alt.model, "anthropic/claude-3-sonnet");
        assert_eq!(alt.confidence, 0.8);
    }
}
