//! Patch generation module
//!
//! This module handles generating patches for api.json with proper
//! deduplication and validation.

pub mod path_correction;
pub mod deduplication;

pub use path_correction::{PathCorrection, generate_path_corrections};
pub use deduplication::{deduplicate_patches, PatchSet};
