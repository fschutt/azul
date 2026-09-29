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
        .map_or((false, "", TransientDismiss::Outside), |s| {
            (s.open, s.label, s.dismiss)
        });
    let cfg = if open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    }
    .with_dismiss(dismiss);
    let mut node = NodeData::create_node(NodeType::TransientWindow(cfg));
    node.add_callback(
        EventFilter::Component(ComponentEventFilter::Dismissed),
        data.clone(),
        Callback {
            cb: on_dismissed,
            ctx: azul_core::refany::OptionRefAny::None,
        }
        .to_core(),
    );
    let popup = Dom::create_from_data(node).with_child(
        Dom::create_div()
            .with_css("width: 240px; height: 160px; background: white;".into())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                label,
            )),
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
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
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
    assert!(
        parent.pending_window_creates.is_empty(),
        "nothing to create"
    );
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
            "the popup's content is measured on scratch caches, never parked in the parent's \
             layout_results — hit testing and the display list must not see it"
        );
        assert_ne!(w.content_dom, DomId::ROOT_ID);
        let a = w.placement.anchor_rect;
        assert!(
            (a.size.width - 60.0).abs() < 1.0 && (a.size.height - 24.0).abs() < 1.0,
            "anchor must be the swatch's rect, got {a:?}"
        );
        assert!(
            (w.content_size.width - 240.0).abs() < 1.0
                && (w.content_size.height - 160.0).abs() < 1.0,
            "content-sized popup must take its content's extent, got {:?}",
            w.content_size
        );
        assert!(
            w.surface.is_some(),
            "the shell attached its mailbox to the open window"
        );
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
    let root = plw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("popup root laid out");
    let root_id = root.styled_dom.root.into_crate_internal().unwrap();
    let hierarchy = root.styled_dom.node_hierarchy.as_container();
    let child = hierarchy
        .get(root_id)
        .and_then(|h| h.first_child_id(root_id))
        .expect("the box");
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
    let id = parent
        .get_layout_window()
        .unwrap()
        .transient_windows
        .open_windows()[0]
        .content_dom;

    // Rebuild with it still open and unchanged: same window, nothing queued,
    // mailbox untouched.
    relayout(&mut parent);
    assert!(
        parent.pending_window_creates.is_empty(),
        "a still-open popup is not re-created"
    );
    assert_eq!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()[0]
            .content_dom,
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
    assert!(
        !lw.layout_results.contains_key(&id),
        "a closed popup's layout result is dropped"
    );
    let (closed, _, _) = mailbox_state(&popup_opts.window_state);
    assert!(closed, "the mailbox tells the popup to close");

    // And a popup that reads that mailbox closes itself on its next pass.
    let mut popup = headless(popup_opts, parent.common.app_data.clone());
    popup.regenerate_layout().expect("popup layout");
    assert!(
        close_requested(&popup),
        "the popup must close itself when the parent said so"
    );
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
    assert!(parent
        .get_layout_window()
        .unwrap()
        .transient_windows
        .open_windows()
        .is_empty());
    assert!(
        mailbox_state(&popup_opts.window_state).0,
        "mailbox says closed"
    );

    // ...the app heard about it through its Dismissed callback...
    parent
        .regenerate_layout()
        .expect("drain the Dismissed event");
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
    assert_eq!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .len(),
        1
    );
    assert_eq!(
        parent.pending_window_creates.len(),
        1,
        "re-armed: a new popup is created"
    );
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
    assert!(parent
        .get_layout_window()
        .unwrap()
        .transient_windows
        .open_windows()
        .is_empty());

    // Still open=true in the app; rebuild twice: nothing reopens.
    relayout(&mut parent);
    relayout(&mut parent);
    assert!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .is_empty(),
        "a dismissed node stays closed while open is still true"
    );
    assert!(
        parent.pending_window_creates.is_empty(),
        "and nothing is created"
    );

    // false → true re-arms it.
    with_state(&parent, |s| s.open = false);
    relayout(&mut parent);
    with_state(&parent, |s| s.open = true);
    relayout(&mut parent);
    assert_eq!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .len(),
        1
    );
    assert_eq!(
        parent.pending_window_creates.len(),
        1,
        "re-armed: a new popup is created"
    );
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
    assert_eq!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .len(),
        1
    );
    parent.snapshot_window_state_baseline("test.release");
    parent.common.mouse_state_mut().left_down = false;
    let _ = parent.process_window_events(0);

    // Escape in the popup does.
    popup.snapshot_window_state_baseline("test.escape");
    popup.common.keyboard_state_mut().pressed_virtual_keycodes =
        vec![VirtualKeyCode::Escape].into();
    let _ = popup.process_window_events(0);
    assert!(close_requested(&popup), "the popup closes itself on Escape");
    let (_, dismissed, _) = mailbox_state(&popup_opts.window_state);
    assert!(dismissed, "and posts dismissed to the parent");

    // The parent's next pass picks it up.
    relayout(&mut parent);
    assert!(parent
        .get_layout_window()
        .unwrap()
        .transient_windows
        .open_windows()
        .is_empty());
    assert_eq!(dismissed_calls(&parent), 1, "Dismissed fired once");
    assert!(
        parent.pending_window_creates.is_empty(),
        "nothing re-created"
    );

    // dismiss=none: Escape is ignored.
    let mut parent2 = make_parent(true, TransientDismiss::None);
    parent2.regenerate_layout().expect("layout");
    let opts2 = take_queued_popup(&mut parent2);
    let mut popup2 = headless(opts2.clone(), parent2.common.app_data.clone());
    popup2.regenerate_layout().expect("popup layout");
    popup2.snapshot_window_state_baseline("test.escape");
    popup2.common.keyboard_state_mut().pressed_virtual_keycodes =
        vec![VirtualKeyCode::Escape].into();
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
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = picker_widget_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");
    assert!(
        parent.pending_window_creates.is_empty(),
        "closed until clicked"
    );

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
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                    swatch_node,
                )),
            })
            .expect("swatch rect");
        azul_core::geom::LogicalPosition::new(
            r.origin.x + r.size.width / 2.0,
            r.origin.y + r.size.height / 2.0,
        )
    };
    click_at(&mut parent, swatch);
    parent
        .regenerate_layout()
        .expect("reconcile after the click");

    let popup_opts = take_queued_popup(&mut parent);
    let size = popup_opts.window_state.size.dimensions;
    assert!(
        size.width > 200.0 && size.height > 150.0,
        "the picker panel must have been measured at a real size, got {size:?}"
    );
    {
        let lw = parent.get_layout_window().unwrap();
        assert_eq!(lw.transient_windows.open_windows().len(), 1);
        assert_eq!(
            lw.transient_windows.forced_open_nodes().len(),
            1,
            "held open by the widget"
        );
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
            let v: Vec<String> = n
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .map(|c| format!("{c:?}"))
                .collect();
            v
        })
        .collect();
    assert!(
        classes
            .iter()
            .any(|c| c.contains("__azul_native_color_picker_plane")),
        "{classes:?}"
    );
    assert!(classes
        .iter()
        .any(|c| c.contains("__azul_native_color_picker_hue")));
    assert!(
        nodes.len() > 15,
        "the panel flattened to only {} nodes",
        nodes.len()
    );

    // A second click on the swatch closes it.
    click_at(&mut parent, swatch);
    parent
        .regenerate_layout()
        .expect("reconcile after the second click");
    {
        let lw = parent.get_layout_window().unwrap();
        assert!(
            lw.transient_windows.open_windows().is_empty(),
            "closed again"
        );
        assert!(lw.transient_windows.forced_open_nodes().is_empty());
    }
    assert!(
        mailbox_state(&popup_opts.window_state).0,
        "the popup was told to close"
    );
    assert!(parent.pending_window_creates.is_empty());

    // And a press elsewhere in the parent while it is open dismisses it, after
    // which the widget's own Dismissed handler has reset its toggle: the next
    // click OPENS (not "closes" a popup that is already gone).
    click_at(&mut parent, swatch);
    parent.regenerate_layout().expect("open again");
    let _ = take_queued_popup(&mut parent);
    click_at(
        &mut parent,
        azul_core::geom::LogicalPosition::new(400.0, 400.0),
    );
    parent
        .regenerate_layout()
        .expect("dismissed by the outside press");
    assert!(parent
        .get_layout_window()
        .unwrap()
        .transient_windows
        .open_windows()
        .is_empty());
    click_at(&mut parent, swatch);
    parent.regenerate_layout().expect("reopen");
    assert_eq!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .len(),
        1
    );
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
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
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
            .find(|n| {
                nodes.get(*n).is_some_and(|nd| {
                    format!("{:?}", nd.get_ids_and_classes()).contains("color_picker_plane")
                })
            })
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
            .find(|n| {
                nodes.get(*n).is_some_and(|nd| {
                    format!("{:?}", nd.get_ids_and_classes()).contains("color_picker_plane")
                })
            })
            .unwrap();
        let mut ds = nodes.get(n).unwrap().get_callbacks().as_ref()[0]
            .refany
            .clone();
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
    assert!(
        popup.get_layout_window().unwrap().pointer_capture.is_some(),
        "the press captured the pointer"
    );
    let after_press = picker_color(&popup);

    // ...then move far OUTSIDE the plane (below the whole popup) with the
    // button held: the plane must still receive the move and pick the
    // clamped bottom-right = the darkest value, not keep the press colour.
    let far = azul_core::geom::LogicalPosition::new(
        plane.origin.x + plane.size.width + 200.0,
        plane.origin.y + plane.size.height + 400.0,
    );
    popup.snapshot_window_state_baseline("t.drag");
    popup.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(far);
    popup.update_hit_test_at(far);
    let _ = popup.process_window_events(0);
    let after_drag = picker_color(&popup);
    assert_ne!(
        after_drag, after_press,
        "the drag kept following the pointer outside the plane"
    );
    assert_eq!(
        (after_drag.r, after_drag.g, after_drag.b),
        (0, 0, 0),
        "clamped to the plane's bottom = black"
    );

    // Release ends the capture.
    popup.snapshot_window_state_baseline("t.up");
    popup.common.mouse_state_mut().left_down = false;
    let _ = popup.process_window_events(0);
    assert!(
        popup.get_layout_window().unwrap().pointer_capture.is_none(),
        "released on mouse-up"
    );
}

/// Escape pressed in the PARENT (the popup may not hold keyboard focus on
/// every platform) dismisses its popups too.
#[test]
fn escape_in_the_parent_dismisses_its_popups() {
    let mut parent = make_parent(true, TransientDismiss::Outside);
    parent.regenerate_layout().expect("layout");
    let popup_opts = take_queued_popup(&mut parent);
    parent.snapshot_window_state_baseline("test.escape");
    parent.common.keyboard_state_mut().pressed_virtual_keycodes =
        vec![VirtualKeyCode::Escape].into();
    let _ = parent.process_window_events(0);
    assert!(parent
        .get_layout_window()
        .unwrap()
        .transient_windows
        .open_windows()
        .is_empty());
    assert!(
        mailbox_state(&popup_opts.window_state).0,
        "the popup was told to close"
    );
    parent.regenerate_layout().expect("drain");
    assert_eq!(dismissed_calls(&parent), 1);
}

// ---------------------------------------------------------------------------
// Tear-off (plan §5): the grip drag, dock back, zones, the API
// ---------------------------------------------------------------------------

use azul_core::{geom::LogicalPosition, transient::TransientTearoff, window::WindowType};

/// The rect of the first node whose classes contain `class`, in `window`.
fn rect_of_class(window: &HeadlessWindow, class: &str) -> azul_core::geom::LogicalRect {
    let lw = window.get_layout_window().unwrap();
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = root.styled_dom.node_data.as_container();
    let n = nodes
        .linear_iter()
        .find(|n| {
            nodes
                .get(*n)
                .is_some_and(|nd| format!("{:?}", nd.get_ids_and_classes()).contains(class))
        })
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
    let buttons = if window.get_current_window_state().mouse_state.left_down {
        BUTTON_STATE_LEFT
    } else {
        BUTTON_STATE_NONE
    };
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
    window.record_input_sample(
        pos,
        if down {
            BUTTON_STATE_LEFT
        } else {
            BUTTON_STATE_NONE
        },
        down,
        !down,
        None,
    );
    let _ = window.process_window_events(0);
}

/// Drag from `from` by `delta` in `window` through the real gesture path
/// (press, a move past the 5px drag threshold, the move to the end, release).
fn drag_by(window: &mut HeadlessWindow, from: LogicalPosition, delta: LogicalPosition) {
    move_to(window, from, "t.hover");
    press(window, true, "t.down");
    // Past the threshold first: DragStart fires on this move.
    let step = LogicalPosition::new(
        from.x + delta.x.signum() * 8.0,
        from.y + delta.y.signum() * 8.0,
    );
    move_to(window, step, "t.start");
    move_to(
        window,
        LogicalPosition::new(from.x + delta.x, from.y + delta.y),
        "t.drag",
    );
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
        following: d.following,
        // master's a11y work added this field without updating the test's
        // field-by-field copy of the mailbox.
        focus_visible: d.focus_visible,
        forwarded_keys: d.forwarded_keys.clone(),
        takes_focus: d.takes_focus,
        autofocused: d.autofocused,
    }
}

/// The colour picker's grip: dragging it off the swatch turns the popup into
/// a frameless (Menu-type) torn panel titled "Colour" at the drop point;
/// dragging its grip back over the swatch docks it — a popup again, fresh id
/// each way, the old window told to close each way.
#[test]
fn dragging_the_grip_tears_the_picker_off_and_dragging_it_back_docks_it() {
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
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
    let grip_mid = LogicalPosition::new(
        grip.origin.x + grip.size.width / 2.0,
        grip.origin.y + grip.size.height / 2.0,
    );

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
    assert!(
        mailbox(&popup_opts.window_state).closed,
        "the popup was told to close"
    );
    {
        let lw = parent.get_layout_window().unwrap();
        let open = lw.transient_windows.open_windows();
        assert_eq!(open.len(), 1, "still exactly one window for the node");
        assert_ne!(
            open[0].content_dom, popup_dom,
            "a fresh id for the new kind of window"
        );
        let torn = open[0].torn.expect("torn off");
        assert!((torn.x - report.origin.x).abs() < 1.0 && (torn.y - report.origin.y).abs() < 1.0);
    }
    let top_opts = take_queued_popup(&mut parent);
    // A torn-off panel is a frameless (Menu-type) drag proxy, not a decorated
    // toplevel — borderless, parent-relative, alpha-capable — but not pinned.
    assert_eq!(
        top_opts.window_state.flags.window_type,
        WindowType::Menu,
        "a frameless torn panel"
    );
    assert_eq!(top_opts.window_state.title.as_str(), "Colour");
    assert!(!top_opts.window_state.flags.is_always_on_top);
    let tm = mailbox(&top_opts.window_state);
    assert!(tm.torn);
    assert_eq!(
        tm.content_size, popup_opts.window_state.size.dimensions,
        "same content, same size"
    );

    // The toplevel lays the same panel out. It is FRAMELESS (no OS close
    // button), so Escape is what closes it — the only affordance it has.
    let mut top = headless(top_opts.clone(), app_data.clone());
    top.regenerate_layout().expect("toplevel layout");
    top.snapshot_window_state_baseline("t.escape");
    top.common.keyboard_state_mut().pressed_virtual_keycodes = vec![VirtualKeyCode::Escape].into();
    let _ = top.process_window_events(0);
    assert!(
        close_requested(&top),
        "a frameless torn-off palette closes on Escape"
    );
    top.snapshot_window_state_baseline("t.escape_up");
    top.common.keyboard_state_mut().pressed_virtual_keycodes = vec![].into();
    let _ = top.process_window_events(0);

    // Drag the toplevel's grip so the pointer lands on the swatch: it docks.
    let grip = rect_of_class(&top, "color_picker_grip");
    let grip_mid = LogicalPosition::new(
        grip.origin.x + grip.size.width / 2.0,
        grip.origin.y + grip.size.height / 2.0,
    );
    let pointer_now = LogicalPosition::new(tm.origin.x + grip_mid.x, tm.origin.y + grip_mid.y);
    drag_by(
        &mut top,
        grip_mid,
        LogicalPosition::new(swatch.x - pointer_now.x, swatch.y - pointer_now.y),
    );
    let report = mailbox(&top_opts.window_state).drop.expect("reported");
    assert!(
        swatch_rect.contains(report.cursor),
        "the pointer is over the swatch: {:?}",
        report.cursor
    );

    relayout(&mut parent);
    assert!(
        mailbox(&top_opts.window_state).closed,
        "the toplevel was told to close"
    );
    let docked_opts = take_queued_popup(&mut parent);
    assert_eq!(
        docked_opts.window_state.flags.window_type,
        WindowType::Menu,
        "a popup again"
    );
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
        .map_or((false, false, TransientTearoff::None), |s| {
            (s.open, s.torn_attr, s.tearoff)
        });
    let cfg = if open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    }
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
        Callback {
            cb: on_torn_off,
            ctx: azul_core::refany::OptionRefAny::None,
        }
        .to_core(),
    );
    node.add_callback(
        EventFilter::Component(ComponentEventFilter::Docked),
        data.clone(),
        Callback {
            cb: on_docked,
            ctx: azul_core::refany::OptionRefAny::None,
        }
        .to_core(),
    );
    let mut float = NodeData::create_node(NodeType::Div);
    float.add_callback(
        EventFilter::Hover(azul_core::events::HoverEventFilter::MouseUp),
        data.clone(),
        Callback {
            cb: on_float_clicked,
            ctx: azul_core::refany::OptionRefAny::None,
        }
        .to_core(),
    );
    let popup = Dom::create_from_data(node).with_child(
        Dom::create_div()
            .with_css("width: 200px; height: 120px; background: white;".into())
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(
                        vec![azul_core::dom::IdOrClass::Class("grip".into())].into(),
                    )
                    .with_css("height: 16px; -azul-app-region: drag;".into()),
            )
            .with_child(Dom::create_from_data(float).with_css("height: 20px;".into())),
    );
    let anchor = Dom::create_div()
        .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("anchor".into())].into())
        .with_css(
            "position: absolute; left: 300px; top: 20px; width: 80px; height: 24px; background: \
             #888;"
                .into(),
        )
        .with_child(popup);
    let zone = |left: f32| {
        Dom::create_div()
            .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("dock".into())].into())
            .with_css(&format!(
                "position: absolute; left: {left}px; top: 200px; width: 120px; height: 300px; \
                 background: #ddd;"
            ))
    };
    Dom::create_body()
        .with_child(anchor)
        .with_child(zone(0.0))
        .with_child(zone(680.0))
}

fn make_zones_parent(
    open: bool,
    torn: bool,
    tearoff: TransientTearoff,
) -> (HeadlessWindow, Arc<std::sync::Mutex<Vec<&'static str>>>) {
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let app_data = Arc::new(RefCell::new(RefAny::new(TearState {
        open,
        torn_attr: torn,
        tearoff,
        events: events.clone(),
    })));
    let mut options = WindowCreateOptions::default();
    // Tall enough for a 120px popup below a zone ending at y=500.
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 700.0,
    };
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
    parent
        .get_layout_window()
        .unwrap()
        .transient_windows
        .open_windows()[0]
        .source_node
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
    assert!(
        (first.origin.y - (anchor.origin.y + anchor.size.height)).abs() < 1.0,
        "below the anchor"
    );
    let node = transient_node(&parent);

    // 1. Drop on the right zone: re-anchored, same window, placement moved.
    let mut popup = headless(popup_opts.clone(), parent.common.app_data.clone());
    popup.regenerate_layout().expect("popup layout");
    let grip = rect_of_class(&popup, "grip");
    let grip_mid = LogicalPosition::new(grip.origin.x + grip.size.width / 2.0, grip.origin.y + 8.0);
    let pointer_now =
        LogicalPosition::new(first.origin.x + grip_mid.x, first.origin.y + grip_mid.y);
    let zone_point = LogicalPosition::new(700.0, 300.0);
    drag_by(
        &mut popup,
        grip_mid,
        LogicalPosition::new(zone_point.x - pointer_now.x, zone_point.y - pointer_now.y),
    );
    relayout(&mut parent);
    assert!(
        parent.pending_window_creates.is_empty(),
        "a zone dock keeps the popup window"
    );
    assert!(!mailbox(&popup_opts.window_state).closed);
    let m = mailbox(&popup_opts.window_state);
    assert!(
        (m.placement.anchor_rect.origin.x - 680.0).abs() < 1.0
            && (m.placement.anchor_rect.origin.y - 200.0).abs() < 1.0,
        "anchored to the zone now: {:?}",
        m.placement.anchor_rect
    );
    assert!(
        (m.origin.y - 500.0).abs() < 1.0 && (m.origin.x - 600.0).abs() < 1.0,
        "placed below the zone, slid in from the right edge: {:?}",
        m.origin
    );
    assert_eq!(
        *events.lock().unwrap(),
        vec!["docked-on-zone"],
        "a drop onto a zone is a dock: the node hears `Docked` with the zone readable"
    );
    {
        let lw = parent.get_layout_window().unwrap();
        assert_eq!(lw.transient_windows.anchor_overrides().len(), 1);
        assert_eq!(lw.transient_windows.open_windows()[0].source_node, node);
    }

    // 2. Drop in the open: torn off, the event fires, a toplevel is queued.
    let popup_origin = m.origin;
    let pointer_now =
        LogicalPosition::new(popup_origin.x + grip_mid.x, popup_origin.y + grip_mid.y);
    let free_point = LogicalPosition::new(400.0, 100.0);
    drag_by(
        &mut popup,
        grip_mid,
        LogicalPosition::new(free_point.x - pointer_now.x, free_point.y - pointer_now.y),
    );
    relayout(&mut parent);
    assert!(mailbox(&popup_opts.window_state).closed);
    let top_opts = take_queued_popup(&mut parent);
    assert_eq!(top_opts.window_state.flags.window_type, WindowType::Menu);
    assert_eq!(top_opts.window_state.title.as_str(), "Tools");
    assert_eq!(*events.lock().unwrap(), vec!["docked-on-zone", "torn-off"]);

    // 3. The app sets `torn="false"`: docked (onto the zone it last had).
    set_torn_attr(&parent, true); // matches reality first: no change...
    relayout(&mut parent);
    assert!(
        parent.pending_window_creates.is_empty(),
        "attribute == state: nothing happens"
    );
    set_torn_attr(&parent, false);
    relayout(&mut parent);
    assert!(
        mailbox(&top_opts.window_state).closed,
        "the toplevel closes"
    );
    let docked = take_queued_popup(&mut parent);
    assert_eq!(docked.window_state.flags.window_type, WindowType::Menu);
    assert!(
        (mailbox(&docked.window_state).placement.anchor_rect.origin.x - 680.0).abs() < 1.0,
        "still the zone"
    );
    assert_eq!(
        *events.lock().unwrap(),
        vec!["docked-on-zone", "torn-off", "docked-on-zone"],
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

    let changed = parent
        .get_layout_window_mut()
        .unwrap()
        .set_transient_window_torn(node, true);
    assert!(changed);
    relayout(&mut parent);
    assert!(mailbox(&popup_opts.window_state).closed);
    let top_opts = take_queued_popup(&mut parent);
    assert_eq!(top_opts.window_state.flags.window_type, WindowType::Menu);
    let origin = mailbox(&top_opts.window_state).origin;
    assert_eq!(
        origin,
        mailbox(&popup_opts.window_state).origin,
        "torn off where the popup was"
    );
    assert_eq!(*events.lock().unwrap(), vec!["torn-off"]);

    // Again: nothing.
    assert!(!parent
        .get_layout_window_mut()
        .unwrap()
        .set_transient_window_torn(node, true));

    let changed = parent
        .get_layout_window_mut()
        .unwrap()
        .set_transient_window_torn(node, false);
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
    assert!(!plain
        .get_layout_window_mut()
        .unwrap()
        .set_transient_window_torn(node, true));
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
    assert_eq!(
        top_opts.window_state.flags.window_type,
        WindowType::Menu,
        "torn=\"true\" opens torn"
    );
    let mut top = headless(top_opts.clone(), parent.common.app_data.clone());
    top.regenerate_layout().expect("layout");

    // The close button.
    let _ = top.request_window_close("test.close_button");
    assert!(
        mailbox(&top_opts.window_state).dismissed,
        "the closing palette reports itself dismissed"
    );
    relayout(&mut parent);
    assert!(parent
        .get_layout_window()
        .unwrap()
        .transient_windows
        .open_windows()
        .is_empty());
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
            .with_on_value_change(
                data,
                on_app_color
                    as azul_layout::widgets::color_input::ColorInputOnValueChangeCallbackType,
            )
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
    let app_data = Arc::new(RefCell::new(RefAny::new(EyedropperApp {
        reported: reported.clone(),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
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
        LogicalPosition::new(
            button.origin.x + button.size.width / 2.0,
            button.origin.y + button.size.height / 2.0,
        ),
    );
    // Headless reads no screen: the request was issued on the popup's
    // manager and answered "cancelled" in the same pass.
    let lw = popup.get_layout_window().unwrap();
    assert!(!lw.eyedropper_manager.has_pending_async() || in_flight_anywhere());
    let _ = popup.process_window_events(0);
    assert!(
        reported.lock().unwrap().is_empty(),
        "a cancelled pick reports nothing"
    );

    // 2. A second pick, answered with a real colour the way a backend does.
    click_at(
        &mut popup,
        LogicalPosition::new(
            button.origin.x + button.size.width / 2.0,
            button.origin.y + button.size.height / 2.0,
        ),
    );
    // Re-issue: the headless shell cancelled immediately, so emulate a
    // platform that is still sampling - issue directly on the manager.
    let id = popup
        .get_layout_window_mut()
        .unwrap()
        .eyedropper_manager
        .begin_request();
    assert!(
        in_flight_anywhere(),
        "a pick in flight keeps popups from light-dismissing"
    );
    push_result(EyedropperResult {
        request_id: id,
        color: Some(ColorU {
            r: 10,
            g: 200,
            b: 30,
            a: 255,
        }),
    });
    popup.snapshot_window_state_baseline("t.pump");
    let _ = popup.process_window_events(0);
    assert!(!in_flight_anywhere());
    let got = reported.lock().unwrap().clone();
    assert_eq!(
        got.last().copied(),
        Some(ColorU {
            r: 10,
            g: 200,
            b: 30,
            a: 255
        }),
        "reported: {got:?}"
    );

    // The swatch in the PARENT follows on its next pass (RefreshDomAllWindows
    // from the pick wakes it; the app stores the colour).
    relayout(&mut parent);
    let swatch_bg = {
        let lw = parent.get_layout_window().unwrap();
        let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
        format!(
            "{:?}",
            root.styled_dom
                .node_data
                .as_container()
                .get(azul_core::id::NodeId::new(1))
                .map(|n| n.get_style().clone())
        )
    };
    let _ = swatch_bg; // the app-side colour is what matters; the dom follows from it
}

// ---------------------------------------------------------------------------
// Transparent + shaped windows: the frame carries alpha, the shape follows it
// ---------------------------------------------------------------------------

/// A popup whose node asks for `material="transparent"`; its panel has
/// rounded corners (12px radius) on a white background.
extern "C" fn rounded_popup_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::window::WindowBackgroundMaterial;
    let open = data.downcast_ref::<PickerState>().is_some_and(|s| s.open);
    let cfg = if open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    }
    .with_material(WindowBackgroundMaterial::Transparent);
    let popup = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(cfg)))
        .with_child(
            Dom::create_div()
                .with_css("width: 120px; height: 80px; background: white; border-radius: 12px;"),
        );
    let anchor = Dom::create_div()
        .with_css("width: 60px; height: 24px; margin: 40px; background: #e66465;")
        .with_child(popup);
    Dom::create_body().with_child(anchor)
}

/// A popup whose node carries a CLIP MASK (the DOM's own mask): that implies
/// a transparent window - the mask is the window's shape.
extern "C" fn masked_popup_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::{
        geom::{LogicalRect, LogicalSize as LS},
        resources::{ImageMask, ImageRef, RawImage, RawImageData, RawImageFormat},
    };
    let open = data.downcast_ref::<PickerState>().is_some_and(|s| s.open);
    let cfg = if open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    };
    let mut node = NodeData::create_node(NodeType::TransientWindow(cfg));
    // A 100x60 mask: opaque on the left half only.
    let mut px = vec![0u8; 100 * 60];
    for y in 0..60 {
        for x in 0..50 {
            px[y * 100 + x] = 255;
        }
    }
    let mask = ImageRef::new_rawimage(RawImage {
        width: 100,
        height: 60,
        pixels: RawImageData::U8(px.into()),
        premultiplied_alpha: false,
        data_format: RawImageFormat::R8,
        tag: Vec::new().into(),
    })
    .expect("mask image");
    node.set_clip_mask(ImageMask {
        image: mask,
        rect: LogicalRect::new(LogicalPosition::zero(), LS::new(100.0, 60.0)),
        repeat: false,
    });
    let popup = Dom::create_from_data(node)
        .with_child(Dom::create_div().with_css("width: 100px; height: 60px; background: white;"));
    let anchor = Dom::create_div()
        .with_css("width: 60px; height: 24px; margin: 40px; background: #e66465;")
        .with_child(popup);
    Dom::create_body().with_child(anchor)
}

fn transparent_parent(cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom) -> HeadlessWindow {
    let app_data = Arc::new(RefCell::new(RefAny::new(PickerState {
        open: true,
        label: "",
        dismiss: TransientDismiss::Outside,
        ack_dismiss: true,
        dismissed_calls: Arc::new(AtomicUsize::new(0)),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 400.0,
        height: 300.0,
    };
    options.window_state.layout_callback = LayoutCallback::create(cb);
    headless(options, app_data)
}

/// The popup window is created with the transparent material; laid out and
/// rendered headless through the shared CPU path, its frame is cleared to
/// alpha 0 where the rounded panel does not paint, and the shape computed
/// from it is NOT one full rectangle (the corners are cut) - with every
/// opaque pixel inside it and every corner pixel outside.
#[test]
fn a_transparent_popup_renders_alpha_and_a_non_rectangular_shape() {
    use azul_core::window::WindowBackgroundMaterial;
    let mut parent = transparent_parent(rounded_popup_layout);
    parent.regenerate_layout().expect("layout");
    let popup_opts = take_queued_popup(&mut parent);
    assert_eq!(
        popup_opts.window_state.flags.background_material,
        WindowBackgroundMaterial::Transparent,
        "material=\"transparent\" on the node reaches the popup's window state"
    );
    let mut popup = headless(popup_opts, parent.common.app_data.clone());
    popup.regenerate_layout().expect("popup layout + frame");

    let frame = popup
        .cpu_backend
        .last_frame
        .as_ref()
        .expect("a frame was rendered");
    let (w, h) = (frame.width(), frame.height());
    assert!(w >= 100 && h >= 60, "{w}x{h}");
    let alpha_at = |x: u32, y: u32| frame.data()[((y * w + x) * 4 + 3) as usize];
    assert_eq!(
        alpha_at(0, 0),
        0,
        "the corner outside the 12px radius is transparent"
    );
    assert_eq!(
        alpha_at(w / 2, h / 2),
        255,
        "the panel's middle is opaque white"
    );

    assert!(popup.cpu_backend.transparent && popup.cpu_backend.shape_from_alpha);
    let shape = popup
        .cpu_backend
        .last_shape
        .clone()
        .expect("a shape was computed");
    assert!(
        shape.len() > 1,
        "rounded corners: more than one rect, got {}",
        shape.len()
    );
    let covers = |x: u32, y: u32| {
        shape
            .iter()
            .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
    };
    assert!(covers(w / 2, h / 2), "the body is inside the shape");
    assert!(!covers(0, 0), "the transparent corner is outside the shape");
    // The first (top) rect starts right of the corner: the corner is cut.
    assert!(
        shape[0].x > 0,
        "top row starts past the corner radius: {:?}",
        shape[0]
    );
    // Applied once; an identical next frame does not re-issue it.
    assert!(popup.cpu_backend.take_changed_shape().is_some());
    assert!(popup.cpu_backend.take_changed_shape().is_none());
}

/// A clip mask on the `<transient-window>` node makes the popup transparent
/// without asking, and the rendered shape is the mask's: the left half.
#[test]
fn a_clip_mask_on_the_node_is_the_popups_shape() {
    use azul_core::window::WindowBackgroundMaterial;
    let mut parent = transparent_parent(masked_popup_layout);
    parent.regenerate_layout().expect("layout");
    let popup_opts = take_queued_popup(&mut parent);
    assert_eq!(
        popup_opts.window_state.flags.background_material,
        WindowBackgroundMaterial::Transparent,
        "a clip mask implies a transparent window"
    );
    let mut popup = headless(popup_opts, parent.common.app_data.clone());
    popup.regenerate_layout().expect("popup layout + frame");
    let frame = popup.cpu_backend.last_frame.as_ref().expect("frame");
    let (w, h) = (frame.width(), frame.height());
    let alpha_at = |x: u32, y: u32| frame.data()[((y * w + x) * 4 + 3) as usize];
    assert_eq!(alpha_at(10, h / 2), 255, "inside the mask: painted");
    assert_eq!(alpha_at(w - 10, h / 2), 0, "outside the mask: nothing");
    let shape = popup.cpu_backend.last_shape.clone().expect("shape");
    assert!(
        shape.iter().all(|r| r.x + r.width <= w / 2 + 1),
        "the shape stops at the mask's edge: {shape:?}"
    );
}

// ---------------------------------------------------------------------------
// dock="inline": a panel that is CONTENT of its zone, torn off and re-docked
// ---------------------------------------------------------------------------

/// A workspace: a home column holding the panel, two dock zones, all three
/// matching `.dock`. The panel is `dock="inline" tearoff="zone:.dock"`.
extern "C" fn workspace_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::dom::{AttributeNameValue, AttributeType, IdOrClass::Class};
    let (open, torn) = data
        .downcast_ref::<TearState>()
        .map_or((true, false), |s| (s.open, s.torn_attr));
    let cfg = if open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    }
    .with_tearoff(TransientTearoff::Zone)
    .with_dock(azul_core::transient::TransientDock::Inline)
    .with_torn(torn);
    let mut node = NodeData::create_node(NodeType::TransientWindow(cfg));
    node.set_attributes(
        vec![
            AttributeType::Title("Tools".into()),
            AttributeType::Custom(AttributeNameValue {
                attr_name: "tearoff-zone".into(),
                value: ".dock".into(),
            }),
        ]
        .into(),
    );
    node.add_callback(
        EventFilter::Component(ComponentEventFilter::TornOff),
        data.clone(),
        Callback {
            cb: on_torn_off,
            ctx: azul_core::refany::OptionRefAny::None,
        }
        .to_core(),
    );
    node.add_callback(
        EventFilter::Component(ComponentEventFilter::Docked),
        data.clone(),
        Callback {
            cb: on_docked,
            ctx: azul_core::refany::OptionRefAny::None,
        }
        .to_core(),
    );
    let panel = Dom::create_from_data(node)
        .with_ids_and_classes(vec![Class("panel".into())].into())
        .with_css("width: 100%; height: 120px; background: #dde;".into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![Class("grip".into())].into())
                .with_css("height: 16px; background: #99a; -azul-app-region: drag;".into()),
        )
        .with_child(Dom::create_div().with_css("height: 100px;".into()));
    let column = |left: f32, class: &str, child: Option<Dom>| {
        let mut d = Dom::create_div()
            .with_ids_and_classes(vec![Class("dock".into()), Class(class.into())].into())
            .with_css(&format!(
                "position: absolute; left: {left}px; top: 20px; width: 200px; height: 500px; \
                 background: #eee;"
            ));
        if let Some(c) = child {
            d = d.with_child(c);
        }
        d
    };
    Dom::create_body()
        .with_child(column(0.0, "home", Some(panel)))
        .with_child(column(300.0, "zone-b", None))
        .with_child(column(600.0, "zone-c", None))
}

fn rect_of(window: &HeadlessWindow, class: &str) -> Option<azul_core::geom::LogicalRect> {
    let lw = window.get_layout_window().unwrap();
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = root.styled_dom.node_data.as_container();
    let n = nodes.linear_iter().find(|n| {
        nodes.get(*n).is_some_and(|nd| {
            format!("{:?}", nd.get_ids_and_classes()).contains(&format!("\"{class}\""))
        })
    })?;
    lw.get_node_layout_rect(azul_core::dom::DomNodeId {
        dom: DomId::ROOT_ID,
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(n)),
    })
}

fn inside(inner: azul_core::geom::LogicalRect, outer: azul_core::geom::LogicalRect) -> bool {
    inner.origin.x >= outer.origin.x - 0.5
        && inner.origin.y >= outer.origin.y - 0.5
        && inner.max_x() <= outer.max_x() + 0.5
        && inner.max_y() <= outer.max_y() + 0.5
}

/// `dock="inline"`: the panel is laid out as content of its column on the
/// very first frame (no popup window); dragging its grip into the open
/// tears it off into a toplevel and removes it from the flow; dragging
/// the toplevel's grip onto zone B docks it INLINE in B (grafted - the
/// app's DOM still has it under the home column); dragging its grip from
/// B to C moves it to C; each dock fires `Docked`.
#[test]
fn an_inline_docked_panel_is_content_of_its_zone_and_moves_between_zones() {
    use azul_core::window::WindowType;
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let app_data = Arc::new(RefCell::new(RefAny::new(TearState {
        open: true,
        torn_attr: false,
        tearoff: TransientTearoff::Zone,
        events: events.clone(),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 900.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = workspace_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");

    // 1. Inline at home, first frame, nothing queued.
    let home = rect_of(&parent, "home").expect("home column");
    let zone_b = rect_of(&parent, "zone-b").expect("zone b");
    let zone_c = rect_of(&parent, "zone-c").expect("zone c");
    let panel = rect_of(&parent, "panel").expect("the panel is laid out on the first frame");
    assert!(
        inside(panel, home),
        "inline in its home column: {panel:?} in {home:?}"
    );
    assert!(
        (panel.size.width - 200.0).abs() < 1.0,
        "fills the column: {panel:?}"
    );
    assert!(
        parent.pending_window_creates.is_empty(),
        "no popup for an inline panel"
    );
    assert_eq!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .len(),
        1
    );
    assert!(parent
        .get_layout_window()
        .unwrap()
        .transient_windows
        .open_windows()[0]
        .is_inline());

    // 2. Drag the grip into the open (between the columns, below them).
    let grip = rect_of(&parent, "grip").expect("grip");
    let grip_mid = LogicalPosition::new(grip.origin.x + 100.0, grip.origin.y + 8.0);
    drag_by(&mut parent, grip_mid, LogicalPosition::new(150.0, 540.0)); // to (250, 568): no zone
                                                                        // there
    assert!(
        parent.get_layout_window().unwrap().inline_tear.is_none(),
        "the drag ended"
    );
    parent
        .regenerate_layout()
        .expect("re-layout after the drop");
    let top_opts = take_queued_popup(&mut parent);
    assert_eq!(
        top_opts.window_state.flags.window_type,
        WindowType::Menu,
        "torn off into a frameless panel"
    );
    assert_eq!(top_opts.window_state.title.as_str(), "Tools");
    assert!(
        rect_of(&parent, "panel").is_none(),
        "torn off: no longer in the parent's flow"
    );
    assert_eq!(*events.lock().unwrap(), vec!["torn-off"]);
    let top_origin = mailbox(&top_opts.window_state).origin;
    assert!(
        (top_origin.x - (home.origin.x + 150.0)).abs() < 1.0
            && (top_origin.y - (panel.origin.y + 540.0)).abs() < 1.0,
        "the toplevel opens where the panel's box was dragged to: {top_origin:?}"
    );

    // 3. The toplevel's grip dragged onto zone B: docked INLINE in B.
    let mut top = headless(top_opts.clone(), app_data.clone());
    top.regenerate_layout().expect("toplevel layout");
    let tgrip = rect_of(&top, "grip").expect("grip in the toplevel");
    let tgrip_mid = LogicalPosition::new(tgrip.origin.x + 100.0, tgrip.origin.y + 8.0);
    let pointer_now = LogicalPosition::new(top_origin.x + tgrip_mid.x, top_origin.y + tgrip_mid.y);
    let b_centre = LogicalPosition::new(zone_b.origin.x + 100.0, zone_b.origin.y + 250.0);
    drag_by(
        &mut top,
        tgrip_mid,
        LogicalPosition::new(b_centre.x - pointer_now.x, b_centre.y - pointer_now.y),
    );
    assert!(
        mailbox(&top_opts.window_state).drop.is_some(),
        "the drop was reported"
    );
    relayout(&mut parent);
    assert!(
        mailbox(&top_opts.window_state).closed,
        "the toplevel closes"
    );
    assert!(
        parent.pending_window_creates.is_empty(),
        "no popup: inline again"
    );
    let panel = rect_of(&parent, "panel").expect("back in the flow");
    assert!(
        inside(panel, zone_b),
        "grafted into zone B: {panel:?} in {zone_b:?}"
    );
    assert!(!inside(panel, home), "and not at home");
    assert_eq!(*events.lock().unwrap(), vec!["torn-off", "docked-on-zone"]);
    {
        let lw = parent.get_layout_window().unwrap();
        let w = &lw.transient_windows.open_windows()[0];
        assert!(w.is_inline() && w.anchor_override.is_some());
    }

    // 3b. Content, not a popup: a press elsewhere in the parent and Escape
    //     dismiss nothing (the demo lost its panel to the first press).
    click_at(&mut parent, LogicalPosition::new(100.0, 400.0));
    parent.snapshot_window_state_baseline("t.escape");
    parent.common.keyboard_state_mut().pressed_virtual_keycodes =
        vec![VirtualKeyCode::Escape].into();
    let _ = parent.process_window_events(0);
    parent.snapshot_window_state_baseline("t.escape_up");
    parent.common.keyboard_state_mut().pressed_virtual_keycodes = vec![].into();
    let _ = parent.process_window_events(0);
    parent.regenerate_layout().expect("re-layout");
    let panel = rect_of(&parent, "panel").expect("an inline panel is not light-dismissed");
    assert!(
        inside(panel, zone_b),
        "still in B after a press + Escape in the parent"
    );

    // 4. From B to C, inline to inline: grafted into C, `Docked` again.
    let grip = rect_of(&parent, "grip").expect("grip in B");
    let grip_mid = LogicalPosition::new(grip.origin.x + 100.0, grip.origin.y + 8.0);
    let c_centre = LogicalPosition::new(zone_c.origin.x + 100.0, zone_c.origin.y + 250.0);
    drag_by(
        &mut parent,
        grip_mid,
        LogicalPosition::new(c_centre.x - grip_mid.x, c_centre.y - grip_mid.y),
    );
    parent
        .regenerate_layout()
        .expect("re-layout after the move");
    let panel = rect_of(&parent, "panel").expect("still inline");
    assert!(
        inside(panel, zone_c),
        "moved to zone C: {panel:?} in {zone_c:?}"
    );
    assert!(parent.pending_window_creates.is_empty());
    assert_eq!(
        *events.lock().unwrap(),
        vec!["torn-off", "docked-on-zone", "docked-on-zone"]
    );

    // 5. A drag released inside its own zone changes nothing.
    let grip = rect_of(&parent, "grip").expect("grip in C");
    let grip_mid = LogicalPosition::new(grip.origin.x + 100.0, grip.origin.y + 8.0);
    drag_by(&mut parent, grip_mid, LogicalPosition::new(0.0, 200.0));
    parent.regenerate_layout().expect("re-layout");
    let panel2 = rect_of(&parent, "panel").expect("still inline");
    assert_eq!(panel2, panel, "a drop inside the same zone is a no-op");
    assert_eq!(events.lock().unwrap().len(), 3);

    // 6. An identical rebuild keeps the graft (the fast path must not forget it): still in C after
    //    a RefreshDom with no DOM change.
    relayout(&mut parent);
    relayout(&mut parent);
    let panel3 = rect_of(&parent, "panel").expect("still inline");
    assert!(
        inside(panel3, zone_c),
        "the graft survives identical rebuilds"
    );
}

// ---------------------------------------------------------------------------
// The keyboard across the picker popup (focus and arrows, 2026-09-26)
// ---------------------------------------------------------------------------

use azul_core::{dom::DomNodeId, window::OptionVirtualKeyCode};
use azul_layout::widgets::color_input::{ColorPickerData, Hsv};

/// `key` goes down while `held` are already down, through the real pass.
fn key_down(window: &mut HeadlessWindow, key: VirtualKeyCode, held: &[VirtualKeyCode], site: &str) {
    window.snapshot_window_state_baseline(site);
    {
        let ks = window.common.keyboard_state_mut();
        let mut pressed: Vec<VirtualKeyCode> = held.to_vec();
        pressed.push(key);
        ks.pressed_virtual_keycodes = pressed.into();
        ks.current_virtual_keycode = OptionVirtualKeyCode::Some(key);
        ks.sync_modifiers();
    }
    let _ = window.process_window_events(0);
}

/// Every key comes back up, through the real pass.
fn keys_up(window: &mut HeadlessWindow, site: &str) {
    window.snapshot_window_state_baseline(site);
    {
        let ks = window.common.keyboard_state_mut();
        ks.pressed_virtual_keycodes = Vec::<VirtualKeyCode>::new().into();
        ks.current_virtual_keycode = OptionVirtualKeyCode::None;
        ks.sync_modifiers();
    }
    let _ = window.process_window_events(0);
}

/// The platform's primary shortcut modifier, the one the picker's large step
/// uses: Cmd on a Mac, Ctrl everywhere else.
fn primary_modifier() -> VirtualKeyCode {
    if azul_core::window::mac_shortcut_conventions() {
        VirtualKeyCode::LWin
    } else {
        VirtualKeyCode::LControl
    }
}

/// The first node whose classes contain `class`, in `window`'s root dom.
fn node_with_class(window: &HeadlessWindow, class: &str) -> DomNodeId {
    let lw = window.get_layout_window().unwrap();
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = root.styled_dom.node_data.as_container();
    let n = nodes
        .linear_iter()
        .find(|n| {
            nodes
                .get(*n)
                .is_some_and(|nd| format!("{:?}", nd.get_ids_and_classes()).contains(class))
        })
        .unwrap_or_else(|| panic!("no node with class {class}"));
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(n)),
    }
}

/// The node `window` focuses, if any.
fn focused(window: &HeadlessWindow) -> Option<DomNodeId> {
    window
        .get_layout_window()
        .and_then(|lw| lw.focus_manager.get_focused_node().copied())
}

/// The colour the picker holds, as HSV, read through the plane's own
/// callback: the one `RefAny` the parent's swatch and the popup's controls
/// share. Read off the RGB colour, so a 1% step shows up within rounding.
fn picker_hsv(window: &HeadlessWindow) -> Hsv {
    let plane = node_with_class(window, "color_picker_plane");
    let lw = window.get_layout_window().unwrap();
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = root.styled_dom.node_data.as_container();
    let mut data = nodes
        .get(plane.node.into_crate_internal().unwrap())
        .unwrap()
        .get_callbacks()
        .as_ref()[0]
        .refany
        .clone();
    let picker = data
        .downcast_ref::<ColorPickerData>()
        .expect("the plane's callback carries the picker state");
    Hsv::from_color(picker.current_color())
}

/// The showcase's ColorInput with its picker opened by a click on the
/// swatch, plus the popup window the run loop would create for it, after the
/// popup's first pass (the one that autofocuses its first control).
fn open_picker_by_click() -> (HeadlessWindow, HeadlessWindow) {
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = picker_widget_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");
    let swatch = rect_of_class(&parent, "native_color_input");
    click_at(
        &mut parent,
        LogicalPosition::new(
            swatch.origin.x + swatch.size.width / 2.0,
            swatch.origin.y + swatch.size.height / 2.0,
        ),
    );
    parent.regenerate_layout().expect("reconcile");
    let popup_opts = take_queued_popup(&mut parent);
    let mut popup = headless(popup_opts, app_data);
    popup.regenerate_layout().expect("popup layout");
    let _ = popup.process_window_events(0);
    (parent, popup)
}

/// P0-0: the engine half of "arrows move the colour". A key delivered to the
/// POPUP (what macOS, Win32 and Wayland do: the popup holds the keyboard)
/// reaches the autofocused plane's `Focus(VirtualKeyDown)` handler: Right
/// adds 1% saturation, the primary modifier + Right adds 10%. The widget's
/// own unit test only re-implemented the arithmetic; this drives the real
/// pass end to end.
#[test]
fn an_arrow_in_the_picker_popup_moves_saturation_by_one_percent() {
    let (_parent, mut popup) = open_picker_by_click();
    let plane = node_with_class(&popup, "color_picker_plane");
    assert_eq!(
        focused(&popup),
        Some(plane),
        "premise: the popup autofocused its first control, the plane"
    );

    let s0 = picker_hsv(&popup).s;
    key_down(&mut popup, VirtualKeyCode::Right, &[], "t.right");
    keys_up(&mut popup, "t.right.up");
    let s1 = picker_hsv(&popup).s;
    assert!(
        (s1 - s0 - 0.01).abs() < 0.005,
        "Right in the popup adds 1% saturation: {s0} -> {s1}"
    );

    key_down(
        &mut popup,
        VirtualKeyCode::Right,
        &[primary_modifier()],
        "t.primary_right",
    );
    keys_up(&mut popup, "t.primary_right.up");
    let s2 = picker_hsv(&popup).s;
    assert!(
        (s2 - s1 - 0.10).abs() < 0.005,
        "the primary modifier + Right in the popup adds 10% saturation: {s1} -> {s2}"
    );
}

/// Headless windows turn every glide off, the focus ring included (it is a
/// glide). This turns the ring back on, at a 1 ms glide, for `window`.
fn ring_on(window: &mut HeadlessWindow) {
    let lw = window.get_layout_window_mut().expect("layout window");
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations {
        focus_ring_duration_ms: 1,
        ..azul_core::resources::SystemAnimations::disabled()
    });
}

/// How many focus rings `window`'s root display list paints: the 2px
/// accent-coloured `Border` the ring post-pass inserts.
fn focus_rings(window: &HeadlessWindow) -> usize {
    use azul_css::{css::CssPropertyValue, props::basic::ColorU};
    use azul_layout::solver3::display_list::DisplayListItem;
    let accent = ColorU {
        r: 43,
        g: 87,
        b: 154,
        a: 255,
    };
    let lw = window.get_layout_window().unwrap();
    lw.layout_results.get(&DomId::ROOT_ID).map_or(0, |lr| {
        lr.display_list
            .items
            .iter()
            .filter(|item| match item {
                DisplayListItem::Border { colors, .. } => {
                    matches!(&colors.top, Some(CssPropertyValue::Exact(c)) if c.inner == accent)
                }
                _ => false,
            })
            .count()
    })
}

/// A parent with the showcase's ColorInput and the focus ring on.
fn ringed_picker_parent(app_data: Arc<RefCell<RefAny>>) -> HeadlessWindow {
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = picker_widget_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data);
    ring_on(&mut parent);
    parent.regenerate_layout().expect("layout");
    parent
}

/// REPORT 1: open the picker from the KEYBOARD (Tab to the swatch, Space),
/// then let the popup take the keyboard the way macOS / Win32 / Wayland do:
/// the parent resigns, the popup autofocuses its plane with the inherited
/// ring. Exactly ONE ring may be on screen - the popup's. The parent kept
/// painting its own ring on the swatch, so the two were out of sync.
#[test]
fn a_keyboard_opened_picker_leaves_exactly_one_ring() {
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut parent = ringed_picker_parent(app_data.clone());

    key_down(&mut parent, VirtualKeyCode::Tab, &[], "t.tab");
    keys_up(&mut parent, "t.tab.up");
    let swatch = node_with_class(&parent, "native_color_input");
    assert_eq!(focused(&parent), Some(swatch), "premise: Tab focused the swatch");
    assert_eq!(
        focus_rings(&parent),
        1,
        "premise: the keyboard-focused swatch is ringed (no popup yet)"
    );

    key_down(&mut parent, VirtualKeyCode::Space, &[], "t.space");
    keys_up(&mut parent, "t.space.up");
    parent.regenerate_layout().expect("reconcile");
    let popup_opts = take_queued_popup(&mut parent);
    assert!(
        mailbox(&popup_opts.window_state).focus_visible,
        "premise: a keyboard-opened popup inherits the ring"
    );

    // The popup becomes the key window; the parent resigns.
    parent.snapshot_window_state_baseline("t.resign");
    parent
        .common
        .update_unsynced_state(|ws| ws.window_focused = false);
    let _ = parent.process_window_events(0);
    let mut popup = headless(popup_opts, app_data);
    ring_on(&mut popup);
    popup.regenerate_layout().expect("popup layout");
    let _ = popup.process_window_events(0);

    assert_eq!(
        focused(&parent),
        Some(swatch),
        "the invoker keeps its focus (it gets it back on close)"
    );
    assert_eq!(
        focus_rings(&parent),
        0,
        "the parent paints no ring while the popup holds the keyboard"
    );
    assert_eq!(
        focused(&popup),
        Some(node_with_class(&popup, "color_picker_plane")),
        "the popup autofocused its plane"
    );
    assert_eq!(
        focus_rings(&popup),
        1,
        "and rings it, the one ring on screen"
    );
}

/// P1-6: Escape that reaches the PARENT (X11, where the override-redirect
/// popup never gets the keyboard) closes the picker - and that is all it
/// does. The same key used to run the parent's own default too, `ClearFocus`,
/// so the app saw the swatch blur and, a pass later, focus again (the owed
/// focus restore), with a window focusing nothing in between.
#[test]
fn escape_in_the_parent_closes_the_picker_without_blurring_the_swatch() {
    let (mut parent, _popup) = open_picker_by_click();
    let swatch = node_with_class(&parent, "native_color_input");
    assert_eq!(
        focused(&parent),
        Some(swatch),
        "premise: the click focused the swatch"
    );

    key_down(&mut parent, VirtualKeyCode::Escape, &[], "t.escape");
    assert!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .is_empty(),
        "Escape in the parent closed the picker"
    );
    assert_eq!(
        focused(&parent),
        Some(swatch),
        "the Escape that closed the popup did not also clear the parent's focus"
    );
    keys_up(&mut parent, "t.escape.up");
}

/// P0-2, REPORT 2 ON X11: the picker is open, but the popup is an
/// override-redirect window that never gets the input focus, so every key
/// lands in the PARENT - which ran its own default (spatial navigation from
/// the swatch, onto the Slider hidden under the popup) while the plane the
/// popup rings did nothing. The parent must hand a key it receives while a
/// focus-taking popup is open to that popup, keep its own focus on the
/// invoker, and not ring the invoker while the popup holds the keyboard.
/// Backend-neutral: the headless parent stays the active window, exactly as
/// an X11 parent does.
#[test]
fn a_key_the_parent_receives_while_its_picker_is_open_drives_the_picker() {
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut parent = ringed_picker_parent(app_data.clone());
    key_down(&mut parent, VirtualKeyCode::Tab, &[], "t.tab");
    keys_up(&mut parent, "t.tab.up");
    let swatch = node_with_class(&parent, "native_color_input");
    assert_eq!(focused(&parent), Some(swatch), "premise: Tab focused the swatch");
    key_down(&mut parent, VirtualKeyCode::Space, &[], "t.space");
    keys_up(&mut parent, "t.space.up");
    parent.regenerate_layout().expect("reconcile");
    let popup_opts = take_queued_popup(&mut parent);
    let mut popup = headless(popup_opts, app_data);
    ring_on(&mut popup);
    popup.regenerate_layout().expect("popup layout");
    let _ = popup.process_window_events(0);
    assert_eq!(
        focused(&popup),
        Some(node_with_class(&popup, "color_picker_plane")),
        "premise: the popup autofocused its plane"
    );

    // Right arrives at the PARENT; the popup's next pass is its wake-up.
    let s0 = picker_hsv(&popup).s;
    key_down(&mut parent, VirtualKeyCode::Right, &[], "t.right");
    let _ = popup.process_window_events(0);
    keys_up(&mut parent, "t.right.up");
    let _ = popup.process_window_events(0);
    let s1 = picker_hsv(&popup).s;
    assert!(
        (s1 - s0 - 0.01).abs() < 0.005,
        "a Right the parent received moved the picker's saturation by 1%: {s0} -> {s1}"
    );
    assert_eq!(
        focused(&parent),
        Some(swatch),
        "the parent's own focus stays on the invoker"
    );
    assert_eq!(
        focus_rings(&parent),
        0,
        "the invoker is not ringed while its popup holds the keyboard"
    );
}

/// A ComboBox: a field whose `<transient-window>` holds the option list.
extern "C" fn combobox_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_layout::widgets::combobox::ComboBox;
    Dom::create_body().with_child(
        ComboBox::new(azul_css::StringVec::from_vec(vec![
            "Red".into(),
            "Green".into(),
            "Blue".into(),
        ]))
        .dom(),
    )
}

/// P0-3: not every popup takes the keyboard. A combobox's list follows the
/// WAI-ARIA combobox pattern: DOM focus STAYS on the field (typing keeps
/// editing it) and the list is only its popup. Every transient popup used to
/// autofocus its first tab stop, so opening the list moved focus onto the
/// first option - where typed text had no handler.
#[test]
fn a_combobox_list_popup_does_not_take_focus() {
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = combobox_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");
    let field_rect = rect_of_class(&parent, "combobox-input");
    click_at(
        &mut parent,
        LogicalPosition::new(
            field_rect.origin.x + field_rect.size.width / 2.0,
            field_rect.origin.y + field_rect.size.height / 2.0,
        ),
    );
    parent.regenerate_layout().expect("reconcile");
    let field = node_with_class(&parent, "combobox-input");
    assert_eq!(
        focused(&parent),
        Some(field),
        "premise: the click focused the field"
    );
    let popup_opts = take_queued_popup(&mut parent);
    let mut popup = headless(popup_opts, app_data);
    popup.regenerate_layout().expect("popup layout");
    let _ = popup.process_window_events(0);

    assert_eq!(
        focused(&popup),
        None,
        "the list popup does not take focus (no autofocus of its first option)"
    );
    assert!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_keyboard_owner()
            .is_none(),
        "and does not hold the parent's keyboard: keys stay with the field"
    );
    assert_eq!(focused(&parent), Some(field), "the field keeps its focus");
}

/// A FOCUSABLE swatch whose popup follows the app's `open` flag - the
/// documented primary API ("the app never touches a window, it toggles
/// `open`"), which reaches no callback seam.
extern "C" fn focusable_swatch_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let open = data
        .downcast_ref::<PickerState>()
        .map_or(false, |s| s.open);
    let cfg = if open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    }
    .with_dismiss(TransientDismiss::Escape);
    let popup = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(cfg)))
        .with_child(Dom::create_div().with_css("width: 120px; height: 60px;".into()));
    let swatch = Dom::create_div()
        .with_css("width: 60px; height: 24px; margin: 40px; background: #e66465;".into())
        .with_tab_index(azul_core::dom::TabIndex::Auto)
        .with_child(popup);
    Dom::create_body().with_child(swatch)
}

/// P1-7: a popup the APP opened through its `open` attribute owes focus
/// back exactly like one a widget opened with `set_transient_window_open`.
/// Only that callback seam recorded where focus was, so an attribute-opened
/// popup dismissed with Escape (or an outside press that cleared focus)
/// handed nothing back.
#[test]
fn a_popup_opened_by_its_attribute_owes_focus_back_on_dismissal() {
    let app_data = Arc::new(RefCell::new(RefAny::new(PickerState {
        open: false,
        label: "",
        dismiss: TransientDismiss::Escape,
        ack_dismiss: true,
        dismissed_calls: Arc::new(AtomicUsize::new(0)),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = focusable_swatch_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data);
    parent.regenerate_layout().expect("layout");

    key_down(&mut parent, VirtualKeyCode::Tab, &[], "t.tab");
    keys_up(&mut parent, "t.tab.up");
    let swatch = focused(&parent).expect("premise: Tab focused the swatch");

    with_state(&parent, |s| s.open = true);
    relayout(&mut parent);
    let _ = take_queued_popup(&mut parent);

    key_down(&mut parent, VirtualKeyCode::Escape, &[], "t.escape");
    assert!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .is_empty(),
        "premise: Escape dismissed the popup"
    );
    let owed = parent
        .get_layout_window_mut()
        .unwrap()
        .transient_windows
        .take_pending_focus_restore();
    assert_eq!(
        owed,
        Some((swatch, true)),
        "the dismissal owes the Tab-focused swatch (ringed) its focus back"
    );
    keys_up(&mut parent, "t.escape.up");
}

/// App state for the blur-on-close scenario: the popup's `open` flag and how
/// often its field heard `FocusLost`.
struct BlurProbe {
    open: bool,
    blurs: Arc<AtomicUsize>,
}

extern "C" fn on_field_blur(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(p) = data.downcast_ref::<BlurProbe>() {
        p.blurs.fetch_add(1, Ordering::SeqCst);
    }
    Update::DoNothing
}

/// A swatch whose `outside`-dismissable popup holds one focusable field that
/// commits on `FocusLost` - the shape of the picker's hex field.
extern "C" fn blur_probe_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let open = data.downcast_ref::<BlurProbe>().map_or(false, |p| p.open);
    let cfg = if open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    }
    .with_dismiss(TransientDismiss::Outside);
    let field = Dom::create_div()
        .with_css("width: 120px; height: 24px;".into())
        .with_tab_index(azul_core::dom::TabIndex::Auto)
        .with_callback(
            EventFilter::Focus(azul_core::events::FocusEventFilter::FocusLost),
            data.clone(),
            Callback {
                cb: on_field_blur,
                ctx: azul_core::refany::OptionRefAny::None,
            }
            .to_core(),
        );
    let popup = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(cfg)))
        .with_child(field);
    let swatch = Dom::create_div()
        .with_css("width: 60px; height: 24px; margin: 40px; background: #e66465;".into())
        .with_child(popup);
    Dom::create_body().with_child(swatch)
}

/// P1-8: a popup that closes takes its focused control's `FocusLost` with
/// it. The picker's hex field COMMITS on focus loss, so typing `#00ff00`
/// and then clicking back into the parent (a light dismiss) dropped the
/// value: the popup window was destroyed with its `FocusManager`, and no
/// Blur ever reached the field. A close is not a cancel.
#[test]
fn a_light_dismiss_blurs_the_popups_focused_field_before_it_closes() {
    let blurs = Arc::new(AtomicUsize::new(0));
    let app_data = Arc::new(RefCell::new(RefAny::new(BlurProbe {
        open: true,
        blurs: blurs.clone(),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = blur_probe_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");
    let popup_opts = take_queued_popup(&mut parent);
    let mut popup = headless(popup_opts, app_data);
    popup.regenerate_layout().expect("popup layout");
    let _ = popup.process_window_events(0);
    assert!(
        focused(&popup).is_some(),
        "premise: the popup autofocused its field"
    );

    // A press in the parent: outside the popup, a light dismiss.
    click_at(&mut parent, LogicalPosition::new(400.0, 400.0));
    // The popup's next pass reads `closed` from its mailbox.
    popup.regenerate_layout().expect("popup reads its mailbox");
    assert!(close_requested(&popup), "premise: the popup closes itself");
    assert_eq!(
        blurs.load(Ordering::SeqCst),
        1,
        "the focused field heard FocusLost before its window went away"
    );
}

/// A ColorInput between two plain tab stops: `.stop-before`, the swatch,
/// `.stop-after`.
extern "C" fn picker_between_two_stops_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_layout::widgets::color_input::{color_from_hex, ColorInput};
    let stop = |class: &'static str| -> Dom {
        Dom::create_div()
            .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class(class.into())].into())
            .with_css("width: 80px; height: 20px;".into())
            .with_tab_index(azul_core::dom::TabIndex::Auto)
    };
    Dom::create_body()
        .with_child(stop("stop-before"))
        .with_child(ColorInput::create(color_from_hex("#ff5733").expect("a colour")).dom())
        .with_child(stop("stop-after"))
}

/// Tab past `.stop-before` onto the swatch and open the picker with Space:
/// `(parent, popup, swatch)`, the popup after its first pass (its plane
/// autofocused and ringed), the ring on in both windows.
fn open_picker_between_stops_from_the_keyboard() -> (HeadlessWindow, HeadlessWindow, DomNodeId)
{
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = picker_between_two_stops_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    ring_on(&mut parent);
    parent.regenerate_layout().expect("layout");
    for site in ["t.tab1", "t.tab2"] {
        key_down(&mut parent, VirtualKeyCode::Tab, &[], site);
        keys_up(&mut parent, site);
    }
    let swatch = node_with_class(&parent, "native_color_input");
    assert_eq!(
        focused(&parent),
        Some(swatch),
        "premise: two Tabs reach the swatch"
    );
    key_down(&mut parent, VirtualKeyCode::Space, &[], "t.space");
    keys_up(&mut parent, "t.space.up");
    parent.regenerate_layout().expect("reconcile");
    let popup_opts = take_queued_popup(&mut parent);
    let mut popup = headless(popup_opts, app_data);
    ring_on(&mut popup);
    popup.regenerate_layout().expect("popup layout");
    let _ = popup.process_window_events(0);
    (parent, popup, swatch)
}

/// USER RULING (open question 2): Escape CLOSES the picker - no colour
/// restore - and hands focus AND a visible ring back to the swatch, so the
/// next Tab continues from it. Escape delivered to the POPUP, as macOS and
/// Win32 deliver it (the popup is the key window).
#[test]
fn escape_in_the_picker_closes_it_and_tab_moves_on_from_the_swatch() {
    let (mut parent, mut popup, swatch) = open_picker_between_stops_from_the_keyboard();
    key_down(&mut popup, VirtualKeyCode::Escape, &[], "t.escape");
    assert!(close_requested(&popup), "premise: Escape closed the picker");

    // The parent reads the dismissal on its next layout (the wake-up the
    // popup asked for) and pays the owed focus on its next pass.
    parent.regenerate_layout().expect("the parent reads the dismissal");
    let _ = parent.process_window_events(0);
    assert_eq!(focused(&parent), Some(swatch), "focus is back on the swatch");
    assert!(
        parent.get_layout_window().unwrap().focus_manager.focus_is_visible,
        "as KEYBOARD focus"
    );
    assert_eq!(
        focus_rings(&parent),
        1,
        "and the swatch is ringed before the next key"
    );

    key_down(&mut parent, VirtualKeyCode::Tab, &[], "t.tab");
    keys_up(&mut parent, "t.tab.up");
    assert_eq!(
        focused(&parent),
        Some(node_with_class(&parent, "stop-after")),
        "Tab continues from the swatch to the stop after it, not from the first stop"
    );
}

/// The same ruling with Escape delivered to the PARENT, as X11 delivers it
/// (the override-redirect popup never has the keyboard): the picker closes,
/// focus and ring stay on the swatch, Shift+Tab goes to the stop before it.
#[test]
fn escape_in_the_parent_closes_the_picker_and_shift_tab_moves_back_from_the_swatch() {
    let (mut parent, _popup, swatch) = open_picker_between_stops_from_the_keyboard();
    key_down(&mut parent, VirtualKeyCode::Escape, &[], "t.escape");
    keys_up(&mut parent, "t.escape.up");
    assert!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .is_empty(),
        "premise: Escape in the parent closed the picker"
    );
    parent
        .regenerate_layout()
        .expect("drain the Dismissed event");
    let _ = parent.process_window_events(0);
    assert_eq!(focused(&parent), Some(swatch), "focus is on the swatch");
    assert!(
        parent.get_layout_window().unwrap().focus_manager.focus_is_visible,
        "as KEYBOARD focus"
    );
    assert_eq!(focus_rings(&parent), 1, "and the swatch is ringed");

    key_down(
        &mut parent,
        VirtualKeyCode::Tab,
        &[VirtualKeyCode::LShift],
        "t.shift_tab",
    );
    keys_up(&mut parent, "t.shift_tab.up");
    assert_eq!(
        focused(&parent),
        Some(node_with_class(&parent, "stop-before")),
        "Shift+Tab goes from the swatch to the stop before it"
    );
}

/// The user ruling (2026-09-28) along the path a device takes on macOS and
/// Win32, where the picker is the KEY window: the parent RESIGNS the
/// keyboard when the picker opens, Escape goes to the picker, the picker
/// closes, and the parent becomes key again - its activation (WindowFocusIn)
/// arriving BEFORE it reads the picker's dismissal. Through that whole round
/// trip focus must come back to the swatch, ringed, and the next Tab must go
/// to the stop AFTER it - on the device it restarted at the first stop.
///
/// This drives the engine's half of that path; if it holds and a device
/// still restarts at 0, the backend's own activation handling is the
/// suspect: run it with `AZ_FOCUS_TRACE=1` (every KeyDown's window, focus
/// and default action, every popup focus decision).
#[test]
fn escape_in_the_key_window_picker_returns_focus_through_the_activation_round_trip() {
    let (mut parent, mut popup, swatch) = open_picker_between_stops_from_the_keyboard();

    // The picker becomes the key window: the parent resigns.
    parent.snapshot_window_state_baseline("t.resign");
    parent
        .common
        .update_unsynced_state(|ws| ws.window_focused = false);
    let _ = parent.process_window_events(0);
    assert_eq!(focused(&parent), Some(swatch), "premise: the swatch keeps its focus");

    // Escape in the picker closes it.
    key_down(&mut popup, VirtualKeyCode::Escape, &[], "t.escape");
    assert!(close_requested(&popup), "premise: Escape closed the picker");

    // The parent is key again BEFORE it reads the dismissal...
    parent.snapshot_window_state_baseline("t.reactivate");
    parent
        .common
        .update_unsynced_state(|ws| ws.window_focused = true);
    let _ = parent.process_window_events(0);
    // ...then reads it (the wake-up the picker asked for) and pays the focus
    // it owes on its next pass.
    parent.regenerate_layout().expect("the parent reads the dismissal");
    let _ = parent.process_window_events(0);

    assert_eq!(focused(&parent), Some(swatch), "focus is back on the swatch");
    assert!(
        parent.get_layout_window().unwrap().focus_manager.focus_is_visible,
        "as KEYBOARD focus"
    );
    assert_eq!(focus_rings(&parent), 1, "and the swatch is ringed");

    key_down(&mut parent, VirtualKeyCode::Tab, &[], "t.tab");
    keys_up(&mut parent, "t.tab.up");
    assert_eq!(
        focused(&parent),
        Some(node_with_class(&parent, "stop-after")),
        "Tab continues from the swatch to the stop after it, not from the first stop"
    );
}

/// P2-11: a popup autofocuses its first control ONCE. The autofocus re-armed
/// on every pass while nothing was focused, so a click on a non-focusable
/// spot of the picker (its padding) - which clears focus, as a click on
/// nothing does - was undone on the very next pass: the plane was focused
/// again, with the keyboard ring the popup inherited when it opened.
#[test]
fn a_popup_autofocuses_only_once() {
    let (_parent, mut popup, _swatch) = open_picker_between_stops_from_the_keyboard();
    let plane = node_with_class(&popup, "color_picker_plane");
    assert_eq!(
        focused(&popup),
        Some(plane),
        "premise: the popup autofocused its plane"
    );
    let plane_rect = rect_of_class(&popup, "color_picker_plane");
    // The panel's padding, left of the plane: nothing focusable is there.
    let padding = LogicalPosition::new(
        plane_rect.origin.x - 4.0,
        plane_rect.origin.y + plane_rect.size.height / 2.0,
    );
    click_at(&mut popup, padding);
    let _ = popup.process_window_events(0);
    assert_eq!(
        focused(&popup),
        None,
        "a click on nothing leaves nothing focused; the autofocus does not come back"
    );
}

/// The showcase ComboBox (Red, Green, Blue) in a laid-out parent, its list
/// closed, plus the app data the popup windows share.
fn combobox_parent() -> (HeadlessWindow, Arc<RefCell<RefAny>>) {
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = combobox_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");
    (parent, app_data)
}

/// A ComboBox whose list was opened by a click on its field: `(parent,
/// popup, field)`, the popup after its first pass.
fn open_combobox() -> (HeadlessWindow, HeadlessWindow, DomNodeId) {
    let (mut parent, app_data) = combobox_parent();
    let field_rect = rect_of_class(&parent, "combobox-input");
    click_at(
        &mut parent,
        LogicalPosition::new(
            field_rect.origin.x + field_rect.size.width / 2.0,
            field_rect.origin.y + field_rect.size.height / 2.0,
        ),
    );
    parent.regenerate_layout().expect("reconcile");
    let field = node_with_class(&parent, "combobox-input");
    let popup_opts = take_queued_popup(&mut parent);
    let mut popup = headless(popup_opts, app_data);
    popup.regenerate_layout().expect("popup layout");
    let _ = popup.process_window_events(0);
    (parent, popup, field)
}

/// Every node of `window`'s root dom carrying exactly the class `class`, in
/// document order.
fn nodes_classed(window: &HeadlessWindow, class: &str) -> Vec<DomNodeId> {
    let lw = window.get_layout_window().unwrap();
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = root.styled_dom.node_data.as_container();
    nodes
        .linear_iter()
        .filter(|n| {
            nodes.get(*n).is_some_and(|nd| {
                nd.get_ids_and_classes().as_ref().iter().any(|c| {
                    matches!(c, azul_core::dom::IdOrClass::Class(s) if s.as_str() == class)
                })
            })
        })
        .map(|n| DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(n)),
        })
        .collect()
}

const COMBOBOX_OPTION: &str = "__azul-native-combobox-option";
const COMBOBOX_OPTION_ACTIVE: &str = "__azul-native-combobox-option-active";

/// The options the list SHOWS as active (the widget's marker class), as
/// indices into the list.
fn active_options(popup: &HeadlessWindow) -> Vec<usize> {
    let active = nodes_classed(popup, COMBOBOX_OPTION_ACTIVE);
    nodes_classed(popup, COMBOBOX_OPTION)
        .iter()
        .enumerate()
        .filter(|(_, o)| active.contains(o))
        .map(|(i, _)| i)
        .collect()
}

/// The options the list ANNOUNCES as selected (their accessibility state),
/// as indices into the list.
fn announced_options(popup: &HeadlessWindow) -> Vec<usize> {
    let lw = popup.get_layout_window().unwrap();
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = root.styled_dom.node_data.as_container();
    nodes_classed(popup, COMBOBOX_OPTION)
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            nodes
                .get(o.node.into_crate_internal().unwrap())
                .and_then(|nd| nd.get_accessibility_info())
                .is_some_and(|a| {
                    a.states
                        .as_ref()
                        .contains(&azul_core::a11y::AccessibilityState::Selected)
                })
        })
        .map(|(i, _)| i)
        .collect()
}

/// The combobox's shared state, read through the field's own callback.
fn combobox_state(parent: &HeadlessWindow) -> azul_layout::widgets::combobox::ComboBoxState {
    let field = node_with_class(parent, "combobox-input");
    let lw = parent.get_layout_window().unwrap();
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = root.styled_dom.node_data.as_container();
    let mut data = nodes
        .get(field.node.into_crate_internal().unwrap())
        .unwrap()
        .get_callbacks()
        .as_ref()[0]
        .refany
        .clone();
    let wrapper = data
        .downcast_ref::<azul_layout::widgets::combobox::ComboBoxStateWrapper>()
        .expect("the field's callback carries the combobox state");
    wrapper.inner.clone()
}

/// One key typed in the PARENT while the list is open, the popup running a
/// pass after each transition - the parent forwards the list's keys, the
/// popup replays them (X11 delivers at once; headless drives it here).
fn type_into_combobox(
    parent: &mut HeadlessWindow,
    popup: &mut HeadlessWindow,
    key: VirtualKeyCode,
    site: &str,
) {
    key_down(parent, key, &[], site);
    let _ = popup.process_window_events(0);
    keys_up(parent, site);
    let _ = popup.process_window_events(0);
}

/// WAI-ARIA combobox (items 1 + 2 of the focus leftovers): DOM focus never
/// leaves the field. A navigation key typed while the list is open moves
/// the list's ACTIVE option - shown, and announced as the selected one -
/// while the field keeps focus (so typing keeps editing it) and the list
/// window takes none. The first navigation key used to move REAL focus into
/// the list's first option.
#[test]
fn down_in_the_field_makes_an_option_active_and_focus_stays_on_the_field() {
    let (mut parent, mut popup, field) = open_combobox();

    type_into_combobox(&mut parent, &mut popup, VirtualKeyCode::Down, "t.down1");
    assert_eq!(focused(&popup), None, "the list never takes focus");
    assert_eq!(focused(&parent), Some(field), "the field keeps it");
    assert_eq!(active_options(&popup), vec![0], "Down: the first option is active");
    assert_eq!(announced_options(&popup), vec![0], "and announced as selected");

    type_into_combobox(&mut parent, &mut popup, VirtualKeyCode::Down, "t.down2");
    assert_eq!(active_options(&popup), vec![1], "Down again: the second");
    assert_eq!(announced_options(&popup), vec![1]);

    type_into_combobox(&mut parent, &mut popup, VirtualKeyCode::Up, "t.up");
    assert_eq!(active_options(&popup), vec![0], "Up: back to the first");

    type_into_combobox(&mut parent, &mut popup, VirtualKeyCode::End, "t.end");
    assert_eq!(active_options(&popup), vec![2], "End: the last");
    assert_eq!(focused(&popup), None);
    assert_eq!(focused(&parent), Some(field));
}

/// The same when the list window gets the key itself (no parent involved):
/// it moves the active option, it never focuses one.
#[test]
fn a_navigation_key_the_list_window_receives_moves_its_active_option_not_focus() {
    let (_parent, mut popup, _field) = open_combobox();
    assert_eq!(focused(&popup), None, "premise: the list took no focus");
    key_down(&mut popup, VirtualKeyCode::Down, &[], "t.down");
    keys_up(&mut popup, "t.down.up");
    assert_eq!(focused(&popup), None, "no option is focused");
    assert_eq!(active_options(&popup), vec![0], "the first option is active");
}

/// Enter picks the ACTIVE option, like a click on it: the field's value
/// becomes that option's, and the list closes.
#[test]
fn enter_in_the_field_picks_the_active_option_and_closes_the_list() {
    let (mut parent, mut popup, field) = open_combobox();
    type_into_combobox(&mut parent, &mut popup, VirtualKeyCode::Down, "t.down1");
    type_into_combobox(&mut parent, &mut popup, VirtualKeyCode::Down, "t.down2");
    type_into_combobox(&mut parent, &mut popup, VirtualKeyCode::Return, "t.enter");

    assert!(close_requested(&popup), "Enter closed the list");
    let state = combobox_state(&parent);
    assert_eq!(state.selected, 1, "the active option was picked");
    assert_eq!(state.text.as_str(), "Green");
    assert_eq!(focused(&parent), Some(field), "the field keeps focus");
}

/// Down on a CLOSED combobox opens its list - the keyboard's way to what a
/// click on the field does - and focus stays on the field.
#[test]
fn down_on_a_closed_combobox_opens_its_list() {
    let (mut parent, _) = combobox_parent();
    key_down(&mut parent, VirtualKeyCode::Tab, &[], "t.tab");
    keys_up(&mut parent, "t.tab.up");
    let field = node_with_class(&parent, "combobox-input");
    assert_eq!(focused(&parent), Some(field), "premise: Tab focused the field");

    key_down(&mut parent, VirtualKeyCode::Down, &[], "t.down");
    keys_up(&mut parent, "t.down.up");
    parent.regenerate_layout().expect("reconcile");
    assert_eq!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .len(),
        1,
        "Down opened the list"
    );
    assert_eq!(focused(&parent), Some(field), "focus stayed on the field");
}

/// The list is not the active window (nothing would dismiss it for losing
/// focus it never had), so it closes with its PARENT's deactivation - the
/// user went to another window or app, as a native list does.
#[test]
fn a_list_popup_closes_when_its_parent_window_is_deactivated() {
    let (mut parent, _popup, _field) = open_combobox();
    parent.snapshot_window_state_baseline("t.deactivate");
    parent
        .common
        .update_unsynced_state(|ws| ws.window_focused = false);
    let _ = parent.process_window_events(0);
    assert!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .is_empty(),
        "the list closed with its window's deactivation"
    );
}

/// GUARD for the rule above: a popup that TAKES focus (the picker) makes
/// its parent resign the keyboard to it - that is no reason to close it.
#[test]
fn a_focus_taking_popup_survives_its_parent_resigning_the_keyboard() {
    let (mut parent, _popup) = open_picker_by_click();
    parent.snapshot_window_state_baseline("t.resign");
    parent
        .common
        .update_unsynced_state(|ws| ws.window_focused = false);
    let _ = parent.process_window_events(0);
    assert_eq!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .len(),
        1,
        "the picker stays open"
    );
}

// ---------------------------------------------------------------------------
// The date grid across months (roving-tabindex leftover, 2026-09-28)
// ---------------------------------------------------------------------------

use azul_layout::widgets::date_picker::{DatePicker, DatePickerState};

/// The app's own date, which the picker's `on_change` keeps - the host
/// rebuild that turns the calendar to another month.
struct DateApp {
    date: DatePickerState,
}

extern "C" fn date_app_changed(mut data: RefAny, _info: CallbackInfo, state: DatePickerState) -> Update {
    if let Some(mut app) = data.downcast_mut::<DateApp>() {
        app.date = state;
    }
    Update::RefreshDom
}

extern "C" fn date_app_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_layout::widgets::date_picker::DatePickerOnChangeCallbackType;
    let date = data
        .downcast_ref::<DateApp>()
        .map(|a| a.date)
        .expect("the app data is a DateApp");
    let on_change: DatePickerOnChangeCallbackType = date_app_changed;
    Dom::create_body().with_child(
        DatePicker::create(date.year, date.month, date.day)
            .with_on_change(data.clone(), on_change)
            .dom(),
    )
}

/// The node of `window`'s root dom that asks for focus (`autofocus`).
fn autofocus_node(window: &HeadlessWindow) -> Option<DomNodeId> {
    let lw = window.get_layout_window().unwrap();
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = root.styled_dom.node_data.as_container();
    nodes
        .linear_iter()
        .find(|n| nodes.get(*n).is_some_and(NodeData::has_autofocus))
        .map(|n| DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(n)),
        })
}

/// The accessibility name `node` declares in `window`.
fn a11y_name_of(window: &HeadlessWindow, node: DomNodeId) -> String {
    let lw = window.get_layout_window().unwrap();
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    root.styled_dom
        .node_data
        .as_container()
        .get(node.node.into_crate_internal().unwrap())
        .and_then(|nd| nd.get_accessibility_info())
        .and_then(|a| a.accessibility_name.as_ref().map(|s| s.as_str().to_string()))
        .unwrap_or_default()
}

/// WAI-ARIA APG date grid, end to end: Right on the last day of the shown
/// month turns the calendar to the next month (the app keeps the date and
/// rebuilds) and focus lands on that month's first day - the day the arrow
/// aimed at - not on whatever cell sat in the old day's slot. The arrow used
/// to be swallowed at the edge of the month.
#[test]
fn an_arrow_off_the_last_day_turns_the_month_and_focus_lands_on_the_day_it_aimed_at() {
    let app_data = Arc::new(RefCell::new(RefAny::new(DateApp {
        date: DatePickerState {
            year: 2024,
            month: 2,
            day: 29,
        },
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = date_app_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data.clone());
    parent.regenerate_layout().expect("layout");
    let field = rect_of_class(&parent, "__azul-native-date-picker");
    click_at(
        &mut parent,
        LogicalPosition::new(
            field.origin.x + field.size.width / 2.0,
            field.origin.y + field.size.height / 2.0,
        ),
    );
    parent.regenerate_layout().expect("reconcile");
    let popup_opts = take_queued_popup(&mut parent);
    let mut popup = headless(popup_opts, app_data.clone());
    popup.regenerate_layout().expect("popup layout");
    let _ = popup.process_window_events(0);
    let feb29 = autofocus_node(&popup).expect("the calendar asks focus for its selected day");
    assert_eq!(focused(&popup), Some(feb29), "premise: the calendar opened on the 29th");

    key_down(&mut popup, VirtualKeyCode::Right, &[], "t.right");
    keys_up(&mut popup, "t.right.up");
    let date = {
        let mut app = app_data.borrow().clone();
        let a = app.downcast_ref::<DateApp>().expect("DateApp");
        a.date
    };
    assert_eq!(
        date,
        DatePickerState {
            year: 2024,
            month: 3,
            day: 1
        },
        "Right on February 29th turned the calendar to March 1st"
    );

    // The host rebuilds onto March; the popup adopts the new content.
    parent.regenerate_layout().expect("the parent rebuilds onto March");
    popup.regenerate_layout().expect("the popup adopts March");
    let march1 = autofocus_node(&popup).expect("March's calendar asks focus for the 1st");
    assert!(
        a11y_name_of(&popup, march1).contains("1 March 2024"),
        "premise: the Tab-stop day is March 1st, got {:?}",
        a11y_name_of(&popup, march1)
    );
    assert_eq!(
        focused(&popup),
        Some(march1),
        "focus followed the arrow into the new month"
    );
}

// ---------------------------------------------------------------------------
// The widgets demo's Popover (device report 2026-09-28: "the popover cannot
// be closed again")
// ---------------------------------------------------------------------------

/// The demo's toggle handler: it ignores the new state and asks for a
/// rebuild, as `examples/azul-widgets` does (`bump` returns `RefreshDom`).
extern "C" fn popover_toggled_refresh(
    _data: RefAny,
    _info: CallbackInfo,
    _state: azul_layout::widgets::popover::PopoverState,
) -> Update {
    Update::RefreshDom
}

/// The Overlays card of the widgets demo: a Popover built `with_open(false)`
/// on every layout, whose toggle callback returns `RefreshDom`.
extern "C" fn demo_popover_layout(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_layout::widgets::{
        button::Button,
        popover::{Popover, PopoverOnToggleCallbackType},
    };
    let on_toggle: PopoverOnToggleCallbackType = popover_toggled_refresh;
    Dom::create_body().with_child(
        Popover::new(
            Button::create("Open popover".into()).dom(),
            Dom::create_div()
                .with_ids_and_classes(
                    vec![azul_core::dom::IdOrClass::Class("demo-popover-body".into())].into(),
                )
                .with_child(Dom::create_p_with_text("Popover content")),
        )
        .with_open(false)
        .with_on_toggle(data, on_toggle)
        .dom(),
    )
}

/// Whether the demo popover's content is on screen: either laid out with a
/// real box in the parent window, or shown in an open transient window.
fn demo_popover_shown(parent: &HeadlessWindow) -> bool {
    let lw = parent.get_layout_window().unwrap();
    if !lw.transient_windows.open_windows().is_empty() {
        return true;
    }
    let root = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = root.styled_dom.node_data.as_container();
    nodes.linear_iter().any(|n| {
        let is_body = nodes
            .get(n)
            .is_some_and(|nd| format!("{:?}", nd.get_ids_and_classes()).contains("demo-popover-body"));
        is_body
            && lw
                .get_node_layout_rect(DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(n)),
                })
                .is_some_and(|r| r.size.width > 0.0 && r.size.height > 0.0)
    })
}

/// The demo builds its popover `with_open(false)` on every layout and its
/// toggle callback returns `RefreshDom`. The first click on the trigger
/// opens the popover; the second must close it again.
#[test]
fn the_widgets_demo_popover_closes_again_on_a_second_click() {
    let app_data = Arc::new(RefCell::new(RefAny::new(0u8)));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = demo_popover_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data);
    parent.regenerate_layout().expect("layout");
    assert!(
        !demo_popover_shown(&parent),
        "premise: the popover starts closed"
    );

    let trigger = rect_of_class(&parent, "__azul-native-popover-trigger");
    let at = LogicalPosition::new(
        trigger.origin.x + trigger.size.width / 2.0,
        trigger.origin.y + trigger.size.height / 2.0,
    );

    click_at(&mut parent, at);
    parent.regenerate_layout().expect("the app's RefreshDom");
    assert!(
        demo_popover_shown(&parent),
        "the first click opens the popover"
    );
    parent.pending_window_creates.clear();

    click_at(&mut parent, at);
    parent.regenerate_layout().expect("the app's RefreshDom");
    assert!(
        !demo_popover_shown(&parent),
        "the second click closes the popover again"
    );
}

// ---------------------------------------------------------------------------
// `dismiss="outside-only"`: light dismiss by the engine, Escape by the content
// ---------------------------------------------------------------------------

/// An `outside-only` popup leaves Escape to its content (a dialog answers it
/// with a cancelable `cancel` first), but an outside press still light-
/// dismisses it, like `outside`.
#[test]
fn an_outside_only_popup_leaves_escape_to_its_content_but_closes_on_an_outside_press() {
    let mut parent = make_parent(true, TransientDismiss::OutsideOnly);
    parent.regenerate_layout().expect("layout");
    let popup_opts = take_queued_popup(&mut parent);
    let mut popup = headless(popup_opts.clone(), parent.common.app_data.clone());
    popup.regenerate_layout().expect("popup layout");

    // Escape in the popup: the engine does not close it.
    popup.snapshot_window_state_baseline("test.escape");
    popup.common.keyboard_state_mut().pressed_virtual_keycodes =
        vec![VirtualKeyCode::Escape].into();
    let _ = popup.process_window_events(0);
    assert!(
        !close_requested(&popup),
        "Escape is the content's to answer, the engine must not close the popup"
    );
    assert!(!mailbox_state(&popup_opts.window_state).1, "nothing posted");

    // Escape in the parent: the engine does not close it either.
    parent.snapshot_window_state_baseline("test.parent.escape");
    parent.common.keyboard_state_mut().pressed_virtual_keycodes =
        vec![VirtualKeyCode::Escape].into();
    let _ = parent.process_window_events(0);
    parent.snapshot_window_state_baseline("test.parent.escape.up");
    parent.common.keyboard_state_mut().pressed_virtual_keycodes =
        Vec::<VirtualKeyCode>::new().into();
    let _ = parent.process_window_events(0);
    assert_eq!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .len(),
        1,
        "Escape in the parent leaves an outside-only popup open"
    );

    // A press in the parent (outside the popup) does close it.
    parent.snapshot_window_state_baseline("test.press");
    parent.common.mouse_state_mut().cursor_position =
        CursorPosition::InWindow(LogicalPosition::new(400.0, 400.0));
    parent.common.mouse_state_mut().left_down = true;
    let _ = parent.process_window_events(0);
    assert!(
        parent
            .get_layout_window()
            .unwrap()
            .transient_windows
            .open_windows()
            .is_empty(),
        "an outside press light-dismisses it"
    );
    parent.snapshot_window_state_baseline("test.release");
    parent.common.mouse_state_mut().left_down = false;
    let _ = parent.process_window_events(0);
}

// ---------------------------------------------------------------------------
// Dialog: HTML `<dialog>` on a transient window (the 2026-09-28 remodel)
// ---------------------------------------------------------------------------

use azul_layout::widgets::dialog::{
    Dialog, DialogClosedBy, DialogOnCancelCallbackType, DialogOnCloseCallbackType, DialogState,
};

/// The app behind the dialog scenarios.
struct DialogProbe {
    open: bool,
    closed_by: DialogClosedBy,
    /// `cancel` calls `prevent_default`.
    keep_open: bool,
    cancels: usize,
    /// The return value of every `close`.
    closes: Vec<String>,
}

extern "C" fn probe_cancel(mut data: RefAny, mut info: CallbackInfo, _state: DialogState) -> Update {
    let keep_open = match data.downcast_mut::<DialogProbe>() {
        Some(mut p) => {
            p.cancels += 1;
            p.keep_open
        }
        None => false,
    };
    if keep_open {
        info.prevent_default();
    }
    Update::DoNothing
}

/// A well-behaved app: `close` drops its `open` flag and rebuilds.
extern "C" fn probe_close(mut data: RefAny, _info: CallbackInfo, state: DialogState) -> Update {
    if let Some(mut p) = data.downcast_mut::<DialogProbe>() {
        p.open = false;
        p.closes.push(state.return_value.as_str().to_string());
    }
    Update::RefreshDom
}

/// The dialog's "OK" control: `dialog.close("ok")`.
extern "C" fn probe_ok(_data: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let _ = Dialog::close_from(&mut info, hit, "ok".into());
    Update::DoNothing
}

/// A focusable stop, then a modal "Delete file" dialog whose content holds
/// a focusable "OK" control.
extern "C" fn dialog_probe_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (open, closed_by) = data
        .downcast_ref::<DialogProbe>()
        .map_or((false, DialogClosedBy::Auto), |p| (p.open, p.closed_by));
    let on_cancel: DialogOnCancelCallbackType = probe_cancel;
    let on_close: DialogOnCloseCallbackType = probe_close;
    let ok = Dom::create_div()
        .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("dialog-ok".into())].into())
        .with_css("width: 80px; height: 24px;".into())
        .with_tab_index(azul_core::dom::TabIndex::Auto)
        .with_callback(
            EventFilter::Hover(azul_core::events::HoverEventFilter::Click),
            data.clone(),
            Callback {
                cb: probe_ok,
                ctx: azul_core::refany::OptionRefAny::None,
            }
            .to_core(),
        )
        .with_child(Dom::create_p_with_text("OK"));
    let stop = Dom::create_div()
        .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("dialog-stop".into())].into())
        .with_css("width: 80px; height: 20px;".into())
        .with_tab_index(azul_core::dom::TabIndex::Auto);
    Dom::create_body().with_child(stop).with_child(
        Dialog::create(
            Dom::create_div()
                .with_child(Dom::create_p_with_text("Delete it?"))
                .with_child(ok),
        )
        .with_title("Delete file".into())
        .with_modal(true)
        .with_open(open)
        .with_closed_by(closed_by)
        .with_on_cancel(data.clone(), on_cancel)
        .with_on_close(data, on_close)
        .dom(),
    )
}

fn dialog_parent(closed_by: DialogClosedBy, keep_open: bool) -> HeadlessWindow {
    let app_data = Arc::new(RefCell::new(RefAny::new(DialogProbe {
        open: false,
        closed_by,
        keep_open,
        cancels: 0,
        closes: Vec::new(),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = dialog_probe_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data);
    parent.regenerate_layout().expect("layout");
    parent
}

fn with_probe<R>(window: &HeadlessWindow, f: impl FnOnce(&mut DialogProbe) -> R) -> R {
    let mut app = window.common.app_data.borrow_mut();
    let mut probe = app
        .downcast_mut::<DialogProbe>()
        .expect("the app state is a DialogProbe");
    f(&mut probe)
}

/// The app shows its modal dialog; returns the popup window the run loop
/// would create, after its first pass (the one that autofocuses).
fn show_probe_dialog(parent: &mut HeadlessWindow) -> (HeadlessWindow, WindowCreateOptions) {
    with_probe(parent, |p| p.open = true);
    relayout(parent);
    let popup_opts = take_queued_popup(parent);
    let mut popup = headless(popup_opts.clone(), parent.common.app_data.clone());
    popup.regenerate_layout().expect("popup layout");
    let _ = popup.process_window_events(0);
    (popup, popup_opts)
}

/// `showModal()`: the dialog's window covers its parent exactly and takes
/// the focus (its first control). Escape runs `cancel`, then closes it: the
/// app hears `close` with an empty return value, and the parent's focus is
/// where it was.
#[test]
fn escape_in_a_modal_dialog_runs_cancel_then_closes_it_and_hands_focus_back() {
    let mut parent = dialog_parent(DialogClosedBy::Auto, false);
    key_down(&mut parent, VirtualKeyCode::Tab, &[], "t.tab");
    keys_up(&mut parent, "t.tab.up");
    let stop = node_with_class(&parent, "dialog-stop");
    assert_eq!(focused(&parent), Some(stop), "premise: Tab focused the stop");

    let (mut popup, popup_opts) = show_probe_dialog(&mut parent);
    assert_eq!(
        popup_opts.window_state.size.dimensions,
        LogicalSize {
            width: 800.0,
            height: 600.0
        },
        "the modal dialog's window covers its parent"
    );
    assert!(
        matches!(
            popup_opts.window_state.position,
            azul_core::window::WindowPosition::RelativeToParentWindow(p) if p.x == 0 && p.y == 0
        ),
        "at the parent's origin: {:?}",
        popup_opts.window_state.position
    );
    assert_eq!(
        focused(&popup),
        Some(node_with_class(&popup, "dialog-ok")),
        "the dialog focused its first control"
    );

    key_down(&mut popup, VirtualKeyCode::Escape, &[], "t.escape");
    assert_eq!(with_probe(&popup, |p| p.cancels), 1, "cancel ran");
    assert!(close_requested(&popup), "then the dialog closed its window");

    parent
        .regenerate_layout()
        .expect("the parent reads the dismissal");
    assert_eq!(
        with_probe(&parent, |p| p.closes.clone()),
        vec![String::new()],
        "close, with the empty return value Escape leaves"
    );
    let _ = parent.process_window_events(0);
    assert_eq!(focused(&parent), Some(stop), "focus is where it was");
}

/// `event.preventDefault()` in `cancel` keeps the dialog open on Escape.
#[test]
fn a_cancel_that_prevents_default_keeps_the_modal_dialog_open() {
    let mut parent = dialog_parent(DialogClosedBy::Auto, true);
    let (mut popup, _) = show_probe_dialog(&mut parent);
    key_down(&mut popup, VirtualKeyCode::Escape, &[], "t.escape");
    assert_eq!(with_probe(&popup, |p| p.cancels), 1, "cancel ran");
    assert!(!close_requested(&popup), "and kept the dialog open");
}

/// `closedby="any"`: a press on a modal dialog's backdrop is a light
/// dismiss (a close request: `cancel` first). A press inside the dialog
/// never is, and without `closedby="any"` the backdrop is inert.
#[test]
fn a_press_on_the_backdrop_closes_a_modal_dialog_only_with_closedby_any() {
    for (closed_by, closes) in [(DialogClosedBy::Any, true), (DialogClosedBy::Auto, false)] {
        let mut parent = dialog_parent(closed_by, false);
        let (mut popup, _) = show_probe_dialog(&mut parent);

        let panel = rect_of_class(&popup, "__azul-native-dialog-panel");
        click_at(
            &mut popup,
            LogicalPosition::new(panel.origin.x + 4.0, panel.origin.y + 4.0),
        );
        assert!(
            !close_requested(&popup),
            "a press inside the dialog ({closed_by:?})"
        );

        click_at(&mut popup, LogicalPosition::new(5.0, 5.0));
        assert_eq!(
            close_requested(&popup),
            closes,
            "a press on the backdrop ({closed_by:?})"
        );
        assert_eq!(
            with_probe(&popup, |p| p.cancels),
            usize::from(closes),
            "a light dismiss is a close request: cancel first ({closed_by:?})"
        );
    }
}

/// The dialog answers Escape with a WINDOW key handler, and the parent lays
/// out the same node (the `<transient-window>` template). An Escape pressed
/// in the parent while the dialog holds the keyboard is forwarded to the
/// dialog's window; the parent's copy must never run `cancel` itself - that
/// would be a second `cancel` against the same app state.
#[test]
fn an_escape_in_the_parent_does_not_reach_the_dialog_it_shows() {
    let mut parent = dialog_parent(DialogClosedBy::Auto, false);
    let (_popup, _) = show_probe_dialog(&mut parent);

    key_down(&mut parent, VirtualKeyCode::Escape, &[], "t.parent.escape");
    keys_up(&mut parent, "t.parent.escape.up");
    assert_eq!(
        with_probe(&parent, |p| p.cancels),
        0,
        "the dialog's key handler ran in the parent's window"
    );
}

/// A control inside the dialog closes it with a return value
/// (`Dialog::close_from`, HTML `close("ok")`): no `cancel`, and the app's
/// `close` sees "ok".
#[test]
fn a_control_inside_the_dialog_closes_it_with_its_return_value() {
    let mut parent = dialog_parent(DialogClosedBy::Auto, false);
    let (mut popup, _) = show_probe_dialog(&mut parent);

    let ok = rect_of_class(&popup, "dialog-ok");
    click_at(
        &mut popup,
        LogicalPosition::new(
            ok.origin.x + ok.size.width / 2.0,
            ok.origin.y + ok.size.height / 2.0,
        ),
    );
    assert!(close_requested(&popup), "OK closed the dialog");
    assert_eq!(with_probe(&popup, |p| p.cancels), 0, "close() runs no cancel");

    parent
        .regenerate_layout()
        .expect("the parent reads the dismissal");
    assert_eq!(
        with_probe(&parent, |p| p.closes.clone()),
        vec!["ok".to_string()]
    );
}

// ---------------------------------------------------------------------------
// Modal: reopening after its close button closed it
// ---------------------------------------------------------------------------

/// The app behind the modal scenario: whether it shows the modal, and how
/// often the modal told it it closed.
struct ModalProbe {
    open: bool,
    closes: usize,
}

/// A well-behaved app: the modal's `on_close` drops the `open` flag.
extern "C" fn modal_probe_closed(
    mut data: RefAny,
    _info: CallbackInfo,
    _state: azul_layout::widgets::modal::ModalState,
) -> Update {
    if let Some(mut p) = data.downcast_mut::<ModalProbe>() {
        p.open = false;
        p.closes += 1;
    }
    Update::RefreshDom
}

/// The widgets demo's Modal, opened and closed through the app's state.
extern "C" fn modal_probe_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_layout::widgets::modal::{Modal, ModalOnCloseCallbackType};
    let open = data.downcast_ref::<ModalProbe>().map_or(false, |p| p.open);
    let on_close: ModalOnCloseCallbackType = modal_probe_closed;
    Dom::create_body().with_child(
        Modal::create(Dom::create_p_with_text("Modal body goes here."))
            .with_title("Example dialog".into())
            .with_open(open)
            .with_close_button(true)
            .with_on_close(data.clone(), on_close)
            .dom(),
    )
}

fn with_modal_probe<R>(window: &HeadlessWindow, f: impl FnOnce(&mut ModalProbe) -> R) -> R {
    let mut app = window.common.app_data.borrow_mut();
    let mut probe = app
        .downcast_mut::<ModalProbe>()
        .expect("the app state is a ModalProbe");
    f(&mut probe)
}

/// The rect of the first node whose classes contain `class` in `window`,
/// if it is laid out there with a real box.
fn laid_out_rect(window: &HeadlessWindow, class: &str) -> Option<azul_core::geom::LogicalRect> {
    let lw = window.get_layout_window()?;
    let root = lw.layout_results.get(&DomId::ROOT_ID)?;
    let nodes = root.styled_dom.node_data.as_container();
    let n = nodes.linear_iter().find(|n| {
        nodes
            .get(*n)
            .is_some_and(|nd| format!("{:?}", nd.get_ids_and_classes()).contains(class))
    })?;
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(n)),
    })
    .filter(|r| r.size.width > 0.0 && r.size.height > 0.0)
}

/// Whether the modal is on screen: its panel laid out in the parent, or an
/// open transient window.
fn modal_shown(parent: &HeadlessWindow) -> bool {
    let popups = parent
        .get_layout_window()
        .map_or(0, |lw| lw.transient_windows.open_windows().len());
    popups > 0 || laid_out_rect(parent, "__azul-native-modal-panel").is_some()
}

fn center_of(r: azul_core::geom::LogicalRect) -> LogicalPosition {
    LogicalPosition::new(
        r.origin.x + r.size.width / 2.0,
        r.origin.y + r.size.height / 2.0,
    )
}

/// The app opens its modal, the user closes it with its close button (the
/// app hears `on_close` and drops its flag), then the app opens it again: it
/// must show again.
#[test]
fn a_modal_the_app_reopens_after_its_close_button_closed_it_shows_again() {
    let app_data = Arc::new(RefCell::new(RefAny::new(ModalProbe {
        open: false,
        closes: 0,
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = modal_probe_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut parent = headless(options, app_data);
    parent.regenerate_layout().expect("layout");
    assert!(!modal_shown(&parent), "premise: closed until the app opens it");

    with_modal_probe(&parent, |p| p.open = true);
    relayout(&mut parent);
    assert!(modal_shown(&parent), "the app opened it");

    // The user presses the close button, wherever the modal is shown: in
    // the parent's own layout, or in the modal's window.
    if let Some(x) = laid_out_rect(&parent, "-close") {
        click_at(&mut parent, center_of(x));
    } else {
        let popup_opts = take_queued_popup(&mut parent);
        let mut popup = headless(popup_opts, parent.common.app_data.clone());
        popup.regenerate_layout().expect("popup layout");
        let _ = popup.process_window_events(0);
        let x = laid_out_rect(&popup, "-close").expect("the close button is in the modal's window");
        click_at(&mut popup, center_of(x));
    }
    parent.regenerate_layout().expect("the app's rebuild");
    assert_eq!(
        with_modal_probe(&parent, |p| p.closes),
        1,
        "the app heard the close"
    );
    assert!(!modal_shown(&parent), "the close button closed it");

    with_modal_probe(&parent, |p| p.open = true);
    relayout(&mut parent);
    assert!(modal_shown(&parent), "the reopened modal shows again");
}
