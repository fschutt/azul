//! Basic / primitive CSS property types and their parsing code.
//!
//! This barrel module collects the foundational types used throughout
//! `css::props::layout`, `css::props::style`, and `css::props::property`:
//!
//! - `angle` — angle units (`deg`, `rad`, `grad`, `turn`)
//! - `animation` — animation / transition descriptors
//! - `color` — color values and parsing (`rgb`, `hsl`, named colors, …)
//! - `direction` — directional enums (LTR / RTL, writing modes)
//! - `error` — CSS-parsing error types
//! - `font` — font-family, font-weight, and related types
//! - `geometry` — geometric primitives (points, sizes, rects)
//! - `image` — CSS image values (`url()`, gradients)
//! - `length` — CSS length units (`px`, `em`, `%`, …)
//! - `parse` — shared low-level parsing helpers (not glob-re-exported;
//!   use qualified paths, e.g. `basic::parse::parse_pixel_value`)
//! - `pixel` — device-pixel types
//! - `time` — duration / time units

pub mod angle;
pub mod animation;
pub mod color;
pub mod direction;
pub mod error;
pub mod font;
pub mod geometry;
pub mod image;
pub mod length;
/// Shared low-level parsing helpers — not glob-re-exported because its
/// items are meant to be used via qualified paths.
pub mod parse;
pub mod pixel;
pub mod time;

pub use self::{
    angle::*, animation::*, color::*, direction::*, error::*, font::*, geometry::*, image::*,
    length::*, pixel::*, time::*,
};
