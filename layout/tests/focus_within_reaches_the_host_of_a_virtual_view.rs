//! `:focus-within` crosses a `VirtualView`: a view's content is part of the
//! page around it, so a focused node inside the view is a focus within the
//! view's host and every node above it (Selectors 4 s9.5: the element or one
//! of its descendants has the focus). The focus restyle worked DOM by DOM and
//! stopped at the view's own root, so a frame around a document page, a
//! terminal or a map never drew its focus ring while the focus was inside.
//!
//! `LayoutWindow::focus_change_per_dom` names, for every DOM a focus move
//! touches, the nodes `:focus` moves between and the nodes `:focus-within`
//! follows: the focused node in its own DOM, the host of the view holding it
//! in the DOM above.

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::{FocusChange, NodeHierarchyItemId, StyledDom},
};
use azul_css::props::{basic::color::ColorU, style::StyleBackgroundContent};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

struct ViewData;

/// The view's content: a field that can take the focus.
extern "C" fn render_view(_: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let dom = Dom::create_div()
        .with_class("field")
        .with_tab_index(azul_core::dom::TabIndex::Auto)
        .with_css("width: 100px; height: 20px;");
    let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
    VirtualViewReturn::with_dom(dom, rect, rect)
}

const CSS: &str = "
    .frame { background: #00ff00; padding: 4px; }
    .frame:focus-within { background: #ff0000; }
";

fn page() -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div().with_class("frame").with_child(
            Dom::create_virtual_view(RefAny::new(ViewData), VirtualViewCallback::create(render_view))
                .with_css("width: 200px; height: 40px;"),
        ),
    );
    StyledDom::create(&mut dom, azul_css::parser2::new_from_str(CSS).0)
}

fn laid_out() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        page(),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

fn with_class(styled: &StyledDom, class: &str) -> NodeId {
    (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class(class))
        .unwrap_or_else(|| panic!("no .{class}"))
}

fn background(styled: &StyledDom, node: NodeId) -> Option<ColorU> {
    let state = styled.styled_nodes.as_container()[node].styled_node_state;
    azul_layout::solver3::getters::get_background_contents(styled, node, &state)
        .into_iter()
        .find_map(|bg| match bg {
            StyleBackgroundContent::Color(c) => Some(c),
            _ => None,
        })
}

const RED: ColorU = ColorU { r: 255, g: 0, b: 0, a: 255 };
const GREEN: ColorU = ColorU { r: 0, g: 255, b: 0, a: 255 };

/// The focus moves as the shells move it: every DOM the change touches is
/// restyled with its own nodes.
fn move_focus(lw: &mut LayoutWindow, old: Option<DomNodeId>, new: Option<DomNodeId>) {
    for (dom, change) in lw.focus_change_per_dom(old, new) {
        let styled = &mut lw
            .layout_results
            .get_mut(&dom)
            .expect("a laid-out DOM")
            .styled_dom;
        let _ = styled.restyle_on_focus_change(
            FocusChange {
                lost_focus: change.lost,
                gained_focus: change.gained,
            },
            (change.lost_within, change.gained_within),
        );
    }
}

#[test]
fn a_frame_around_a_virtual_view_is_focus_within_while_the_views_field_has_the_focus() {
    let mut lw = laid_out();
    let child = *lw
        .layout_results
        .keys()
        .find(|d| **d != DomId::ROOT_ID)
        .expect("harness: the view's content is laid out");
    let field = with_class(&lw.layout_results[&child].styled_dom, "field");
    let frame = with_class(&lw.layout_results[&DomId::ROOT_ID].styled_dom, "frame");
    assert_eq!(
        background(&lw.layout_results[&DomId::ROOT_ID].styled_dom, frame),
        Some(GREEN),
        "harness: resting"
    );

    let focused = DomNodeId {
        dom: child,
        node: NodeHierarchyItemId::from_crate_internal(Some(field)),
    };
    move_focus(&mut lw, None, Some(focused));
    let root = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    assert!(
        root.styled_nodes.as_container()[frame].styled_node_state.focus_within,
        "the frame holds the view that holds the focused field"
    );
    assert_eq!(background(root, frame), Some(RED));

    move_focus(&mut lw, Some(focused), None);
    let root = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    assert!(!root.styled_nodes.as_container()[frame].styled_node_state.focus_within);
    assert_eq!(background(root, frame), Some(GREEN), "resting again");
}
