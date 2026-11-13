//! A screen reader reaches the controls of an open popover.
//!
//! An open `<transient-window>` is a window of its own: the parent gives its
//! subtree `display: none`, and the popup's window lays the same subtree out
//! and renders it. Its controls are still nodes of the parent's DOM, with the
//! parent's callbacks, so a press addressed to the parent (the debug server's
//! `accessibility_action`, AzCalendar's Save) must reach them while the
//! popover is open, the way a browser's popover content stays pressable.
//! Once it closes, or inside an element the popover hides, they are gone.
#![cfg(feature = "a11y")]

use azul_core::{
    dom::{AccessibilityAction, Dom, DomId, DomNodeId, IdOrClass, NodeData, NodeId, NodeType},
    events::{EventFilter, HoverEventFilter},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
    transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, transient::collect_open_transient_windows,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { width: 600px; height: 400px; font-size: 14px; }
    .draft-save { width: 120px; height: 40px; }
    .hidden-option { display: none; }
"#;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn now() -> Instant {
    Instant::from(std::time::Instant::now())
}

/// `body > p + div > transient-window > (div.draft-save, div.hidden-option >
/// div.hidden-inner)`: a week line and the draft's popover, open or closed.
fn week_with_draft(open: bool) -> Dom {
    let config = if open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    }
    .with_anchor(TransientAnchor::Viewport)
    .with_dismiss(TransientDismiss::None);
    let popover = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(config)))
        .with_child(Dom::create_div().with_ids_and_classes(class("draft-save")))
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("hidden-option"))
                .with_child(Dom::create_div().with_ids_and_classes(class("hidden-inner"))),
        );
    Dom::create_body()
        .with_child(Dom::create_p_with_text("AzMeet link waits for the server"))
        .with_child(Dom::create_div().with_child(popover))
}

/// Lay `dom` out in `lw` and reconcile its popovers, the way a shell's pass
/// does (`reconcile_transient_windows`).
fn show(lw: &mut LayoutWindow, mut dom: Dom) {
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let ws = lw.current_window_state.clone();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();
    let wanted = collect_open_transient_windows(
        &lw.layout_results[&DomId::ROOT_ID].styled_dom,
        &[],
        &[],
        |_| {
            Some(LogicalRect::new(
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(600.0, 400.0),
            ))
        },
    );
    let _ = lw
        .transient_windows
        .reconcile(&wanted, |_, _| Some(LogicalSize::new(120.0, 40.0)));
}

fn window_showing(dom: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 400.0);
    lw.current_window_state = ws;
    show(&mut lw, dom);
    lw
}

fn node_with_class(lw: &LayoutWindow, class: &str) -> NodeId {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .find(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == class))
        })
        .unwrap_or_else(|| panic!("no node with class {class}"))
}

/// The events a screen reader's press on `node` of the root DOM dispatches.
fn press(lw: &mut LayoutWindow, node: NodeId) -> Vec<EventFilter> {
    let target = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    };
    lw.process_accessibility_action(DomId::ROOT_ID, node, AccessibilityAction::Default, now())
        .remove(&target)
        .map(|(events, _)| events)
        .unwrap_or_default()
}

#[test]
fn a_press_on_a_control_of_an_open_popover_clicks_it() {
    let mut lw = window_showing(week_with_draft(true));
    assert_eq!(
        lw.transient_windows.open_windows().len(),
        1,
        "harness: the draft's popover is open"
    );
    let save = node_with_class(&lw, "draft-save");
    assert_eq!(
        press(&mut lw, save),
        vec![EventFilter::Hover(HoverEventFilter::Click)],
        "Save is rendered by the popover's window, so a press addressed to the parent must \
         click it; it was answered as gone because the parent gives it no box"
    );
}

#[test]
fn a_press_inside_an_element_the_open_popover_hides_is_answered_as_gone() {
    let mut lw = window_showing(week_with_draft(true));
    let hidden = node_with_class(&lw, "hidden-inner");
    assert_eq!(
        press(&mut lw, hidden),
        Vec::new(),
        "a node under `display: none` in the popover is rendered nowhere"
    );
}

#[test]
fn a_press_on_a_control_of_a_closed_popover_is_answered_as_gone() {
    let mut lw = window_showing(week_with_draft(true));
    show(&mut lw, week_with_draft(false));
    assert!(
        lw.transient_windows.open_windows().is_empty(),
        "harness: the draft's popover closed"
    );
    let save = node_with_class(&lw, "draft-save");
    assert_eq!(
        press(&mut lw, save),
        Vec::new(),
        "a closed popover's controls are rendered nowhere"
    );
}
