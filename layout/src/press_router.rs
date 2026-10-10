//! The pointer-press arbiter: does a press belong to a scrollbar, or to the
//! document under it?
//!
//! ONE answer for every way a press reaches a window: the native shells
//! (macOS, Windows, X11, Wayland), the dll's headless backend, a SCRIPTED
//! press (`DebugEvent::MouseDown`, the `click` op, any callback that pushes a
//! button through `modify_window_state` - all of them arrive as a
//! `ModifyWindowState` / `QueueWindowStateSequence` state push) and the
//! headless E2E runner. The order is fixed: the scrollbar layer first, then
//! the content - and the scrollbar layer is what is PAINTED there
//! ([`LayoutWindow::scrollbar_at`]): a bar an ancestor clips away, or one
//! under a box painted over it (a `z-index` dialog's button over a list),
//! leaves the press to what the user sees, as in Chrome.
//!
//! This used to live in each shell, BEFORE the shared event pass
//! (`perform_scrollbar_hit_test` -> `handle_scrollbar_click` /
//! `handle_track_click` / `handle_scrollbar_drag` in the dll's
//! `common/event.rs`), while a scripted press skipped it entirely: it only
//! wrote `left_down` and ran the event pass. So no layout test and no AZ_E2E
//! script could reproduce a press a scrollbar took on a device - the
//! phantom-scrollbar press that stopped text selection in an overflowing
//! `TextInput` was invisible to every harness.
//!
//! What stays in the shells is platform work only: recording the button in
//! the window state, pointer capture, the redraw request.
//!
//! The held thumb lives on the window ([`LayoutWindow::scrollbar_drag`]), not
//! in the shell: `LayoutWindow::remap_node_ids` already carries it across a
//! DOM rebuild, and a scripted press and a physical release (or the other
//! way round) see the same drag.

use azul_core::{
    dom::{DomId, NodeId, ScrollbarOrientation},
    events::MouseButton,
    geom::LogicalPosition,
    hit_test::ScrollbarHitId,
    task::Instant,
    window::MouseState,
};
use azul_css::system::ScrollbarTrackClick;

use crate::{
    headless::CpuHitTester,
    managers::scroll_state::{ScrollManager, ScrollbarComponent, ScrollbarHit},
    window::{LayoutWindow, ScrollbarDragState},
};

/// One arrow-button click scrolls one LINE: the unit a wheel detent and a
/// keyboard arrow scroll by in the shells (`WHEEL_SCROLL_PIXELS_PER_LINE` in
/// the dll's `common/event.rs`, which pins the two equal).
pub const SCROLLBAR_ARROW_STEP_PX: f32 = 20.0;

/// A track click in page mode moves this fraction of the visible length, so
/// one line of context stays on screen (the Windows default).
const TRACK_PAGE_FRACTION: f32 = 0.9;

/// Where a pointer press went.
#[derive(Debug, Clone, Copy)]
pub enum PressTarget {
    /// A scrollbar took the press, and has already acted on it: a press on
    /// the thumb started a drag ([`LayoutWindow::scrollbar_drag`]), a press on
    /// the track paged or jumped, a press on an arrow button stepped one line.
    /// The caller must NOT also hand the press to the content under the bar.
    Scrollbar(ScrollbarHit),
    /// No scrollbar is under the point, or the button does not work
    /// scrollbars: the press is the document's, for the event pass.
    Content,
}

impl PressTarget {
    /// Whether a scrollbar took the press.
    #[must_use]
    pub const fn is_scrollbar(&self) -> bool {
        matches!(self, Self::Scrollbar(_))
    }
}

impl LayoutWindow {
    /// THE press arbiter: scrollbar first, then content.
    ///
    /// `position` is in window space (the space the shells report the
    /// pointer in and the scrollbar tracks are kept in). Only the primary
    /// button works a scrollbar; any other button is always
    /// [`PressTarget::Content`], so a right press on a bar reaches the
    /// context-menu path instead of grabbing the thumb.
    ///
    /// On [`PressTarget::Scrollbar`] the scrollbar has already acted (see
    /// the variant). A primary press while a drag is still held means its
    /// release was lost: that drag is ended first.
    pub fn route_press(
        &mut self,
        position: LogicalPosition,
        button: MouseButton,
        now: Instant,
    ) -> PressTarget {
        if button != MouseButton::Left {
            return PressTarget::Content;
        }
        if self.currently_dragging_thumb.take().is_some() {
            self.scroll_manager.end_thumb_drag(now.clone());
        }
        let Some(hit) = self.scrollbar_at(position) else {
            return PressTarget::Content;
        };
        match hit.component {
            ScrollbarComponent::Thumb => {
                let initial_scroll_offset = self
                    .scroll_manager
                    .get_current_offset(hit.dom_id, hit.node_id)
                    .unwrap_or_default();
                self.currently_dragging_thumb = Some(ScrollbarDragState {
                    hit_id: scrollbar_hit_id(&hit),
                    initial_mouse_pos: position,
                    initial_scroll_offset,
                });
                self.scroll_manager
                    .begin_thumb_drag(hit.dom_id, hit.node_id, hit.orientation, now);
            }
            ScrollbarComponent::Track
            | ScrollbarComponent::TopButton
            | ScrollbarComponent::BottomButton => {
                // MWA-C-scroll: `SystemStyle.scrollbar_preferences.track_click`
                // is the OS setting (jump-to-position or page); a window with
                // no system style yet takes the default.
                let track_click = self
                    .system_style
                    .as_ref()
                    .map(|s| s.scrollbar_preferences.track_click)
                    .unwrap_or_default();
                if let Some(delta) = track_click_delta(&self.scroll_manager, &hit, track_click) {
                    scroll_axis_by(
                        &mut self.scroll_manager,
                        hit.dom_id,
                        hit.node_id,
                        hit.orientation,
                        delta,
                        now,
                    );
                }
            }
        }
        PressTarget::Scrollbar(hit)
    }

    /// THE SCROLLBAR a press at `position` (window space) lands on: the bar
    /// PAINTED on top there, and which part of it. `None` leaves the press to
    /// the content.
    ///
    /// Chrome's rule: a scrollbar is hit only where it is painted - clipped
    /// by its ancestors and under whatever paints after it, like any other
    /// box. The scroll manager's tracks
    /// ([`ScrollManager::hit_test_scrollbars`]) know only where each bar
    /// lies, so an `overflow: hidden` ancestor that cut a bar off, or a
    /// dialog painted over it, still lost its press to the bar (`AzDrive`'s
    /// "I was hacked..." button over its file list's clipped bar scrolled the
    /// list). The tracks stay the cheap first filter - most presses are on no
    /// bar at all - and the part pressed; the paint order, read off the
    /// display lists by the layout-side hit tester the shells dispatch with
    /// ([`CpuHitTester::scrollbar_at`]), says which bar, if any, is on top.
    ///
    /// The hit tester is built for the question from the window's own
    /// layout results: the shells keep theirs outside the window, and a
    /// press on a bar is rare. A dom the window holds no layout for (a
    /// scroll manager fed by hand) has no paint order to read, and keeps the
    /// tracks' answer.
    #[must_use]
    pub fn scrollbar_at(&self, position: LogicalPosition) -> Option<ScrollbarHit> {
        let under = self.scroll_manager.hit_test_scrollbars(position)?;
        if !self.layout_results.contains_key(&under.dom_id) {
            return Some(under);
        }
        let mut painted = CpuHitTester::new();
        painted.rebuild_from_layout_with_gpu(&self.layout_results, Some(&self.gpu_state_manager));
        let resolve = |d: DomId, n: NodeId| self.scroll_manager.get_current_offset(d, n);
        let resolve_tf = |d: DomId, n: NodeId| self.gpu_state_manager.painted_transform_of(d, n);
        let (dom_id, node_id, orientation) =
            painted.scrollbar_at(position, &resolve, &resolve_tf)?;
        self.scroll_manager
            .hit_test_scrollbar_axis(dom_id, node_id, orientation, position)
    }

    /// The other half of a thumb press: a pointer move while the thumb is
    /// held scrolls its box, and nothing else sees the move.
    ///
    /// Returns `true` when a held thumb took the move (the caller must not
    /// hand it to the content), `false` when no thumb is held.
    pub fn route_move(&mut self, position: LogicalPosition, now: Instant) -> bool {
        let Some(drag) = self.currently_dragging_thumb else {
            return false;
        };
        let (dom_id, node_id, orientation) = match drag.hit_id {
            ScrollbarHitId::VerticalThumb(d, n) => (d, n, ScrollbarOrientation::Vertical),
            ScrollbarHitId::HorizontalThumb(d, n) => (d, n, ScrollbarOrientation::Horizontal),
            // `route_press` only ever holds a thumb; a track id here would be
            // a drag nobody can move. It still owns the pointer until the
            // release, like any other held thumb.
            ScrollbarHitId::VerticalTrack(..) | ScrollbarHitId::HorizontalTrack(..) => {
                return true;
            }
        };
        if let Some(target) =
            thumb_drag_target(&self.scroll_manager, &drag, dom_id, node_id, orientation, position)
        {
            let current = self
                .scroll_manager
                .get_current_offset(dom_id, node_id)
                .unwrap_or_default();
            let delta = match orientation {
                ScrollbarOrientation::Vertical => target - current.y,
                ScrollbarOrientation::Horizontal => target - current.x,
            };
            scroll_axis_by(&mut self.scroll_manager, dom_id, node_id, orientation, delta, now);
        }
        true
    }

    /// A button release: the primary button lets go of a held thumb.
    ///
    /// Returns `true` when this release ended a drag. The caller then owes
    /// the window state the button's fall, and nothing else: the press was
    /// the scrollbar's, so the release is too.
    pub fn route_release(&mut self, button: MouseButton, now: Instant) -> bool {
        if button != MouseButton::Left || self.currently_dragging_thumb.take().is_none() {
            return false;
        }
        self.scroll_manager.end_thumb_drag(now);
        true
    }

    /// The scrollbar thumb the pointer is holding, if any.
    #[must_use]
    pub const fn scrollbar_drag(&self) -> Option<&ScrollbarDragState> {
        self.currently_dragging_thumb.as_ref()
    }

    /// The press router for a pointer STATE push: what a scripted press,
    /// move or release is (`DebugEvent::MouseDown`, `click`, a callback's
    /// `modify_window_state`). The same arbitration as the physical entry
    /// points above, for the primary pointer.
    ///
    /// `baseline` is the pointer state the event pass will diff against,
    /// `current` the pushed one. Whatever the scrollbar layer takes - the
    /// primary button and the cursor, for a press on a bar and for every
    /// move and the release of a held thumb - is written into `baseline`, so
    /// the pass does not ALSO turn it into a `MouseDown` on the content under
    /// the bar. That is the scripted twin of the shells' sanctioned swallow
    /// (`discard_input_delta`). Other buttons stay in the delta.
    ///
    /// Returns `true` when the scrollbar layer took the pointer change.
    pub fn route_pointer_transition(
        &mut self,
        baseline: &mut MouseState,
        current: &MouseState,
        now: Instant,
    ) -> bool {
        let position = current.cursor_position.get_position();
        let pressed = current.left_down && !baseline.left_down;
        let taken = if self.currently_dragging_thumb.is_some() && !pressed {
            if let Some(p) = position {
                self.route_move(p, now.clone());
            }
            // The release, or a push that says the button is already up (its
            // release went somewhere else): either way nothing holds the
            // thumb any more.
            if !current.left_down {
                self.route_release(MouseButton::Left, now);
            }
            true
        } else if pressed {
            position.is_some_and(|p| self.route_press(p, MouseButton::Left, now).is_scrollbar())
        } else {
            false
        };
        if taken {
            baseline.cursor_position = current.cursor_position;
            baseline.left_down = current.left_down;
        }
        taken
    }
}

/// The `ScrollbarHitId` a hit names: the thumb, or the rest of the bar
/// (track and arrow buttons alike).
const fn scrollbar_hit_id(hit: &ScrollbarHit) -> ScrollbarHitId {
    match (hit.orientation, hit.component) {
        (ScrollbarOrientation::Vertical, ScrollbarComponent::Thumb) => {
            ScrollbarHitId::VerticalThumb(hit.dom_id, hit.node_id)
        }
        (ScrollbarOrientation::Vertical, _) => ScrollbarHitId::VerticalTrack(hit.dom_id, hit.node_id),
        (ScrollbarOrientation::Horizontal, ScrollbarComponent::Thumb) => {
            ScrollbarHitId::HorizontalThumb(hit.dom_id, hit.node_id)
        }
        (ScrollbarOrientation::Horizontal, _) => {
            ScrollbarHitId::HorizontalTrack(hit.dom_id, hit.node_id)
        }
    }
}

/// How far a press on the track or an arrow button scrolls, along the bar's
/// axis: arrow buttons one line toward the arrow, the track by the OS
/// preference (jump to the clicked position, or a page toward it).
///
/// `None` when the bar or its box is gone.
fn track_click_delta(
    scroll: &ScrollManager,
    hit: &ScrollbarHit,
    track_click: ScrollbarTrackClick,
) -> Option<f32> {
    let bar = scroll
        .get_scrollbar_state(hit.dom_id, hit.node_id, hit.orientation)
        .filter(|s| s.visible)?;
    // `get_scroll_node_info`, not the raw state: its `max_scroll_*` prefer
    // `virtual_scroll_size` over `content_rect`, and on a `VirtualView` the
    // content rect holds the VIEWPORT size, so the extent derived from it was
    // zero and `JumpToPosition` a silent no-op on every virtualized list.
    let info = scroll.get_scroll_node_info(hit.dom_id, hit.node_id)?;
    let (click, track_start, track_len, container_len, max_scroll, current) = match hit.orientation
    {
        ScrollbarOrientation::Vertical => (
            hit.global_position.y,
            bar.track_rect.origin.y,
            bar.track_rect.size.height,
            info.container_rect.size.height,
            info.max_scroll_y,
            info.current_offset.y,
        ),
        ScrollbarOrientation::Horizontal => (
            hit.global_position.x,
            bar.track_rect.origin.x,
            bar.track_rect.size.width,
            info.container_rect.size.width,
            info.max_scroll_x,
            info.current_offset.x,
        ),
    };
    // 0.0 = top/left end of the track, 1.0 = bottom/right end.
    let click_ratio = ((click - track_start) / track_len).clamp(0.0, 1.0);
    Some(match hit.component {
        ScrollbarComponent::TopButton => -SCROLLBAR_ARROW_STEP_PX,
        ScrollbarComponent::BottomButton => SCROLLBAR_ARROW_STEP_PX,
        ScrollbarComponent::Track | ScrollbarComponent::Thumb => match track_click {
            ScrollbarTrackClick::JumpToPosition => click_ratio * max_scroll - current,
            ScrollbarTrackClick::PageUpDown => {
                // Page toward the click: before the thumb pages back, past
                // it pages forward.
                let page = container_len * TRACK_PAGE_FRACTION;
                let thumb_center = bar.thumb_position_ratio + bar.thumb_size_ratio * 0.5;
                if click_ratio < thumb_center {
                    -page
                } else {
                    page
                }
            }
        },
    })
}

/// Where a held thumb puts its box when the pointer is at `position`: the
/// offset at the press plus the pointer's travel since, scaled from the
/// thumb's free travel on the track to the box's scroll range.
///
/// `None` when the bar or its box is gone.
fn thumb_drag_target(
    scroll: &ScrollManager,
    drag: &ScrollbarDragState,
    dom_id: DomId,
    node_id: NodeId,
    orientation: ScrollbarOrientation,
    position: LogicalPosition,
) -> Option<f32> {
    let bar = scroll
        .get_scrollbar_state(dom_id, node_id, orientation)
        .filter(|s| s.visible)?;
    // `get_scroll_node_info` for the same `VirtualView` reason as the track
    // click: a thumb that was grabbable but clamped every target to [0, 0].
    let info = scroll.get_scroll_node_info(dom_id, node_id)?;
    let (pixel_delta, track_size, max_scroll, initial) = match orientation {
        ScrollbarOrientation::Vertical => (
            position.y - drag.initial_mouse_pos.y,
            bar.track_rect.size.height,
            info.max_scroll_y,
            drag.initial_scroll_offset.y,
        ),
        ScrollbarOrientation::Horizontal => (
            position.x - drag.initial_mouse_pos.x,
            bar.track_rect.size.width,
            info.max_scroll_x,
            drag.initial_scroll_offset.x,
        ),
    };
    // `max(0.0)` also maps a NaN range to 0, so the clamp below cannot panic.
    let max_scroll = max_scroll.max(0.0);
    let thumb_size = bar.thumb_size_ratio * track_size;
    let usable_track = (track_size - thumb_size).max(1.0);
    let scroll_delta = (pixel_delta / usable_track) * max_scroll;
    Some((initial + scroll_delta).clamp(0.0, max_scroll))
}

/// Move `node_id`'s box by `delta` along `orientation`, immediately, and
/// re-derive the bars so the next hit test sees the thumb where it now is.
///
/// A frame-level change: no relayout and no display-list rebuild, the
/// renderer picks the new offset up from the scroll manager.
fn scroll_axis_by(
    scroll: &mut ScrollManager,
    dom_id: DomId,
    node_id: NodeId,
    orientation: ScrollbarOrientation,
    delta: f32,
    now: Instant,
) {
    let current = scroll.get_current_offset(dom_id, node_id).unwrap_or_default();
    let target = match orientation {
        ScrollbarOrientation::Vertical => LogicalPosition::new(current.x, current.y + delta),
        ScrollbarOrientation::Horizontal => LogicalPosition::new(current.x + delta, current.y),
    };
    scroll.set_scroll_position(dom_id, node_id, target, now);
    scroll.calculate_scrollbar_states();
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use azul_core::{
        dom::{DomId, NodeId, ScrollbarOrientation},
        events::MouseButton,
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        task::Instant,
        window::{CursorPosition, MouseState},
    };
    use rust_fontconfig::FcFontCache;

    use super::{PressTarget, SCROLLBAR_ARROW_STEP_PX};
    use crate::{managers::scroll_state::ScrollbarComponent, window::LayoutWindow};

    const DOM: DomId = DomId { inner: 0 };

    fn node() -> NodeId {
        NodeId::new(1)
    }

    /// A window with one 200x100 box at the origin over 1000px of content,
    /// with a CLASSIC 12px vertical bar (reserved, with arrow buttons).
    fn window_with_a_scroll_box() -> LayoutWindow {
        let mut w = LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new");
        w.scroll_manager.register_or_update_scroll_node(
            DOM,
            node(),
            LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(200.0, 100.0)),
            LogicalSize::new(200.0, 1000.0),
            Instant::now(),
            crate::solver3::scrollbar::ScrollbarPresence::None,
            crate::solver3::scrollbar::ScrollbarPresence::Classic { thickness: 12.0 },
        );
        w.scroll_manager.calculate_scrollbar_states();
        w
    }

    fn bar(w: &LayoutWindow) -> crate::managers::scroll_state::ScrollbarState {
        *w.scroll_manager
            .get_scrollbar_state(DOM, node(), ScrollbarOrientation::Vertical)
            .expect("the box overflows, so it has a vertical bar")
    }

    /// The centre of the thumb, in window space.
    fn thumb_centre(w: &LayoutWindow) -> LogicalPosition {
        let b = bar(w);
        LogicalPosition::new(
            b.track_rect.origin.x + b.track_rect.size.width / 2.0,
            b.track_rect.origin.y + b.button_size + b.thumb_offset + b.thumb_length / 2.0,
        )
    }

    fn offset_y(w: &LayoutWindow) -> f32 {
        w.scroll_manager
            .get_current_offset(DOM, node())
            .unwrap_or_default()
            .y
    }

    fn component(target: PressTarget) -> Option<ScrollbarComponent> {
        match target {
            PressTarget::Scrollbar(hit) => Some(hit.component),
            PressTarget::Content => None,
        }
    }

    #[test]
    fn a_press_beside_every_bar_is_the_contents() {
        let mut w = window_with_a_scroll_box();
        let target = w.route_press(LogicalPosition::new(50.0, 50.0), MouseButton::Left, Instant::now());
        assert!(matches!(target, PressTarget::Content));
        assert!(w.scrollbar_drag().is_none());
        assert_eq!(offset_y(&w), 0.0);
    }

    #[test]
    fn a_press_on_the_thumb_grabs_it_and_scrolls_nothing_yet() {
        let mut w = window_with_a_scroll_box();
        let at = thumb_centre(&w);
        let target = w.route_press(at, MouseButton::Left, Instant::now());
        assert_eq!(component(target), Some(ScrollbarComponent::Thumb));
        let drag = w.scrollbar_drag().expect("a thumb press holds the thumb");
        assert_eq!(drag.initial_mouse_pos, at);
        assert_eq!(offset_y(&w), 0.0);
        assert_eq!(
            w.scroll_manager.thumb_drag(),
            Some((DOM, node(), ScrollbarOrientation::Vertical)),
            "the scroll manager must know the bar is held, or it fades under the pointer"
        );
    }

    /// Only the primary button works a scrollbar: a right press on a thumb is
    /// the content's (its context menu), and grabs nothing.
    #[test]
    fn a_non_primary_press_on_a_thumb_is_the_contents() {
        let mut w = window_with_a_scroll_box();
        let at = thumb_centre(&w);
        for button in [MouseButton::Right, MouseButton::Middle, MouseButton::Other(3)] {
            let target = w.route_press(at, button, Instant::now());
            assert!(matches!(target, PressTarget::Content), "{button:?}");
            assert!(w.scrollbar_drag().is_none(), "{button:?}");
        }
    }

    /// Dragging the thumb maps the pointer's travel onto the scroll range by
    /// the ratio of the thumb's free travel on the track.
    #[test]
    fn moving_a_held_thumb_scrolls_by_the_track_ratio() {
        let mut w = window_with_a_scroll_box();
        let at = thumb_centre(&w);
        let b = bar(&w);
        let _ = w.route_press(at, MouseButton::Left, Instant::now());

        assert!(w.route_move(LogicalPosition::new(at.x, at.y + 10.0), Instant::now()));

        let track = b.track_rect.size.height;
        let max_scroll = 1000.0 - 100.0;
        let expected = 10.0 / (track - b.thumb_size_ratio * track) * max_scroll;
        assert!(
            (offset_y(&w) - expected).abs() < 0.01,
            "expected {expected}, got {}",
            offset_y(&w)
        );
    }

    /// The target is always measured from the PRESS, so a drag far past the
    /// end clamps and a drag back returns exactly where it started.
    #[test]
    fn a_held_thumb_clamps_at_the_ends_and_returns_to_its_start() {
        let mut w = window_with_a_scroll_box();
        let at = thumb_centre(&w);
        let _ = w.route_press(at, MouseButton::Left, Instant::now());

        let _ = w.route_move(LogicalPosition::new(at.x, at.y + 10_000.0), Instant::now());
        assert_eq!(offset_y(&w), 900.0, "clamped at the scroll range");
        let _ = w.route_move(LogicalPosition::new(at.x, at.y - 10_000.0), Instant::now());
        assert_eq!(offset_y(&w), 0.0, "clamped at the start");
        let _ = w.route_move(at, Instant::now());
        assert_eq!(offset_y(&w), 0.0, "back at the press point, back at the press offset");
    }

    #[test]
    fn a_move_without_a_held_thumb_is_the_contents() {
        let mut w = window_with_a_scroll_box();
        let at = thumb_centre(&w);
        assert!(!w.route_move(LogicalPosition::new(at.x, at.y + 10.0), Instant::now()));
        assert_eq!(offset_y(&w), 0.0);
    }

    /// Only the primary release lets go: a right click while the thumb is
    /// held is the content's, and the drag goes on.
    #[test]
    fn the_primary_release_lets_go_of_the_thumb() {
        let mut w = window_with_a_scroll_box();
        let at = thumb_centre(&w);
        let _ = w.route_press(at, MouseButton::Left, Instant::now());

        assert!(!w.route_release(MouseButton::Right, Instant::now()));
        assert!(w.scrollbar_drag().is_some(), "a right release does not end a left drag");

        assert!(w.route_release(MouseButton::Left, Instant::now()));
        assert!(w.scrollbar_drag().is_none());
        assert_eq!(w.scroll_manager.thumb_drag(), None);
        assert!(
            !w.route_release(MouseButton::Left, Instant::now()),
            "with nothing held, a release is the content's"
        );
    }

    /// A press on the track below the thumb pages toward the click (the
    /// default preference): 90% of the box's visible height.
    #[test]
    fn a_press_on_the_track_pages_toward_the_click() {
        let mut w = window_with_a_scroll_box();
        let b = bar(&w);
        let below_thumb = LogicalPosition::new(
            b.track_rect.origin.x + b.track_rect.size.width / 2.0,
            b.track_rect.origin.y + b.track_rect.size.height - b.button_size - 2.0,
        );
        let target = w.route_press(below_thumb, MouseButton::Left, Instant::now());
        assert_eq!(component(target), Some(ScrollbarComponent::Track));
        assert!(w.scrollbar_drag().is_none(), "a track press holds nothing");
        assert!((offset_y(&w) - 90.0).abs() < 0.01, "got {}", offset_y(&w));
    }

    #[test]
    fn a_press_on_an_arrow_button_steps_one_line() {
        let mut w = window_with_a_scroll_box();
        let b = bar(&w);
        let down_arrow = LogicalPosition::new(
            b.track_rect.origin.x + b.track_rect.size.width / 2.0,
            b.track_rect.origin.y + b.track_rect.size.height - b.button_size / 2.0,
        );
        let target = w.route_press(down_arrow, MouseButton::Left, Instant::now());
        assert_eq!(component(target), Some(ScrollbarComponent::BottomButton));
        assert_eq!(offset_y(&w), SCROLLBAR_ARROW_STEP_PX);
    }

    fn pointer(at: LogicalPosition, left_down: bool) -> MouseState {
        MouseState {
            cursor_position: CursorPosition::InWindow(at),
            left_down,
            ..MouseState::default()
        }
    }

    /// The scripted path: a state push that presses on the thumb is taken,
    /// and folded into the baseline so the event pass sees no `MouseDown`.
    /// The following moves and the release are taken the same way.
    #[test]
    fn a_pushed_press_move_and_release_on_a_thumb_are_the_scrollbars() {
        let mut w = window_with_a_scroll_box();
        let at = thumb_centre(&w);
        let to = LogicalPosition::new(at.x, at.y + 10.0);

        let mut baseline = pointer(at, false);
        let pressed = pointer(at, true);
        assert!(w.route_pointer_transition(&mut baseline, &pressed, Instant::now()));
        assert_eq!(baseline, pressed, "the whole press is the scrollbar's");
        assert!(w.scrollbar_drag().is_some());

        let moved = pointer(to, true);
        assert!(w.route_pointer_transition(&mut baseline, &moved, Instant::now()));
        assert_eq!(baseline, moved);
        assert!(offset_y(&w) > 0.0, "the pushed move dragged the thumb");

        let released = pointer(to, false);
        assert!(w.route_pointer_transition(&mut baseline, &released, Instant::now()));
        assert_eq!(baseline, released);
        assert!(w.scrollbar_drag().is_none());
    }

    /// A pushed press beside the bars, and a right press on one, stay in the
    /// delta for the event pass.
    #[test]
    fn a_pushed_press_the_scrollbars_do_not_take_stays_in_the_delta() {
        let mut w = window_with_a_scroll_box();

        let mut baseline = pointer(LogicalPosition::new(50.0, 50.0), false);
        let before = baseline;
        let pressed = pointer(LogicalPosition::new(50.0, 50.0), true);
        assert!(!w.route_pointer_transition(&mut baseline, &pressed, Instant::now()));
        assert_eq!(baseline, before, "the baseline is untouched");

        let at = thumb_centre(&w);
        let mut baseline = pointer(at, false);
        let before = baseline;
        let right = MouseState {
            right_down: true,
            ..pointer(at, false)
        };
        assert!(!w.route_pointer_transition(&mut baseline, &right, Instant::now()));
        assert_eq!(baseline, before);
        assert!(w.scrollbar_drag().is_none());
    }
}
