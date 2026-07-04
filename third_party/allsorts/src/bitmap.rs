#![deny(missing_docs)]

//! Bitmap font handling.

pub mod cbdt;
pub mod sbix;

use crate::error::ParseError;

/// Bit depth of bitmap data.
#[derive(Debug, PartialEq, Eq, Copy, Clone, PartialOrd)]
pub enum BitDepth {
    /// 1-bit per pixel (black and white).
    One = 1,
    /// 2-bits per pixel (grey).
    Two = 2,
    /// 4-bits per pixel (grey).
    Four = 4,
    /// 8-bits per pixel (grey).
    Eight = 8,
    /// 32-bits per pixel (RGBA)
    ThirtyTwo = 32,
}

/// A bitmap glyph with metrics.
pub struct BitmapGlyph {
    /// Horizontal pixels per em.
    ///
    /// Will be `None` if image data is a vector image.
    pub ppem_x: Option<u16>,
    /// Vertical pixels per em.
    ///
    /// Will be `None` if image data is a vector image.
    pub ppem_y: Option<u16>,
    /// `true` if this glyph's bitmap data should be flipped horizontally.
    pub should_flip_hori: bool,
    /// Glyph metrics in pixels.
    pub metrics: Metrics,
    /// Bitmap data.
    pub bitmap: Bitmap,
    /// Glyph index the bitmap data originates from, which can differ from the actual glyph index.
    ///
    /// E.g. an sbix glyph can point to bitmap data belonging to a different glyph altogether.
    pub bitmap_id: u16,
}

/// Bitmap data, either raw or encapsulated in a container format like PNG.
#[allow(missing_docs)]
pub enum Bitmap {
    Embedded(EmbeddedBitmap),
    Encapsulated(EncapsulatedBitmap),
}

/// Raw bitmap data.
pub struct EmbeddedBitmap {
    /// The width of the bitmap in pixels.
    pub width: u8,
    /// The height of the bitmap in pixels.
    pub height: u8,
    /// The format of the pixel data.
    pub format: BitDepth,
    /// Raw pixel data.
    pub data: Box<[u8]>,
}

/// Bitmap data encapsulated in a container format like PNG.
pub struct EncapsulatedBitmap {
    /// The container format used to hold the bitmap data.
    pub format: EncapsulatedFormat,
    /// Bitmap data.
    pub data: Box<[u8]>,
}

/// The container format of an `EncapsulatedBitmap`.
#[allow(missing_docs)]
pub enum EncapsulatedFormat {
    Jpeg,
    Png,
    Tiff,
    Svg,
    /// A format not part of the OpenType specification.
    Other(u32),
}

/// Bitmap glyph metrics either embedded or from `hmtx`/`vmtx`.
#[derive(Debug)]
pub enum Metrics {
    /// Metrics were embedded with the bitmap.
    Embedded(EmbeddedMetrics),
    /// Metrics are available in the `hmtx` and `vmtx` tables.
    HmtxVmtx(OriginOffset),
}

/// Bitmap offset from glyph origin in font units.
#[derive(Debug)]
pub struct OriginOffset {
    /// The horizontal (x-axis) offset from the left edge of the graphic to the glyph’s origin.
    pub x: i16,
    /// The vertical (y-axis) offset from the bottom edge of the graphic to the glyph’s origin.
    pub y: i16,
}

/// Metrics embedded alongside the bitmap.
///
/// One or both of the horizontal or vertical metrics with always be present.
#[derive(Debug)]
pub struct EmbeddedMetrics {
    /// Horizontal pixels per em.
    pub ppem_x: u8,
    /// Vertical pixels per em.
    pub ppem_y: u8,
    /// Horizontal metrics.
    hori: Option<BitmapMetrics>,
    /// Vertical metrics.
    vert: Option<BitmapMetrics>,
}

/// The actual embedded bitmap glyph metrics in pixels.
#[derive(Copy, Clone, Debug)]
pub struct BitmapMetrics {
    /// Distance in pixels from the horizontal origin to the left edge of the bitmap.
    pub origin_offset_x: i16,
    /// Distance in pixels from the horizontal origin to the bottom edge of the bitmap.
    pub origin_offset_y: i16,
    /// Advance width in pixels.
    pub advance: u8,
    /// The spacing of the line before the baseline in pixels.
    pub ascender: i8,
    /// The spacing of the line after the baseline in pixels.
    pub descender: i8,
}

impl EmbeddedMetrics {
    fn new(
        ppem_x: u8,
        ppem_y: u8,
        hori: Option<BitmapMetrics>,
        vert: Option<BitmapMetrics>,
    ) -> Result<Self, ParseError> {
        if hori.is_none() && vert.is_none() {
            return Err(ParseError::MissingValue);
        }

        Ok(EmbeddedMetrics {
            ppem_x,
            ppem_y,
            hori,
            vert,
        })
    }

    /// Metrics for horizontal layout.
    pub fn hori(&self) -> Option<&BitmapMetrics> {
        self.hori.as_ref()
    }

    /// Metrics for vertical layout.
    pub fn vert(&self) -> Option<&BitmapMetrics> {
        self.vert.as_ref()
    }
}

/// Returns true if `value` is closer to zero than `current_best`, favouring positive values even
/// if they're further away from zero.
///
/// Both call sites pass an `i16` or `i32` "ppem difference"; widening to `i32` keeps a
/// single concrete impl with no runtime cost.
fn bigger_or_closer_to_zero(value: i32, current_best: i32) -> bool {
    if value == 0 {
        return true;
    } else if current_best == 0 {
        return false;
    }

    match (current_best > 0, value > 0) {
        (true, true) if value < current_best => true,
        (true, false) => false,
        (false, true) => true,
        (false, false) if value > current_best => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bigger_or_closer_to_zero() {
        // zero always wins
        assert!(bigger_or_closer_to_zero(0, -1));
        assert!(bigger_or_closer_to_zero(0, 0));
        assert!(bigger_or_closer_to_zero(0, 1));
        assert!(!bigger_or_closer_to_zero(-1, 0));
        assert!(!bigger_or_closer_to_zero(1, 0));

        // current best is negative
        assert!(bigger_or_closer_to_zero(10, -5)); // positive wins, even if further from zero
        assert!(bigger_or_closer_to_zero(-2, -5)); // negative wins if closer to zero
        assert!(!bigger_or_closer_to_zero(-7, -5));

        // current best is positive
        assert!(bigger_or_closer_to_zero(2, 5)); // positive wins if smaller
        assert!(!bigger_or_closer_to_zero(-2, 5)); // positive wins, even if further from zero
        assert!(!bigger_or_closer_to_zero(7, 5));
    }
}
