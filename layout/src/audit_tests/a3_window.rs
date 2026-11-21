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

// -- A4-09: an image replaced in place leaves the list as the GPU renderer holds it -------------

/// A picture of `size` x `size` pixels, another one on every call.
fn picture(size: usize) -> azul_core::resources::ImageRef {
    azul_core::resources::ImageRef::null_image(
        size,
        size,
        azul_core::resources::RawImageFormat::BGRA8,
        Vec::new(),
    )
}

/// A tile that arrives for a node whose slot already shows a picture of the same size changes
/// pixels, not the list the renderer holds: the list keeps its identity as the GPU record
/// knows it (`display_list_is_held`), so it is not translated and sent again. The FIRST
/// picture of a node, a picture of another size, and a list that had changed since it was
/// sent are still owed to the renderer.
#[test]
fn an_image_replaced_in_place_leaves_the_list_the_renderer_holds() {
    use crate::overlay::ContentChange;

    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_image(picture(4)).with_css("width: 40px; height: 30px;"),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let (root, tile) = (DomId::ROOT_ID, NodeId::new(1));
    const ENV: u64 = 7;
    let sent = |lw: &mut LayoutWindow| {
        lw.gpu_state_manager
            .note_display_lists_submitted(&lw.layout_results, ENV);
    };
    let held = |lw: &LayoutWindow| {
        lw.gpu_state_manager
            .display_list_is_held(root, &lw.layout_results[&root].display_list, ENV)
    };
    let arrive = |lw: &mut LayoutWindow, image: azul_core::resources::ImageRef| {
        let _ = lw.apply_content_change(ContentChange::Image {
            dom_id: root,
            node_id: tile,
            image,
            dirty_rect: None,
        });
    };

    sent(&mut lw);
    assert!(held(&lw), "harness: the renderer holds the list it was sent");

    // The first picture of the node: its slot is new, the list names it for the first time.
    arrive(&mut lw, picture(4));
    assert!(!held(&lw), "the first picture of a node is a change of the list");

    // The renderer took it; the next tile of the same size is pixels only.
    sent(&mut lw);
    arrive(&mut lw, picture(4));
    assert!(held(&lw), "a picture of the same size in a keyed slot leaves the list held");

    // A list that changed for another reason since it was sent stays owed.
    lw.regenerate_display_list_for_dom(root);
    assert!(!held(&lw), "harness: a rebuilt list is not the one that was sent");
    arrive(&mut lw, picture(4));
    assert!(!held(&lw), "a change still owed to the renderer is not marked as sent");
}

// -- A3-window-31: the CSS diff of a view that rendered again ---------------------------------

/// The data of the view of [`moved_label_page`]: how many labels it draws and how far to the
/// right of their places.
struct MovedLabels {
    count: usize,
    shift: Arc<AtomicUsize>,
}

/// A map's label layer as the editor builds it: every label a span of one class (the sheet on
/// the layer: eight declarations) with its place and its colour as its OWN declarations. A
/// colour per label is an inherited value per label (the text resolves it): the store of
/// inherited values has as many distinct values as the view has labels.
extern "C" fn moved_label_page(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    use azul_css::{
        dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
        props::{
            basic::{color::ColorU, PixelValue},
            layout::position::{LayoutLeft, LayoutTop},
            property::CssProperty,
            style::text::StyleTextColor,
        },
    };

    let (count, shift) = data
        .downcast_ref::<MovedLabels>()
        .map(|labels| (labels.count, labels.shift.load(Ordering::SeqCst)))
        .expect("the view's data");
    let mut layer = Dom::create_div().with_css(
        ".t { position: absolute; width: 60px; height: 12px; font-size: 12px; line-height: 1; \
         white-space: pre; cursor: pointer; background: #ffee99; }",
    );
    for i in 0..count {
        let left = (10 + shift + (i % 8) * 70) as f32;
        let top = (10 + (i / 8) * 16) as f32;
        let colour = ColorU::rgb((i % 256) as u8, ((i / 256) % 256) as u8, 0);
        layer = layer.with_child(
            Dom::create_span_with_text(format!("Label {i}"))
                .with_class("t".into())
                .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                    CssPropertyWithConditions::simple(CssProperty::left(LayoutLeft {
                        inner: PixelValue::px(left),
                    })),
                    CssPropertyWithConditions::simple(CssProperty::top(LayoutTop {
                        inner: PixelValue::px(top),
                    })),
                    CssPropertyWithConditions::simple(CssProperty::text_color(StyleTextColor {
                        inner: colour,
                    })),
                ])),
        );
    }
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 400.0));
    VirtualViewReturn::with_dom(layer, window, window)
}

/// A window whose view holds `count` labels of [`moved_label_page`], laid out; the view's
/// document.
fn moved_label_window(count: usize, shift: &Arc<AtomicUsize>) -> (LayoutWindow, DomId) {
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(
            RefAny::new(MovedLabels {
                count,
                shift: shift.clone(),
            }),
            VirtualViewCallback::create(moved_label_page),
        )
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

/// The one view of the window renders again, as `update_virtual_view` has it render.
fn render_the_view_again(lw: &mut LayoutWindow, ws: &FullWindowState) {
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
    let _ = lw.process_pending_virtual_view_updates(
        ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
    );
}

/// `(steps, property types)` of the CSS diff of a view of `count` labels that renders again
/// with every label five pixels further right.
fn css_diff_of_a_move(count: usize) -> (usize, usize) {
    let shift = Arc::new(AtomicUsize::new(0));
    let (mut lw, _child) = moved_label_window(count, &shift);
    shift.store(5, Ordering::SeqCst);
    crate::window::CSS_DIFF_WORK.with(|work| work.set((0, 0)));
    let ws = lw.current_window_state.clone();
    render_the_view_again(&mut lw, &ws);
    crate::window::CSS_DIFF_WORK.with(core::cell::Cell::get)
}

/// The size sweep of A3-window-31: the diff of a view whose labels all moved takes a step per
/// label. Each label has an inherited value of its own here, so the old diff - which read
/// every distinct inherited value of the DOM for every label, four times - was labels
/// squared.
#[test]
fn the_css_diff_of_a_view_whose_labels_moved_takes_a_step_per_label() {
    let (small, _) = css_diff_of_a_move(40);
    let (large, _) = css_diff_of_a_move(160);
    assert!(small > 0, "the labels' own styles moved: the two cascades were compared");
    assert!(
        large < 6 * small,
        "4 x the labels took {large} steps after {small}: not 4 x the work"
    );
}

/// The budget of A3-window-31: a label whose place moved resolves the declarations of the
/// layer that moved - its own `left`, `top` and `color` - and not the eight of its class
/// (and every `*` rule twice) as well.
#[test]
fn a_label_that_moved_compares_its_own_declarations_only() {
    let labels = 40;
    let (_, types) = css_diff_of_a_move(labels);
    assert!(types >= labels, "every label's `left` was compared ({types} types)");
    assert!(
        types <= 4 * labels,
        "{types} property types compared for {labels} labels whose `left` moved: more than \
         their own three declarations each"
    );
}

/// Two cascades under different selector contexts are not compared type by type (204 types
/// for every node): the view lays out cold, and shows the labels where they are now.
#[test]
fn a_view_rendered_again_under_another_window_context_is_laid_out_without_a_diff() {
    let shift = Arc::new(AtomicUsize::new(0));
    let (mut lw, child) = moved_label_window(12, &shift);
    // Child DOM: the layer 0, then span `1 + 2 i` and its text for label `i`.
    let first_left = |lw: &LayoutWindow| {
        lw.get_node_bounds(child, NodeId::new(1))
            .map(|bounds| bounds.origin.x)
            .expect("the first label's box")
    };
    let before = first_left(&lw);

    // The window gets wider and the view renders again before any pass told its old
    // document about the new context.
    shift.store(5, Ordering::SeqCst);
    let mut ws = lw.current_window_state.clone();
    ws.size.dimensions = LogicalSize::new(700.0, 400.0);
    lw.current_window_state = ws.clone();
    crate::window::CSS_DIFF_WORK.with(|work| work.set((0, 0)));
    render_the_view_again(&mut lw, &ws);

    let (_, types) = crate::window::CSS_DIFF_WORK.with(core::cell::Cell::get);
    assert_eq!(types, 0, "no property was resolved on both sides");
    assert_eq!(
        first_left(&lw),
        before + 5,
        "the labels are laid out where the new render puts them"
    );
}

// -- A3-solver-41, this area's half: what the CSS diff leaves to the node's fingerprint -------

/// A page of boxes of class `t` under a body that carries `sheet`; box `i` has `own[i]` as
/// its own declarations. Nodes: the body 0, box `i` at `1 + i`.
fn boxes_page(sheet: &str, own: &[Vec<azul_css::props::property::CssProperty>]) -> StyledDom {
    use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};

    let mut dom = Dom::create_body().with_css(sheet);
    for declarations in own {
        dom = dom.with_child(
            Dom::create_div()
                .with_class("t".into())
                .with_css_props(CssPropertyWithConditionsVec::from_vec(
                    declarations
                        .iter()
                        .cloned()
                        .map(CssPropertyWithConditions::simple)
                        .collect(),
                )),
        );
    }
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

/// Every node of a page of `boxes` boxes is the node that stood at its place.
fn boxes_in_place(boxes: usize) -> Vec<azul_core::diff::NodeMove> {
    (0..=boxes)
        .map(|i| azul_core::diff::NodeMove {
            old_node_id: NodeId::new(i),
            new_node_id: NodeId::new(i),
        })
        .collect()
}

fn own_left(px: f32) -> azul_css::props::property::CssProperty {
    use azul_css::props::{basic::PixelValue, layout::position::LayoutLeft};
    azul_css::props::property::CssProperty::left(LayoutLeft {
        inner: PixelValue::px(px),
    })
}

fn own_width(px: f32) -> azul_css::props::property::CssProperty {
    use azul_css::props::{basic::PixelValue, layout::dimensions::LayoutWidth};
    azul_css::props::property::CssProperty::width(LayoutWidth::Px(PixelValue::px(px)))
}

const BOXES_SHEET: &str = ".t { position: absolute; top: 0px; width: 20px; height: 10px; }";

/// A box whose own `left` moved, and nothing else of its cascade, is not named in the CSS
/// diff: its fingerprint says `INLINE_STYLE_INSETS`, and the layout keeps the box and places
/// it. Named (scope `Full`: `left`), every label of a map that only moved was built fresh.
#[test]
fn a_box_whose_own_insets_moved_is_left_to_its_fingerprint() {
    let old = boxes_page(BOXES_SHEET, &[vec![own_left(10.0)], vec![own_left(20.0)], vec![own_left(30.0)]]);
    let new = boxes_page(BOXES_SHEET, &[vec![own_left(15.0)], vec![own_left(25.0)], vec![own_left(35.0)]]);
    let diff = crate::window::css_diff_between(&old, &new, &boxes_in_place(3))
        .expect("the two pages have the same context");
    assert!(diff.is_empty(), "no box is named: {diff:?}");
}

/// What the fingerprint calls a layout change is still named: the second box's own `width`
/// moved with its `left`.
#[test]
fn a_box_whose_own_width_moved_with_its_insets_is_named_in_the_css_diff() {
    let old = boxes_page(
        BOXES_SHEET,
        &[vec![own_left(10.0)], vec![own_left(20.0), own_width(20.0)], vec![own_left(30.0)]],
    );
    let new = boxes_page(
        BOXES_SHEET,
        &[vec![own_left(15.0)], vec![own_left(25.0), own_width(30.0)], vec![own_left(35.0)]],
    );
    let diff = crate::window::css_diff_between(&old, &new, &boxes_in_place(3))
        .expect("the two pages have the same context");
    let named: Vec<NodeId> = diff.iter().map(|(node, _)| *node).collect();
    assert_eq!(named, vec![NodeId::new(2)], "the box whose width moved, and no other");
    assert!(
        diff[0].1 > azul_css::props::property::RelayoutScope::None,
        "a width is laid out again"
    );
}

/// An inset a STYLESHEET moved shows in no node's own data: the fingerprints are equal, so
/// the diff has to name the boxes or the layout serves their old places.
#[test]
fn an_inset_a_stylesheet_moved_is_named_in_the_css_diff() {
    let own = [vec![], vec![], vec![]];
    let old = boxes_page(".t { position: absolute; left: 5px; width: 20px; height: 10px; }", &own);
    let new = boxes_page(".t { position: absolute; left: 9px; width: 20px; height: 10px; }", &own);
    let diff = crate::window::css_diff_between(&old, &new, &boxes_in_place(3))
        .expect("the two pages have the same context");
    let named: Vec<NodeId> = diff.iter().map(|(node, _)| *node).collect();
    assert_eq!(
        named,
        vec![NodeId::new(1), NodeId::new(2), NodeId::new(3)],
        "every box the rule places"
    );
}

// -- A3-window-41: the fade flag of a window that holds a view ---------------------------------

/// A scrollbar of the window's own DOM in mid-fade asks for frames, also when the window
/// holds a view: the view's DOM has no bar, its pass came last and its "nothing fades here"
/// used to be the window's answer.
#[test]
fn a_fading_bar_of_the_window_asks_for_frames_with_a_view_in_the_window() {
    use azul_core::task::{Duration, Instant};

    let mut dom = Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(
            Dom::create_div()
                .with_class("box".into())
                .with_css("width: 300px; height: 100px; overflow-y: scroll;")
                .with_child(Dom::create_div().with_css("height: 400px; width: 280px;")),
        )
        .with_child(
            Dom::create_virtual_view(RefAny::new(Labels(4)), VirtualViewCallback::create(label_page))
                .with_css("width: 300px; height: 100px;"),
        );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    assert!(lw.layout_results.len() > 1, "the window holds a view's document");
    let scroller = {
        let nodes = lw.layout_results[&DomId::ROOT_ID].styled_dom.node_data.as_container();
        (0..nodes.len())
            .map(NodeId::new)
            .find(|n| nodes[*n].has_class("box"))
            .expect("the box")
    };

    // A hold of 10 ms and a fade of a minute: 40 ms after the scroll the bar is in mid-fade
    // (opacity 0.9995) and stays there for the length of the test.
    lw.gpu_state_manager.fade_delay = Duration::from_millis(10);
    lw.gpu_state_manager.fade_duration = Duration::from_millis(60_000);
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        scroller,
        LogicalPosition::new(0.0, 60.0),
        Instant::now(),
    );
    std::thread::sleep(std::time::Duration::from_millis(40));
    let _ = lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
    assert!(
        lw.gpu_state_manager.scrollbar_fade_active,
        "the window's bar is fading: the platform loop has to keep drawing frames"
    );
}

// -- A3-window-28: the per-frame scrollbar fade pass --------------------------------------------

/// The budget of A3-window-28: a frame of a window without a scrollbar computes a fade for no
/// box - not for each of its view's labels (every laid-out box carries scrollbar info, with
/// no bar in it).
#[test]
fn a_frame_computes_a_scrollbar_fade_for_no_box_of_a_page_without_bars() {
    for labels in [40, 160] {
        let (mut lw, _child) = label_window(labels);
        crate::window::SCROLLBAR_FADE_WORK.with(|work| work.set(0));
        let _ = lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
        let _ = lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
        assert_eq!(
            crate::window::SCROLLBAR_FADE_WORK.with(core::cell::Cell::get),
            0,
            "{labels} labels, no bar: no box's fade is computed"
        );
        assert!(
            lw.gpu_state_manager
                .caches
                .values()
                .all(|cache| cache.scrollbar_v_opacity_keys.is_empty()
                    && cache.scrollbar_h_opacity_keys.is_empty()),
            "and no fade key is made"
        );
    }
}

// -- A3-window-22 / -23 / -24: what the font chains of a DOM were resolved for -----------------

fn font_resolutions() -> usize {
    crate::window::FONT_RESOLUTIONS.with(core::cell::Cell::get)
}

/// A3-window-22: a view that renders again with the font stacks and the characters it had
/// resolves no font. Its pass started from an empty slot of the font manager and resolved
/// cold every time: every node's stack and every character of the view.
#[test]
fn a_view_rendered_again_with_the_stacks_and_characters_it_had_resolves_no_font() {
    let shift = Arc::new(AtomicUsize::new(0));
    let (mut lw, _child) = moved_label_window(12, &shift);
    let ws = lw.current_window_state.clone();
    // The same labels, five pixels further right: another DOM, the same text.
    shift.store(5, Ordering::SeqCst);
    crate::window::FONT_RESOLUTIONS.with(|ran| ran.set(0));
    render_the_view_again(&mut lw, &ws);
    assert_eq!(font_resolutions(), 0, "the chains of the view's last pass answer for it");
}

/// A page of one line of text, laid out in `lw` as a rebuild of the window's DOM.
fn lay_out_text(lw: &mut LayoutWindow, ws: &FullWindowState, text: &str) {
    let mut dom = Dom::create_body().with_child(Dom::create_p_with_text(text));
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    lw.layout_and_generate_display_list(
        styled,
        ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

/// A3-window-24: the same font stacks over other CHARACTERS are another requirement (a
/// chain holds the faces its characters need). A rebuilt page whose text has a character it
/// did not have resolves again; one whose characters were all covered before does not, also
/// when one of them had left the text in between.
#[test]
fn a_rebuilt_page_resolves_its_fonts_again_for_a_character_it_did_not_have() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lay_out_text(&mut lw, &ws, "abc");

    crate::window::FONT_RESOLUTIONS.with(|ran| ran.set(0));
    lay_out_text(&mut lw, &ws, "cab cab");
    assert_eq!(font_resolutions(), 0, "the same characters in another order");
    lay_out_text(&mut lw, &ws, "abd");
    assert_eq!(font_resolutions(), 1, "`d` is new: the chains were resolved for a, b, c");
    lay_out_text(&mut lw, &ws, "abc");
    assert_eq!(font_resolutions(), 1, "`c` was covered before and still is");
}

/// A3-window-23: the font collector may run on the keep-set of the window's own DOM only
/// while no child DOM is alive - and a view's document the running pass set aside is alive.
#[test]
fn the_font_collector_does_not_run_while_a_views_document_is_set_aside() {
    let (mut lw, child) = label_window(4);
    assert!(
        !lw.font_keep_set_is_the_windows(DomId::ROOT_ID),
        "a view's document is laid out"
    );
    // As the funnel of a new generation leaves the window for the pass over its own DOM.
    let root = lw.layout_results.remove(&DomId::ROOT_ID);
    let view = lw.layout_results.remove(&child).expect("the view's document");
    lw.retired_child_results.insert(child, view);
    assert!(lw.layout_results.is_empty() && root.is_some());
    assert!(
        !lw.font_keep_set_is_the_windows(DomId::ROOT_ID),
        "the view's document is set aside, to be taken back or rendered again"
    );
    assert!(
        !lw.font_keep_set_is_the_windows(child),
        "and never on a child's own keep-set"
    );
    lw.retired_child_results.clear();
    assert!(lw.font_keep_set_is_the_windows(DomId::ROOT_ID), "a window without a view");
}
