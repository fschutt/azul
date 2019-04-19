//! Shared datatypes for azul-* crates

extern crate azul_css;
extern crate gleam;

pub mod app;
pub mod async;
pub mod callbacks;
pub mod dom;
pub mod id_tree;
pub mod window;

mod stack_checked_pointer;
mod ui_state;

type FastHashMap<T, U> = ::std::collections::HashMap<T, U>;
type FastHashSet<T> = ::std::collections::HashSet<T>;