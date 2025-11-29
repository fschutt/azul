//! Mock font implementation for testing text layout.
//!
//! Provides a `MockFont` that simulates font behavior without requiring
//! actual font files, useful for unit testing text layout functionality.

use std::collections::BTreeMap;

use crate::text3::cache::LayoutFontMetrics;

/// A mock font implementation for testing text layout without real fonts.
///
/// This allows testing text shaping, layout, and rendering code paths
/// without needing to load actual TrueType/OpenType font files.
///
/// # Example
///
/// ```ignore
/// let metrics = LayoutFontMetrics {
///     units_per_em: 1000,
///     ascent: 800.0,
///     descent: 200.0,
///     line_gap: 100.0,
/// };
/// let mock = MockFont::new(metrics)
///     .with_space_width(250)
///     .with_glyph_advance(65, 600); // 'A' = 600 units
/// ```
#[derive(Debug, Clone)]
pub struct MockFont {
    /// Font metrics (ascent, descent, etc.).
    pub font_metrics: LayoutFontMetrics,
    /// Width of the space character in font units.
    pub space_width: Option<usize>,
    /// Horizontal advance widths keyed by glyph ID.
    pub glyph_advances: BTreeMap<u16, u16>,
    /// Glyph bounding box sizes (width, height) keyed by glyph ID.
    pub glyph_sizes: BTreeMap<u16, (i32, i32)>,
    /// Unicode codepoint to glyph ID mapping.
    pub glyph_indices: BTreeMap<u32, u16>,
}

impl MockFont {
    /// Creates a new `MockFont` with the given font metrics.
    pub fn new(font_metrics: LayoutFontMetrics) -> Self {
        MockFont {
            font_metrics,
            space_width: Some(10),
            glyph_advances: BTreeMap::new(),
            glyph_sizes: BTreeMap::new(),
            glyph_indices: BTreeMap::new(),
        }
    }

    /// Sets the space character width.
    pub fn with_space_width(mut self, width: usize) -> Self {
        self.space_width = Some(width);
        self
    }

    /// Adds a horizontal advance value for a glyph.
    pub fn with_glyph_advance(mut self, glyph_index: u16, advance: u16) -> Self {
        self.glyph_advances.insert(glyph_index, advance);
        self
    }

    /// Adds a bounding box size for a glyph.
    pub fn with_glyph_size(mut self, glyph_index: u16, size: (i32, i32)) -> Self {
        self.glyph_sizes.insert(glyph_index, size);
        self
    }

    /// Adds a Unicode codepoint to glyph ID mapping.
    pub fn with_glyph_index(mut self, unicode: u32, index: u16) -> Self {
        self.glyph_indices.insert(unicode, index);
        self
    }
}
