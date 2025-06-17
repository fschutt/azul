use azul_core::{
    app_resources::{IdNamespace, RendererResources, FontInstanceKey, FontSource}, // Added FontInstanceKey, FontSource
    callbacks::{DocumentId, IFrameCallbackInfo, IFrameCallbackReturn, PipelineId, RefAny},
    dom::{Dom, NodeData, NodeType, NodeDataInlineCssProperty}, // Added NodeType
    id_tree::{Node, NodeDataContainer, NodeHierarchy, NodeId},
    styled_dom::{
        DomId, NodeHierarchyItem, NodeHierarchyItemId, ParentWithNodeDepth, StyledDom, StyledNode,
    },
    ui_solver::{WhConstraint, WidthSolvedResult, DEFAULT_FONT_SIZE_PX}, // Added DEFAULT_FONT_SIZE_PX
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{parser::CssApiWrapper, *}; // CssProperty, LayoutWidth, LayoutHeight, StyleDisplay, Display are covered by *
use azul_layout::solver2::do_the_layout_internal; // Added import for layout function

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
                last_child: Some(NodeId::new(1)),
            },
            // 1
            Node {
                parent: Some(NodeId::new(0)),
                previous_sibling: None,
                next_sibling: Some(NodeId::new(5)),
                last_child: Some(NodeId::new(2)),
            },
            // 2
            Node {
                parent: Some(NodeId::new(1)),
                previous_sibling: None,
                next_sibling: None,
                last_child: Some(NodeId::new(4)),
            },
            // 3
            Node {
                parent: Some(NodeId::new(2)),
                previous_sibling: None,
                next_sibling: Some(NodeId::new(4)),
                last_child: None,
            },
            // 4
            Node {
                parent: Some(NodeId::new(2)),
                previous_sibling: Some(NodeId::new(3)),
                next_sibling: None,
                last_child: None,
            },
            // 5
            Node {
                parent: Some(NodeId::new(1)),
                previous_sibling: Some(NodeId::new(2)),
                next_sibling: None,
                last_child: None,
            },
        ],
    }
}

#[test]
fn test_hash() {
    let a = NodeData::div();
    let b = NodeData::div();
    assert_eq!(a.calculate_node_data_hash(), b.calculate_node_data_hash())
}

struct A {}

extern "C" fn render_iframe(_: &mut RefAny, _: &mut IFrameCallbackInfo) -> IFrameCallbackReturn {
    IFrameCallbackReturn {
        dom: StyledDom::default(),
        scroll_size: LogicalSize::zero(),
        scroll_offset: LogicalPosition::zero(),
        virtual_scroll_size: LogicalSize::zero(),
        virtual_scroll_offset: LogicalPosition::zero(),
    }
}

#[test]
fn test_full_dom() {
    let mut app_resources = RendererResources::default();

    let styled_dom = StyledDom::new(&mut Dom::body(), CssApiWrapper::empty());

    let layout_result = azul_layout::solver2::do_the_layout_internal(
        DomId::ROOT_ID,
        None,
        styled_dom,
        &mut app_resources,
        &DocumentId {
            namespace_id: IdNamespace(0),
            id: 0,
        },
        LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(800.0, 600.0)),
        &mut None,
    );

    assert_eq!(
        layout_result.rects.as_ref()[NodeId::new(0)].size,
        LogicalSize::new(800.0, 600.0)
    );
}

#[test]
fn test_full_dom_2() {
    let mut app_resources = RendererResources::default();

    // tag_ids_to_node_ids gets generated?

    let styled_dom = Dom::iframe(RefAny::new(A {}), render_iframe)
        .with_inline_css_props(
            vec![
                NodeDataInlineCssProperty::Normal(CssProperty::display(LayoutDisplay::Flex)),
                NodeDataInlineCssProperty::Normal(CssProperty::flex_grow(LayoutFlexGrow {
                    inner: FloatValue::const_new(1),
                })),
                NodeDataInlineCssProperty::Normal(CssProperty::width(LayoutWidth {
                    inner: PixelValue::const_percent(100),
                })),
                NodeDataInlineCssProperty::Normal(CssProperty::height(LayoutHeight {
                    inner: PixelValue::const_percent(100),
                })),
                NodeDataInlineCssProperty::Normal(CssProperty::box_sizing(
                    LayoutBoxSizing::BorderBox,
                )),
            ]
            .into(),
        )
        .style(CssApiWrapper::empty());

    let layout_result = azul_layout::solver2::do_the_layout_internal(
        DomId::ROOT_ID,
        None,
        styled_dom,
        &mut app_resources,
        &DocumentId {
            namespace_id: IdNamespace(0),
            id: 0,
        },
        LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(800.0, 600.0)),
        &mut None,
    );

    println!("layout result: {:#?}", layout_result);
}

#[test]
fn anonymous_table_cell_layout() {
    let mut app_resources = RendererResources::default();
    // Required for text layout, even if minimal.
    // Using a simple system font source for testing.
    app_resources.add_font_source_with_id(
        FontId::new("system-font"),
        FontSource::System("sans-serif".into()),
    );
    app_resources.load_system_font(
        FontId::new("system-font"),
        &mut FontInstanceKey::new(0, 0, 0, 0),
    ).unwrap();


    let mut dom = Dom::div(); // Root node, will become the table
    dom.add_child(
        Dom::div() // This div will be wrapped by anonymous tr/td
            .with_child(Dom::text("Hello").with_id("inner-text"))
            .with_id("outer-content")
    );

    dom.set_id("table-root");
    dom.add_inline_css_property(CssProperty::display(LayoutDisplay::Table).into());
    dom.add_inline_css_property(CssProperty::width(LayoutWidth::px(200.0)).into());
    dom.add_inline_css_property(CssProperty::height(LayoutHeight::px(100.0)).into());
    // To make text size predictable, set font size on table, it will be inherited.
    dom.add_inline_css_property(CssProperty::font_size(LayoutFontSize::px(DEFAULT_FONT_SIZE_PX)).into());


    let styled_dom = dom.style(CssApiWrapper::empty());

    // Expected Node Ids after anonymous box generation:
    // 0: table-root (original Dom root)
    // 1: anonymous <tr> (child of 0)
    // 2: anonymous <td> (child of 1)
    // 3: outer-content Div (child of 2)
    // 4: inner-text Span (actually an anonymous inline block wrapping text) (child of 3)
    // 5: Text node "Hello" (child of 4)

    let node_data_container = styled_dom.node_data.as_container();

    // Verify anonymous node generation (simple check based on expected count and types)
    assert_eq!(node_data_container.len(), 6, "Expected 6 nodes after anonymous generation");

    let table_node_id = NodeId::new(0);
    let anon_tr_id = NodeId::new(1);
    let anon_td_id = NodeId::new(2);
    let outer_content_id = NodeId::new(3);
    let anon_inline_wrapper_id = NodeId::new(4); // Wrapper for "Hello"
    let text_node_id = NodeId::new(5);


    assert_eq!(node_data_container[table_node_id].get_node_type(), NodeType::Div); // Original type
    assert_eq!(node_data_container[anon_tr_id].get_node_type(), NodeType::Tr);
    assert!(node_data_container[anon_tr_id].is_anonymous());
    assert_eq!(node_data_container[anon_td_id].get_node_type(), NodeType::Td);
    assert!(node_data_container[anon_td_id].is_anonymous());
    assert_eq!(node_data_container[outer_content_id].get_node_type(), NodeType::Div);
    assert!(!node_data_container[outer_content_id].is_anonymous());
    // Node 4 is an anonymous inline block created by azul-core to wrap the text node.
    assert_eq!(node_data_container[anon_inline_wrapper_id].get_node_type(), NodeType::Div); // usually Div for anon inline
    assert!(node_data_container[anon_inline_wrapper_id].is_anonymous());
    if let NodeType::Text(text_val) = node_data_container[text_node_id].get_node_type() {
        assert_eq!(text_val.get_text(), "Hello");
    } else {
        panic!("Node 5 should be a text node");
    }


    let layout_result = do_the_layout_internal(
        DomId::ROOT_ID, // Assuming DomId::ROOT_ID maps to table_node_id for a single DOM
        None,
        styled_dom,
        &mut app_resources,
        &DocumentId { namespace_id: IdNamespace(0), id: 0, },
        LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(800.0, 600.0)), // Window size
        &mut Some(Vec::new()), // Enable debug messages
    );

    let rects = layout_result.rects.as_ref();

    // Table itself
    assert_eq!(rects[table_node_id].size.width, 200.0);
    assert_eq!(rects[table_node_id].size.height, 100.0);

    // Anonymous TD should take up the table's content box area
    // Assuming no padding/border on the table for simplicity in this test.
    let td_rect = &rects[anon_td_id];
    assert!(td_rect.size.width <= 200.0 && td_rect.size.width > 0.0);
    assert!(td_rect.size.height <= 100.0 && td_rect.size.height > 0.0);
    // TD is positioned relative to TR, TR relative to Table.
    // For a single cell table, TD's static pos relative to table content box origin should be (0,0)
    // This requires tracing parent positions if they are not 0,0.
    // For simplicity, let's check the final absolute position of content.

    // The "outer-content" Div is inside the TD
    let outer_div_rect = &rects[outer_content_id];
    assert!(outer_div_rect.size.width <= td_rect.size.width && outer_div_rect.size.width > 0.0);
    assert!(outer_div_rect.size.height <= td_rect.size.height && outer_div_rect.size.height > 0.0);

    // The anonymous inline wrapper for text "Hello"
    let text_wrapper_rect = &rects[anon_inline_wrapper_id];
    let text_rect = &rects[text_node_id]; // Text node itself usually has zero size in solver2, wrapper has size.

    // Check position of the text wrapper relative to the start of the table's content area
    // The table is at (0,0). Anon TR and TD are also at (0,0) relative to their parents.
    // The Div "outer-content" would be at (0,0) inside the TD (assuming no padding on TD).
    // The text wrapper for "Hello" would be at (0,0) inside the Div.
    let text_wrapper_abs_x = text_wrapper_rect.position.get_static_offset().x;
    let text_wrapper_abs_y = text_wrapper_rect.position.get_static_offset().y;

    // These checks are approximate due to potential default cell padding by browser CSS / table defaults
    // which are not explicitly zeroed out here. If table has default padding, these will fail.
    // For a robust test, all paddings/borders on table/td should be set to 0.
    assert!(text_wrapper_abs_x < 10.0, "Text X position {} should be close to 0", text_wrapper_abs_x); // Close to left edge
    assert!(text_wrapper_abs_y < 10.0, "Text Y position {} should be close to 0", text_wrapper_abs_y); // Close to top edge

    // Check size of the text wrapper (should be based on "Hello")
    // This depends on font metrics, which are hard to get precisely in tests without full rendering.
    // We know DEFAULT_FONT_SIZE_PX. A rough estimate for "Hello" (5 chars).
    let expected_text_width_approx = DEFAULT_FONT_SIZE_PX * 5.0 * 0.6; // 0.6 is a rough char width factor
    let expected_text_height_approx = DEFAULT_FONT_SIZE_PX * 1.2; // 1.2 for line height factor

    assert!(text_wrapper_rect.size.width >= DEFAULT_FONT_SIZE_PX * 2.0 && text_wrapper_rect.size.width < 200.0, "Text width {} seems off", text_wrapper_rect.size.width); // Greater than a few chars, less than cell
    assert!(text_wrapper_rect.size.height >= DEFAULT_FONT_SIZE_PX && text_wrapper_rect.size.height < 100.0, "Text height {} seems off", text_wrapper_rect.size.height);
}
