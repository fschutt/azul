//! Shared datatypes for azul-* crates

extern crate azul_css;
extern crate gleam;
#[cfg(feature = "css_parser")]
extern crate azul_css_parser;

pub mod app_resources;
pub mod callbacks;
pub mod display_list;
pub mod dom;
pub mod diff;
pub mod gl;
pub mod id_tree;
pub mod ui_description;
pub mod ui_state;
pub mod ui_solver;
pub mod style;
pub mod task;
pub mod window;
pub mod window_state;

mod stack_checked_pointer;

// Typedef for possible faster implementation of hashing
pub type FastHashMap<T, U> = ::std::collections::HashMap<T, U>;
pub type FastHashSet<T> = ::std::collections::HashSet<T>;
