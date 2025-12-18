use azul_core::{
    app_resources::{IdNamespace, RendererResources},
    callbacks::{DocumentId, IFrameCallbackInfo, IFrameCallbackReturn, PipelineId, RefAny},
    dom::{Dom, NodeData, NodeDataInlineCssProperty},
    id_tree::{Node, NodeDataContainer, NodeHierarchy, NodeId},
    styled_dom::{
        DomId, NodeHierarchyItem, NodeHierarchyItemId, ParentWithNodeDepth, StyledDom, StyledNode,
    },
    ui_solver::{WhConstraint, WidthSolvedResult},
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{parser::CssApiWrapper, *};

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
    let a = NodeData::new_div();
    let b = NodeData::new_div();
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

    let styled_dom = StyledDom::new_node(&mut Dom::new_body(), CssApiWrapper::empty());

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
