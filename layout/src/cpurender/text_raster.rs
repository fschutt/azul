//! Text into pixels: a string set in a font and rasterised into an RGBA8
//! [`RawImage`] - what a photo editor's text tool, a watermark or a label
//! drawn into a picture needs (the DOM draws text into the WINDOW; this draws
//! it into an image the app owns).
//!
//! One path, no twin: the lines are shaped by the text engine
//! (`text3::default::shape_text_for_parsed_font`: kerning, ligatures, marks)
//! and their glyphs rasterised by the CPU renderer's own glyph path
//! (`raster::render_text`) in GRAYSCALE coverage - the target is
//! transparent, so there is no background for LCD subpixels to sit on.
//!
//! - [`text_image`] / [`text_image_with`]: a straight-alpha RGBA8 image as
//!   big as the lines' boxes (`RawImage::from_text` in the C API).
//! - [`draw_text`] / [`draw_text_with`]: the same pixels composited into an
//!   RGBA8 / BGRA8 image (straight or premultiplied) with the first line's
//!   box at (`x`, `y`) (`RawImage::draw_text`).
//!
//! The `_with` forms take the font cache (tests hand in a memory font); the
//! plain forms use a font cache of the system fonts built once per thread
//! on first use (a scan of the font folders - the first call takes that
//! long, so an app that draws text in a callback may warm it on a thread).

use azul_core::resources::{OptionRawImage, RawImage, RawImageData, RawImageFormat};
use azul_css::{props::basic::ColorU, AzString};

/// The largest em size and the largest side of a text image, in pixels.
pub const TEXT_RASTER_MAX_SIZE_PX: f32 = 4096.0;
pub const TEXT_RASTER_MAX_SIDE_PX: u32 = 16384;

/// How [`text_image`] / [`draw_text`] set a text: the font, its size, the
/// line spacing and the colour.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct TextRasterStyle {
    /// A family name: `sans-serif`, `serif`, `monospace` or an installed
    /// family. A family that is not there falls back to one that covers the
    /// text.
    pub font_family: AzString,
    /// The em size in image pixels.
    pub size_px: f32,
    /// The distance between baselines as a multiple of the font's own line
    /// height (1.0 = the font's).
    pub line_height: f32,
    /// The ink colour; its alpha scales the coverage.
    pub color: ColorU,
    /// The bold face of the family (the regular one where it has none).
    pub bold: bool,
    /// The italic face of the family (the regular one where it has none).
    pub italic: bool,
}

impl TextRasterStyle {
    /// `font_family` at `size_px`, in `color`, the font's own line height.
    #[must_use]
    pub fn create(font_family: AzString, size_px: f32, color: ColorU) -> Self {
        Self {
            font_family,
            size_px,
            line_height: 1.0,
            color,
            bold: false,
            italic: false,
        }
    }

    #[must_use]
    pub fn with_bold(mut self, bold: bool) -> Self {
        self.bold = bold;
        self
    }

    #[must_use]
    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }

    /// Baselines `line_height` times the font's line height apart.
    #[must_use]
    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    /// The line-height multiple in use: a non-positive or non-finite one is 1.
    fn line_factor(&self) -> f32 {
        if self.line_height.is_finite() && self.line_height > 0.0 {
            self.line_height
        } else {
            1.0
        }
    }
}

/// A set text: PREMULTIPLIED RGBA8 rows (the renderer's own format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPixels {
    pub width: u32,
    pub height: u32,
    pub rgba_premultiplied: Vec<u8>,
}
