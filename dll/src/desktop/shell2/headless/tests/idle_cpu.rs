//! Idle-CPU laws (IDLE_CPU, 2026-09-30).
//!
//! USER RULING: "we need to cull refreshes for animations if the changes
//! aren't on-screen". An animation tick asks for a frame only when the area it
//! changes can be seen: the animated node's painted bounds, after its clip
//! chain and transforms, intersect the window's visible rect, and the window
//! itself can show anything (not minimized, not fully occluded). A culled
//! animation keeps its clock - it is time-based - so a spinner scrolled back
//! into view shows the right phase at once, with no catch-up burst.
//!
//! The spinner is the case the user measured: AzWidgets sat at 35% CPU with
//! nothing happening but three spinners.

use super::*;
use azul_core::{
    dom::{DomId, NodeId},
    task::CSS_ANIMATION_TIMER_ID,
};

/// A 300x200 `overflow-y: scroll` box: a 32px spinner at its top, then
/// 1000px of filler, so scrolling the box by 600px takes the spinner far out
/// of its clip while the box itself stays on screen.
extern "C" fn spinner_in_scroller_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_css::{
        dynamic_selector::CssPropertyWithConditions,
        props::{
            layout::{
                dimensions::{LayoutHeight, LayoutWidth},
                overflow::LayoutOverflow,
            },
            property::CssProperty,
        },
    };
    use azul_layout::widgets::spinner::Spinner;

    let filler = Dom::create_div().with_css_props(
        vec![
            CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(280.0))),
            CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(1000.0))),
        ]
        .into(),
    );
    let scroller = Dom::create_div()
        .with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(300.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(200.0))),
                CssPropertyWithConditions::simple(CssProperty::overflow_y(LayoutOverflow::Scroll)),
            ]
            .into(),
        )
        .with_child(Spinner::create().dom())
        .with_child(filler);
    Dom::create_body().with_child(scroller)
}

/// One driver period, in test-clock ms.
const FRAME_MS: u64 = 16;

/// A laid-out [`spinner_in_scroller_layout`] window on a FROZEN test clock
/// (the caller resets it), with the animation driver armed and two frames
/// behind it: the first driver frame samples the tracks and mints their GPU
/// keys, the repaint after it rebuilds the display list around them - so the
/// list binds the spinner's animated groups, as it does in a running app.
fn spinner_window() -> HeadlessWindow {
    azul_core::task::reset_test_clock();
    azul_core::task::freeze_test_clock();
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, spinner_in_scroller_layout);
    window.regenerate_layout().expect("initial layout");
    window.arm_animation_drivers_if_needed();
    for _ in 0..2 {
        let _ = azul_core::task::advance_test_clock_ms(FRAME_MS);
        let _ = window.process_timers_and_threads();
        window.relayout_only().expect("repaint");
    }
    window
}

fn lw(window: &HeadlessWindow) -> &LayoutWindow {
    window.common.layout_window.as_ref().expect("layout window")
}

fn driver_armed(window: &HeadlessWindow) -> bool {
    lw(window).timers.contains_key(&CSS_ANIMATION_TIMER_ID)
}

/// The spinner's container node.
fn spinner_node(window: &HeadlessWindow) -> NodeId {
    node_with_class(window, "__azul-native-spinner").expect("the spinner is laid out")
}

/// Scroll the box around the spinner to `y`, the way the scroll physics
/// timer commits a wheel step.
fn scroll_box_to(window: &mut HeadlessWindow, y: f32) {
    let spinner = spinner_node(window);
    let lw = window.common.layout_window.as_mut().expect("layout window");
    let scroller = lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .and_then(|r| r.styled_dom.node_hierarchy.as_container()[spinner].parent_id())
        .expect("the spinner sits in the scroll box");
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        scroller,
        LogicalPosition::new(0.0, y),
        azul_core::task::Instant::now(),
    );
}

/// `(t, duration_s)` of the spinner's first LOOPING track - the container's
/// fade-in is a single pass and settles on its own.
fn looping_track(window: &HeadlessWindow) -> Option<(f32, f32)> {
    lw(window)
        .live_tracks
        .values()
        .find(|tr| tr.iterations_left.is_none())
        .map(|tr| (tr.t, tr.duration_s))
}

/// Distance between two phases on the unit circle.
fn phase_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(1.0);
    d.min(1.0 - d)
}

/// Run `n` driver periods; how many of them asked for a frame.
fn frames_requested_over(window: &mut HeadlessWindow, n: usize) -> usize {
    (0..n)
        .filter(|_| {
            let _ = azul_core::task::advance_test_clock_ms(FRAME_MS);
            window.process_timers_and_threads()
        })
        .count()
}

#[test]
fn an_offscreen_spinner_requests_no_frames() {
    let mut window = spinner_window();
    let armed_while_visible = driver_armed(&window);
    scroll_box_to(&mut window, 600.0);

    let requested = frames_requested_over(&mut window, 10);
    let still_armed = driver_armed(&window);
    let dirty = window.common.display_list_dirty;
    azul_core::task::reset_test_clock();

    assert!(
        armed_while_visible,
        "harness: a visible spinner keeps the animation driver armed"
    );
    assert_eq!(
        requested, 0,
        "a spinner scrolled 600px out of its 200px box asked for {requested} frames over 10 \
         driver periods; nothing it changes can be seen, so it must ask for none"
    );
    assert!(
        !still_armed,
        "with only a culled animation left, the driver timer must be removed so the window can \
         go idle"
    );
    assert!(!dirty, "a culled tick must not mark the display list dirty");
}

#[test]
fn a_spinner_scrolled_back_into_view_requests_a_frame_on_the_next_tick_at_its_wall_clock_phase() {
    let mut window = spinner_window();
    scroll_box_to(&mut window, 600.0);
    let (t_culled, duration_s) = looping_track(&window).expect("the ring loops");
    let culled_ms: u64 = 12 * FRAME_MS;
    let _ = frames_requested_over(&mut window, 12);

    // The scroll back is a pass of its own; every pass ends by arming the
    // animation drivers (`arm_animation_drivers_if_needed`).
    scroll_box_to(&mut window, 0.0);
    window.arm_animation_drivers_if_needed();
    let _ = azul_core::task::advance_test_clock_ms(FRAME_MS);
    let first = window.process_timers_and_threads();
    let (t_back, _) = looping_track(&window).expect("the ring still loops");
    // No catch-up burst: the frame after it is an ordinary one.
    let _ = azul_core::task::advance_test_clock_ms(FRAME_MS);
    let second = window.process_timers_and_threads();
    let (t_next, _) = looping_track(&window).expect("the ring still loops");
    azul_core::task::reset_test_clock();

    assert!(
        first,
        "the spinner is visible again: the very next driver tick must ask for a frame"
    );
    #[allow(clippy::cast_precision_loss)]
    let elapsed_s = (culled_ms + FRAME_MS) as f32 / 1000.0;
    let expected = (t_culled + elapsed_s / duration_s).rem_euclid(1.0);
    assert!(
        phase_distance(t_back, expected) < 0.02,
        "a culled animation keeps its clock: after {elapsed_s}s the loop must stand at phase \
         {expected}, not {t_back} (it was {t_culled} when it left the screen)"
    );
    #[allow(clippy::cast_precision_loss)]
    let one_frame = FRAME_MS as f32 / 1000.0 / duration_s;
    assert!(
        second && phase_distance(t_next, (t_back + one_frame).rem_euclid(1.0)) < 0.01,
        "after the catch-up the loop steps one frame per tick (second frame requested: \
         {second}, phase {t_back} -> {t_next})"
    );
}

#[test]
fn a_spinner_in_a_minimized_window_requests_no_frames() {
    let mut window = spinner_window();
    window
        .common
        .update_window_state(event::WindowStateSource::Os, |ws| {
            ws.flags.frame = WindowFrame::Minimized;
        });
    window.arm_animation_drivers_if_needed();

    let requested = frames_requested_over(&mut window, 10);
    let still_armed = driver_armed(&window);
    azul_core::task::reset_test_clock();

    assert_eq!(
        requested, 0,
        "a minimized window shows nothing: its spinner asked for {requested} frames"
    );
    assert!(!still_armed, "and its animation driver must be removed");
}

/// macOS reports a fully covered window through `occlusionState`; the shell
/// records it on the layout window (`window_occluded`) and every animation
/// in it is culled.
#[test]
fn a_spinner_in_an_occluded_window_requests_no_frames() {
    let mut window = spinner_window();
    if let Some(lw) = window.common.layout_window.as_mut() {
        lw.window_occluded = true;
    }
    window.arm_animation_drivers_if_needed();

    let requested = frames_requested_over(&mut window, 10);
    let still_armed = driver_armed(&window);
    azul_core::task::reset_test_clock();

    assert_eq!(
        requested, 0,
        "a covered window shows nothing: its spinner asked for {requested} frames"
    );
    assert!(!still_armed, "and its animation driver must be removed");
}

#[test]
fn a_visible_spinners_frame_damages_only_its_own_rect() {
    let mut window = spinner_window();
    let spinner = spinner_node(&window);
    let rect = lw(&window)
        .get_node_layout_rect(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(spinner)),
        })
        .expect("the spinner has a box");

    let _ = azul_core::task::advance_test_clock_ms(FRAME_MS);
    let requested = window.process_timers_and_threads();
    window.relayout_only().expect("repaint");
    let damage = window.cpu_backend.last_frame_damage.clone();
    azul_core::task::reset_test_clock();

    assert!(requested, "harness: a visible spinner asks for its frame");
    // Every part turns about the box centre: its corners sweep a circle of
    // radius D/sqrt(2), a little past the box on every side.
    let slack = rect.size.width.max(rect.size.height) * 0.25 + 1.0;
    let (x0, y0) = (rect.origin.x - slack, rect.origin.y - slack);
    let (x1, y1) = (
        rect.origin.x + rect.size.width + slack,
        rect.origin.y + rect.size.height + slack,
    );
    match &damage {
        FrameDamage::Rects(rects) => {
            assert!(!rects.is_empty(), "a moving spinner damages something");
            for r in rects {
                assert!(
                    r.origin.x >= x0
                        && r.origin.y >= y0
                        && r.origin.x + r.size.width <= x1
                        && r.origin.y + r.size.height <= y1,
                    "a spinner frame damaged {r:?}, outside the spinner's own {rect:?}"
                );
            }
        }
        other => panic!(
            "a spinner frame must repaint only the spinner's rect, the frame damaged {other:?}"
        ),
    }
}

/// USER RULING: "we should remove the timer if we don't have any timer
/// running". A window with no animation, no caret tween, no focused editable
/// and no user timer registers no timer at all and asks for no frames: the
/// caret tween and blink timers exist only while they have work, the CSS
/// driver only while something visible moves.
#[test]
fn an_idle_window_registers_no_timers_and_requests_no_frames() {
    azul_core::task::reset_test_clock();
    azul_core::task::freeze_test_clock();
    let state = Arc::new(RefCell::new(RefAny::new(UiState {
        label: "nothing moves here".to_string(),
    })));
    let mut window = make_harness_window(&state);
    window.regenerate_layout().expect("initial layout");
    window.arm_animation_drivers_if_needed();
    let timers: Vec<usize> = lw(&window).timers.keys().map(|t| t.id).collect();
    let requested = frames_requested_over(&mut window, 10);
    azul_core::task::reset_test_clock();

    assert!(
        timers.is_empty(),
        "an idle window must register no timer, it registered {timers:x?}"
    );
    assert_eq!(
        requested, 0,
        "an idle window asked for {requested} frames over 10 periods"
    );
}
