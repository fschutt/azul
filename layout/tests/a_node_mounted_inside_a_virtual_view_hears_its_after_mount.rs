//! A node mounted inside a virtual view hears its `AfterMount`.
//!
//! A view's content is a DOM of its own, rendered by its callback and
//! re-rendered as the view scrolls or its data changes. The root DOM's
//! reconciliation turns every node that enters into a `Mount` lifecycle event
//! (`AfterMount` callbacks: a field that takes the keyboard, a map that starts
//! fetching); a view's DOM was diffed against its previous render too (to carry
//! focus, scroll and selections across), but its events were dropped - and its
//! first render was not diffed at all. So a node inside a view never heard its
//! `AfterMount`: AzDrive's rename field, in the folder view's rows, never took
//! the keyboard, and Escape after Ctrl+Shift+N went to the view instead.
//!
//! Not compiled by the author (house rule).

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use azul_core::{
    callbacks::{Update, VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, EventFilter, NodeId},
    events::{ComponentEventFilter, EventType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::{Callback, CallbackInfo, ExternalSystemCallbacks},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

struct ViewData {
    /// Whether the view shows its second field (the one that mounts on a re-render).
    second: Arc<AtomicBool>,
}

extern "C" fn mounted(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

/// A field listening for its mount, with `id`.
fn field(id: &str) -> Dom {
    Dom::create_div()
        .with_id(id.into())
        .with_css("height: 20px;")
        .with_callback(
            EventFilter::Component(ComponentEventFilter::AfterMount),
            RefAny::new(()),
            Callback::from_ptr(mounted).to_core(),
        )
}

extern "C" fn render_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let second = data
        .downcast_ref::<ViewData>()
        .is_some_and(|v| v.second.load(Ordering::SeqCst));
    let mut dom = Dom::create_div().with_child(field("first"));
    if second {
        dom = dom.with_child(field("second"));
    }
    let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
    VirtualViewReturn::with_dom(dom, rect, rect)
}

fn page(second: &Arc<AtomicBool>) -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_virtual_view(
            RefAny::new(ViewData {
                second: Arc::clone(second),
            }),
            VirtualViewCallback::create(render_view),
        )
        .with_css("width: 200px; height: 100px;"),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

fn lay_out(lw: &mut LayoutWindow, styled: StyledDom) {
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

/// The view's DOM and the node with `id` in it.
fn in_view(lw: &LayoutWindow, id: &str) -> Option<(DomId, NodeId)> {
    lw.layout_results
        .iter()
        .filter(|(dom, _)| **dom != DomId::ROOT_ID)
        .find_map(|(dom, result)| {
            result
                .styled_dom
                .node_data
                .as_container()
                .internal
                .iter()
                .position(|n| n.has_id(id))
                .map(|i| (*dom, NodeId::new(i)))
        })
}

/// Whether a `Mount` event for (`dom`, `node`) waits to be dispatched.
fn mount_queued(lw: &LayoutWindow, (dom, node): (DomId, NodeId)) -> bool {
    lw.pending_lifecycle_events.iter().any(|ev| {
        ev.event_type == EventType::Mount
            && ev.target.dom == dom
            && ev.target.node.into_crate_internal() == Some(node)
    })
}

#[test]
fn a_node_mounted_inside_a_virtual_view_hears_its_after_mount() {
    let second = Arc::new(AtomicBool::new(false));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws;

    lay_out(&mut lw, page(&second));
    let first = in_view(&lw, "first").expect("the view rendered its first field");
    assert!(
        mount_queued(&lw, first),
        "the view's first render mounts its nodes: {:?}",
        lw.pending_lifecycle_events
            .iter()
            .map(|e| (e.event_type, e.target))
            .collect::<Vec<_>>()
    );

    lw.pending_lifecycle_events.clear();
    second.store(true, Ordering::SeqCst);
    lay_out(&mut lw, page(&second));
    let added = in_view(&lw, "second").expect("the view rendered its second field");
    assert!(mount_queued(&lw, added), "a node that enters a re-render mounts");
    let first = in_view(&lw, "first").expect("still there");
    assert!(
        !mount_queued(&lw, first),
        "a node the re-render kept was mounted before - it does not mount again"
    );
}
