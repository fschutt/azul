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

// -- A3-window-09: a pass that keeps the layout tree does not copy it --------------------------

/// A view that shows a slice of a document longer than its box on both axes: a box of another
/// size is served by the slice it has, so the view is kept and its document laid out again
/// for the new box (`follow_host_in_kept_view`).
extern "C" fn sliced_page(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let _call = data
        .downcast_ref::<Watched>()
        .map(|watched| watched.0.fetch_add(1, Ordering::SeqCst))
        .expect("the view's data");
    let mut rows = Dom::create_div();
    for row in 0..40 {
        rows = rows.with_child(Dom::create_div().with_css(&format!(
            "position: absolute; left: 10px; top: {}px; width: 50%; height: 4px; background: \
             #336699;",
            10 + row * 6
        )));
    }
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(300.0, 300.0));
    let document = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(3000.0, 3000.0));
    VirtualViewReturn::with_dom(rows, window, document)
}

/// A relayout of the same DOM for a smaller window keeps the view and lays its document out
/// again for the smaller box, on the retained tree. The pass takes that tree as its own: the
/// kept result lets go of it first. It used to hold it through the pass, and the pass copied
/// every box of the document (`solver3::cache::unshare`) per frame of a window resize.
#[test]
fn a_kept_view_laid_out_for_another_box_copies_no_layout_tree() {
    use crate::solver3::cache::TREE_COPIES;

    let calls = Arc::new(AtomicUsize::new(0));
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(
            RefAny::new(Watched(Arc::clone(&calls))),
            VirtualViewCallback::create(sliced_page),
        )
        .with_css("position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px;"),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "harness: the view was rendered");

    // The same DOM, a smaller window: what a shell's resize does. Nothing but the styled DOM
    // of the root's result is kept, so the root's own tree is held by the solver alone.
    let previous = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("the root's result");
    let styled = previous.styled_dom;
    drop(previous.layout_tree);
    drop(previous.display_list);
    ws.size.dimensions = LogicalSize::new(260.0, 240.0);
    lw.current_window_state = ws.clone();
    TREE_COPIES.with(|copies| copies.set(0));
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "harness: the view is kept (its slice serves the smaller box), not rendered again"
    );
    let child = *lw
        .layout_results
        .keys()
        .find(|dom| **dom != DomId::ROOT_ID)
        .expect("the view's document");
    assert!(
        (lw.layout_results[&child].viewport.size.width - 260.0).abs() < 0.5,
        "harness: the view's document was laid out for the new box: {:?}",
        lw.layout_results[&child].viewport.size
    );
    assert_eq!(
        TREE_COPIES.with(std::cell::Cell::get),
        0,
        "no pass copied a layout tree the window still held"
    );
}

// -- A3-window-07: a hover colour is painted into the list the view has (Z3) -------------------

use azul_core::styled_dom::HoverChange;

use crate::solver3::display_list::{DisplayList, DisplayListItem};

/// The data of the view of [`label_page`]: how many labels it draws.
struct Labels(usize);

/// A map's label layer: `count` absolutely placed spans with a text, each with a hover colour.
/// Every third label has a background of its own, every third is turned (a reference frame).
/// Child DOM: div 0, then span `1 + 2 i` and its text `2 + 2 i` for label `i`.
extern "C" fn label_page(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let count = data
        .downcast_ref::<Labels>()
        .map(|labels| labels.0)
        .expect("the view's data");
    let mut layer = Dom::create_div();
    for i in 0..count {
        let own = match i % 3 {
            1 => "background: #ffee99;",
            2 => "transform-origin: 0px 9px; transform: rotate(12deg);",
            _ => "",
        };
        layer = layer.with_child(Dom::create_span_with_text(format!("Label {i}")).with_css(&format!(
            "position: absolute; left: {}px; top: {}px; width: 60px; height: 12px; font-size: \
             12px; line-height: 1; white-space: pre; cursor: pointer; {own} :hover {{ \
             background: rgba(66, 134, 244, 0.35); }}",
            10 + (i % 8) * 70,
            10 + (i / 8) * 16,
        )));
    }
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 400.0));
    VirtualViewReturn::with_dom(layer, window, window)
}

/// A window whose view holds `count` labels, laid out; the view's document.
fn label_window(count: usize) -> (LayoutWindow, DomId) {
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(RefAny::new(Labels(count)), VirtualViewCallback::create(label_page))
            .with_css("position: absolute; left: 0px; top: 0px; width: 600px; height: 400px;"),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 400.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let child = *lw
        .layout_results
        .keys()
        .find(|dom| **dom != DomId::ROOT_ID)
        .expect("the view's document");
    (lw, child)
}

/// The span of label `i` and its text, as the hover manager enters and leaves them.
fn label_nodes(i: usize) -> Vec<NodeId> {
    vec![NodeId::new(2 + 2 * i), NodeId::new(1 + 2 * i)]
}

/// A list as text, one line per item with the node and the paint call it belongs to.
fn lines(list: &DisplayList) -> Vec<String> {
    list.items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            format!(
                "{item:?} | node {:?} | call {:?}",
                list.node_mapping.get(i),
                list.layout_node_mapping.get(i)
            )
        })
        .collect()
}

/// The pointer enters (or leaves) label `i`: the restyle, shown by the decoration patch.
/// Returns the view's list as the patch left it.
fn hover(lw: &mut LayoutWindow, child: DomId, i: usize, enters: bool) -> Vec<String> {
    let (entered_nodes, left_nodes) = if enters {
        (label_nodes(i), Vec::new())
    } else {
        (Vec::new(), label_nodes(i))
    };
    let restyle = lw
        .layout_results
        .get_mut(&child)
        .expect("the view's document")
        .styled_dom
        .restyle_on_state_change(
            None,
            Some(HoverChange {
                left_nodes,
                entered_nodes,
            }),
            None,
        );
    assert!(
        restyle.changed_nodes.contains_key(&NodeId::new(1 + 2 * i)),
        "harness: the span of label {i} restyled: {:?}",
        restyle.changed_nodes.keys().collect::<Vec<_>>()
    );
    assert!(
        lw.patch_restyled_decorations(child, &restyle.changed_nodes),
        "the hover of label {i} (enters: {enters}) is shown by a patch"
    );
    lines(&lw.layout_results[&child].display_list)
}

/// What the patch leaves is what a build of the list gives: for a label without a background
/// (the hover colour is an item the list did not hold: the insert), for one with a background
/// of its own (its item takes the hover colour) and for a turned one (inside its reference
/// frame); entering and leaving.
#[test]
fn a_hover_colour_patched_into_the_list_is_the_list_a_build_gives() {
    let (mut lw, child) = label_window(12);
    for label in [0_usize, 1, 2, 9, 10, 11] {
        for enters in [true, false] {
            let patched = hover(&mut lw, child, label, enters);
            lw.regenerate_display_list_for_dom(child);
            let built = lines(&lw.layout_results[&child].display_list);
            assert_eq!(
                patched.len(),
                built.len(),
                "label {label}, enters {enters}: the item count of a build"
            );
            for (index, (p, b)) in patched.iter().zip(&built).enumerate() {
                assert_eq!(p, b, "label {label}, enters {enters}: item {index}");
            }
        }
    }
}

/// The count budget of a hover: no list is built, and the list after it is the list before
/// it with the ONE item of the label's background put in (and taken out again on the leave) -
/// every other item is the item it was, in its place. 400 labels: a build emits them all.
#[test]
fn a_hover_over_one_label_of_many_touches_that_labels_items_alone() {
    let (mut lw, child) = label_window(400);
    let before = lines(&lw.layout_results[&child].display_list);
    assert!(before.len() >= 400, "harness: the view's list holds the labels ({})", before.len());
    lw.frame_report.reset_counters();

    // Label 201 (201 % 3 == 0: no background of its own).
    let hovered = hover(&mut lw, child, 201, true);
    assert_eq!(lw.frame_report.dl_rebuilds, 0, "no list was built");
    assert_eq!(lw.frame_report.dl_node_patches, 1, "one patch");
    assert_eq!(hovered.len(), before.len() + 1, "one item more: the hover colour");
    let at = hovered
        .iter()
        .zip(&before)
        .position(|(h, b)| h != b)
        .unwrap_or(before.len());
    assert!(
        matches!(
            lw.layout_results[&child].display_list.items[at],
            DisplayListItem::Rect { .. }
        ),
        "the item put in is the label's background: {}",
        hovered[at]
    );
    assert_eq!(&hovered[..at], &before[..at], "the items before it are untouched");
    assert_eq!(&hovered[at + 1..], &before[at..], "the items behind it moved by one, unchanged");

    let left = hover(&mut lw, child, 201, false);
    assert_eq!(lw.frame_report.dl_rebuilds, 0, "no list was built for the leave either");
    assert_eq!(left, before, "the list is the one it was before the hover");
}

/// A restyle the patch cannot show is refused whole and leaves the list as it is: a property
/// that is no decoration (the caller builds the list then).
#[test]
fn a_restyle_that_is_no_decoration_is_not_patched() {
    use azul_css::props::{
        basic::PixelValue, layout::dimensions::LayoutWidth, property::CssProperty,
    };

    let (mut lw, child) = label_window(12);
    let before = lines(&lw.layout_results[&child].display_list);
    let node = NodeId::new(1);
    let restyle = lw
        .layout_results
        .get_mut(&child)
        .expect("the view's document")
        .styled_dom
        .restyle_user_properties(&[(node, CssProperty::width(LayoutWidth::Px(PixelValue::px(90.0))))]);
    assert!(!restyle.is_empty(), "harness: the width changed");
    assert!(!lw.patch_restyled_decorations(child, &restyle), "a width is no decoration");
    assert_eq!(lines(&lw.layout_results[&child].display_list), before, "the list is untouched");
}

// -- A3-window-19: the generation of a DOM, and what is kept per generation (A3-window-05) ------

/// The generation of a DOM moves when the DOM is another one - a layout pass of it, a style
/// the app wrote - and stays through everything else: a frame, a hover (shown by a patch), a
/// value the display list binds (a zoom step's transform), a pass of ANOTHER DOM, and a
/// re-render that built the DOM the view had.
#[test]
fn a_doms_generation_moves_with_the_dom_and_only_then() {
    use azul_css::props::{
        basic::{AngleValue, PixelValue},
        layout::dimensions::LayoutWidth,
        property::CssProperty,
        style::transform::StyleTransform,
    };

    use crate::overlay::{ContentChange, ContentDirtyTier};

    let (mut lw, child) = label_window(12);
    let generations = |lw: &LayoutWindow| (lw.dom_generation(DomId::ROOT_ID), lw.dom_generation(child));
    let (root, view) = generations(&lw);
    assert!(root > 0 && view > 0 && root != view, "a laid-out DOM has a generation of its own");
    assert_eq!(lw.dom_generation(DomId { inner: 999 }), 0, "a DOM without a result has none");

    // A frame.
    lw.prepare_frame_content();
    assert_eq!(generations(&lw), (root, view), "a frame changes no DOM");

    // A hover, shown by a patch of the list.
    let _ = hover(&mut lw, child, 0, true);
    assert_eq!(generations(&lw), (root, view), "an interaction state is not the DOM");

    // A value the list binds: the turned label 2 (span 5) is turned further.
    let turned = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: child,
        node_id: NodeId::new(5),
        props: vec![CssProperty::transform(
            vec![StyleTransform::Rotate(AngleValue::deg(20.0))].into(),
        )],
        override_only: false,
    });
    assert!(
        matches!(turned.tier, ContentDirtyTier::Paint),
        "harness: the transform of a turned label is a bound value"
    );
    assert_eq!(generations(&lw), (root, view), "a bound value is not the DOM");

    // A style the app writes into the view's DOM: that DOM alone is another one.
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: child,
        node_id: NodeId::new(1),
        props: vec![CssProperty::width(LayoutWidth::Px(PixelValue::px(90.0)))],
        override_only: false,
    });
    let (root_after, view_styled) = generations(&lw);
    assert_eq!(root_after, root, "the window's own DOM was not written");
    assert!(view_styled > view, "the view's DOM has another style");

    // The window's DOM laid out again (the same DOM, the same size): the view is kept and
    // nothing in it moved.
    let previous = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("the root's result");
    let styled = previous.styled_dom;
    drop(previous.layout_tree);
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
    let (root_relaid, view_kept) = generations(&lw);
    assert!(root_relaid > root, "a layout pass of the root");
    assert_eq!(view_kept, view_styled, "the kept view's DOM is the one it was");
}

/// A view rendered again: another DOM is another generation; the same DOM built again is not.
#[test]
fn a_view_rendered_again_has_another_generation_only_with_another_dom() {
    // `watched_page` draws another colour on every render.
    let mut w = WatchedWindow::new();
    let (root, view) = (w.lw.dom_generation(DomId::ROOT_ID), w.lw.dom_generation(w.child));
    w.render_again();
    assert!(w.lw.dom_generation(w.child) > view, "the view's DOM was laid out again");
    assert_eq!(w.lw.dom_generation(DomId::ROOT_ID), root, "the window's DOM was not");

    // `label_page` draws the same labels on every render: the view takes its child back.
    let (mut lw, child) = label_window(12);
    let view = lw.dom_generation(child);
    let (host, node) = *lw
        .virtual_view_manager
        .all_view_keys()
        .first()
        .expect("the page holds a view");
    let mut updates = BTreeMap::new();
    let mut views = FastBTreeSet::new();
    views.insert(node);
    updates.insert(host, views);
    lw.queue_virtual_view_updates(updates);
    let ws = lw.current_window_state.clone();
    let _ = lw.process_pending_virtual_view_updates(
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
    );
    assert_eq!(lw.dom_generation(child), view, "the same DOM built again is the DOM it was");
}

/// The count budget of A3-window-05: the search for callback images walks a DOM's layout
/// nodes once per generation of the DOM. Every frame used to walk every layout node of
/// every DOM (400 labels here, 12,470 in a map view) to find none.
#[test]
fn a_frame_of_an_unchanged_dom_walks_no_layout_node_for_callback_images() {
    use crate::window::CALLBACK_IMAGE_SCAN;

    let (mut lw, child) = label_window(400);
    CALLBACK_IMAGE_SCAN.with(|scanned| scanned.set(0));
    lw.prepare_frame_content();
    let first = CALLBACK_IMAGE_SCAN.with(std::cell::Cell::get);
    assert!(first >= 400, "harness: the first frame walks each DOM once ({first})");

    lw.prepare_frame_content();
    lw.prepare_frame_content();
    let _ = hover(&mut lw, child, 3, true);
    lw.prepare_frame_content();
    assert_eq!(
        CALLBACK_IMAGE_SCAN.with(std::cell::Cell::get),
        first,
        "frames (and a hover) over unchanged DOMs walk no layout node"
    );
}

/// How often the canvas of [`canvas_page`] was drawn.
static LATE_CANVAS_DRAWN: AtomicUsize = AtomicUsize::new(0);

extern "C" fn late_canvas(
    _data: RefAny,
    _info: crate::callbacks::RenderImageCallbackInfo,
) -> azul_core::resources::ImageRef {
    LATE_CANVAS_DRAWN.fetch_add(1, Ordering::SeqCst);
    azul_core::resources::ImageRef::null_image(
        4,
        4,
        azul_core::resources::RawImageFormat::BGRA8,
        Vec::new(),
    )
}

/// A view without a canvas on its first render and with one from the second on.
extern "C" fn canvas_page(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let call = data
        .downcast_ref::<Watched>()
        .map(|watched| watched.0.fetch_add(1, Ordering::SeqCst))
        .expect("the view's data");
    let mut dom = Dom::create_div().with_child(Dom::create_div().with_css(
        "position: absolute; left: 10px; top: 10px; width: 20px; height: 20px; background: \
         #336699;",
    ));
    if call > 0 {
        let canvas = azul_core::resources::ImageRef::callback(
            crate::callbacks::RenderImageCallback::create(late_canvas).to_core(),
            RefAny::new(()),
        );
        dom = dom.with_child(
            Dom::create_image(canvas)
                .with_css("position: absolute; left: 50px; top: 10px; width: 40px; height: 30px;"),
        );
    }
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(300.0, 300.0));
    VirtualViewReturn::with_dom(dom, window, window)
}

/// What is kept per generation is found again when the DOM is another one: a canvas that
/// comes with a view's re-render is drawn by the next frame (a search that kept "this DOM
/// has no canvas" across the re-render would never draw it).
#[test]
fn a_canvas_that_comes_with_a_re_render_is_drawn_by_the_next_frame() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(
            RefAny::new(Watched(Arc::clone(&calls))),
            VirtualViewCallback::create(canvas_page),
        )
        .with_css("position: absolute; left: 0px; top: 0px; width: 300px; height: 300px;"),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw.prepare_frame_content();
    assert_eq!(LATE_CANVAS_DRAWN.load(Ordering::SeqCst), 0, "harness: no canvas yet");

    let (host, node) = *lw
        .virtual_view_manager
        .all_view_keys()
        .first()
        .expect("the page holds a view");
    let mut updates = BTreeMap::new();
    let mut views = FastBTreeSet::new();
    views.insert(node);
    updates.insert(host, views);
    lw.queue_virtual_view_updates(updates);
    let updated = lw.process_pending_virtual_view_updates(
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
    );
    assert_eq!(updated.len(), 1, "harness: the view was rendered again");
    assert_eq!(calls.load(Ordering::SeqCst), 2, "harness: its callback ran again");

    lw.prepare_frame_content();
    assert_eq!(
        LATE_CANVAS_DRAWN.load(Ordering::SeqCst),
        1,
        "the canvas that came with the re-render is drawn"
    );
}

// -- A3-window-20: a view that states its version is kept over a rebuild of the window's DOM ---

/// The window's DOM with the view of [`watched_page`] (another colour on every render), its
/// node stating `version` or none. Every call builds new nodes with new data, as an app's
/// layout callback does.
fn versioned_page(calls: &Arc<AtomicUsize>, version: Option<u64>) -> StyledDom {
    let mut view = Dom::create_virtual_view(
        RefAny::new(Watched(Arc::clone(calls))),
        VirtualViewCallback::create(watched_page),
    )
    .with_css("position: absolute; left: 0px; top: 0px; width: 300px; height: 300px;");
    if let Some(version) = version {
        view.set_virtual_view_version(version);
    }
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(view);
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

/// A window laid out from [`versioned_page`].
fn versioned_window(calls: &Arc<AtomicUsize>, version: Option<u64>) -> (LayoutWindow, FullWindowState) {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_new_generation(
        versioned_page(calls, version),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    (lw, ws)
}

fn rebuild(lw: &mut LayoutWindow, ws: &FullWindowState, calls: &Arc<AtomicUsize>, version: Option<u64>) {
    lw.layout_new_generation(
        versioned_page(calls, version),
        ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
}

/// THE COUNT: a rebuild of the window's DOM (`RefreshDom`) whose view states the version it
/// was rendered for invokes the view's callback 0 times, lays its document out 0 times and
/// leaves its display list where it is. Another version renders it; a view without a version
/// is rendered by every rebuild, as before.
#[test]
fn a_rebuild_with_an_unchanged_view_version_invokes_no_view_callback() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (mut lw, ws) = versioned_window(&calls, Some(7));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "harness: the first layout renders the view");
    let child = *lw
        .layout_results
        .keys()
        .find(|dom| **dom != DomId::ROOT_ID)
        .expect("the view's document");
    let list = Arc::clone(&lw.layout_results[&child].display_list);
    let generation = lw.dom_generation(child);

    for _ in 0..3 {
        rebuild(&mut lw, &ws, &calls, Some(7));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the same version: the callback is not invoked");
    assert_eq!(lw.dom_generation(child), generation, "the view's document was not laid out");
    assert!(
        Arc::ptr_eq(&list, &lw.layout_results[&child].display_list),
        "the view's display list is the one it was"
    );

    rebuild(&mut lw, &ws, &calls, Some(8));
    assert_eq!(calls.load(Ordering::SeqCst), 2, "another version: the view is rendered");
    rebuild(&mut lw, &ws, &calls, Some(8));
    assert_eq!(calls.load(Ordering::SeqCst), 2, "and kept again while that version stands");

    rebuild(&mut lw, &ws, &calls, None);
    rebuild(&mut lw, &ws, &calls, None);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        4,
        "a view without a version is rendered by every rebuild"
    );
}

/// The shells' "DOM unchanged" exits of a rebuild queue every view for a re-render, all but a
/// view that states the version it was rendered for; an explicit request renders it anyway.
#[test]
fn the_unchanged_dom_exit_of_a_rebuild_keeps_a_view_that_states_its_version() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (mut lw, ws) = versioned_window(&calls, Some(7));
    let (_, view) = *lw
        .virtual_view_manager
        .all_view_keys()
        .first()
        .expect("the page holds a view");
    let mut drain = |lw: &mut LayoutWindow| {
        let _ = lw.process_pending_virtual_view_updates(
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
        );
    };

    lw.queue_virtual_view_reinvoke_after_rebuild();
    drain(&mut lw);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the same version: nothing is queued");

    // The rebuild handed the view's node another version (its payload is carried onto the
    // retained node).
    lw.layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("the root's result")
        .styled_dom
        .node_data
        .as_container_mut()[view]
        .set_virtual_view_version(8);
    lw.queue_virtual_view_reinvoke_after_rebuild();
    drain(&mut lw);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "another version: the view is rendered");
    lw.queue_virtual_view_reinvoke_after_rebuild();
    drain(&mut lw);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "and kept while that version stands");

    // What the app asks for by name is done.
    lw.queue_all_virtual_view_reinvoke();
    drain(&mut lw);
    assert_eq!(calls.load(Ordering::SeqCst), 3, "an explicit re-render ignores the version");
}
