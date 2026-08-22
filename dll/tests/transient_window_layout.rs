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
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
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

