//! CSS property definitions, parsers, and formatters
//!
//! This module provides a hierarchical organization of CSS properties:
//! - `basic/` - Fundamental value types (colors, angles, pixel values)
//! - `layout/` - Properties affecting box model and positioning
//! - `style/` - Visual properties that don't affect layout

pub mod formatter;
pub mod property;

pub mod basic;
pub mod layout;
pub mod style;

// Re-export the main property types for convenience
pub use formatter::*;
pub use property::*;

// Re-export all basic types
pub use basic::{angle::*, color::*, direction::*, value::*};

// Re-export all layout types
pub use layout::{dimensions::*, display::*, flex::*, overflow::*, position::*, spacing::*};

// Re-export all style types
pub use style::{
    background::*, border::*, border_radius::*, box_shadow::*, effects::*, filter::*, font::*,
    text::*, transform::*,
};
