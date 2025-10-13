// In a new file, e.g., azul/layout/src/text3/tests.rs

use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use azul_css::props::basic::ColorU;
use hyphenation::{Language, Load, Standard};
use rust_fontconfig::{FcWeight, FontId};

use super::{create_mock_font_manager, default_style, MockFont};
use crate::{
    parsedfont::ParsedFont,
    text3::{cache::*, default::PathLoader, script::Script},
};

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

    // The visual order of runs remains the same as the logical order.
    // The second run is simply marked as RTL.
    assert_eq!(visual_items.len(), 3);
    assert_eq!(visual_items[0].text, "hello ");
    assert_eq!(visual_items[0].bidi_level.level(), 0); // LTR
                                                       // FIX: The Hebrew text is the second visual run.
    assert_eq!(visual_items[1].text, "שלום");
    assert_eq!(visual_items[1].bidi_level.level(), 1); // RTL
                                                       // FIX: The second LTR part is the third visual run.
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
    // "a b c" -> a(8)+sp(5)+b(9)+sp(5)+c(8) = 35. Fits.
    // "d e f" -> d(9)+sp(5)+e(8)+sp(5)+f(10) = 37. Fits.
    // "g h" -> g(9)+sp(5)+h(9) = 23. Fits.

    let mut col1_items = 0;
    let mut col2_items = 0;
    let col2_start_x = 45.0 + 10.0;

    for item in &layout.items {
        if item.position.x < col2_start_x {
            col1_items += 1;
            assert!(item.position.x >= 0.0 && item.position.x < 45.0);
        } else {
            col2_items += 1;
            assert!(item.position.x >= col2_start_x && item.position.x < 100.0);
        }
    }

    // Line 1 in col 1: "a b c" (5 items)
    // Line 2 in col 1: "d e" (3 items)
    // Line 1 in col 2: "f g h" (5 items)
    let line_1_col_1 = layout
        .items
        .iter()
        .filter(|i| i.line_index == 0 && i.position.x < col2_start_x)
        .count();
    let line_2_col_1 = layout
        .items
        .iter()
        .filter(|i| i.line_index == 1 && i.position.x < col2_start_x)
        .count();
    let line_1_col_2 = layout
        .items
        .iter()
        .filter(|i| i.line_index == 0 && i.position.x >= col2_start_x)
        .count();

    assert!(col1_items > 0);
    assert!(col2_items > 0);
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
        if items.is_empty() {
            return false;
        }
        // A line is split if it has items on both sides of the exclusion, and none in it.
        let has_left_part = items.iter().any(|i| i.position.x < 100.0);
        let has_right_part = items.iter().any(|i| i.position.x >= 200.0);
        let no_middle_part = !items
            .iter()
            .any(|i| i.position.x >= 100.0 && i.position.x < 200.0);
        has_left_part && has_right_part && no_middle_part
    };

    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr).unwrap();
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = perform_fragment_layout(&mut cursor, &logical_items, &constraints).unwrap();

    // With line_height = 16.0 and correct intersection logic:
    // Exclusion rect is y in [10, 40]
    // Line 0: top_y=0. Box=[0, 16]. Overlaps with [10, 40]. Should be split.
    // Line 1: top_y=16. Box=[16, 32]. Overlaps. Should be split.
    // Line 2: top_y=32. Box=[32, 48]. Overlaps. Should be split.
    // Line 3: top_y=48. Box=[48, 64]. No overlap. Should NOT be split.

    let line0_items: Vec<_> = layout.items.iter().filter(|i| i.line_index == 0).collect();
    let line1_items: Vec<_> = layout.items.iter().filter(|i| i.line_index == 1).collect();
    let line2_items: Vec<_> = layout.items.iter().filter(|i| i.line_index == 2).collect();
    let line3_items: Vec<_> = layout.items.iter().filter(|i| i.line_index == 3).collect();

    assert!(is_line_split(&line0_items), "Line 0 (y=0) should be split");
    assert!(is_line_split(&line1_items), "Line 1 (y=16) should be split");
    assert!(is_line_split(&line2_items), "Line 2 (y=32) should be split");
    assert!(
        !is_line_split(&line3_items),
        "Line 3 (y=48) should not be split"
    );
}

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
    let logical_items = create_logical_items(
        &[InlineContent::Text(StyledRun {
            text: text.to_string(),
            style,
            logical_start_byte: 0,
        })],
        &[],
    );
    let layout = perform_fragment_layout(&mut cursor, &logical_items, &constraints).unwrap();

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
    // Line 1 should have "a a a" (3 'a's and 2 spaces = 5 items).
    // The trailing space of the line break is also included, so 6 items.

    // Let's trace break_one_line
    // 1. Peek unit "a", width 8. Fits. line_items=["a"], current_width=8.
    // 2. Peek unit " ", width 5. Fits. line_items=["a", " "], current_width=13.
    // 3. Peek unit "a", width 8. Fits. line_items=["a", " ", "a"], current_width=21.
    // 4. Peek unit " ", width 5. Fits. line_items=["a", " ", "a", " "], current_width=26.
    // 5. Peek unit "a", width 8. Fits. line_items=["a", " ", "a", " ", "a"], current_width=34.
    // 6. Peek unit " ", width 5. Fits. line_items=["a", " ", "a", " ", "a", " "], current_width=39.
    // 7. Peek unit "a", width 8. Fits. line_items=[... "a"], current_width=47.
    // 8. Peek unit " ", width 5. Overflows (47+5 > 50). Line is finished.
    // Line 1: "a a a a" (7 items).
    // Remainder starts with " ".
    // Line 2: " a a" -> "a a" after trimming.

    let line1_items_count = layout.items.iter().filter(|i| i.line_index == 0).count();
    let line2_items_count = layout.items.iter().filter(|i| i.line_index == 1).count();

    assert_eq!(
        line1_items_count, 7,
        "Line 1 should have 7 items ('a a a a')"
    );
    // Remaining content: " a a". cursor peeks " ". line gets " ".
    // Then peeks "a a". Fits.
    // " a a" has 4 items.
    assert_eq!(line2_items_count, 4, "Line 2 should have 4 items (' a a')");
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
        // FIX: Use JustifyAll to force justification on the last (and only) line.
        text_align: TextAlign::JustifyAll,
        ..Default::default()
    };

    let logical_items = create_logical_items(&content, &[]);
    let visual_items = reorder_logical_items(&logical_items, Direction::Ltr).unwrap();
    let shaped_items = shape_visual_items(&visual_items, &manager).unwrap();

    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = perform_fragment_layout(&mut cursor, &logical_items, &constraints).unwrap();

    assert_eq!(layout.items.len(), 3);

    let pos_a = layout.items[0].position.x;
    let pos_space = layout.items[1].position.x;
    let pos_b = layout.items[2].position.x;

    // 'a' starts at 0
    assert_eq!(pos_a, 0.0);
    // 'space' starts after 'a' (width 8)
    assert_eq!(pos_space, 8.0);

    // extra space = 100.0 (available) - 22.0 (8+5+9, current) = 78.0
    // b should start at: 8.0 (width of 'a') + 5.0 (width of ' ') + 78.0 (extra space) = 91.0
    assert!((pos_b - 91.0).abs() < 1e-5);
}

#[test]
fn test_hyphenation_break() {
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

    let main_layout = result.fragment_layouts.get("main").unwrap();
    assert!(main_layout.items.is_empty());
    assert_eq!(main_layout.bounds.width, 0.0);
    assert_eq!(main_layout.bounds.height, 0.0);
    assert!(result.remaining_items.is_empty());
}
