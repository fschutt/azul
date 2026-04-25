//! Aggregated default values for typed CSS properties.
//!
//! Typed wrappers built on top of the raw scalar defaults (e.g.
//! [`crate::props::basic::pixel::DEFAULT_FONT_SIZE`]). These exist so
//! consumers that need a `StyleFontSize` / `StyleTextColor` value at
//! const-time do not have to reconstruct them locally — and do not
//! duplicate the underlying numbers.

use crate::props::{
    basic::{color::ColorU, font::StyleFontSize, pixel::PixelValue},
    style::StyleTextColor,
};

/// Default font size (`16px`) used when no explicit size is specified.
///
/// The numeric value matches [`crate::props::basic::pixel::DEFAULT_FONT_SIZE`].
pub const DEFAULT_FONT_SIZE: StyleFontSize = StyleFontSize {
    inner: PixelValue::const_px(16),
};

/// Default font family identifier used as a fallback.
pub const DEFAULT_FONT_ID: &str = "serif";

/// Default text color (opaque black).
pub const DEFAULT_TEXT_COLOR: StyleTextColor = StyleTextColor {
    inner: ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    },
};
