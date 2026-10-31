//! A clipped or covered scroll box takes no pointer: its bar is pressed, and
//! its content wheeled, only where they are painted on top.
//!
//! The press arbiter (`LayoutWindow::route_press`) asked the scroll
//! manager's bars first, by their track rects alone
//! (`ScrollManager::hit_test_scrollbars`). An ancestor's `overflow: hidden`
//! that clips the bar away, and content painted over the bar, did not count:
//! AzDrive's inline dialog (`z-index: 100`) sat over its file list's bar, and
//! a press on the dialog's "I was hacked..." button scrolled the list instead
//! (SYNC17, `scripts/azdrive_e2e.py` step 25j). The wheel had the same blind
//! spot: `convert_cpu_hit_test_to_full` offered every scroll container whose
//! box held the pointer, so a wheel over the dialog scrolled the list
//! beneath it, and a wheel where an ancestor clipped a scroll box away still
//! scrolled that box.
//!
//! Chrome hits a scrollbar only where it is painted - clipped by its
//! ancestors and under whatever paints after it, like any other box - and a
//! wheel drives the scroll chain of the box under the pointer.
//!
//! Two pages in a 400x300 window, `body { margin: 0 }`:
//! - CLIPPED: a 200x200 `overflow: hidden` box around a 300x200
//!   `overflow-y: scroll` box over 1000px. The inner box's bar lies right of
//!   x=200, where the outer box clips it away.
//! - COVERED: a 300x200 `overflow-y: scroll` list over 1000px, and an
//!   absolutely positioned `z-index: 100` dialog at (220, 20) whose 100x40
//!   button (x 230..330, y 30..70) lies over the list's bar. A plain div
//!   stands in for the button: the pointer hits boxes, whatever their role.

use azul_core::{
    dom::{Dom, DomId, NodeId, ScrollbarOrientation},
    events::MouseButton,
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
    task::Instant,
    transform::ComputedTransform3D,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    headless::{convert_cpu_hit_test_to_full, CpuHitTester},
    managers::{
        hover::InputPointId,
        scroll_state::{
            ScrollInputDevice, ScrollInputSource, ScrollbarComponent, ScrollbarHit,
            ScrollbarState,
        },
    },
    press_router::PressTarget,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const BODY: NodeId = NodeId::new(0);

/// CLIPPED: body(0) > outer(1) > inner(2) > tall(3).
const OUTER: NodeId = NodeId::new(1);
const INNER: NodeId = NodeId::new(2);
const INNER_CONTENT: NodeId = NodeId::new(3);

/// COVERED: body(0) > [list(1) > tall(2), dialog(3) > button(4)].
const LIST: NodeId = NodeId::new(1);
const BUTTON: NodeId = NodeId::new(4);

fn clipped_page() -> Dom {
    Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_css("width: 200px; height: 200px; overflow: hidden;")
            .with_child(
                Dom::create_div()
                    .with_css("width: 300px; height: 200px; overflow-y: scroll;")
                    .with_child(Dom::create_div().with_css("height: 1000px;")),
            ),
    )
}

fn covered_page() -> Dom {
    Dom::create_body()
        .with_css("margin: 0;")
        .with_child(
            Dom::create_div()
                .with_css("width: 300px; height: 200px; overflow-y: scroll;")
                .with_child(Dom::create_div().with_css("height: 1000px;")),
        )
        .with_child(
            Dom::create_div()
                .with_css(
                    "position: absolute; left: 220px; top: 20px; width: 100px; height: 40px; \
                     padding: 10px; z-index: 100; background-color: #ffffff;",
                )
                .with_child(Dom::create_div().with_css("height: 40px; background-color: #cccccc;")),
        )
}

/// body(0) > list(1) > tall(2), nothing else.
fn plain_page() -> Dom {
    Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_css("width: 300px; height: 200px; overflow-y: scroll;")
            .with_child(Dom::create_div().with_css("height: 1000px;")),
    )
}

fn window(page: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

fn now() -> Instant {
    Instant::from(std::time::Instant::now())
}

fn bar(lw: &LayoutWindow, node: NodeId) -> ScrollbarState {
    *lw.scroll_manager
        .get_scrollbar_state(DomId::ROOT_ID, node, ScrollbarOrientation::Vertical)
        .expect("harness: the scroll box overflows, so it has a vertical bar")
}

/// The centre of the bar's thumb, in window space.
fn thumb_centre(b: &ScrollbarState) -> LogicalPosition {
    LogicalPosition::new(
        b.track_rect.origin.x + b.track_rect.size.width / 2.0,
        b.track_rect.origin.y + b.button_size + b.thumb_offset + b.thumb_length / 2.0,
    )
}

fn offset_y(lw: &LayoutWindow, node: NodeId) -> f32 {
    lw.scroll_manager
        .get_current_offset(DomId::ROOT_ID, node)
        .unwrap_or_default()
        .y
}

/// THE press arbiter every shell, the scripted pointer and the E2E runner
/// press through: `Some` when a scrollbar took the press.
fn press(lw: &mut LayoutWindow, at: LogicalPosition) -> Option<ScrollbarHit> {
    match lw.route_press(at, MouseButton::Left, now()) {
        PressTarget::Scrollbar(hit) => Some(hit),
        PressTarget::Content => None,
    }
}

/// The layout-side hit tester every backend dispatches pointer input through
/// (`CommonWindowState::perform_hit_test`), rebuilt for this layout.
fn hit_tester(lw: &LayoutWindow) -> CpuHitTester {
    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
    tester
}

/// The box painted on top at `at`: the content the pointer is over.
fn painted_under(lw: &LayoutWindow, at: LogicalPosition) -> Option<NodeId> {
    let tester = hit_tester(lw);
    let resolve = |d: DomId, n: NodeId| lw.scroll_manager.get_current_offset(d, n);
    let no_transform = |_: DomId, _: NodeId| -> Option<ComputedTransform3D> { None };
    tester
        .hit_test_scrolled(at, &resolve, &no_transform)
        .first()
        .map(|hit| hit.1)
}

/// The scroll box a wheel notch at `at` drives, through the platform wheel
/// ingress (`record_scroll_from_hit_test`) fed by the hover hit test.
fn wheel(lw: &mut LayoutWindow, at: LogicalPosition) -> Option<(DomId, NodeId)> {
    let tester = hit_tester(lw);
    let hit = {
        let resolve = |d: DomId, n: NodeId| lw.scroll_manager.get_current_offset(d, n);
        let no_transform = |_: DomId, _: NodeId| -> Option<ComputedTransform3D> { None };
        let hits = tester.hit_test_scrolled(at, &resolve, &no_transform);
        convert_cpu_hit_test_to_full(
            &tester,
            &hits,
            None,
            &lw.layout_results,
            at,
            &resolve,
            &no_transform,
        )
    };
    lw.hover_manager.push_hit_test(InputPointId::Mouse, hit);
    lw.scroll_manager
        .record_scroll_from_hit_test(
            0.0,
            -40.0,
            ScrollInputSource::WheelDiscrete,
            ScrollInputDevice::MouseWheel,
            &lw.hover_manager,
            &InputPointId::Mouse,
            now(),
        )
        .map(|(dom, node, _)| (dom, node))
}

/// A press where an ancestor's `overflow: hidden` clips the bar away goes to
/// what is painted there - the page - and not to the bar.
#[test]
fn a_bar_its_ancestor_clips_away_takes_no_press() {
    let mut lw = window(clipped_page());
    let b = bar(&lw, INNER);
    let at = thumb_centre(&b);
    assert!(
        at.x > 200.0,
        "harness: the inner bar {:?} lies right of the 200px outer box that clips it",
        b.track_rect
    );
    let under = painted_under(&lw, at);
    assert!(
        matches!(under, None | Some(BODY) | Some(OUTER)),
        "what is painted at {at:?} is the page, not the clipped box or its content \
         ({INNER:?}, {INNER_CONTENT:?}): the hit tester found {under:?}"
    );
    let pressed = press(&mut lw, at);
    assert!(
        pressed.is_none(),
        "a press at {at:?}, where the outer box's `overflow: hidden` clips the inner bar \
         {:?} away, must go to what is painted there ({under:?}); the scrollbar took it: \
         {pressed:?}",
        b.track_rect
    );
    assert_eq!(offset_y(&lw, INNER), 0.0, "the clipped box must not scroll");
    assert!(lw.scrollbar_drag().is_none(), "no thumb is held: {:?}", lw.scrollbar_drag());
}

/// A button painted over a bar - an absolutely positioned `z-index: 100`
/// dialog's - takes the press on it.
#[test]
fn a_button_painted_over_a_bar_takes_the_press() {
    let mut lw = window(covered_page());
    let b = bar(&lw, LIST);
    let at = LogicalPosition::new(
        b.track_rect.origin.x + b.track_rect.size.width / 2.0,
        50.0,
    );
    let under = painted_under(&lw, at);
    assert_eq!(
        under,
        Some(BUTTON),
        "harness: the dialog's button is painted at {at:?}, over the list's bar {:?}",
        b.track_rect
    );
    let pressed = press(&mut lw, at);
    assert!(
        pressed.is_none(),
        "a press on the button at {at:?} must reach the button ({under:?}); the list's \
         scrollbar under it took it: {pressed:?}"
    );
    assert_eq!(offset_y(&lw, LIST), 0.0, "a press on the button must not scroll the list");
    assert!(lw.scrollbar_drag().is_none(), "no thumb is held: {:?}", lw.scrollbar_drag());
}

/// Control: the part of the same bar below the dialog is still the bar's.
#[test]
fn the_part_of_a_bar_beside_a_dialog_is_still_pressed() {
    let mut lw = window(covered_page());
    let b = bar(&lw, LIST);
    // The dialog's border box ends at y=80; the bar runs to y=200.
    let at = LogicalPosition::new(
        b.track_rect.origin.x + b.track_rect.size.width / 2.0,
        b.track_rect.origin.y + b.track_rect.size.height - b.button_size - 20.0,
    );
    assert!(at.y > 80.0, "harness: {at:?} is below the dialog");
    let pressed = press(&mut lw, at);
    assert!(
        pressed.is_some_and(|hit| hit.dom_id == DomId::ROOT_ID && hit.node_id == LIST),
        "a press at {at:?} on the list's bar {:?}, beside the dialog, is the bar's: got \
         {pressed:?} (painted there: {:?})",
        b.track_rect,
        painted_under(&lw, at)
    );
}

/// Control: a plain visible bar is pressed where it is painted.
#[test]
fn a_visible_bar_is_still_pressed() {
    let mut lw = window(plain_page());
    let b = bar(&lw, LIST);
    let at = thumb_centre(&b);
    let pressed = press(&mut lw, at);
    assert!(
        pressed.is_some_and(|hit| hit.node_id == LIST
            && hit.orientation == ScrollbarOrientation::Vertical
            && hit.component == ScrollbarComponent::Thumb),
        "a press on the thumb at {at:?} of the plain list's bar {:?} grabs it: got {pressed:?} \
         (painted there: {:?})",
        b.track_rect,
        painted_under(&lw, at)
    );
    assert!(lw.scrollbar_drag().is_some(), "the thumb is held");
}

/// A wheel over a dialog does not scroll the list painted under it.
#[test]
fn a_wheel_over_a_dialog_does_not_scroll_the_list_under_it() {
    let mut lw = window(covered_page());
    let at = LogicalPosition::new(260.0, 50.0);
    let under = painted_under(&lw, at);
    assert_eq!(under, Some(BUTTON), "harness: the button is painted at {at:?}");
    let target = wheel(&mut lw, at);
    assert!(
        target != Some((DomId::ROOT_ID, LIST)),
        "a wheel at {at:?} over the dialog ({under:?}) must scroll the dialog's own scroll \
         chain or nothing; it scrolled the list under it: {target:?}"
    );
}

/// A wheel where an ancestor clips a scroll box away does not scroll it.
#[test]
fn a_wheel_where_an_ancestor_clips_a_scroll_box_away_does_not_scroll_it() {
    let mut lw = window(clipped_page());
    let at = LogicalPosition::new(250.0, 100.0);
    let under = painted_under(&lw, at);
    let target = wheel(&mut lw, at);
    assert!(
        target != Some((DomId::ROOT_ID, INNER)),
        "a wheel at {at:?}, where the outer box clips the inner scroll box away (painted \
         there: {under:?}), must not scroll it: {target:?}"
    );
}

/// Control: a wheel over the list, where nothing covers it, scrolls it.
#[test]
fn a_wheel_over_an_uncovered_list_scrolls_it() {
    let mut lw = window(covered_page());
    let at = LogicalPosition::new(100.0, 100.0);
    let target = wheel(&mut lw, at);
    assert_eq!(
        target,
        Some((DomId::ROOT_ID, LIST)),
        "a wheel at {at:?} over the uncovered list scrolls it (painted there: {:?})",
        painted_under(&lw, at)
    );
}
