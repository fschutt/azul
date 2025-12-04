//! Output generation module
//!
//! This module handles generating reports and applying patches.

pub mod report;
pub mod apply;

pub use report::{generate_report, Report};
pub use apply::{apply_patches, ApplyResult};
