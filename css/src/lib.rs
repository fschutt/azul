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
#![allow(warnings)]

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
