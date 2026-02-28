//! Shared datatypes for azul-* crates

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(warnings)]

#[macro_use]
extern crate core;
#[macro_use]
extern crate alloc;
#[macro_use]
extern crate azul_css;

/// Useful macros for implementing Azul APIs without duplicating code
#[macro_use]
pub mod macros;
/// Unified debug logging system
#[macro_use]
pub mod debug;
/// Type definitions for various types of callbacks plus focus and scroll handling
#[macro_use]
pub mod callbacks;
/// Accessibility types: core data definitions and helper functions
pub mod a11y;
/// `Dom` construction, `NodeData` and `NodeType` management functions
pub mod dom;
/// Unified drag context for text selection, scrollbar, node, and window drags
pub mod drag;
/// Icon system for loading and resolving icons from fonts, images, or zip packs
pub mod icon;
/// Type definitions for Glyphs
pub mod glyph;
/// Functions to manage adding fonts + images, garbage collection
pub mod resources;
/// Primitives for cursor and text selection handling
pub mod selection;
/// Algorithms to create git-like diffs between two doms in linear time
pub mod diff;
/// Animation system
pub mod animation;
/// Event handling (mouse, keyboard, window events)
pub mod events;
/// Geometry module for physical and logical sizes
pub mod geom;
/// Contains OpenGL helper functions (to compile / link shaders)
pub mod gl;
/// FXAA (Fast Approximate Anti-Aliasing) shader implementation
pub mod gl_fxaa;
/// OpenGL constants
pub mod glconst;
/// GPU value synchronization (colors, transforms) for WebRender preparation
pub mod gpu;
/// Hit-testing module
pub mod hit_test;
/// Type-safe hit-test tag system for WebRender integration
pub mod hit_test_tag;
/// Internal, arena-based storage for Dom nodes
pub mod id;
/// Types for handling menus (context menu, menubar)
pub mod menu;
/// Cache for CSS Properties
pub mod prop_cache;
/// Compact cache builder: CssPropertyCache → CompactLayoutCache
pub mod compact_cache_builder;
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
/// User-agent default stylesheet
pub mod ua_css;
/// Handles the UI layout and UI layout solver
pub mod ui_solver;
/// Window creation / interaction with the OS' windowing API
pub mod window;
/// XML structures
pub mod xml;
/// JSON value types (data definitions, no serde_json dependency)
pub mod json;

// Typedef for possible faster implementation of hashing
pub type FastHashMap<T, U> = alloc::collections::BTreeMap<T, U>;
pub type FastBTreeSet<T> = alloc::collections::BTreeSet<T>;
