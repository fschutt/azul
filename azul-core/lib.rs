//! Shared datatypes for azul-* crates

extern crate azul_css;
extern crate gleam;
#[cfg(feature = "css_parser")]
extern crate azul_css_parser;

pub mod app_resources;
/// Type definitions for various types of callbacks, as well as focus and scroll handling
pub mod callbacks;
pub mod display_list;
pub mod dom;
/// Algorithms to create git-like diffs between two doms in linear time
pub mod diff;
pub mod gl;
pub mod id_tree;
pub mod style;
pub mod traits;
pub mod task;
pub mod ui_description;
pub mod ui_state;
pub mod ui_solver;
pub mod window;
pub mod window_state;

// Typedef for possible faster implementation of hashing
pub type FastHashMap<T, U> = ::std::collections::HashMap<T, U>;
pub type FastHashSet<T> = ::std::collections::HashSet<T>;
