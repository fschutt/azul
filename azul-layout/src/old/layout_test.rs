use azul_css::RectLayout;
use azul_core::id_tree::{Node, NodeId, NodeHierarchy, NodeDataContainer};
use crate::layout_solver::{
    WhConstraint, determine_preferred_width,
    WidthCalculatedRect, WidthSolvedResult,
    width_calculated_rect_arena_from_rect_layout_arena
};

/// Returns a DOM for testing so we don't have to construct it every time.
/// The DOM structure looks like this:
///
/// ```no_run
/// 0
/// '- 1
///    '-- 2
///    '   '-- 3
///    '   '--- 4
///    '-- 5
/// ```
fn get_testing_hierarchy() -> NodeHierarchy {
    NodeHierarchy {
        internal: vec![
            // 0
            Node {
                parent: None,
                previous_sibling: None,
                next_sibling: None,
                first_child: Some(NodeId::new(1)),
                last_child: Some(NodeId::new(1)),
            },
            // 1
            Node {
                parent: Some(NodeId::new(0)),
                previous_sibling: None,
                next_sibling: Some(NodeId::new(5)),
                first_child: Some(NodeId::new(2)),
                last_child: Some(NodeId::new(2)),
            },
            // 2
            Node {
                parent: Some(NodeId::new(1)),
                previous_sibling: None,
                next_sibling: None,
                first_child: Some(NodeId::new(3)),
                last_child: Some(NodeId::new(4)),
            },
            // 3
            Node {
                parent: Some(NodeId::new(2)),
                previous_sibling: None,
                next_sibling: Some(NodeId::new(4)),
                first_child: None,
                last_child: None,
            },
            // 4
            Node {
                parent: Some(NodeId::new(2)),
                previous_sibling: Some(NodeId::new(3)),
                next_sibling: None,
                first_child: None,
                last_child: None,
            },
            // 5
            Node {
                parent: Some(NodeId::new(1)),
                previous_sibling: Some(NodeId::new(2)),
                next_sibling: None,
                first_child: None,
                last_child: None,
            },
        ]
    }
}

/// Returns the same arena, but pre-fills nodes at [(NodeId, RectLayout)]
/// with the layout rect
fn get_display_rectangle_arena(constraints: &[(usize, RectLayout)]) -> (NodeHierarchy, NodeDataContainer<RectLayout>) {
    let arena = get_testing_hierarchy();
    let mut arena_data = vec![RectLayout::default(); arena.len()];
    for (id, rect) in constraints {
        arena_data[*id] = *rect;
    }
    (arena, NodeDataContainer { internal: arena_data })
}

#[test]
fn test_determine_preferred_width() {
    use azul_css::{LayoutMinWidth, LayoutMaxWidth, PixelValue, LayoutWidth};

    let layout = RectLayout {
        width: None,
        min_width: None,
        max_width: None,
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::Unconstrained);

    let layout = RectLayout {
        width: Some(LayoutWidth { inner: PixelValue::px(500.0) }.into()),
        min_width: None,
        max_width: None,
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(500.0));

    let layout = RectLayout {
        width: Some(LayoutWidth { inner: PixelValue::px(500.0) }.into()),
        min_width: Some(LayoutMinWidth { inner: PixelValue::px(600.0) }.into()),
        max_width: None,
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(600.0));

    let layout = RectLayout {
        width: Some(LayoutWidth { inner: PixelValue::px(10000.0) }.into()),
        min_width: Some(LayoutMinWidth { inner: PixelValue::px(600.0) }.into()),
        max_width: Some(LayoutMaxWidth { inner: PixelValue::px(800.0) }.into()),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(800.0));

    let layout = RectLayout {
        width: None,
        min_width: Some(LayoutMinWidth { inner: PixelValue::px(600.0) }.into()),
        max_width: Some(LayoutMaxWidth { inner: PixelValue::px(800.0) }.into()),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::Between(600.0, 800.0));

    let layout = RectLayout {
        width: None,
        min_width: None,
        max_width: Some(LayoutMaxWidth { inner: PixelValue::px(800.0) }.into()),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::Between(0.0, 800.0));

    let layout = RectLayout {
        width: Some(LayoutWidth { inner: PixelValue::px(1000.0) }.into()),
        min_width: None,
        max_width: Some(LayoutMaxWidth { inner: PixelValue::px(800.0) }.into()),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(800.0));

    let layout = RectLayout {
        width: Some(LayoutWidth { inner: PixelValue::px(1200.0) }.into()),
        min_width: Some(LayoutMinWidth { inner: PixelValue::px(1000.0) }.into()),
        max_width: Some(LayoutMaxWidth { inner: PixelValue::px(800.0) }.into()),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(800.0));

    let layout = RectLayout {
        width: Some(LayoutWidth { inner: PixelValue::px(1200.0)}.into()),
        min_width: Some(LayoutMinWidth { inner: PixelValue::px(1000.0)}.into()),
        max_width: Some(LayoutMaxWidth { inner: PixelValue::px(400.0)}.into()),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(400.0));
}

/// Tests that the nodes get filled correctly
#[test]
fn test_fill_out_preferred_width() {

    use azul_css::*;

    let (node_hierarchy, node_data) = get_display_rectangle_arena(&[
        (0, RectLayout {
            direction: Some(LayoutDirection::Row.into()),
            .. Default::default()
        }),
        (1, RectLayout {
            max_width: Some(LayoutMaxWidth { inner: PixelValue::px(200.0) }.into()),
            padding_left: Some(LayoutPaddingLeft { inner: PixelValue::px(20.0) }.into()),
            padding_right: Some(LayoutPaddingRight { inner: PixelValue::px(20.0) }.into()),
            direction: Some(LayoutDirection::Row.into()),
            .. Default::default()
        }),
        (2, RectLayout {
            direction: Some(LayoutDirection::Row.into()),
            .. Default::default()
        })
    ]);

    let preferred_widths = node_data.transform(|_, _| None);
    let mut width_filled_out_data = width_calculated_rect_arena_from_rect_layout_arena(&node_data, &preferred_widths, &node_hierarchy);

    // Test some basic stuff - test that `get_flex_basis` works

    // Nodes 0, 2, 3, 4 and 5 have no basis
    assert_eq!(width_filled_out_data[NodeId::new(0)].get_flex_basis_horizontal(), 0.0);

    // Node 1 has a padding on left and right of 20, so a flex-basis of 40.0
    assert_eq!(width_filled_out_data[NodeId::new(1)].get_flex_basis_horizontal(), 40.0);
    assert_eq!(width_filled_out_data[NodeId::new(1)].get_horizontal_padding(), 40.0);

    assert_eq!(width_filled_out_data[NodeId::new(2)].get_flex_basis_horizontal(), 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(3)].get_flex_basis_horizontal(), 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(4)].get_flex_basis_horizontal(), 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(5)].get_flex_basis_horizontal(), 0.0);

    assert_eq!(width_filled_out_data[NodeId::new(0)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data[NodeId::new(1)].preferred_width, WhConstraint::Between(0.0, 200.0));
    assert_eq!(width_filled_out_data[NodeId::new(2)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data[NodeId::new(3)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data[NodeId::new(4)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data[NodeId::new(5)].preferred_width, WhConstraint::Unconstrained);

    // Test the flex-basis sum
    assert_eq!(width_filled_out_data.sum_children_flex_basis(NodeId::new(2), &node_hierarchy, &node_data), 0.0);
    assert_eq!(width_filled_out_data.sum_children_flex_basis(NodeId::new(1), &node_hierarchy, &node_data), 0.0);
    assert_eq!(width_filled_out_data.sum_children_flex_basis(NodeId::new(0), &node_hierarchy, &node_data), 40.0);

    // -- Section 2: Test that size-bubbling works:
    //
    // Size-bubbling should take the 40px padding and "bubble" it towards the
    let non_leaf_nodes_sorted_by_depth = node_hierarchy.get_parents_sorted_by_depth();

    // ID 5 has no child, so it's not returned, same as 3 and 4
    assert_eq!(non_leaf_nodes_sorted_by_depth, vec![
        (0, NodeId::new(0)),
        (1, NodeId::new(1)),
        (2, NodeId::new(2)),
    ]);

    width_filled_out_data.bubble_preferred_widths_to_parents(&node_hierarchy, &node_data, &non_leaf_nodes_sorted_by_depth);

    // This step shouldn't have touched the flex_grow_px
    for node in &width_filled_out_data.internal {
        assert_eq!(node.flex_grow_px, 0.0);
    }

    // This step should not modify the `preferred_width`
    assert_eq!(width_filled_out_data[NodeId::new(0)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data[NodeId::new(1)].preferred_width, WhConstraint::Between(0.0, 200.0));
    assert_eq!(width_filled_out_data[NodeId::new(2)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data[NodeId::new(3)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data[NodeId::new(4)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data[NodeId::new(5)].preferred_width, WhConstraint::Unconstrained);

    // The padding of the Node 1 should have bubbled up to be the minimum width of Node 0
    assert_eq!(width_filled_out_data[NodeId::new(0)].min_inner_size_px, 40.0);
    assert_eq!(width_filled_out_data[NodeId::new(1)].get_flex_basis_horizontal(), 40.0);
    assert_eq!(width_filled_out_data[NodeId::new(1)].min_inner_size_px, 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(2)].get_flex_basis_horizontal(), 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(2)].min_inner_size_px, 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(3)].get_flex_basis_horizontal(), 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(3)].min_inner_size_px, 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(4)].get_flex_basis_horizontal(), 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(4)].min_inner_size_px, 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(5)].get_flex_basis_horizontal(), 0.0);
    assert_eq!(width_filled_out_data[NodeId::new(5)].min_inner_size_px, 0.0);

    // -- Section 3: Test if growing the sizes works

    let window_width = 754.0; // pixel

    // - window_width: 754px
    // 0                -- [] - expecting width to stretch to 754 px
    // '- 1             -- [max-width: 200px; padding: 20px] - expecting width to stretch to 200 px
    //    '-- 2         -- [] - expecting width to stretch to 160px
    //    '   '-- 3     -- [] - expecting width to stretch to 80px (half of 160)
    //    '   '-- 4     -- [] - expecting width to stretch to 80px (half of 160)
    //    '-- 5         -- [] - expecting width to stretch to 554px (754 - 200px max-width of earlier sibling)

    width_filled_out_data.apply_flex_grow(&node_hierarchy, &node_data, &non_leaf_nodes_sorted_by_depth, window_width);

    assert_eq!(width_filled_out_data[NodeId::new(0)].solved_result(), WidthSolvedResult {
        min_width: 40.0,
        space_added: window_width - 40.0,
    });
    assert_eq!(width_filled_out_data[NodeId::new(1)].solved_result(), WidthSolvedResult {
        min_width: 0.0,
        space_added: 200.0,
    });
    assert_eq!(width_filled_out_data[NodeId::new(2)].solved_result(), WidthSolvedResult {
        min_width: 0.0,
        space_added: 160.0,
    });
    assert_eq!(width_filled_out_data[NodeId::new(3)].solved_result(), WidthSolvedResult {
        min_width: 0.0,
        space_added: 80.0,
    });
    assert_eq!(width_filled_out_data[NodeId::new(4)].solved_result(), WidthSolvedResult {
        min_width: 0.0,
        space_added: 80.0,
    });
    assert_eq!(width_filled_out_data[NodeId::new(5)].solved_result(), WidthSolvedResult {
        min_width: 0.0,
        space_added: window_width - 200.0,
    });
}
