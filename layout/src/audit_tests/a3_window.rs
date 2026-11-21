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
        // A size of its own: a view is a replaced box, and an absolutely positioned
        // replaced box with `width: auto` is as wide as its intrinsic size (300 x 150), not
        // as its insets leave room for (CSS 2 10.3.8). The four insets alone, as this test
        // had them, left the view 300 x 150 in both windows.
        .with_css("position: absolute; left: 0px; top: 0px; width: 100%; height: 100%;"),
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

/// While this lives, the windows this thread builds bind a state colour when a state first
/// PAINTS it (the binding of A3-window-50), not from the build of the list (A3-window-55):
/// for the tests of the decoration patch, which is what still shows every restyle that is
/// not the value of a bound colour (a border, a shadow, a gradient, a node without a key).
struct BoundWhenPainted;

impl BoundWhenPainted {
    fn on_this_thread() -> Self {
        crate::window::set_state_colors_from_the_build_on_this_thread(Some(false));
        Self
    }
}

impl Drop for BoundWhenPainted {
    fn drop(&mut self) {
        crate::window::set_state_colors_from_the_build_on_this_thread(None);
    }
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

/// The pointer enters (or leaves) label `i`: the restyle, shown by
/// `patch_restyled_decorations` (the value of a colour key, or a run painted into the list).
/// Returns the view's list as that left it.
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
    let _binding = BoundWhenPainted::on_this_thread();
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
    let _binding = BoundWhenPainted::on_this_thread();
    let (mut lw, child) = label_window(400);
    let before = lines(&lw.layout_results[&child].display_list);
    assert!(before.len() >= 400, "harness: the view's list holds the labels ({})", before.len());
    lw.frame_report.reset_counters();

    // Label 201 (201 % 3 == 0: no background of its own).
    let list = Arc::as_ptr(&lw.layout_results[&child].display_list);
    let hovered = hover(&mut lw, child, 201, true);
    assert_eq!(lw.frame_report.dl_rebuilds, 0, "no list was built");
    assert_eq!(lw.frame_report.dl_node_patches, 1, "one patch");
    // A3-window-38: patched where it lies. The view's own solver cache holds the list too,
    // and lets go of it for the patch; it was copied whole the first time after every pass.
    assert!(
        core::ptr::eq(list, Arc::as_ptr(&lw.layout_results[&child].display_list)),
        "the view's list is the allocation it was"
    );
    assert!(
        lw.child_layout_caches[&child]
            .cached_display_list
            .as_ref()
            .is_none_or(|cached| Arc::ptr_eq(&cached.5, &lw.layout_results[&child].display_list)),
        "and the view's solver cache holds the patched list, not the one before the patch"
    );
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

    // The leave (A4-23): the item put in was bound to a colour key of the label, and the
    // colour going is that key's value. No list is built or patched for it; the list keeps
    // the rect, which paints nothing now, and is painted as it was before the hover.
    let left = hover(&mut lw, child, 201, false);
    assert_eq!(lw.frame_report.dl_rebuilds, 0, "no list was built for the leave either");
    assert_eq!(lw.frame_report.dl_node_patches, 1, "nor patched: the leave is a value");
    assert_eq!(left, hovered, "the list holds the items it held");
    assert_eq!(painted_lines(&lw, child), before, "and is painted as before the hover");
}

/// `lines` with the id of every `TransformKey { id: .. }` blanked: for comparing the lists of
/// two different windows, whose key ids come from one process-wide counter.
fn without_transform_key_ids(lines: &[String]) -> Vec<String> {
    const OPEN: &str = "TransformKey { id: ";
    lines
        .iter()
        .map(|line| {
            let mut out = String::with_capacity(line.len());
            let mut rest = line.as_str();
            while let Some(at) = rest.find(OPEN) {
                let after = &rest[at + OPEN.len()..];
                let digits = after.bytes().take_while(u8::is_ascii_digit).count();
                out.push_str(&rest[..at + OPEN.len()]);
                out.push('_');
                rest = &after[digits..];
            }
            out.push_str(rest);
            out
        })
        .collect()
}

/// The list of `dom` as the renderers paint it: a rect bound to a colour key in the key's
/// value (`GpuValueCache::for_each_bound_color`), left out where that value is see-through
/// (both renderers paint nothing for it); every other item as it is.
fn painted_lines(lw: &LayoutWindow, dom: DomId) -> Vec<String> {
    let list = &lw.layout_results[&dom].display_list;
    let mut values: BTreeMap<usize, azul_css::props::basic::color::ColorU> = BTreeMap::new();
    if let Some(cache) = lw.gpu_state_manager.caches.get(&dom) {
        cache.for_each_bound_color(|key, color| {
            values.insert(key, color);
        });
    }
    list.items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let value = list.color_binding(i).and_then(|key| values.get(&key.id).copied());
            let item = match (item, value) {
                (
                    DisplayListItem::Rect {
                        bounds,
                        border_radius,
                        ..
                    },
                    Some(color),
                ) => {
                    if color.a == 0 {
                        return None;
                    }
                    DisplayListItem::Rect {
                        bounds: *bounds,
                        color,
                        border_radius: *border_radius,
                    }
                }
                _ => item.clone(),
            };
            Some(format!(
                "{item:?} | node {:?} | call {:?}",
                list.node_mapping.get(i),
                list.layout_node_mapping.get(i)
            ))
        })
        .collect()
}

/// Audit A4-23. A hover that comes AGAIN over a label is a value: the first enter paints the
/// label's background into the list and binds it to a colour key; the leave, the next enter
/// and the next leave write that key's colour and touch no list (to the GPU renderer a
/// patched list is a whole list: translated and scene-built again for every enter and
/// leave). What is painted is at every step what a list built from the styles paints.
/// Mutations: the bind of step 4 removed, or `by_value` always empty (the leave patches:
/// `dl_node_patches` 1); `set_color_value` not called (the painted list keeps the hover
/// colour after the leave).
#[test]
fn a_hover_that_comes_again_is_a_value_and_no_list() {
    let _binding = BoundWhenPainted::on_this_thread();
    let (mut lw, child) = label_window(40);
    let at_rest = lines(&lw.layout_results[&child].display_list);
    // Label 9 (9 % 3 == 0: no background of its own).
    let hovered = hover(&mut lw, child, 9, true);
    assert_eq!(painted_lines(&lw, child), hovered, "the bound colour is the hover colour");
    let list = Arc::as_ptr(&lw.layout_results[&child].display_list);
    lw.frame_report.reset_counters();

    for (enters, painted) in [(false, &at_rest), (true, &hovered), (false, &at_rest)] {
        drop(hover(&mut lw, child, 9, enters));
        assert_eq!(lw.frame_report.dl_rebuilds, 0, "enters {enters}: no list is built");
        assert_eq!(lw.frame_report.dl_node_patches, 0, "enters {enters}: no list is patched");
        assert!(
            core::ptr::eq(list, Arc::as_ptr(&lw.layout_results[&child].display_list)),
            "enters {enters}: the list is the allocation it was"
        );
        assert_eq!(&painted_lines(&lw, child), painted, "enters {enters}: what is painted");
    }

    // A build of the list lets the key go: the list holds what the styles say and nothing
    // else, and the next enter is a patch again.
    lw.regenerate_display_list_for_dom(child);
    assert_eq!(lines(&lw.layout_results[&child].display_list), at_rest, "a built list at rest");
    lw.frame_report.reset_counters();
    drop(hover(&mut lw, child, 9, true));
    assert_eq!(lw.frame_report.dl_node_patches, 1, "after a build the hover is painted in");
    assert_eq!(painted_lines(&lw, child), hovered);
}

/// A node whose state colour is NOT one solid background takes the patch, bound or not: label
/// 10 has a background of its own, so its hover run replaces a rect (it is bound then, and
/// its leave is a value back to its own colour); a label left through a build of the list
/// has no key and its leave is a patch.
#[test]
fn a_state_colour_without_a_key_still_takes_the_patch() {
    let _binding = BoundWhenPainted::on_this_thread();
    let (mut lw, child) = label_window(40);
    let at_rest = lines(&lw.layout_results[&child].display_list);
    lw.frame_report.reset_counters();
    drop(hover(&mut lw, child, 10, true));
    assert_eq!(lw.frame_report.dl_node_patches, 1, "the enter is painted into the list");
    drop(hover(&mut lw, child, 10, false));
    assert_eq!(lw.frame_report.dl_node_patches, 1, "the leave is the key's value");
    assert_eq!(painted_lines(&lw, child), at_rest, "its own background is painted again");

    drop(hover(&mut lw, child, 9, true));
    lw.regenerate_display_list_for_dom(child);
    lw.frame_report.reset_counters();
    drop(hover(&mut lw, child, 9, false));
    assert_eq!(
        (lw.frame_report.dl_rebuilds, lw.frame_report.dl_node_patches),
        (0, 1),
        "no key after a build: the leave takes the run out of the list"
    );
    assert_eq!(lines(&lw.layout_results[&child].display_list), at_rest);
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

// -- A3-window-55 (A4-23, the eager binding): a state colour is a value from the first enter ---

/// How many rects of `dom`'s list the renderers paint in the labels' hover colour
/// (`rgba(66, 134, 244, 0.35)`): a rect bound to a colour key counts in the key's value.
fn hover_painted(lw: &LayoutWindow, dom: DomId) -> usize {
    let hover = azul_css::props::basic::color::ColorU {
        r: 66,
        g: 134,
        b: 244,
        a: 89,
    };
    let list = &lw.layout_results[&dom].display_list;
    let mut values: BTreeMap<usize, azul_css::props::basic::color::ColorU> = BTreeMap::new();
    if let Some(cache) = lw.gpu_state_manager.caches.get(&dom) {
        cache.for_each_bound_color(|key, color| {
            values.insert(key, color);
        });
    }
    list.items
        .iter()
        .enumerate()
        .filter(|(index, item)| {
            let DisplayListItem::Rect { color, .. } = item else {
                return false;
            };
            list.color_binding(*index)
                .and_then(|key| values.get(&key.id).copied())
                .unwrap_or(*color)
                == hover
        })
        .count()
}

/// How many colour keys the document `dom` holds.
fn color_keys(lw: &LayoutWindow, dom: DomId) -> usize {
    lw.gpu_state_manager
        .caches
        .get(&dom)
        .map_or(0, |cache| cache.color_keys.len())
}

/// What a build binds: the background rect of every node that has a `:hover` background, a
/// see-through rect where the node has no background of its own, and nothing for any other
/// node. 12 labels, 4 of them with a background of their own: 12 bound rects, 12 keys, 8
/// items more than the same page built without the binding; a page of labels without a
/// state rule gets no key and no rect.
/// Mutations: the see-through layer not pushed in `paint_node_background_and_border_inner`
/// (4 bound rects, 0 items more); `state_colored_nodes` answering every tagged node (keys on
/// the page without a state rule).
#[test]
fn a_build_binds_one_rect_per_node_with_a_state_colour_and_none_else() {
    let bound = |lw: &LayoutWindow, dom: DomId| {
        lw.layout_results[&dom]
            .display_list
            .color_bindings
            .iter()
            .flatten()
            .count()
    };
    let (lw, child) = label_window(12);
    assert_eq!(bound(&lw, child), 12, "one bound rect per label with a hover colour");
    assert_eq!(color_keys(&lw, child), 12, "and one key each");
    assert_eq!(color_keys(&lw, DomId::ROOT_ID), 0, "the window's own nodes have no state rule");
    assert_eq!(hover_painted(&lw, child), 0, "nothing is hovered");
    let items = lw.layout_results[&child].display_list.items.len();

    let (plain_items, plain_bound, plain_keys) = {
        let _binding = BoundWhenPainted::on_this_thread();
        let (plain, plain_child) = label_window(12);
        (
            plain.layout_results[&plain_child].display_list.items.len(),
            bound(&plain, plain_child),
            color_keys(&plain, plain_child),
        )
    };
    assert_eq!((plain_bound, plain_keys), (0, 0), "harness: the switch binds nothing at a build");
    assert_eq!(
        items,
        plain_items + 8,
        "one see-through rect for each of the 8 labels without a background of their own"
    );

    // Labels without a state rule (every one of them takes the pointer: `cursor`).
    let (lw, child) = moved_label_window(12, &Arc::new(AtomicUsize::new(0)));
    assert_eq!((bound(&lw, child), color_keys(&lw, child)), (0, 0), "no state rule: no key");
}

/// The budget of the eager binding: a pointer swept across labels it never touched changes
/// the view's list 0 times - no build, no patch, the list the allocation it was - and writes
/// one value per enter and per leave; and what is painted is at every step what a list BUILT
/// under the same states paints. In a view of 40 labels as in one of 160.
/// Mutations: `bind_state_colors` given no nodes (the first enter of each label is a patch:
/// `dl_node_patches` 12); `set_color_value` not called in the value path (0 writes, and the
/// first step paints no hover); the value path taken without asking the list for the key
/// (not caught here: see `a_node_whose_rect_the_list_does_not_bind_takes_the_patch`).
#[test]
fn a_pointer_swept_over_labels_it_never_touched_writes_values_and_changes_no_list() {
    use crate::window::STATE_COLOR_WRITES;

    const SWEPT: usize = 12;
    for labels in [40_usize, 160] {
        let (mut lw, child) = label_window(labels);
        let list = Arc::as_ptr(&lw.layout_results[&child].display_list);
        let at_rest = painted_lines(&lw, child);
        lw.frame_report.reset_counters();
        STATE_COLOR_WRITES.with(|written| written.set(0));
        let mut steps: Vec<Vec<String>> = Vec::new();
        for label in 0..SWEPT {
            for enters in [true, false] {
                drop(hover(&mut lw, child, label, enters));
                steps.push(painted_lines(&lw, child));
            }
        }
        assert_eq!(
            (lw.frame_report.dl_rebuilds, lw.frame_report.dl_node_patches),
            (0, 0),
            "{labels} labels: no list was built or patched"
        );
        assert!(
            core::ptr::eq(list, Arc::as_ptr(&lw.layout_results[&child].display_list)),
            "{labels} labels: the view's list is the allocation it was"
        );
        assert_eq!(
            STATE_COLOR_WRITES.with(core::cell::Cell::get),
            2 * SWEPT,
            "{labels} labels: a value per enter and per leave"
        );
        assert_eq!(painted_lines(&lw, child), at_rest, "{labels} labels: at rest again");

        // The same sweep with the list BUILT after every step.
        let (mut built, built_child) = label_window(labels);
        let mut step = 0;
        for label in 0..SWEPT {
            for enters in [true, false] {
                drop(hover(&mut built, built_child, label, enters));
                built.regenerate_display_list_for_dom(built_child);
                // The verifier's correction of the premise: `lw` and `built` are two windows, and
                // `TransformKey::unique()` is a process-wide counter (core/src/gpu.rs), so the
                // reference frames of the turned labels never share key ids across windows (seen:
                // 13 `PushReferenceFrame` lines, ids 4702.. against 5008..). With the id blanked 0
                // of the 188 lines differ. Everything else of a line is compared as it was.
                assert_eq!(
                    without_transform_key_ids(&steps[step]),
                    without_transform_key_ids(&painted_lines(&built, built_child)),
                    "{labels} labels, label {label}, enters {enters}: what a built list paints"
                );
                step += 1;
            }
        }
    }
}

/// A node with a key whose rect the list does not hold is NOT served by a value (nobody
/// would read it): it takes the patch, which paints the run. Here the key is there and the
/// bound rect is taken out of the list behind the window's back.
/// Mutation: the value path taken for any node with a key, the list not asked
/// (`binds_a_color_key`): the hover writes a value no rect shows, `hover_painted` stays 0.
#[test]
fn a_node_whose_rect_the_list_does_not_bind_takes_the_patch() {
    let (mut lw, child) = label_window(12);
    // Label 3 (no background of its own): its see-through rect loses its binding.
    let span = NodeId::new(7);
    {
        let result = lw.layout_results.get_mut(&child).expect("the view's document");
        let list = Arc::make_mut(&mut result.display_list);
        let mut unbound = 0;
        for index in 0..list.items.len() {
            if list.node_mapping.get(index).copied().flatten() == Some(span)
                && list.color_bindings.get(index).copied().flatten().is_some()
            {
                list.color_bindings[index] = None;
                unbound += 1;
            }
        }
        list.frame_index.clear();
        assert_eq!(unbound, 1, "harness: the label had one bound rect");
    }
    if let Some(cache) = lw.child_layout_caches.get_mut(&child) {
        cache.cached_display_list = None;
    }
    lw.frame_report.reset_counters();
    drop(hover(&mut lw, child, 3, true));
    assert_eq!(hover_painted(&lw, child), 1, "the hover is painted");
    assert_eq!(
        (lw.frame_report.dl_rebuilds, lw.frame_report.dl_node_patches),
        (0, 1),
        "by a run painted into the list"
    );
}

/// A state colour that goes by value stays right through everything that can happen between
/// an enter and a leave: a BUILD of the list (it paints the hover it is built under, and the
/// leave after it is a value still), a layout PASS of the document (the same), and a build or
/// a pass AFTER the leave (the colour does not come back).
/// Mutations: `bind_state_colors` writing no value for a kept key (after a build the leave
/// is right but a hover that began before the build shows the colour of the rest); the keys
/// let go at every build as before A3-window-55 (`dl_node_patches` is not 0).
#[test]
fn a_state_colour_by_value_is_right_through_a_build_and_a_pass() {
    use azul_css::props::{
        basic::PixelValue, layout::dimensions::LayoutWidth, property::CssProperty,
    };

    use crate::overlay::{ContentChange, ContentDirtyTier};

    let (mut lw, child) = label_window(40);
    let at_rest = painted_lines(&lw, child);
    lw.frame_report.reset_counters();

    // Enter, a build in between, leave.
    drop(hover(&mut lw, child, 9, true));
    assert_eq!(hover_painted(&lw, child), 1, "the hover is painted");
    lw.regenerate_display_list_for_dom(child);
    assert_eq!(hover_painted(&lw, child), 1, "a built list paints the hover it is built under");
    drop(hover(&mut lw, child, 9, false));
    assert_eq!(painted_lines(&lw, child), at_rest, "the leave after a build");

    // Enter and leave, then a build: the colour does not come back.
    drop(hover(&mut lw, child, 10, true));
    drop(hover(&mut lw, child, 10, false));
    lw.regenerate_display_list_for_dom(child);
    assert_eq!(painted_lines(&lw, child), at_rest, "a build after a leave");

    // Enter, a layout pass of the document in between (label 1 gets another width), leave.
    let pass = |lw: &mut LayoutWindow, width: f32| {
        let tier = lw
            .apply_content_change(ContentChange::NodeCss {
                dom_id: child,
                node_id: NodeId::new(3),
                props: vec![CssProperty::width(LayoutWidth::Px(PixelValue::px(width)))],
                override_only: false,
            })
            .tier;
        assert_eq!(tier, ContentDirtyTier::Relayout, "premise: a width is a pass");
    };
    drop(hover(&mut lw, child, 9, true));
    pass(&mut lw, 64.0);
    assert_eq!(hover_painted(&lw, child), 1, "after a pass the hovered label is painted hovered");
    drop(hover(&mut lw, child, 9, false));
    assert_eq!(hover_painted(&lw, child), 0, "and the leave after a pass takes the colour");

    // Enter and leave, then a pass: the colour does not come back with it.
    drop(hover(&mut lw, child, 11, true));
    drop(hover(&mut lw, child, 11, false));
    pass(&mut lw, 66.0);
    assert_eq!(hover_painted(&lw, child), 0, "a hover that went does not come back with a pass");

    assert_eq!(
        lw.frame_report.dl_node_patches, 0,
        "every enter and leave above was a value: no run was painted into a list"
    );
    assert_eq!(color_keys(&lw, child), 40, "the keys are those of the labels, no more");
}

/// The data of the view of [`counted_label_page`]: how many labels it draws now.
struct CountedLabels(Arc<AtomicUsize>);

/// [`label_page`] with a number of labels that can change between two renders; every label
/// has a hover colour and no background of its own.
extern "C" fn counted_label_page(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let count = data
        .downcast_ref::<CountedLabels>()
        .map(|labels| labels.0.load(Ordering::SeqCst))
        .expect("the view's data");
    let mut layer = Dom::create_div();
    for i in 0..count {
        layer = layer.with_child(Dom::create_span_with_text(format!("Label {i}")).with_css(&format!(
            "position: absolute; left: {}px; top: {}px; width: 60px; height: 12px; font-size: \
             12px; line-height: 1; white-space: pre; cursor: pointer; :hover {{ background: \
             rgba(66, 134, 244, 0.35); }}",
            10 + (i % 8) * 70,
            10 + (i / 8) * 16,
        )));
    }
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 400.0));
    VirtualViewReturn::with_dom(layer, window, window)
}

/// A label that is REMOVED while the pointer is over it (the view renders again without it)
/// leaves no colour and no key behind, labels that come later get their keys, and a label
/// of the new document is a value from its first enter. The keys of a document number its
/// state-coloured nodes after every render, whatever was hovered.
/// Mutation: `bind_state_colors` keeping the keys the cache holds (`retain_colors` removed):
/// 12 keys after the render of 8 labels.
#[test]
fn a_label_removed_while_hovered_leaves_no_colour_and_no_key() {
    let count = Arc::new(AtomicUsize::new(12));
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(
            RefAny::new(CountedLabels(count.clone())),
            VirtualViewCallback::create(counted_label_page),
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
    let view = |lw: &LayoutWindow| {
        *lw.layout_results
            .keys()
            .find(|dom| **dom != DomId::ROOT_ID)
            .expect("the view's document")
    };
    let child = view(&lw);
    assert_eq!(color_keys(&lw, child), 12, "a key per label");

    // The pointer is over the last label when the view renders again without it.
    drop(hover(&mut lw, child, 11, true));
    assert_eq!(hover_painted(&lw, child), 1, "harness: the last label is hovered");
    count.store(8, Ordering::SeqCst);
    render_the_view_again(&mut lw, &ws);
    let child = view(&lw);
    assert_eq!(
        lw.layout_results[&child].styled_dom.node_count(),
        17,
        "harness: the view rendered 8 labels"
    );
    assert_eq!(hover_painted(&lw, child), 0, "no label is painted hovered");
    assert_eq!(color_keys(&lw, child), 8, "the keys are those of the labels that are left");

    // Four labels come back: each has its key and its rect, none is hovered.
    count.store(12, Ordering::SeqCst);
    render_the_view_again(&mut lw, &ws);
    let child = view(&lw);
    assert_eq!(color_keys(&lw, child), 12, "a key per label again");
    assert_eq!(hover_painted(&lw, child), 0, "the label that came back is not hovered");
    lw.frame_report.reset_counters();
    drop(hover(&mut lw, child, 11, true));
    assert_eq!(hover_painted(&lw, child), 1, "its hover is painted");
    assert_eq!(
        (lw.frame_report.dl_rebuilds, lw.frame_report.dl_node_patches),
        (0, 0),
        "by a value: the label was bound by the build of its document's list"
    );
}

/// A state colour that FADES (the window's DOM: `animation: background`) changes no list
/// either: the hover starts the fade and owes nothing else, every frame of the fade is the
/// value of the node's key, and the key stays when the fade ends - so the leave and its
/// fade back are values too. It was a build of the window's list per enter and per leave
/// (the patch refused the unchanged run of a fading node) and a key minted and let go per
/// fade.
/// Mutations: the fade's end releasing the key of a state colour (`tick_animations`): the
/// leave paints a run (`dl_node_patches` 1) or builds the list; a fading node sent to the
/// patch as before (`dl_rebuilds` 1 at the enter).
#[test]
fn a_hover_that_fades_changes_no_list_from_its_first_frame() {
    use azul_css::props::basic::color::ColorU;

    let rest = ColorU {
        r: 0xee,
        g: 0xee,
        b: 0xee,
        a: 0xff,
    };
    let lit = ColorU {
        r: 0x33,
        g: 0x66,
        b: 0xff,
        a: 0xff,
    };
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(Dom::create_div().with_css(
        "width: 80px; height: 24px; background: #eeeeee; animation: background 160ms linear; \
         :hover { background: #3366ff; }",
    ));
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let (root, node) = (DomId::ROOT_ID, NodeId::new(1));

    // The colour the renderers fill the box with.
    let shown = |lw: &LayoutWindow| {
        let list = &lw.layout_results[&root].display_list;
        let cache = lw.gpu_state_manager.caches.get(&root);
        list.items.iter().enumerate().find_map(|(index, item)| {
            if list.node_mapping.get(index).copied().flatten() != Some(node) {
                return None;
            }
            let DisplayListItem::Rect { color, .. } = item else {
                return None;
            };
            let value = list
                .color_binding(index)
                .and_then(|_| cache.and_then(|cache| cache.color_value(node)));
            Some(value.unwrap_or(*color))
        })
    };
    // The pointer enters or leaves the box, as the shell applies it (`apply_hover_restyle`).
    let state_change = |lw: &mut LayoutWindow, enters: bool| {
        let before = lw.node_states(root, [node]);
        let (entered_nodes, left_nodes) = if enters {
            (vec![node], Vec::new())
        } else {
            (Vec::new(), vec![node])
        };
        let restyled = lw
            .layout_results
            .get_mut(&root)
            .expect("laid out")
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
            lw.seed_state_change_transitions(root, &before, &restyled.changed_nodes),
            "harness: the state change starts a fade (enters: {enters})"
        );
        assert!(
            lw.patch_restyled_decorations(root, &restyled.changed_nodes),
            "the restyle of a fading state colour owes no list (enters: {enters})"
        );
    };
    // The frames of the next 0.4 s; a frame that owes a list is counted by `dl_rebuilds`.
    let frames = |lw: &mut LayoutWindow| {
        for _ in 0..24 {
            let _ = lw.tick_animations(1.0 / 60.0);
            if lw.take_animation_frame_work()
                == azul_core::events::ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            {
                lw.regenerate_display_list_for_dom(root);
            }
        }
    };

    assert_eq!(shown(&lw), Some(rest), "harness: the box rests");
    assert_eq!(color_keys(&lw, root), 1, "harness: the box has its key from the build");
    let list = Arc::as_ptr(&lw.layout_results[&root].display_list);
    lw.frame_report.reset_counters();

    state_change(&mut lw, true);
    assert_eq!(shown(&lw), Some(rest), "frame 0 of the fade holds the colour on screen");
    frames(&mut lw);
    assert!(lw.css_transitions.is_empty(), "harness: the fade is over");
    assert_eq!(shown(&lw), Some(lit), "the box is painted in its hover colour");
    assert_eq!(color_keys(&lw, root), 1, "the key of a state colour stays when its fade ends");

    state_change(&mut lw, false);
    frames(&mut lw);
    assert!(lw.css_transitions.is_empty(), "harness: the fade back is over");
    assert_eq!(shown(&lw), Some(rest), "and at rest again after the leave");

    assert_eq!(
        (lw.frame_report.dl_rebuilds, lw.frame_report.dl_node_patches),
        (0, 0),
        "no list was built or patched for the enter, the leave or a frame of either fade"
    );
    assert!(
        core::ptr::eq(list, Arc::as_ptr(&lw.layout_results[&root].display_list)),
        "the window's list is the allocation it was"
    );
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

/// The FIRST picture that arrives for a node that shows a null image (a map's tile being
/// drawn) leaves the list the renderer holds too, once the renderer has keyed the node's
/// place (`reserve_image_places` of the GPU scan, A4's `93aeb0a41`: the slot's key shows
/// `ImageRefHash::PLACE`): the list names that key before and after. Without a place the
/// first picture is a change of the list, as before.
/// Mutation: the null-image alternative of `slot_keeps_its_shape` removed (the list is owed
/// again after the first arrival).
#[test]
fn the_first_picture_on_a_kept_place_leaves_the_list_the_renderer_holds() {
    use azul_core::resources::{
        ImageKey, ImageRef, ImageRefHash, ImageSlot, RawImage, RawImageData, RawImageFormat,
    };

    use crate::overlay::ContentChange;

    let being_drawn = || ImageRef::null_image(4, 4, RawImageFormat::RGBA8, Vec::new());
    let pixels = |fill: u8| {
        ImageRef::new_rawimage(RawImage {
            pixels: RawImageData::U8(vec![fill; 4 * 4 * 4].into()),
            width: 4,
            height: 4,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        })
        .expect("a picture in memory")
    };
    let dom = Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(Dom::create_image(being_drawn()).with_css("width: 40px; height: 30px;"))
        .with_child(Dom::create_image(being_drawn()).with_css("width: 40px; height: 30px;"));
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
    let (root, kept, unkept) = (DomId::ROOT_ID, NodeId::new(1), NodeId::new(2));
    const ENV: u64 = 7;
    // What the GPU scan does for a node that shows a null image: its slot gets its key, and
    // the key shows the place.
    let key = ImageKey::unique_image_slot(lw.id_namespace);
    lw.renderer_resources
        .image_slots
        .insert(ImageSlot::Node(root, kept), key);
    lw.renderer_resources
        .image_key_map
        .insert(key, ImageRefHash::PLACE);
    lw.gpu_state_manager
        .note_display_lists_submitted(&lw.layout_results, ENV);
    let held = |lw: &LayoutWindow| {
        lw.gpu_state_manager
            .display_list_is_held(root, &lw.layout_results[&root].display_list, ENV)
    };
    assert!(held(&lw), "harness: the renderer holds the list it was sent");

    let _ = lw.apply_content_change(ContentChange::Image {
        dom_id: root,
        node_id: kept,
        image: pixels(200),
        dirty_rect: None,
    });
    assert!(held(&lw), "the first picture on a kept place is pixels under the place's key");

    let _ = lw.apply_content_change(ContentChange::Image {
        dom_id: root,
        node_id: unkept,
        image: pixels(90),
        dirty_rect: None,
    });
    assert!(!held(&lw), "a node without a place: its first picture is a change of the list");
}

/// A view of one image (a map's tile layer).
extern "C" fn tile_page(_data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let layer = Dom::create_div()
        .with_child(Dom::create_image(picture(4)).with_css("width: 40px; height: 30px;"));
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 400.0));
    VirtualViewReturn::with_dom(layer, window, window)
}

/// A3-window-54: a tile that arrives in a VIEW is patched into the view's list where it lies.
/// The view's document has a solver cache of its own that holds its list; the patch lets it
/// go of the list and gives it the patched one, as the window's cache always did. It copied
/// the view's whole list for the first tile after every pass of the view, and the cache kept
/// the list with the tile from before.
/// Mutation: the `else` arm of `patch_display_list` answering `None` again (the list is
/// another allocation after the patch, and the cache holds the old one).
#[test]
fn a_tile_that_arrives_in_a_view_is_patched_into_the_list_where_it_lies() {
    use crate::overlay::ContentChange;

    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(tile_page))
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
    let list = Arc::as_ptr(&lw.layout_results[&child].display_list);
    let _ = lw.apply_content_change(ContentChange::Image {
        dom_id: child,
        node_id: NodeId::new(1),
        image: picture(4),
        dirty_rect: None,
    });
    assert!(
        core::ptr::eq(list, Arc::as_ptr(&lw.layout_results[&child].display_list)),
        "the view's list is the allocation it was"
    );
    assert!(
        lw.child_layout_caches[&child]
            .cached_display_list
            .as_ref()
            .is_none_or(|cached| Arc::ptr_eq(&cached.5, &lw.layout_results[&child].display_list)),
        "the view's solver cache holds the patched list"
    );
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
    // "cabcab", not "cab cab": the space is a character "abc" did not have, and by the rule
    // this test pins it is another requirement (the test resolved once here, rightly).
    lay_out_text(&mut lw, &ws, "cabcab");
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

// -- A3-window-33: CSS dirt of a DOM whose turn the one slot is not ----------------------------

/// Dirt held for the window's DOM while the slot is a view's is one entry per node (the
/// widest scope), is taken by the window's next layout pass, and leaves the view's alone.
#[test]
fn css_dirt_held_beside_the_slot_is_taken_by_its_own_doms_pass() {
    use azul_css::props::property::RelayoutScope;

    let (mut lw, child) = label_window(4);
    lw.pending_css_dirty = Some((child, vec![(NodeId::new(1), RelayoutScope::Full)]));
    lw.hold_css_dirt(DomId::ROOT_ID, vec![(NodeId::new(0), RelayoutScope::None)]);
    lw.hold_css_dirt(
        DomId::ROOT_ID,
        vec![(NodeId::new(0), RelayoutScope::Full), (NodeId::new(1), RelayoutScope::None)],
    );
    assert_eq!(
        lw.css_dirty_waiting.get(&DomId::ROOT_ID),
        Some(&vec![(NodeId::new(0), RelayoutScope::Full), (NodeId::new(1), RelayoutScope::None)]),
        "per node the widest scope"
    );
    assert!(!lw.animation_tick_is_values_only(), "a layout is owed");

    // The window is laid out again (its DOM as it is).
    let ws = lw.current_window_state.clone();
    let root = lw.layout_results[&DomId::ROOT_ID].styled_dom.clone();
    lw.layout_and_generate_display_list(
        root,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    assert!(
        !lw.css_dirty_waiting.contains_key(&DomId::ROOT_ID),
        "the window's pass took what waited for it"
    );
}

// -- A3-window-30: a node's layout rect ---------------------------------------------------------

/// The rect of a node is found by the tree's index of its boxes: one step per call, whatever
/// the size of the view (it was a scan of every layout node), and the box it finds is the
/// one `get_node_bounds` reads. No node has no rect.
#[test]
fn a_nodes_layout_rect_is_found_without_a_walk_of_the_layout_tree() {
    use azul_core::{dom::DomNodeId, styled_dom::NodeHierarchyItemId};

    for labels in [40, 160] {
        let (lw, child) = label_window(labels);
        let last = NodeId::new(1 + 2 * (labels - 1));
        crate::window::LAYOUT_NODE_LOOKUPS.with(|work| work.set(0));
        let rect = lw
            .get_node_layout_rect(DomNodeId {
                dom: child,
                node: NodeHierarchyItemId::from_crate_internal(Some(last)),
            })
            .expect("the last label has a box");
        assert_eq!(
            crate::window::LAYOUT_NODE_LOOKUPS.with(core::cell::Cell::get),
            1,
            "{labels} labels: one lookup"
        );
        let bounds = lw.get_node_bounds(child, last).expect("the last label has a box");
        assert_eq!(
            (rect.origin.x as isize, rect.origin.y as isize),
            (bounds.origin.x, bounds.origin.y),
            "the same box as `get_node_bounds`"
        );
        assert_eq!(
            lw.get_node_layout_rect(DomNodeId {
                dom: child,
                node: NodeHierarchyItemId::from_crate_internal(None),
            }),
            None,
            "no node, no rect (it was the rect of the first anonymous box)"
        );
    }
}

// -- A3-window-37: the writers of the GPU value maps say so (A3-core-08) -----------------------

/// A bar that comes on screen writes its opacity and moves the cache's value generation; a
/// frame in which it rests (inside its hold) writes nothing and leaves it. A reader that
/// skips work on an unchanged generation needs both.
#[test]
fn a_bar_that_shows_moves_the_value_generation_and_a_resting_one_does_not() {
    use azul_core::task::Instant;

    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_class("box".into())
            .with_css("width: 300px; height: 100px; overflow-y: scroll;")
            .with_child(Dom::create_div().with_css("height: 400px; width: 280px;")),
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
    let scroller = {
        let nodes = lw.layout_results[&DomId::ROOT_ID].styled_dom.node_data.as_container();
        (0..nodes.len())
            .map(NodeId::new)
            .find(|n| nodes[*n].has_class("box"))
            .expect("the box")
    };
    let generation = |lw: &LayoutWindow| {
        lw.gpu_state_manager
            .caches
            .get(&DomId::ROOT_ID)
            .map_or(0, azul_core::gpu::GpuValueCache::value_generation)
    };
    // A frame before the scroll: the bar's key and its resting opacity are there.
    let _ = lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
    let hidden = generation(&lw);

    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        scroller,
        LogicalPosition::new(0.0, 60.0),
        Instant::now(),
    );
    let _ = lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
    let shown = generation(&lw);
    assert_ne!(shown, hidden, "the bar's opacity was written: 0 -> 1");

    // Inside the hold of 500 ms the bar stays at 1.
    let _ = lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
    assert_eq!(generation(&lw), shown, "nothing was written");
}

// -- A3-window-06: the per-frame search for callback backgrounds -------------------------------

fn background_items_scanned() -> usize {
    crate::window::BACKGROUND_ITEM_SCAN.with(core::cell::Cell::get)
}

/// The budget of A3-window-06: with a callback image registered by name (every window of a
/// build with the built-in materials has some), a frame over the lists of the frame before
/// looks at no display-list item to find the `Image` items; a list that was spliced is
/// walked once more, and only that list.
#[test]
fn a_frame_over_the_lists_of_the_frame_before_walks_no_item_for_callback_backgrounds() {
    let _binding = BoundWhenPainted::on_this_thread();
    let (mut lw, child) = label_window(40);
    lw.image_cache.add_css_image_id(
        "probe".into(),
        azul_core::resources::ImageRef::callback(
            crate::callbacks::RenderImageCallback::create(late_canvas).to_core(),
            RefAny::new(()),
        ),
    );
    lw.prepare_frame_content();
    assert!(background_items_scanned() > 0, "the first frame walks the lists");

    crate::window::BACKGROUND_ITEM_SCAN.with(|scanned| scanned.set(0));
    lw.prepare_frame_content();
    lw.prepare_frame_content();
    assert_eq!(background_items_scanned(), 0, "the same lists: nothing is walked");

    // A hover colour is spliced into the view's list (or the list is built again).
    let _ = hover(&mut lw, child, 3, true);
    lw.prepare_frame_content();
    assert_eq!(
        background_items_scanned(),
        lw.layout_results[&child].display_list.items.len(),
        "the view's list, once; the window's own was not touched"
    );
}

// -- A3-window-14: the reconciliation of a rebuild keeps a first rect only for a node that
// declares a move, and finds the mounted and unmounted roots without a set of every node -------

/// A body with a spacer `spacer_px` high, `boxes` boxes of 20 px below it (the box `mover`
/// declares `animation: move`), and with `panel` a box of two children after them.
fn rects_page(spacer_px: f32, boxes: usize, mover: Option<usize>, panel: bool) -> StyledDom {
    let mut body = Dom::create_body()
        .with_css("margin: 0px; padding: 0px;")
        .with_child(Dom::create_div().with_css(&format!("width: 100px; height: {spacer_px}px;")));
    for i in 0..boxes {
        let own = if mover == Some(i) {
            "animation: move 200ms ease-out;"
        } else {
            ""
        };
        body = body
            .with_child(Dom::create_div().with_css(&format!("width: 50px; height: 20px; {own}")));
    }
    if panel {
        body = body.with_child(
            Dom::create_div()
                .with_css("width: 80px; height: 40px;")
                .with_child(Dom::create_div().with_css("width: 10px; height: 10px;"))
                .with_child(Dom::create_div().with_css("width: 10px; height: 10px;")),
        );
    }
    StyledDom::create(&mut body, azul_css::css::Css::empty())
}

/// `first` laid out, then `next` reconciled against it as the shells do before they lay it out.
fn reconciliation_of(first: StyledDom, mut next: StyledDom) -> crate::window::PendingReconciliation {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_new_generation(
        first,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw.begin_reconciliation(DomId::ROOT_ID, &mut next, azul_core::task::Instant::now())
}

/// Every matched node's rect was looked up and kept on every rebuild; only a node that declares
/// a move is ever slid from it. A page of boxes that all moved down 40 px and declare nothing
/// keeps none, at 50 boxes and at 200.
#[test]
fn a_rebuild_keeps_no_first_rect_for_a_page_without_a_declared_move() {
    for boxes in [50usize, 200] {
        let pending = reconciliation_of(
            rects_page(20.0, boxes, None, false),
            rects_page(60.0, boxes, None, false),
        );
        assert!(
            pending.node_moves.len() >= boxes,
            "premise: the boxes are matched ({} of {boxes})",
            pending.node_moves.len()
        );
        assert_eq!(
            pending.first_rects.len(),
            0,
            "nothing declares a move: no rect of the old layout is read ({boxes} boxes)"
        );
    }
}

/// The node that declares a move keeps its first rect (it slides from there), and it alone.
#[test]
fn a_rebuild_keeps_the_first_rect_of_the_node_that_declares_a_move() {
    let pending = reconciliation_of(
        rects_page(20.0, 50, Some(7), false),
        rects_page(60.0, 50, Some(7), false),
    );
    assert_eq!(pending.move_modes.len(), 1, "premise: one box declares a move");
    assert_eq!(pending.first_rects.len(), 1, "its rect before the rebuild, and no other");
    let rect = pending.first_rects.values().next().expect("the rect");
    assert_eq!(
        (rect.size.width, rect.size.height),
        (50.0, 20.0),
        "the box the old layout gave the node that slides"
    );
}

/// A subtree that comes with the rebuild is mounted at its root and entered node by node, in
/// document order; one that goes is unmounted at its root: as the sets of node ids said.
#[test]
fn a_subtree_that_comes_or_goes_is_mounted_or_unmounted_at_its_root() {
    let came = reconciliation_of(rects_page(20.0, 12, None, false), rects_page(20.0, 12, None, true));
    assert_eq!(came.entered.len(), 3, "the panel and its two children are new: {:?}", came.entered);
    assert!(came.entered.windows(2).all(|pair| pair[0] < pair[1]), "in document order");
    assert_eq!(came.mounted, vec![came.entered[0]], "mounted once, at the panel");
    assert!(came.unmounted.is_empty(), "nothing went");

    let went = reconciliation_of(rects_page(20.0, 12, None, true), rects_page(20.0, 12, None, false));
    assert!(went.entered.is_empty() && went.mounted.is_empty(), "nothing came");
    assert_eq!(went.unmounted.len(), 1, "unmounted once, at the panel: {:?}", went.unmounted);
}

// -- A3-window-44: a geometry write to a node of a view's document lays that document out -------

/// The place and the cost of label 3 of a view of `labels` labels after `set_node_style` gave it
/// another `left` and `top`: `(how far its box moved, list builds, nodes of the view's document
/// the solver took over by the diff)`.
fn a_label_written_to_another_place(labels: usize) -> ((f32, f32), usize, usize) {
    use azul_core::{dom::DomNodeId, styled_dom::NodeHierarchyItemId};

    use crate::overlay::{ContentChange, ContentDirtyTier};

    let (mut lw, child) = label_window(labels);
    // Label 3 (a plain one: no background, no transform of its own): its span is node 7 of
    // the view's document, at left 10 + 3 * 70 = 220, top 10.
    let span = NodeId::new(1 + 2 * 3);
    let id = DomNodeId {
        dom: child,
        node: NodeHierarchyItemId::from_crate_internal(Some(span)),
    };
    let before = lw.get_node_position(id).expect("the label is laid out");
    lw.frame_report.reset_counters();
    let result = lw.apply_content_change(ContentChange::NodeStyle {
        dom_id: child,
        node_id: span,
        style: azul_css::css::Css::parse_inline(
            "position: absolute; left: 245px; top: 31px; width: 60px; height: 12px; font-size: \
             12px; line-height: 1; white-space: pre; cursor: pointer; :hover { background: \
             rgba(66, 134, 244, 0.35); }",
        ),
    });
    assert_eq!(result.tier, ContentDirtyTier::Relayout, "`left` and `top` are geometry");
    let after = lw.get_node_position(id).expect("the label is laid out");
    (
        (after.x - before.x, after.y - before.y),
        lw.frame_report.dl_rebuilds as usize,
        lw.child_layout_caches[&child].last_fingerprint_skips,
    )
}

/// A label of a map view dragged with `left` / `top` (one `set_node_style` per mouse move):
/// its box is where the style puts it, 25 px right and 21 px down of (220, 10). The write built
/// the view's whole list from the boxes of the layout before it and laid nothing out: the label
/// stayed where it was until the view rendered again.
#[test]
fn a_label_written_to_another_place_is_laid_out_there() {
    let (moved, list_builds, _) = a_label_written_to_another_place(40);
    assert_eq!(moved, (25.0, 21.0), "left 220 -> 245, top 10 -> 31");
    assert_eq!(list_builds, 0, "no list is built from the boxes of the layout before the write");
}

/// The pass that places it compares the written node alone: every other node of the view's
/// document is taken over by the diff, at 40 labels (81 nodes) and at 160 (321).
#[test]
fn the_pass_after_a_write_compares_the_written_node_alone() {
    for labels in [40usize, 160] {
        let nodes = 1 + 2 * labels;
        let (_, _, taken_over) = a_label_written_to_another_place(labels);
        assert!(
            taken_over + 4 >= nodes,
            "{labels} labels: {taken_over} of {nodes} nodes taken over without a comparison"
        );
    }
}

// -- A3-window-43: a node dragged inside a view moves by a value; its view's list opens the frame

/// The reference frames the list of `dom` opens under `key`.
fn frames_under(lw: &LayoutWindow, dom: DomId, key: azul_core::resources::TransformKey) -> usize {
    lw.layout_results[&dom]
        .display_list
        .items
        .iter()
        .filter(|item| {
            matches!(item, DisplayListItem::PushReferenceFrame { transform_key, .. } if *transform_key == key)
        })
        .count()
}

/// Label 0 of a view (node 1 of its document) is dragged: the first move builds the VIEW's
/// list once, which opens the node's frame, and asks for a repaint; every later move writes a
/// value and builds nothing; the window's own list is the allocation it was throughout. The
/// first move used to ask for the display-list tier, for which the shell builds the window's
/// list and never the view's.
#[test]
fn a_node_dragged_inside_a_view_moves_by_a_value_after_its_first_move() {
    use azul_core::{
        drag::{DragContext, DragData},
        events::ProcessEventResult,
    };

    let (mut lw, child) = label_window(16);
    let node = NodeId::new(1);
    let press = LogicalPosition::new(20.0, 15.0);
    // A weak reference: it names the list by its allocation and keeps that allocation from
    // being handed to a list built later.
    let windows_list = Arc::downgrade(&lw.layout_results[&DomId::ROOT_ID].display_list);
    lw.gesture_drag_manager.active_drag =
        Some(DragContext::node_drag(child, node, press, DragData::default(), 1));
    lw.frame_report.reset_counters();

    lw.gesture_drag_manager
        .update_active_drag_positions(LogicalPosition::new(30.0, 18.0));
    assert_eq!(
        lw.update_drag_visual(),
        ProcessEventResult::ShouldReRenderCurrentWindow,
        "the first move: the view's list is built here, the frame is a repaint"
    );
    assert_eq!(lw.frame_report.dl_rebuilds, 1, "the view's list, once");
    let key = lw
        .gpu_state_manager
        .caches
        .get(&child)
        .and_then(|cache| cache.anim_transform_keys.get(&node).copied())
        .expect("the dragged node is keyed");
    assert_eq!(frames_under(&lw, child, key), 1, "the view's list opens the node's frame");

    for step in 1..=5 {
        let pointer = LogicalPosition::new(30.0 + 7.0 * step as f32, 18.0 + 3.0 * step as f32);
        lw.gesture_drag_manager.update_active_drag_positions(pointer);
        assert_eq!(lw.update_drag_visual(), ProcessEventResult::ShouldReRenderCurrentWindow);
        let value = lw
            .gpu_state_manager
            .caches
            .get(&child)
            .and_then(|cache| cache.anim_current_transform_values.get(&node).copied())
            .expect("the dragged node has a value");
        assert_eq!(
            value,
            azul_core::transform::ComputedTransform3D::new_translation(
                pointer.x - press.x,
                pointer.y - press.y,
                0.0
            ),
            "move {step}: the pointer's way since the press"
        );
    }
    assert_eq!(lw.frame_report.dl_rebuilds, 1, "five moves built no list");
    assert!(
        core::ptr::eq(
            windows_list.as_ptr(),
            Arc::as_ptr(&lw.layout_results[&DomId::ROOT_ID].display_list)
        ),
        "the window's own list is the allocation it was"
    );

    assert!(lw.end_drag_visual(), "the drag moved something back");
    assert_eq!(frames_under(&lw, child, key), 0, "the view's list closed the frame");
}

// -- A3-window-49: a label whose `left` / `top` alone moved is moved where it lies -------------

/// The data of the view of [`turned_label_page`]: how many labels it draws, and where label 2
/// stands and how far it is turned.
struct TurnedLabels {
    count: usize,
    left: f32,
    top: f32,
    degrees: f32,
}

/// The style a map editor writes for a label it drags or turns (`set_node_style` per mouse
/// move): its place, its pivot, its angle.
fn dragged_label_css(left: f32, top: f32, degrees: f32) -> String {
    format!(
        "position: absolute; left: {left}px; top: {top}px; width: 60px; height: 12px; font-size: \
         12px; line-height: 1; white-space: pre; transform-origin: 0px 9px; transform: \
         rotate({degrees}deg);"
    )
}

/// A map's label layer in which every label opens a reference frame (the style of
/// [`dragged_label_css`]): `count` spans with a text, label 2 at the place and angle of the
/// view's data. Child DOM: div 0, then span `1 + 2 i` and its text `2 + 2 i` for label `i`.
extern "C" fn turned_label_page(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let (count, left, top, degrees) = data
        .downcast_ref::<TurnedLabels>()
        .map(|labels| (labels.count, labels.left, labels.top, labels.degrees))
        .expect("the view's data");
    let mut layer = Dom::create_div();
    for i in 0..count {
        let css = if i == 2 {
            dragged_label_css(left, top, degrees)
        } else {
            dragged_label_css(
                (10 + (i % 8) * 70) as f32,
                (10 + (i / 8) * 16) as f32,
                if i % 3 == 2 { 12.0 } else { 0.0 },
            )
        };
        layer = layer.with_child(Dom::create_span_with_text(format!("Label {i}")).with_css(&css));
    }
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 400.0));
    VirtualViewReturn::with_dom(layer, window, window)
}

/// A window whose view holds `count` labels of [`turned_label_page`], laid out; the view's
/// document.
fn turned_label_window(count: usize, left: f32, top: f32, degrees: f32) -> (LayoutWindow, DomId) {
    let data = TurnedLabels {
        count,
        left,
        top,
        degrees,
    };
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(RefAny::new(data), VirtualViewCallback::create(turned_label_page))
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

/// The span of label 2 of [`turned_label_page`].
const DRAGGED: usize = 5;

/// The app writes label 2's style: another place, another angle.
fn write_label(
    lw: &mut LayoutWindow,
    child: DomId,
    left: f32,
    top: f32,
    degrees: f32,
) -> crate::overlay::ContentDirtyTier {
    lw.apply_content_change(crate::overlay::ContentChange::NodeStyle {
        dom_id: child,
        node_id: NodeId::new(DRAGGED),
        style: azul_css::css::Css::parse_inline(&dragged_label_css(left, top, degrees)),
    })
    .tier
}

/// What a move of the drag of label 2 looked at, in a view of `count` labels: `(layout boxes,
/// display-list items, layout passes, list builds, the tier, how far the box moved)` of the
/// third move (the first may be any write, the second finds the frames of the list once).
fn a_move_of_a_drag(
    count: usize,
) -> (usize, usize, u32, u32, crate::overlay::ContentDirtyTier, (f32, f32)) {
    use azul_core::{dom::DomNodeId, styled_dom::NodeHierarchyItemId};

    use crate::{solver3::display_list::node_patch::MOVE_ITEM_VISITS, window::MOVED_BOXES};

    let (mut lw, child) = turned_label_window(count, 150.0, 10.0, 12.0);
    let id = DomNodeId {
        dom: child,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(DRAGGED))),
    };
    drop(write_label(&mut lw, child, 175.0, 31.0, 12.0));
    drop(write_label(&mut lw, child, 178.0, 33.0, 12.0));
    let before = lw.get_node_position(id).expect("the label is laid out");
    lw.frame_report.reset_counters();
    MOVED_BOXES.with(|boxes| boxes.set(0));
    MOVE_ITEM_VISITS.with(|items| items.set(0));
    let tier = write_label(&mut lw, child, 181.0, 36.0, 12.0);
    let after = lw.get_node_position(id).expect("the label is laid out");
    (
        MOVED_BOXES.with(core::cell::Cell::get),
        MOVE_ITEM_VISITS.with(core::cell::Cell::get),
        lw.frame_report.layout_passes,
        lw.frame_report.dl_rebuilds,
        tier,
        (after.x - before.x, after.y - before.y),
    )
}

/// One move of a label drag looks at the label's own boxes and items: as many in a view of 40
/// labels as in one of 160, with no layout pass and no list build. It was a pass over the
/// view's document (the cascade's tail, the walks of a layout pass, the patched list) per
/// mouse move.
/// Mutations: `move_node_in_place` answering `None` (the pass runs: `layout_passes` 1, tier
/// `Relayout`); `frame_push_of` walking the list on every ask (the items grow with the view);
/// `bump_for_a_move` not keeping the table of frames (the same).
#[test]
fn a_move_of_a_label_drag_looks_at_that_label_alone() {
    let small = a_move_of_a_drag(40);
    let large = a_move_of_a_drag(160);
    for (count, (boxes, items, passes, builds, tier, moved)) in [(40, small), (160, large)] {
        assert_eq!(tier, crate::overlay::ContentDirtyTier::Paint, "{count} labels: a move is a repaint");
        assert_eq!(passes, 0, "{count} labels: no layout pass");
        assert_eq!(builds, 0, "{count} labels: no list build");
        assert_eq!(moved, (3.0, 3.0), "{count} labels: left 178 -> 181, top 33 -> 36");
        assert!(boxes >= 1 && boxes <= 4, "{count} labels: the label's boxes, {boxes}");
        assert!(items >= 2 && items <= 64, "{count} labels: the label's items, {items}");
    }
    assert_eq!(small.0, large.0, "the boxes looked at do not grow with the view");
    assert_eq!(small.1, large.1, "the items looked at do not grow with the view");
}

/// Whether two lists say the same: the same items in the same order for the same nodes and
/// paint calls, their boxes and glyphs within a hundredth of a pixel (a sum of the same terms
/// in another order), the proven backgrounds on the same runs.
fn same_lists(got: &DisplayList, want: &DisplayList) -> Result<(), String> {
    let near = |a: f32, b: f32| (a - b).abs() <= 0.01;
    let near_rect = |a: LogicalRect, b: LogicalRect| {
        near(a.origin.x, b.origin.x)
            && near(a.origin.y, b.origin.y)
            && near(a.size.width, b.size.width)
            && near(a.size.height, b.size.height)
    };
    if got.items.len() != want.items.len() {
        return Err(format!("{} items against {}", got.items.len(), want.items.len()));
    }
    for (i, (a, b)) in got.items.iter().zip(&want.items).enumerate() {
        if core::mem::discriminant(a) != core::mem::discriminant(b) {
            return Err(format!("item {i}: {a:?} against {b:?}"));
        }
        if got.node_mapping.get(i) != want.node_mapping.get(i)
            || got.layout_node_mapping.get(i) != want.layout_node_mapping.get(i)
        {
            return Err(format!("item {i}: another node or paint call"));
        }
        match (a.bounds(), b.bounds()) {
            (Some(x), Some(y)) if near_rect(x, y) => {}
            (None, None) => {}
            (x, y) => return Err(format!("item {i}: bounds {x:?} against {y:?}")),
        }
        if let (
            DisplayListItem::Text { glyphs: x, .. },
            DisplayListItem::Text { glyphs: y, .. },
        ) = (a, b)
        {
            let same = x.len() == y.len()
                && x.iter().zip(y).all(|(p, q)| {
                    p.index == q.index && near(p.point.x, q.point.x) && near(p.point.y, q.point.y)
                });
            if !same {
                return Err(format!("item {i}: other glyphs"));
            }
        }
        match (
            got.uniform_text_bgs.get(i).copied().flatten(),
            want.uniform_text_bgs.get(i).copied().flatten(),
        ) {
            (Some((c, r)), Some((d, s))) if c == d && near_rect(r.0, s.0) => {}
            (None, None) => {}
            (x, y) => return Err(format!("item {i}: proven background {x:?} against {y:?}")),
        }
    }
    Ok(())
}

/// After two moves of the drag the view's document is what a fresh layout of the moved DOM
/// gives: every box where that layout puts it (in the window's result and in the solver's
/// cache, which the next pass starts from) and the list the list of that layout.
/// Mutations: `+ delta` -> `- delta` in `move_node_in_place` or in `move_frame`; the
/// `PushReferenceFrame` arm of `move_frame` removed (the frame's box stays behind); the
/// solver's positions not moved; the compact slots not written (the next pass puts the label
/// back: the last block); the box's `AbsPlacement` not dropped by the move (the next pass
/// places the label from its last solve, at (175, 31): the solver's own check panics in a
/// test build, the last assertion fails without it).
#[test]
fn a_label_moved_where_it_lies_stands_as_a_fresh_layout_puts_it() {
    let (mut lw, child) = turned_label_window(40, 150.0, 10.0, 12.0);
    drop(write_label(&mut lw, child, 175.0, 31.0, 12.0));
    drop(write_label(&mut lw, child, 178.0, 33.0, 12.0));
    assert_eq!(
        write_label(&mut lw, child, 181.5, 36.25, 12.0),
        crate::overlay::ContentDirtyTier::Paint,
        "premise: the third move is made where the label lies"
    );
    // The solver keeps no placement for the moved box: the one it had was solved from the
    // insets of (175, 31), and no solve was made for where the box is now.
    let moved_box = lw.layout_results[&child].layout_tree.dom_to_layout[&NodeId::new(DRAGGED)][0]
        .index();
    assert!(
        lw.child_layout_caches[&child]
            .cache_map
            .entries
            .get(moved_box)
            .and_then(|entry| entry.abs_interior.as_deref())
            .is_none_or(|run| run.placement.is_none()),
        "the placement of the solve before the moves does not stand for the moved box"
    );
    let (fresh, fresh_child) = turned_label_window(40, 181.5, 36.25, 12.0);

    let near = |a: f32, b: f32| (a - b).abs() <= 0.01;
    let got = &lw.layout_results[&child];
    let want = &fresh.layout_results[&fresh_child];
    assert_eq!(got.calculated_positions.len(), want.calculated_positions.len());
    for (i, (a, b)) in got
        .calculated_positions
        .iter()
        .zip(&want.calculated_positions)
        .enumerate()
    {
        assert!(near(a.x, b.x) && near(a.y, b.y), "box {i}: {a:?}, a fresh layout: {b:?}");
    }
    for (i, (a, b)) in lw.child_layout_caches[&child]
        .calculated_positions
        .iter()
        .zip(&want.calculated_positions)
        .enumerate()
    {
        assert!(near(a.x, b.x) && near(a.y, b.y), "the solver's box {i}: {a:?}, fresh: {b:?}");
    }
    if let Err(difference) = same_lists(&got.display_list, &want.display_list) {
        panic!("the moved list against the list of a fresh layout: {difference}");
    }

    // A pass over the document afterwards (label 0 gets another width: a write that is no
    // move) starts from the moved boxes and from the styles as they are: label 2 stays.
    use azul_core::{dom::DomNodeId, styled_dom::NodeHierarchyItemId};
    let tier = lw
        .apply_content_change(crate::overlay::ContentChange::NodeStyle {
            dom_id: child,
            node_id: NodeId::new(1),
            style: azul_css::css::Css::parse_inline(
                "position: absolute; left: 10px; top: 10px; width: 64px; height: 12px; font-size: \
                 12px; line-height: 1; white-space: pre;",
            ),
        })
        .tier;
    assert_eq!(tier, crate::overlay::ContentDirtyTier::Relayout, "premise: a width is a pass");
    let label = |dom: DomId| DomNodeId {
        dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(DRAGGED))),
    };
    let stands = lw.get_node_position(label(child)).expect("the label is laid out");
    let should = fresh
        .get_node_position(label(fresh_child))
        .expect("the label is laid out");
    assert!(
        near(stands.x, should.x) && near(stands.y, should.y),
        "after a pass the label stands at {stands:?}, a fresh layout puts it at {should:?}"
    );
}

// -- Bit for bit: what a pass or a write leaves is what a fresh layout of the same DOM gives ----

/// Whether two documents stand and are painted alike BIT FOR BIT: every box at the same
/// position (the bits of the two `f32`s), and the lists the same items for the same nodes and
/// paint calls (an `f32` prints as the shortest text that reads back to its bits).
fn same_bits(got: &LayoutWindow, dom: DomId, want: &LayoutWindow, want_dom: DomId) -> Result<(), String> {
    same_within(got, dom, want, want_dom, 0)
}

/// How many `f32`s lie between `a` and `b` (0: the same number; a unit in the last place
/// apart: 1): the distance in the float's own steps, whatever its exponent.
fn ulps_apart(a: f32, b: f32) -> u64 {
    if a == b {
        return 0;
    }
    if a.is_nan() || b.is_nan() || a.is_sign_negative() != b.is_sign_negative() {
        return u64::MAX;
    }
    u64::from(a.to_bits().abs_diff(b.to_bits()))
}

/// [`same_bits`] with every coordinate allowed to lie up to `ulps` units in the last place
/// from the other document's (0: bit for bit; a zero of either sign is the same place).
fn same_within(
    got: &LayoutWindow,
    dom: DomId,
    want: &LayoutWindow,
    want_dom: DomId,
    ulps: u64,
) -> Result<(), String> {
    let near = |a: f32, b: f32| ulps_apart(a, b) <= ulps;
    let near_rect = |a: LogicalRect, b: LogicalRect| {
        near(a.origin.x, b.origin.x)
            && near(a.origin.y, b.origin.y)
            && near(a.size.width, b.size.width)
            && near(a.size.height, b.size.height)
    };
    let (got, want) = (&got.layout_results[&dom], &want.layout_results[&want_dom]);
    if got.calculated_positions.len() != want.calculated_positions.len() {
        return Err("another number of boxes".to_string());
    }
    for (i, (a, b)) in got
        .calculated_positions
        .iter()
        .zip(&want.calculated_positions)
        .enumerate()
    {
        if !(near(a.x, b.x) && near(a.y, b.y)) {
            return Err(format!("box {i} stands at {a:?}, a fresh layout puts it at {b:?}"));
        }
    }
    // The items: kind, node and paint call, then every coordinate (not the items' `Debug`
    // text: a frame prints its key, which is another one in another window).
    let (got, want) = (&got.display_list, &want.display_list);
    if got.items.len() != want.items.len() {
        return Err(format!("{} items against {}", got.items.len(), want.items.len()));
    }
    for (i, (a, b)) in got.items.iter().zip(&want.items).enumerate() {
        if core::mem::discriminant(a) != core::mem::discriminant(b)
            || got.node_mapping.get(i) != want.node_mapping.get(i)
            || got.layout_node_mapping.get(i) != want.layout_node_mapping.get(i)
        {
            return Err(format!("item {i}: {a:?} against {b:?}"));
        }
        match (a.bounds(), b.bounds()) {
            (Some(x), Some(y)) if near_rect(x, y) => {}
            (None, None) => {}
            (x, y) => return Err(format!("item {i}: bounds {x:?}, a fresh layout: {y:?}")),
        }
        if let (
            DisplayListItem::Text { glyphs: x, .. },
            DisplayListItem::Text { glyphs: y, .. },
        ) = (a, b)
        {
            let same = x.len() == y.len()
                && x.iter().zip(y).all(|(p, q)| {
                    p.index == q.index && near(p.point.x, q.point.x) && near(p.point.y, q.point.y)
                });
            if !same {
                return Err(format!("item {i}: other glyphs"));
            }
        }
        match (
            got.uniform_text_bgs.get(i).copied().flatten(),
            want.uniform_text_bgs.get(i).copied().flatten(),
        ) {
            (Some((c, r)), Some((d, s))) if c == d && near_rect(r.0, s.0) => {}
            (None, None) => {}
            _ => return Err(format!("item {i}: another proven background")),
        }
    }
    Ok(())
}

/// The data of the view of [`rows_page`]: how high the box above the rows is.
struct RowsPage(f32);

/// A box and forty rows below it in the flow, each with its background in its content box
/// (a rect at `position + padding`: a sum the painter makes). Child DOM: div 0, the box 1,
/// the rows 2 ..= 41.
extern "C" fn rows_page(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let above = data
        .downcast_ref::<RowsPage>()
        .map(|page| page.0)
        .expect("the view's data");
    let mut page = Dom::create_div()
        .with_child(Dom::create_div().with_css(&format!("width: 100px; height: {above}px;")));
    for _ in 0..40 {
        page = page.with_child(Dom::create_div().with_css(
            "width: 90.3px; height: 13.3px; padding: 2.3px; background: #336699; \
             background-clip: content-box;",
        ));
    }
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 900.0));
    VirtualViewReturn::with_dom(page, window, window)
}

/// A window whose view holds [`rows_page`], laid out; the view's document.
fn rows_window(above: f32) -> (LayoutWindow, DomId) {
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(RefAny::new(RowsPage(above)), VirtualViewCallback::create(rows_page))
            .with_css("position: absolute; left: 0px; top: 0px; width: 600px; height: 900px;"),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 900.0);
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

/// The list of a PATCHED pass is the list a build gives, bit for bit. The box above forty
/// rows gets another height (10.7 to 255.8 px): the rows move down by 245.1 px and nothing
/// else of them changes. A pass that copies their runs shifts every coordinate by the row's
/// move, and `(old + 2.3) + move` is not `new + 2.3` in the last bit for some of such sums
/// (two roundings against one; in single precision, rows 3, 4, 6, 8, 10, 12 and 28 of this
/// page when a row's place is the sum of the heights above it): the patched list and a
/// built one differ by an ulp in a row's rect, which is what the equality walks of the
/// diff-first pipeline found. A row that moved is painted again (the diff-first owner's
/// fix in `try_copy_cached_run`: a run is copied only where it lies).
///
/// The test runs its own mutant first, because the page it had (a move by 27.2 px) passed
/// with and without the fix: under `MovedRunPolicy::AlwaysTranslate` (the per-thread twin
/// of `AZ_DL_TRANSLATE_MOVED_RUNS=1`) the same pass must translate the runs of the forty
/// rows - or it is no patched pass that meets moved boxes, and proves nothing - and must
/// leave a list that is NOT the built one - or these sums add exactly, and prove nothing
/// either. Then the default policy: bit for bit the built list.
/// Mutation: a moved node's run copied and shifted again by every pass: the last
/// `same_bits` names the first row whose rect is an ulp off.
#[test]
fn a_box_that_moved_in_a_pass_is_painted_as_a_fresh_layout_paints_it() {
    use azul_css::props::{
        basic::PixelValue, layout::dimensions::LayoutHeight, property::CssProperty,
    };

    use crate::{
        overlay::{ContentChange, ContentDirtyTier},
        solver3::display_list::{
            moved_runs_translated_on_this_thread, set_moved_run_policy_on_this_thread,
            MovedRunPolicy,
        },
    };

    const BEFORE: f32 = 10.7;
    const AFTER: f32 = 255.8;
    // The box above the rows gets its new height: one pass of the view's document.
    let grow = |lw: &mut LayoutWindow, child: DomId| {
        let tier = lw
            .apply_content_change(ContentChange::NodeCss {
                dom_id: child,
                node_id: NodeId::new(1),
                props: vec![CssProperty::height(LayoutHeight::Px(PixelValue::px(AFTER)))],
                override_only: false,
            })
            .tier;
        assert_eq!(tier, ContentDirtyTier::Relayout, "premise: a height is a pass");
    };
    let (fresh, fresh_child) = rows_window(AFTER);

    // The mutant, run here: every moved run copied and shifted.
    set_moved_run_policy_on_this_thread(Some(MovedRunPolicy::AlwaysTranslate));
    let translated_before = moved_runs_translated_on_this_thread();
    let (mut shifted, shifted_child) = rows_window(BEFORE);
    grow(&mut shifted, shifted_child);
    let translated = moved_runs_translated_on_this_thread() - translated_before;
    let shifted_is_the_built_list =
        same_bits(&shifted, shifted_child, &fresh, fresh_child).is_ok();
    set_moved_run_policy_on_this_thread(None);
    assert!(
        translated >= 40,
        "premise: the pass patches its list and meets the forty rows it moved ({translated} \
         runs were translated; with none, this pass builds its list in full and the test \
         needs a pass that patches: a resize, as `audit_tests::diff_first` has)"
    );
    assert!(
        !shifted_is_the_built_list,
        "premise: a list whose moved runs are shifted is not the built one on this page \
         (if it is, these moves add exactly and the page tells nothing)"
    );

    // The pass as it is: a moved box is painted anew.
    set_moved_run_policy_on_this_thread(Some(MovedRunPolicy::TweenFramesTranslate));
    let (mut lw, child) = rows_window(BEFORE);
    grow(&mut lw, child);
    let painted = same_bits(&lw, child, &fresh, fresh_child);
    set_moved_run_policy_on_this_thread(None);
    if let Err(difference) = painted {
        panic!("after the pass: {difference}");
    }
}

/// A label dragged by its `left` / `top` to places whose sums do not come out even in an
/// `f32` (59.8, 255.8, 61.8: the positions of the verifier's report), in two parts.
///
/// (a) While it is dragged, each write is a move where the label lies (`old + delta`): every
/// box and item stands within a few units in the last place of where a fresh layout of the
/// moved DOM puts it - at most [`ULPS_A_MOVE`] per move made since the last pass, counted in
/// the float's own steps, not in pixels - and what the window keeps agrees with itself bit
/// for bit: the solver's positions with the result's, the label's frame with the label's box.
///
/// (b) After the next pass of the document (another label gets another width: no move),
/// every box and item is BIT FOR BIT what a fresh layout that saw the same writes holds. This
/// is the guard that matters: the drift of a drag must not survive a pass. It stands on what
/// the move drops - the box's `AbsPlacement` (the pass solves the box from its insets) and
/// the solver's kept list (the pass paints every item from the solved positions; a pass that
/// kept the list would copy the run of a box it finds where the move left it). It does not
/// need the patched build to paint a moved box (the diff-first owner's fix of
/// `try_copy_cached_run`): there is no kept list to copy from.
/// Mutations: the `AbsPlacement` not dropped by the move (the solver's check panics in a
/// test build; without it (b) names the label's box); `cached_display_list` given the moved
/// list back in `move_node_in_place` ((b) names an item of the label an ulp off, for the
/// places where the label's solved position is the moved one and its items are not);
/// `+ delta` -> `- delta` ((a) names the box).
#[test]
fn a_label_written_to_places_that_do_not_add_exactly_stands_bit_for_bit() {
    use crate::overlay::ContentDirtyTier;

    /// What one move can add to the distance from a fresh layout, in units in the last
    /// place: the rounding of the delta, the rounding of `old + delta`, and for an item
    /// the rounding its own `position + k` already carried, each up to half a step of the
    /// larger operand (two steps of a sum in the binade below).
    const ULPS_A_MOVE: u64 = 4;

    // Another label gets another width: a write that is no move, so a pass of the document.
    let widen = |lw: &mut LayoutWindow, dom: DomId| {
        let tier = lw
            .apply_content_change(crate::overlay::ContentChange::NodeStyle {
                dom_id: dom,
                node_id: NodeId::new(1),
                style: azul_css::css::Css::parse_inline(
                    "position: absolute; left: 10px; top: 10px; width: 64px; height: 12px; \
                     font-size: 12px; line-height: 1; white-space: pre;",
                ),
            })
            .tier;
        assert_eq!(tier, ContentDirtyTier::Relayout, "premise: a width is a pass");
    };

    let (mut lw, child) = turned_label_window(40, 150.0, 10.0, 12.0);
    let label_box = lw.layout_results[&child].layout_tree.dom_to_layout[&NodeId::new(DRAGGED)][0]
        .index();
    let places = [(59.8, 255.8), (61.8, 255.9), (63.7, 257.3), (64.1, 258.6), (61.8, 255.8)];
    let (mut since_a_pass, mut moves) = (0_u64, 0_usize);
    for (left, top) in places {
        match write_label(&mut lw, child, left, top, 12.0) {
            ContentDirtyTier::Paint => {
                since_a_pass += 1;
                moves += 1;
            }
            _ => since_a_pass = 0,
        }
        // (a) within the bound of a fresh layout...
        let (fresh, fresh_child) = turned_label_window(40, left, top, 12.0);
        if let Err(difference) =
            same_within(&lw, child, &fresh, fresh_child, ULPS_A_MOVE * since_a_pass)
        {
            panic!("({left}, {top}), {since_a_pass} moves since a pass: {difference}");
        }
        // ...and in agreement with itself, bit for bit.
        let result = &lw.layout_results[&child];
        let bits = |p: LogicalPosition| (p.x.to_bits(), p.y.to_bits());
        for (i, (kept, shown)) in lw.child_layout_caches[&child]
            .calculated_positions
            .iter()
            .zip(&result.calculated_positions)
            .enumerate()
        {
            assert_eq!(bits(*kept), bits(*shown), "({left}, {top}): the solver's box {i}");
        }
        let frame = result
            .display_list
            .items
            .iter()
            .enumerate()
            .find_map(|(i, item)| match item {
                DisplayListItem::PushReferenceFrame { bounds, .. }
                    if result.display_list.node_mapping.get(i).copied().flatten()
                        == Some(NodeId::new(DRAGGED)) =>
                {
                    Some(bounds.0.origin)
                }
                _ => None,
            })
            .expect("the label opens a reference frame");
        assert_eq!(
            bits(frame),
            bits(result.calculated_positions[label_box]),
            "({left}, {top}): the label's frame is at the label's box"
        );
    }
    assert!(moves >= 3, "premise: the writes of the drag are moves where the label lies ({moves})");

    // (b) the next pass ends the drift.
    widen(&mut lw, child);
    let (last_left, last_top) = places[places.len() - 1];
    let (mut fresh, fresh_child) = turned_label_window(40, last_left, last_top, 12.0);
    widen(&mut fresh, fresh_child);
    if let Err(difference) = same_bits(&lw, child, &fresh, fresh_child) {
        panic!("after the pass that follows the drag: {difference}");
    }
}

/// The data of the view of [`float_box_page`]: where the dragged box stands, and how wide the
/// box beside it is.
struct FloatBoxPage {
    left: f32,
    top: f32,
    beside: f32,
}

/// The style of the dragged box of [`float_box_page`]: placed by `left` / `top`, with a frame
/// of its own (a transform).
fn dragged_box_css(left: f32, top: f32) -> String {
    format!(
        "position: absolute; left: {left}px; top: {top}px; width: 120px; height: 60px; \
         transform-origin: 0px 0px; transform: rotate(5deg);"
    )
}

/// A box that is dragged and holds a FLOAT with a block of inline content beside it. Child
/// DOM: div 0; 1 an absolutely placed box beside; 2 the dragged box; 3 its float; 4 a block;
/// 5 the inline box in that block.
extern "C" fn float_box_page(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let (left, top, beside) = data
        .downcast_ref::<FloatBoxPage>()
        .map(|page| (page.left, page.top, page.beside))
        .expect("the view's data");
    let dragged = Dom::create_div()
        .with_css(&dragged_box_css(left, top))
        .with_child(
            Dom::create_div()
                .with_css("float: left; width: 40px; height: 30px; background: #ff0000;"),
        )
        .with_child(Dom::create_div().with_child(Dom::create_div().with_css(
            "display: inline-block; width: 30px; height: 10px; background: #0000ff;",
        )));
    let layer = Dom::create_div()
        .with_child(Dom::create_div().with_css(&format!(
            "position: absolute; left: 10px; top: 10px; width: {beside}px; height: 12px;"
        )))
        .with_child(dragged);
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 400.0));
    VirtualViewReturn::with_dom(layer, window, window)
}

/// A window whose view holds [`float_box_page`], laid out; the view's document.
fn float_box_window(left: f32, top: f32, beside: f32) -> (LayoutWindow, DomId) {
    let data = FloatBoxPage { left, top, beside };
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(RefAny::new(data), VirtualViewCallback::create(float_box_page))
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

/// The float contexts the solver keeps for `dom` (`LayoutCache::float_cache`), as text, by
/// the layout box of their block formatting context.
fn kept_floats(lw: &LayoutWindow, dom: DomId) -> BTreeMap<usize, String> {
    lw.child_layout_caches[&dom]
        .float_cache
        .iter()
        .map(|(bfc, floats)| (*bfc, format!("{floats:?}")))
        .collect()
}

/// A move in place and the solver's kept FLOATS (`LayoutCache::float_cache`). A float
/// context is kept per block formatting context, its floats in that context's own space
/// (`layout_bfc`: the rects it writes into the flow's relative positions). The box a move
/// in place moves is absolutely positioned or fixed: it is no float of any context (CSS 2.2
/// 9.7; `layout_bfc` places it before it asks for `float`) and nothing flows around it; the
/// floats INSIDE it stand relative to their context, which moved with them. So a move leaves
/// every kept context what a fresh layout of the moved DOM keeps, and nothing is dropped for
/// it. Here: a dragged box that holds a float is moved where it lies; its boxes, its list
/// and the kept float contexts are those of a fresh layout, also after a later pass.
/// Mutations: the boxes below the moved one not shifted in `move_node_in_place` (the float
/// stays behind); a float context kept in the window's space (it would differ from the fresh
/// one by the drag's (31.5, 26.25)).
#[test]
fn a_dragged_box_that_holds_a_float_stands_as_a_fresh_layout_puts_it() {
    use crate::overlay::{ContentChange, ContentDirtyTier};

    let near = |a: f32, b: f32| (a - b).abs() <= 0.01;
    let same_boxes = |got: &LayoutWindow, dom: DomId, want: &LayoutWindow, want_dom: DomId, when: &str| {
        let got = &got.layout_results[&dom].calculated_positions;
        let want = &want.layout_results[&want_dom].calculated_positions;
        assert_eq!(got.len(), want.len(), "{when}: the boxes of the document");
        for (i, (a, b)) in got.iter().zip(want).enumerate() {
            assert!(
                near(a.x, b.x) && near(a.y, b.y),
                "{when}: box {i} stands at {a:?}, a fresh layout puts it at {b:?}"
            );
        }
    };
    let write = |lw: &mut LayoutWindow, dom: DomId, left: f32, top: f32| {
        lw.apply_content_change(ContentChange::NodeStyle {
            dom_id: dom,
            node_id: NodeId::new(2),
            style: azul_css::css::Css::parse_inline(&dragged_box_css(left, top)),
        })
        .tier
    };

    let (mut lw, child) = float_box_window(150.0, 40.0, 60.0);
    drop(write(&mut lw, child, 175.0, 61.0));
    drop(write(&mut lw, child, 178.0, 63.0));
    assert_eq!(
        write(&mut lw, child, 181.5, 66.25),
        ContentDirtyTier::Paint,
        "premise: the third move is made where the box lies"
    );

    let (fresh, fresh_child) = float_box_window(181.5, 66.25, 60.0);
    let floats = kept_floats(&fresh, fresh_child);
    assert!(
        floats.values().any(|context| context.contains("Left")),
        "harness: the solver keeps the context of the left float: {floats:?}"
    );
    same_boxes(&lw, child, &fresh, fresh_child, "after the moves");
    assert_eq!(
        kept_floats(&lw, child),
        floats,
        "the kept float contexts are those of a fresh layout of the moved box"
    );
    if let Err(difference) = same_lists(
        &lw.layout_results[&child].display_list,
        &fresh.layout_results[&fresh_child].display_list,
    ) {
        panic!("the moved list against the list of a fresh layout: {difference}");
    }

    // A pass over the document afterwards (the box beside gets another width) starts from
    // the moved boxes and from the kept float contexts.
    let tier = lw
        .apply_content_change(ContentChange::NodeStyle {
            dom_id: child,
            node_id: NodeId::new(1),
            style: azul_css::css::Css::parse_inline(
                "position: absolute; left: 10px; top: 10px; width: 64px; height: 12px;",
            ),
        })
        .tier;
    assert_eq!(tier, ContentDirtyTier::Relayout, "premise: a width is a pass");
    let (fresh, fresh_child) = float_box_window(181.5, 66.25, 64.0);
    same_boxes(&lw, child, &fresh, fresh_child, "after a pass");
    assert_eq!(
        kept_floats(&lw, child),
        kept_floats(&fresh, fresh_child),
        "after a pass the kept float contexts are those of a fresh layout"
    );
}

/// A turn alone (the rotate tool: `transform` is all that differs) is a value under the key
/// the list binds: no pass, no list build, the list the allocation it was, the matrix the one
/// a fresh layout of the turned DOM publishes.
/// Mutation: `refresh_transform_value_of` not called in `move_node_in_place` (the value stays
/// the one of 12 degrees).
#[test]
fn a_label_turned_where_it_lies_is_a_value_and_no_list() {
    let (mut lw, child) = turned_label_window(40, 150.0, 10.0, 12.0);
    drop(write_label(&mut lw, child, 150.0, 10.0, 20.0));
    let list = Arc::as_ptr(&lw.layout_results[&child].display_list);
    lw.frame_report.reset_counters();
    assert_eq!(
        write_label(&mut lw, child, 150.0, 10.0, 31.0),
        crate::overlay::ContentDirtyTier::Paint
    );
    assert_eq!(lw.frame_report.layout_passes, 0, "no layout pass");
    assert_eq!(lw.frame_report.dl_rebuilds, 0, "no list build");
    assert!(
        core::ptr::eq(list, Arc::as_ptr(&lw.layout_results[&child].display_list)),
        "the list is the allocation it was"
    );
    let (fresh, fresh_child) = turned_label_window(40, 150.0, 10.0, 31.0);
    let value = |window: &LayoutWindow, dom: DomId| {
        window
            .gpu_state_manager
            .caches
            .get(&dom)
            .and_then(|cache| cache.css_current_transform_values.get(&NodeId::new(DRAGGED)).copied())
            .expect("the label has a transform value")
    };
    assert_eq!(value(&lw, child), value(&fresh, fresh_child), "rotate(31deg) about (0, 9)");
}

// -- A3-solver-55 (the engine half): clearing an override the node's own style states too ------

/// `set_css_property(transform)` pins its value as an override and keeps the node's own
/// style in sync with it. Clearing that override afterwards (a widget handing the value back
/// when its gesture ends: `override_css_property(node, initial)`) changes nothing any reader
/// resolves, so it takes the entry out and rebuilds no table of the cascade - it was a rebuild
/// of the tables of the whole DOM per cleared box. Nothing is left to migrate onto the next
/// rebuild's node (`migrate_user_overrides_from`), which then stands where its own style
/// puts it.
/// Mutation: the fast path of `apply_node_css_change` removed (the clear runs
/// `restyle_user_properties`: one compact-cache build, tier not `Unchanged`).
#[test]
fn clearing_an_override_the_own_style_states_too_rebuilds_no_cascade() {
    use azul_css::props::{
        basic::PixelValue,
        property::{CssProperty, CssPropertyType},
        style::{StyleTransform, StyleTransformTranslate2D},
    };

    use crate::overlay::{ContentChange, ContentDirtyTier};

    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div().with_css("width: 40px; height: 20px; transform: translate(10px, 0px);"),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let (root, node) = (DomId::ROOT_ID, NodeId::new(1));
    let moved = CssProperty::transform(
        vec![StyleTransform::Translate(StyleTransformTranslate2D {
            x: PixelValue::px(30.0),
            y: PixelValue::px(0.0),
        })]
        .into(),
    );
    let pinned = |lw: &LayoutWindow| {
        lw.layout_results[&root]
            .styled_dom
            .get_css_property_cache()
            .user_overridden_properties
            .get(node.index())
            .is_some_and(|entries| {
                entries.iter().any(|(ty, _)| *ty == CssPropertyType::Transform)
            })
    };
    let published = |lw: &LayoutWindow| {
        lw.gpu_state_manager
            .caches
            .get(&root)
            .and_then(|cache| cache.css_current_transform_values.get(&node).copied())
            .expect("the box has a transform value")
    };

    // The gesture: the widget writes the transform (inline style and override).
    drop(lw.apply_content_change(ContentChange::NodeCss {
        dom_id: root,
        node_id: node,
        props: vec![moved],
        override_only: false,
    }));
    assert!(pinned(&lw), "harness: the write pinned its value");
    let shown = published(&lw);

    // The gesture ends: the override is cleared.
    let tables = azul_core::styled_dom::compact_cache_rebuilds_on_this_thread();
    let tier = lw
        .apply_content_change(ContentChange::NodeCss {
            dom_id: root,
            node_id: node,
            props: vec![CssProperty::initial(CssPropertyType::Transform)],
            override_only: true,
        })
        .tier;
    assert_eq!(tier, ContentDirtyTier::Unchanged, "nothing any reader resolves changed");
    assert_eq!(
        azul_core::styled_dom::compact_cache_rebuilds_on_this_thread(),
        tables,
        "no table of the cascade was built for the clear"
    );
    assert!(!pinned(&lw), "the override is gone: a rebuild has none to carry over");
    assert_eq!(published(&lw), shown, "the box stays where the gesture left it");
}

// -- A3-window-16: a patched frame of a fade writes the window's list where it lies -------------

/// A text or border colour fade patches the colour into the window's list on every frame
/// (`patch_paint_colors`; a border's here). The solver's cache holds that very list (each patched frame hands
/// it the patched one), so `Arc::make_mut` copied the whole list per frame; the cache now
/// lets go of it for the patch. With nobody else holding the list, a patched frame leaves it
/// the allocation it was.
/// Mutation: the release of `cached.5` before the patches removed (every patched frame is
/// another allocation).
#[test]
fn a_patched_frame_of_a_colour_fade_copies_no_list() {
    use azul_css::props::{
        basic::color::ColorU,
        property::{CssProperty, CssPropertyType},
        style::StyleBorderTopColor,
    };

    use crate::overlay::ContentChange;

    // A border colour, not a text colour: this window has no font (an empty font cache), so
    // a text paints no glyph run, a fade of its colour finds no item to patch and every
    // frame of it builds the list - the fixture never reached the patch it is about.
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div().with_css(
            "width: 80px; height: 24px; border: 2px solid #000000; animation: all 300ms linear;",
        ),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let (root, node) = (DomId::ROOT_ID, NodeId::new(1));
    drop(lw.apply_content_change(ContentChange::NodeCss {
        dom_id: root,
        node_id: node,
        props: vec![CssProperty::border_top_color(StyleBorderTopColor {
            inner: ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
        })],
        override_only: false,
    }));
    assert!(
        lw.css_transitions
            .iter()
            .any(|t| t.node == node && t.prop_type == CssPropertyType::BorderTopColor),
        "harness: the colour write seeds a fade"
    );
    assert!(
        lw.layout_cache
            .cached_display_list
            .as_ref()
            .is_some_and(|cached| Arc::ptr_eq(&cached.5, &lw.layout_results[&root].display_list)),
        "harness: the solver's cache holds the window's list (what a patched frame copied)"
    );

    // Frame 1 as the shells run it (it may build the list); then the fade's frames.
    let _ = lw.tick_animations(0.016);
    if !lw.take_transition_patched() && !lw.animation_tick_is_values_only() {
        lw.regenerate_display_list_for_dom(root);
    }
    let mut patched_frames = 0;
    for frame in 2..14 {
        if lw.css_transitions.is_empty() {
            break;
        }
        let before = Arc::as_ptr(&lw.layout_results[&root].display_list);
        let _ = lw.tick_animations(0.016);
        if lw.take_transition_patched() {
            patched_frames += 1;
            assert!(
                core::ptr::eq(before, Arc::as_ptr(&lw.layout_results[&root].display_list)),
                "frame {frame}: the list is patched where it lies"
            );
        } else if !lw.animation_tick_is_values_only() {
            lw.regenerate_display_list_for_dom(root);
        }
    }
    assert!(patched_frames >= 3, "harness: the fade's frames are patches ({patched_frames})");
}

// -- A3-window-15: a decoration written from a callback is painted into the kept list ----------

/// `set_css_property(background)` on one label of a view: its background run is painted into
/// the list the view has, as a hover's is, and the write is a repaint - not a build of the
/// view's whole list per written property. What the list then holds is what a build gives.
/// Mutation: the patch arm of `apply_node_css_change` removed (one list build, tier
/// `RebuildDisplayList`).
#[test]
fn a_decoration_written_from_a_callback_is_painted_into_the_list() {
    use azul_css::props::{
        basic::color::ColorU, property::CssProperty, style::StyleBackgroundContent,
    };

    use crate::overlay::{ContentChange, ContentDirtyTier};

    let (mut lw, child) = label_window(40);
    lw.frame_report.reset_counters();
    let marked = CssProperty::background_content(
        vec![StyleBackgroundContent::Color(ColorU {
            r: 255,
            g: 200,
            b: 0,
            a: 255,
        })]
        .into(),
    );
    // Label 3 (3 % 3 == 0: no background of its own): its span is node 7.
    let tier = lw
        .apply_content_change(ContentChange::NodeCss {
            dom_id: child,
            node_id: NodeId::new(7),
            props: vec![marked],
            override_only: false,
        })
        .tier;
    assert_eq!(tier, ContentDirtyTier::Paint, "a decoration is a repaint");
    assert_eq!(lw.frame_report.dl_rebuilds, 0, "no list was built");
    let patched = painted_lines(&lw, child);
    lw.regenerate_display_list_for_dom(child);
    // As the renderers paint both (a rect bound to a colour key in the key's value).
    assert_eq!(patched, painted_lines(&lw, child), "what a build gives");
}
