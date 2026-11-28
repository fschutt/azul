//! A touch the system takes away (Android's `ACTION_CANCEL`, iOS's `touchesCancelled`: the shade, the back
//! gesture, a parent that intercepted) fires nothing: no click on what it was on, no long press, no drag of
//! the page or of a selection (`LayoutWindow::cancel_touch_gestures`, the call a shell makes for the cancel
//! instead of the release it would make of a lift).
//!
//! The engine derives no `TouchCancel` from the window state (a cancelled point and a lifted one are the
//! same delta), so each shell used to emulate it with what it had (the Android shell moves the pointer out
//! of the window); this is the one place that says what a cancel means for every recogniser the engine owns.

use std::time::{Duration, Instant};

use azul_core::{
    dom::{DomId, DomNodeId},
    events::{
        EventData, EventSource, EventType, KeyModifiers, MouseButton, MouseEventData,
        SyntheticEvent,
    },
    geom::LogicalPosition,
    id::NodeId,
    styled_dom::NodeHierarchyItemId,
    task::{Instant as CoreInstant, SystemTick},
    window::WindowPosition,
};
use azul_layout::window::LayoutWindow;
use rust_fontconfig::FcFontCache;

fn window() -> LayoutWindow {
    LayoutWindow::new(FcFontCache::default()).expect("a window")
}

fn at(base: Instant, ms: u64) -> CoreInstant {
    (base + Duration::from_millis(ms)).into()
}

fn finger_down(lw: &mut LayoutWindow, base: Instant, p: LogicalPosition) {
    lw.gesture_drag_manager
        .start_input_session(p, at(base, 0), 0x01, WindowPosition::Uninitialized, p);
}

/// Held 600 ms without moving, a touch is a long press; cancelled, it was never one.
///
/// Mutation: `cancel_touch_gestures` without `end_current_session` (the session stays live): the premise's
/// answer stays after the cancel.
#[test]
fn a_touch_cancelled_while_held_is_no_long_press() {
    let mut lw = window();
    let base = Instant::now();
    finger_down(&mut lw, base, LogicalPosition::new(50.0, 50.0));
    let _ = lw.gesture_drag_manager.record_hold_sample(at(base, 600));
    assert!(
        lw.gesture_drag_manager.detect_long_press().is_some(),
        "premise: a touch held for 600 ms is a long press"
    );

    lw.cancel_touch_gestures();

    assert!(lw.gesture_drag_manager.detect_long_press().is_none());
}

/// Moved 30 px, a touch is a drag; cancelled, it was never one (no drag of the page follows).
#[test]
fn a_touch_cancelled_after_a_pan_is_no_drag() {
    let mut lw = window();
    let base = Instant::now();
    let p = LogicalPosition::new(50.0, 50.0);
    finger_down(&mut lw, base, p);
    let q = LogicalPosition::new(80.0, 50.0);
    let _ = lw.gesture_drag_manager.record_input_sample(q, at(base, 100), 0x01, q);
    assert!(
        lw.gesture_drag_manager.detect_drag().is_some(),
        "premise: a touch moved 30 px is a drag"
    );

    lw.cancel_touch_gestures();

    assert!(lw.gesture_drag_manager.detect_drag().is_none());
}

/// A press owes a release that completes a click on the node it was on; a cancel forgets the press, so the
/// release that follows is no click.
///
/// Mutation: `cancel_touch_gestures` without `clear_press_targets`: the press is still on file after it.
#[test]
fn a_touch_cancelled_forgets_where_it_was_pressed_so_its_release_is_no_click() {
    let mut lw = window();
    let target = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(3))),
    };
    let mut events = vec![SyntheticEvent::new(
        EventType::MouseDown,
        EventSource::User,
        target,
        CoreInstant::Tick(SystemTick { tick_counter: 0 }),
        EventData::Mouse(MouseEventData {
            position: LogicalPosition::new(50.0, 50.0),
            button: MouseButton::Left,
            buttons: 1,
            modifiers: KeyModifiers::default(),
            ..Default::default()
        }),
    )];
    lw.hover_manager.apply_press_target_capture(&mut events, &|_, _| true);
    assert_eq!(
        lw.hover_manager.press_target(MouseButton::Left),
        Some(target),
        "premise: the press is on file"
    );

    lw.cancel_touch_gestures();

    assert_eq!(lw.hover_manager.press_target(MouseButton::Left), None);
}

/// A press on selectable text latches the anchor of a selection drag; a cancel lets go of it, so no later
/// move extends a selection from where the touch landed.
#[test]
fn a_touch_cancelled_lets_go_of_the_anchor_of_a_selection_drag() {
    let mut lw = window();
    lw.text_selection_drag_anchor = Some(LogicalPosition::new(12.0, 8.0));

    lw.cancel_touch_gestures();

    assert_eq!(lw.text_selection_drag_anchor, None);
}

/// A cancel with nothing pending is harmless, and twice is as good as once.
#[test]
fn a_cancel_with_nothing_pending_changes_nothing() {
    let mut lw = window();
    lw.cancel_touch_gestures();
    lw.cancel_touch_gestures();
    assert!(lw.gesture_drag_manager.detect_drag().is_none());
    assert!(lw.gesture_drag_manager.detect_long_press().is_none());
    assert_eq!(lw.hover_manager.press_target(MouseButton::Left), None);
}
