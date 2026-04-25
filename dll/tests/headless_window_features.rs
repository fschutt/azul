//! End-to-end proof that the cross-platform window-feature primitives —
//! `TouchPoint`/`TouchPointVec`, `FullScreenMode`, `AcceleratorKey`,
//! `VirtualKeyCode::get_lowercase`, and `ScrollResult` — are reachable from
//! the public API and observable via the `HeadlessWindow` harness.
//!
//! These types lived as orphan declarations in `core/src/window.rs` for some
//! time. The headless test below exists to make sure each one is wired into
//! a live state path (window state mutation, accelerator match, IME fallback)
//! so any future refactor that "drops dead code" first has to remove this
//! test.

use std::cell::RefCell;
use std::sync::Arc;

use azul_core::events::MouseButton;
use azul_core::geom::LogicalPosition;
use azul_core::icon::{IconProviderHandle, SharedIconProvider};
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_core::window::{
    AcceleratorKey, FullScreenMode, OptionVirtualKeyCode, TouchPoint, VirtualKeyCode, WindowFrame,
};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

use azul::desktop::shell2::headless::{HeadlessEvent, HeadlessWindow};

fn make_window() -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    let app_data = Arc::new(RefCell::new(RefAny::new(())));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());

    HeadlessWindow::new(
        WindowCreateOptions::default(),
        app_data,
        AppConfig::default(),
        icon_provider,
        fc_cache,
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

#[test]
fn virtual_key_code_get_lowercase_returns_locale_independent_char() {
    // Latin keys produce a typed character without an IME round-trip.
    assert_eq!(VirtualKeyCode::S.get_lowercase(), Some('s'));
    assert_eq!(VirtualKeyCode::Numpad0.get_lowercase(), Some('0'));
    assert_eq!(VirtualKeyCode::Period.get_lowercase(), Some('.'));
    // Non-character keys return None — the IME / layout-aware path must
    // resolve them.
    assert_eq!(VirtualKeyCode::Escape.get_lowercase(), None);
    assert_eq!(VirtualKeyCode::F5.get_lowercase(), None);
}

#[test]
fn synthesize_character_input_uses_get_lowercase_fallback() {
    let mut window = make_window();

    // KeyDown for `S` should synthesize a TextInput("s") via get_lowercase().
    let synth = window.synthesize_character_input(VirtualKeyCode::S);
    assert_eq!(synth, Some('s'));

    // The synthesized TextInput is queued for the next poll.
    let next = window.poll_event();
    assert!(
        matches!(next, Some(HeadlessEvent::TextInput { ref text }) if text == "s"),
        "expected synthesized TextInput(\"s\") in queue, got {:?}",
        next,
    );

    // KeyDown for Escape returns None (no fallback char) and queues nothing.
    let synth_none = window.synthesize_character_input(VirtualKeyCode::Escape);
    assert_eq!(synth_none, None);
    assert!(window.poll_event().is_none());
}

#[test]
fn accelerator_key_matches_individual_modifiers() {
    let mut window = make_window();

    // Initially nothing is pressed.
    assert!(!window.matches_accelerator(&[AcceleratorKey::Ctrl]));
    assert!(!window.matches_accelerator(&[AcceleratorKey::Key(VirtualKeyCode::S)]));

    // Press Ctrl+S.
    let kb = &mut window.common.current_window_state.keyboard_state;
    kb.pressed_virtual_keycodes =
        vec![VirtualKeyCode::LControl, VirtualKeyCode::S].into();
    kb.current_virtual_keycode = OptionVirtualKeyCode::Some(VirtualKeyCode::S);

    // The chord [Ctrl, Key(S)] should fire now.
    let chord = [
        AcceleratorKey::Ctrl,
        AcceleratorKey::Key(VirtualKeyCode::S),
    ];
    assert!(window.matches_accelerator(&chord));

    // Adding Shift to the chord should make it not fire (shift isn't down).
    let chord_with_shift = [
        AcceleratorKey::Ctrl,
        AcceleratorKey::Shift,
        AcceleratorKey::Key(VirtualKeyCode::S),
    ];
    assert!(!window.matches_accelerator(&chord_with_shift));

    // Each AcceleratorKey individually evaluates correctly via .matches().
    assert!(AcceleratorKey::Ctrl.matches(&window.common.current_window_state.keyboard_state));
    assert!(!AcceleratorKey::Shift.matches(&window.common.current_window_state.keyboard_state));
    assert!(AcceleratorKey::Key(VirtualKeyCode::S)
        .matches(&window.common.current_window_state.keyboard_state));

    // Empty chord matches trivially.
    assert!(window.matches_accelerator(&[]));
}

#[test]
fn fullscreen_mode_toggles_window_frame_state() {
    let mut window = make_window();

    // Default: Normal frame, FastFullScreen transition style.
    assert_eq!(
        window.common.current_window_state.flags.frame,
        WindowFrame::Normal
    );
    assert_eq!(
        window.common.current_window_state.flags.fullscreen_mode,
        FullScreenMode::FastFullScreen
    );

    // FastFullScreen → frame becomes Fullscreen.
    window.set_fullscreen_mode(FullScreenMode::FastFullScreen);
    assert_eq!(
        window.common.current_window_state.flags.frame,
        WindowFrame::Fullscreen
    );
    assert_eq!(
        window.common.current_window_state.flags.fullscreen_mode,
        FullScreenMode::FastFullScreen
    );

    // SlowWindowed → frame becomes Normal again, transition style records "slow".
    window.set_fullscreen_mode(FullScreenMode::SlowWindowed);
    assert_eq!(
        window.common.current_window_state.flags.frame,
        WindowFrame::Normal
    );
    assert_eq!(
        window.common.current_window_state.flags.fullscreen_mode,
        FullScreenMode::SlowWindowed
    );

    // SlowFullScreen → Fullscreen with slow transition style.
    window.set_fullscreen_mode(FullScreenMode::SlowFullScreen);
    assert_eq!(
        window.common.current_window_state.flags.frame,
        WindowFrame::Fullscreen
    );
    assert_eq!(
        window.common.current_window_state.flags.fullscreen_mode,
        FullScreenMode::SlowFullScreen
    );
}

#[test]
fn touch_points_are_recorded_on_window_state() {
    let mut window = make_window();

    // No touches initially.
    assert_eq!(
        window.common.current_window_state.touch_state.num_touches,
        0
    );
    assert_eq!(
        window
            .common
            .current_window_state
            .touch_state
            .touch_points
            .len(),
        0
    );

    // Inject a two-finger pinch.
    let p1 = TouchPoint {
        id: 1,
        position: LogicalPosition::new(100.0, 200.0),
        force: 0.5,
    };
    let p2 = TouchPoint {
        id: 2,
        position: LogicalPosition::new(150.0, 220.0),
        force: 0.7,
    };
    window.inject_touch_points([p1, p2]);

    let touch_state = &window.common.current_window_state.touch_state;
    assert_eq!(touch_state.num_touches, 2);
    assert_eq!(touch_state.touch_points.len(), 2);
    let pts = touch_state.touch_points.as_ref();
    assert_eq!(pts[0].id, 1);
    assert_eq!(pts[0].position.x, 100.0);
    assert_eq!(pts[1].id, 2);
    assert_eq!(pts[1].force, 0.7);

    // Inject empty list — clears the touch state.
    let empty: [TouchPoint; 0] = [];
    window.inject_touch_points(empty);
    let touch_state = &window.common.current_window_state.touch_state;
    assert_eq!(touch_state.num_touches, 0);
    assert_eq!(touch_state.touch_points.len(), 0);
}

#[test]
fn process_system_scroll_returns_populated_scroll_result() {
    let mut window = make_window();

    // Non-zero delta → 1 scrolled node, no overscroll, wheel-style.
    let result = window.process_system_scroll(LogicalPosition::new(0.0, -120.0), false);
    assert_eq!(result.scrolled_nodes, 1);
    assert_eq!(result.remaining_delta, LogicalPosition::zero());
    assert!(!result.hit_scrollbar);

    // Zero delta → no nodes scrolled.
    let result_zero = window.process_system_scroll(LogicalPosition::zero(), false);
    assert_eq!(result_zero.scrolled_nodes, 0);

    // hit_scrollbar=true → flag propagates through (scrollbar drag).
    let result_drag = window.process_system_scroll(LogicalPosition::new(40.0, 0.0), true);
    assert_eq!(result_drag.scrolled_nodes, 1);
    assert!(result_drag.hit_scrollbar);

    // The free helper is also callable directly (used by embedders that want
    // a `ScrollResult` without going through a window handle).
    let direct = azul_core::window::process_system_scroll(
        LogicalPosition::new(10.0, 10.0),
        false,
    );
    assert_eq!(direct.scrolled_nodes, 1);
    assert!(!direct.hit_scrollbar);
}

#[test]
fn headless_event_queue_round_trip_for_keyboard_input() {
    // Sanity check that the existing event-injection plumbing still works
    // alongside the new helpers: queue a KeyDown and a synthesized TextInput,
    // poll them in order.
    let mut window = make_window();
    window.inject_event(HeadlessEvent::KeyDown {
        virtual_keycode: VirtualKeyCode::A,
    });
    let _ = window.synthesize_character_input(VirtualKeyCode::A);
    window.inject_event(HeadlessEvent::MouseDown {
        button: MouseButton::Left,
    });

    assert!(matches!(
        window.poll_event(),
        Some(HeadlessEvent::KeyDown { virtual_keycode }) if virtual_keycode == VirtualKeyCode::A
    ));
    assert!(matches!(
        window.poll_event(),
        Some(HeadlessEvent::TextInput { ref text }) if text == "a"
    ));
    assert!(matches!(
        window.poll_event(),
        Some(HeadlessEvent::MouseDown { button }) if button == MouseButton::Left
    ));
    assert!(window.poll_event().is_none());
}
