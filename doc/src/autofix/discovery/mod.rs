//! Workspace discovery module
//!
//! This module handles discovering types in the Rust workspace.
//! It keeps type discovery separate from api.json parsing.

pub mod crates;
pub mod parallel;
pub mod workspace;

pub use crates::{CratePriority, CRATE_PRIORITIES};
pub use parallel::{discover_workspace_types_parallel, ParallelDiscoveryConfig};
pub use workspace::{discover_workspace_types, TypeLocation, WorkspaceIndex};
