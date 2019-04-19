//! Shared datatypes for azul-* crates

extern crate azul_css;
extern crate gleam;

pub mod app;
pub mod app_resources;
pub mod async;
pub mod callbacks;
pub mod dom;
pub mod id_tree;
pub mod window;
pub mod ui_state;

mod stack_checked_pointer;

type FastHashMap<T, U> = ::std::collections::HashMap<T, U>;
type FastHashSet<T> = ::std::collections::HashSet<T>;