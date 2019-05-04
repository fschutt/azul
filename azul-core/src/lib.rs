//! Shared datatypes for azul-* crates

pub extern crate azul_css as css;
pub extern crate gleam;
#[cfg(feature = "css_parser")]
pub extern crate azul_css_parser as css_parser;

pub mod app;
pub mod app_resources;
pub mod async;
pub mod callbacks;
pub mod dom;
pub mod id_tree;
pub mod window;
pub mod ui_state;
pub mod gl;
pub mod display_list;

mod stack_checked_pointer;

type FastHashMap<T, U> = ::std::collections::HashMap<T, U>;
type FastHashSet<T> = ::std::collections::HashSet<T>;