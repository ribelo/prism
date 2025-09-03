// Simple name-based router - the only routing we need
pub mod name_based;
pub mod model_router;

// Export the router and its types
pub use name_based::{NameBasedRouter, RoutingDecision};
pub use model_router::ModelRouter;
