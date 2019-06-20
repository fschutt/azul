//! Shared datatypes for azul-* crates

extern crate azul_css;
extern crate gleam;
#[cfg(feature = "css_parser")]
extern crate azul_css_parser;

pub mod app;
pub mod app_resources;
pub mod async;
pub mod callbacks;
pub mod dom;
pub mod diff;
pub mod id_tree;
pub mod window;
pub mod ui_state;
pub mod gl;
pub mod display_list;
pub mod ui_solver;
pub mod style;
pub mod ui_description;

mod stack_checked_pointer;

// Typedef for possible faster implementation of hashing
pub type FastHashMap<T, U> = ::std::collections::HashMap<T, U>;
pub type FastHashSet<T> = ::std::collections::HashSet<T>;
