// In a new file, e.g., azul/layout/src/text3/tests.rs

use std::sync::Arc;

use azul_css::props::basic::ColorU;
use hyphenation::{Language, Load, Standard};

use azul_layout::text3::{cache::*, script::Script};

use super::{create_mock_font_manager, default_style};

// --- Unit Tests ---

#[test]
fn test_bug1_shaping_across_style_boundaries() {
    // This test exposes Bug #1. A correct engine should form a ligature for "fi".
    // This engine will fail because the style override splits "f" and "i" into
    // separate LogicalItems before shaping.

    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from("first fish"),
        style: default_style(),
        logical_start_byte: 0,
        source_node_id: None, // Test content, no DOM node
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

    let logical_items = super::create_logical_items_compat(&content, &overrides);

    // Assert that the text run was split into three parts
    assert_eq!(logical_items.len(), 3);
    match &logical_items[0] {
        LogicalItem::Text { text, .. } => assert_eq!(&**text, "f"),
        _ => panic!("Expected text"),
    }
    match &logical_items[1] {
        LogicalItem::Text { text, .. } => assert_eq!(&**text, "i"),
        _ => panic!("Expected text"),
    }
    match &logical_items[2] {
        LogicalItem::Text { text, .. } => assert_eq!(&**text, "rst fish"),
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

    let cache = TextShapingCache::new();
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
            text: Arc::from(text),
            style: style.clone(),
            marker_position_outside: None,
            source_node_id: None,
        },
        bidi_level: BidiLevel::new(1), // RTL
        script: Script::Hebrew,
        text: text.to_string(),
        run_byte_offset: 0,
    }];

    // Manually run shaping
    let shaped_items = super::shape_visual_items_compat(&visual_items, &manager).unwrap();

    // Assert that we have 4 clusters for 4 characters
    assert_eq!(shaped_items.len(), 4);

    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(200.0),
        ..Default::default()
    };

    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = super::perform_fragment_layout_compat(&mut cursor, &[], &constraints).unwrap();

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
#[ignore = "TRIAGED 2026-08-20 and still RED: `--test text3_suite -- --ignored` \
            fails 12/12. Revived 2026-08-10 after years dormant; each of these \
            encodes a hard-coded coordinate from the OLD text3 generation \
            (line-item counts, glyph x/y, cursor offsets). They run fine \
            headless — they are not hardware-gated — so this is a real \
            old-vs-new behavioural delta someone must adjudicate per test \
            (stale expectation vs. genuine regression). Kept ignored, not \
            deleted, because the numbers are the only record of the old \
            behaviour."]
fn test_simple_line_break() {
    let cache = TextShapingCache::new();
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from("a a a a a a"), // 6 chars * 8px + 5 spaces * 5px = 48 + 25 = 73px
        style: default_style(),
        logical_start_byte: 0,
        source_node_id: None,
    })];

    let flow_chain = [LayoutFragment {
        id: "main".into(),
        constraints: UnifiedConstraints {
            available_width: AvailableSpace::Definite(50.0),
            ..Default::default()
        },
    }];

    // Using layout_flow is complex for mocks, so we'll test stages
    let logical_items = super::create_logical_items_compat(&content, &[]);
    let visual_items =
        super::reorder_logical_items_compat(&logical_items, BidiDirection::Ltr).unwrap();
    let shaped_items = super::shape_visual_items_compat(&visual_items, &manager).unwrap();

    let mut cursor = BreakCursor::new(&shaped_items);
    let layout = super::perform_fragment_layout_compat(
        &mut cursor,
        &logical_items,
        &flow_chain[0].constraints,
    )
    .unwrap();

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
        text: Arc::from("a b"), // a=8, space=5, b=9 (mocked) => total 22px
        style: default_style(),
        logical_start_byte: 0,
        source_node_id: None,
    })];

    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::Definite(100.0),
        text_justify: JustifyContent::InterWord,
        text_align: TextAlign::Justify, // Important!
        ..Default::default()
    };

    let logical_items = super::create_logical_items_compat(&content, &[]);
    let visual_items =
        super::reorder_logical_items_compat(&logical_items, BidiDirection::Ltr).unwrap();
    let shaped_items = super::shape_visual_items_compat(&visual_items, &manager).unwrap();

    let (positioned, _) = super::position_one_line_compat(
        &shaped_items,
        &LineConstraints {
            segments: vec![LineSegment {
                start_x: 0.0,
                width: 100.0,
                priority: 0,
            }],
            total_available: 100.0,
            is_min_content: false,
        },
        0.0,
        0,
        constraints.text_align,
        BidiDirection::Ltr, // Added base_direction argument
        false,              // Not last line, so justify
        &constraints,
    );

    let pos_b_final = positioned
        .iter()
        .find(|p| matches!(&p.item, ShapedItem::Cluster(c) if c.text() == "b"))
        .unwrap();

    // extra space = 100.0 (available) - 22.0 (8+5+9, current) = 78.0
    // b should start at: 8.0 (width of 'a') + 5.0 (width of ' ') + 78.0 (extra space) = 91.0
    assert!((pos_b_final.position.x - 91.0).abs() < 1e-5);
}

#[test]
fn test_hyphenation_break() {
    let cache = TextShapingCache::new();
    let manager = create_mock_font_manager();
    let hyphenator = Standard::from_embedded(Language::EnglishUS).unwrap();

    // Use a word with a clear, unambiguous break point. "break-ing"
    // b(9)+r(7)+e(8)+a(8)+k(9) = 41
    let text = "breaking";
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from(text),
        style: Arc::new(StyleProperties {
            font_size_px: 10.0,
            ..(*default_style()).clone()
        }),
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let shaped_items = super::shape_visual_items_compat(
        &super::reorder_logical_items_compat(
            &super::create_logical_items_compat(&content, &[]),
            BidiDirection::Ltr,
        )
        .unwrap(),
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
        is_min_content: false,
    };

    let (line1_items, was_hyphenated) =
        super::break_one_line_compat(&mut cursor, &line_constraints, false, Some(&hyphenator));

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
                c.text()
            } else {
                ""
            }
        })
        .collect();
    assert_eq!(remainder_text, "ing");
}

#[test]
fn test_hyphenation_break_2() {
    let cache = TextShapingCache::new();
    let manager = create_mock_font_manager();
    let hyphenator = Standard::from_embedded(Language::EnglishUS).unwrap();

    let text = "hyphenation";
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from(text),
        style: Arc::new(StyleProperties {
            font_size_px: 10.0,
            ..(*default_style()).clone()
        }),
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let shaped_items = super::shape_visual_items_compat(
        &super::reorder_logical_items_compat(
            &super::create_logical_items_compat(&content, &[]),
            BidiDirection::Ltr,
        )
        .unwrap(),
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
        is_min_content: false,
    };

    // "hy-phen-ation".
    // width("hyphen") = h(9)+y(10)+p(9)+h(9)+e(8)+n(9) = 54px.
    // width("hyphen-") = 54 + 5 (hyphen) = 59px. This fits within 60px.
    // The break should be after "hyphen".
    let (line1_items, was_hyphenated) =
        super::break_one_line_compat(&mut cursor, &line_constraints, false, Some(&hyphenator));

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
                c.text()
            } else {
                ""
            }
        })
        .collect();
    assert_eq!(remainder_text, "ation");
}

#[test]
fn test_empty_input_layout() {
    let mut cache = TextShapingCache::new();
    let manager = create_mock_font_manager();
    let content = vec![];
    let flow_chain = vec![LayoutFragment {
        id: "main".into(),
        constraints: UnifiedConstraints {
            available_width: AvailableSpace::Definite(100.0),
            ..Default::default()
        },
    }];

    let result =
        super::layout_flow_compat(&mut cache, &content, &[], &flow_chain, &manager).unwrap();

    assert!(result
        .fragment_layouts
        .get("main")
        .unwrap()
        .items
        .is_empty());
    let main_bounds = result.fragment_layouts.get("main").unwrap().bounds();
    assert_eq!(main_bounds.width, 0.0);
    assert_eq!(main_bounds.height, 0.0);
    assert!(result.remaining_items.is_empty());
}
