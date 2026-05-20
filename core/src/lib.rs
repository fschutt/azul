//! Shared datatypes for azul-* crates
//!
//! `azul-core` provides the platform-independent core types used throughout
//! the Azul toolkit. Key modules include [`dom`] for DOM construction,
//! [`callbacks`] for event callback types, [`styled_dom`] for the CSSOM,
//! and [`window`] for OS windowing abstractions.
//!
//! This crate depends on [`azul_css`] for CSS property definitions and is
//! consumed by `azul-layout`, `azul-dll`, and the platform shell crates.
//! It supports `no_std` environments via `#![cfg_attr(not(feature = "std"), no_std)]`.

#![cfg_attr(not(feature = "std"), no_std)]
// Lint policy: deny correctness/safety issues, warn on style
#![deny(unused_must_use)]
#![warn(clippy::all)]
#![allow(
    clippy::non_canonical_partial_ord_impl,
    clippy::legacy_numeric_constants,
    clippy::should_implement_trait,
    clippy::result_unit_err,
    clippy::ptr_as_ptr,
    clippy::too_many_arguments,
    clippy::type_complexity,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_parens,
    dead_code,
    unused_doc_comments,
    unused_assignments,                    // compact_cache_builder incremental updates
    mismatched_lifetime_syntaxes,
    unexpected_cfgs,
    unpredictable_function_pointer_comparisons, // intentional in dom callback comparison
    improper_ctypes_definitions,           // xml component fns use Rust fn pointers internally
    static_mut_refs,                       // TODO: migrate to OnceLock for Rust 2024
)]

// `extern crate` + `#[macro_use]` required for `no_std` support:
// makes `core` and `alloc` macros available without `use` imports.
#[macro_use]
extern crate core;
#[macro_use]
extern crate alloc;
#[macro_use]
extern crate azul_css;

/// Internal macros for `Vec`, `Option`, and callback boilerplate.
#[macro_use]
pub mod macros;
/// Debug logging system with category filtering.
#[macro_use]
pub mod debug;
/// Unified `AZ_PROFILE` gate for memory and CPU profiling instrumentation.
pub mod profile;
/// Callback types: layout, event, timer, thread, and focus handling.
#[macro_use]
pub mod callbacks;
/// Host-language callback invoker registry — the C-ABI surface managed-FFI
/// bindings (Lua, Ruby, …) use to register one per-kind invoker + a single
/// shared releaser, so callbacks can be created via `_createFromHostHandle`
/// without the host having to generate trampolines for struct-by-value
/// signatures their FFI library can't handle.
#[macro_use]
pub mod host_invoker;
/// Accessibility types for screen-reader integration (AccessKit).
pub mod a11y;
/// DOM construction: `Dom`, `NodeData`, `NodeType`, and the CSS-in-Rust API.
pub mod dom;
/// Drag context for text selection, scrollbar, node, and window drags.
pub mod drag;
/// Icon provider system for loading icons from fonts, images, or zip packs.
pub mod icon;
/// Resource management: font/image loading, caching, and garbage collection.
pub mod resources;
/// Text selection and cursor positioning for inline content.
pub mod selection;
/// Linear-time DOM diffing for incremental updates.
pub mod diff;
/// CSS animation and transition configuration.
pub mod animation;
/// Event filtering: mouse, keyboard, window, and synthetic events.
pub mod events;
/// Biometric-auth POD types — `BiometricKind` + `BiometricResult` + `BiometricPrompt`.
/// Stateful manager lives in `azul_layout::managers::biometric`.
pub mod biometric;
/// Geolocation POD types — `LocationFix` + `GeolocationProbeConfig`.
/// Stateful manager lives in `azul_layout::managers::geolocation`.
pub mod geolocation;
/// Logical and physical coordinate types (`LogicalSize`, `PhysicalPosition`, etc.).
pub mod geom;
/// OpenGL context wrappers, shader compilation, and texture cache.
pub mod gl;
/// FXAA (Fast Approximate Anti-Aliasing) shader.
pub mod gl_fxaa;
/// OpenGL constants (GL 1.1 through GL 4.x).
pub mod glconst;
/// GPU value cache for CSS transforms and opacity.
pub mod gpu;
/// Hit-test results: which DOM nodes are under the cursor.
pub mod hit_test;
/// Type-safe hit-test tag system for compositor integration.
pub mod hit_test_tag;
/// Arena-based node tree storage and hierarchy management.
pub mod id;
/// System-keyring POD types — `KeyringRequest` + `KeyringResult`.
/// Stateful manager lives in `azul_layout::managers::keyring`.
pub mod keyring;
/// Menu system: context menus, dropdown menus, and menu bars.
pub mod menu;
/// CSS property cache for efficient per-node style resolution.
pub mod prop_cache;
/// Converts `CssPropertyCache` into compact three-tier numeric cache.
pub mod compact_cache_builder;
/// Type-erased, ref-counted smart pointer with runtime borrow checking.
pub mod refany;
/// CSS cascade: selector matching, specificity, and property inheritance.
pub mod style;
/// `StyledDom` — the result of applying CSS to a DOM tree (the CSSOM).
pub mod styled_dom;
/// SVG rendering, path tessellation, and geometric operations.
pub mod svg;
/// SVG `d=""` path data parser.
pub mod svg_path_parser;
/// Timer, thread, and async task management.
pub mod task;
/// 3D transform matrix computation for CSS transforms.
pub mod transform;
/// Built-in user-agent default stylesheet.
pub mod ua_css;
/// Default font/text constants and small geometry helpers for layout.
pub mod ui_solver;
/// Window configuration, input state, and platform-specific options.
pub mod window;
/// XML and XHTML parsing for declarative UI definitions.
pub mod xml;
/// JSON value types for the C API (no serde dependency).
pub mod json;

/// Ordered map alias used throughout `azul-core`.
///
/// This is backed by `BTreeMap` (not a hash map) because the `core` crate
/// supports `no_std`, where `HashMap` is unavailable. The webrender crates
/// define their own `FastHashMap` using `HashMap` + `FxHasher`.
pub type OrderedMap<T, U> = alloc::collections::BTreeMap<T, U>;
pub type FastBTreeSet<T> = alloc::collections::BTreeSet<T>;
