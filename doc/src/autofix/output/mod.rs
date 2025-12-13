//! Output generation module
//!
//! This module handles generating reports and applying patches.

pub mod apply;
pub mod report;

pub use apply::{apply_patches, ApplyResult};
pub use report::{generate_report, Report};
