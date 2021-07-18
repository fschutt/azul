//! Shared datatypes for azul-* crates

#![cfg_attr(not(feature = "std"), no_std)]

#[macro_use]
extern crate core;
#[macro_use]
extern crate alloc;

extern crate libm;

#[cfg(feature = "multithreading")]
extern crate rayon;
#[macro_use]
extern crate azul_css;
extern crate gl_context_loader;
#[cfg(feature = "css_parser")]
extern crate azul_css_parser;

/// Useful macros for implementing Azul APIs without duplicating code
#[macro_use]
pub mod macros;
/// Type definitions for various types of callbacks, as well as focus and scroll handling
#[macro_use]
pub mod callbacks;
/// Functions to manage adding fonts + images, garbage collection
pub mod app_resources;
/// Layout and display list creation algorithm, z-index reordering of a `CachedDisplayList`
pub mod display_list;
/// `Dom` construction, `NodeData` and `NodeType` management functions
pub mod dom;
// Algorithms to create git-like diffs between two doms in linear time
// pub mod diff;
/// Contains OpenGL helper functions (to compile / link shaders), `VirtualGlDriver` for unit testing
pub mod gl;
/// Internal, arena-based storage for Dom nodes
pub mod id_tree;
/// CSS cascading module
pub mod style;
/// SVG module
pub mod svg;
/// XML structures
pub mod xml;
/// Main `Layout` and `GetTextLayout` trait definition
pub mod traits;
/// Async (task, thread, timer) helper functions
pub mod task;
/// `StyledDom` = CSSOM
pub mod styled_dom;
/// Handles the UI layout and UI layout solver
pub mod ui_solver;
/// Window creation / interaction with the OS' windowing API
pub mod window;
/// Window state handling / synchronization
pub mod window_state;

// Typedef for possible faster implementation of hashing
pub type FastHashMap<T, U> = alloc::collections::BTreeMap<T, U>;
pub type FastBTreeSet<T> = alloc::collections::BTreeSet<T>;
