// Root of the `text3_suite` test target (see layout/Cargo.toml).
//
// This is the ported text3 corpus: 57 formerly-dormant tests moved onto the
// LoadedFonts pipeline. Its style differs from library code on purpose —
// intermediate bindings are kept to document what a step computes even when
// the assertion only reads one of them, and the shared helpers below are used
// by some submodules and not others. Those are exactly the lints below, and
// they carry no signal here.
#![allow(unused_variables, dead_code, clippy::field_reassign_with_default)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use azul_css::props::basic::ColorU;
use hyphenation::Language;

use azul_layout::font_traits::FontLoaderTrait;
use azul_layout::text3::{
    cache::{
        BidiDirection, BidiLevel, FontSelector, FontStack, Glyph, GlyphOrientation, GlyphSource,
        LayoutError, LayoutFontMetrics, LineHeight, ParsedFontTrait, Point, PositionedItem,
        ShapedItem, Spacing, StyleProperties, TextDecoration, TextOrientation, TextTransform,
        VerticalMetrics, WritingMode,
    },
    script::Script,
};

pub mod five;
pub mod four;
pub mod one;
pub mod six;
pub mod three;
pub mod two;

// --- API-generation compat wrappers -----------------------------------
// These tests were written against the FontProviderTrait generation of
// the pipeline (2-arg shape, 3-arg fragment layout). The wrappers adapt
// them to the current LoadedFonts pipeline; every test uses the same
// create_mock_font_manager() glyph table, so the wrappers build the
// LoadedFonts from that fixture directly.

pub fn mock_loaded_fonts() -> azul_layout::text3::cache::LoadedFonts<MockFont> {
    let manager = create_mock_font_manager();
    let fonts = manager.loader.fonts.clone();
    fonts
        .values()
        .map(|f| (rust_fontconfig::FontId::new(), (**f).clone()))
        .collect()
}

pub fn create_logical_items_compat(
    content: &[azul_layout::text3::cache::InlineContent],
    overrides: &[azul_layout::text3::cache::StyleOverride],
) -> Vec<azul_layout::text3::cache::LogicalItem> {
    azul_layout::text3::cache::create_logical_items(content, overrides, &mut None)
}

pub fn reorder_logical_items_compat(
    items: &[azul_layout::text3::cache::LogicalItem],
    dir: BidiDirection,
) -> Result<Vec<azul_layout::text3::cache::VisualItem>, LayoutError> {
    azul_layout::text3::cache::reorder_logical_items(
        items,
        dir,
        azul_layout::text3::cache::UnicodeBidi::Normal,
        &mut None,
    )
}

pub fn shape_visual_items_compat(
    items: &[azul_layout::text3::cache::VisualItem],
    _manager: &MockFontManager,
) -> Result<Vec<ShapedItem>, LayoutError> {
    let loaded = mock_loaded_fonts();
    let chain = HashMap::new();
    let fc = rust_fontconfig::FcFontCache::default();
    azul_layout::text3::cache::shape_visual_items(items, &chain, &fc, &loaded, &mut None)
}

pub fn perform_fragment_layout_compat(
    cursor: &mut azul_layout::text3::cache::BreakCursor<'_>,
    logical: &[azul_layout::text3::cache::LogicalItem],
    constraints: &azul_layout::text3::cache::UnifiedConstraints,
) -> Result<azul_layout::text3::cache::UnifiedLayout, LayoutError> {
    let loaded = mock_loaded_fonts();
    azul_layout::text3::cache::perform_fragment_layout(
        cursor,
        logical,
        constraints,
        &mut None,
        &loaded,
    )
}

pub fn break_one_line_compat<'a>(
    cursor: &mut azul_layout::text3::cache::BreakCursor<'a>,
    lc: &azul_layout::text3::cache::LineConstraints,
    is_vertical: bool,
    hyphenator: Option<&hyphenation::Standard>,
) -> (Vec<ShapedItem>, bool) {
    azul_layout::text3::cache::break_one_line(
        cursor,
        lc,
        is_vertical,
        hyphenator,
        &mock_loaded_fonts(),
        Default::default(),
        Default::default(),
        Default::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn position_one_line_compat(
    line_items: &[ShapedItem],
    lc: &azul_layout::text3::cache::LineConstraints,
    line_top_y: f32,
    line_index: usize,
    text_align: azul_layout::text3::cache::TextAlign,
    base_direction: BidiDirection,
    is_last_line: bool,
    constraints: &azul_layout::text3::cache::UnifiedConstraints,
) -> (Vec<PositionedItem>, f32) {
    azul_layout::text3::cache::position_one_line(
        line_items,
        lc,
        line_top_y,
        line_index,
        text_align,
        base_direction,
        is_last_line,
        constraints,
        &mut None,
        &mock_loaded_fonts(),
        false,
    )
}

pub fn justify_kashida_and_rebuild_compat(
    items: Vec<ShapedItem>,
    lc: &azul_layout::text3::cache::LineConstraints,
    is_vertical: bool,
) -> Vec<ShapedItem> {
    azul_layout::text3::cache::justify_kashida_and_rebuild(
        items,
        lc,
        is_vertical,
        &mut None,
        &mock_loaded_fonts(),
    )
}

pub fn layout_flow_compat(
    cache: &mut azul_layout::text3::cache::TextShapingCache,
    content: &[azul_layout::text3::cache::InlineContent],
    overrides: &[azul_layout::text3::cache::StyleOverride],
    flow_chain: &[azul_layout::text3::cache::LayoutFragment],
    _manager: &MockFontManager,
) -> Result<azul_layout::text3::cache::FlowLayout, LayoutError> {
    cache.layout_flow(
        content,
        overrides,
        flow_chain,
        &HashMap::new(),
        &rust_fontconfig::FcFontCache::default(),
        &mock_loaded_fonts(),
        &mut None,
    )
}

// --- Mocking Infrastructure ---

#[derive(Debug, Clone)]
pub struct MockFont {
    id: u16,
    metrics: LayoutFontMetrics,
    glyphs: HashMap<char, (u16, f32)>,
    ligatures: HashMap<String, (u16, f32)>,
}

impl azul_layout::text3::cache::ShallowClone for MockFont {
    fn shallow_clone(&self) -> Self {
        self.clone()
    }
}

impl ParsedFontTrait for MockFont {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        _language: Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError> {
        let mut result_glyphs = Vec::new();
        let char_indices: Vec<(usize, char)> = text.char_indices().collect();

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
                        font_hash: self.get_hash(),
                        font_metrics: self.get_font_metrics(),
                        style: Arc::new(style.clone()),
                        source: GlyphSource::Char,
                        logical_byte_index: byte_index,
                        logical_byte_len: lig_len,
                        content_index: 0,
                        cluster: byte_index as u32,
                        advance: *advance,
                        kerning: 0.0,
                        offset: Point::default(),
                        vertical_advance: 0.0,
                        vertical_origin_y: 0.0,
                        vertical_bearing: Point::default(),
                        orientation: GlyphOrientation::Horizontal,
                        script,
                        bidi_level: BidiLevel::new(if direction == BidiDirection::Rtl {
                            1
                        } else {
                            0
                        }),
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
                font_hash: self.get_hash(),
                font_metrics: self.get_font_metrics(),
                style: Arc::new(style.clone()),
                source: GlyphSource::Char,
                logical_byte_index: byte_index,
                logical_byte_len: char.len_utf8(),
                content_index: 0,
                cluster: byte_index as u32,
                advance,
                kerning: 0.0,
                offset: Point::default(),
                vertical_advance: 0.0,
                vertical_origin_y: 0.0,
                vertical_bearing: Point::default(),
                orientation: GlyphOrientation::Horizontal,
                script, // Simplified for mock
                bidi_level: BidiLevel::new(if direction == BidiDirection::Rtl {
                    1
                } else {
                    0
                }),
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
    ) -> Option<azul_core::geom::LogicalSize> {
        self.glyphs.values().find_map(|(id, advance)| {
            if *id == glyph_id {
                Some(azul_core::geom::LogicalSize {
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
        self.metrics
    }

    fn num_glyphs(&self) -> u16 {
        256
    }

    fn get_space_width(&self) -> Option<usize> {
        Some(10)
    }
}

#[derive(Debug)]
pub struct MockFontLoader {
    fonts: HashMap<String, Arc<MockFont>>,
}

impl FontLoaderTrait<MockFont> for MockFontLoader {
    fn load_font(&self, _font_bytes: &[u8], _font_index: usize) -> Result<MockFont, LayoutError> {
        // In a real mock, you'd probably identify the font by bytes,
        // but for tests we can just return a default font.
        Ok((**self.fonts.get("mock").unwrap()).clone())
    }
}

// A mock FontManager that doesn't use fontconfig
pub struct MockFontManager {
    loader: Arc<MockFontLoader>,
    cache: Mutex<HashMap<FontSelector, Arc<MockFont>>>,
}

impl MockFontManager {
    pub fn new(loader: Arc<MockFontLoader>) -> Self {
        Self {
            loader,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

pub fn create_mock_font_manager() -> MockFontManager {
    let mut glyphs = HashMap::new();
    // Latin lowercase
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

    // Latin uppercase (for "Hello World")
    glyphs.insert('H', (24, 10.0));
    glyphs.insert('W', (25, 12.0));

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
            cap_height: Some(70.0),
            x_height: Some(50.0),
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

pub fn create_mock_font_loader() -> Arc<MockFontLoader> {
    let mut glyphs = HashMap::new();
    // Latin lowercase
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

    // Latin uppercase (for "Hello World")
    glyphs.insert('H', (24, 10.0));
    glyphs.insert('W', (25, 12.0));

    // Digits
    ('0'..='9').for_each(|c| {
        glyphs.insert(c, (30 + (c as u32 - '0' as u32) as u16, 8.0));
    });

    let mut ligatures = HashMap::new();
    ligatures.insert("fi".to_string(), (1000, 12.0));

    let mock_font = Arc::new(MockFont {
        id: 1,
        metrics: LayoutFontMetrics {
            ascent: 80.0,
            descent: -20.0,
            cap_height: Some(70.0),
            x_height: Some(50.0),
            line_gap: 0.0,
            units_per_em: 100,
        },
        glyphs,
        ligatures,
    });

    let mut fonts = HashMap::new();
    fonts.insert("mock".to_string(), mock_font);

    Arc::new(MockFontLoader { fonts })
}

pub fn default_style() -> Arc<StyleProperties> {
    Arc::new(StyleProperties {
        font_stack: FontStack::Stack(vec![FontSelector {
            family: "mock".into(),
            ..FontSelector::default()
        }]),
        font_size_px: 10.0,
        color: ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        letter_spacing: Spacing::Px(0),
        word_spacing: Spacing::Px(0),
        line_height: LineHeight::Px(12.0),
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
        ..StyleProperties::default()
    })
}

/// Helper function to extract the text content from a layout result.
fn get_text_from_items(items: &[PositionedItem]) -> String {
    items
        .iter()
        .map(|p_item| match &p_item.item {
            ShapedItem::Cluster(c) => c.text().to_string(),
            _ => String::new(),
        })
        .collect()
}
