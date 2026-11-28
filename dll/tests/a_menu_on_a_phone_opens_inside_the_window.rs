//! A menu an app opens on a window with ONE surface (a phone) opens inside the window, end to end
//! through the headless shell: no window is queued, the menu's rows are in the page, a tap on a row
//! runs the item in this window and closes the menu, a press outside closes it, and a secondary
//! press on a node with a context menu opens that menu where the pointer is.
//!
//! The window is a desktop headless window put on a phone display (`WindowFlags::display_preset`);
//! it runs the real `regenerate_layout` and `process_window_events` of every backend.

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul::desktop::shell2::{common::event::PlatformWindow, headless::HeadlessWindow};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, Update},
    display_features::DisplayPreset,
    dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, IdOrClass},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    icon::{IconProviderHandle, SharedIconProvider},
    menu::{Menu, MenuItem, StringMenuItem},
    refany::RefAny,
    resources::AppConfig,
    window::CursorPosition,
};
use azul_layout::{
    callbacks::{Callback, CallbackInfo},
    window_state::WindowCreateOptions,
};
use rust_fontconfig::FcFontCache;

/// What the page counts: the picks of each item, and how often the control was tapped.
struct MenuState {
    first: Arc<AtomicUsize>,
    second: Arc<AtomicUsize>,
    control_taps: Arc<AtomicUsize>,
}

extern "C" fn on_first(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(s) = data.downcast_ref::<MenuState>() {
        s.first.fetch_add(1, Ordering::SeqCst);
    }
    Update::DoNothing
}

extern "C" fn on_second(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(s) = data.downcast_ref::<MenuState>() {
        s.second.fetch_add(1, Ordering::SeqCst);
    }
    Update::DoNothing
}

fn items(data: &RefAny) -> Menu {
    Menu::create(
        vec![
            MenuItem::String(
                StringMenuItem::create("First".into()).with_callback(data.clone(), on_first as usize),
            ),
            MenuItem::String(
                StringMenuItem::create("Second".into()).with_callback(data.clone(), on_second as usize),
            ),
        ]
        .into(),
    )
}

/// The control: a tap opens its menu, as a drop-down's does.
extern "C" fn on_control(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(s) = data.downcast_ref::<MenuState>() {
        s.control_taps.fetch_add(1, Ordering::SeqCst);
    }
    info.open_menu_for_hit_node(items(&data));
    Update::DoNothing
}

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

/// A control at (40, 40), 160 x 40, that opens a menu when tapped; a box at (40, 300) that has a
/// context menu of its own.
extern "C" fn page(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let control = Dom::create_div()
        .with_ids_and_classes(class("control"))
        .with_css("width: 160px; height: 40px; margin: 40px; background: #3366cc;")
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            data.clone(),
            Callback::from_ptr(on_control).to_core(),
        );
    let with_menu = Dom::create_div()
        .with_ids_and_classes(class("target"))
        .with_css("width: 200px; height: 100px; margin-left: 40px; margin-top: 200px; background: #dddddd;")
        .with_context_menu(items(&data));
    Dom::create_body()
        .with_css("margin: 0;")
        .with_child(control)
        .with_child(with_menu)
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
fn phone(preset: DisplayPreset) -> HeadlessWindow {
    let app_data = Arc::new(RefCell::new(RefAny::new(MenuState {
        first: Arc::new(AtomicUsize::new(0)),
        second: Arc::new(AtomicUsize::new(0)),
        control_taps: Arc::new(AtomicUsize::new(0)),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize::new(412.0, 915.0);
    options.window_state.flags.display_preset = preset;
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = page;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut window = headless(options, app_data);
    window.regenerate_layout().expect("layout");
    window
}

fn count(window: &HeadlessWindow, pick: impl Fn(&MenuState) -> &Arc<AtomicUsize>) -> usize {
    let mut app = window.common.app_data.borrow_mut();
    app.downcast_ref::<MenuState>()
        .map_or(usize::MAX, |s| pick(&s).load(Ordering::SeqCst))
}

fn rect_of_class(window: &HeadlessWindow, class: &str, nth: usize) -> Option<LogicalRect> {
    let lw = window.get_layout_window().unwrap();
    let nodes = lw.layout_results[&DomId::ROOT_ID].styled_dom.node_data.as_container();
    let node = (0..nodes.len())
        .map(azul_core::id::NodeId::new)
        .filter(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .nth(nth)?;
    lw.get_node_rect_in_viewport(DomNodeId {
        dom: DomId::ROOT_ID,
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node)),
    })
}

fn centre(rect: LogicalRect) -> LogicalPosition {
    LogicalPosition::new(
        rect.origin.x + rect.size.width / 2.0,
        rect.origin.y + rect.size.height / 2.0,
    )
}

/// Press-and-release at `pos` through the real event path (hit test included), then the
/// regeneration the pass asked for, as the shell's frame does.
fn tap_at(window: &mut HeadlessWindow, pos: LogicalPosition) {
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
    window.regenerate_layout().expect("the regeneration the tap asked for");
}

/// A secondary press at `pos`, then the regeneration it asked for.
fn right_click_at(window: &mut HeadlessWindow, pos: LogicalPosition) {
    window.snapshot_window_state_baseline("test.move");
    window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(pos);
    window.update_hit_test_at(pos);
    let _ = window.process_window_events(0);
    window.snapshot_window_state_baseline("test.down");
    window.common.mouse_state_mut().right_down = true;
    let _ = window.process_window_events(0);
    window.snapshot_window_state_baseline("test.up");
    window.common.mouse_state_mut().right_down = false;
    let _ = window.process_window_events(0);
    window.regenerate_layout().expect("the regeneration the click asked for");
}

fn open_menus(window: &HeadlessWindow) -> Vec<azul_layout::transient_reconcile::PopupInfo> {
    window
        .get_layout_window()
        .unwrap()
        .open_popups()
        .into_iter()
        .filter(|p| p.is_menu)
        .collect()
}

/// THE flow: tap the control, its menu is in the page below it; tap a row, the item runs in this
/// window and the menu is gone.
#[test]
fn a_tap_on_the_control_opens_its_menu_in_the_page_and_a_tap_on_a_row_picks() {
    let mut window = phone(DisplayPreset::Pixel8);
    assert!(open_menus(&window).is_empty());

    let control = rect_of_class(&window, "control", 0).expect("the control is laid out");
    tap_at(&mut window, centre(control));
    assert_eq!(count(&window, |s| &s.control_taps), 1);
    assert!(window.pending_window_creates.is_empty(), "a phone opens no window for a menu");
    let menus = open_menus(&window);
    assert_eq!(menus.len(), 1, "the menu is open: {menus:?}");
    let rect = menus[0].rect.expect("an overlay has a box");
    assert!(
        rect.origin.y >= control.origin.y + control.size.height - 1.0,
        "below the control: {rect:?} {control:?}"
    );

    let second = rect_of_class(&window, "__azul-menu-item", 1).expect("the second row");
    assert!(second.size.height >= 48.0, "a finger high: {second:?}");
    tap_at(&mut window, centre(second));
    assert_eq!(count(&window, |s| &s.second), 1, "the item ran");
    assert_eq!(count(&window, |s| &s.first), 0, "and only that one");
    assert!(open_menus(&window).is_empty(), "the menu closed with the pick");
    assert!(
        rect_of_class(&window, "__azul-menu-item", 0).is_none(),
        "and is gone from the page"
    );
}

/// A press outside the menu closes it and runs nothing.
#[test]
fn a_press_outside_the_menu_closes_it_without_a_pick() {
    let mut window = phone(DisplayPreset::Pixel8);
    let control = rect_of_class(&window, "control", 0).unwrap();
    tap_at(&mut window, centre(control));
    assert_eq!(open_menus(&window).len(), 1);

    tap_at(&mut window, LogicalPosition::new(380.0, 800.0));
    assert!(open_menus(&window).is_empty());
    assert_eq!(count(&window, |s| &s.first) + count(&window, |s| &s.second), 0);
}

/// The control tapped again closes its menu, instead of opening a second one.
#[test]
fn a_second_tap_on_the_control_closes_its_menu() {
    let mut window = phone(DisplayPreset::Pixel8);
    let control = rect_of_class(&window, "control", 0).unwrap();
    tap_at(&mut window, centre(control));
    assert_eq!(open_menus(&window).len(), 1);
    tap_at(&mut window, centre(control));
    assert_eq!(count(&window, |s| &s.control_taps), 2);
    assert!(open_menus(&window).is_empty(), "closed by asking again");
}

/// The secondary press on a node with a context menu opens it inside the window, at the pointer.
#[test]
fn a_secondary_press_opens_the_context_menu_at_the_pointer() {
    let mut window = phone(DisplayPreset::Pixel8);
    let target = rect_of_class(&window, "target", 0).expect("the box with the menu");
    let at = LogicalPosition::new(target.origin.x + 20.0, target.origin.y + 20.0);
    right_click_at(&mut window, at);

    assert!(window.pending_window_creates.is_empty());
    let menus = open_menus(&window);
    assert_eq!(menus.len(), 1, "the box's menu is open: {menus:?}");
    let rect = menus[0].rect.expect("a box");
    assert!((rect.origin.x - at.x).abs() < 2.0, "at the pointer: {rect:?} {at:?}");
}

/// The same window on a desktop (no preset) is as it always was: no menu overlay is made - the
/// shell shows the menu its own way (here the headless shell's menu window).
#[test]
fn the_same_menu_on_a_desktop_is_no_overlay() {
    let mut window = phone(DisplayPreset::Off);
    let control = rect_of_class(&window, "control", 0).unwrap();
    tap_at(&mut window, centre(control));
    assert!(open_menus(&window).is_empty(), "no overlay on a window that opens windows");
    assert!(
        window.get_layout_window().unwrap().menu_overlays.is_empty(),
        "nothing recorded"
    );
}
