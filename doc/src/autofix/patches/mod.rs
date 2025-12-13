//! Patch generation module
//!
//! This module handles generating patches for api.json with proper
//! deduplication and validation.

pub mod deduplication;
pub mod path_correction;

pub use deduplication::{deduplicate_patches, PatchSet};
pub use path_correction::{generate_path_corrections, PathCorrection};
