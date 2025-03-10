use azul_css::RectLayout;
use azul_core::{
    id_tree::{Node, NodeId, NodeHierarchy, NodeDataContainer},
    styled_dom::{NodeHierarchyItem, NodeHierarchyItemId, StyledNode, ParentWithNodeDepth},
    ui_solver::{WhConstraint, WidthSolvedResult},
};
use crate::layout_solver::{
    determine_preferred_width,
    width_calculated_rect_arena_from_rect_layout_arena,
    width_calculated_rect_arena_sum_children_flex_basis,
    bubble_preferred_widths_to_parents,
    width_calculated_rect_arena_apply_flex_grow,
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
fn get_display_rectangle_arena(constraints: &[(usize, RectLayout)]) -> (NodeDataContainer<NodeHierarchyItem>, Vec<ParentWithNodeDepth>, NodeDataContainer<StyledNode>) {
    let node_hierarchy = get_testing_hierarchy();
    let node_depths = node_hierarchy.as_ref().get_parents_sorted_by_depth();
    let mut node_data = vec![StyledNode::default(); node_hierarchy.as_ref().len()];
    for (id, rect) in constraints {
        node_data[*id].layout = rect.clone();
    }

    (
     NodeDataContainer { internal: node_hierarchy.internal.into_iter().map(|i| NodeHierarchyItem::from(i)).collect() },
     node_depths.into_iter().map(|(depth, node_id)| ParentWithNodeDepth { depth, node_id: NodeHierarchyItemId::from_crate_internal(Some(node_id)) }).collect(),
     NodeDataContainer { internal: node_data }
    )
}


#[cfg(test)]
mod dom_tests {

    use azul_core::{
        dom::Dom,
        callbacks::{RefAny, PipelineId, IFrameCallbackInfo, IFrameCallbackReturn},
        app_resources::AppResources,
        styled_dom::{DomId, StyledDom},
        id_tree::NodeId,
        window::{LogicalRect, LogicalPosition, LogicalSize},
    };
    use azul_css::*;

    struct A { }

    extern "C" fn render_iframe(_: &RefAny, _: IFrameCallbackInfo) -> IFrameCallbackReturn {
        IFrameCallbackReturn {
            styled_dom: StyledDom::default(),
            size: LayoutRect::zero(),
            virtual_size: None.into(),
        }
    }

    #[test]
    fn test_full_dom() {

        let mut app_resources = AppResources::default();

        let styled_dom = StyledDom::new(Dom::body(), Css::empty());

        let layout_result = crate::layout_solver::do_the_layout_internal(
            DomId::ROOT_ID,
            None,
            styled_dom,
            &mut app_resources,
            PipelineId::DUMMY,
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(800.0, 600.0))
        );

        assert_eq!(layout_result.rects.as_ref()[NodeId::new(0)].size, LogicalSize::new(800.0, 600.0));
    }

    #[test]
    fn test_full_dom_2() {

        let mut app_resources = AppResources::default();

        // tag_ids_to_node_ids gets generated?

        let styled_dom = Dom::iframe(RefAny::new(A { }), render_iframe)
            .with_inline_css(CssProperty::display(LayoutDisplay::Flex))
            .with_inline_css(CssProperty::flex_grow(LayoutFlexGrow { inner: FloatValue::const_new(1) }))
            .with_inline_css(CssProperty::width(LayoutWidth { inner: PixelValue::const_percent(100) }))
            .with_inline_css(CssProperty::height(LayoutHeight { inner: PixelValue::const_percent(100) }))
            .with_inline_css(CssProperty::box_sizing(LayoutBoxSizing::BorderBox))
            .style(Css::empty());

        let layout_result = crate::layout_solver::do_the_layout_internal(
            DomId::ROOT_ID,
            None,
            styled_dom,
            &mut app_resources,
            PipelineId::DUMMY,
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(800.0, 600.0))
        );

        println!("layout result: {:#?}", layout_result);
    }
}

#[test]
fn test_determine_preferred_width() {
    use azul_css::{LayoutMinWidth, LayoutMaxWidth, PixelValue, LayoutWidth};
    use azul_core::styled_dom::StyledNode;

    let layout = StyledNode {
        layout: RectLayout {
            width: None.into(),
            min_width: None.into(),
            max_width: None.into(),
            .. Default::default()
        },
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::Unconstrained);

    let layout = StyledNode {
        layout: RectLayout {
            width: Some(LayoutWidth { inner: PixelValue::px(500.0) }.into()).into(),
            min_width: None.into(),
            max_width: None.into(),
            .. Default::default()
        },
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(500.0));

    let layout = StyledNode {
        layout: RectLayout {
            width: Some(LayoutWidth { inner: PixelValue::px(500.0) }.into()).into(),
            min_width: Some(LayoutMinWidth { inner: PixelValue::px(600.0) }.into()).into(),
            max_width: None.into(),
            .. Default::default()
        },
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(600.0));

    let layout = StyledNode {
        layout: RectLayout {
            width: Some(LayoutWidth { inner: PixelValue::px(10000.0) }.into()).into(),
            min_width: Some(LayoutMinWidth { inner: PixelValue::px(600.0) }.into()).into(),
            max_width: Some(LayoutMaxWidth { inner: PixelValue::px(800.0) }.into()).into(),
            .. Default::default()
        },
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(800.0));

    let layout = StyledNode {
        layout: RectLayout {
            width: None.into(),
            min_width: Some(LayoutMinWidth { inner: PixelValue::px(600.0) }.into()).into(),
            max_width: Some(LayoutMaxWidth { inner: PixelValue::px(800.0) }.into()).into(),
            .. Default::default()
        },
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::Between(600.0, 800.0));

    let layout = StyledNode {
        layout: RectLayout {
            width: None.into(),
            min_width: None.into(),
            max_width: Some(LayoutMaxWidth { inner: PixelValue::px(800.0) }.into()).into(),
            .. Default::default()
        },
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::Between(0.0, 800.0));

    let layout = StyledNode {
        layout: RectLayout {
            width: Some(LayoutWidth { inner: PixelValue::px(1000.0) }.into()).into(),
            min_width: None.into(),
            max_width: Some(LayoutMaxWidth { inner: PixelValue::px(800.0) }.into()).into(),
            .. Default::default()
        },
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(800.0));

    let layout = StyledNode {
        layout: RectLayout {
            width: Some(LayoutWidth { inner: PixelValue::px(1200.0) }.into()).into(),
            min_width: Some(LayoutMinWidth { inner: PixelValue::px(1000.0) }.into()).into(),
            max_width: Some(LayoutMaxWidth { inner: PixelValue::px(800.0) }.into()).into(),
            .. Default::default()
        },
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(800.0));

    let layout = StyledNode {
        layout: RectLayout {
            width: Some(LayoutWidth { inner: PixelValue::px(1200.0)}.into()).into(),
            min_width: Some(LayoutMinWidth { inner: PixelValue::px(1000.0)}.into()).into(),
            max_width: Some(LayoutMaxWidth { inner: PixelValue::px(400.0)}.into()).into(),
            .. Default::default()
        },
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout, None, 800.0), WhConstraint::EqualTo(400.0));
}

/// Tests that the nodes get filled correctly
#[test]
fn test_fill_out_preferred_width() {

    use azul_css::*;
    use std::collections::BTreeSet;
    use crate::layout_solver::{
        get_layout_positions,
        get_layout_flex_grows,
        get_layout_flex_directions,
    };

    let window_width = 754.0; // pixel

    let (node_hierarchy, node_depths, node_data) = get_display_rectangle_arena(&[
        (0, RectLayout {
            direction: Some(LayoutFlexDirection::Row.into()).into(),
            .. Default::default()
        }),
        (1, RectLayout {
            max_width: Some(LayoutMaxWidth { inner: PixelValue::px(200.0) }.into()).into(),
            padding_left: Some(LayoutPaddingLeft { inner: PixelValue::px(20.0) }.into()).into(),
            padding_right: Some(LayoutPaddingRight { inner: PixelValue::px(20.0) }.into()).into(),
            direction: Some(LayoutFlexDirection::Row.into()).into(),
            .. Default::default()
        }),
        (2, RectLayout {
            direction: Some(LayoutFlexDirection::Row.into()).into(),
            .. Default::default()
        })
    ]);

    let preferred_widths = node_data.as_ref().transform(|_, _| None);
    let mut width_filled_out_data = width_calculated_rect_arena_from_rect_layout_arena(
        &node_data.as_ref(),
        &preferred_widths.as_ref(),
        &node_hierarchy.as_ref(),
        &node_depths,
        window_width,
    );

    // Test some basic stuff - test that `get_flex_basis` works

    // Nodes 0, 2, 3, 4 and 5 have no basis
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(0)].get_flex_basis_horizontal(800.0), 0.0);

    // Node 1 has a padding on left and right of 20, so a flex-basis of 40.0
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(1)].get_flex_basis_horizontal(800.0), 40.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(1)].get_horizontal_padding(800.0), 40.0);

    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(2)].get_flex_basis_horizontal(800.0), 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(3)].get_flex_basis_horizontal(800.0), 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(4)].get_flex_basis_horizontal(800.0), 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(5)].get_flex_basis_horizontal(800.0), 0.0);

    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(0)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(1)].preferred_width, WhConstraint::Between(0.0, 200.0));
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(2)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(3)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(4)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(5)].preferred_width, WhConstraint::Unconstrained);

    let layout_positions = get_layout_positions(&node_data.as_ref());
    let layout_flex_grows = get_layout_flex_grows(&node_data.as_ref());
    let layout_directions = get_layout_flex_directions(&node_data.as_ref());

    // Test the flex-basis sum
    assert_eq!(width_calculated_rect_arena_sum_children_flex_basis(&mut width_filled_out_data.as_ref_mut(), NodeId::new(2), &node_hierarchy.as_ref(), &layout_positions.as_ref()), 0.0);
    assert_eq!(width_calculated_rect_arena_sum_children_flex_basis(&mut width_filled_out_data.as_ref_mut(), NodeId::new(1), &node_hierarchy.as_ref(), &layout_positions.as_ref()), 0.0);
    assert_eq!(width_calculated_rect_arena_sum_children_flex_basis(&mut width_filled_out_data.as_ref_mut(), NodeId::new(0), &node_hierarchy.as_ref(), &layout_positions.as_ref()), 40.0);

    // -- Section 2: Test that size-bubbling works:
    //
    // Size-bubbling should take the 40px padding and "bubble" it towards the

    // ID 5 has no child, so it's not returned, same as 3 and 4
    assert_eq!(node_depths, vec![
        ParentWithNodeDepth { depth: 0, node_id: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))) },
        ParentWithNodeDepth { depth: 1, node_id: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))) },
        ParentWithNodeDepth { depth: 2, node_id: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))) },
    ]);

    bubble_preferred_widths_to_parents(&mut width_filled_out_data.as_ref_mut(), &node_hierarchy.as_ref(), &layout_positions.as_ref(), &node_depths);

    // This step shouldn't have touched the flex_grow_px
    for node in &width_filled_out_data.internal {
        assert_eq!(node.flex_grow_px, 0.0);
    }

    // This step should not modify the `preferred_width`
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(0)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(1)].preferred_width, WhConstraint::Between(0.0, 200.0));
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(2)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(3)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(4)].preferred_width, WhConstraint::Unconstrained);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(5)].preferred_width, WhConstraint::Unconstrained);

    // The padding of the Node 1 should have bubbled up to be the minimum width of Node 0
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(0)].min_inner_size_px, 40.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(1)].get_flex_basis_horizontal(800.0), 40.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(1)].min_inner_size_px, 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(2)].get_flex_basis_horizontal(800.0), 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(2)].min_inner_size_px, 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(3)].get_flex_basis_horizontal(800.0), 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(3)].min_inner_size_px, 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(4)].get_flex_basis_horizontal(800.0), 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(4)].min_inner_size_px, 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(5)].get_flex_basis_horizontal(800.0), 0.0);
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(5)].min_inner_size_px, 0.0);

    // -- Section 3: Test if growing the sizes works

    // - window_width: 754px
    // 0                -- [] - expecting width to stretch to 754 px
    // '- 1             -- [max-width: 200px; padding: 20px] - expecting width to stretch to 200 px
    //    '-- 2         -- [] - expecting width to stretch to 160px
    //    '   '-- 3     -- [] - expecting width to stretch to 80px (half of 160)
    //    '   '-- 4     -- [] - expecting width to stretch to 80px (half of 160)
    //    '-- 5         -- [] - expecting width to stretch to 554px (754 - 200px max-width of earlier sibling)

    let parents_to_recalc = node_depths.iter().filter_map(|n| n.node_id.into_crate_internal()).collect::<BTreeSet<_>>();
    width_calculated_rect_arena_apply_flex_grow(
        &mut width_filled_out_data.as_ref_mut(),
        &node_hierarchy.as_ref(),
        &layout_flex_grows.as_ref(),
        &layout_positions.as_ref(),
        &layout_directions.as_ref(),
        &node_depths,
        window_width,
        &parents_to_recalc
    );

    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(0)].solved_result(), WidthSolvedResult {
        min_width: 40.0,
        space_added: window_width - 40.0,
    });
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(1)].solved_result(), WidthSolvedResult {
        min_width: 0.0,
        space_added: 200.0,
    });
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(2)].solved_result(), WidthSolvedResult {
        min_width: 0.0,
        space_added: 160.0,
    });
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(3)].solved_result(), WidthSolvedResult {
        min_width: 0.0,
        space_added: 80.0,
    });
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(4)].solved_result(), WidthSolvedResult {
        min_width: 0.0,
        space_added: 80.0,
    });
    assert_eq!(width_filled_out_data.as_ref()[NodeId::new(5)].solved_result(), WidthSolvedResult {
        min_width: 0.0,
        space_added: window_width - 200.0,
    });
}
