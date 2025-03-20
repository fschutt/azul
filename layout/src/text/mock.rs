use alloc::collections::btree_map::BTreeMap;

use azul_core::app_resources::{Advance, FontMetrics, GlyphInfo, GlyphOrigin, Placement, RawGlyph};

use super::{shaping::ShapedTextBufferUnsized, FontImpl};

/// A mock font implementation for testing text layout functionality without requiring real fonts
#[derive(Debug, Clone)]
pub struct MockFont {
    pub font_metrics: FontMetrics,
    pub space_width: Option<usize>,
    pub glyph_advances: BTreeMap<u16, u16>,
    pub glyph_sizes: BTreeMap<u16, (i32, i32)>,
    pub glyph_indices: BTreeMap<u32, u16>,
}

impl MockFont {
    /// Create a new MockFont with the given font metrics
    pub fn new(font_metrics: FontMetrics) -> Self {
        MockFont {
            font_metrics,
            space_width: Some(10), // Default space width
            glyph_advances: BTreeMap::new(),
            glyph_sizes: BTreeMap::new(),
            glyph_indices: BTreeMap::new(),
        }
    }

    /// Set the space width
    pub fn with_space_width(mut self, width: usize) -> Self {
        self.space_width = Some(width);
        self
    }

    /// Add a glyph advance value
    pub fn with_glyph_advance(mut self, glyph_index: u16, advance: u16) -> Self {
        self.glyph_advances.insert(glyph_index, advance);
        self
    }

    /// Add a glyph size
    pub fn with_glyph_size(mut self, glyph_index: u16, size: (i32, i32)) -> Self {
        self.glyph_sizes.insert(glyph_index, size);
        self
    }

    /// Add a Unicode code point to glyph index mapping
    pub fn with_glyph_index(mut self, unicode: u32, index: u16) -> Self {
        self.glyph_indices.insert(unicode, index);
        self
    }
}

impl FontImpl for MockFont {
    fn get_space_width(&self) -> Option<usize> {
        self.space_width
    }

    fn get_horizontal_advance(&self, glyph_index: u16) -> u16 {
        self.glyph_advances.get(&glyph_index).copied().unwrap_or(0)
    }

    fn get_glyph_size(&self, glyph_index: u16) -> Option<(i32, i32)> {
        self.glyph_sizes.get(&glyph_index).copied()
    }

    fn shape(&self, text: &[u32], _script: u32, _lang: Option<u32>) -> ShapedTextBufferUnsized {
        // Simple implementation for testing
        let mut infos = Vec::new();

        for &ch in text {
            if let Some(glyph_index) = self.lookup_glyph_index(ch) {
                let adv_x = self.get_horizontal_advance(glyph_index);
                let (size_x, size_y) = self.get_glyph_size(glyph_index).unwrap_or((0, 0));

                let glyph = RawGlyph {
                    unicode_codepoint: Some(ch).into(),
                    glyph_index,
                    liga_component_pos: 0,
                    glyph_origin: GlyphOrigin::Char(char::from_u32(ch).unwrap_or('\u{FFFD}')),
                    small_caps: false,
                    multi_subst_dup: false,
                    is_vert_alt: false,
                    fake_bold: false,
                    fake_italic: false,
                    variation: None.into(),
                };

                let advance = Advance {
                    advance_x: adv_x,
                    size_x,
                    size_y,
                    kerning: 0,
                };

                let info = GlyphInfo {
                    glyph,
                    size: advance,
                    kerning: 0,
                    placement: Placement::None,
                };

                infos.push(info);
            }
        }

        ShapedTextBufferUnsized { infos }
    }

    fn lookup_glyph_index(&self, c: u32) -> Option<u16> {
        self.glyph_indices.get(&c).copied()
    }

    fn get_font_metrics(&self) -> &FontMetrics {
        &self.font_metrics
    }
}
