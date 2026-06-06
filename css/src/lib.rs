//! Provides datatypes used to describe an application's style using the Azul GUI framework.
//!
//! # Modules
//!
//! - [`css`]: Stylesheet types (rules, selectors, declarations).
//! - [`props`]: Typed CSS property values (colors, lengths, layout properties).
//! - [`parser2`]: CSS string parsing (feature-gated behind `"parser"`).
//! - [`system`]: Native OS theme discovery and system colors.
//! - [`shape`]: Text shaping and glyph layout.
//! - [`shape_parser`]: Font and shape metric parsing.
//! - [`dynamic_selector`]: Runtime selector matching helpers.
//! - [`compact_cache`]: Compact caching utilities for resolved styles.
//! - [`corety`]: Core type aliases re-exported at crate root.
// Lint policy: deny correctness/safety issues, warn on style
#![deny(unused_must_use)]
#![warn(clippy::all)]
// Allowed: macros generate PartialOrd alongside Ord, legacy numeric constants
// in spec-derived code, into_iter naming for custom collection types
#![allow(
    clippy::non_canonical_partial_ord_impl,
    clippy::legacy_numeric_constants,
    clippy::should_implement_trait,       // into_iter on custom vecs
    clippy::result_unit_err,              // parser returns Result<_, ()>
    clippy::ptr_as_ptr,                   // FFI pointer casts
    clippy::too_many_arguments,
    unused_imports,                        // conditional compilation
    unused_variables,
    unused_mut,
    unused_parens,
    dead_code,
    unused_doc_comments,                   // doc comments before macro invocations
    ambiguous_glob_reexports,             // layout/style mod re-exports
    mismatched_lifetime_syntaxes,         // macro-generated code
    unreachable_patterns,                  // exhaustive match in generated code
)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

/// [az-diag REVERT] wasm-only gate for `AzString::copy_from_bytes`'s diagnostic markers.
/// Set `true` ONLY by the wasm cb-runner (`AzStartup_initLayoutCache` in dll/web) around the
/// lifted layout-callback call. The native `render_initial_page` path runs the SAME cb with this
/// flag `false`, so the cb's fixed-address `write_volatile(0x40xxx)` probes are skipped natively
/// (they'd write a wild native pointer and crash the server). Reading the static is native-safe.
pub static AZ_WASM_CB_ACTIVE: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

#[macro_use]
/// Internal macros for reducing boilerplate in property definitions.
pub mod macros;
/// Multi-language code generation backends (Rust, C++, Python).
pub mod codegen;
/// Three-tier numeric property cache for fast style resolution.
pub mod compact_cache;
/// FFI-safe core type aliases (`AzString`, `AzVec`, `OptionT`, etc.).
pub mod corety;
/// Stylesheet types: rules, selectors, declarations, and specificity.
pub mod css;
/// Typed default values for CSS properties (font size, font id, text color).
pub mod defaults;
/// Runtime CSS selector matching (`:hover`, `@os`, `@media`, etc.).
pub mod dynamic_selector;
/// CSS string parser (tokenizer, declaration parser, shorthand expansion).
#[cfg(feature = "parser")]
pub mod parser2;
/// Typed CSS property values: colors, lengths, layout, backgrounds, etc.
pub mod props;
/// CSS Shape data structures (`shape-inside`, `shape-outside`, `clip-path`).
pub mod shape;
/// Parser for CSS shape functions and font metrics.
pub mod shape_parser;
/// Native OS theme discovery: system colors, fonts, and DPI.
pub mod system;

pub use self::corety::*;
