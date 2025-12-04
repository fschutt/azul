//! API analysis module
//!
//! This module handles analysis of api.json to extract type references
//! and determine which types are used/unused.

pub mod extractor;
pub mod reachability;

pub use extractor::{extract_types_from_api, ApiTypeExtractor};
pub use reachability::{find_unused_types, ReachabilityAnalysis};
