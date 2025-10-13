use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use azul_css::props::basic::ColorU;
use hyphenation::Language;

use crate::text3::{
    cache::{
        BidiLevel, Direction, FontLoaderTrait, LayoutFontMetrics, FontProviderTrait, FontRef, Glyph,
        GlyphOrientation, GlyphSource, LayoutError, ParsedFontTrait, Point, PositionedItem,
        ShapedItem, Spacing, StyleProperties, TextDecoration, TextOrientation, TextTransform,
        VerticalMetrics, WritingMode,
    },
    script::Script,
};

pub mod five;
pub mod four;
pub mod one;
pub mod three;
pub mod two;

// --- Mocking Infrastructure ---

#[derive(Debug, Clone)]
struct MockFont {
    id: u32,
    metrics: LayoutFontMetrics,
    glyphs: HashMap<char, (u16, f32)>, // char -> (glyph_id, advance)
    ligatures: HashMap<String, (u16, f32)>, // ligature string -> (glyph_id, advance)
}

impl ParsedFontTrait for MockFont {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        _language: Language,
        direction: Direction,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph<Self>>, LayoutError> {
        let mut result_glyphs = Vec::new();
        let mut char_indices: Vec<(usize, char)> = text.char_indices().collect();

        // In RTL, the shaper processes text in logical order, but the layout might reverse it
        // later. Our mock shaper will just process what it's given.

        let mut text_cursor = 0;

        while text_cursor < char_indices.len() {
            let (byte_index, char) = char_indices[text_cursor];

            // Check for ligatures (e.g., "fi")
            let mut applied_ligature = false;
            for (lig_str, (glyph_id, advance)) in &self.ligatures {
                if text[byte_index..].starts_with(lig_str) {
                    let lig_len = lig_str.len();
                    result_glyphs.push(Glyph {
                        glyph_id: *glyph_id,
                        codepoint: lig_str.chars().next().unwrap(),
                        font: Arc::new(self.clone()),
                        style: Arc::new(style.clone()),
                        source: GlyphSource::Char,
                        logical_byte_index: byte_index,
                        logical_byte_len: lig_len,
                        content_index: 0,
                        cluster: byte_index as u32,
                        advance: *advance,
                        offset: Point::default(),
                        vertical_advance: 0.0,
                        vertical_origin_y: 0.0,
                        vertical_bearing: Point::default(),
                        orientation: GlyphOrientation::Horizontal,
                        script,
                        bidi_level: BidiLevel::new(if direction == Direction::Rtl { 1 } else { 0 }),
                    });

                    text_cursor += lig_str.chars().count();
                    applied_ligature = true;
                    break;
                }
            }

            if applied_ligature {
                continue;
            }

            // Regular character
            let (glyph_id, advance) = self.glyphs.get(&char).cloned().unwrap_or((0, 10.0));
            result_glyphs.push(Glyph {
                glyph_id,
                codepoint: char,
                font: Arc::new(self.clone()),
                style: Arc::new(style.clone()),
                source: GlyphSource::Char,
                logical_byte_index: byte_index,
                logical_byte_len: char.len_utf8(),
                content_index: 0,
                cluster: byte_index as u32,
                advance,
                offset: Point::default(),
                vertical_advance: 0.0,
                vertical_origin_y: 0.0,
                vertical_bearing: Point::default(),
                orientation: GlyphOrientation::Horizontal,
                script, // Simplified for mock
                bidi_level: BidiLevel::new(if direction == Direction::Rtl { 1 } else { 0 }),
            });
            text_cursor += 1;
        }
        Ok(result_glyphs)
    }

    fn get_hash(&self) -> u64 {
        self.id as u64
    }

    // NOTE: This is fake, we don't have glyph sizes here - also very slow, but ok for mocking
    fn get_glyph_size(
        &self,
        glyph_id: u16,
        font_size: f32,
    ) -> Option<azul_core::window::LogicalSize> {
        self.glyphs.values().find_map(|(id, advance)| {
            if *id == glyph_id {
                Some(azul_core::window::LogicalSize {
                    width: *advance,
                    height: font_size,
                })
            } else {
                None
            }
        })
    }

    fn get_hyphen_glyph_and_advance(&self, _font_size: f32) -> Option<(u16, f32)> {
        Some((99, 5.0)) // Hyphen glyph ID 99, advance 5.0
    }

    fn get_kashida_glyph_and_advance(&self, _font_size: f32) -> Option<(u16, f32)> {
        Some((100, 10.0))
    }

    fn has_glyph(&self, codepoint: u32) -> bool {
        self.glyphs
            .contains_key(&(std::char::from_u32(codepoint).unwrap_or('\0')))
    }

    fn get_vertical_metrics(&self, _glyph_id: u16) -> Option<VerticalMetrics> {
        None
    }

    fn get_font_metrics(&self) -> LayoutFontMetrics {
        self.metrics.clone()
    }

    fn num_glyphs(&self) -> u16 {
        256
    }
}

#[derive(Debug)]
struct MockFontLoader {
    fonts: HashMap<String, Arc<MockFont>>,
}

impl FontLoaderTrait<MockFont> for MockFontLoader {
    fn load_font(
        &self,
        _font_bytes: &[u8],
        _font_index: usize,
    ) -> Result<Arc<MockFont>, LayoutError> {
        // In a real mock, you'd probably identify the font by bytes,
        // but for tests we can just return a default font.
        Ok(self.fonts.get("mock").unwrap().clone())
    }
}

// A mock FontManager that doesn't use fontconfig
struct MockFontManager {
    loader: Arc<MockFontLoader>,
    cache: Mutex<HashMap<FontRef, Arc<MockFont>>>,
}

impl MockFontManager {
    fn new(loader: Arc<MockFontLoader>) -> Self {
        Self {
            loader,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

impl FontProviderTrait<MockFont> for MockFontManager {
    fn load_font(&self, font_ref: &FontRef) -> Result<Arc<MockFont>, LayoutError> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(font) = cache.get(font_ref) {
            return Ok(font.clone());
        }
        let font = self
            .loader
            .fonts
            .get(&font_ref.family)
            .ok_or_else(|| LayoutError::FontNotFound(font_ref.clone()))?;
        cache.insert(font_ref.clone(), font.clone());
        Ok(font.clone())
    }
}

fn create_mock_font_manager() -> MockFontManager {
    let mut glyphs = HashMap::new();
    // Latin
    glyphs.insert('f', (1, 10.0));
    glyphs.insert('i', (2, 4.0));
    glyphs.insert('l', (3, 4.0));
    glyphs.insert('a', (4, 8.0));
    glyphs.insert('s', (5, 8.0));
    glyphs.insert('h', (6, 9.0));
    glyphs.insert('o', (7, 9.0));
    glyphs.insert('m', (8, 12.0));
    glyphs.insert(' ', (10, 5.0));
    glyphs.insert('y', (11, 10.0));
    glyphs.insert('p', (12, 9.0));
    glyphs.insert('e', (13, 8.0));
    glyphs.insert('n', (14, 9.0));
    glyphs.insert('t', (15, 7.0));
    glyphs.insert('b', (16, 9.0));
    glyphs.insert('r', (17, 7.0));
    glyphs.insert('k', (18, 9.0));
    glyphs.insert('g', (19, 9.0));
    glyphs.insert('w', (20, 10.0));
    glyphs.insert('d', (21, 9.0));
    glyphs.insert('c', (22, 8.0));
    glyphs.insert('u', (23, 9.0));

    // Digits
    ('0'..='9').for_each(|c| {
        glyphs.insert(c, (30 + (c as u32 - '0' as u32) as u16, 8.0));
    });

    // Hebrew
    glyphs.insert('א', (100, 10.0));
    glyphs.insert('ב', (101, 10.0));
    glyphs.insert('ג', (102, 10.0));
    glyphs.insert('ד', (103, 10.0));
    glyphs.insert('ש', (200, 10.0));
    glyphs.insert('ל', (201, 10.0));
    glyphs.insert('ו', (202, 10.0));
    glyphs.insert('ם', (203, 10.0));

    // Arabic
    glyphs.insert('م', (300, 8.0));
    glyphs.insert('ر', (301, 7.0));
    glyphs.insert('ح', (302, 9.0));
    glyphs.insert('ب', (303, 7.0));
    glyphs.insert('ا', (304, 6.0));

    let mut ligatures = HashMap::new();
    ligatures.insert("fi".to_string(), (1000, 12.0));

    let mock_font = Arc::new(MockFont {
        id: 1,
        metrics: LayoutFontMetrics {
            ascent: 80.0,
            descent: -20.0,
            line_gap: 0.0,
            units_per_em: 100,
        },
        glyphs,
        ligatures,
    });

    let mut fonts = HashMap::new();
    fonts.insert("mock".to_string(), mock_font);

    let loader = Arc::new(MockFontLoader { fonts });
    MockFontManager::new(loader)
}

pub fn default_style() -> Arc<StyleProperties> {
    Arc::new(StyleProperties {
        font_ref: FontRef {
            family: "mock".into(),
            ..FontRef::invalid()
        },
        font_size_px: 10.0,
        color: ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        letter_spacing: Spacing::Px(0),
        word_spacing: Spacing::Px(0),
        line_height: 12.0,
        text_decoration: TextDecoration::default(),
        font_features: Vec::new(),
        font_variations: Vec::new(),
        tab_size: 4.0,
        text_transform: TextTransform::default(),
        writing_mode: WritingMode::HorizontalTb,
        text_orientation: TextOrientation::Mixed,
        text_combine_upright: None,
        font_variant_caps: Default::default(),
        font_variant_numeric: Default::default(),
        font_variant_ligatures: Default::default(),
        font_variant_east_asian: Default::default(),
    })
}

/// Helper function to extract the text content from a layout result.
fn get_text_from_items<T: ParsedFontTrait>(items: &[PositionedItem<T>]) -> String {
    items
        .iter()
        .map(|p_item| match &p_item.item {
            ShapedItem::Cluster(c) => c.text.clone(),
            _ => String::new(),
        })
        .collect()
}
