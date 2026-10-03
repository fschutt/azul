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
