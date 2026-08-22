//! `<transient-window>`, engine side, end to end: an open popup in a real
//! layout pass produces a laid-out content dom the backend can put on a
//! surface, a closed one produces nothing, and toggling `open` across
//! rebuilds opens, keeps, and closes the window without re-creating it.
//!
//! This is the stop-point for step 2 of
//! `scripts/TRANSIENT_WINDOW_PLAN_2026_08_22.md`, driven through the headless
//! window so it exercises `regenerate_layout` exactly as every backend does —
//! nothing here is a unit-level shortcut past the real pass.
//!
//! No surface is created headless (there is nothing to draw on), which is the
//! point: the engine's half is testable on CI without a display, and the
//! per-backend half is then only "make a surface this big, here".

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::{common::event::PlatformWindow, headless::HeadlessWindow};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::{Dom, DomId, NodeData, NodeType},
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::AppConfig,
    transient::TransientWindowConfig,
};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

/// App state: whether the popup is open. The layout callback reads it, so
/// flipping it and re-laying out is exactly what an app's click handler does.
#[derive(Default)]
struct PickerState {
    open: bool,
}

/// A swatch with a popup anchored to it. The popup's content is a sized box
/// holding a bare text node — deliberately not a `<p>`, whose 1em top margin
/// would collapse through the box and shift it down 16px (correct CSS, and
/// the popup would rightly be 176 tall), which only muddies the number below.
extern "C" fn picker_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let open = data.downcast_ref::<PickerState>().is_some_and(|s| s.open);
    let popup = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(
        if open { TransientWindowConfig::opened() } else { TransientWindowConfig::closed() },
    )))
    .with_child(
        Dom::create_div()
            .with_css("width: 240px; height: 160px; background: white;".into())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Choose your colour")),
    );
    let swatch = Dom::create_div()
        .with_css("width: 60px; height: 24px; margin: 40px; background: #e66465;".into())
        .with_child(popup);
    Dom::create_body().with_child(swatch)
}

fn make_window(open: bool) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    let app_data = Arc::new(RefCell::new(RefAny::new(PickerState { open })));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());

    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = azul_core::geom::LogicalSize { width: 800.0, height: 600.0 };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = picker_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);

    HeadlessWindow::new(
        options,
        app_data,
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        icon_provider,
        fc_cache,
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

fn set_open(window: &HeadlessWindow, open: bool) {
    let mut app = window.common.app_data.borrow_mut();
    let guard = app.downcast_mut::<PickerState>();
    if let Some(mut s) = guard {
        s.open = open;
    }
}

/// A closed popup produces NO content dom and NO open window. The default
/// state of every `<transient-window>` must cost nothing.
#[test]
fn a_closed_transient_window_opens_nothing() {
    let mut window = make_window(false);
    window.regenerate_layout().expect("layout");

    let lw = window.get_layout_window().expect("layout window");
    assert!(
        lw.transient_windows.open_windows().is_empty(),
        "closed popup must not be tracked as open"
    );
    assert_eq!(
        lw.layout_results.len(),
        1,
        "only the root dom should be laid out; got {:?}",
        lw.layout_results.keys().collect::<Vec<_>>()
    );
}

/// An open popup is laid out as its own dom, anchored to its parent's rect,
/// and sized to its content.
#[test]
fn an_open_transient_window_lays_out_its_content_as_its_own_dom() {
    let mut window = make_window(true);
    window.regenerate_layout().expect("layout");

    let lw = window.get_layout_window().expect("layout window");
    let open = lw.transient_windows.open_windows();
    assert_eq!(open.len(), 1, "exactly one popup must be open");
    let w = &open[0];

    // Its content was laid out under its own dom id...
    assert!(
        lw.layout_results.contains_key(&w.content_dom),
        "the popup's content must have a layout result under {:?}; have {:?}",
        w.content_dom,
        lw.layout_results.keys().collect::<Vec<_>>()
    );
    assert_ne!(w.content_dom, DomId::ROOT_ID);

    // ...it is anchored to the SWATCH (60x24 at margin 40), not to itself...
    let a = w.placement.anchor_rect;
    assert!(
        (a.size.width - 60.0).abs() < 1.0 && (a.size.height - 24.0).abs() < 1.0,
        "anchor must be the swatch's rect, got {a:?}"
    );

    // ...and the content came out at the size its box asked for.
    assert!(
        (w.content_size.width - 240.0).abs() < 1.0 && (w.content_size.height - 160.0).abs() < 1.0,
        "content-sized popup must take its content's extent, got {:?}",
        w.content_size
    );

    // The diff the backend reads says: one window to CREATE.
    assert_eq!(lw.pending_transient_diff.opened, vec![w.content_dom]);
    assert!(lw.pending_transient_diff.closed.is_empty());
}

/// Flipping `open` across rebuilds opens, KEEPS, then closes the window —
/// and a still-open popup is never re-created in between. That continuity is
/// what separates a popup from a flicker.
#[test]
fn toggling_open_across_rebuilds_opens_keeps_and_closes_without_recreating() {
    let mut window = make_window(false);
    window.regenerate_layout().expect("layout 1: closed");
    assert!(window.get_layout_window().unwrap().transient_windows.open_windows().is_empty());

    // Open it.
    set_open(&window, true);
    window.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    window.regenerate_layout().expect("layout 2: open");
    let id = {
        let lw = window.get_layout_window().unwrap();
        assert_eq!(lw.pending_transient_diff.opened.len(), 1, "it must OPEN");
        lw.transient_windows.open_windows()[0].content_dom
    };

    // Rebuild with it still open: same window, no re-creation.
    window.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    window.regenerate_layout().expect("layout 3: still open");
    {
        let lw = window.get_layout_window().unwrap();
        assert!(
            lw.pending_transient_diff.opened.is_empty() && lw.pending_transient_diff.closed.is_empty(),
            "a rebuild with the popup still open must neither open nor close it; got {:?}",
            lw.pending_transient_diff
        );
        assert_eq!(
            lw.transient_windows.open_windows()[0].content_dom,
            id,
            "the SAME content dom must survive the rebuild"
        );
    }

    // Close it.
    set_open(&window, false);
    window.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    window.regenerate_layout().expect("layout 4: closed");
    let lw = window.get_layout_window().unwrap();
    assert_eq!(lw.pending_transient_diff.closed, vec![id], "it must CLOSE");
    assert!(lw.transient_windows.open_windows().is_empty());
    assert!(
        !lw.layout_results.contains_key(&id),
        "a closed popup's layout result must be dropped, not leaked"
    );
}
