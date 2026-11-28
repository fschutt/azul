//! A window with ONE surface (a phone) shows its `<transient-window>`s inside itself, end to end
//! through the headless shell: no child window is queued, the node is laid out over the page at the
//! box the engine placed, a press outside it or Escape dismisses it and tells the app, a press inside
//! it does not.
//!
//! The window is a desktop headless window put on a phone display (`WindowFlags::display_preset`,
//! the test hook of the display API): the same file of a scenario runs on every host. It runs the real
//! `PlatformWindow::regenerate_layout` and `process_window_events`, as every backend does.

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul::desktop::shell2::{common::event::PlatformWindow, headless::HeadlessWindow};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, RelayoutReason, Update},
    display_features::DisplayPreset,
    dom::{Dom, DomId, DomNodeId, NodeData, NodeType},
    events::{ComponentEventFilter, EventFilter},
    geom::{LogicalPosition, LogicalSize},
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

/// App state: whether the popup is open, and how often its `Dismissed` handler ran.
struct PickerState {
    open: bool,
    dismissed_calls: Arc<AtomicUsize>,
}

/// The app's reaction to the engine closing the popup for it: drop the flag.
extern "C" fn on_dismissed(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<PickerState>() {
        s.dismissed_calls.fetch_add(1, Ordering::SeqCst);
        s.open = false;
    }
    Update::RefreshDom
}

/// A swatch at (40, 40), 60 x 24, with a popup anchored to it: a 240 x 160 panel.
extern "C" fn picker_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let open = data.downcast_ref::<PickerState>().is_some_and(|s| s.open);
    let cfg = if open {
        TransientWindowConfig::opened()
    } else {
        TransientWindowConfig::closed()
    }
    .with_dismiss(TransientDismiss::Outside);
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
            .with_ids_and_classes(
                vec![azul_core::dom::IdOrClass::Class("panel".into())].into(),
            )
            .with_css("width: 240px; height: 160px; background: white;".into()),
    );
    let swatch = Dom::create_div()
        .with_css("width: 60px; height: 24px; margin: 40px; background: #e66465;".into())
        .with_child(popup);
    Dom::create_body()
        .with_css("margin: 0;".into())
        .with_child(swatch)
}

fn headless(options: WindowCreateOptions, app_data: Arc<RefCell<RefAny>>) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    let mut handle = IconProviderHandle::default();
    handle.set_resolver(azul_layout::icon::default_icon_resolver);
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

/// A headless window of a Pixel 8's size, put on its display: a window with one surface.
fn phone(open: bool, preset: DisplayPreset) -> HeadlessWindow {
    let app_data = Arc::new(RefCell::new(RefAny::new(PickerState {
        open,
        dismissed_calls: Arc::new(AtomicUsize::new(0)),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize::new(412.0, 915.0);
    options.window_state.flags.display_preset = preset;
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = picker_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    headless(options, app_data)
}

fn dismissed_calls(window: &HeadlessWindow) -> usize {
    let mut app = window.common.app_data.borrow_mut();
    app.downcast_ref::<PickerState>()
        .map_or(usize::MAX, |s| s.dismissed_calls.load(Ordering::SeqCst))
}

fn app_says_open(window: &HeadlessWindow) -> bool {
    let mut app = window.common.app_data.borrow_mut();
    app.downcast_ref::<PickerState>().is_some_and(|s| s.open)
}

fn open_windows(window: &HeadlessWindow) -> usize {
    window
        .get_layout_window()
        .unwrap()
        .transient_windows
        .open_windows()
        .len()
}

fn panel_rect(window: &HeadlessWindow) -> Option<azul_core::geom::LogicalRect> {
    let lw = window.get_layout_window().unwrap();
    let nodes = lw.layout_results[&DomId::ROOT_ID].styled_dom.node_data.as_container();
    let panel = (0..nodes.len())
        .map(azul_core::id::NodeId::new)
        .find(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some("panel")))
        })?;
    lw.get_node_rect_in_viewport(DomNodeId {
        dom: DomId::ROOT_ID,
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(panel)),
    })
}

fn press_at(window: &mut HeadlessWindow, x: f32, y: f32) {
    window.snapshot_window_state_baseline("test.press");
    window.common.mouse_state_mut().cursor_position =
        CursorPosition::InWindow(LogicalPosition::new(x, y));
    window.common.mouse_state_mut().left_down = true;
    let _ = window.process_window_events(0);
    window.snapshot_window_state_baseline("test.release");
    window.common.mouse_state_mut().left_down = false;
    let _ = window.process_window_events(0);
}

/// On a phone no window is made for the popup: the shell has nothing to queue, the engine shows the
/// node itself over the page, below its swatch, at the size it measured.
#[test]
fn a_popup_of_a_phone_is_no_window_but_a_box_in_the_page() {
    let mut parent = phone(true, DisplayPreset::Pixel8);
    parent.regenerate_layout().expect("layout");

    assert!(
        parent.pending_window_creates.is_empty(),
        "a phone opens no second window: {}",
        parent.pending_window_creates.len()
    );
    assert_eq!(open_windows(&parent), 1);
    let w = parent.get_layout_window().unwrap().transient_windows.open_windows()[0].clone();
    assert!(w.overlay.is_some() && w.has_no_surface(), "an overlay: {w:?}");

    let rect = panel_rect(&parent).expect("the panel is laid out in this window");
    assert!(
        (rect.size.width - 240.0).abs() < 1.0 && (rect.size.height - 160.0).abs() < 1.0,
        "at the size the engine measured: {rect:?}"
    );
    assert!((rect.origin.x - 40.0).abs() < 1.0, "left edges lined up with the swatch: {rect:?}");
    // The Pixel 8 has a 48 px status bar: the page starts under it, the swatch is 40 px in.
    assert!(
        (rect.origin.y - (48.0 + 40.0 + 24.0)).abs() < 1.5,
        "right under the swatch: {rect:?}"
    );
}

/// The same window on a desktop (no preset) is as it always was: a child window is queued.
#[test]
fn the_same_popup_on_a_desktop_is_still_a_window() {
    let mut parent = phone(true, DisplayPreset::Off);
    parent.regenerate_layout().expect("layout");
    assert_eq!(parent.pending_window_creates.len(), 1);
    assert!(panel_rect(&parent).is_none(), "its content is not laid out in this window");
}

/// A press outside the box dismisses it, once: the node is held closed although the app still says
/// `open`, and the app's `Dismissed` handler runs and drops its flag.
#[test]
fn a_press_outside_a_phones_popup_dismisses_it_and_tells_the_app() {
    let mut parent = phone(true, DisplayPreset::Pixel8);
    parent.regenerate_layout().expect("layout");
    assert_eq!(open_windows(&parent), 1);

    // Inside the panel: nothing.
    press_at(&mut parent, 100.0, 150.0);
    assert_eq!(open_windows(&parent), 1, "a press inside it keeps it");

    // Outside it.
    press_at(&mut parent, 300.0, 800.0);
    assert_eq!(open_windows(&parent), 0, "the press outside closed it");

    parent.regenerate_layout().expect("drain the Dismissed event");
    assert_eq!(dismissed_calls(&parent), 1, "the app heard it once");
    assert!(!app_says_open(&parent), "and dropped its flag");
    assert!(panel_rect(&parent).is_none(), "it is gone from the page");
    assert!(parent.pending_window_creates.is_empty());
}

/// Escape dismisses it.
#[test]
fn escape_dismisses_a_phones_popup() {
    let mut parent = phone(true, DisplayPreset::Pixel8);
    parent.regenerate_layout().expect("layout");
    parent.snapshot_window_state_baseline("test.escape");
    parent.common.keyboard_state_mut().pressed_virtual_keycodes =
        vec![VirtualKeyCode::Escape].into();
    let _ = parent.process_window_events(0);
    assert_eq!(open_windows(&parent), 0);
    parent.regenerate_layout().expect("drain");
    assert_eq!(dismissed_calls(&parent), 1);
}

/// Flipping the app's `open` across rebuilds shows and hides the box in the SAME window.
#[test]
fn flipping_open_shows_and_hides_the_box() {
    let mut parent = phone(false, DisplayPreset::Pixel8);
    parent.regenerate_layout().expect("layout 1: closed");
    assert!(panel_rect(&parent).is_none());

    {
        let mut app = parent.common.app_data.borrow_mut();
        app.downcast_mut::<PickerState>().unwrap().open = true;
    }
    parent.request_regeneration(RelayoutReason::RefreshDom);
    parent.regenerate_layout().expect("layout 2: open");
    assert!(panel_rect(&parent).is_some(), "shown");
    assert!(parent.pending_window_creates.is_empty());

    {
        let mut app = parent.common.app_data.borrow_mut();
        app.downcast_mut::<PickerState>().unwrap().open = false;
    }
    parent.request_regeneration(RelayoutReason::RefreshDom);
    parent.regenerate_layout().expect("layout 3: closed");
    assert!(panel_rect(&parent).is_none(), "hidden again");
}
