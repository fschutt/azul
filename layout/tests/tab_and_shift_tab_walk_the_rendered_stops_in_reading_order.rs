//! Tab and Shift+Tab walk the stops the window renders, in reading order.
//!
//! Tab visits every keyboard stop once, in the order a reader meets them, and
//! Shift+Tab visits the same stops in reverse. A stop the window does not
//! render (a `display: none` panel, a closed section) is no stop: focus there
//! is invisible. A virtual view's controls are reached where the view sits,
//! not after everything else in the window.

use std::collections::BTreeSet;

use azul_core::{
    callbacks::{FocusTarget, VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomNodeId, IdOrClass, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItem, StyledDom},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::focus_cursor::{resolve_focus_target, FocusResolution},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { width: 400px; font-size: 14px; }
    button { height: 24px; }
    .gone { display: none; }
"#;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn button(text: &str) -> Dom {
    Dom::create_button_no_a11y(AzString::from(text))
}

fn window(mut dom: Dom) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();
    lw
}

/// The first text inside the stop `id`, to name it in an assertion. It need
/// not be a direct child: a virtual view's content wraps a button's text in
/// an anonymous paragraph.
fn text_of(lw: &LayoutWindow, id: DomNodeId) -> String {
    let Some(lr) = lw.layout_results.get(&id.dom) else {
        return String::new();
    };
    let Some(node) = id.node.into_crate_internal() else {
        return String::new();
    };
    let data = lr.styled_dom.node_data.as_ref();
    let hierarchy = lr.styled_dom.node_hierarchy.as_ref();
    // Depth first in document order: a node's first child is visited before
    // its next sibling.
    let mut stack: Vec<_> = hierarchy
        .get(node.index())
        .and_then(|h| h.first_child_id(node))
        .into_iter()
        .collect();
    while let Some(c) = stack.pop() {
        if let Some(NodeType::Text(t)) = data.get(c.index()).map(|d| &d.node_type) {
            return t.as_str().to_string();
        }
        if let Some(next) = hierarchy.get(c.index()).and_then(NodeHierarchyItem::next_sibling_id) {
            stack.push(next);
        }
        if let Some(first) = hierarchy.get(c.index()).and_then(|h| h.first_child_id(c)) {
            stack.push(first);
        }
    }
    String::new()
}

/// Every stop Tab (`forward`) or Shift+Tab visits from nothing focused, once
/// round.
fn walk(lw: &LayoutWindow, forward: bool) -> Vec<String> {
    let target = if forward {
        FocusTarget::Next
    } else {
        FocusTarget::Previous
    };
    let mut current = None;
    let mut seen: Vec<DomNodeId> = Vec::new();
    for _ in 0..64 {
        match resolve_focus_target(&target, &lw.layout_results, current, &BTreeSet::new()) {
            Ok(FocusResolution::Resolved(next)) => {
                if seen.contains(&next) {
                    break;
                }
                seen.push(next);
                current = Some(next);
            }
            _ => break,
        }
    }
    seen.into_iter().map(|id| text_of(lw, id)).collect()
}

#[test]
fn shift_tab_visits_the_tab_stops_in_reverse() {
    let lw = window(
        Dom::create_body()
            .with_child(button("One"))
            .with_child(Dom::create_div().with_child(button("Two")).with_child(button("Three")))
            .with_child(button("Four")),
    );
    let forward = walk(&lw, true);
    let mut backward = walk(&lw, false);
    backward.reverse();
    assert_eq!(forward, vec!["One", "Two", "Three", "Four"]);
    assert_eq!(backward, forward, "Shift+Tab is Tab in reverse");
}

#[test]
fn a_control_the_window_does_not_render_is_no_tab_stop() {
    let lw = window(
        Dom::create_body()
            .with_child(button("One"))
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("gone"))
                    .with_child(button("Hidden")),
            )
            .with_child(button("Two")),
    );
    assert_eq!(
        walk(&lw, true),
        vec!["One", "Two"],
        "Tab must never land on a control inside a display:none subtree"
    );
}

extern "C" fn render_inside(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let dom = Dom::create_div().with_child(button("Inside"));
    let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
    VirtualViewReturn::with_dom(dom, rect, rect)
}

#[test]
fn a_virtual_views_controls_are_reached_where_the_view_sits() {
    let lw = window(
        Dom::create_body()
            .with_child(button("Before"))
            .with_child(
                Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(render_inside))
                    .with_css("width: 200px; height: 60px;"),
            )
            .with_child(button("After")),
    );
    assert_eq!(
        walk(&lw, true),
        vec!["Before", "Inside", "After"],
        "Tab reaches the view's button between the buttons around the view"
    );
    let mut backward = walk(&lw, false);
    backward.reverse();
    assert_eq!(backward, vec!["Before", "Inside", "After"]);
}
