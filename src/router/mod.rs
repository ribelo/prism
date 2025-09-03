// Simple name-based router - the only routing we need
pub mod name_based;

// Export the router and its types
pub use name_based::{NameBasedRouter, RoutingDecision};
