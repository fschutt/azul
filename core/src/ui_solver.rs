//! Default font/text constants and small geometry types used by the layout
//! solver and text shaping pipeline.

use azul_css::props::{
    basic::{ColorU as StyleColorU, PixelValue, StyleFontSize},
    style::StyleTextColor,
};

use crate::geom::{LogicalPosition, LogicalSize};

/// Default font size (`16px`) used when no explicit size is specified.
pub const DEFAULT_FONT_SIZE: StyleFontSize = StyleFontSize {
    inner: PixelValue::const_px(16),
};
/// Default font family identifier used as a fallback.
pub const DEFAULT_FONT_ID: &str = "serif";
/// Default text color (opaque black).
pub const DEFAULT_TEXT_COLOR: StyleTextColor = StyleTextColor {
    inner: StyleColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    },
};
/// Resolved top/right/bottom/left offsets in logical pixels (used for
/// margins, padding, and borders after CSS resolution).
#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ResolvedOffsets {
    pub top: f32,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}

impl ResolvedOffsets {
    pub const fn zero() -> Self {
        Self {
            top: 0.0,
            left: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }
    #[must_use]
    pub fn total_vertical(&self) -> f32 {
        self.top + self.bottom
    }
    #[must_use]
    pub fn total_horizontal(&self) -> f32 {
        self.left + self.right
    }
}

/// Index into a font's glyph table.
type GlyphIndex = u32;

/// A single positioned glyph with its index, screen position, and size.
#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
pub struct GlyphInstance {
    pub index: GlyphIndex,
    pub point: LogicalPosition,
    pub size: LogicalSize,
}

impl GlyphInstance {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.point.scale_for_dpi(scale_factor);
        self.size.scale_for_dpi(scale_factor);
    }
}
