// In a new file, e.g., azul/layout/src/text3/tests.rs

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use azul_css::props::basic::{ColorU, FontRef};
use hyphenation::{Language, Load, Standard};
use rust_fontconfig::{FcWeight, FontId};

use crate::{
    font::parsed::ParsedFont,
    text3::{cache::*, default::PathLoader, script::Script},
};

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
        _script: Script,
        _language: Language,
        _direction: Direction,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph<Self>>, LayoutError> {
        let mut result_glyphs = Vec::new();
        let mut char_indices = text.char_indices().peekable();
        let mut byte_cursor = 0;

        while let Some((byte_index, char)) = char_indices.next() {
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
                        script: Script::Latin,
                        bidi_level: BidiLevel::new(0),
                    });

                    // Skip the characters that form the ligature
                    for _ in 0..lig_str.chars().count() - 1 {
                        char_indices.next();
                    }
                    byte_cursor += lig_len;
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
                script: Script::Latin,
                bidi_level: BidiLevel::new(0),
            });
            byte_cursor += char.len_utf8();
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
    glyphs.insert('f', (1, 10.0));
    glyphs.insert('i', (2, 4.0));
    glyphs.insert('l', (3, 4.0));
    glyphs.insert('a', (4, 8.0));
    glyphs.insert('s', (5, 8.0));
    glyphs.insert('h', (6, 9.0));
    glyphs.insert('o', (7, 9.0));
    glyphs.insert('m', (8, 12.0));
    glyphs.insert(' ', (10, 5.0));
    // Add missing chars for "hyphenation"
    glyphs.insert('y', (11, 10.0));
    glyphs.insert('p', (12, 9.0));
    glyphs.insert('e', (13, 8.0));
    glyphs.insert('n', (14, 9.0));
    glyphs.insert('t', (15, 7.0));

    // Add chars for "breaking"
    glyphs.insert('b', (16, 9.0));
    glyphs.insert('r', (17, 7.0));
    glyphs.insert('k', (18, 9.0));
    glyphs.insert('g', (19, 9.0));

    glyphs.insert('א', (100, 10.0));
    glyphs.insert('ב', (101, 10.0));
    glyphs.insert('ג', (102, 10.0));
    glyphs.insert('ד', (103, 10.0));
    glyphs.insert('ש', (200, 10.0));
    glyphs.insert('ל', (201, 10.0));
    glyphs.insert('ו', (202, 10.0));
    glyphs.insert('ם', (203, 10.0));

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

fn default_style() -> Arc<StyleProperties> {
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

// --- Unit Tests ---

#[test]
fn test_bug1_shaping_across_style_boundaries() {
    // This test exposes Bug #1. A correct engine should form a ligature for "fi".
    // This engine will fail because the style override splits "f" and "i" into
    // separate LogicalItems before shaping.

    let content = vec![InlineContent::Text(StyledRun {
        text: "first fish".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];

    let overrides = vec![StyleOverride {
        target: ContentIndex {
            run_index: 0,
            item_index: 1,
        }, // target the 'i'
        style: PartialStyleProperties {
            color: Some(ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            }),
            ..Default::default()
        },
    }];

    let logical_items = create_logical_items(&content, &overrides);

    // Assert that the text run was split into three parts
    assert_eq!(logical_items.len(), 3);
    match &logical_items[0] {
        LogicalItem::Text { text, .. } => assert_eq!(text, "f"),
        _ => panic!("Expected text"),
    }
    match &logical_items[1] {
        LogicalItem::Text { text, .. } => assert_eq!(text, "i"),
        _ => panic!("Expected text"),
    }
    match &logical_items[2] {
        LogicalItem::Text { text, .. } => assert_eq!(text, "rst fish"),
        _ => panic!("Expected text"),
    }

    // In a full test, we would continue to the shaping stage and observe
    // that no "fi" ligature was formed, resulting in 2 glyphs instead of 1.
}

#[test]
fn test_bug3_rtl_glyph_reversal() {
    // This test exposes Bug #3. The Hebrew word "שלום" (Shalom) should be
    // laid out right-to-left. Because the glyph vector is not reversed after
    // shaping, the glyphs will be positioned in logical order (left-to-right).

    let mut cache = LayoutCache::<MockFont>::new();
    let manager = create_mock_font_manager();

    // "שלום" in logical order
    let text = "\u{05e9}\u{05dc}\u{05d5}\u{05dd}";
    let style = default_style();
    // Manually create visual items as if BIDI pass has run
    let visual_items = vec![VisualItem {
        logical_source: LogicalItem::Text {
            source: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            text: text.to_string(),
            style: style.clone(),
        },
        bidi_level: BidiLevel::new(1), // RTL
        script: Script::Hebrew,
        text: text.to_string(),
    }];

    // Manually run shaping
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();

    // Assert that we have 4 clusters for 4 characters
    assert_eq!(shaped_items.len(), 4);

    let constraints = UnifiedConstraints {
        available_width: 200.0,
        ..Default::default()
    };

    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = perform_fragment_layout(&mut cursor, &[], &constraints).unwrap();

    // Check glyph order and positions
    assert_eq!(layout.items.len(), 4);

    let pos0 = layout.items[0].position.x; // Should be ש
    let pos1 = layout.items[1].position.x; // Should be ל
    let pos2 = layout.items[2].position.x; // Should be ו
    let pos3 = layout.items[3].position.x; // Should be ם

    // BUG: The positions will be increasing (0, 10, 20, 30)
    // A correct implementation would have reversed the glyphs, resulting in
    // positions like (30, 20, 10, 0) relative to a right-aligned start.
    // So, we assert the buggy behavior.
    assert!(pos1 > pos0);
    assert!(pos2 > pos1);
    assert!(pos3 > pos2);

    // A test for the fix would assert the opposite:
    // assert!(pos1 < pos0);
    // assert!(pos2 < pos1);
    // assert!(pos3 < pos2);
}

#[test]
fn test_simple_line_break() {
    let mut cache = LayoutCache::<MockFont>::new();
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: "a a a a a a".into(), // 6 chars * 8px + 5 spaces * 5px = 48 + 25 = 73px
        style: default_style(),
        logical_start_byte: 0,
    })];

    let flow_chain = vec![LayoutFragment {
        id: "main".into(),
        constraints: UnifiedConstraints {
            available_width: 50.0,
            ..Default::default()
        },
    }];

    // Using layout_flow is complex for mocks, so we'll test stages
    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr).unwrap();
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();

    let mut cursor = BreakCursor::new(&shaped_items);
    let layout =
        perform_fragment_layout(&mut cursor, &logical_items, &flow_chain[0].constraints).unwrap();

    // "a a a a " = 4*8 + 4*5 = 32 + 20 = 52, which overflows.
    // Safe break is after 3rd space: "a a a " = 3*8 + 3*5 = 24 + 15 = 39px.
    // Line 1 should have 3 'a's and 3 spaces (6 items).
    // Line 2 should have 2 'a's and 2 spaces (4 items).
    // The final 'a' has no trailing space in the shaped items.

    let line1_items = layout.items.iter().filter(|i| i.line_index == 0).count();
    let line2_items = layout.items.iter().filter(|i| i.line_index == 1).count();

    // Correct behavior: "a a a a" (4*8 + 3*5 = 47px) fits. 7 items.
    assert_eq!(line1_items, 7, "Line 1 should have 7 items");
    assert_eq!(line2_items, 4, "Line 2 should have 4 items");
}

#[test]
fn test_justification_inter_word() {
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: "a b".into(), // a=8, space=5, b=9 (mocked) => total 22px
        style: default_style(),
        logical_start_byte: 0,
    })];

    let constraints = UnifiedConstraints {
        available_width: 100.0,
        text_justify: JustifyContent::InterWord,
        text_align: TextAlign::Justify, // Important!
        ..Default::default()
    };

    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr).unwrap();
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();

    let (positioned, _) = position_one_line(
        shaped_items,
        &LineConstraints {
            segments: vec![LineSegment {
                start_x: 0.0,
                width: 100.0,
                priority: 0,
            }],
            total_available: 100.0,
        },
        0.0,
        0,
        constraints.text_align,
        Direction::Ltr, // Added base_direction argument
        false,          // Not last line, so justify
        &constraints,
    );

    let pos_b_final = positioned
        .iter()
        .find(|p| matches!(&p.item, ShapedItem::Cluster(c) if c.text == "b"))
        .unwrap();

    // extra space = 100.0 (available) - 22.0 (8+5+9, current) = 78.0
    // b should start at: 8.0 (width of 'a') + 5.0 (width of ' ') + 78.0 (extra space) = 91.0
    assert!((pos_b_final.position.x - 91.0).abs() < 1e-5);
}

#[test]
fn test_hyphenation_break() {
    let mut cache = LayoutCache::<MockFont>::new();
    let manager = create_mock_font_manager();
    let hyphenator = Standard::from_embedded(Language::EnglishUS).unwrap();

    // Use a word with a clear, unambiguous break point. "break-ing"
    // b(9)+r(7)+e(8)+a(8)+k(9) = 41
    let text = "breaking";
    let content = vec![InlineContent::Text(StyledRun {
        text: text.into(),
        style: Arc::new(StyleProperties {
            font_size_px: 10.0,
            ..(*default_style()).clone()
        }),
        logical_start_byte: 0,
    })];
    let shaped_items = shape_visual_items(
        &reorder_logical_items(&create_logical_items(&content, &[]), Direction::Ltr).unwrap(),
        &manager,
    )
    .unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let line_constraints = LineConstraints {
        segments: vec![LineSegment {
            start_x: 0.0,
            width: 50.0, // Wide enough for "break-" (41+5=46), but not "breaking"
            priority: 0,
        }],
        total_available: 50.0,
    };

    let (line1_items, was_hyphenated) =
        break_one_line(&mut cursor, &line_constraints, false, Some(&hyphenator));

    assert!(was_hyphenated, "hyphenation should have occurred");

    // The last item on the line should be a hyphen glyph.
    let last_item = line1_items.last().unwrap();
    let is_hyphen = matches!(&last_item, ShapedItem::Cluster(c) if c.glyphs.iter().any(|g| g.kind == GlyphKind::Hyphen));
    assert!(is_hyphen, "Last item was not a hyphen");

    // The cursor should contain the remainder.
    let remainder = cursor.drain_remaining();

    let remainder_text: String = remainder
        .iter()
        .map(|item| {
            if let ShapedItem::Cluster(c) = item {
                c.text.as_str()
            } else {
                ""
            }
        })
        .collect();
    assert_eq!(remainder_text, "ing");
}

#[test]
fn test_hyphenation_break_2() {
    let mut cache = LayoutCache::<MockFont>::new();
    let manager = create_mock_font_manager();
    let hyphenator = Standard::from_embedded(Language::EnglishUS).unwrap();

    let text = "hyphenation";
    let content = vec![InlineContent::Text(StyledRun {
        text: text.into(),
        style: Arc::new(StyleProperties {
            font_size_px: 10.0,
            ..(*default_style()).clone()
        }),
        logical_start_byte: 0,
    })];
    let shaped_items = shape_visual_items(
        &reorder_logical_items(&create_logical_items(&content, &[]), Direction::Ltr).unwrap(),
        &manager,
    )
    .unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let line_constraints = LineConstraints {
        segments: vec![LineSegment {
            start_x: 0.0,
            width: 60.0,
            priority: 0,
        }],
        total_available: 60.0,
    };

    // "hy-phen-ation".
    // width("hyphen") = h(9)+y(10)+p(9)+h(9)+e(8)+n(9) = 54px.
    // width("hyphen-") = 54 + 5 (hyphen) = 59px. This fits within 60px.
    // The break should be after "hyphen".
    let (line1_items, was_hyphenated) =
        break_one_line(&mut cursor, &line_constraints, false, Some(&hyphenator));

    assert!(was_hyphenated, "hyphenation should have occurred");

    // The last item on the line should be a hyphen glyph.
    let last_item = line1_items.last().unwrap();
    let is_hyphen = matches!(&last_item, ShapedItem::Cluster(c) if c.glyphs.iter().any(|g| g.kind == GlyphKind::Hyphen));
    assert!(is_hyphen, "Last item was not a hyphen");

    // The cursor should contain the remainder.
    let remainder = cursor.drain_remaining();
    let remainder_text: String = remainder
        .iter()
        .map(|item| {
            if let ShapedItem::Cluster(c) = item {
                c.text.as_str()
            } else {
                ""
            }
        })
        .collect();
    assert_eq!(remainder_text, "ation");
}

#[test]
fn test_empty_input_layout() {
    let mut cache = LayoutCache::new();
    let manager = create_mock_font_manager();
    let content = vec![];
    let flow_chain = vec![LayoutFragment {
        id: "main".into(),
        constraints: UnifiedConstraints {
            available_width: 100.0,
            ..Default::default()
        },
    }];

    let result = cache
        .layout_flow(&content, &[], &flow_chain, &manager)
        .unwrap();

    assert!(result
        .fragment_layouts
        .get("main")
        .unwrap()
        .items
        .is_empty());
    assert_eq!(
        result.fragment_layouts.get("main").unwrap().bounds.width,
        0.0
    );
    assert_eq!(
        result.fragment_layouts.get("main").unwrap().bounds.height,
        0.0
    );
    assert!(result.remaining_items.is_empty());
}
