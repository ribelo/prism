// Simple name-based router - the only routing we need
pub mod model_router;
pub mod name_based;

// Export the router and its types
pub use model_router::ModelRouter;
pub use name_based::{NameBasedRouter, RoutingDecision};
