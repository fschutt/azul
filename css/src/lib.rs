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

#[macro_use]
pub mod macros;
pub mod compact_cache;
pub mod corety;
pub mod css;
pub mod dynamic_selector;
pub mod format_rust_code;
#[cfg(feature = "parser")]
pub mod parser2;
pub mod props;
pub mod shape;
pub mod shape_parser;
pub mod system;

pub use self::corety::*;
