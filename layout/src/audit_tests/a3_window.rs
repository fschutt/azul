//! Area A3-window: `window.rs` (layout entry, restyle, reconciliation, virtual views, the
//! per-frame scans) and the display-list patching of a restyle.
//!
//! Findings: `audit/A3-window.md`.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul_core::{
    callbacks::{CoreCallback, VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId},
    events::{ComponentEventFilter, EventFilter, EventType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::{OptionRefAny, RefAny},
    resources::RendererResources,
    styled_dom::StyledDom,
    FastBTreeSet,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};

// -- A3-window-04: a view rendered again is reconciled against the arena where it lies --------

/// The data of the view of [`page`]: how often its callback ran.
struct Watched(Arc<AtomicUsize>);

/// A view of three nodes: its root, a box that is red on every even render and green on every
/// odd one (so no render builds the DOM of the one before), and a node that hears its mount.
extern "C" fn watched_page(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let call = data
        .downcast_ref::<Watched>()
        .map(|watched| watched.0.fetch_add(1, Ordering::SeqCst))
        .expect("the view's data");
    let colour = if call % 2 == 0 { "#ff0000" } else { "#00ff00" };
    let mut watched = Dom::create_div()
        .with_css("position: absolute; left: 10px; top: 80px; width: 20px; height: 20px;");
    watched.add_callback(
        EventFilter::Component(ComponentEventFilter::AfterMount),
        RefAny::new(0u32),
        CoreCallback {
            cb: 0,
            ctx: OptionRefAny::None,
        },
    );
    let dom = Dom::create_div()
        .with_child(Dom::create_div().with_css(&format!(
            "position: absolute; left: 10px; top: 20px; width: 100px; height: 40px; background: \
             {colour};"
        )))
        .with_child(watched);
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(300.0, 300.0));
    VirtualViewReturn::with_dom(dom, window, window)
}

/// The window's DOM: a body with the view of [`watched_page`], shown or not.
fn page(calls: &Arc<AtomicUsize>, shown: bool) -> StyledDom {
    let display = if shown { "block" } else { "none" };
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(
            RefAny::new(Watched(Arc::clone(calls))),
            VirtualViewCallback::create(watched_page),
        )
        .with_css(&format!(
            "display: {display}; position: absolute; left: 0px; top: 0px; width: 300px; height: \
             300px;"
        )),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

struct WatchedWindow {
    lw: LayoutWindow,
    ws: FullWindowState,
    calls: Arc<AtomicUsize>,
    host: DomId,
    view: NodeId,
    child: DomId,
}

impl WatchedWindow {
    fn new() -> Self {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(300.0, 300.0);
        lw.current_window_state = ws.clone();
        lw.layout_new_generation(
            page(&calls, true),
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");
        let (host, view) = *lw
            .virtual_view_manager
            .all_view_keys()
            .first()
            .expect("the page holds a view");
        let child = lw
            .virtual_view_manager
            .get_nested_dom_id(host, view)
            .expect("the view's child DOM");
        Self {
            lw,
            ws,
            calls,
            host,
            view,
            child,
        }
    }

    /// The app built its DOM again (a new generation): every view renders again.
    fn rebuild(&mut self, shown: bool) {
        self.lw
            .layout_new_generation(
                page(&self.calls, shown),
                &self.ws,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut None,
            )
            .expect("the page lays out again");
    }

    /// The frame's drain of a queued re-render of the view.
    fn render_again(&mut self) {
        let mut updates = BTreeMap::new();
        let mut views = FastBTreeSet::new();
        views.insert(self.view);
        updates.insert(self.host, views);
        self.lw.queue_virtual_view_updates(updates);
        let updated = self.lw.process_pending_virtual_view_updates(
            &self.ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
        );
        assert_eq!(updated.len(), 1, "the view was rendered again");
    }

    /// The mounts queued for nodes of a view's document, and forget them.
    fn take_view_mounts(&mut self) -> usize {
        let mounts = self
            .lw
            .pending_lifecycle_events
            .iter()
            .filter(|e| e.event_type == EventType::Mount && e.target.dom != DomId::ROOT_ID)
            .count();
        self.lw.pending_lifecycle_events.clear();
        mounts
    }
}

/// A queued re-render reconciles the view's new DOM against the arena of its live result, read
/// in place: the node that stayed is the node it was (no second mount).
#[test]
fn a_view_rendered_again_by_a_queued_update_is_matched_against_its_live_arena() {
    let mut w = WatchedWindow::new();
    assert_eq!(w.take_view_mounts(), 1, "harness: the first render mounts the watched node");
    w.render_again();
    assert_eq!(w.calls.load(Ordering::SeqCst), 2, "harness: the callback ran again");
    assert_eq!(w.take_view_mounts(), 0, "the watched node is the node it was");
}

/// A new generation of the window's DOM retires the view's result for the pass and renders the
/// view again: its new DOM is reconciled against the retired result's arena, read in place (it
/// used to be deep-copied into `previous_child_arenas` before every such pass, and the new
/// arena with it). The node that stayed is the node it was, and the pass leaves no copy behind.
#[test]
fn a_view_rendered_again_by_a_new_generation_is_matched_against_its_retired_arena() {
    let mut w = WatchedWindow::new();
    assert_eq!(w.take_view_mounts(), 1, "harness: the first render mounts the watched node");
    w.rebuild(true);
    assert_eq!(w.calls.load(Ordering::SeqCst), 2, "harness: the callback ran again");
    assert_eq!(w.take_view_mounts(), 0, "the watched node is the node it was");
    assert!(w.lw.retired_child_results.is_empty(), "the retired result went with the pass");
    assert!(
        w.lw.previous_child_arenas.is_empty(),
        "a view that was rendered again leaves no arena stashed"
    );
}

/// A pass that renders no view for a child (its host has no box) leaves the child's arena for
/// the render that comes later: the one case an arena is still copied.
#[test]
fn a_view_no_pass_rendered_again_leaves_its_arena_for_the_next_render() {
    let mut w = WatchedWindow::new();
    assert_eq!(w.take_view_mounts(), 1, "harness: the first render mounts the watched node");
    w.rebuild(false);
    assert_eq!(w.calls.load(Ordering::SeqCst), 1, "harness: a hidden view is not rendered");
    let stashed = w
        .lw
        .previous_child_arenas
        .get(&w.child)
        .map(|(nodes, hierarchy)| (nodes.len(), hierarchy.len()));
    assert!(
        stashed.is_some_and(|(nodes, hierarchy)| nodes >= 3 && nodes == hierarchy),
        "the arena of the child that was not rendered again is kept: {stashed:?}"
    );
    assert!(w.lw.retired_child_results.is_empty(), "the retired result went with the pass");
}
