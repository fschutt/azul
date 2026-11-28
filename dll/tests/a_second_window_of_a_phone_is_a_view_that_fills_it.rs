//! A second window of the app on a window with ONE surface (a phone) is a view that fills the
//! surface, end to end through the headless shell: it is made as a window of the app (its own
//! layout window, state and callbacks, so `close_window()` inside it means IT), but it takes the
//! size, the density and the display of the surface over, whatever the app asked for a window of its
//! own - and on a desktop it is the window the app asked for.
//!
//! The window is a desktop headless window put on a phone display (`WindowFlags::display_preset`).

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul::desktop::shell2::{
    common::event::{PlatformWindow, WindowStateSource},
    headless::HeadlessWindow,
};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, Update},
    display_features::{DisplayFeatures, DisplayPreset},
    dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, IdOrClass},
    geom::{LogicalPosition, LogicalSize},
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::AppConfig,
    window::{CursorPosition, DarkLightMode},
};
use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::SafeAreaInsets,
};
use azul_layout::{
    callbacks::{Callback, CallbackInfo},
    window_state::WindowCreateOptions,
};
use rust_fontconfig::FcFontCache;

/// What the app counts: the windows it made.
struct Mailer {
    composes_opened: Arc<AtomicUsize>,
}

/// The second window: a page with a button that closes the window it is in.
extern "C" fn compose_page(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Class("discard".into())].into())
            .with_css("width: 120px; height: 48px; background: #cc3333;")
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                RefAny::new(()),
                Callback::from_ptr(on_discard).to_core(),
            ),
    )
}

extern "C" fn on_discard(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.close_window();
    Update::DoNothing
}

/// The first window: a button that opens the second, as AzMail's "New message" does.
extern "C" fn inbox_page(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Class("new-message".into())].into())
            .with_css("width: 160px; height: 48px; margin: 40px; background: #3366cc;")
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                data,
                Callback::from_ptr(on_new_message).to_core(),
            ),
    )
}

extern "C" fn on_new_message(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(app) = data.downcast_ref::<Mailer>() {
        app.composes_opened.fetch_add(1, Ordering::SeqCst);
    }
    let mut options = WindowCreateOptions::default();
    let page: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = compose_page;
    options.window_state.layout_callback = LayoutCallback::create(page);
    options.window_state.window_id = "compose-1".into();
    // What the app asks of a window of its own: a desktop window, not a phone's.
    options.window_state.size.dimensions = LogicalSize::new(880.0, 700.0);
    info.create_window(options);
    Update::DoNothing
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

fn inbox(preset: DisplayPreset) -> HeadlessWindow {
    let app_data = Arc::new(RefCell::new(RefAny::new(Mailer {
        composes_opened: Arc::new(AtomicUsize::new(0)),
    })));
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize::new(412.0, 915.0);
    options.window_state.flags.display_preset = preset;
    let page: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = inbox_page;
    options.window_state.layout_callback = LayoutCallback::create(page);
    options.window_state.window_id = "inbox".into();
    let mut window = headless(options, app_data);
    window.regenerate_layout().expect("layout");
    window
}

fn centre_of_class(window: &HeadlessWindow, class: &str) -> LogicalPosition {
    let lw = window.get_layout_window().unwrap();
    let nodes = lw.layout_results[&DomId::ROOT_ID].styled_dom.node_data.as_container();
    let node = (0..nodes.len())
        .map(azul_core::id::NodeId::new)
        .find(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .expect("the control");
    let rect = lw
        .get_node_rect_in_viewport(DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node)),
        })
        .expect("laid out");
    LogicalPosition::new(
        rect.origin.x + rect.size.width / 2.0,
        rect.origin.y + rect.size.height / 2.0,
    )
}

/// Press-and-release at `pos` through the real event path (hit test included).
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
}

/// THE flow: the tap opens the app's second window, and on a phone it fills the surface.
#[test]
fn the_second_window_of_a_phone_takes_the_surface_over() {
    let mut window = inbox(DisplayPreset::Pixel8);
    let at = centre_of_class(&window, "new-message");
    tap_at(&mut window, at);
    window.pump_children();

    assert_eq!(window.children.len(), 1, "the app's second window exists");
    let compose = &window.children[0];
    let state = compose.common.current_window_state();
    assert_eq!(state.window_id.as_str(), "compose-1", "it is the window the app made, by its id");
    assert_eq!(
        state.size.dimensions,
        LogicalSize::new(412.0, 915.0),
        "and it is as big as the surface, not as the app asked (880 x 700)"
    );
    assert_eq!(state.flags.display_preset, DisplayPreset::Pixel8, "on the same display");
}

/// On a desktop the window is the window the app asked for.
#[test]
fn the_same_window_on_a_desktop_is_the_size_the_app_asked() {
    let mut window = inbox(DisplayPreset::Off);
    let at = centre_of_class(&window, "new-message");
    tap_at(&mut window, at);
    window.pump_children();

    assert_eq!(window.children.len(), 1);
    assert_eq!(
        window.children[0].common.current_window_state().size.dimensions,
        LogicalSize::new(880.0, 700.0)
    );
}

/// `close_window()` inside the view closes the view, not the window under it.
#[test]
fn closing_the_view_leaves_the_window_under_it() {
    let mut window = inbox(DisplayPreset::Pixel8);
    let at = centre_of_class(&window, "new-message");
    tap_at(&mut window, at);
    window.pump_children();
    assert_eq!(window.children.len(), 1);

    // The first window's page is still the first window's.
    {
        let child = &mut window.children[0];
        child.regenerate_layout().expect("the view's layout");
        let discard = centre_of_class(child, "discard");
        tap_at(child, discard);
    }
    // The close the app asked for is the window's close protocol, run by the loop's turn.
    window.pump_children();
    assert!(window.children.is_empty(), "the view is gone");
    assert!(window.is_open(), "and the window under it is not");
}

fn bar_of(px: f32) -> SafeAreaInsets {
    SafeAreaInsets {
        top: OptionPixelValue::Some(PixelValue::px(px)),
        ..SafeAreaInsets::default()
    }
}

fn opposite(mode: DarkLightMode) -> DarkLightMode {
    match mode {
        DarkLightMode::Dark => DarkLightMode::Light,
        DarkLightMode::Light => DarkLightMode::Dark,
    }
}

/// A view pushed over a window has the bars and the cutout the window under it was told about, from
/// its first layout: they are the surface's.
#[test]
fn a_pushed_view_has_the_bars_of_the_surface_from_the_start() {
    let mut under = inbox(DisplayPreset::Pixel8);
    let mut view = inbox(DisplayPreset::Pixel8);
    {
        let lw = under.common.layout_window.as_mut().unwrap();
        lw.safe_area_insets = bar_of(48.0);
        lw.display_features = DisplayFeatures::simulate(DisplayPreset::Pixel8, LogicalSize::new(412.0, 915.0));
    }
    assert_eq!(
        view.common.layout_window.as_ref().unwrap().safe_area_insets,
        SafeAreaInsets::default(),
        "before: the view knows nothing of the surface"
    );

    view.common.adopt_surface_of(&under.common);

    let under_lw = under.common.layout_window.as_ref().unwrap();
    let view_lw = view.common.layout_window.as_ref().unwrap();
    assert_eq!(view_lw.safe_area_insets, under_lw.safe_area_insets);
    assert_eq!(view_lw.safe_area_insets.top, OptionPixelValue::Some(PixelValue::px(48.0)));
    assert_eq!(view_lw.display_features, under_lw.display_features);
}

/// When the view closes, the window under it takes over what the system told the view meanwhile: a
/// turn of the phone, the bars, the night - it was the view the system spoke to, not the window
/// that waited.
#[test]
fn the_window_that_comes_back_takes_over_what_the_surface_did_meanwhile() {
    let mut under = inbox(DisplayPreset::Pixel8);
    let mut view = inbox(DisplayPreset::Pixel8);
    let night = opposite(under.common.current_window_state().mode);
    view.common
        .update_window_state(WindowStateSource::Os, |state| {
            state.size.dimensions = LogicalSize::new(915.0, 412.0);
        });
    view.common.write_shown_mode(night);
    view.common.layout_window.as_mut().unwrap().safe_area_insets = bar_of(24.0);

    under.snapshot_window_state_baseline("test.view.pop");
    let moved = under.common.take_over_surface_of(&view.common);

    let state = under.common.current_window_state();
    assert_eq!(state.size.dimensions, LogicalSize::new(915.0, 412.0), "the phone turned");
    assert_eq!(state.mode, night, "and it is night");
    assert!(moved, "the mode moved: the window is restyled, not only built again");
    assert_eq!(
        under.common.layout_window.as_ref().unwrap().safe_area_insets.top,
        OptionPixelValue::Some(PixelValue::px(24.0)),
        "the bars are the new ones"
    );
}

/// A view closed over a surface that did not change is a pop like any other: nothing to restyle.
#[test]
fn a_surface_that_did_not_change_moves_nothing() {
    let mut under = inbox(DisplayPreset::Pixel8);
    let view = inbox(DisplayPreset::Pixel8);
    under.snapshot_window_state_baseline("test.view.pop");
    assert!(!under.common.take_over_surface_of(&view.common));
    assert_eq!(under.common.current_window_state().size.dimensions, LogicalSize::new(412.0, 915.0));
}
