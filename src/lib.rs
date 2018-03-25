//! azul is a library for creating graphical user interfaces in Rust. 
//! 
//! ## How it works
//! 
//! azul requires your app data to "serialize" itself into a UI. 
//! This is different from how other GUI frameworks work, so it requires a bit of explanation:
//! 
//! Your app data is one global struct for your whole application. This is the "model". 
//! azul takes your model and requires you to build a DOM tree to translate the model into a view.
//! This (layouting, restyling, constraint solving) is done every 2 milliseconds. However, if your 
//! UI doesn't change, nothing is done (in order to not stress the CPU too much).
//!
//! This model makes conditional UI elements and conditional styling very easy. azul takes care
//! of caching for you - your CSS and DOM elements are cached and diffed for changes, in order to 
//! maximize performance. A full screen redraw should not take longer than 16 milliseconds 
//! (currently the frame time is around 1 - 2 milliseconds).
//! 
//! ## Hello world example
//! 
//! For more examples, please look in the `/examples` folder.


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
extern crate image;
extern crate rusttype;
extern crate app_units;

/// Global application (Initialization starts here)
mod app;
/// Wrapper for the application data & application state
mod app_state;
/// Styling & CSS parsing
mod css;
/// DOM / HTML node handling
pub mod dom;
/// The layout traits for creating a layout-able application
pub mod traits;
/// Window handling
pub mod window;
/// Font & image resource handling, lookup and caching
mod resources;
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
/// Dom / CSS caching
mod cache;
/// Image handling
mod images;
/// Font handling
mod font;

/// Faster implementation of a HashMap
type FastHashMap<T, U> = ::std::collections::HashMap<T, U, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;
type FastHashSet<T> = ::std::collections::HashSet<T, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;

pub use app::App;
pub use app_state::AppState;
pub use css::{CssRule, Css};

/// Quick exports of common types
pub mod prelude {
    pub use {App, AppState, CssRule, Css};
    pub use dom::{Dom, NodeType, Callback, CheckboxState, On, UpdateScreen};
    pub use traits::LayoutScreen;
    pub use webrender::api::{ColorF, ColorU};
    pub use window::{MonitorIter, Window, WindowCreateOptions,
                     WindowId, WindowPlacement};
    pub use window::{MouseMode, UpdateBehaviour, UpdateMode, WindowClass, 
                     WindowCreateError, WindowDecorations, WindowMonitorTarget, RendererType};
    pub use font::FontError;
    pub use images::ImageType;
    
    // from the extern crate image
    pub use image::ImageError;
}

