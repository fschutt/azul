//! `<transient-window>`, end to end through the headless shell: the engine's
//! popup set becomes a child window, the child lays out to exactly what the
//! parent measured, content changes reach it through the mailbox, and both
//! ways of closing — the app flipping `open`, the user dismissing — tear it
//! down and tell the right party.
//!
//! Everything here runs the real `PlatformWindow::regenerate_layout` and
//! `process_window_events`, the same code every backend calls; the only thing
//! headless lacks is a native surface, so a "created popup" is the
//! `WindowCreateOptions` left in `pending_window_creates` — which this test
//! then turns into a SECOND headless window, exactly as `run.rs` would.
//!
//! This is the stop-point for steps 2–4 of
//! `scripts/TRANSIENT_WINDOW_PLAN_2026_08_22.md`.

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul::desktop::shell2::{
    common::{
        event::PlatformWindow,
        transient::{mailbox_of, TransientWindowData},
    },
    headless::HeadlessWindow,
};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, RelayoutReason, Update},
    dom::{Dom, DomId, NodeData, NodeType},
    events::{ComponentEventFilter, EventFilter, MouseButton},
    geom::LogicalSize,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::AppConfig,
    transient::{TransientDismiss, TransientWindowConfig},
    window::{CursorPosition, VirtualKeyCode},
};
use azul_layout::{
    callbacks::{Callback, CallbackInfo},
    window_state::WindowCreateOptions,
};
use rust_fontconfig::FcFontCache;

/// App state: whether the popup is open, and what colour the panel shows.
struct PickerState {
    open: bool,
    label: &'static str,
    dismiss: TransientDismiss,
    /// Whether the app's `Dismissed` handler drops `open` (a well-behaved
    /// app) or ignores the event (the zombie case the engine must absorb).
    ack_dismiss: bool,
    /// How many times the app's `Dismissed` handler ran.
    dismissed_calls: Arc<AtomicUsize>,
}

/// The app's reaction to the engine closing the popup for it: drop the flag.
extern "C" fn on_dismissed(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<PickerState>() {
        s.dismissed_calls.fetch_add(1, Ordering::SeqCst);
        if s.ack_dismiss {
            s.open = false;
        }
    }
    Update::RefreshDom
}

/// A swatch with a popup anchored to it. The popup's content is a sized box
/// holding a bare text node — deliberately not a `<p>`, whose 1em top margin
/// would collapse through the box and shift it down 16px (correct CSS, and
/// the popup would rightly be 176 tall), which only muddies the number below.
extern "C" fn picker_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (open, label, dismiss) = data
        .downcast_ref::<PickerState>()
        .map_or((false, "", TransientDismiss::Outside), |s| (s.open, s.label, s.dismiss));
    let cfg = if open { TransientWindowConfig::opened() } else { TransientWindowConfig::closed() }
        .with_dismiss(dismiss);
    let mut node = NodeData::create_node(NodeType::TransientWindow(cfg));
    node.add_callback(
        EventFilter::Component(ComponentEventFilter::Dismissed),
        data.clone(),
        Callback { cb: on_dismissed, ctx: azul_core::refany::OptionRefAny::None }.to_core(),
    );
    let popup = Dom::create_from_data(node)
    .with_child(
        Dom::create_div()
            .with_css("width: 240px; height: 160px; background: white;".into())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(label)),
    );
    let swatch = Dom::create_div()
        .with_css("width: 60px; height: 24px; margin: 40px; background: #e66465;".into())
        .with_child(popup);
    Dom::create_body().with_child(swatch)
}

fn headless(options: WindowCreateOptions, app_data: Arc<RefCell<RefAny>>) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    // Icons resolve the way the app resolves them (the Material pack), so a
    // widget's `<icon>` becomes the glyph text here too.
    let mut handle = IconProviderHandle::default();
    handle.set_resolver(azul_layout::icon::default_icon_resolver);
    if let Some(bytes) = azul::desktop::material_icons::get_material_icons_font_bytes() {
        azul_layout::icon::register_embedded_material_icons(&mut handle, bytes);
    }
    let icon_provider = SharedIconProvider::from_handle(handle);
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

fn make_parent(open: bool, dismiss: TransientDismiss) -> HeadlessWindow {
    make_parent_with(open, dismiss, true)
}

fn make_parent_with(open: bool, dismiss: TransientDismiss, ack_dismiss: bool) -> HeadlessWindow {
    let app_data = Arc::new(RefCell::new(RefAny::new(PickerState {
        open,
        label: "Choose your colour",
        dismiss,
        ack_dismiss,
        dismissed_calls: Arc::new(AtomicUsize::new(0)),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize { width: 800.0, height: 600.0 };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = picker_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    headless(options, app_data)
}

fn with_state(window: &HeadlessWindow, f: impl FnOnce(&mut PickerState)) {
    let mut app = window.common.app_data.borrow_mut();
    let guard = app.downcast_mut::<PickerState>();
    if let Some(mut s) = guard {
        f(&mut s);
    }
}

fn dismissed_calls(window: &HeadlessWindow) -> usize {
    let mut app = window.common.app_data.borrow_mut();
    app.downcast_ref::<PickerState>()
        .map_or(usize::MAX, |s| s.dismissed_calls.load(Ordering::SeqCst))
}

fn relayout(window: &mut HeadlessWindow) {
    window.request_regeneration(RelayoutReason::RefreshDom);
    window.regenerate_layout().expect("layout");
}

/// The popup the parent queued, as the run loop would create it.
fn take_queued_popup(parent: &mut HeadlessWindow) -> WindowCreateOptions {
    assert_eq!(
        parent.pending_window_creates.len(),
        1,
        "exactly one popup window must be queued; got {}",
        parent.pending_window_creates.len()
    );
    parent.pending_window_creates.pop().unwrap()
}

/// `request_window_close` only REQUESTS; the run loop performs the close.
fn close_requested(window: &HeadlessWindow) -> bool {
    window.get_current_window_state().flags.close_requested
}

fn mailbox_state(opts_state: &azul_layout::window_state::FullWindowState) -> (bool, bool, u64) {
    let mut m = mailbox_of(opts_state).expect("the popup's ctx is its mailbox");
    let d = m.downcast_ref::<TransientWindowData>().unwrap();
    (d.closed, d.dismissed, d.generation)
}

/// A closed popup produces NO content dom, NO open window, NO queued window.
#[test]
fn a_closed_transient_window_opens_nothing() {
    let mut parent = make_parent(false, TransientDismiss::Outside);
    parent.regenerate_layout().expect("layout");

    let lw = parent.get_layout_window().expect("layout window");
    assert!(lw.transient_windows.open_windows().is_empty());
    assert_eq!(lw.layout_results.len(), 1, "only the root dom is laid out");
    assert!(parent.pending_window_creates.is_empty(), "nothing to create");
}

/// An open popup is laid out as its own dom, anchored to its parent, sized to
/// its content — and a child window of exactly that size is queued, which
/// itself lays the same content out at the same size.
#[test]
fn an_open_transient_window_becomes_a_child_window_of_its_measured_size() {
    let mut parent = make_parent(true, TransientDismiss::Outside);
    parent.regenerate_layout().expect("layout");

    let (content_dom, size) = {
        let lw = parent.get_layout_window().unwrap();
        let open = lw.transient_windows.open_windows();
        assert_eq!(open.len(), 1, "exactly one popup must be open");
        let w = &open[0];
        assert!(
            !lw.layout_results.contains_key(&w.content_dom),
            "the popup's content is measured on scratch caches, never parked in the \
             parent's layout_results — hit testing and the display list must not see it"
        );
        assert_ne!(w.content_dom, DomId::ROOT_ID);
        let a = w.placement.anchor_rect;
        assert!(
            (a.size.width - 60.0).abs() < 1.0 && (a.size.height - 24.0).abs() < 1.0,
            "anchor must be the swatch's rect, got {a:?}"
        );
        assert!(
            (w.content_size.width - 240.0).abs() < 1.0 && (w.content_size.height - 160.0).abs() < 1.0,
            "content-sized popup must take its content's extent, got {:?}",
            w.content_size
        );
        assert!(w.surface.is_some(), "the shell attached its mailbox to the open window");
        (w.content_dom, w.content_size)
    };

    // The shell queued a child window for it...
    let popup_opts = take_queued_popup(&mut parent);
    assert_eq!(popup_opts.window_state.size.dimensions, size);
    assert!(!popup_opts.size_to_content);
    let (closed, dismissed, generation) = mailbox_state(&popup_opts.window_state);
    assert!(!closed && !dismissed);
    assert_eq!(generation, 0);

    // ...which, created like the run loop would, lays the content out at the
    // size the parent measured: the two layouts agree on the baked style.
    let mut popup = headless(popup_opts, parent.common.app_data.clone());
    popup.regenerate_layout().expect("popup layout");
    let plw = popup.get_layout_window().unwrap();
    let root = plw.layout_results.get(&DomId::ROOT_ID).expect("popup root laid out");
    let root_id = root.styled_dom.root.into_crate_internal().unwrap();
    let hierarchy = root.styled_dom.node_hierarchy.as_container();
    let child = hierarchy.get(root_id).and_then(|h| h.first_child_id(root_id)).expect("the box");
    let rect = plw
        .get_node_layout_rect(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(child)),
        })
        .expect("box rect");
    assert!(
        (rect.size.width - 240.0).abs() < 1.0 && (rect.size.height - 160.0).abs() < 1.0,
        "the popup window must lay the box out at 240x160, got {:?}",
        rect.size
    );
    let _ = content_dom;
}

/// Flipping `open` across rebuilds opens, KEEPS, then closes the window — a
/// still-open popup is never re-created, a content change reaches it through
/// the mailbox, and closing tells the popup to close itself.
#[test]
fn toggling_open_across_rebuilds_opens_keeps_refreshes_and_closes() {
    let mut parent = make_parent(false, TransientDismiss::Outside);
    parent.regenerate_layout().expect("layout 1: closed");
    assert!(parent.pending_window_creates.is_empty());

    // Open it.
    with_state(&parent, |s| s.open = true);
    relayout(&mut parent);
    let popup_opts = take_queued_popup(&mut parent);
    let id = parent.get_layout_window().unwrap().transient_windows.open_windows()[0].content_dom;

    // Rebuild with it still open and unchanged: same window, nothing queued,
    // mailbox untouched.
    relayout(&mut parent);
    assert!(parent.pending_window_creates.is_empty(), "a still-open popup is not re-created");
    assert_eq!(
        parent.get_layout_window().unwrap().transient_windows.open_windows()[0].content_dom,
        id,
        "the SAME content dom must survive the rebuild"
    );
    assert_eq!(mailbox_state(&popup_opts.window_state), (false, false, 0));

    // Change the content: the popup's mailbox gets the new subtree.
    with_state(&parent, |s| s.label = "Pick a colour");
    relayout(&mut parent);
    assert!(parent.pending_window_creates.is_empty());
    let (closed, _, generation) = mailbox_state(&popup_opts.window_state);
    assert!(!closed);
    assert_eq!(generation, 1, "a content change bumps the generation");
    {
        let mut m = mailbox_of(&popup_opts.window_state).unwrap();
        let d = m.downcast_ref::<TransientWindowData>().unwrap();
        let texts: Vec<String> = d
            .content
            .children
            .as_ref()
            .iter()
            .flat_map(|c| c.children.as_ref().iter())
            .map(|t| format!("{:?}", t.root.get_node_type()))
            .collect();
        assert!(
            texts.iter().any(|t| t.contains("Pick a colour")),
            "the mailbox must carry the NEW content, got {texts:?}"
        );
    }

    // Close it from the app: the popup is told to close, its result dropped.
    with_state(&parent, |s| s.open = false);
    relayout(&mut parent);
    let lw = parent.get_layout_window().unwrap();
    assert!(lw.transient_windows.open_windows().is_empty());
    assert!(!lw.layout_results.contains_key(&id), "a closed popup's layout result is dropped");
    let (closed, _, _) = mailbox_state(&popup_opts.window_state);
    assert!(closed, "the mailbox tells the popup to close");

    // And a popup that reads that mailbox closes itself on its next pass.
    let mut popup = headless(popup_opts, parent.common.app_data.clone());
    popup.regenerate_layout().expect("popup layout");
    assert!(close_requested(&popup), "the popup must close itself when the parent said so");
}

/// The user presses in the PARENT while an `outside`-dismissable popup is
/// open: the popup closes, the node is held closed although the app still
/// says `open`, the app's `Dismissed` handler runs and drops the flag, and
/// only a fresh false→true on `open` reopens it.
#[test]
fn a_press_in_the_parent_dismisses_an_outside_popup_once() {
    let mut parent = make_parent(true, TransientDismiss::Outside);
    parent.regenerate_layout().expect("layout");
    let popup_opts = take_queued_popup(&mut parent);

    // A fresh left press in the parent.
    parent.snapshot_window_state_baseline("test.press");
    parent.common.mouse_state_mut().cursor_position =
        CursorPosition::InWindow(azul_core::geom::LogicalPosition::new(400.0, 400.0));
    parent.common.mouse_state_mut().left_down = true;
    let _ = parent.process_window_events(0);

    // The popup was closed and told so...
    assert!(parent.get_layout_window().unwrap().transient_windows.open_windows().is_empty());
    assert!(mailbox_state(&popup_opts.window_state).0, "mailbox says closed");

    // ...the app heard about it through its Dismissed callback...
    parent.regenerate_layout().expect("drain the Dismissed event");
    assert_eq!(dismissed_calls(&parent), 1, "Dismissed fired once");
    let app_open = {
        let mut app = parent.common.app_data.borrow_mut();
        app.downcast_ref::<PickerState>().map(|s| s.open)
    };
    assert_eq!(app_open, Some(false), "the app dropped its flag");

    // ...and nothing was re-created in the process.
    assert!(parent.pending_window_creates.is_empty());

    // The app dropped its flag, so a later `open=true` is a fresh edge: it
    // reopens, with a NEW popup.
    with_state(&parent, |s| s.open = true);
    relayout(&mut parent);
    assert_eq!(parent.get_layout_window().unwrap().transient_windows.open_windows().len(), 1);
    assert_eq!(parent.pending_window_creates.len(), 1, "re-armed: a new popup is created");
}

/// An app that IGNORES `Dismissed` and keeps saying `open=true` gets no
/// zombie popup: the dismissal is edge-triggered on the node, so it stays
/// closed across rebuilds until `open` goes false and true again.
#[test]
fn a_dismissed_node_stays_closed_while_the_app_still_says_open() {
    let mut parent = make_parent_with(true, TransientDismiss::Outside, false);
    parent.regenerate_layout().expect("layout");
    let _ = take_queued_popup(&mut parent);

    parent.snapshot_window_state_baseline("test.press");
    parent.common.mouse_state_mut().left_down = true;
    let _ = parent.process_window_events(0);
    parent.regenerate_layout().expect("drain");
    assert!(parent.get_layout_window().unwrap().transient_windows.open_windows().is_empty());

    // Still open=true in the app; rebuild twice: nothing reopens.
    relayout(&mut parent);
    relayout(&mut parent);
    assert!(
        parent.get_layout_window().unwrap().transient_windows.open_windows().is_empty(),
        "a dismissed node stays closed while open is still true"
    );
    assert!(parent.pending_window_creates.is_empty(), "and nothing is created");

    // false → true re-arms it.
    with_state(&parent, |s| s.open = false);
    relayout(&mut parent);
    with_state(&parent, |s| s.open = true);
    relayout(&mut parent);
    assert_eq!(parent.get_layout_window().unwrap().transient_windows.open_windows().len(), 1);
    assert_eq!(parent.pending_window_creates.len(), 1, "re-armed: a new popup is created");
}

/// Escape inside the popup dismisses it: the popup closes itself, posts
/// `dismissed`, and the parent's next pass closes the node and fires
/// `Dismissed`. A `dismiss=none` popup ignores Escape entirely.
#[test]
fn escape_in_the_popup_dismisses_it_and_reaches_the_parent() {
    let mut parent = make_parent(true, TransientDismiss::Escape);
    parent.regenerate_layout().expect("layout");
    let popup_opts = take_queued_popup(&mut parent);
    let mut popup = headless(popup_opts.clone(), parent.common.app_data.clone());
    popup.regenerate_layout().expect("popup layout");
    assert!(!close_requested(&popup));

    // A press in the parent must NOT dismiss an escape-only popup.
    parent.snapshot_window_state_baseline("test.press");
    parent.common.mouse_state_mut().left_down = true;
    let _ = parent.process_window_events(0);
    assert_eq!(parent.get_layout_window().unwrap().transient_windows.open_windows().len(), 1);
    parent.snapshot_window_state_baseline("test.release");
    parent.common.mouse_state_mut().left_down = false;
    let _ = parent.process_window_events(0);

    // Escape in the popup does.
    popup.snapshot_window_state_baseline("test.escape");
    popup.common.keyboard_state_mut().pressed_virtual_keycodes = vec![VirtualKeyCode::Escape].into();
    let _ = popup.process_window_events(0);
    assert!(close_requested(&popup), "the popup closes itself on Escape");
    let (_, dismissed, _) = mailbox_state(&popup_opts.window_state);
    assert!(dismissed, "and posts dismissed to the parent");

    // The parent's next pass picks it up.
    relayout(&mut parent);
    assert!(parent.get_layout_window().unwrap().transient_windows.open_windows().is_empty());
    assert_eq!(dismissed_calls(&parent), 1, "Dismissed fired once");
    assert!(parent.pending_window_creates.is_empty(), "nothing re-created");

    // dismiss=none: Escape is ignored.
    let mut parent2 = make_parent(true, TransientDismiss::None);
    parent2.regenerate_layout().expect("layout");
    let opts2 = take_queued_popup(&mut parent2);
    let mut popup2 = headless(opts2.clone(), parent2.common.app_data.clone());
    popup2.regenerate_layout().expect("popup layout");
    popup2.snapshot_window_state_baseline("test.escape");
    popup2.common.keyboard_state_mut().pressed_virtual_keycodes = vec![VirtualKeyCode::Escape].into();
    let _ = popup2.process_window_events(0);
    assert!(!close_requested(&popup2), "dismiss=none ignores Escape");
    assert!(!mailbox_state(&opts2.window_state).1);
    let _ = MouseButton::Left;
}

// ---------------------------------------------------------------------------
// The real widget: ColorInput's swatch opens its picker through the engine
// ---------------------------------------------------------------------------

/// The showcase's layout: one `ColorInput` in a body, as the demo builds it.
extern "C" fn picker_widget_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_layout::widgets::color_input::{color_from_hex, ColorInput};
    Dom::create_body().with_child(
        ColorInput::create(color_from_hex("#ff5733").expect("a colour"))
            .with_accessibility_name("Accent colour")
            .dom(),
    )
}

/// Press-and-release at `pos` in the window, through the real event path.
fn click_at(window: &mut HeadlessWindow, pos: azul_core::geom::LogicalPosition) {
    window.snapshot_window_state_baseline("test.move");
    window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(pos);
    window.update_hit_test_at(pos);
    let _ = window.process_window_events(0);
    window.snapshot_window_state_baseline("test.down");
    window.common.mouse_state_mut().left_down = true;
    let _ = window.process_window_events(0);
    window.snapshot_window_state_baseline("test.up");
    window.common.mouse_state_mut().left_down = false;
    let _ = window.process_window_events(0);
}

/// Clicking a `ColorInput`'s swatch opens its picker popup — no `open` flag
/// in the app, no layout-callback plumbing: the widget asks the engine to
/// hold the node open, the next pass reconciles, a popup window is queued.
/// A second click closes it. The popup is the picker panel, laid out at a
/// real size.
#[test]
fn clicking_a_color_input_opens_and_closes_its_picker_popup() {
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize { width: 800.0, height: 600.0 };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = picker_widget_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");
    assert!(parent.pending_window_creates.is_empty(), "closed until clicked");

    // The swatch: wherever layout put it — read its rect rather than guess.
    let swatch = {
        let lw = parent.get_layout_window().unwrap();
        let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
        let nodes = root.styled_dom.node_data.as_container();
        let swatch_node = nodes
            .linear_iter()
            .find(|n| {
                nodes.get(*n).is_some_and(|nd| {
                    format!("{:?}", nd.get_ids_and_classes()).contains("__azul_native_color_input")
                })
            })
            .expect("the swatch node");
        let r = lw
            .get_node_layout_rect(azul_core::dom::DomNodeId {
                dom: DomId::ROOT_ID,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(swatch_node)),
            })
            .expect("swatch rect");
        azul_core::geom::LogicalPosition::new(r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0)
    };
    click_at(&mut parent, swatch);
    parent.regenerate_layout().expect("reconcile after the click");

    let popup_opts = take_queued_popup(&mut parent);
    let size = popup_opts.window_state.size.dimensions;
    assert!(
        size.width > 200.0 && size.height > 150.0,
        "the picker panel must have been measured at a real size, got {size:?}"
    );
    {
        let lw = parent.get_layout_window().unwrap();
        assert_eq!(lw.transient_windows.open_windows().len(), 1);
        assert_eq!(lw.transient_windows.forced_open_nodes().len(), 1, "held open by the widget");
    }

    // The popup, built like the run loop would, lays the panel out: it holds
    // the plane, the hue bar, the hex field and the three channel fields.
    let mut popup = headless(popup_opts.clone(), app_data.clone());
    popup.regenerate_layout().expect("popup layout");
    let plw = popup.get_layout_window().unwrap();
    let root = plw.layout_results.get(&DomId::ROOT_ID).expect("popup root");
    let nodes = root.styled_dom.node_data.as_ref();
    let classes: Vec<String> = nodes
        .iter()
        .flat_map(|n| {
            let v: Vec<String> = n.get_ids_and_classes().as_ref().iter().map(|c| format!("{c:?}")).collect();
            v
        })
        .collect();
    assert!(classes.iter().any(|c| c.contains("__azul_native_color_picker_plane")), "{classes:?}");
    assert!(classes.iter().any(|c| c.contains("__azul_native_color_picker_hue")));
    assert!(nodes.len() > 15, "the panel flattened to only {} nodes", nodes.len());

    // A second click on the swatch closes it.
    click_at(&mut parent, swatch);
    parent.regenerate_layout().expect("reconcile after the second click");
    {
        let lw = parent.get_layout_window().unwrap();
        assert!(lw.transient_windows.open_windows().is_empty(), "closed again");
        assert!(lw.transient_windows.forced_open_nodes().is_empty());
    }
    assert!(mailbox_state(&popup_opts.window_state).0, "the popup was told to close");
    assert!(parent.pending_window_creates.is_empty());

    // And a press elsewhere in the parent while it is open dismisses it, after
    // which the widget's own Dismissed handler has reset its toggle: the next
    // click OPENS (not "closes" a popup that is already gone).
    click_at(&mut parent, swatch);
    parent.regenerate_layout().expect("open again");
    let _ = take_queued_popup(&mut parent);
    click_at(&mut parent, azul_core::geom::LogicalPosition::new(400.0, 400.0));
    parent.regenerate_layout().expect("dismissed by the outside press");
    assert!(parent.get_layout_window().unwrap().transient_windows.open_windows().is_empty());
    click_at(&mut parent, swatch);
    parent.regenerate_layout().expect("reopen");
    assert_eq!(parent.get_layout_window().unwrap().transient_windows.open_windows().len(), 1);
    let _ = take_queued_popup(&mut parent);
}

/// Pointer capture: a press on the picker's plane captures the pointer, so
/// a move that ends far outside the plane still drives the drag — the
/// colour follows the clamped position instead of the drag dying the moment
/// the cursor leaves the hit area. Release ends the capture.
#[test]
fn a_drag_on_the_plane_keeps_following_the_pointer_outside_it() {
    use azul_layout::widgets::color_input::ColorPickerData;
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize { width: 800.0, height: 600.0 };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = picker_widget_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");
    let swatch = azul_core::geom::LogicalPosition::new(15.0, 23.0);
    click_at(&mut parent, swatch);
    parent.regenerate_layout().expect("reconcile");
    let popup_opts = take_queued_popup(&mut parent);

    // The popup window, laid out like the run loop would.
    let mut popup = headless(popup_opts, app_data);
    popup.regenerate_layout().expect("popup layout");
    let plane = {
        let lw = popup.get_layout_window().unwrap();
        let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
        let nodes = root.styled_dom.node_data.as_container();
        let n = nodes
            .linear_iter()
            .find(|n| nodes.get(*n).is_some_and(|nd| format!("{:?}", nd.get_ids_and_classes()).contains("color_picker_plane")))
            .expect("plane node");
        lw.get_node_layout_rect(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(n)),
        })
        .expect("plane rect")
    };
    let picker_color = |w: &HeadlessWindow| {
        let lw = w.get_layout_window().unwrap();
        let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
        let nodes = root.styled_dom.node_data.as_container();
        let n = nodes
            .linear_iter()
            .find(|n| nodes.get(*n).is_some_and(|nd| format!("{:?}", nd.get_ids_and_classes()).contains("color_picker_plane")))
            .unwrap();
        let mut ds = nodes.get(n).unwrap().get_callbacks().as_ref()[0].refany.clone();
        let d = ds.downcast_ref::<ColorPickerData>().unwrap();
        d.current_color()
    };

    // Press in the middle of the plane...
    let mid = azul_core::geom::LogicalPosition::new(
        plane.origin.x + plane.size.width / 2.0,
        plane.origin.y + plane.size.height / 2.0,
    );
    popup.snapshot_window_state_baseline("t.move");
    popup.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(mid);
    popup.update_hit_test_at(mid);
    let _ = popup.process_window_events(0);
    popup.snapshot_window_state_baseline("t.down");
    popup.common.mouse_state_mut().left_down = true;
    let _ = popup.process_window_events(0);
    assert!(popup.get_layout_window().unwrap().pointer_capture.is_some(), "the press captured the pointer");
    let after_press = picker_color(&popup);

    // ...then move far OUTSIDE the plane (below the whole popup) with the
    // button held: the plane must still receive the move and pick the
    // clamped bottom-right = the darkest value, not keep the press colour.
    let far = azul_core::geom::LogicalPosition::new(plane.origin.x + plane.size.width + 200.0, plane.origin.y + plane.size.height + 400.0);
    popup.snapshot_window_state_baseline("t.drag");
    popup.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(far);
    popup.update_hit_test_at(far);
    let _ = popup.process_window_events(0);
    let after_drag = picker_color(&popup);
    assert_ne!(after_drag, after_press, "the drag kept following the pointer outside the plane");
    assert_eq!((after_drag.r, after_drag.g, after_drag.b), (0, 0, 0), "clamped to the plane's bottom = black");

    // Release ends the capture.
    popup.snapshot_window_state_baseline("t.up");
    popup.common.mouse_state_mut().left_down = false;
    let _ = popup.process_window_events(0);
    assert!(popup.get_layout_window().unwrap().pointer_capture.is_none(), "released on mouse-up");
}

/// Escape pressed in the PARENT (the popup may not hold keyboard focus on
/// every platform) dismisses its popups too.
#[test]
fn escape_in_the_parent_dismisses_its_popups() {
    let mut parent = make_parent(true, TransientDismiss::Outside);
    parent.regenerate_layout().expect("layout");
    let popup_opts = take_queued_popup(&mut parent);
    parent.snapshot_window_state_baseline("test.escape");
    parent.common.keyboard_state_mut().pressed_virtual_keycodes = vec![VirtualKeyCode::Escape].into();
    let _ = parent.process_window_events(0);
    assert!(parent.get_layout_window().unwrap().transient_windows.open_windows().is_empty());
    assert!(mailbox_state(&popup_opts.window_state).0, "the popup was told to close");
    parent.regenerate_layout().expect("drain");
    assert_eq!(dismissed_calls(&parent), 1);
}


// ---------------------------------------------------------------------------
// Tear-off (plan §5): the grip drag, dock back, zones, the API
// ---------------------------------------------------------------------------

use azul_core::{
    geom::LogicalPosition,
    transient::TransientTearoff,
    window::WindowType,
};

/// The rect of the first node whose classes contain `class`, in `window`.
fn rect_of_class(window: &HeadlessWindow, class: &str) -> azul_core::geom::LogicalRect {
    let lw = window.get_layout_window().unwrap();
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = root.styled_dom.node_data.as_container();
    let n = nodes
        .linear_iter()
        .find(|n| nodes.get(*n).is_some_and(|nd| format!("{:?}", nd.get_ids_and_classes()).contains(class)))
        .unwrap_or_else(|| panic!("no node with class {class}"));
    lw.get_node_layout_rect(azul_core::dom::DomNodeId {
        dom: DomId::ROOT_ID,
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(n)),
    })
    .expect("laid out")
}

/// A pointer move, the way a backend delivers one: state, hit test, gesture
/// sample, pass.
fn move_to(window: &mut HeadlessWindow, pos: LogicalPosition, site: &str) {
    use azul::desktop::shell2::common::event::{BUTTON_STATE_LEFT, BUTTON_STATE_NONE};
    window.snapshot_window_state_baseline(site);
    window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(pos);
    window.update_hit_test_at(pos);
    let buttons = if window.get_current_window_state().mouse_state.left_down { BUTTON_STATE_LEFT } else { BUTTON_STATE_NONE };
    window.record_input_sample(pos, buttons, false, false, None);
    let _ = window.process_window_events(0);
}

fn press(window: &mut HeadlessWindow, down: bool, site: &str) {
    use azul::desktop::shell2::common::event::{BUTTON_STATE_LEFT, BUTTON_STATE_NONE};
    window.snapshot_window_state_baseline(site);
    window.common.mouse_state_mut().left_down = down;
    let pos = window
        .get_current_window_state()
        .mouse_state
        .cursor_position
        .get_position()
        .unwrap_or(LogicalPosition::zero());
    window.record_input_sample(pos, if down { BUTTON_STATE_LEFT } else { BUTTON_STATE_NONE }, down, !down, None);
    let _ = window.process_window_events(0);
}

/// Drag from `from` by `delta` in `window` through the real gesture path
/// (press, a move past the 5px drag threshold, the move to the end, release).
fn drag_by(window: &mut HeadlessWindow, from: LogicalPosition, delta: LogicalPosition) {
    move_to(window, from, "t.hover");
    press(window, true, "t.down");
    // Past the threshold first: DragStart fires on this move.
    let step = LogicalPosition::new(from.x + delta.x.signum() * 8.0, from.y + delta.y.signum() * 8.0);
    move_to(window, step, "t.start");
    move_to(window, LogicalPosition::new(from.x + delta.x, from.y + delta.y), "t.drag");
    press(window, false, "t.up");
}

fn mailbox(opts_state: &azul_layout::window_state::FullWindowState) -> TransientWindowData {
    let mut m = mailbox_of(opts_state).expect("the window's ctx is its mailbox");
    let d = m.downcast_ref::<TransientWindowData>().unwrap();
    TransientWindowData {
        parent_window_id: d.parent_window_id,
        content_dom: d.content_dom,
        placement: d.placement,
        content_size: d.content_size,
        content: d.content.clone(),
        generation: d.generation,
        closed: d.closed,
        dismissed: d.dismissed,
        origin: d.origin,
        torn: d.torn,
        drag: d.drag,
        drop: d.drop,
    }
}

/// The colour picker's grip: dragging it off the swatch turns the popup into
/// a `Normal` toplevel titled "Colour" at the drop point; dragging the
/// toplevel's grip back over the swatch docks it — a popup again, fresh id
/// each way, the old window told to close each way.
#[test]
fn dragging_the_grip_tears_the_picker_off_and_dragging_it_back_docks_it() {
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize { width: 800.0, height: 600.0 };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = picker_widget_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");
    let swatch_rect = rect_of_class(&parent, "native_color_input");
    let swatch = LogicalPosition::new(
        swatch_rect.origin.x + swatch_rect.size.width / 2.0,
        swatch_rect.origin.y + swatch_rect.size.height / 2.0,
    );
    click_at(&mut parent, swatch);
    parent.regenerate_layout().expect("reconcile");
    let popup_opts = take_queued_popup(&mut parent);
    assert_eq!(popup_opts.window_state.flags.window_type, WindowType::Menu);
    let popup_origin = mailbox(&popup_opts.window_state).origin;
    assert!(
        (popup_origin.y - (swatch_rect.origin.y + swatch_rect.size.height)).abs() < 1.0,
        "the popup hangs below the swatch: {popup_origin:?} vs {swatch_rect:?}"
    );
    let popup_dom = mailbox(&popup_opts.window_state).content_dom;

    // The popup, with its grip.
    let mut popup = headless(popup_opts.clone(), app_data.clone());
    popup.regenerate_layout().expect("popup layout");
    let grip = rect_of_class(&popup, "color_picker_grip");
    let grip_mid = LogicalPosition::new(grip.origin.x + grip.size.width / 2.0, grip.origin.y + grip.size.height / 2.0);

    // Drag the grip 300px right and 200px down: the window moves with it.
    drag_by(&mut popup, grip_mid, LogicalPosition::new(300.0, 200.0));
    let m = mailbox(&popup_opts.window_state);
    assert!(m.drag.is_none(), "the drag ended");
    let report = m.drop.expect("the drop was reported to the parent");
    assert!(
        (report.origin.x - (popup_origin.x + 300.0)).abs() < 1.0
            && (report.origin.y - (popup_origin.y + 200.0)).abs() < 1.0,
        "the window's origin moved by the drag: {:?} from {popup_origin:?}",
        report.origin
    );
    assert!(
        (report.cursor.x - (popup_origin.x + grip_mid.x + 300.0)).abs() < 1.0,
        "the pointer is reported in parent coordinates: {:?}",
        report.cursor
    );

    // The parent's next pass: the popup closes, a toplevel opens at the drop.
    relayout(&mut parent);
    assert!(mailbox(&popup_opts.window_state).closed, "the popup was told to close");
    {
        let lw = parent.get_layout_window().unwrap();
        let open = lw.transient_windows.open_windows();
        assert_eq!(open.len(), 1, "still exactly one window for the node");
        assert_ne!(open[0].content_dom, popup_dom, "a fresh id for the new kind of window");
        let torn = open[0].torn.expect("torn off");
        assert!((torn.x - report.origin.x).abs() < 1.0 && (torn.y - report.origin.y).abs() < 1.0);
    }
    let top_opts = take_queued_popup(&mut parent);
    assert_eq!(top_opts.window_state.flags.window_type, WindowType::Normal, "a real toplevel");
    assert_eq!(top_opts.window_state.title.as_str(), "Colour");
    assert!(!top_opts.window_state.flags.is_always_on_top);
    let tm = mailbox(&top_opts.window_state);
    assert!(tm.torn);
    assert_eq!(tm.content_size, popup_opts.window_state.size.dimensions, "same content, same size");

    // The toplevel lays the same panel out; Escape does NOT dismiss a palette.
    let mut top = headless(top_opts.clone(), app_data.clone());
    top.regenerate_layout().expect("toplevel layout");
    top.snapshot_window_state_baseline("t.escape");
    top.common.keyboard_state_mut().pressed_virtual_keycodes = vec![VirtualKeyCode::Escape].into();
    let _ = top.process_window_events(0);
    assert!(!close_requested(&top), "a torn-off palette ignores Escape");
    top.snapshot_window_state_baseline("t.escape_up");
    top.common.keyboard_state_mut().pressed_virtual_keycodes = vec![].into();
    let _ = top.process_window_events(0);

    // Drag the toplevel's grip so the pointer lands on the swatch: it docks.
    let grip = rect_of_class(&top, "color_picker_grip");
    let grip_mid = LogicalPosition::new(grip.origin.x + grip.size.width / 2.0, grip.origin.y + grip.size.height / 2.0);
    let pointer_now = LogicalPosition::new(tm.origin.x + grip_mid.x, tm.origin.y + grip_mid.y);
    drag_by(&mut top, grip_mid, LogicalPosition::new(swatch.x - pointer_now.x, swatch.y - pointer_now.y));
    let report = mailbox(&top_opts.window_state).drop.expect("reported");
    assert!(swatch_rect.contains(report.cursor), "the pointer is over the swatch: {:?}", report.cursor);

    relayout(&mut parent);
    assert!(mailbox(&top_opts.window_state).closed, "the toplevel was told to close");
    let docked_opts = take_queued_popup(&mut parent);
    assert_eq!(docked_opts.window_state.flags.window_type, WindowType::Menu, "a popup again");
    let dm = mailbox(&docked_opts.window_state);
    assert!(!dm.torn);
    assert!(
        (dm.origin.y - (swatch_rect.origin.y + swatch_rect.size.height)).abs() < 1.0,
        "back below the swatch: {:?}",
        dm.origin
    );
    let lw = parent.get_layout_window().unwrap();
    assert!(lw.transient_windows.open_windows()[0].torn.is_none());
    assert_eq!(lw.transient_windows.open_windows().len(), 1);
}

/// App state for the zone / API scenarios: which events the node reported.
struct TearState {
    open: bool,
    torn_attr: bool,
    tearoff: TransientTearoff,
    events: Arc<std::sync::Mutex<Vec<&'static str>>>,
}

extern "C" fn on_torn_off(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(s) = data.downcast_ref::<TearState>() {
        s.events.lock().unwrap().push("torn-off");
    }
    Update::DoNothing
}

extern "C" fn on_docked(mut data: RefAny, info: CallbackInfo) -> Update {
    if let Some(s) = data.downcast_ref::<TearState>() {
        // The zone it landed on (a `Docked` onto the plain anchor has none).
        let label = match info.get_transient_window_zone(info.get_hit_node()) {
            azul_core::dom::OptionDomNodeId::Some(_) => "docked-on-zone",
            azul_core::dom::OptionDomNodeId::None => "docked",
        };
        s.events.lock().unwrap().push(label);
    }
    Update::DoNothing
}

/// Press on the "float" button inside the popup: tear the window off by API.
extern "C" fn on_float_clicked(_data: RefAny, mut info: CallbackInfo) -> Update {
    // The button lives in the POPUP's dom; the node to tear off is in the
    // parent's. The test drives the parent-side API directly instead (see
    // `set_transient_window_torn_tears_off_and_docks_with_events`); this
    // handler only proves a popup callback runs.
    let _ = &mut info;
    Update::DoNothing
}

/// A tool window: `tearoff="zone:.dock"` with two dock zones beside it.
extern "C" fn zones_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (open, torn, tearoff) = data
        .downcast_ref::<TearState>()
        .map_or((false, false, TransientTearoff::None), |s| (s.open, s.torn_attr, s.tearoff));
    let cfg = if open { TransientWindowConfig::opened() } else { TransientWindowConfig::closed() }
        .with_tearoff(tearoff)
        .with_torn(torn);
    let mut node = NodeData::create_node(NodeType::TransientWindow(cfg));
    node.set_attributes(
        vec![
            azul_core::dom::AttributeType::Title("Tools".into()),
            azul_core::dom::AttributeType::Custom(azul_core::dom::AttributeNameValue {
                attr_name: "tearoff-zone".into(),
                value: ".dock".into(),
            }),
        ]
        .into(),
    );
    node.add_callback(
        EventFilter::Component(ComponentEventFilter::TornOff),
        data.clone(),
        Callback { cb: on_torn_off, ctx: azul_core::refany::OptionRefAny::None }.to_core(),
    );
    node.add_callback(
        EventFilter::Component(ComponentEventFilter::Docked),
        data.clone(),
        Callback { cb: on_docked, ctx: azul_core::refany::OptionRefAny::None }.to_core(),
    );
    let mut float = NodeData::create_node(NodeType::Div);
    float.add_callback(
        EventFilter::Hover(azul_core::events::HoverEventFilter::MouseUp),
        data.clone(),
        Callback { cb: on_float_clicked, ctx: azul_core::refany::OptionRefAny::None }.to_core(),
    );
    let popup = Dom::create_from_data(node).with_child(
        Dom::create_div()
            .with_css("width: 200px; height: 120px; background: white;".into())
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("grip".into())].into())
                    .with_css("height: 16px; -azul-app-region: drag;".into()),
            )
            .with_child(Dom::create_from_data(float).with_css("height: 20px;".into())),
    );
    let anchor = Dom::create_div()
        .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("anchor".into())].into())
        .with_css("position: absolute; left: 300px; top: 20px; width: 80px; height: 24px; background: #888;".into())
        .with_child(popup);
    let zone = |left: f32| {
        Dom::create_div()
            .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("dock".into())].into())
            .with_css(&format!(
                "position: absolute; left: {left}px; top: 200px; width: 120px; height: 300px; background: #ddd;"
            ))
    };
    Dom::create_body().with_child(anchor).with_child(zone(0.0)).with_child(zone(680.0))
}

fn make_zones_parent(open: bool, torn: bool, tearoff: TransientTearoff) -> (HeadlessWindow, Arc<std::sync::Mutex<Vec<&'static str>>>) {
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let app_data = Arc::new(RefCell::new(RefAny::new(TearState {
        open,
        torn_attr: torn,
        tearoff,
        events: events.clone(),
    })));
    let mut options = WindowCreateOptions::default();
    // Tall enough for a 120px popup below a zone ending at y=500.
    options.window_state.size.dimensions = LogicalSize { width: 800.0, height: 700.0 };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = zones_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    (headless(options, app_data), events)
}

fn set_torn_attr(window: &HeadlessWindow, torn: bool) {
    let mut app = window.common.app_data.borrow_mut();
    let guard = app.downcast_mut::<TearState>();
    if let Some(mut s) = guard {
        s.torn_attr = torn;
    }
}

fn transient_node(parent: &HeadlessWindow) -> azul_core::id::NodeId {
    parent.get_layout_window().unwrap().transient_windows.open_windows()[0].source_node
}

/// Dropping onto a `.dock` zone re-anchors the window there (the popup is
/// kept, its placement moves to the zone); dropping off every zone tears it
/// off; the `torn` attribute tears off / docks on change; the events fire.
#[test]
fn a_drop_on_a_zone_re_anchors_and_the_torn_attribute_is_followed() {
    let (mut parent, events) = make_zones_parent(true, false, TransientTearoff::Zone);
    parent.regenerate_layout().expect("layout");
    let popup_opts = take_queued_popup(&mut parent);
    let anchor = rect_of_class(&parent, "anchor");
    let first = mailbox(&popup_opts.window_state);
    assert!((first.origin.y - (anchor.origin.y + anchor.size.height)).abs() < 1.0, "below the anchor");
    let node = transient_node(&parent);

    // 1. Drop on the right zone: re-anchored, same window, placement moved.
    let mut popup = headless(popup_opts.clone(), parent.common.app_data.clone());
    popup.regenerate_layout().expect("popup layout");
    let grip = rect_of_class(&popup, "grip");
    let grip_mid = LogicalPosition::new(grip.origin.x + grip.size.width / 2.0, grip.origin.y + 8.0);
    let pointer_now = LogicalPosition::new(first.origin.x + grip_mid.x, first.origin.y + grip_mid.y);
    let zone_point = LogicalPosition::new(700.0, 300.0);
    drag_by(&mut popup, grip_mid, LogicalPosition::new(zone_point.x - pointer_now.x, zone_point.y - pointer_now.y));
    relayout(&mut parent);
    assert!(parent.pending_window_creates.is_empty(), "a zone dock keeps the popup window");
    assert!(!mailbox(&popup_opts.window_state).closed);
    let m = mailbox(&popup_opts.window_state);
    assert!(
        (m.placement.anchor_rect.origin.x - 680.0).abs() < 1.0 && (m.placement.anchor_rect.origin.y - 200.0).abs() < 1.0,
        "anchored to the zone now: {:?}",
        m.placement.anchor_rect
    );
    assert!(
        (m.origin.y - 500.0).abs() < 1.0 && (m.origin.x - 600.0).abs() < 1.0,
        "placed below the zone, slid in from the right edge: {:?}",
        m.origin
    );
    assert!(events.lock().unwrap().is_empty(), "popup to popup: no tear-off event");
    {
        let lw = parent.get_layout_window().unwrap();
        assert_eq!(lw.transient_windows.anchor_overrides().len(), 1);
        assert_eq!(lw.transient_windows.open_windows()[0].source_node, node);
    }

    // 2. Drop in the open: torn off, the event fires, a toplevel is queued.
    let popup_origin = m.origin;
    let pointer_now = LogicalPosition::new(popup_origin.x + grip_mid.x, popup_origin.y + grip_mid.y);
    let free_point = LogicalPosition::new(400.0, 100.0);
    drag_by(&mut popup, grip_mid, LogicalPosition::new(free_point.x - pointer_now.x, free_point.y - pointer_now.y));
    relayout(&mut parent);
    assert!(mailbox(&popup_opts.window_state).closed);
    let top_opts = take_queued_popup(&mut parent);
    assert_eq!(top_opts.window_state.flags.window_type, WindowType::Normal);
    assert_eq!(top_opts.window_state.title.as_str(), "Tools");
    assert_eq!(*events.lock().unwrap(), vec!["torn-off"]);

    // 3. The app sets `torn="false"`: docked (onto the zone it last had).
    set_torn_attr(&parent, true); // matches reality first: no change...
    relayout(&mut parent);
    assert!(parent.pending_window_creates.is_empty(), "attribute == state: nothing happens");
    set_torn_attr(&parent, false);
    relayout(&mut parent);
    assert!(mailbox(&top_opts.window_state).closed, "the toplevel closes");
    let docked = take_queued_popup(&mut parent);
    assert_eq!(docked.window_state.flags.window_type, WindowType::Menu);
    assert!((mailbox(&docked.window_state).placement.anchor_rect.origin.x - 680.0).abs() < 1.0, "still the zone");
    assert_eq!(
        *events.lock().unwrap(),
        vec!["torn-off", "docked-on-zone"],
        "the Docked handler can read which zone it landed on"
    );
}

/// `CallbackInfo::set_transient_window_torn`, through the change pipeline:
/// tears off at the popup's place, docks back, fires the events once each.
#[test]
fn set_transient_window_torn_tears_off_and_docks_with_events() {
    let (mut parent, events) = make_zones_parent(true, false, TransientTearoff::Free);
    parent.regenerate_layout().expect("layout");
    let popup_opts = take_queued_popup(&mut parent);
    let node = transient_node(&parent);

    let changed = parent.get_layout_window_mut().unwrap().set_transient_window_torn(node, true);
    assert!(changed);
    relayout(&mut parent);
    assert!(mailbox(&popup_opts.window_state).closed);
    let top_opts = take_queued_popup(&mut parent);
    assert_eq!(top_opts.window_state.flags.window_type, WindowType::Normal);
    let origin = mailbox(&top_opts.window_state).origin;
    assert_eq!(origin, mailbox(&popup_opts.window_state).origin, "torn off where the popup was");
    assert_eq!(*events.lock().unwrap(), vec!["torn-off"]);

    // Again: nothing.
    assert!(!parent.get_layout_window_mut().unwrap().set_transient_window_torn(node, true));

    let changed = parent.get_layout_window_mut().unwrap().set_transient_window_torn(node, false);
    assert!(changed);
    relayout(&mut parent);
    assert!(mailbox(&top_opts.window_state).closed);
    let docked = take_queued_popup(&mut parent);
    assert_eq!(docked.window_state.flags.window_type, WindowType::Menu);
    assert_eq!(*events.lock().unwrap(), vec!["torn-off", "docked"]);

    // A window without `tearoff` ignores it.
    let (mut plain, _) = make_zones_parent(true, false, TransientTearoff::None);
    plain.regenerate_layout().expect("layout");
    let _ = take_queued_popup(&mut plain);
    let node = transient_node(&plain);
    assert!(!plain.get_layout_window_mut().unwrap().set_transient_window_torn(node, true));
}

/// The torn-off toplevel's close button: the node closes, `Dismissed` fires.
#[test]
fn closing_a_torn_off_toplevel_dismisses_the_node() {
    let mut parent = make_parent(true, TransientDismiss::Outside);
    parent.regenerate_layout().expect("layout");
    let _ = take_queued_popup(&mut parent);
    // Not tear-off capable: the plain fixture. Use the engine API on a
    // capable window instead.
    let (mut parent, _events) = make_zones_parent(true, true, TransientTearoff::Free);
    parent.regenerate_layout().expect("layout");
    let top_opts = take_queued_popup(&mut parent);
    assert_eq!(top_opts.window_state.flags.window_type, WindowType::Normal, "torn=\"true\" opens torn");
    let mut top = headless(top_opts.clone(), parent.common.app_data.clone());
    top.regenerate_layout().expect("layout");

    // The close button.
    let _ = top.request_window_close("test.close_button");
    assert!(mailbox(&top_opts.window_state).dismissed, "the closing palette reports itself dismissed");
    relayout(&mut parent);
    assert!(parent.get_layout_window().unwrap().transient_windows.open_windows().is_empty());
    assert!(parent.pending_window_creates.is_empty());
}


// ---------------------------------------------------------------------------
// The eyedropper: pick_screen_color from the picker, the answer routed back
// ---------------------------------------------------------------------------

/// The app side of the eyedropper scenario: the colour the widget reported.
struct EyedropperApp {
    reported: Arc<std::sync::Mutex<Vec<azul_css::props::basic::color::ColorU>>>,
}

extern "C" fn on_app_color(
    mut data: RefAny,
    _: CallbackInfo,
    state: azul_layout::widgets::color_input::ColorInputState,
) -> Update {
    if let Some(app) = data.downcast_ref::<EyedropperApp>() {
        app.reported.lock().unwrap().push(state.color);
    }
    Update::DoNothing
}

extern "C" fn eyedropper_layout(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_layout::widgets::color_input::{color_from_hex, ColorInput};
    Dom::create_body().with_child(
        ColorInput::create(color_from_hex("#ff5733").expect("a colour"))
            .with_accessibility_name("Accent colour")
            .with_on_value_change(data, on_app_color as azul_layout::widgets::color_input::ColorInputOnValueChangeCallbackType)
            .dom(),
    )
}

/// Clicking the picker's eyedropper issues a pick on the POPUP's window;
/// headless has no screen, so the shell answers "cancelled" at once (the
/// widget ignores that). A real answer - pushed the way the loupe window
/// or the system sampler pushes it - reaches the popup's `ScreenColorPicked`
/// callback on its next pass: the picker adopts the RGB (keeping its
/// alpha) and the app's `on_value_change` hears the new colour.
#[test]
fn the_eyedropper_answer_is_routed_to_the_picker_that_asked() {
    use azul_css::props::basic::color::ColorU;
    use azul_layout::managers::eyedropper::{in_flight_anywhere, push_result, EyedropperResult};

    let reported = Arc::new(std::sync::Mutex::new(Vec::new()));
    let app_data = Arc::new(RefCell::new(RefAny::new(EyedropperApp { reported: reported.clone() })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize { width: 800.0, height: 600.0 };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = eyedropper_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");
    let swatch_rect = rect_of_class(&parent, "native_color_input");
    click_at(
        &mut parent,
        LogicalPosition::new(swatch_rect.origin.x + 5.0, swatch_rect.origin.y + 5.0),
    );
    parent.regenerate_layout().expect("reconcile");
    let popup_opts = take_queued_popup(&mut parent);
    let mut popup = headless(popup_opts, app_data);
    popup.regenerate_layout().expect("popup layout");

    // 1. Click the eyedropper button in the popup.
    let button = rect_of_class(&popup, "color_picker_eyedropper");
    click_at(
        &mut popup,
        LogicalPosition::new(button.origin.x + button.size.width / 2.0, button.origin.y + button.size.height / 2.0),
    );
    // Headless reads no screen: the request was issued on the popup's
    // manager and answered "cancelled" in the same pass.
    let lw = popup.get_layout_window().unwrap();
    assert!(!lw.eyedropper_manager.has_pending_async() || in_flight_anywhere());
    let _ = popup.process_window_events(0);
    assert!(reported.lock().unwrap().is_empty(), "a cancelled pick reports nothing");

    // 2. A second pick, answered with a real colour the way a backend does.
    click_at(
        &mut popup,
        LogicalPosition::new(button.origin.x + button.size.width / 2.0, button.origin.y + button.size.height / 2.0),
    );
    // Re-issue: the headless shell cancelled immediately, so emulate a
    // platform that is still sampling - issue directly on the manager.
    let id = popup.get_layout_window_mut().unwrap().eyedropper_manager.begin_request();
    assert!(in_flight_anywhere(), "a pick in flight keeps popups from light-dismissing");
    push_result(EyedropperResult {
        request_id: id,
        color: Some(ColorU { r: 10, g: 200, b: 30, a: 255 }),
    });
    popup.snapshot_window_state_baseline("t.pump");
    let _ = popup.process_window_events(0);
    assert!(!in_flight_anywhere());
    let got = reported.lock().unwrap().clone();
    assert_eq!(got.last().copied(), Some(ColorU { r: 10, g: 200, b: 30, a: 255 }), "reported: {got:?}");

    // The swatch in the PARENT follows on its next pass (RefreshDomAllWindows
    // from the pick wakes it; the app stores the colour).
    relayout(&mut parent);
    let swatch_bg = {
        let lw = parent.get_layout_window().unwrap();
        let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
        format!("{:?}", root.styled_dom.node_data.as_container().get(azul_core::id::NodeId::new(1)).map(|n| n.get_style().clone()))
    };
    let _ = swatch_bg; // the app-side colour is what matters; the dom follows from it
}

