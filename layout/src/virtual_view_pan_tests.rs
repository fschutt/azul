//! A pan of a `VirtualView` is a scroll offset, not a change of the host's display list.
//!
//! The map editor pans its map view - a `VirtualView` that drew the tiles and a margin
//! around the view - by moving the view's materialized window
//! (`CallbackInfo::update_virtual_view`) for every mouse move while the picture stays
//! inside the margin, and by rendering the view again once it leaves it. With the style
//! sidebar open the window's list holds thousands of items, and the pan became
//! "insanely slower". The call writes the view's offset into the manager and the scroll
//! manager - and into the host list's `VirtualView` item (the CPU raster, its damage diff
//! and the hit tester's placements read it there), through `Arc::make_mut`: a copy of
//! every item of the host's list (the layout cache and the backends hold it too), and a
//! new list for the renderer to translate and send - for a field the GPU never
//! translates (WebRender scrolls the child's pipeline by the live offset).
//!
//! Now the item is written IN PLACE when nothing but the layout cache's share and the
//! renderer's record holds the list, and the record is renewed for the same list: the
//! renderer still holds it, nothing is copied, translated or sent. A list somebody else
//! holds (the CPU backend's last frame) is copied before it is written, so the diff sees
//! the view move, and a write that finds nothing to change copies nothing.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
    FastBTreeSet,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::{DisplayList, DisplayListItem},
    window::LayoutWindow,
    window_state::FullWindowState,
};

/// The sidebar's rows: two display-list items each (a fill and a border), so the
/// host's list holds well over ten thousand.
const ROWS: usize = 6_000;

/// How often the view's callback ran.
struct Invocations(Arc<AtomicUsize>);

/// The map: 600 px of stripes behind a 300 px box (the margin a pan shows).
extern "C" fn stripes(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    if let Some(counter) = data.downcast_ref::<Invocations>() {
        counter.0.fetch_add(1, Ordering::SeqCst);
    }
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; width: 600px; height: 300px;");
    for i in 0..15 {
        row = row.with_child(Dom::create_div().with_css(if i % 2 == 0 {
            "width: 40px; height: 300px; background: #ff0000;"
        } else {
            "width: 40px; height: 300px; background: #0000ff;"
        }));
    }
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 300.0));
    VirtualViewReturn::with_dom(row, window, window)
}

/// A sidebar of thousands of rows, and the map beside it.
fn host(invocations: &Arc<AtomicUsize>) -> Dom {
    let mut sidebar = Dom::create_div().with_css("width: 200px; height: 320px; overflow: hidden;");
    for i in 0..ROWS {
        sidebar = sidebar.with_child(Dom::create_div().with_css(if i % 2 == 0 {
            "height: 3px; background: #dde6f5; border-bottom: 1px solid #888888;"
        } else {
            "height: 3px; background: #f5e6dd; border-bottom: 1px solid #888888;"
        }));
    }
    Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(sidebar)
        .with_child(
            Dom::create_virtual_view(
                RefAny::new(Invocations(Arc::clone(invocations))),
                VirtualViewCallback::create(stripes),
            )
            .with_css("position: absolute; left: 200px; top: 0px; width: 300px; height: 300px;"),
        )
}

struct Pan {
    lw: LayoutWindow,
    view: NodeId,
    child: DomId,
    invocations: Arc<AtomicUsize>,
}

fn pan_window() -> Pan {
    let invocations = Arc::new(AtomicUsize::new(0));
    let mut dom = host(&invocations);
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 320.0);
    lw.current_window_state = ws.clone();
    lw.layout_new_generation(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let (host_dom, view) = *lw
        .virtual_view_manager
        .all_view_keys()
        .first()
        .expect("the page holds a view");
    assert_eq!(host_dom, DomId::ROOT_ID);
    let child = lw
        .virtual_view_manager
        .get_nested_dom_id(host_dom, view)
        .expect("the view's child DOM");
    let items = lw.layout_results[&DomId::ROOT_ID].display_list.items.len();
    assert!(items >= 10_000, "harness: the host's list holds {items} items");
    Pan {
        lw,
        view,
        child,
        invocations,
    }
}

/// What `CallbackInfo::update_virtual_view(view, Some(window at shift), None)` does in the
/// shell, and the map editor's pan: the picture `shift` px along.
fn pan_to(pan: &mut Pan, shift: f32) {
    pan.lw.apply_virtual_view_geometry(
        DomId::ROOT_ID,
        pan.view,
        Some(LogicalRect::new(
            LogicalPosition::new(shift, 0.0),
            LogicalSize::new(600.0, 300.0),
        )),
        None,
    );
}

/// The renderer receives the lists: what a GPU frame records after it sent them.
fn the_renderer_holds_the_lists(pan: &mut Pan) {
    pan.lw
        .gpu_state_manager
        .note_display_lists_submitted(&pan.lw.layout_results, 0);
}

/// The host's list as a holder of it has it.
fn host_list(pan: &Pan) -> Arc<DisplayList> {
    Arc::clone(&pan.lw.layout_results[&DomId::ROOT_ID].display_list)
}

/// The address of the host's list: its identity, without holding it.
fn host_address(pan: &Pan) -> *const DisplayList {
    Arc::as_ptr(&pan.lw.layout_results[&DomId::ROOT_ID].display_list)
}

/// Where the view's content is NOW, the live value a GPU frame reads.
fn live_offset(pan: &Pan) -> LogicalPosition {
    pan.lw.virtual_view_content_offset(DomId::ROOT_ID, pan.view)
}

/// The `content_offset` a list carries for the view: what the CPU raster, its diff and the
/// hit tester's placements read.
fn offset_in(list: &DisplayList, child: DomId) -> LogicalPosition {
    list.items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::VirtualView {
                child_dom_id,
                content_offset,
                ..
            } if *child_dom_id == child => Some(*content_offset),
            _ => None,
        })
        .expect("the list holds the view")
}

fn item_offset(pan: &Pan) -> LogicalPosition {
    offset_in(&pan.lw.layout_results[&DomId::ROOT_ID].display_list, pan.child)
}

#[test]
fn a_pan_inside_the_margin_leaves_the_list_the_renderer_holds_where_it_is() {
    let mut pan = pan_window();
    the_renderer_holds_the_lists(&mut pan);
    assert!(
        pan.lw.gpu_state_manager.display_list_is_held(
            DomId::ROOT_ID,
            &pan.lw.layout_results[&DomId::ROOT_ID].display_list,
            0
        ),
        "premise: the renderer holds the host's list"
    );
    let host_before = host_address(&pan);
    let child_before = Arc::as_ptr(&pan.lw.layout_results[&pan.child].display_list);
    let calls = pan.invocations.load(Ordering::SeqCst);
    pan.lw.frame_report.reset_counters();

    for step in 1..=50 {
        pan_to(&mut pan, -(step as f32));
    }

    assert_eq!(
        host_address(&pan),
        host_before,
        "fifty moves of the view and the host's list is the same list: nothing was copied"
    );
    assert!(
        pan.lw.gpu_state_manager.display_list_is_held(
            DomId::ROOT_ID,
            &pan.lw.layout_results[&DomId::ROOT_ID].display_list,
            0
        ),
        "the renderer still holds it: there is no new list to translate and send"
    );
    assert!(
        !pan.lw
            .gpu_state_manager
            .display_lists_unsubmitted(&pan.lw.layout_results),
        "no list is owed to the renderer"
    );
    assert_eq!(
        Arc::as_ptr(&pan.lw.layout_results[&pan.child].display_list),
        child_before,
        "and the view's own list is untouched"
    );
    assert_eq!(pan.lw.frame_report.dl_rebuilds, 0, "no display list was built");
    assert_eq!(pan.lw.frame_report.layout_passes, 0, "no layout ran");
    assert_eq!(
        pan.invocations.load(Ordering::SeqCst),
        calls,
        "the view's callback did not run"
    );
    // The item is where the CPU raster, the diff and the hit tester read the view, and the
    // live offset is what a GPU frame scrolls the child's pipeline by.
    let live = live_offset(&pan);
    assert_ne!(live, LogicalPosition::zero(), "the pan is in the live offset");
    assert_eq!(item_offset(&pan), live);
    assert_eq!(
        pan.lw.virtual_view_frame_scroll_offsets(),
        vec![(pan.child, LogicalPosition::new(-live.x, -live.y))],
        "the child's pipeline is scrolled by it"
    );
}

#[test]
fn the_layout_caches_share_and_the_renderers_record_follow_the_write() {
    let mut pan = pan_window();
    the_renderer_holds_the_lists(&mut pan);
    pan_to(&mut pan, -10.0);
    let host_now = host_address(&pan);
    if let Some(cached) = pan.lw.layout_cache.cached_display_list.as_ref() {
        assert_eq!(
            Arc::as_ptr(&cached.5),
            host_now,
            "the layout cache holds the list it held: a layout pass that hits the cache is \
             handed the list the window has"
        );
    }
    let record = pan
        .lw
        .gpu_state_manager
        .submitted_display_lists
        .get(&DomId::ROOT_ID)
        .expect("the renderer's record");
    assert_eq!(record.as_ptr(), host_now, "the record is of the list as written");
}

#[test]
fn a_list_somebody_else_holds_is_copied_and_not_written_under_them() {
    let mut pan = pan_window();
    // The CPU backend's last frame: its damage diff compares it with the next one.
    let last_frame = host_list(&pan);
    let shown_then = offset_in(&last_frame, pan.child);
    pan.lw.frame_report.reset_counters();
    pan_to(&mut pan, -10.0);
    assert!(
        !Arc::ptr_eq(&last_frame, &host_list(&pan)),
        "the window's list is a new one"
    );
    assert_eq!(
        offset_in(&last_frame, pan.child),
        shown_then,
        "the last frame still has the view where it was: the diff can see it move"
    );
    assert_ne!(item_offset(&pan), shown_then, "and the window's list has it moved");
    assert_eq!(item_offset(&pan), live_offset(&pan));
    assert_eq!(pan.lw.frame_report.dl_rebuilds, 0, "no list was built for it");
}

/// The GPU path names a display list by its allocation and keeps a WEAK reference for that
/// (the fonts and images of a list, its text shadows, the digest of the bound values): the
/// reference holds the address, it does not own the list and reads nothing. While one lived
/// the write was a copy (`Arc::get_mut` refuses a list with a weak reference), the copy was a
/// list the renderer did not hold, and every pan frame of a GPU window translated the host's
/// list again (the dll suite's `a_pan_inside_the_margin_is_a_scroll_offset_..`: 10 lists in
/// ten frames). A list this window alone OWNS is written in place whatever names it.
/// Mutation: `Arc::strong_count(..) == 1` back to `Arc::get_mut(..)` moves the list.
#[test]
fn a_list_that_is_only_named_by_weak_references_is_written_in_place() {
    let mut pan = pan_window();
    the_renderer_holds_the_lists(&mut pan);
    let named = Arc::downgrade(&host_list(&pan));
    let before = host_address(&pan);
    pan.lw.frame_report.reset_counters();
    pan_to(&mut pan, -10.0);
    assert_eq!(host_address(&pan), before, "the window's list is where it was");
    assert_eq!(named.as_ptr(), before, "and the name still names it");
    assert_eq!(item_offset(&pan), live_offset(&pan), "the item moved");
    assert!(
        !pan.lw
            .gpu_state_manager
            .display_lists_unsubmitted(&pan.lw.layout_results),
        "no list is owed to the renderer"
    );
    assert_eq!(pan.lw.frame_report.dl_rebuilds, 0, "no list was built for it");
}

#[test]
fn a_write_that_finds_nothing_to_change_copies_nothing() {
    let mut pan = pan_window();
    pan_to(&mut pan, -10.0);
    // The list as somebody else holds it: the CPU backend's last frame.
    let held = host_list(&pan);
    assert!(
        !pan.lw.patch_virtual_view_content_offset(DomId::ROOT_ID, pan.view),
        "the item already shows the offset"
    );
    assert!(
        Arc::ptr_eq(&held, &host_list(&pan)),
        "nothing to write, so nothing is copied: `Arc::make_mut` of a shared list copied every \
         item of it and made the renderer's copy stale"
    );
}

#[test]
fn leaving_the_margin_runs_the_callback_once_and_leaves_the_hosts_list_alone() {
    let mut pan = pan_window();
    the_renderer_holds_the_lists(&mut pan);
    let host_before = host_address(&pan);
    for step in 1..=30 {
        pan_to(&mut pan, -(step as f32) * 7.0);
    }
    let calls = pan.invocations.load(Ordering::SeqCst);

    // The picture is out of margin: the app asks for the view again, and the frame's
    // drain runs the callback.
    let mut updates = BTreeMap::new();
    let mut views = FastBTreeSet::new();
    views.insert(pan.view);
    updates.insert(DomId::ROOT_ID, views);
    pan.lw.queue_virtual_view_updates(updates);
    let window_state = pan.lw.current_window_state.clone();
    let updated = pan.lw.process_pending_virtual_view_updates(
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
    );

    assert_eq!(updated.len(), 1, "the view was rendered again");
    assert_eq!(
        pan.invocations.load(Ordering::SeqCst),
        calls + 1,
        "its callback ran once"
    );
    assert_eq!(
        host_address(&pan),
        host_before,
        "and the host's list is the list the renderer holds: only the view's own list is new"
    );
    assert!(
        pan.lw.gpu_state_manager.display_list_is_held(
            DomId::ROOT_ID,
            &pan.lw.layout_results[&DomId::ROOT_ID].display_list,
            0
        ),
        "the renderer still holds the host's list"
    );
}
