// css/src/props/mod.rs

//! Contains all CSS property definitions, organized by category.

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
