#![deny(unused_must_use)]
#![allow(dead_code)]
#![allow(unused_imports)]

extern crate webrender;
extern crate cassowary;
extern crate twox_hash;
extern crate glium;
extern crate gleam;
extern crate euclid;
extern crate simplecss;

/// Global application (Initialization starts here)
pub mod app;
/// Wrapper for the application data & application state
pub mod app_state;
/// Styling & CSS parsing
pub mod css;
/// DOM / HTML node handling
pub mod dom;
/// The layout traits for creating a layout-able application
pub mod traits;
/// Window handling
pub mod window;
/// Input handling (mostly glium)
mod input;
/// UI Description & display list handling (webrender)
mod ui_description;
/// Constraint handling
mod constraints;
/// Converts the UI description (the styled HTML nodes)
/// to an actual display list (+ layout)
mod display_list;
/// CSS parser
mod css_parser;
/// Slab allocator for nodes, based on IDs (replaces kuchiki + markup5ever)
mod id_tree;
/// State handling for user interfaces
mod ui_state;

/// Faster implementation of a HashMap
type FastHashMap<T, U> = ::std::collections::HashMap<T, U, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;
type FastHashSet<T> = ::std::collections::HashSet<T, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;

/// Quick exports of common types
pub mod prelude {
    pub use traits::LayoutScreen;
    pub use dom::On;
    pub use window::{WindowId, WindowCreateOptions};
    pub use app_state::AppState;
    pub use css::Css;
    pub use dom::{NodeType, Dom, Callback};
    pub use app::App;
}