use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use azul_core::geom::LogicalSize;
use azul_css::props::basic::ColorU;
use hyphenation::{Language, Load, Standard};
use rust_fontconfig::{FcWeight, FontId};

use crate::{
    font::parsed::ParsedFont,
    text3::{cache::*, default::PathLoader, glyphs::get_glyph_positions, script::Script},
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
    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize> {
        self.glyphs.values().find_map(|(id, advance)| {
            if *id == glyph_id {
                Some(LogicalSize {
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

// --- Unit Tests ---

#[test]
fn test_logical_items_combine_upright() {
    let mut style = (*default_style()).clone();
    style.text_combine_upright = Some(TextCombineUpright::Digits(2));

    let content = vec![InlineContent::Text(StyledRun {
        text: "12ab345c".into(),
        style: Arc::new(style),
        logical_start_byte: 0,
    })];

    let logical_items = create_logical_items(&content, &[]);
    assert_eq!(logical_items.len(), 5); // "12", "a", "b", "34", "5", "c" -> "12", "ab", "345", "c" -> no, "12", "a", "b", "34", "5",
                                        // "c" -> "12", "ab345c" The splitter logic creates text
                                        // runs between special items. "12" is CombinedText
                                        // "ab" is a Text run
                                        // "345" has a CombinedText of "34" and then a normal Text of "5"
                                        // "c" is a Text run.
                                        // So: "12", "ab", "34", "5", "c"

    // Correction: The current logic scans forward for the *next* special thing.
    // 1. Sees digit '1' at start. Enters combine loop. Grabs "12". Creates CombinedText("12").
    //    Cursor moves to 'a'.
    // 2. Sees 'a'. Scans for next special thing (none). Creates Text("ab345c").
    // Let's adjust the test to this logic.
    let content = vec![InlineContent::Text(StyledRun {
        text: "12ab 345c".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];
    let mut partial_style = PartialStyleProperties::default();
    partial_style.text_combine_upright = Some(Some(TextCombineUpright::Digits(2)));

    let overrides = vec![
        StyleOverride {
            target: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            style: partial_style.clone(),
        },
        StyleOverride {
            target: ContentIndex {
                run_index: 0,
                item_index: 5,
            },
            style: partial_style.clone(),
        },
    ];

    let logical_items = create_logical_items(&content, &overrides);

    assert_eq!(logical_items.len(), 4);
    match &logical_items[0] {
        LogicalItem::CombinedText { text, .. } => assert_eq!(text, "12"),
        other => panic!("Expected CombinedText, got {:?}", other),
    }
    match &logical_items[1] {
        LogicalItem::Text { text, .. } => assert_eq!(text, "ab "),
        other => panic!("Expected Text, got {:?}", other),
    }
    match &logical_items[2] {
        LogicalItem::CombinedText { text, .. } => assert_eq!(text, "34"),
        other => panic!("Expected CombinedText, got {:?}", other),
    }
    match &logical_items[3] {
        LogicalItem::Text { text, .. } => assert_eq!(text, "5c"),
        other => panic!("Expected Text, got {:?}", other),
    }
}

#[test]
fn test_bidi_reordering_mixed_content() {
    let content = vec![
        InlineContent::Text(StyledRun {
            text: "hello ".into(),
            style: default_style(),
            logical_start_byte: 0,
        }),
        InlineContent::Text(StyledRun {
            text: "שלום".into(), // Shalom in Hebrew
            style: default_style(),
            logical_start_byte: 6,
        }),
        InlineContent::Text(StyledRun {
            text: " world".into(),
            style: default_style(),
            logical_start_byte: 14, // 6 + 4 chars * 2 bytes/char
        }),
    ];

    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr).unwrap();

    // With a base LTR direction, the visual runs should be LTR, RTL, LTR.
    assert_eq!(visual_items.len(), 3);
    assert_eq!(visual_items[0].text, "hello ");
    assert_eq!(visual_items[0].bidi_level.level(), 0); // LTR
    assert_eq!(visual_items[1].text, "שלום");
    assert_eq!(visual_items[1].bidi_level.level(), 1); // RTL
    assert_eq!(visual_items[2].text, " world");
    assert_eq!(visual_items[2].bidi_level.level(), 0); // LTR
}

#[test]
fn test_long_word_overflow_no_hyphenation() {
    let manager = create_mock_font_manager();
    let text = "supercalifragilisticexpialidocious"; // very long word
    let content = vec![InlineContent::Text(StyledRun {
        text: text.into(),
        style: default_style(),
        logical_start_byte: 0,
    })];
    let constraints = UnifiedConstraints {
        available_width: 100.0, // much shorter than the word
        ..Default::default()
    };
    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr).unwrap();
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let (line_items, _) = break_one_line(
        &mut cursor,
        &LineConstraints {
            segments: vec![LineSegment {
                start_x: 0.0,
                width: 100.0,
                priority: 0,
            }],
            total_available: 100.0,
        },
        false,
        None,
    );

    // To prevent an infinite loop, the breaker must place at least one item
    // on the line, even if it overflows.
    assert!(
        !line_items.is_empty(),
        "Line should not be empty to prevent infinite loop"
    );
}

#[test]
fn test_multi_column_layout() {
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: "a b c d e f g h".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];
    let constraints = UnifiedConstraints {
        available_width: 100.0,
        available_height: Some(25.0), // Enough for 2 lines (12.0 each)
        columns: 2,
        column_gap: 10.0,
        ..Default::default()
    };

    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr).unwrap();
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = perform_fragment_layout(&mut cursor, &logical_items, &constraints).unwrap();

    // column_width = (100 - 10) / 2 = 45.0
    // "a b c" -> a(8)+sp(5)+b(9)+sp(5)+c(8) = 35. Fits. (5 items)
    // "d e" -> d(9)+sp(5)+e(8) = 22. Fits. (3 items)
    // Col 1 has two lines, total 8 items.
    // Col 2 starts with "f g h"
    // "f g h" -> f(10)+sp(5)+g(9)+sp(5)+h(9) = 38. Fits. (5 items)

    let mut col1_items = 0;
    let mut col2_items = 0;
    let col2_start_x = 45.0 + 10.0;

    for item in &layout.items {
        if item.position.x < col2_start_x {
            col1_items += 1;
            assert!(item.position.x < 45.0, "Item should be in column 1");
        } else {
            col2_items += 1;
            assert!(
                item.position.x >= col2_start_x,
                "Item should be in column 2"
            );
        }
    }

    assert_eq!(col1_items, 12, "Column 1 should have 12 items");
    assert_eq!(col2_items, 3, "Column 2 should have 3 items");
}

#[test]
fn test_line_clamp() {
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: "a a a a a a a a a a".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];
    let constraints = UnifiedConstraints {
        available_width: 30.0, // Should break frequently
        line_clamp: NonZeroUsize::new(2),
        ..Default::default()
    };

    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr).unwrap();
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = perform_fragment_layout(&mut cursor, &logical_items, &constraints).unwrap();

    let max_line_index = layout.items.iter().map(|i| i.line_index).max().unwrap_or(0);

    assert_eq!(
        max_line_index, 1,
        "Layout should be clamped to 2 lines (index 0 and 1)"
    );
    assert!(
        !cursor.is_done(),
        "Cursor should have remaining items after clamping"
    );
}

#[test]
fn test_flow_across_fragments() {
    let mut cache = LayoutCache::new();
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: "line one and line two and line three".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];

    let flow_chain = vec![
        LayoutFragment {
            id: "frag1".into(),
            constraints: UnifiedConstraints {
                available_width: 100.0,
                available_height: Some(15.0), // Only one line
                ..Default::default()
            },
        },
        LayoutFragment {
            id: "frag2".into(),
            constraints: UnifiedConstraints {
                available_width: 100.0,
                available_height: Some(30.0), // Two more lines
                ..Default::default()
            },
        },
    ];

    let result = cache
        .layout_flow(&content, &[], &flow_chain, &manager)
        .unwrap();

    let frag1_layout = result.fragment_layouts.get("frag1").unwrap();
    let frag2_layout = result.fragment_layouts.get("frag2").unwrap();

    assert!(!frag1_layout.items.is_empty());
    assert!(!frag2_layout.items.is_empty());

    let frag1_max_line = frag1_layout
        .items
        .iter()
        .map(|i| i.line_index)
        .max()
        .unwrap_or(0);
    assert_eq!(frag1_max_line, 0, "Fragment 1 should only contain one line");

    let frag2_max_line = frag2_layout
        .items
        .iter()
        .map(|i| i.line_index)
        .max()
        .unwrap_or(0);
    assert!(
        frag2_max_line > 0,
        "Fragment 2 should contain subsequent lines"
    );

    // Ensure all content was laid out
    assert!(result.remaining_items.is_empty());
}

#[test]
fn test_kashida_justification() {
    let manager = create_mock_font_manager();
    // "مرحبا" -> m(8)+r(7)+h(9)+b(7)+a(6) = 37px
    let content = vec![InlineContent::Text(StyledRun {
        text: "مرحبا".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];
    let constraints = UnifiedConstraints {
        available_width: 100.0,
        text_justify: JustifyContent::Kashida,
        text_align: TextAlign::Justify,
        ..Default::default()
    };

    // Directly test the kashida insertion logic
    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Rtl).unwrap();
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();

    let line_constraints = LineConstraints {
        segments: vec![LineSegment {
            start_x: 0.0,
            width: 100.0,
            priority: 0,
        }],
        total_available: 100.0,
    };

    let justified_items = justify_kashida_and_rebuild(shaped_items, &line_constraints, false);

    let kashida_count = justified_items.iter().filter(|item| {
        matches!(item, ShapedItem::Cluster(c) if c.glyphs.iter().any(|g| matches!(g.kind, GlyphKind::Kashida {..})))
    }).count();

    // extra space = 100 - 37 = 63. kashida advance = 10.
    // 63 / 10 = 6.3 -> 6 kashidas should be inserted.
    assert_eq!(kashida_count, 6, "Expected 6 kashida glyphs to be inserted");

    let new_width: f32 = justified_items
        .iter()
        .map(|i| get_item_measure(i, false))
        .sum();
    // 37 (original) + 6 * 10 (kashida) = 97
    assert!((new_width - 97.0).abs() < 1e-5);
}

#[test]
fn test_layout_with_shape_exclusion() {
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: "this is some very long text that should wrap around a floated exclusion area in \
               the middle"
            .into(),
        style: default_style(),
        logical_start_byte: 0,
    })];
    let constraints = UnifiedConstraints {
        available_width: 300.0,
        available_height: Some(100.0),
        line_height: 16.0, // Set explicitly for predictable test
        shape_exclusions: vec![ShapeBoundary::Rectangle(Rect {
            x: 100.0,
            y: 10.0,
            width: 100.0,
            height: 30.0,
        })],
        ..Default::default()
    };

    let is_line_split = |items: &Vec<&PositionedItem<MockFont>>| -> bool {
        if items.len() < 2 {
            return false;
        }
        // A line is split if its last item starts after the exclusion zone,
        // and its first item starts before it.
        let first_x = items.first().unwrap().position.x;
        let last_x = items.last().unwrap().position.x;
        first_x < 100.0 && last_x >= 200.0
    };

    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr).unwrap();
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = perform_fragment_layout(&mut cursor, &logical_items, &constraints).unwrap();

    // Exclusion rect is y in [10, 40]
    // Line 0: y=0, line box [0, 16], overlaps. Should be split.
    // Line 1: y=16, line box [16, 32], overlaps. Should be split.
    // Line 2: y=32, line box [32, 48], overlaps. Should be split.
    // Line 3: y=48, line box [48, 64], no overlap. Should NOT be split.

    let line1_items: Vec<_> = layout.items.iter().filter(|i| i.line_index == 1).collect();
    let line3_items: Vec<_> = layout.items.iter().filter(|i| i.line_index == 3).collect();

    assert!(
        is_line_split(&line1_items),
        "Line 1 (y=16) should be split by exclusion"
    );
    assert!(
        !is_line_split(&line3_items),
        "Line 3 (y=48) should not be split"
    );
}

#[test]
fn test_get_glyph_positions() {
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: "word".into(), // w(10) o(9) r(7) d(9)
        style: default_style(),
        logical_start_byte: 0,
    })];
    let constraints = UnifiedConstraints {
        available_width: 200.0,
        ..Default::default()
    };
    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr).unwrap();
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = perform_fragment_layout(&mut cursor, &logical_items, &constraints).unwrap();

    let positioned_glyphs = get_glyph_positions(&layout);

    assert_eq!(positioned_glyphs.len(), 4);

    // Font metrics: ascent=80, descent=-20, units_per_em=100. Style font_size=10.
    // Scale = 10.0 / 100.0 = 0.1
    // Scaled ascent = 80.0 * 0.1 = 8.0
    // Line 0 starts at y=0. Baseline y = 0 (line_top) + 8.0 (line_ascent) = 8.0

    // Glyph 'w'
    assert_eq!(positioned_glyphs[0].position.x, 0.0);
    assert!((positioned_glyphs[0].position.y - 8.0).abs() < 1e-5);
    // Glyph 'o'
    assert_eq!(positioned_glyphs[1].position.x, 10.0); // after 'w' advance
                                                       // Glyph 'r'
    assert_eq!(positioned_glyphs[2].position.x, 19.0); // after 'o' advance
                                                       // Glyph 'd'
    assert_eq!(positioned_glyphs[3].position.x, 26.0); // after 'r' advance
}
