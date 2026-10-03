//! The AZ_E2E / AZ_DEBUG host (HEADLESS6, 2026-10-03): what a script can see
//! and drive through the headless backend.
//!
//! The /e2e corpus runs two ways: in-process (`azul_layout::e2e::runner`, the
//! `layout/tests/e2e_json.rs` gate) and against a real app
//! (`AZ_BACKEND=headless AZ_E2E=<dir> AzPaint`), which goes through THIS
//! backend. Every op the in-process runner answers must be answerable here
//! too, or a scenario that is green in CI is red against an app.

use super::*;

/// `<body>` with one silver 120x30 box: something to paint.
extern "C" fn one_box_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_css::{
        dynamic_selector::CssPropertyWithConditions,
        props::{
            layout::dimensions::{LayoutHeight, LayoutWidth},
            property::CssProperty,
        },
    };
    Dom::create_body().with_child(
        Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(120.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(30.0))),
            ]
            .into(),
        ),
    )
}

/// `assert_damage_sound { "pixel_identity": true }` compares the frame the
/// damage-driven path PAINTED against an independent full repaint. The
/// in-process runner published that frame; this backend did not, so the four
/// corpus scenarios that ask (bug-slider-thumb-trail, op-resize-grow-
/// exposed-strip, op-resize-grow-reflow, op-resize-shrink-stays-full) failed
/// against every app with "this host does not publish the damage-driven
/// framebuffer".
#[cfg(all(
    feature = "cpurender",
    any(feature = "debug-server", feature = "e2e-scripting")
))]
#[test]
fn a_headless_window_publishes_the_frame_it_painted_for_the_pixel_identity_check() {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, one_box_layout);
    // What `AZ_E2E` / `AZ_DEBUG` switch on when the window opens.
    window.publish_presented_frame = true;
    window.regenerate_layout().expect("a painted frame");
    let _ = window.common.take_regeneration();

    let frame = window
        .cpu_backend
        .last_frame
        .as_ref()
        .expect("the CPU backend painted a frame");
    let lw = window.common.layout_window.as_ref().expect("layout window");
    let (w, h, rgba) = debug_server::e2e_presented_frame(lw)
        .expect("the painted frame is published for the pixel-identity check");
    assert_eq!((w, h), (frame.width(), frame.height()));
    assert!(
        rgba == frame.data(),
        "the published pixels are the painted ones"
    );
}

/// The copy costs a frame's worth of memory traffic per present, so a window
/// no script can ask about does not publish.
#[cfg(all(
    feature = "cpurender",
    any(feature = "debug-server", feature = "e2e-scripting")
))]
#[test]
fn a_headless_window_no_script_drives_publishes_no_frame() {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, one_box_layout);
    window.publish_presented_frame = false;
    window.regenerate_layout().expect("a painted frame");
    let _ = window.common.take_regeneration();
    let lw = window.common.layout_window.as_ref().expect("layout window");
    assert!(debug_server::e2e_presented_frame(lw).is_none());
}

/// What [`record_window_id_layout`] saw.
struct SeenWindowId {
    id: String,
}

/// Records the window id its `layout()` was called for.
extern "C" fn record_window_id_layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    if let Some(mut seen) = data.downcast_mut::<SeenWindowId>() {
        seen.id = info.get_window_id().as_str().to_string();
    }
    Dom::create_body()
}

/// CAL3: AzCalendar could open only ONE event editor, because a layout
/// callback could not tell which window it was building. A window opened
/// with an id (`WindowCreateOptions.window_state.window_id`) hands it to
/// every `layout()` of that window.
#[test]
fn a_layout_callback_is_told_which_window_it_builds() {
    let state = Arc::new(RefCell::new(RefAny::new(SeenWindowId {
        id: String::new(),
    })));
    let mut window = make_window_with(&state, record_window_id_layout);
    window
        .common
        .update_window_state(event::WindowStateSource::App, |ws| {
            ws.window_id = "azcalendar-editor-2".into();
        });
    window.regenerate_layout().expect("a layout pass");
    let _ = window.common.take_regeneration();

    let seen = state
        .borrow_mut()
        .downcast_ref::<SeenWindowId>()
        .map(|s| s.id.clone())
        .expect("the app state");
    assert_eq!(seen, "azcalendar-editor-2");
}

/// A two-item context menu.
fn copy_paste_menu() -> azul_core::menu::Menu {
    use azul_core::menu::{Menu, MenuItem, StringMenuItem};
    Menu::create(
        vec![
            MenuItem::String(StringMenuItem::create("Copy".into())),
            MenuItem::String(StringMenuItem::create("Paste".into())),
        ]
        .into(),
    )
}

/// Every text a window's root DOM shows.
fn texts_of(window: &HeadlessWindow) -> Vec<String> {
    let lw = window.common.layout_window.as_ref().expect("layout window");
    lw.layout_results
        .get(&azul_core::dom::DomId::ROOT_ID)
        .map(|r| {
            r.styled_dom
                .node_data
                .as_ref()
                .iter()
                .filter_map(|n| n.get_node_type().get_text())
                .map(|t| t.as_str().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// A laid-out window with nothing open.
fn settled_window() -> HeadlessWindow {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, one_box_layout);
    window.regenerate_layout().expect("a layout pass");
    let _ = window.common.take_regeneration();
    window
}

/// DRIVE2: `show_menu_from_callback` was a no-op in the headless backend,
/// so no E2E script could open a context menu or a ribbon drop-down, let
/// alone click an item. A menu is now what the X11 / Wayland fallback makes
/// it: a window of its own with the menu DOM, laid out and pumped by this
/// window's loop - which the debug server reaches by `window_id`.
#[test]
fn a_menu_opened_in_a_headless_window_is_a_window_a_script_can_reach() {
    let mut window = settled_window();
    window.show_menu_from_callback(&copy_paste_menu(), LogicalPosition::new(20.0, 10.0), None);
    window.pump_children();

    assert_eq!(window.children.len(), 1, "the menu is a window of its own");
    let menu = &window.children[0];
    let ws = menu.common.current_window_state();
    assert_eq!(ws.window_id.as_str(), "azul-menu");
    assert_eq!(ws.flags.window_type, azul_core::window::WindowType::Menu);
    let texts = texts_of(menu);
    assert!(
        texts.iter().any(|t| t == "Copy") && texts.iter().any(|t| t == "Paste"),
        "the menu window shows its items: {texts:?}"
    );
}

/// A menu opened while another is open (a submenu, a second drop-down)
/// gets an id of its own, or a script could only ever reach the first.
#[test]
fn a_second_open_menu_gets_an_id_of_its_own() {
    let mut window = settled_window();
    window.show_menu_from_callback(&copy_paste_menu(), LogicalPosition::new(20.0, 10.0), None);
    window.pump_children();
    window.show_menu_from_callback(&copy_paste_menu(), LogicalPosition::new(60.0, 10.0), None);
    window.pump_children();

    let ids: Vec<String> = window
        .children
        .iter()
        .map(|c| {
            c.common
                .current_window_state()
                .window_id
                .as_str()
                .to_string()
        })
        .collect();
    assert_eq!(
        ids,
        vec!["azul-menu".to_string(), "azul-menu-2".to_string()]
    );
}
