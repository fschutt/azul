//! Shared datatypes for azul-* crates

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(warnings)]

#[macro_use]
extern crate core;
#[macro_use]
extern crate alloc;

extern crate libm;

#[macro_use]
extern crate azul_css;
#[cfg(feature = "css_parser")]
extern crate azul_css_parser;
extern crate gl_context_loader;

/// Useful macros for implementing Azul APIs without duplicating code
#[macro_use]
pub mod macros;
/// Type definitions for various types of callbacks, as well as focus and scroll handling
#[macro_use]
pub mod callbacks;
/// Functions to manage adding fonts + images, garbage collection
pub mod app_resources;
/// Contains functions to format a CSS stylesheet to a Rust string
pub mod css;
/// Layout and display list creation algorithm, z-index reordering of a `CachedDisplayList`
pub mod display_list;
/// `Dom` construction, `NodeData` and `NodeType` management functions
pub mod dom;
/// Functions to paginate a DOM into multiple pages (sub-DOMs) for printing
pub mod pagination;
// Algorithms to create git-like diffs between two doms in linear time
// pub mod diff;
/// Contains OpenGL helper functions (to compile / link shaders), `VirtualGlDriver` for unit testing
pub mod gl;
/// Internal, arena-based storage for Dom nodes
pub mod id_tree;
/// CSS cascading module
pub mod style;
/// `StyledDom` = CSSOM
pub mod styled_dom;
/// SVG module
pub mod svg;
/// Async (task, thread, timer) helper functions
pub mod task;
/// Main `Layout` and `GetTextLayout` trait definition
pub mod traits;
/// Handles the UI layout and UI layout solver
pub mod ui_solver;
/// Window creation / interaction with the OS' windowing API
pub mod window;
/// Window state handling / synchronization
pub mod window_state;
/// XML structures
pub mod xml;

// Typedef for possible faster implementation of hashing
pub type FastHashMap<T, U> = alloc::collections::BTreeMap<T, U>;
pub type FastBTreeSet<T> = alloc::collections::BTreeSet<T>;
