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
// === "extreme lints" lockdown (2026-06-20) — maximal opt-in lint set ===
// All clippy groups + opt-in rustc lints, warn-level so normal builds still
// pass; the CI clippy job runs `-D warnings`, turning every one of these into
// the outstanding-lint-failure report for Monday triage. NOT yet fixed.
#![warn(
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    // missing_docs,  // TODO(docs): re-enable as a dedicated final docs pass; disabled
    //                // for now so the cleanup focuses on code-quality lints, not doc debt.
    missing_debug_implementations,
    missing_copy_implementations,
    unreachable_pub,
    unused_qualifications,
    unused_lifetimes,
    unused_import_braces,
    unused_macro_rules,
    unused_crate_dependencies,
    meta_variable_misuse,
    trivial_casts,
    trivial_numeric_casts,
    elided_lifetimes_in_paths,
    single_use_lifetimes,
    variant_size_differences,
    non_ascii_idents,
    unsafe_op_in_unsafe_fn,
    let_underscore_drop,
)]
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

#[macro_use]
/// Internal macros for reducing boilerplate in property definitions.
pub mod macros;
/// Internal numeric-cast helpers (named, documented `as` conversions).
/// `pub` + `pub(crate)` fns so it trips neither `unreachable_pub` nor
/// `redundant_pub_crate` (they conflict); `#[doc(hidden)]` keeps it out of the API.
#[doc(hidden)]
pub mod cast;
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
