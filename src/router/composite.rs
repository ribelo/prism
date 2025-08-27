use crate::{
    error::{Result, SetuError},
    router::traits::{PrioritizedRouter, RouteRequest, Router, RoutingDecision},
};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, warn};

/// Composite router that combines multiple routing strategies
pub struct CompositeRouter {
    /// Ordered list of routers to try (by priority)
    routers: Vec<Arc<dyn PrioritizedRouter>>,

    /// Whether to enable fallback routing (try all routers if primary fails)
    enable_fallback: bool,

    /// Minimum confidence threshold for routing decisions
    min_confidence: f64,
}

impl CompositeRouter {
    pub fn new() -> Self {
        Self {
            routers: Vec::new(),
            enable_fallback: true,
            min_confidence: 0.0,
        }
    }

    pub fn with_router(mut self, router: Arc<dyn PrioritizedRouter>) -> Self {
        self.routers.push(router);
        // Keep routers sorted by priority
        self.routers.sort_by_key(|r| r.priority());
        self
    }

    pub fn with_fallback(mut self, enable: bool) -> Self {
        self.enable_fallback = enable;
        self
    }

    pub fn with_min_confidence(mut self, threshold: f64) -> Self {
        self.min_confidence = threshold;
        self
    }

    /// Add multiple routers at once
    pub fn with_routers(mut self, routers: Vec<Arc<dyn PrioritizedRouter>>) -> Self {
        self.routers.extend(routers);
        self.routers.sort_by_key(|r| r.priority());
        self
    }

    /// Try routing with a specific router and validate the result
    async fn try_router(
        &self,
        router: &Arc<dyn PrioritizedRouter>,
        request: &RouteRequest,
    ) -> Option<RoutingDecision> {
        // Check if router can handle the request
        if !router.can_handle(request).await {
            debug!(
                "Router {} cannot handle request for model {}",
                router.name(),
                request.model
            );
            return None;
        }

        // Try to route
        match router.route(request).await {
            Ok(decision) => {
                debug!(
                    "Router {} produced decision: {} -> {} (confidence: {:.2})",
                    router.name(),
                    request.model,
                    decision.provider,
                    decision.confidence
                );

                // Check confidence threshold
                if decision.confidence >= self.min_confidence {
                    Some(decision)
                } else {
                    debug!(
                        "Router {} decision below confidence threshold ({:.2} < {:.2})",
                        router.name(),
                        decision.confidence,
                        self.min_confidence
                    );
                    None
                }
            }
            Err(e) => {
                debug!("Router {} failed to route: {}", router.name(), e);
                None
            }
        }
    }
}

impl Default for CompositeRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Router for CompositeRouter {
    async fn route(&self, request: &RouteRequest) -> Result<RoutingDecision> {
        if self.routers.is_empty() {
            return Err(SetuError::Other(
                "No routers configured in CompositeRouter".to_string(),
            ));
        }

        debug!(
            "CompositeRouter routing request for model: {}",
            request.model
        );

        let mut best_decision: Option<RoutingDecision> = None;
        let mut attempted_routers = Vec::new();

        // When fallback is disabled, only try the first router that can handle the request
        if !self.enable_fallback {
            for router in &self.routers {
                attempted_routers.push(router.name());

                if router.can_handle(request).await {
                    debug!(
                        "Router {} can handle request, trying (fallback disabled)",
                        router.name()
                    );
                    // Try to route directly since we already checked can_handle
                    match router.route(request).await {
                        Ok(decision) => {
                            debug!(
                                "Router {} produced decision: {} -> {} (confidence: {:.2})",
                                router.name(),
                                request.model,
                                decision.provider,
                                decision.confidence
                            );

                            // Check confidence threshold
                            if decision.confidence >= self.min_confidence {
                                best_decision = Some(decision);
                            } else {
                                debug!(
                                    "Router {} decision below confidence threshold ({:.2} < {:.2})",
                                    router.name(),
                                    decision.confidence,
                                    self.min_confidence
                                );
                            }
                        }
                        Err(e) => {
                            debug!("Router {} failed to route: {}", router.name(), e);
                        }
                    }
                    // Stop after trying the first router that can handle, regardless of success
                    break;
                } else {
                    debug!(
                        "Router {} cannot handle request, skipping (fallback disabled)",
                        router.name()
                    );
                }
            }
        } else {
            // When fallback is enabled, try all routers in priority order
            for router in &self.routers {
                attempted_routers.push(router.name());

                if let Some(decision) = self.try_router(router, request).await {
                    debug!(
                        "Router {} succeeded with confidence {:.2}",
                        router.name(),
                        decision.confidence
                    );

                    // If this is the highest priority router that succeeded, use its decision
                    if best_decision.is_none()
                        || decision.confidence > best_decision.as_ref().unwrap().confidence
                    {
                        best_decision = Some(decision);

                        // Stop if we have very high confidence
                        if best_decision.as_ref().unwrap().confidence >= 0.9 {
                            break;
                        }
                    }
                }
            }
        }

        match best_decision {
            Some(mut decision) => {
                // Enhance decision with composite router metadata
                let composite_reason = format!(
                    "CompositeRouter: {} (tried: {})",
                    decision.reason,
                    attempted_routers.join(", ")
                );
                decision.reason = composite_reason;

                Ok(decision)
            }
            None => {
                let error_msg = format!(
                    "No router could handle request for model '{}' (tried: {})",
                    request.model,
                    attempted_routers.join(", ")
                );
                warn!("{}", error_msg);
                Err(SetuError::Other(error_msg))
            }
        }
    }

    fn name(&self) -> &'static str {
        "CompositeRouter"
    }

    async fn can_handle(&self, request: &RouteRequest) -> bool {
        // Can handle if any router can handle
        for router in &self.routers {
            if router.can_handle(request).await {
                return true;
            }
        }
        false
    }

    fn supported_providers(&self) -> Vec<String> {
        // Union of all supported providers from all routers
        let mut providers = std::collections::HashSet::new();
        for router in &self.routers {
            providers.extend(router.supported_providers().into_iter());
        }
        providers.into_iter().collect()
    }
}

/// Builder for creating a CompositeRouter with common configurations
pub struct CompositeRouterBuilder {
    router: CompositeRouter,
}

impl CompositeRouterBuilder {
    pub fn new() -> Self {
        Self {
            router: CompositeRouter::new(),
        }
    }

    /// Create a standard router with provider and model routing
    pub fn standard(available_providers: Vec<String>, default_provider: String) -> Self {
        use crate::router::{ModelRouter, ProviderRouter};

        let provider_router = Arc::new(ProviderRouter::new(available_providers));
        let model_router = Arc::new(ModelRouter::new(default_provider));

        Self {
            router: CompositeRouter::new()
                .with_router(provider_router)
                .with_router(model_router)
                .with_fallback(true)
                .with_min_confidence(0.0),
        }
    }

    pub fn with_router(mut self, router: Arc<dyn PrioritizedRouter>) -> Self {
        self.router = self.router.with_router(router);
        self
    }

    pub fn with_fallback(mut self, enable: bool) -> Self {
        self.router = self.router.with_fallback(enable);
        self
    }

    pub fn with_min_confidence(mut self, threshold: f64) -> Self {
        self.router = self.router.with_min_confidence(threshold);
        self
    }

    pub fn build(self) -> CompositeRouter {
        self.router
    }
}

impl Default for CompositeRouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::traits::{RouteRequest, RouterPriority};
    use std::sync::Arc;

    // Mock router for testing
    struct MockRouter {
        name: &'static str,
        priority: RouterPriority,
        can_handle_result: bool,
        route_result: Option<Result<RoutingDecision>>,
    }

    impl MockRouter {
        fn new(name: &'static str, priority: RouterPriority) -> Self {
            Self {
                name,
                priority,
                can_handle_result: true,
                route_result: Some(Ok(RoutingDecision::new(
                    "test_provider".to_string(),
                    "test_model".to_string(),
                    "test_model".to_string(),
                )
                .with_reason(format!("Routed by {}", name)))),
            }
        }

        fn with_can_handle(mut self, result: bool) -> Self {
            self.can_handle_result = result;
            self
        }

        fn with_route_result(mut self, result: Option<Result<RoutingDecision>>) -> Self {
            self.route_result = result;
            self
        }
    }

    #[async_trait]
    impl Router for MockRouter {
        async fn route(&self, _request: &RouteRequest) -> Result<RoutingDecision> {
            match self.route_result.as_ref() {
                Some(Ok(decision)) => Ok(decision.clone()),
                Some(Err(e)) => Err(SetuError::Other(e.to_string())),
                None => Err(SetuError::Other(
                    "Mock router configured to fail".to_string(),
                )),
            }
        }

        fn name(&self) -> &'static str {
            self.name
        }

        async fn can_handle(&self, _request: &RouteRequest) -> bool {
            self.can_handle_result
        }
    }

    impl PrioritizedRouter for MockRouter {
        fn priority(&self) -> RouterPriority {
            self.priority
        }
    }

    #[tokio::test]
    async fn test_composite_router_priority_ordering() {
        let low_priority = Arc::new(MockRouter::new("low", RouterPriority::Default));
        let high_priority = Arc::new(MockRouter::new("high", RouterPriority::Explicit));

        let composite = CompositeRouter::new()
            .with_router(low_priority) // Add low priority first
            .with_router(high_priority); // Add high priority second

        let request = RouteRequest::new("test-model".to_string());
        let decision = composite.route(&request).await.unwrap();

        // Should use high priority router
        assert!(decision.reason.contains("Routed by high"));
    }

    #[tokio::test]
    async fn test_composite_router_fallback() {
        let failing_router =
            Arc::new(MockRouter::new("failing", RouterPriority::Explicit).with_route_result(None));
        let fallback_router = Arc::new(MockRouter::new("fallback", RouterPriority::Default));

        let composite = CompositeRouter::new()
            .with_router(failing_router)
            .with_router(fallback_router)
            .with_fallback(true);

        let request = RouteRequest::new("test-model".to_string());
        let decision = composite.route(&request).await.unwrap();

        // Should use fallback router
        assert!(decision.reason.contains("Routed by fallback"));
    }

    #[tokio::test]
    async fn test_composite_router_no_fallback() {
        let failing_router = Arc::new(
            MockRouter::new("failing", RouterPriority::Explicit)
                .with_can_handle(true) // Can handle but will fail
                .with_route_result(Some(Err(SetuError::Other(
                    "Router configured to fail".to_string(),
                )))),
        );
        let fallback_router = Arc::new(MockRouter::new("fallback", RouterPriority::Default));

        let composite = CompositeRouter::new()
            .with_router(failing_router)
            .with_router(fallback_router)
            .with_fallback(false);

        let request = RouteRequest::new("test-model".to_string());
        let result = composite.route(&request).await;

        // Should fail because first router that can handle fails and fallback is disabled
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_composite_router_confidence_threshold() {
        let low_confidence_router = Arc::new({
            let decision = RoutingDecision::new(
                "provider".to_string(),
                "model".to_string(),
                "model".to_string(),
            )
            .with_confidence(0.3);

            MockRouter::new("low_confidence", RouterPriority::Explicit)
                .with_route_result(Some(Ok(decision)))
        });

        let composite = CompositeRouter::new()
            .with_router(low_confidence_router)
            .with_min_confidence(0.5); // Higher than router's confidence

        let request = RouteRequest::new("test-model".to_string());
        let result = composite.route(&request).await;

        // Should fail due to confidence threshold
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_standard_builder() {
        let composite = CompositeRouterBuilder::standard(
            vec!["anthropic".to_string(), "openrouter".to_string()],
            "openrouter".to_string(),
        )
        .build();

        // Test with provider hint (should use ProviderRouter)
        let request =
            RouteRequest::new("some-model".to_string()).with_provider_hint("anthropic".to_string());
        let decision = composite.route(&request).await.unwrap();
        assert_eq!(decision.provider, "anthropic");

        // Test with model pattern (should use ModelRouter)
        let request = RouteRequest::new("claude-3-sonnet".to_string());
        let decision = composite.route(&request).await.unwrap();
        assert_eq!(decision.provider, "anthropic");
    }

    #[tokio::test]
    async fn test_empty_router_list() {
        let composite = CompositeRouter::new();
        let request = RouteRequest::new("test-model".to_string());
        let result = composite.route(&request).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No routers configured")
        );
    }
}
