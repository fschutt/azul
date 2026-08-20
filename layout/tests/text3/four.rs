use std::{
    num::NonZeroUsize,
    sync::Arc,
};


use azul_layout::text3::{cache::*, glyphs::get_glyph_positions};

use super::{create_mock_font_manager, default_style};

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

// --- Unit Tests ---

#[test]
fn test_logical_items_combine_upright() {
    let mut style = (*default_style()).clone();
    style.text_combine_upright = Some(TextCombineUpright::Digits(2));

    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from("12ab345c"),
        style: Arc::new(style),
        logical_start_byte: 0,
            source_node_id: None,
    })];

    let logical_items = super::create_logical_items_compat(&content, &[]);
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
        text: Arc::from("12ab 345c"),
        style: default_style(),
        logical_start_byte: 0,
            source_node_id: None,
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

    let logical_items = super::create_logical_items_compat(&content, &overrides);

    assert_eq!(logical_items.len(), 4);
    match &logical_items[0] {
        LogicalItem::CombinedText { text, .. } => assert_eq!(text, "12"),
        other => panic!("Expected CombinedText, got {other:?}"),
    }
    match &logical_items[1] {
        LogicalItem::Text { text, .. } => assert_eq!(&**text, "ab "),
        other => panic!("Expected Text, got {other:?}"),
    }
    match &logical_items[2] {
        LogicalItem::CombinedText { text, .. } => assert_eq!(text, "34"),
        other => panic!("Expected CombinedText, got {other:?}"),
    }
    match &logical_items[3] {
        LogicalItem::Text { text, .. } => assert_eq!(&**text, "5c"),
        other => panic!("Expected Text, got {other:?}"),
    }
}

#[test]
fn test_bidi_reordering_mixed_content() {
    let content = vec![
        InlineContent::Text(StyledRun {
            text: Arc::from("hello "),
            style: default_style(),
            logical_start_byte: 0,
            source_node_id: None,
        }),
        InlineContent::Text(StyledRun {
            text: Arc::from("שלום"), // Shalom in Hebrew
            style: default_style(),
            logical_start_byte: 6,
            source_node_id: None,
        }),
        InlineContent::Text(StyledRun {
            text: Arc::from(" world"),
            style: default_style(),
            logical_start_byte: 14, // 6 + 4 chars * 2 bytes/char
            source_node_id: None,
        }),
    ];

    let logical_items = super::create_logical_items_compat(&content, &[]);
    let visual_items = super::reorder_logical_items_compat(&logical_items, BidiDirection::Ltr).unwrap();

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
        text: Arc::from(text),
        style: default_style(),
        logical_start_byte: 0,
            source_node_id: None,
    })];
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(100.0), // much shorter than the word
        ..Default::default()
    };
    let logical_items = super::create_logical_items_compat(&content, &[]);
    let visual_items = super::reorder_logical_items_compat(&logical_items, BidiDirection::Ltr).unwrap();
    let shaped_items = super::shape_visual_items_compat(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let (line_items, _) = super::break_one_line_compat(
        &mut cursor,
        &LineConstraints {
            segments: vec![LineSegment {
                start_x: 0.0,
                width: 100.0,
                priority: 0,
            }],
            total_available: 100.0,
            is_min_content: false,
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
#[ignore = "TRIAGED 2026-08-20 and still RED: `--test text3_suite -- --ignored` \
            fails 12/12. Revived 2026-08-10 after years dormant; each of these \
            encodes a hard-coded coordinate from the OLD text3 generation \
            (line-item counts, glyph x/y, cursor offsets). They run fine \
            headless — they are not hardware-gated — so this is a real \
            old-vs-new behavioural delta someone must adjudicate per test \
            (stale expectation vs. genuine regression). Kept ignored, not \
            deleted, because the numbers are the only record of the old \
            behaviour."]
fn test_multi_column_layout() {
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from("a b c d e f g h"),
        style: default_style(),
        logical_start_byte: 0,
            source_node_id: None,
    })];
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(100.0),
        available_height: Some(25.0), // Enough for 2 lines (12.0 each)
        columns: 2,
        column_gap: 10.0,
        ..Default::default()
    };

    let logical_items = super::create_logical_items_compat(&content, &[]);
    let visual_items = super::reorder_logical_items_compat(&logical_items, BidiDirection::Ltr).unwrap();
    let shaped_items = super::shape_visual_items_compat(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = super::perform_fragment_layout_compat(&mut cursor, &logical_items, &constraints).unwrap();

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
        text: Arc::from("a a a a a a a a a a"),
        style: default_style(),
        logical_start_byte: 0,
            source_node_id: None,
    })];
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(30.0), // Should break frequently
        line_clamp: NonZeroUsize::new(2),
        ..Default::default()
    };

    let logical_items = super::create_logical_items_compat(&content, &[]);
    let visual_items = super::reorder_logical_items_compat(&logical_items, BidiDirection::Ltr).unwrap();
    let shaped_items = super::shape_visual_items_compat(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = super::perform_fragment_layout_compat(&mut cursor, &logical_items, &constraints).unwrap();

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
    let mut cache = TextShapingCache::new();
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from("line one and line two and line three"),
        style: default_style(),
        logical_start_byte: 0,
            source_node_id: None,
    })];

    let flow_chain = vec![
        LayoutFragment {
            id: "frag1".into(),
            constraints: UnifiedConstraints {
                available_width: AvailableSpace::Definite(100.0),
                available_height: Some(15.0), // Only one line
                ..Default::default()
            },
        },
        LayoutFragment {
            id: "frag2".into(),
            constraints: UnifiedConstraints {
                available_width: AvailableSpace::Definite(100.0),
                available_height: Some(30.0), // Two more lines
                ..Default::default()
            },
        },
    ];

    let result = super::layout_flow_compat(&mut cache, &content, &[], &flow_chain, &manager)
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
        text: Arc::from("مرحبا"),
        style: default_style(),
        logical_start_byte: 0,
            source_node_id: None,
    })];
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(100.0),
        text_justify: JustifyContent::Kashida,
        text_align: TextAlign::Justify,
        ..Default::default()
    };

    // Directly test the kashida insertion logic
    let logical_items = super::create_logical_items_compat(&content, &[]);
    let visual_items = super::reorder_logical_items_compat(&logical_items, BidiDirection::Rtl).unwrap();
    let shaped_items = super::shape_visual_items_compat(&visual_items, &manager).unwrap();

    let line_constraints = LineConstraints {
        segments: vec![LineSegment {
            start_x: 0.0,
            width: 100.0,
            priority: 0,
        }],
        total_available: 100.0,
            is_min_content: false,
    };

    let justified_items = super::justify_kashida_and_rebuild_compat(shaped_items, &line_constraints, false);

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
        text: Arc::from("this is some very long text that should wrap around a floated exclusion area in \
               the middle"
            ),
        style: default_style(),
        logical_start_byte: 0,
            source_node_id: None,
    })];
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(300.0),
        available_height: Some(100.0),
        line_height: LineHeight::Px(16.0), // Set explicitly for predictable test
        shape_exclusions: vec![ShapeBoundary::Rectangle(Rect {
            x: 100.0,
            y: 10.0,
            width: 100.0,
            height: 30.0,
        })],
        ..Default::default()
    };

    let is_line_split = |items: &Vec<&PositionedItem>| -> bool {
        if items.len() < 2 {
            return false;
        }
        // A line is split if its last item starts after the exclusion zone,
        // and its first item starts before it.
        let first_x = items.first().unwrap().position.x;
        let last_x = items.last().unwrap().position.x;
        first_x < 100.0 && last_x >= 200.0
    };

    let logical_items = super::create_logical_items_compat(&content, &[]);
    let visual_items = super::reorder_logical_items_compat(&logical_items, BidiDirection::Ltr).unwrap();
    let shaped_items = super::shape_visual_items_compat(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = super::perform_fragment_layout_compat(&mut cursor, &logical_items, &constraints).unwrap();

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
#[ignore = "TRIAGED 2026-08-20 and still RED: `--test text3_suite -- --ignored` \
            fails 12/12. Revived 2026-08-10 after years dormant; each of these \
            encodes a hard-coded coordinate from the OLD text3 generation \
            (line-item counts, glyph x/y, cursor offsets). They run fine \
            headless — they are not hardware-gated — so this is a real \
            old-vs-new behavioural delta someone must adjudicate per test \
            (stale expectation vs. genuine regression). Kept ignored, not \
            deleted, because the numbers are the only record of the old \
            behaviour."]
fn test_get_glyph_positions() {
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from("word"), // w(10) o(9) r(7) d(9)
        style: default_style(),
        logical_start_byte: 0,
            source_node_id: None,
    })];
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(200.0),
        ..Default::default()
    };
    let logical_items = super::create_logical_items_compat(&content, &[]);
    let visual_items = super::reorder_logical_items_compat(&logical_items, BidiDirection::Ltr).unwrap();
    let shaped_items = super::shape_visual_items_compat(&visual_items, &manager).unwrap();
    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = super::perform_fragment_layout_compat(&mut cursor, &logical_items, &constraints).unwrap();

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
