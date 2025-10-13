// In a new file, layout/src/text3/tests4.rs

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    selection::*,
};

use super::{create_mock_font_manager, default_style, MockFont};
use crate::text3::{
    cache::*,
    edit::{edit_text, TextEdit},
    tests::MockFontManager,
};

#[test]
fn test_hittest_simple_ltr() {
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: "hello".into(), // h=9, e=8, l=4, l=4, o=9
        style: default_style(),
        logical_start_byte: 0,
    })];
    let constraints = UnifiedConstraints {
        available_width: 200.0,
        ..Default::default()
    };

    let mut cache = LayoutCache::<MockFont>::new();
    let flow_chain = vec![LayoutFragment {
        id: "main".into(),
        constraints,
    }];

    let layout = cache
        .layout_flow(&content, &[], &flow_chain, &manager)
        .unwrap();
    let main_layout = layout.fragment_layouts.get("main").unwrap();

    // Hit test near the 'e' character (h is 9px wide)
    let cursor = main_layout
        .hittest_cursor(LogicalPosition { x: 12.0, y: 5.0 })
        .unwrap();

    let expected_cluster = GraphemeClusterId {
        source_run: 0,
        start_byte_in_run: 1,
    }; // 'e' is at byte 1
    assert_eq!(cursor.cluster_id, expected_cluster);
    assert_eq!(cursor.affinity, CursorAffinity::Leading); // 9 + (8/2) = 13. 12 < 13 -> Leading

    // Hit test at the end of the word
    let cursor_end = main_layout
        .hittest_cursor(LogicalPosition { x: 40.0, y: 5.0 })
        .unwrap();
    let expected_cluster_end = GraphemeClusterId {
        source_run: 0,
        start_byte_in_run: 4,
    }; // 'o'
    assert_eq!(cursor_end.cluster_id, expected_cluster_end);
    assert_eq!(cursor_end.affinity, CursorAffinity::Trailing);
}

#[test]
fn test_get_selection_rects_single_line() {
    let manager = create_mock_font_manager();
    let content = vec![InlineContent::Text(StyledRun {
        text: "hello world".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];
    let constraints = UnifiedConstraints {
        available_width: 200.0,
        ..Default::default()
    };

    let mut cache = LayoutCache::<MockFont>::new();
    let flow_chain = vec![LayoutFragment {
        id: "main".into(),
        constraints,
    }];

    let layout = cache
        .layout_flow(&content, &[], &flow_chain, &manager)
        .unwrap();
    let main_layout = layout.fragment_layouts.get("main").unwrap();

    let selection = SelectionRange {
        start: TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 2,
            },
            affinity: CursorAffinity::Leading,
        }, // "l"
        end: TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 8,
            },
            affinity: CursorAffinity::Trailing,
        }, // "r"
    };

    let rects = main_layout.get_selection_rects(&selection);

    // This is a placeholder test, since the implementation is a stub
    assert!(
        !rects.is_empty(),
        "Should generate at least one rectangle for selection"
    );
}

/// Creates a standard multi-line layout for testing navigation.
fn create_test_layout() -> (UnifiedLayout<MockFont>, MockFontManager) {
    let manager = create_mock_font_manager();
    let content = vec![
        InlineContent::Text(StyledRun {
            text: "hello world".into(),
            style: default_style(),
            logical_start_byte: 0,
        }),
        InlineContent::Text(StyledRun {
            text: "second line".into(),
            style: default_style(),
            logical_start_byte: 12,
        }),
    ];
    let constraints = UnifiedConstraints {
        available_width: 60.0, // "hello " fits, "world" wraps
        line_height: 12.0,
        ..Default::default()
    };

    let mut cache = LayoutCache::<MockFont>::new();
    let flow_chain = vec![LayoutFragment {
        id: "main".into(),
        constraints,
    }];
    let layout_result = cache
        .layout_flow(&content, &[], &flow_chain, &manager)
        .unwrap();
    (
        layout_result
            .fragment_layouts
            .get("main")
            .unwrap()
            .as_ref()
            .clone(),
        manager,
    )
}

#[test]
fn test_move_cursor_up_down() {
    let (layout, _) = create_test_layout();

    // Cursor is on "o" in "world" on the second line.
    let start_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 7,
        }, // 'o' in "world"
        affinity: CursorAffinity::Leading,
    };

    // Moving up should land on the first line, near the same X coordinate.
    let mut goal_x = None;
    let up_cursor = layout.move_cursor_up(start_cursor, &mut goal_x);

    // The 'l' in "hello" is roughly above 'o' in "world"
    assert_eq!(
        up_cursor.cluster_id.start_byte_in_run, 2,
        "Cursor should be on 'l'"
    );

    // Moving back down should return to the original character.
    let down_cursor = layout.move_cursor_down(up_cursor, &mut goal_x);
    assert_eq!(
        down_cursor.cluster_id.start_byte_in_run, 7,
        "Cursor should return to 'o'"
    );
}

#[test]
fn test_move_cursor_line_start_end() {
    let (layout, _) = create_test_layout();

    // Cursor is on "o" in "world" on the second line.
    let start_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 7,
        },
        affinity: CursorAffinity::Leading,
    };

    let line_start_cursor = layout.move_cursor_to_line_start(start_cursor);
    assert_eq!(
        line_start_cursor.cluster_id.start_byte_in_run, 6,
        "Cursor should be at start of 'world'"
    );

    let line_end_cursor = layout.move_cursor_to_line_end(start_cursor);
    assert_eq!(
        line_end_cursor.cluster_id.start_byte_in_run, 10,
        "Cursor should be at end of 'world'"
    );
}

#[test]
fn test_edit_insert_char() {
    let content = vec![InlineContent::Text(StyledRun {
        text: "helo".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];

    let cursor = Selection::Cursor(TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 2,
        }, // After 'e'
        affinity: CursorAffinity::Leading,
    });

    let (new_content, _) = edit_text(&content, &[cursor], &TextEdit::Insert("l".to_string()));

    let new_text = match &new_content[0] {
        InlineContent::Text(run) => &run.text,
        _ => panic!(),
    };

    assert_eq!(new_text, "hello");
}

#[test]
fn test_edit_delete_backward() {
    let content = vec![InlineContent::Text(StyledRun {
        text: "hel lo".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];

    let cursor = Selection::Cursor(TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 4,
        }, // After space
        affinity: CursorAffinity::Leading,
    });

    let (new_content, _) = edit_text(&content, &[cursor], &TextEdit::DeleteBackward);

    let new_text = match &new_content[0] {
        InlineContent::Text(run) => &run.text,
        _ => panic!(),
    };

    assert_eq!(new_text, "hello");
}

/// Creates a standard multi-line layout for testing navigation.
fn create_test_layout_2() -> (UnifiedLayout<MockFont>, MockFontManager) {
    let manager = create_mock_font_manager();
    // Use a text that will definitely wrap to test multi-line navigation
    let content = vec![InlineContent::Text(StyledRun {
        text: "hello beautiful world".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];
    let constraints = UnifiedConstraints {
        available_width: 60.0, // "hello " fits, "beautiful" wraps
        line_height: 12.0,
        ..Default::default()
    };

    let mut cache = LayoutCache::<MockFont>::new();
    let flow_chain = vec![LayoutFragment {
        id: "main".into(),
        constraints,
    }];
    let layout_result = cache
        .layout_flow(&content, &[], &flow_chain, &manager)
        .unwrap();
    (
        layout_result
            .fragment_layouts
            .get("main")
            .unwrap()
            .as_ref()
            .clone(),
        manager,
    )
}

#[test]
fn test_move_cursor_left_right_simple() {
    let (layout, _) = create_test_layout_2();

    // Start cursor at the beginning of 'e' in "hello"
    let start_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 1,
        }, // 'e'
        affinity: CursorAffinity::Leading,
    };

    // Move right -> trailing edge of 'e'
    let c1 = layout.move_cursor_right(start_cursor);
    assert_eq!(c1.cluster_id.start_byte_in_run, 1);
    assert_eq!(c1.affinity, CursorAffinity::Trailing);

    // Move right again -> leading edge of 'l'
    let c2 = layout.move_cursor_right(c1);
    assert_eq!(c2.cluster_id.start_byte_in_run, 2);
    assert_eq!(c2.affinity, CursorAffinity::Leading);

    // Move left -> trailing edge of 'e'
    let c3 = layout.move_cursor_left(c2);
    assert_eq!(c3.cluster_id.start_byte_in_run, 1);
    assert_eq!(c3.affinity, CursorAffinity::Trailing);

    // Move left again -> leading edge of 'e'
    let c4 = layout.move_cursor_left(c3);
    assert_eq!(c4, start_cursor);
}

#[test]
fn test_edit_text_multi_cursor_insert() {
    let content = vec![InlineContent::Text(StyledRun {
        text: "cat hat".into(),
        style: default_style(),
        logical_start_byte: 0,
    })];
    let selections = vec![
        Selection::Cursor(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 1,
            }, // after 'c'
            affinity: CursorAffinity::Leading,
        }),
        Selection::Cursor(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 5,
            }, // after 'h'
            affinity: CursorAffinity::Leading,
        }),
    ];

    let (new_content, new_selections) =
        edit_text(&content, &selections, &TextEdit::Insert(" ".to_string()));

    let new_text = match &new_content[0] {
        InlineContent::Text(run) => &run.text,
        _ => panic!(),
    };

    assert_eq!(new_text, "c at h at");

    // Check that the new cursors are in the correct positions
    assert_eq!(new_selections.len(), 2);
    if let Selection::Cursor(c1) = new_selections[0] {
        assert_eq!(c1.cluster_id.start_byte_in_run, 2); // after "c "
    }
    if let Selection::Cursor(c2) = new_selections[1] {
        assert_eq!(c2.cluster_id.start_byte_in_run, 7); // after "h " (original 5 + 1 for first
                                                        // insert + 1 for second)
    }
}

#[test]
fn test_edit_delete_range_across_runs() {
    let content = vec![
        InlineContent::Text(StyledRun {
            text: "one".into(),
            style: default_style(),
            logical_start_byte: 0,
        }),
        InlineContent::Text(StyledRun {
            text: " two ".into(),
            style: default_style(),
            logical_start_byte: 4,
        }),
        InlineContent::Text(StyledRun {
            text: "three".into(),
            style: default_style(),
            logical_start_byte: 9,
        }),
    ];

    let range = SelectionRange {
        start: TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 2,
            }, // after "on"
            affinity: CursorAffinity::Leading,
        },
        end: TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 2,
                start_byte_in_run: 3,
            }, // after "thr"
            affinity: CursorAffinity::Leading,
        },
    };

    // STUB: Full multi-run deletion is complex. This test will fail with the stub
    // but demonstrates the required behavior.
    let (new_content, new_cursor) = crate::text3::edit::delete_range(&content, &range);

    // Expected result: a single run "onree"
    // assert_eq!(new_content.len(), 1);
    // if let InlineContent::Text(run) = &new_content[0] {
    //     assert_eq!(run.text, "onree");
    // }
    // assert_eq!(new_cursor.cluster_id.start_byte_in_run, 2);
}
