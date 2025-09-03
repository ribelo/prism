// New trait-based router system
pub mod composite;
pub mod factory;
pub mod model;
pub mod provider;
pub mod traits;

// Legacy module for backward compatibility
pub mod name_based;

// Export new router system
pub use composite::{CompositeRouter, CompositeRouterBuilder};
pub use factory::{RouterAdapter, RouterFactory};
pub use model::ModelRouter;
pub use provider::ProviderRouter;
pub use traits::{
    AlternativeProvider, PrioritizedRouter, RouteRequest, Router, RouterPriority, RoutingDecision,
};

// Legacy exports for backward compatibility
pub use name_based::NameBasedRouter;
