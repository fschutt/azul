//! A press on selectable text near the edge of a scrolling box is a click (E41).
//!
//! A pointer that JUMPS to a point and presses there in one pass - a script's `mouse_down` with no `mouse_move`
//! before it, a touchpad tap, a remote desktop - has a move in the pass of its press. The press latched the anchor
//! of a text-selection drag (selectable text is under it), the move of that same pass read as a drag from the
//! anchor to the point it stands on, and that drag started the autoscroll timer; the pointer sat in the band at
//! the edge of the box, the box scrolled under it between the press and the release, and the release landed on
//! another node: no click (AzCompanion, the key 'Try it without a Mac' at the bottom of its column: no click 3 and
//! 7 px from the edge, a click from 17 px; the same press on the blank of the key, or after a hover, a click).
//!
//! A drag of the selection scrolls the box only once the pointer has left the point it pressed at by the drag
//! threshold; a press that did not move, or only jittered, is a click.

use core::sync::atomic::{AtomicUsize, Ordering};

use azul_core::{dom::DomId, events::ProcessEventResult, window::CursorPosition};

use super::*;

/// How often the key was clicked; the app state and the key's callback data share the counter.
#[derive(Debug, Clone, Default)]
struct EdgePage {
    clicks: Arc<AtomicUsize>,
}

extern "C" fn count_click(
    mut data: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(page) = data.downcast_ref::<EdgePage>() {
        page.clicks.fetch_add(1, Ordering::SeqCst);
    }
    azul_core::callbacks::Update::DoNothing
}

/// The key sits so that its centre is 3 px above the bottom edge of the box: 185 px of room above it in a
/// column 200 px high, the key 24 px high.
const BOX_HEIGHT: f32 = 200.0;
const KEY_TOP: f32 = 185.0;
const KEY_HEIGHT: f32 = 24.0;
const KEY_CENTRE_Y: f32 = KEY_TOP + KEY_HEIGHT / 2.0;
const POINTER_X: f32 = 100.0;

/// `body > div.page (300 x 200, overflow-y: auto) > [185 px of room, div.key (24 px high, click: count, the words
/// 'Try it without a Mac'), 600 px of room]`: the box scrolls, and the key is at its bottom edge.
extern "C" fn page_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        events::{EventFilter, HoverEventFilter},
    };

    let page = data
        .downcast_ref::<EdgePage>()
        .map(|p| (*p).clone())
        .unwrap_or_default();
    let key = Dom::create_div()
        .with_class("key".into())
        .with_css("width: 280px; height: 24px;")
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Click),
                callback: CoreCallback {
                    cb: count_click as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(page),
            }]
            .into(),
        )
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "Try it without a Mac",
        ));
    Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_class("page".into())
            .with_css("width: 300px; height: 200px; overflow-y: auto;")
            .with_child(Dom::create_div().with_css("height: 185px;"))
            .with_child(key)
            .with_child(Dom::create_div().with_css("height: 600px;")),
    )
}

/// One frame of the test clock, in ms: longer than a tick of the 60 Hz autoscroll timer.
const FRAME_MS: u64 = 17;

/// The laid-out page on a FROZEN test clock, with its state.
fn settled_window() -> (Arc<RefCell<RefAny>>, HeadlessWindow) {
    azul_core::task::reset_test_clock();
    azul_core::task::freeze_test_clock();
    let state = Arc::new(RefCell::new(RefAny::new(EdgePage::default())));
    let mut window = make_window_with(&state, page_layout);
    window.regenerate_layout().expect("initial layout");
    window.regenerate_layout().expect("settle");
    let _ = window.common.take_regeneration();
    (state, window)
}

/// A script's pointer op (the debug server's `mouse_down` / `mouse_up` / `mouse_move`): the pointer is put at
/// (x, y) and the button takes `down` IN ONE state change, the pass hit-tests at the pointer, and the gesture
/// session records the press, the move or the release. A `mouse_down` with no `mouse_move` before it is a pointer
/// that jumps to its press.
fn scripted_pointer(window: &mut HeadlessWindow, x: f32, y: f32, down: bool) {
    window.snapshot_window_state_baseline("headless.test.scripted_pointer");
    let at = LogicalPosition { x, y };
    let was_down = window.common.current_window_state().mouse_state.left_down;
    {
        let mouse = window.common.mouse_state_mut();
        mouse.cursor_position = CursorPosition::InWindow(at);
        mouse.left_down = down;
    }
    window.update_hit_test_at(at);
    window.record_scripted_pointer_sample(was_down);
    let tier = window.process_window_events(0);
    if tier > ProcessEventResult::DoNothing {
        window.service_frame(tier);
    }
}

/// `frames` frames of the test clock, each followed by the timers that are due.
fn tick(window: &mut HeadlessWindow, frames: usize) {
    for _ in 0..frames {
        let _ = azul_core::task::advance_test_clock_ms(FRAME_MS);
        let _ = window.process_timers_and_threads();
    }
}

fn lw(window: &HeadlessWindow) -> &LayoutWindow {
    window.common.layout_window.as_ref().expect("layout window")
}

/// How far the page box is scrolled.
fn page_offset(window: &HeadlessWindow) -> f32 {
    let page = node_with_class(window, "page").expect("the page box is laid out");
    lw(window)
        .scroll_manager
        .get_current_offset(DomId::ROOT_ID, page)
        .map_or(0.0, |offset| offset.y)
}

fn autoscroll_timer_armed(window: &HeadlessWindow) -> bool {
    lw(window)
        .timers
        .contains_key(&azul_core::task::DRAG_AUTOSCROLL_TIMER_ID)
}

fn clicks_of(state: &Arc<RefCell<RefAny>>) -> usize {
    let mut guard = state.borrow_mut();
    let app: &mut RefAny = &mut guard;
    app.downcast_ref::<EdgePage>()
        .map_or(0, |page| page.clicks.load(Ordering::SeqCst))
}

/// THE REPORT: a press at the key's centre 3 px from the edge with the pointer jumping to it, the release at the
/// same point a moment later. The box must not have moved, and the key is clicked.
///
/// Mutation: the move in the pass of the press taken for a drag (the state before): the timer is armed by the
/// press, the box scrolls ~40 px in four frames, the release lands below the key and there is no click.
#[test]
fn a_press_that_jumps_to_selectable_text_at_the_edge_of_a_scrolling_box_is_a_click() {
    let (state, mut window) = settled_window();
    assert!(
        (KEY_CENTRE_Y - (BOX_HEIGHT - 3.0)).abs() < 0.01,
        "harness: the key's centre is 3 px from the bottom edge of the box"
    );
    assert!(
        page_offset(&window).abs() < f32::EPSILON,
        "harness: the box starts at its top"
    );

    scripted_pointer(&mut window, POINTER_X, KEY_CENTRE_Y, true);
    let armed = autoscroll_timer_armed(&window);
    tick(&mut window, 4);
    let scrolled = page_offset(&window);
    scripted_pointer(&mut window, POINTER_X, KEY_CENTRE_Y, false);
    let clicks = clicks_of(&state);

    let facts = format!(
        "autoscroll armed by the press: {armed}; the box scrolled by {scrolled} px in 4 frames; \
         clicks after the release: {clicks}"
    );
    assert!(
        !armed,
        "a press that has not moved started the autoscroll of the box it pressed in ({facts})"
    );
    assert!(
        scrolled.abs() < f32::EPSILON,
        "the box moved under a pointer that only pressed in it ({facts})"
    );
    assert_eq!(clicks, 1, "a press and a release at one point are a click ({facts})");
}

/// A hand is not still: a press that moves 2 px before it lets go, under the drag threshold, is a click too. The
/// move is a drag of the selection (the highlight follows it), but not one that scrolls the box.
///
/// Mutation: the autoscroll tick without the drag threshold (the state before): the tick scrolls the box from
/// the first move.
#[test]
fn a_press_that_only_jitters_at_the_edge_of_a_scrolling_box_is_a_click() {
    let (state, mut window) = settled_window();

    scripted_pointer(&mut window, POINTER_X, KEY_CENTRE_Y, true);
    scripted_pointer(&mut window, POINTER_X + 2.0, KEY_CENTRE_Y, true);
    tick(&mut window, 4);
    let scrolled = page_offset(&window);
    scripted_pointer(&mut window, POINTER_X + 2.0, KEY_CENTRE_Y, false);
    let clicks = clicks_of(&state);

    let facts = format!("the box scrolled by {scrolled} px in 4 frames; clicks after the release: {clicks}");
    assert!(
        scrolled.abs() < f32::EPSILON,
        "a jitter of 2 px scrolled the box under the pointer ({facts})"
    );
    assert_eq!(clicks, 1, "a press that jittered is still a click ({facts})");
}

/// The other half: a selection drag that has left its press point still scrolls the box when the pointer is at
/// its edge. Pressed on the key at the edge and dragged 100 px sideways (the pointer stays in the band), the
/// box scrolls.
#[test]
fn a_selection_drag_that_left_its_press_point_still_scrolls_the_box_at_its_edge() {
    let (_state, mut window) = settled_window();

    scripted_pointer(&mut window, POINTER_X, KEY_CENTRE_Y, true);
    scripted_pointer(&mut window, POINTER_X + 100.0, KEY_CENTRE_Y, true);
    let armed = autoscroll_timer_armed(&window);
    tick(&mut window, 4);
    let scrolled = page_offset(&window);

    assert!(armed, "a drag armed the autoscroll timer");
    assert!(
        scrolled > 10.0,
        "a drag at the bottom edge of the box scrolls it (scrolled by {scrolled} px in 4 frames)"
    );
}

/// A press that is released where it landed, but never moved, ends the autoscroll timer with it.
#[test]
fn the_release_of_a_press_leaves_no_autoscroll_timer_behind() {
    let (_state, mut window) = settled_window();

    scripted_pointer(&mut window, POINTER_X, KEY_CENTRE_Y, true);
    scripted_pointer(&mut window, POINTER_X + 100.0, KEY_CENTRE_Y, true);
    scripted_pointer(&mut window, POINTER_X + 100.0, KEY_CENTRE_Y, false);

    assert!(
        !autoscroll_timer_armed(&window),
        "the release of the button stops the timer of the drag"
    );
}
