// css/src/props/mod.rs

//! Contains all CSS property definitions, organized by category.
//!
//! Submodules:
//! - [`basic`]: Primitive CSS value types (colors, lengths, percentages, etc.)
//! - [`layout`]: Layout-related properties (dimensions, overflow, grid, flexbox, etc.)
//! - [`style`]: Visual style properties (backgrounds, borders, fonts, etc.)
//! - [`property`]: The core [`property::CssProperty`] enum and its parser
//! - [`formatter`]: CSS serialization (formatting properties back to CSS strings)
//!
//! The [`PixelValueTaker`] trait is re-exported here for external crate use.

// Helper macros used across property modules.
#[macro_use]
mod macros;

// Re-export PixelValueTaker trait so external crates can construct dimension types
pub use macros::PixelValueTaker;

// Public modules for different property categories.
pub mod basic;
pub mod layout;
pub mod style;

// The core CssProperty enum and its parser.
pub mod property;

// A trait for formatting properties back to CSS strings.
pub mod formatter;
