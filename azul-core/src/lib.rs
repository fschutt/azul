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
pub mod id_tree;
pub mod window;
pub mod ui_state;
pub mod gl;
pub mod display_list;

mod stack_checked_pointer;

#[cfg(feature = "faster_hashing")]
extern crate azul_dependencies;
#[cfg(feature = "faster_hashing")]
use azul_dependencies::twox_hash;

// Faster implementation of a HashMap (optional, disabled by default, turn on with --feature="faster-hashing")
#[cfg(feature = "faster_hashing")]
pub type FastHashMap<T, U> = ::std::collections::HashMap<T, U, ::std::hash::BuildHasherDefault<twox_hash::XxHash>>;
#[cfg(feature = "faster_hashing")]
pub type FastHashSet<T> = ::std::collections::HashSet<T, ::std::hash::BuildHasherDefault<twox_hash::XxHash>>;
#[cfg(not(feature = "faster_hashing"))]
pub type FastHashMap<T, U> = ::std::collections::HashMap<T, U>;
#[cfg(not(feature = "faster_hashing"))]
pub type FastHashSet<T> = ::std::collections::HashSet<T>;