//! Hover state management for tracking mouse and touch hover history
//!
//! The `HoverManager` records hit test results for multiple input points
//! (mouse, touch, pen) over multiple frames to enable gesture detection
//! (like `DragStart`) that requires analyzing hover patterns over time
//! rather than just the current frame.

use std::collections::{BTreeMap, VecDeque};

use azul_core::{
    dom::DomNodeId,
    events::{EventData, EventSource, EventType, MouseButton, SyntheticEvent},
};

use crate::hit_test::FullHitTest;

/// Maximum number of frames to keep in hover history
const MAX_HOVER_HISTORY: usize = 5;

/// Pick the front-most deepest hovered node across all hit DOMs.
///
/// Iterates DOMs from highest `DomId` (most-nested child, composited on top)
/// to lowest and returns the FRONT-MOST hit of the first DOM that has a
/// regular hit: the smallest `hit_depth` (both hit testers number hits
/// front to back). Ties fall back to the highest `NodeId`, which is what
/// this used to return outright - right while the arena's DFS order was
/// also the depth order, wrong for an inline-docked `<transient-window>`
/// grafted under a zone with a higher id than its own subtree.
/// See [`HoverManager::current_hover_node_full`].
#[must_use]
pub fn deepest_node_across_doms(ht: &FullHitTest) -> Option<DomNodeId> {
    for (dom_id, hit) in ht.hovered_nodes.iter().rev() {
        let front = hit
            .regular_hit_test_nodes
            .iter()
            .min_by(|(a_id, a), (b_id, b)| a.hit_depth.cmp(&b.hit_depth).then(b_id.cmp(a_id)))
            .map(|(node_id, _)| *node_id);
        if let Some(node_id) = front {
            return Some(DomNodeId {
                dom: *dom_id,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                    node_id,
                )),
            });
        }
    }
    None
}

/// The node a mouse press FOCUSES: the nearest focusable ancestor (self
/// included) of the front-most hit, walking that node's OWN DOM only
/// (9g-ii-e-ii).
///
/// `is_focusable(dom, node)` and `parent(dom, node)` are the two questions
/// the walk asks of the layout results; taking them as closures keeps the
/// rule testable without a laid-out document, and lets the e2e runner and
/// the dll share it instead of each keeping a copy - both copies walked
/// EVERY hit DOM in ascending id order and let the LAST focusable win, so a
/// focusable host node under a `VirtualView` page took the focus a click on
/// the page meant for the page (or for nobody: a click on unfocusable page
/// content is a BLUR, not a focus of whatever the page covers).
#[must_use]
pub fn focusable_under_pointer(
    ht: &FullHitTest,
    is_focusable: impl Fn(azul_core::dom::DomId, azul_core::id::NodeId) -> bool,
    parent: impl Fn(azul_core::dom::DomId, azul_core::id::NodeId) -> Option<azul_core::id::NodeId>,
) -> Option<DomNodeId> {
    let target = deepest_node_across_doms(ht)?;
    let dom = target.dom;
    let mut current = target.node.into_crate_internal();
    while let Some(nid) = current {
        if is_focusable(dom, nid) {
            return Some(DomNodeId {
                dom,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(nid)),
            });
        }
        current = parent(dom, nid);
    }
    None
}

/// Which pointer seat an event belongs to: the seat on a mouse or scroll
/// event, the primary for everything else.
#[must_use]
pub fn seat_of_event(event: &SyntheticEvent) -> u64 {
    match &event.data {
        EventData::Mouse(m) => m.seat_id,
        EventData::Scroll(s) => s.seat_id,
        _ => azul_core::window::PRIMARY_POINTER_SEAT,
    }
}

/// An active pointer capture (`CallbackInfo::capture_pointer`): while set,
/// the capturing SEAT's moves and release are delivered to `node` no matter
/// what is under that cursor (W3C `setPointerCapture`). Per seat (9b-ii-b):
/// a capture a second cursor's press started must not swallow the first
/// cursor's moves, and vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerCapture {
    pub seat_id: u64,
    pub node: DomNodeId,
}

/// Retarget the captured seat's `MouseMove` / `MouseOver` / `MouseUp` at the
/// capturing node; returns whether that seat RELEASED (the capture ends with
/// the release). Other seats' events pass through untouched.
pub fn apply_pointer_capture(events: &mut [SyntheticEvent], capture: PointerCapture) -> bool {
    let mut released = false;
    for ev in events.iter_mut() {
        let EventData::Mouse(mouse) = &ev.data else {
            continue;
        };
        if mouse.seat_id != capture.seat_id {
            continue;
        }
        if matches!(
            ev.event_type,
            EventType::MouseMove | EventType::MouseOver | EventType::MouseUp
        ) {
            ev.target = capture.node;
            ev.current_target = capture.node;
            released |= ev.event_type == EventType::MouseUp;
        }
    }
    released
}

/// Identifier for an input point (mouse, touch, pen, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputPointId {
    /// Mouse cursor - the PRIMARY pointer seat.
    Mouse,
    /// Touch point with unique ID (from TouchEvent.id)
    Touch(u64),
    /// A non-primary pointer seat (9b-ii): a second cursor, keyed by the
    /// platform's seat id. Never carries `PRIMARY_POINTER_SEAT`; use
    /// [`InputPointId::for_seat`], which folds the primary into `Mouse`.
    Seat(u64),
}

impl InputPointId {
    /// The input point that tracks pointer seat `seat_id`.
    #[must_use]
    pub const fn for_seat(seat_id: u64) -> Self {
        if seat_id == azul_core::window::PRIMARY_POINTER_SEAT {
            Self::Mouse
        } else {
            Self::Seat(seat_id)
        }
    }
}

/// Manages hover state history for all input points
///
/// Records hit test results for mouse and touch inputs over multiple frames:
/// - `DragStart` detection (requires movement threshold over multiple frames)
/// - Hover-over event detection
/// - Multi-touch gesture detection
/// - Input path analysis
///
/// The manager maintains a separate history for each active input point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverManager {
    /// Hit test history for each input point
    /// Each point has its own ring buffer of the last N frames
    hover_histories: BTreeMap<InputPointId, VecDeque<FullHitTest>>,
    /// The node each mouse button was PRESSED on, per pointer seat, kept
    /// until that button's release has been delivered to it — see
    /// [`Self::apply_press_target_capture`]. Keyed by seat as well as button
    /// (9b-ii): two cursors can hold Left at once, and the second press must
    /// not overwrite the first's target.
    press_targets: Vec<(u64, MouseButton, DomNodeId)>,
}

impl HoverManager {
    /// Create a new empty `HoverManager`
    #[must_use]
    pub const fn new() -> Self {
        Self {
            hover_histories: BTreeMap::new(),
            press_targets: Vec::new(),
        }
    }

    /// The node `button` is currently pressed on by the PRIMARY seat, if its
    /// release is still owed.
    #[must_use]
    pub fn press_target(&self, button: MouseButton) -> Option<DomNodeId> {
        self.press_target_for(azul_core::window::PRIMARY_POINTER_SEAT, button)
    }

    /// [`Self::press_target`] for any pointer seat.
    #[must_use]
    pub fn press_target_for(&self, seat_id: u64, button: MouseButton) -> Option<DomNodeId> {
        self.press_targets
            .iter()
            .find(|(s, b, _)| *s == seat_id && *b == button)
            .map(|(_, _, t)| *t)
    }

    /// Forget every press target (the owed releases will never come — e.g.
    /// the layout window is torn down).
    pub fn clear_press_targets(&mut self) {
        self.press_targets.clear();
    }

    /// PRESS-TARGET CAPTURE.
    ///
    /// Events are derived from window-state diffs and
    /// targeted at whatever is under the pointer NOW, so a widget that
    /// latched on `MouseDown` only saw its `MouseUp` if the pointer was still
    /// over it when the button came up. Every "stuck input" of the demo test
    /// was this: a slider, a map pan, a paint stroke, a split-pane drag —
    /// each released off its node, each kept dragging on plain moves, each
    /// had grown its own "leave = release" workaround. Browsers and toolkits
    /// solve it the same way (implicit pointer capture / `SetCapture`): the
    /// pressed node gets the release.
    ///
    /// Records the target of every `MouseDown` in `events` per button. For
    /// every `MouseUp`, if the pressed node is neither the release target nor
    /// in its propagation path (`in_release_path(press, release)`), a second
    /// `MouseUp` for the pressed node — delivered AT THAT TARGET ONLY, its
    /// ancestors see the real release — is appended. The release through the
    /// hovered node is untouched (click semantics stay hover-based).
    ///
    /// Call once per pass, after `determine_all_events`, before dispatch. A
    /// release derived from a blur (the OS handlers clear the buttons) goes
    /// through the same door, so a press that ends with the pointer outside
    /// the window is released too.
    pub fn apply_press_target_capture(
        &mut self,
        events: &mut Vec<SyntheticEvent>,
        in_release_path: &dyn Fn(DomNodeId, DomNodeId) -> bool,
    ) {
        let mut captured_releases: Vec<SyntheticEvent> = Vec::new();
        for event in events.iter() {
            let EventData::Mouse(mouse) = &event.data else {
                continue;
            };
            match event.event_type {
                EventType::MouseDown => {
                    self.press_targets
                        .retain(|(s, b, _)| !(*s == mouse.seat_id && *b == mouse.button));
                    self.press_targets
                        .push((mouse.seat_id, mouse.button, event.target));
                }
                EventType::MouseUp => {
                    let Some(pos) = self
                        .press_targets
                        .iter()
                        .position(|(s, b, _)| *s == mouse.seat_id && *b == mouse.button)
                    else {
                        continue;
                    };
                    let (_, _, press_target) = self.press_targets.remove(pos);
                    if press_target == event.target || in_release_path(press_target, event.target) {
                        continue;
                    }
                    captured_releases.push(
                        SyntheticEvent::new(
                            EventType::MouseUp,
                            EventSource::Synthetic,
                            press_target,
                            event.timestamp.clone(),
                            event.data.clone(),
                        )
                        .at_target_only(),
                    );
                }
                _ => {}
            }
        }
        events.extend(captured_releases);
    }

    /// (input points, total history entries across all points). Used by
    /// `AZ_E2E_TEST` to watch for unbounded growth.
    #[must_use]
    pub fn debug_counts(&self) -> (usize, usize) {
        let points = self.hover_histories.len();
        let total: usize = self.hover_histories.values().map(VecDeque::len).sum();
        (points, total)
    }

    /// Push a new hit test result for a specific input point
    ///
    /// The most recent result is always at index 0 for that input point.
    /// If the history is full, the oldest frame is dropped.
    pub fn push_hit_test(&mut self, input_id: InputPointId, hit_test: FullHitTest) {
        let history = self
            .hover_histories
            .entry(input_id)
            .or_insert_with(|| VecDeque::with_capacity(MAX_HOVER_HISTORY));

        // Add to front (most recent)
        history.push_front(hit_test);

        // Remove oldest if we exceed the limit
        if history.len() > MAX_HOVER_HISTORY {
            history.pop_back();
        }
    }

    /// Remove an input point's history (e.g., when touch ends)
    pub fn remove_input_point(&mut self, input_id: &InputPointId) {
        self.hover_histories.remove(input_id);
    }

    /// Get the most recent hit test result for an input point
    ///
    /// Returns None if no hit tests have been recorded for this input point.
    #[must_use]
    pub fn get_current(&self, input_id: &InputPointId) -> Option<&FullHitTest> {
        self.hover_histories
            .get(input_id)
            .and_then(|history| history.front())
    }

    /// Get the most recent mouse cursor hit test (convenience method)
    #[must_use]
    pub fn get_current_mouse(&self) -> Option<&FullHitTest> {
        self.get_current(&InputPointId::Mouse)
    }

    /// Get the hit test result from N frames ago for an input point
    /// (0 = current frame)
    ///
    /// Returns None if the requested frame is not in history.
    #[must_use]
    pub fn get_frame(&self, input_id: &InputPointId, frames_ago: usize) -> Option<&FullHitTest> {
        self.hover_histories
            .get(input_id)
            .and_then(|history| history.get(frames_ago))
    }

    /// Get the entire hover history for an input point (most recent first)
    #[must_use]
    pub fn get_history(&self, input_id: &InputPointId) -> Option<&VecDeque<FullHitTest>> {
        self.hover_histories.get(input_id)
    }

    /// Get all currently tracked input points
    #[must_use]
    pub fn get_active_input_points(&self) -> Vec<InputPointId> {
        self.hover_histories.keys().copied().collect()
    }

    /// Get the number of frames in history for an input point
    #[must_use]
    pub fn frame_count(&self, input_id: &InputPointId) -> usize {
        self.hover_histories.get(input_id).map_or(0, VecDeque::len)
    }

    /// Purge every recorded hit-test entry for `dom_id` across all input
    /// points and all history frames.
    ///
    /// Called when a `VirtualView` child DOM is rebuilt IN PLACE (fresh `NodeIds`,
    /// no reconcile mapping — e.g. a `MapWidget` pan rebuilding the tile grid):
    /// the recorded hits for that DOM reference the OLD generation's `NodeIds`,
    /// and consumers that resolve them against the NEW styled DOM read out of
    /// bounds (the `hit_test.rs` cursor panic: "len is 25 but the index is 27")
    /// or target the wrong node. Unlike incremental reconciles there is no
    /// `NodeId` map to `remap` with, so the only safe option is to forget that
    /// DOM's hits; the next pointer move re-populates them from a fresh
    /// hit test.
    pub fn purge_dom(&mut self, dom_id: &azul_core::dom::DomId) {
        for history in self.hover_histories.values_mut() {
            for frame in history.iter_mut() {
                frame.hovered_nodes.remove(dom_id);
            }
        }
        // A press on a node of a purged (rebuilt) dom can never be released
        // to it: the ids are gone.
        self.press_targets.retain(|(_, _, t)| t.dom != *dom_id);
    }

    /// Clear all hover history for all input points
    pub fn clear(&mut self) {
        self.hover_histories.clear();
    }

    /// Clear history for a specific input point
    pub(crate) fn clear_input_point(&mut self, input_id: &InputPointId) {
        if let Some(history) = self.hover_histories.get_mut(input_id) {
            history.clear();
        }
    }

    /// Check if we have enough frames for gesture detection on an input point
    ///
    /// `DragStart` detection requires analyzing movement over multiple frames.
    /// This returns true if we have at least 2 frames of history.
    #[must_use]
    pub fn has_sufficient_history_for_gestures(&self, input_id: &InputPointId) -> bool {
        self.frame_count(input_id) >= 2
    }

    /// Check if any input point has enough history for gesture detection
    #[must_use]
    pub fn any_has_sufficient_history_for_gestures(&self) -> bool {
        self.hover_histories
            .iter()
            .any(|(_, history)| history.len() >= 2)
    }

    /// Get the deepest hovered node from the current mouse hit test.
    ///
    /// Returns the `NodeId` of the most specific (deepest in DOM tree) node
    /// that the mouse cursor is currently over, or None if not hovering anything.
    ///
    /// NOTE: Assumes single-DOM architecture (uses `DomId { inner: 0 }`).
    #[must_use]
    pub fn current_hover_node(&self) -> Option<azul_core::id::NodeId> {
        let current = self.get_current_mouse()?;
        let dom_id = azul_core::dom::DomId { inner: 0 };
        let ht = current.hovered_nodes.get(&dom_id)?;
        ht.regular_hit_test_nodes.keys().last().copied()
    }

    /// Get the deepest hovered node from the previous frame's mouse hit test.
    ///
    /// Returns the `NodeId` from one frame ago, or None if not hovering anything
    /// or no previous frame exists.
    ///
    /// NOTE: Assumes single-DOM architecture (uses `DomId { inner: 0 }`).
    #[must_use]
    pub fn previous_hover_node(&self) -> Option<azul_core::id::NodeId> {
        let history = self.hover_histories.get(&InputPointId::Mouse)?;
        let previous = history.get(1)?; // index 1 = one frame ago
        let dom_id = azul_core::dom::DomId { inner: 0 };
        let ht = previous.hovered_nodes.get(&dom_id)?;
        ht.regular_hit_test_nodes.keys().last().copied()
    }

    /// Multi-DOM aware: the deepest hovered node across ALL hit DOMs (current
    /// frame). Returns a full `DomNodeId` so events can target `VirtualView` /
    /// iframe child DOMs, not just the root.
    ///
    /// Selection rule: prefer the most-nested DOM that was hit. Child DOMs
    /// (`VirtualView` / iframe content) always have higher `DomId`s than their
    /// host and are composited on top of it, so the highest hit `DomId` is the
    /// front-most surface. Within that DOM the deepest node (last in `NodeId`
    /// order) is the W3C event target; bubbling then reaches ancestor handlers.
    ///
    /// For single-DOM apps only `DomId 0` is ever hit, so this is equivalent to
    /// [`current_hover_node`] wrapped in `DomId { inner: 0 }`.
    #[must_use]
    pub fn current_hover_node_full(&self) -> Option<DomNodeId> {
        deepest_node_across_doms(self.get_current_mouse()?)
    }

    /// Multi-DOM aware counterpart of [`previous_hover_node`] (one frame ago).
    #[must_use]
    pub fn previous_hover_node_full(&self) -> Option<DomNodeId> {
        let history = self.hover_histories.get(&InputPointId::Mouse)?;
        deepest_node_across_doms(history.get(1)?)
    }

    /// [`current_hover_node_full`] for ANY input point, not just the mouse.
    ///
    /// Touch event determination needs this: a finger is a pointer of its own,
    /// so a `TouchStart` must target the node under THAT finger. Every getter
    /// here was mouse-only, which is part of why nothing ever derived a touch
    /// event from `FullWindowState::touch_state`.
    #[must_use]
    pub fn hover_node_full_for(&self, input_id: &InputPointId) -> Option<DomNodeId> {
        deepest_node_across_doms(self.get_current(input_id)?)
    }
}

impl crate::managers::NodeIdRemap for HoverManager {
    /// Remap `NodeIds` in all hover histories after DOM reconciliation.
    ///
    /// Hits on unmounted nodes are dropped (they cannot be hovered any more) —
    /// keeping them would make the hover history describe a node that no longer
    /// exists at that index.
    fn remap_node_ids(&mut self, dom_id: azul_core::dom::DomId, map: &crate::managers::NodeIdMap) {
        // A pressed node follows the rebuild like a hovered one; an unmounted
        // press target is dropped (its release can never be delivered).
        self.press_targets = core::mem::take(&mut self.press_targets)
            .into_iter()
            .filter_map(|(s, b, t)| map.resolve_dom_node_id(dom_id, t).map(|t| (s, b, t)))
            .collect();
        let node_id_map = map.as_btree_map();
        for history in self.hover_histories.values_mut() {
            for hit_test in history.iter_mut() {
                if let Some(ht) = hit_test.hovered_nodes.get_mut(&dom_id) {
                    crate::managers::remap_keys(&mut ht.regular_hit_test_nodes, map);
                    crate::managers::remap_keys(&mut ht.scroll_hit_test_nodes, map);
                    crate::managers::remap_keys(&mut ht.cursor_hit_test_nodes, map);

                    // Remap scrollbar_hit_test_nodes (ScrollbarHitId contains NodeId)
                    let old_sb: Vec<_> = ht.scrollbar_hit_test_nodes.keys().copied().collect();
                    let mut new_sb = BTreeMap::new();
                    for old_key in old_sb {
                        let Some(new_key) = remap_scrollbar_hit_id(&old_key, dom_id, node_id_map)
                        else {
                            // node unmounted — drop the scrollbar hit
                            ht.scrollbar_hit_test_nodes.remove(&old_key);
                            continue;
                        };
                        if let Some(item) = ht.scrollbar_hit_test_nodes.remove(&old_key) {
                            new_sb.insert(new_key, item);
                        }
                    }
                    ht.scrollbar_hit_test_nodes = new_sb;
                }
            }
        }
    }
}

impl Default for HoverManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Remap a `ScrollbarHitId`'s `NodeId` using the reconciliation map.
/// `None` = the node was unmounted, so the hit must be dropped.
/// A `ScrollbarHitId` for a different `DomId` is returned unchanged.
fn remap_scrollbar_hit_id(
    id: &azul_core::hit_test::ScrollbarHitId,
    dom_id: azul_core::dom::DomId,
    node_id_map: &BTreeMap<azul_core::id::NodeId, azul_core::id::NodeId>,
) -> Option<azul_core::hit_test::ScrollbarHitId> {
    use azul_core::hit_test::ScrollbarHitId;
    Some(match id {
        ScrollbarHitId::VerticalTrack(d, n) if *d == dom_id => {
            ScrollbarHitId::VerticalTrack(*d, *node_id_map.get(n)?)
        }
        ScrollbarHitId::VerticalThumb(d, n) if *d == dom_id => {
            ScrollbarHitId::VerticalThumb(*d, *node_id_map.get(n)?)
        }
        ScrollbarHitId::HorizontalTrack(d, n) if *d == dom_id => {
            ScrollbarHitId::HorizontalTrack(*d, *node_id_map.get(n)?)
        }
        ScrollbarHitId::HorizontalThumb(d, n) if *d == dom_id => {
            ScrollbarHitId::HorizontalThumb(*d, *node_id_map.get(n)?)
        }
        other => *other,
    })
}

#[cfg(test)]
mod autotest_generated {
    use azul_core::{
        dom::{DomId, DomNodeId, ScrollbarOrientation},
        geom::LogicalPosition,
        hit_test::{
            CursorHitTestItem, CursorType, HitTest, HitTestItem, OverflowingScrollNode,
            ScrollHitTestItem, ScrollbarHitId, ScrollbarHitTestItem,
        },
        id::NodeId,
        styled_dom::NodeHierarchyItemId,
    };

    use super::*;
    use crate::managers::{NodeIdMap, NodeIdRemap};

    // ---------------------------------------------------------------- fixtures

    fn press_dnid(node: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
        }
    }

    fn mouse_event(ty: EventType, button: MouseButton, target: DomNodeId) -> SyntheticEvent {
        use azul_core::events::{KeyModifiers, MouseEventData};
        SyntheticEvent::new(
            ty,
            EventSource::User,
            target,
            azul_core::task::Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 }),
            EventData::Mouse(MouseEventData {
                position: LogicalPosition::zero(),
                button,
                buttons: 0,
                modifiers: KeyModifiers::default(),
                ..Default::default()
            }),
        )
    }

    #[test]
    fn a_release_over_another_node_is_also_delivered_to_the_pressed_node() {
        // REPORTED (the whole "stuck input" family): a widget that latched on
        // MouseDown never saw its MouseUp when the pointer released off it.
        let mut hm = HoverManager::new();
        let never_related = |_: DomNodeId, _: DomNodeId| false;

        let mut events = vec![mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            press_dnid(3),
        )];
        hm.apply_press_target_capture(&mut events, &never_related);
        assert_eq!(events.len(), 1, "a press adds nothing");
        assert_eq!(hm.press_target(MouseButton::Left), Some(press_dnid(3)));

        let mut events = vec![mouse_event(
            EventType::MouseUp,
            MouseButton::Left,
            press_dnid(9),
        )];
        hm.apply_press_target_capture(&mut events, &never_related);
        assert_eq!(events.len(), 2, "the pressed node gets its own release");
        let captured = &events[1];
        assert_eq!(captured.event_type, EventType::MouseUp);
        assert_eq!(captured.target, press_dnid(3));
        assert!(
            captured.at_target_only,
            "its ancestors already saw the real release"
        );
        assert_eq!(
            events[0].target,
            press_dnid(9),
            "the hovered node's release is untouched"
        );
        assert_eq!(
            hm.press_target(MouseButton::Left),
            None,
            "the owed release was delivered"
        );

        // Nothing pending: a stray release adds nothing.
        let mut events = vec![mouse_event(
            EventType::MouseUp,
            MouseButton::Left,
            press_dnid(9),
        )];
        hm.apply_press_target_capture(&mut events, &never_related);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn a_release_on_the_pressed_node_or_its_descendant_is_not_doubled() {
        let mut hm = HoverManager::new();
        // The hovered node 5 is a descendant of the pressed node 3: the real
        // release bubbles through 3 already.
        let descendant_of = |press: DomNodeId, release: DomNodeId| {
            press == press_dnid(3) && release == press_dnid(5)
        };

        let mut events = vec![mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            press_dnid(3),
        )];
        hm.apply_press_target_capture(&mut events, &descendant_of);
        let mut events = vec![mouse_event(
            EventType::MouseUp,
            MouseButton::Left,
            press_dnid(5),
        )];
        hm.apply_press_target_capture(&mut events, &descendant_of);
        assert_eq!(
            events.len(),
            1,
            "no second release when the path already covers the press"
        );

        let mut events = vec![mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            press_dnid(3),
        )];
        hm.apply_press_target_capture(&mut events, &descendant_of);
        let mut events = vec![mouse_event(
            EventType::MouseUp,
            MouseButton::Left,
            press_dnid(3),
        )];
        hm.apply_press_target_capture(&mut events, &descendant_of);
        assert_eq!(events.len(), 1, "same node: one release");
    }

    #[test]
    fn press_targets_are_per_button_and_follow_remaps() {
        let mut hm = HoverManager::new();
        let never_related = |_: DomNodeId, _: DomNodeId| false;
        let mut events = vec![
            mouse_event(EventType::MouseDown, MouseButton::Left, press_dnid(3)),
            mouse_event(EventType::MouseDown, MouseButton::Right, press_dnid(4)),
        ];
        hm.apply_press_target_capture(&mut events, &never_related);
        // Releasing the right button elsewhere releases node 4 only.
        let mut events = vec![mouse_event(
            EventType::MouseUp,
            MouseButton::Right,
            press_dnid(9),
        )];
        hm.apply_press_target_capture(&mut events, &never_related);
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].target, press_dnid(4));
        assert_eq!(hm.press_target(MouseButton::Left), Some(press_dnid(3)));

        // A rebuild renumbers node 3 → 7; the owed release follows it.
        let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(7))]);
        hm.remap_node_ids(DomId { inner: 0 }, &map);
        assert_eq!(hm.press_target(MouseButton::Left), Some(press_dnid(7)));

        // Purging the dom forgets the press.
        hm.purge_dom(&DomId { inner: 0 });
        assert_eq!(hm.press_target(MouseButton::Left), None);
    }

    #[test]
    fn press_targets_are_per_seat() {
        // Two cursors holding Left at once (9b-ii): the second press must not
        // overwrite the first's target, and each release finds its own.
        use azul_core::events::{KeyModifiers, MouseEventData};
        let seat_event = |ty: EventType, seat_id: u64, target: DomNodeId| {
            SyntheticEvent::new(
                ty,
                EventSource::User,
                target,
                azul_core::task::Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 }),
                EventData::Mouse(MouseEventData {
                    position: LogicalPosition::zero(),
                    button: MouseButton::Left,
                    buttons: 0,
                    modifiers: KeyModifiers::default(),
                    seat_id,
                    ..Default::default()
                }),
            )
        };
        let never_related = |_: DomNodeId, _: DomNodeId| false;
        let mut hm = HoverManager::new();
        let mut events = vec![
            seat_event(EventType::MouseDown, 0, press_dnid(3)),
            seat_event(EventType::MouseDown, 9, press_dnid(4)),
        ];
        hm.apply_press_target_capture(&mut events, &never_related);
        assert_eq!(hm.press_target(MouseButton::Left), Some(press_dnid(3)));
        assert_eq!(hm.press_target_for(9, MouseButton::Left), Some(press_dnid(4)));

        // Seat 9 releases elsewhere: node 4 gets the captured release, and the
        // primary's press is still on file.
        let mut events = vec![seat_event(EventType::MouseUp, 9, press_dnid(1))];
        hm.apply_press_target_capture(&mut events, &never_related);
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].target, press_dnid(4));
        assert_eq!(hm.press_target_for(9, MouseButton::Left), None);
        assert_eq!(hm.press_target(MouseButton::Left), Some(press_dnid(3)));
    }

    fn hit_item(depth: u32) -> HitTestItem {
        HitTestItem {
            point_in_viewport: LogicalPosition::zero(),
            point_relative_to_item: Default::default(),
            is_focusable: false,
            is_virtual_view_hit: None,
            hit_depth: depth,
        }
    }

    fn scroll_item() -> ScrollHitTestItem {
        ScrollHitTestItem {
            point_in_viewport: LogicalPosition::zero(),
            point_relative_to_item: Default::default(),
            scroll_node: OverflowingScrollNode::default(),
        }
    }

    fn cursor_item() -> CursorHitTestItem {
        CursorHitTestItem {
            cursor_type: CursorType::Text,
            hit_depth: 0,
            point_in_viewport: LogicalPosition::zero(),
        }
    }

    fn scrollbar_item() -> ScrollbarHitTestItem {
        ScrollbarHitTestItem {
            point_in_viewport: LogicalPosition::zero(),
            point_relative_to_item: Default::default(),
            orientation: ScrollbarOrientation::Vertical,
        }
    }

    fn dom(inner: usize) -> DomId {
        DomId { inner }
    }

    /// A `FullHitTest` where every `(dom, &[node..])` entry is a set of regular hits.
    /// Node ids are inserted in the given (deliberately unsorted) order.
    fn hits(entries: &[(usize, &[usize])]) -> FullHitTest {
        let mut full = FullHitTest::empty(None);
        for (dom_inner, nodes) in entries {
            let ht = full
                .hovered_nodes
                .entry(dom(*dom_inner))
                .or_insert_with(HitTest::empty);
            for n in *nodes {
                ht.regular_hit_test_nodes
                    .insert(NodeId::new(*n), hit_item(0));
            }
        }
        full
    }

    /// `DomNodeId` for `(dom, node)`, matching what the hover getters return.
    fn dom_node(dom_inner: usize, node: usize) -> DomNodeId {
        DomNodeId {
            dom: dom(dom_inner),
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
        }
    }

    /// A manager whose mouse history is `frames` (pushed oldest-first, so the
    /// LAST element ends up at index 0 = current).
    fn mouse_history(frames: Vec<FullHitTest>) -> HoverManager {
        let mut hm = HoverManager::new();
        for f in frames {
            hm.push_hit_test(InputPointId::Mouse, f);
        }
        hm
    }

    // ------------------------------------------- deepest_node_across_doms (other)

    #[test]
    fn deepest_node_across_doms_empty_returns_none() {
        assert_eq!(deepest_node_across_doms(&FullHitTest::empty(None)), None);
    }

    #[test]
    fn deepest_node_across_doms_uses_nodeid_order_not_insertion_order() {
        // Inserted 2, 9, 7 — BTreeMap key order makes 9 the deepest regardless.
        let ht = hits(&[(0, &[2, 9, 7])]);
        assert_eq!(deepest_node_across_doms(&ht), Some(dom_node(0, 9)));
    }

    #[test]
    fn deepest_node_across_doms_prefers_the_front_most_hit_over_the_highest_id() {
        // A grafted subtree (an inline-docked transient window under a zone
        // with a HIGHER id): the grip (3) is in front of the zone (5).
        let mut full = FullHitTest::empty(None);
        let ht = full
            .hovered_nodes
            .entry(dom(0))
            .or_insert_with(HitTest::empty);
        ht.regular_hit_test_nodes
            .insert(NodeId::new(3), hit_item(0));
        ht.regular_hit_test_nodes
            .insert(NodeId::new(5), hit_item(1));
        ht.regular_hit_test_nodes
            .insert(NodeId::new(0), hit_item(2));
        assert_eq!(deepest_node_across_doms(&full), Some(dom_node(0, 3)));
    }

    #[test]
    fn deepest_node_across_doms_prefers_highest_dom_even_if_its_node_is_shallower() {
        // dom 0 has the deeper NodeId (99) but dom 3 is composited on top.
        let ht = hits(&[(0, &[99]), (3, &[1])]);
        assert_eq!(deepest_node_across_doms(&ht), Some(dom_node(3, 1)));
    }

    #[test]
    fn deepest_node_across_doms_skips_dom_with_no_regular_hits() {
        // dom 5 is "hit" but only in the scroll/cursor/scrollbar maps — the
        // front-most DOM with a REGULAR hit (dom 0) must win instead.
        let mut ht = hits(&[(0, &[4])]);
        let mut empty_regular = HitTest::empty();
        empty_regular
            .scroll_hit_test_nodes
            .insert(NodeId::new(1), scroll_item());
        empty_regular
            .cursor_hit_test_nodes
            .insert(NodeId::new(1), cursor_item());
        empty_regular.scrollbar_hit_test_nodes.insert(
            ScrollbarHitId::VerticalThumb(dom(5), NodeId::new(1)),
            scrollbar_item(),
        );
        ht.hovered_nodes.insert(dom(5), empty_regular);

        assert_eq!(deepest_node_across_doms(&ht), Some(dom_node(0, 4)));
    }

    #[test]
    fn deepest_node_across_doms_all_doms_empty_returns_none() {
        let mut ht = FullHitTest::empty(None);
        ht.hovered_nodes.insert(dom(0), HitTest::empty());
        ht.hovered_nodes.insert(dom(usize::MAX), HitTest::empty());
        assert_eq!(deepest_node_across_doms(&ht), None);
    }

    #[test]
    fn deepest_node_across_doms_extreme_ids_survive_the_nodeid_encoding() {
        // usize::MAX - 1 is the largest NodeId that survives the 1-based
        // `NodeHierarchyItemId` encode (n + 1) without wrapping.
        let max_node = usize::MAX - 1;
        let ht = hits(&[(usize::MAX, &[0, max_node])]);
        let got = deepest_node_across_doms(&ht).expect("a hit exists");
        assert_eq!(got.dom, dom(usize::MAX));
        // The DomNodeId must decode back to exactly the NodeId that was hit.
        assert_eq!(got.node.into_crate_internal(), Some(NodeId::new(max_node)));
    }

    // ------------------------------------------------- new / Default (constructor)

    #[test]
    fn new_manager_is_empty_and_every_getter_is_none_or_zero() {
        let hm = HoverManager::new();
        let mouse = InputPointId::Mouse;

        assert_eq!(hm.debug_counts(), (0, 0));
        assert!(hm.get_active_input_points().is_empty());
        assert!(hm.get_current(&mouse).is_none());
        assert!(hm.get_current_mouse().is_none());
        assert!(hm.get_history(&mouse).is_none());
        assert_eq!(hm.frame_count(&mouse), 0);
        assert!(!hm.has_sufficient_history_for_gestures(&mouse));
        assert!(!hm.any_has_sufficient_history_for_gestures());
        assert!(hm.current_hover_node().is_none());
        assert!(hm.previous_hover_node().is_none());
        assert!(hm.current_hover_node_full().is_none());
        assert!(hm.previous_hover_node_full().is_none());
        // Frame lookups on an unknown point must not panic at any index.
        assert!(hm.get_frame(&mouse, 0).is_none());
        assert!(hm.get_frame(&mouse, usize::MAX).is_none());
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(HoverManager::default(), HoverManager::new());
    }

    // ------------------------------------------------- push_hit_test (numeric)

    #[test]
    fn push_hit_test_index_zero_is_the_newest_frame() {
        let hm = mouse_history(vec![hits(&[(0, &[1])]), hits(&[(0, &[2])])]);

        assert_eq!(hm.frame_count(&InputPointId::Mouse), 2);
        assert_eq!(
            hm.get_frame(&InputPointId::Mouse, 0),
            Some(&hits(&[(0, &[2])]))
        );
        assert_eq!(
            hm.get_frame(&InputPointId::Mouse, 1),
            Some(&hits(&[(0, &[1])]))
        );
        assert_eq!(hm.get_current_mouse(), Some(&hits(&[(0, &[2])])));
    }

    #[test]
    fn push_hit_test_ring_buffer_never_exceeds_max_hover_history() {
        let mut hm = HoverManager::new();
        for i in 0..1000_usize {
            hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[i])]));
        }

        assert_eq!(hm.frame_count(&InputPointId::Mouse), MAX_HOVER_HISTORY);
        assert_eq!(hm.debug_counts(), (1, MAX_HOVER_HISTORY));
        // The retained window is the LAST MAX_HOVER_HISTORY pushes, newest first.
        for ago in 0..MAX_HOVER_HISTORY {
            assert_eq!(
                hm.get_frame(&InputPointId::Mouse, ago),
                Some(&hits(&[(0, &[999 - ago])])),
                "frame {ago} frames ago"
            );
        }
        // Anything older was dropped.
        assert!(hm
            .get_frame(&InputPointId::Mouse, MAX_HOVER_HISTORY)
            .is_none());
    }

    #[test]
    fn get_frame_out_of_range_index_returns_none_without_overflow() {
        let hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let mouse = InputPointId::Mouse;

        assert!(hm.get_frame(&mouse, 0).is_some());
        assert!(hm.get_frame(&mouse, 1).is_none());
        assert!(hm.get_frame(&mouse, usize::MAX).is_none());
        assert!(hm.get_frame(&mouse, usize::MAX / 2).is_none());
        // Unknown input point at a huge index is still just None.
        assert!(hm
            .get_frame(&InputPointId::Touch(u64::MAX), usize::MAX)
            .is_none());
    }

    #[test]
    fn touch_ids_at_u64_boundaries_are_distinct_histories() {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Touch(u64::MIN), hits(&[(0, &[1])]));
        hm.push_hit_test(InputPointId::Touch(u64::MAX), hits(&[(0, &[2])]));
        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[3])]));

        assert_eq!(hm.debug_counts(), (3, 3));
        assert_eq!(
            hm.get_current(&InputPointId::Touch(u64::MIN)),
            Some(&hits(&[(0, &[1])]))
        );
        assert_eq!(
            hm.get_current(&InputPointId::Touch(u64::MAX)),
            Some(&hits(&[(0, &[2])]))
        );
        assert_eq!(hm.get_current_mouse(), Some(&hits(&[(0, &[3])])));
        // Ord derive: Mouse sorts before every Touch, touches sort by id.
        assert_eq!(
            hm.get_active_input_points(),
            vec![
                InputPointId::Mouse,
                InputPointId::Touch(0),
                InputPointId::Touch(u64::MAX),
            ]
        );
    }

    #[test]
    fn debug_counts_stays_bounded_under_a_flood_of_points_and_frames() {
        let mut hm = HoverManager::new();
        for point in 0..100_u64 {
            for frame in 0..50_usize {
                hm.push_hit_test(InputPointId::Touch(point), hits(&[(0, &[frame])]));
            }
        }
        // 100 points, each capped at MAX_HOVER_HISTORY frames — no unbounded growth.
        assert_eq!(hm.debug_counts(), (100, 100 * MAX_HOVER_HISTORY));
    }

    #[test]
    fn push_hit_test_stores_the_value_verbatim_including_focused_node() {
        let focused = dom_node(0, 7);
        let mut ht = FullHitTest::empty(Some(focused));
        ht.hovered_nodes.insert(dom(0), HitTest::empty());

        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, ht.clone());

        assert_eq!(hm.get_current_mouse(), Some(&ht));
        assert_eq!(
            hm.get_current_mouse().map(|h| h.focused_node),
            Some(Some(focused).into())
        );
        // A hovered DOM with zero hits is still "no hovered node".
        assert!(hm.current_hover_node().is_none());
        assert!(hm.current_hover_node_full().is_none());
    }

    #[test]
    fn get_history_returns_all_frames_newest_first() {
        let hm = mouse_history(vec![
            hits(&[(0, &[1])]),
            hits(&[(0, &[2])]),
            hits(&[(0, &[3])]),
        ]);
        let history = hm
            .get_history(&InputPointId::Mouse)
            .expect("history exists");

        assert_eq!(history.len(), 3);
        assert_eq!(history[0], hits(&[(0, &[3])]));
        assert_eq!(history[2], hits(&[(0, &[1])]));
        assert!(hm.get_history(&InputPointId::Touch(0)).is_none());
    }

    // --------------------------------------- remove / clear / clear_input_point

    #[test]
    fn remove_absent_input_point_is_a_noop() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let before = hm.clone();

        hm.remove_input_point(&InputPointId::Touch(0));
        hm.remove_input_point(&InputPointId::Touch(u64::MAX));

        assert_eq!(hm, before);
    }

    #[test]
    fn remove_input_point_only_drops_the_target_point() {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[1])]));
        hm.push_hit_test(InputPointId::Touch(3), hits(&[(0, &[2])]));

        hm.remove_input_point(&InputPointId::Touch(3));

        assert_eq!(hm.debug_counts(), (1, 1));
        assert_eq!(hm.get_active_input_points(), vec![InputPointId::Mouse]);
        assert!(hm.get_current(&InputPointId::Touch(3)).is_none());
        assert_eq!(hm.frame_count(&InputPointId::Touch(3)), 0);
        assert!(hm.get_current_mouse().is_some());
    }

    #[test]
    fn remove_then_push_restarts_the_history_from_scratch() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])]), hits(&[(0, &[2])])]);
        assert!(hm.has_sufficient_history_for_gestures(&InputPointId::Mouse));

        hm.remove_input_point(&InputPointId::Mouse);
        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[3])]));

        assert_eq!(hm.frame_count(&InputPointId::Mouse), 1);
        assert!(!hm.has_sufficient_history_for_gestures(&InputPointId::Mouse));
        assert!(hm.previous_hover_node().is_none());
    }

    #[test]
    fn clear_drops_every_point() {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[1])]));
        hm.push_hit_test(InputPointId::Touch(9), hits(&[(0, &[2])]));

        hm.clear();

        assert_eq!(hm, HoverManager::new());
        assert_eq!(hm.debug_counts(), (0, 0));
        assert!(!hm.any_has_sufficient_history_for_gestures());
        // Clearing twice is still fine.
        hm.clear();
        assert_eq!(hm.debug_counts(), (0, 0));
    }

    #[test]
    fn clear_input_point_empties_history_but_keeps_the_point_registered() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])]), hits(&[(0, &[2])])]);

        hm.clear_input_point(&InputPointId::Mouse);

        // The point remains a key with an EMPTY deque (unlike remove_input_point).
        assert_eq!(hm.debug_counts(), (1, 0));
        assert_eq!(hm.get_active_input_points(), vec![InputPointId::Mouse]);
        assert_eq!(hm.frame_count(&InputPointId::Mouse), 0);
        assert!(hm.get_current_mouse().is_none());
        assert!(hm.get_history(&InputPointId::Mouse).is_some());
        assert!(!hm.has_sufficient_history_for_gestures(&InputPointId::Mouse));
        assert!(!hm.any_has_sufficient_history_for_gestures());
        assert!(hm.current_hover_node().is_none());
        assert!(hm.previous_hover_node().is_none());
    }

    #[test]
    fn clear_input_point_on_an_absent_point_is_a_noop() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let before = hm.clone();

        hm.clear_input_point(&InputPointId::Touch(u64::MAX));

        assert_eq!(hm, before);
    }

    // ------------------------------------------------------------- predicates

    #[test]
    fn has_sufficient_history_needs_at_least_two_frames() {
        let mouse = InputPointId::Mouse;
        let mut hm = HoverManager::new();
        assert!(!hm.has_sufficient_history_for_gestures(&mouse));

        hm.push_hit_test(mouse, hits(&[(0, &[1])]));
        assert!(
            !hm.has_sufficient_history_for_gestures(&mouse),
            "1 frame is not enough"
        );

        hm.push_hit_test(mouse, hits(&[(0, &[2])]));
        assert!(
            hm.has_sufficient_history_for_gestures(&mouse),
            "2 frames is the threshold"
        );

        for i in 0..10 {
            hm.push_hit_test(mouse, hits(&[(0, &[i])]));
        }
        assert!(
            hm.has_sufficient_history_for_gestures(&mouse),
            "stays true when saturated"
        );
    }

    #[test]
    fn any_has_sufficient_history_is_an_or_across_points() {
        let mut hm = HoverManager::new();
        // Three points with one frame each => still false.
        for id in [
            InputPointId::Mouse,
            InputPointId::Touch(0),
            InputPointId::Touch(u64::MAX),
        ] {
            hm.push_hit_test(id, hits(&[(0, &[1])]));
        }
        assert!(!hm.any_has_sufficient_history_for_gestures());

        // A single point reaching 2 frames flips it.
        hm.push_hit_test(InputPointId::Touch(u64::MAX), hits(&[(0, &[2])]));
        assert!(hm.any_has_sufficient_history_for_gestures());

        // Emptying that point's history flips it back.
        hm.clear_input_point(&InputPointId::Touch(u64::MAX));
        assert!(!hm.any_has_sufficient_history_for_gestures());
    }

    // ---------------------------------------------------- hover node getters

    #[test]
    fn current_hover_node_returns_the_deepest_node_of_dom_zero() {
        let hm = mouse_history(vec![hits(&[(0, &[3, 8, 5])])]);

        assert_eq!(hm.current_hover_node(), Some(NodeId::new(8)));
        // Single-DOM: the _full variant is the same node wrapped in DomId 0.
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(0, 8)));
    }

    #[test]
    fn current_hover_node_ignores_non_zero_doms_but_full_does_not() {
        // Only a child DOM was hit — the single-DOM getter is blind to it.
        let hm = mouse_history(vec![hits(&[(2, &[4])])]);

        assert_eq!(hm.current_hover_node(), None);
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(2, 4)));
    }

    #[test]
    fn current_hover_node_full_prefers_the_front_most_child_dom() {
        let hm = mouse_history(vec![hits(&[(0, &[9]), (1, &[2])])]);

        // The root getter still reports the root's deepest node...
        assert_eq!(hm.current_hover_node(), Some(NodeId::new(9)));
        // ...while the multi-DOM getter targets the composited-on-top child.
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(1, 2)));
    }

    #[test]
    fn previous_hover_node_is_none_until_a_second_frame_exists() {
        let hm = mouse_history(vec![hits(&[(0, &[1])])]);

        assert_eq!(hm.current_hover_node(), Some(NodeId::new(1)));
        assert_eq!(hm.previous_hover_node(), None);
        assert_eq!(hm.previous_hover_node_full(), None);
    }

    #[test]
    fn previous_hover_node_reads_frame_one_not_the_oldest_frame() {
        // 6 pushes => the oldest (node 0) is evicted; frame 1 is node 4.
        let hm = mouse_history((0..6).map(|i| hits(&[(0, &[i])])).collect());

        assert_eq!(hm.current_hover_node(), Some(NodeId::new(5)));
        assert_eq!(hm.previous_hover_node(), Some(NodeId::new(4)));
        assert_eq!(hm.previous_hover_node_full(), Some(dom_node(0, 4)));
    }

    #[test]
    fn previous_hover_node_full_sees_child_doms_of_the_previous_frame() {
        let hm = mouse_history(vec![hits(&[(0, &[1]), (7, &[3])]), hits(&[(0, &[2])])]);

        assert_eq!(hm.previous_hover_node(), Some(NodeId::new(1)));
        assert_eq!(hm.previous_hover_node_full(), Some(dom_node(7, 3)));
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(0, 2)));
    }

    #[test]
    fn hover_node_getters_are_none_when_the_frame_hit_nothing() {
        let hm = mouse_history(vec![FullHitTest::empty(None), FullHitTest::empty(None)]);

        assert!(hm.current_hover_node().is_none());
        assert!(hm.previous_hover_node().is_none());
        assert!(hm.current_hover_node_full().is_none());
        assert!(hm.previous_hover_node_full().is_none());
    }

    #[test]
    fn hover_node_getters_ignore_touch_history_entirely() {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Touch(1), hits(&[(0, &[5])]));
        hm.push_hit_test(InputPointId::Touch(1), hits(&[(0, &[6])]));

        assert!(hm.current_hover_node().is_none());
        assert!(hm.previous_hover_node().is_none());
        assert!(hm.current_hover_node_full().is_none());
        assert!(hm.previous_hover_node_full().is_none());
        assert!(hm.any_has_sufficient_history_for_gestures());
    }

    // ------------------------------------------------------ purge_dom (other)

    #[test]
    fn purge_dom_removes_that_dom_from_every_frame_of_every_point() {
        let mut hm = HoverManager::new();
        for id in [InputPointId::Mouse, InputPointId::Touch(2)] {
            hm.push_hit_test(id, hits(&[(0, &[1]), (1, &[2])]));
            hm.push_hit_test(id, hits(&[(0, &[3]), (1, &[4])]));
        }

        hm.purge_dom(&dom(1));

        // Frames themselves are kept — only DOM 1's hits are forgotten.
        assert_eq!(hm.debug_counts(), (2, 4));
        for id in [InputPointId::Mouse, InputPointId::Touch(2)] {
            let history = hm.get_history(&id).expect("history exists");
            for frame in history {
                assert!(!frame.hovered_nodes.contains_key(&dom(1)));
                assert!(frame.hovered_nodes.contains_key(&dom(0)));
            }
        }
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(0, 3)));
        assert_eq!(hm.previous_hover_node_full(), Some(dom_node(0, 1)));
    }

    #[test]
    fn purge_dom_zero_leaves_child_dom_hits_intact() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1]), (4, &[2])])]);

        hm.purge_dom(&dom(0));

        // The single-DOM getter now finds nothing, the multi-DOM one falls back.
        assert_eq!(hm.current_hover_node(), None);
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(4, 2)));
        assert_eq!(hm.frame_count(&InputPointId::Mouse), 1);
    }

    #[test]
    fn purge_absent_or_extreme_dom_id_is_a_noop() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let before = hm.clone();

        hm.purge_dom(&dom(9));
        hm.purge_dom(&dom(usize::MAX));
        assert_eq!(hm, before);

        // Purging on an empty manager must not panic either.
        let mut empty = HoverManager::new();
        empty.purge_dom(&dom(0));
        assert_eq!(empty, HoverManager::new());
    }

    #[test]
    fn purge_dom_twice_is_idempotent() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1]), (1, &[2])])]);

        hm.purge_dom(&dom(1));
        let once = hm.clone();
        hm.purge_dom(&dom(1));

        assert_eq!(hm, once);
    }

    // -------------------------------------------- remap_scrollbar_hit_id (other)

    fn sb_map(pairs: &[(usize, usize)]) -> BTreeMap<NodeId, NodeId> {
        pairs
            .iter()
            .map(|(o, n)| (NodeId::new(*o), NodeId::new(*n)))
            .collect()
    }

    #[test]
    fn remap_scrollbar_hit_id_rewrites_every_variant_of_the_target_dom() {
        let map = sb_map(&[(1, 42)]);
        let d = dom(0);
        let old = NodeId::new(1);
        let new = NodeId::new(42);

        assert_eq!(
            remap_scrollbar_hit_id(&ScrollbarHitId::VerticalTrack(d, old), d, &map),
            Some(ScrollbarHitId::VerticalTrack(d, new))
        );
        assert_eq!(
            remap_scrollbar_hit_id(&ScrollbarHitId::VerticalThumb(d, old), d, &map),
            Some(ScrollbarHitId::VerticalThumb(d, new))
        );
        assert_eq!(
            remap_scrollbar_hit_id(&ScrollbarHitId::HorizontalTrack(d, old), d, &map),
            Some(ScrollbarHitId::HorizontalTrack(d, new))
        );
        assert_eq!(
            remap_scrollbar_hit_id(&ScrollbarHitId::HorizontalThumb(d, old), d, &map),
            Some(ScrollbarHitId::HorizontalThumb(d, new))
        );
    }

    #[test]
    fn remap_scrollbar_hit_id_drops_unmounted_nodes() {
        let map = sb_map(&[(1, 42)]);
        let d = dom(0);
        // Node 2 is absent from the map => unmounted => the hit must be dropped.
        let unmounted = ScrollbarHitId::VerticalThumb(d, NodeId::new(2));

        assert_eq!(remap_scrollbar_hit_id(&unmounted, d, &map), None);
        // Empty map: everything on the target DOM is unmounted.
        let hit = ScrollbarHitId::VerticalThumb(d, NodeId::new(1));
        assert_eq!(remap_scrollbar_hit_id(&hit, d, &BTreeMap::new()), None);
    }

    #[test]
    fn remap_scrollbar_hit_id_passes_other_doms_through_untouched() {
        // The map applies to DOM 0 only; an id naming DOM 1 must NOT be rewritten
        // even though its NodeId happens to be a key in the map.
        let map = sb_map(&[(1, 42)]);
        let other = ScrollbarHitId::HorizontalTrack(dom(1), NodeId::new(1));

        assert_eq!(remap_scrollbar_hit_id(&other, dom(0), &map), Some(other));
        // ...and it survives an empty map too (no accidental drop).
        assert_eq!(
            remap_scrollbar_hit_id(&other, dom(0), &BTreeMap::new()),
            Some(other)
        );
    }

    #[test]
    fn remap_scrollbar_hit_id_handles_extreme_ids() {
        let big = usize::MAX - 1;
        let map = sb_map(&[(big, 0)]);
        let d = dom(usize::MAX);

        assert_eq!(
            remap_scrollbar_hit_id(&ScrollbarHitId::VerticalTrack(d, NodeId::new(big)), d, &map),
            Some(ScrollbarHitId::VerticalTrack(d, NodeId::ZERO))
        );
    }

    // ------------------------------------------------ NodeIdRemap::remap_node_ids

    /// A hit test with one regular + scroll + cursor + scrollbar hit on `node`.
    fn all_maps_hit(dom_inner: usize, node: usize) -> FullHitTest {
        let mut full = FullHitTest::empty(None);
        let mut ht = HitTest::empty();
        ht.regular_hit_test_nodes
            .insert(NodeId::new(node), hit_item(0));
        ht.scroll_hit_test_nodes
            .insert(NodeId::new(node), scroll_item());
        ht.cursor_hit_test_nodes
            .insert(NodeId::new(node), cursor_item());
        ht.scrollbar_hit_test_nodes.insert(
            ScrollbarHitId::VerticalThumb(dom(dom_inner), NodeId::new(node)),
            scrollbar_item(),
        );
        full.hovered_nodes.insert(dom(dom_inner), ht);
        full
    }

    #[test]
    fn remap_node_ids_rewrites_all_four_hit_maps() {
        let mut hm = mouse_history(vec![all_maps_hit(0, 3)]);

        hm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(11))]),
        );

        let ht = &hm.get_current_mouse().expect("frame exists").hovered_nodes[&dom(0)];
        assert_eq!(
            ht.regular_hit_test_nodes
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![NodeId::new(11)]
        );
        assert_eq!(
            ht.scroll_hit_test_nodes.keys().copied().collect::<Vec<_>>(),
            vec![NodeId::new(11)]
        );
        assert_eq!(
            ht.cursor_hit_test_nodes.keys().copied().collect::<Vec<_>>(),
            vec![NodeId::new(11)]
        );
        assert_eq!(
            ht.scrollbar_hit_test_nodes
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![ScrollbarHitId::VerticalThumb(dom(0), NodeId::new(11))]
        );
        assert_eq!(hm.current_hover_node(), Some(NodeId::new(11)));
    }

    #[test]
    fn remap_node_ids_with_an_empty_map_drops_every_hit_of_that_dom() {
        let mut hm = mouse_history(vec![all_maps_hit(0, 3)]);

        // Empty map = nothing matched = every node was unmounted.
        hm.remap_node_ids(dom(0), &NodeIdMap::default());

        let ht = &hm.get_current_mouse().expect("frame exists").hovered_nodes[&dom(0)];
        assert!(ht.regular_hit_test_nodes.is_empty());
        assert!(ht.scroll_hit_test_nodes.is_empty());
        assert!(ht.cursor_hit_test_nodes.is_empty());
        assert!(ht.scrollbar_hit_test_nodes.is_empty());
        assert_eq!(hm.current_hover_node(), None);
        // The (now empty) DOM entry itself is kept — only purge_dom removes it.
        assert!(hm
            .get_current_mouse()
            .expect("frame exists")
            .hovered_nodes
            .contains_key(&dom(0)));
    }

    #[test]
    fn remap_node_ids_drops_unmounted_but_keeps_survivors() {
        // Nodes 1 and 4 hit; only 4 survives the rebuild (as node 0).
        let mut hm = mouse_history(vec![hits(&[(0, &[1, 4])])]);

        hm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(4), NodeId::ZERO)]),
        );

        let ht = &hm.get_current_mouse().expect("frame exists").hovered_nodes[&dom(0)];
        assert_eq!(
            ht.regular_hit_test_nodes
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![NodeId::ZERO]
        );
        assert_eq!(hm.current_hover_node(), Some(NodeId::ZERO));
    }

    #[test]
    fn remap_node_ids_swap_does_not_lose_or_alias_entries() {
        // 1 -> 2 and 2 -> 1 in the same pass: the naive in-place rewrite would
        // clobber one of them. Both must survive with their items swapped.
        let mut full = FullHitTest::empty(None);
        let mut ht = HitTest::empty();
        ht.regular_hit_test_nodes
            .insert(NodeId::new(1), hit_item(10));
        ht.regular_hit_test_nodes
            .insert(NodeId::new(2), hit_item(20));
        full.hovered_nodes.insert(dom(0), ht);
        let mut hm = mouse_history(vec![full]);

        hm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([
                (NodeId::new(1), NodeId::new(2)),
                (NodeId::new(2), NodeId::new(1)),
            ]),
        );

        let ht = &hm.get_current_mouse().expect("frame exists").hovered_nodes[&dom(0)];
        assert_eq!(ht.regular_hit_test_nodes.len(), 2);
        assert_eq!(ht.regular_hit_test_nodes[&NodeId::new(2)].hit_depth, 10);
        assert_eq!(ht.regular_hit_test_nodes[&NodeId::new(1)].hit_depth, 20);
    }

    // ------------------------------------------------- apply_pointer_capture

    fn seat_move(seat_id: u64, target: DomNodeId, ty: EventType) -> SyntheticEvent {
        use azul_core::events::{KeyModifiers, MouseEventData};
        SyntheticEvent::new(
            ty,
            EventSource::User,
            target,
            azul_core::task::Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 }),
            EventData::Mouse(MouseEventData {
                position: LogicalPosition::zero(),
                button: MouseButton::Left,
                buttons: 0,
                modifiers: KeyModifiers::default(),
                seat_id,
                ..Default::default()
            }),
        )
    }

    #[test]
    fn a_capture_retargets_only_its_own_seat_and_ends_on_that_seats_release() {
        // Seat 0 captured node 3; seat 9 is moving over node 5 on its own.
        let capture = PointerCapture {
            seat_id: 0,
            node: press_dnid(3),
        };
        let mut events = vec![
            seat_move(0, press_dnid(8), EventType::MouseMove),
            seat_move(9, press_dnid(5), EventType::MouseMove),
            seat_move(9, press_dnid(5), EventType::MouseUp),
        ];
        let released = apply_pointer_capture(&mut events, capture);
        assert_eq!(events[0].target, press_dnid(3), "the captured seat's move went to the node");
        assert_eq!(events[1].target, press_dnid(5), "the other seat's move did not");
        assert!(!released, "the OTHER seat's release does not end this capture");

        let mut events = vec![seat_move(0, press_dnid(8), EventType::MouseUp)];
        assert!(apply_pointer_capture(&mut events, capture));
        assert_eq!(events[0].target, press_dnid(3));
    }

    // ------------------------------------------------- focusable_under_pointer

    /// Hits: dom 0 (the host) node 3 at depth 1; dom 1 (a page over it) node
    /// 2 at depth 0. Parents: in dom 0, 3 -> 1 -> 0; in dom 1, 2 -> 0.
    fn host_and_page() -> FullHitTest {
        let mut full = FullHitTest::empty(None);
        let mut host = HitTest::empty();
        host.regular_hit_test_nodes.insert(NodeId::new(3), hit_item(1));
        full.hovered_nodes.insert(dom(0), host);
        let mut page = HitTest::empty();
        page.regular_hit_test_nodes.insert(NodeId::new(2), hit_item(0));
        full.hovered_nodes.insert(dom(1), page);
        full
    }

    fn parent_of(d: DomId, n: NodeId) -> Option<NodeId> {
        match (d.inner, n.index()) {
            (0, 3) => Some(NodeId::new(1)),
            (0, 1) => Some(NodeId::new(0)),
            (1, 2) => Some(NodeId::new(0)),
            _ => None,
        }
    }

    #[test]
    fn a_click_on_unfocusable_page_content_does_not_focus_the_host_beneath() {
        // THE DEFECT: both copies of this scan walked every hit DOM and let
        // the last focusable win, so the host's focusable node 1 - under the
        // page, not in the click's own DOM at all - took the focus.
        let ht = host_and_page();
        let host_node_1_is_focusable = |d: DomId, n: NodeId| d.inner == 0 && n.index() == 1;
        assert_eq!(
            focusable_under_pointer(&ht, host_node_1_is_focusable, parent_of),
            None,
            "nothing focusable in the page's own chain: a blur, not the host"
        );
    }

    #[test]
    fn a_click_on_a_page_focuses_the_pages_own_focusable_ancestor() {
        let ht = host_and_page();
        // Both DOMs have a focusable root; the page's wins because the page
        // is the front-most surface.
        let roots = |_: DomId, n: NodeId| n.index() == 0;
        let got = focusable_under_pointer(&ht, roots, parent_of).expect("the page root");
        assert_eq!(got.dom, dom(1));
        assert_eq!(got.node.into_crate_internal(), Some(NodeId::new(0)));
    }

    #[test]
    fn the_walk_starts_at_the_front_most_hit_not_the_largest_id() {
        // One DOM, node 7 (depth 2) behind node 4 (depth 0); 7 is focusable,
        // 4's chain is not. The old largest-id proxy would have focused 7.
        let mut full = FullHitTest::empty(None);
        let mut ht = HitTest::empty();
        ht.regular_hit_test_nodes.insert(NodeId::new(4), hit_item(0));
        ht.regular_hit_test_nodes.insert(NodeId::new(7), hit_item(2));
        full.hovered_nodes.insert(dom(0), ht);
        let seven = |_: DomId, n: NodeId| n.index() == 7;
        let no_parents = |_: DomId, _: NodeId| None;
        assert_eq!(focusable_under_pointer(&full, seven, no_parents), None);
        assert!(focusable_under_pointer(&FullHitTest::empty(None), seven, no_parents).is_none());
    }

    #[test]
    fn remap_node_ids_can_change_which_node_is_deepest() {
        // Old order: 7 is deepest. The rebuild renumbers 3 -> 9 and 7 -> 2,
        // so the deepest hit must be recomputed (9), not carried over.
        let mut hm = mouse_history(vec![hits(&[(0, &[3, 7])])]);
        assert_eq!(hm.current_hover_node(), Some(NodeId::new(7)));

        hm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([
                (NodeId::new(3), NodeId::new(9)),
                (NodeId::new(7), NodeId::new(2)),
            ]),
        );

        assert_eq!(hm.current_hover_node(), Some(NodeId::new(9)));
        assert_eq!(hm.current_hover_node_full(), Some(dom_node(0, 9)));
    }

    #[test]
    fn remap_node_ids_leaves_other_doms_alone() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1]), (1, &[1])])]);

        hm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(5))]),
        );

        let frame = hm.get_current_mouse().expect("frame exists");
        assert_eq!(
            frame.hovered_nodes[&dom(0)]
                .regular_hit_test_nodes
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![NodeId::new(5)],
            "DOM 0 is remapped"
        );
        assert_eq!(
            frame.hovered_nodes[&dom(1)]
                .regular_hit_test_nodes
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![NodeId::new(1)],
            "DOM 1 must be untouched by DOM 0's reconciliation"
        );
    }

    #[test]
    fn remap_node_ids_keeps_foreign_dom_scrollbar_ids_stored_under_the_target_dom() {
        // A scrollbar hit recorded under DOM 0's HitTest but whose ScrollbarHitId
        // names DOM 1: remap_scrollbar_hit_id must pass it through, not drop it.
        let mut full = FullHitTest::empty(None);
        let mut ht = HitTest::empty();
        ht.scrollbar_hit_test_nodes.insert(
            ScrollbarHitId::VerticalTrack(dom(1), NodeId::new(1)),
            scrollbar_item(),
        );
        ht.scrollbar_hit_test_nodes.insert(
            ScrollbarHitId::VerticalTrack(dom(0), NodeId::new(1)),
            scrollbar_item(),
        );
        full.hovered_nodes.insert(dom(0), ht);
        let mut hm = mouse_history(vec![full]);

        hm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(8))]),
        );

        let keys: Vec<_> = hm.get_current_mouse().expect("frame exists").hovered_nodes[&dom(0)]
            .scrollbar_hit_test_nodes
            .keys()
            .copied()
            .collect();
        assert!(
            keys.contains(&ScrollbarHitId::VerticalTrack(dom(1), NodeId::new(1))),
            "foreign-DOM scrollbar id must survive unchanged, got {keys:?}"
        );
        assert!(
            keys.contains(&ScrollbarHitId::VerticalTrack(dom(0), NodeId::new(8))),
            "target-DOM scrollbar id must be rewritten, got {keys:?}"
        );
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn remap_node_ids_applies_to_every_frame_and_every_input_point() {
        let mut hm = HoverManager::new();
        for id in [InputPointId::Mouse, InputPointId::Touch(1)] {
            hm.push_hit_test(id, hits(&[(0, &[2])]));
            hm.push_hit_test(id, hits(&[(0, &[2])]));
        }

        hm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(2), NodeId::new(6))]),
        );

        for id in [InputPointId::Mouse, InputPointId::Touch(1)] {
            for frame in hm.get_history(&id).expect("history exists") {
                assert_eq!(
                    frame.hovered_nodes[&dom(0)]
                        .regular_hit_test_nodes
                        .keys()
                        .copied()
                        .collect::<Vec<_>>(),
                    vec![NodeId::new(6)]
                );
            }
        }
        assert_eq!(hm.previous_hover_node(), Some(NodeId::new(6)));
    }

    #[test]
    fn remap_node_ids_on_an_empty_manager_or_unknown_dom_does_not_panic() {
        let mut empty = HoverManager::new();
        empty.remap_node_ids(dom(usize::MAX), &NodeIdMap::default());
        assert_eq!(empty, HoverManager::new());

        let mut hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let before = hm.clone();
        // Reconciliation for a DOM that was never hit changes nothing.
        hm.remap_node_ids(
            dom(3),
            &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(2))]),
        );
        assert_eq!(hm, before);
    }

    #[test]
    fn remap_node_ids_identity_map_is_idempotent() {
        let mut hm = mouse_history(vec![all_maps_hit(0, 3)]);
        let before = hm.clone();
        let identity = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(3))]);

        hm.remap_node_ids(dom(0), &identity);
        assert_eq!(hm, before, "identity remap must not change anything");

        hm.remap_node_ids(dom(0), &identity);
        assert_eq!(hm, before, "and applying it twice must not either");
    }

    // ------------------------------------------------------------- misc invariants

    #[test]
    fn clone_is_equal_and_independent_of_the_original() {
        let mut hm = mouse_history(vec![hits(&[(0, &[1])])]);
        let snapshot = hm.clone();
        assert_eq!(hm, snapshot);

        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[2])]));

        assert_ne!(hm, snapshot, "the clone must not observe later pushes");
        assert_eq!(snapshot.frame_count(&InputPointId::Mouse), 1);
        assert_eq!(hm.frame_count(&InputPointId::Mouse), 2);
    }

    #[test]
    fn debug_counts_agrees_with_frame_count_and_active_points() {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, hits(&[(0, &[1])]));
        hm.push_hit_test(InputPointId::Touch(7), hits(&[(0, &[1])]));
        hm.push_hit_test(InputPointId::Touch(7), hits(&[(0, &[2])]));

        let (points, total) = hm.debug_counts();
        let active = hm.get_active_input_points();
        assert_eq!(points, active.len());
        assert_eq!(
            total,
            active.iter().map(|id| hm.frame_count(id)).sum::<usize>()
        );
        assert_eq!((points, total), (2, 3));
    }
}
