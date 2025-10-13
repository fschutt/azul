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
extern crate gl_context_loader;

/// Useful macros for implementing Azul APIs without duplicating code
#[macro_use]
pub mod macros;
/// Type definitions for various types of callbacks, as well as focus and scroll handling
#[macro_use]
pub mod callbacks;
/// `Dom` construction, `NodeData` and `NodeType` management functions
pub mod dom;
/// Type definitions for Glyphs
pub mod glyph;
/// Functions to manage adding fonts + images, garbage collection
pub mod resources;
/// Primitives for cursor and text selection handling
pub mod selection;
// Algorithms to create git-like diffs between two doms in linear time
// pub mod diff;
/// Animation system
pub mod animation;
/// Contains OpenGL helper functions (to compile / link shaders), `VirtualGlDriver` for unit testing
pub mod gl;
/// GPU value synchronization (colors, transforms) for WebRender preparation
pub mod gpu;
/// Hit-testing module
pub mod hit_test;
/// Internal, arena-based storage for Dom nodes
pub mod id;
/// Types for handling menus (context menu, menubar)
pub mod menu;
/// Cache for CSS Properties
pub mod prop_cache;
/// Type-erased reference wrapper (like `Box<dyn Any>` but for references)
pub mod refany;
/// CSS cascading module
pub mod style;
/// `StyledDom` = CSSOM
pub mod styled_dom;
/// SVG module
pub mod svg;
/// Async (task, thread, timer) helper functions
pub mod task;
/// CSS transform computation
pub mod transform;
/// Handles the UI layout and UI layout solver
pub mod ui_solver;
/// Window creation / interaction with the OS' windowing API
pub mod window;
/// Window state handling / synchronization
pub mod window_state;
/// XML structures
pub mod xml;

// pub mod old_text_layout; // -- IGNORE, this was for the initial solver only --

// Typedef for possible faster implementation of hashing
pub type FastHashMap<T, U> = alloc::collections::BTreeMap<T, U>;
pub type FastBTreeSet<T> = alloc::collections::BTreeSet<T>;
