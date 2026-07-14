//! Unified text editing manager
//!
//! Single source of truth for all text editing state. `MultiCursorState` is
//! the primary cursor/selection system. `BlinkState` handles the caret blink
//! animation. (Non-editable drag-select is not yet wired — the former
//! `SelectionManager` scaffolding was dead and has been removed; a future
//! implementation should build on `MultiCursorState`.)
//!
//! Every mutation that affects visual output sets `display_list_dirty = true`,
//! ensuring the display list is always regenerated.

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    selection::{MultiCursorState, Selection, TextCursor},
    styled_dom::NodeHierarchyItemId,
    task::Instant,
};


/// Default cursor blink interval in milliseconds
pub const CURSOR_BLINK_INTERVAL_MS: u64 = 530;

/// Cursor blink animation state.
///
/// Extracted from the old `CursorManager` so it can live independently
/// on `TextEditManager` without coupling to cursor position.
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct BlinkState {
    /// Whether the cursor is currently visible (toggled by blink timer)
    pub is_visible: bool,
    /// Timestamp of the last user input event (keyboard, mouse click in text).
    /// Used to determine whether to blink or stay solid while typing.
    pub last_input_time: Option<Instant>,
    /// Whether the cursor blink timer is currently active
    pub blink_timer_active: bool,
}


impl BlinkState {
    #[must_use] pub fn new() -> Self { Self::default() }

    /// Reset blink on user input — cursor stays solid until blink interval elapses.
    pub fn reset_blink_on_input(&mut self, now: Instant) {
        self.is_visible = true;
        self.last_input_time = Some(now);
    }

    /// Toggle cursor visibility (called by blink timer callback).
    pub const fn toggle_visibility(&mut self) -> bool {
        self.is_visible = !self.is_visible;
        self.is_visible
    }

    pub const fn set_visibility(&mut self, visible: bool) {
        self.is_visible = visible;
    }

    pub const fn set_blink_timer_active(&mut self, active: bool) {
        self.blink_timer_active = active;
    }

    #[must_use] pub const fn is_blink_timer_active(&self) -> bool {
        self.blink_timer_active
    }

    /// Check if enough time has passed since last input to start blinking.
    #[must_use] pub fn should_blink(&self, now: &Instant) -> bool {
        use azul_core::task::{Duration, SystemTimeDiff};
        self.last_input_time.as_ref().is_none_or(|last_input| {
                let elapsed = now.duration_since(last_input);
                let blink_interval = Duration::System(SystemTimeDiff::from_millis(CURSOR_BLINK_INTERVAL_MS));
                elapsed.greater_than(&blink_interval)
            })
    }

    /// Clear all blink state (when editing ends).
    pub fn clear(&mut self) {
        self.is_visible = false;
        self.last_input_time = None;
        self.blink_timer_active = false;
    }
}

/// Unified text editing manager.
///
/// `multi_cursor` is the single source of truth for cursor/selection positions.
/// `blink` manages the caret blink animation.
/// `SelectionManager` (sibling module) handles non-editable text drag-select.
#[derive(Debug, Clone)]
pub struct TextEditManager {
    /// Multi-cursor state for contenteditable elements (Sublime Text style).
    /// `Some` whenever a contenteditable element has focus.
    /// Source of truth for `edit_text()` and display list painting.
    pub multi_cursor: Option<MultiCursorState>,
    /// Cursor blink animation state.
    pub blink: BlinkState,
    /// IME preedit (composition) text currently being composed.
    /// Applies to the primary cursor only.
    pub preedit_text: Option<String>,
    /// Byte offset of cursor within preedit text (from IME), or -1 if unset.
    /// Uses -1 sentinel (rather than `Option`) to match platform IME C API conventions.
    pub preedit_cursor_begin: i32,
    /// Byte offset of cursor end within preedit text (from IME), or -1 if unset.
    /// Uses -1 sentinel (rather than `Option`) to match platform IME C API conventions.
    pub preedit_cursor_end: i32,
    /// Set to true by any mutation that changes visual output.
    pub display_list_dirty: bool,
}

impl Default for TextEditManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Only compares `multi_cursor` — blink state, preedit, and dirty flag are
/// transient visual state that should not affect logical equality of the
/// editing session.
impl PartialEq for TextEditManager {
    fn eq(&self, other: &Self) -> bool {
        self.multi_cursor == other.multi_cursor
    }
}

impl TextEditManager {
    /// Create a new text edit manager with no active editing state
    #[must_use] pub fn new() -> Self {
        Self {
            multi_cursor: None,
            blink: BlinkState::new(),
            preedit_text: None,
            preedit_cursor_begin: -1,
            preedit_cursor_end: -1,
            display_list_dirty: false,
        }
    }

    // === Dirty flag ===

    /// Mark that the display list needs regeneration.
    pub const fn mark_dirty(&mut self) {
        self.display_list_dirty = true;
    }

    // === Editing lifecycle ===

    /// Whether a contenteditable element is currently being edited.
    #[must_use] pub const fn has_active_editing(&self) -> bool {
        self.multi_cursor.is_some()
    }

    /// Get the `DomId` of the node being edited.
    #[must_use] pub fn get_editing_dom_id(&self) -> Option<DomId> {
        self.multi_cursor.as_ref().map(|mc| mc.node_id.dom)
    }

    /// Get the `NodeId` of the node being edited.
    #[must_use] pub fn get_editing_node_id(&self) -> Option<NodeId> {
        self.multi_cursor.as_ref()
            .and_then(|mc| mc.node_id.node.into_crate_internal())
    }

    /// Get the primary cursor position (last-added cursor).
    #[must_use] pub fn get_primary_cursor(&self) -> Option<TextCursor> {
        self.multi_cursor.as_ref().and_then(MultiCursorState::get_primary_cursor)
    }

    /// Whether the cursor should be drawn (editing active AND blink visible).
    #[must_use] pub const fn should_draw_cursor(&self) -> bool {
        self.has_active_editing() && self.blink.is_visible
    }

    /// Initialize editing for a newly focused contenteditable element.
    ///
    /// Creates a `MultiCursorState` with a single cursor, starts the blink,
    /// and sets preedit to None.
    pub fn initialize_editing(
        &mut self,
        cursor: TextCursor,
        dom_id: DomId,
        node_id: NodeId,
        contenteditable_key: u64,
    ) {
        let dom_node_id = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        };
        self.multi_cursor = Some(MultiCursorState::new_with_cursor(
            cursor,
            dom_node_id,
            contenteditable_key,
        ));
        self.blink.is_visible = true;
        self.blink.last_input_time = None;
        self.clear_preedit();
        self.mark_dirty();
    }

    /// End editing (focus left the contenteditable element).
    pub fn clear_editing(&mut self) {
        self.multi_cursor = None;
        self.blink.clear();
        self.clear_preedit();
        self.mark_dirty();
    }

    // === IME preedit ===

    /// Set the IME preedit (composition) text.
    pub fn set_preedit(&mut self, text: String, cursor_begin: i32, cursor_end: i32) {
        self.preedit_text = if text.is_empty() { None } else { Some(text) };
        self.preedit_cursor_begin = cursor_begin;
        self.preedit_cursor_end = cursor_end;
        self.mark_dirty();
    }

    /// Clear the IME preedit text (composition ended or cancelled).
    pub fn clear_preedit(&mut self) {
        self.preedit_text = None;
        self.preedit_cursor_begin = -1;
        self.preedit_cursor_end = -1;
        self.mark_dirty();
    }

    // === Convenience for building cursor_locations ===

    /// Build the Vec of cursor locations for `LayoutContext`.
    ///
    /// Returns all cursor positions from `MultiCursorState`, or empty if not editing.
    #[must_use] pub fn build_cursor_locations(&self) -> Vec<(DomId, NodeId, TextCursor)> {
        let Some(ref mc) = self.multi_cursor else {
            return Vec::new();
        };
        let Some(node_id) = mc.node_id.node.into_crate_internal() else {
            return Vec::new();
        };
        mc.selections.iter().map(|s| {
            let cursor = match &s.selection {
                Selection::Cursor(c) => *c,
                Selection::Range(r) => r.end,
            };
            (mc.node_id.dom, node_id, cursor)
        }).collect()
    }

    /// Build a `TextSelection` map for the display list's `paint_selections`.
    ///
    /// Extracts Range selections from `MultiCursorState` into the format that
    /// `LayoutContext.text_selections` expects: `BTreeMap<DomId, TextSelection>`.
    /// The `affected_nodes` map uses the editing node's `NodeId` as key.
    /// NOTE: only one range per node is supported — if multiple cursors have
    /// range selections on the same node, later ranges overwrite earlier ones.
    #[must_use] pub fn build_text_selections_map(&self) -> std::collections::BTreeMap<DomId, azul_core::selection::TextSelection> {
        use azul_core::selection::{TextSelection, SelectionAnchor, SelectionFocus};
        use azul_core::geom::LogicalRect;

        let mut map = std::collections::BTreeMap::new();
        let Some(ref mc) = self.multi_cursor else {
            return map;
        };
        let Some(node_id) = mc.node_id.node.into_crate_internal() else {
            return map;
        };

        let mut affected_nodes = std::collections::BTreeMap::new();
        let mut first_range: Option<azul_core::selection::SelectionRange> = None;
        for sel in &mc.selections {
            if let Selection::Range(range) = &sel.selection {
                affected_nodes.insert(node_id, *range);
                if first_range.is_none() {
                    first_range = Some(*range);
                }
            }
        }

        if let Some(range) = first_range {
            map.insert(mc.node_id.dom, TextSelection {
                dom_id: mc.node_id.dom,
                anchor: SelectionAnchor {
                    ifc_root_node_id: node_id,
                    cursor: range.start,
                    char_bounds: LogicalRect::zero(),
                    mouse_position: azul_core::geom::LogicalPosition::zero(),
                },
                focus: SelectionFocus {
                    ifc_root_node_id: node_id,
                    cursor: range.end,
                    mouse_position: azul_core::geom::LogicalPosition::zero(),
                },
                affected_nodes,
                is_forward: true,
            });
        }

        map
    }
}

impl crate::managers::NodeIdRemap for TextEditManager {
    /// Remap the multi-cursor / selection state onto the rebuilt DOM.
    ///
    /// `MultiCursorState::remap_node_ids` clears the selections when the edited
    /// node is gone; here we additionally drop the whole editing session, since a
    /// cursor whose IFC root no longer exists is not an editing session.
    fn remap_node_ids(&mut self, dom: DomId, map: &crate::managers::NodeIdMap) {
        let Some(ref mut mc) = self.multi_cursor else {
            return;
        };
        if mc.node_id.dom != dom {
            return;
        }
        let unmounted = mc
            .node_id
            .node
            .into_crate_internal()
            .is_none_or(|old| map.resolve(old).is_none());
        if unmounted {
            self.multi_cursor = None;
            self.preedit_text = None;
            self.preedit_cursor_begin = -1;
            self.preedit_cursor_end = -1;
            self.display_list_dirty = true;
            return;
        }
        mc.remap_node_ids(dom, map.as_btree_map());
    }
}

// ============================================================================
// AUTOTEST: adversarial tests for `BlinkState` + `TextEditManager`
// ============================================================================
#[cfg(test)]
mod autotest_generated {
    use azul_core::{
        selection::{
            CursorAffinity, GraphemeClusterId, IdentifiedSelection, SelectionId, SelectionRange,
        },
        task::{Duration, SystemTick, SystemTimeDiff},
    };

    use super::*;
    use crate::managers::{NodeIdMap, NodeIdRemap};

    const DOM0: DomId = DomId { inner: 0 };
    const DOM1: DomId = DomId { inner: 1 };
    /// A `DomId` at the very top of the `usize` range — nothing indexes with it,
    /// so it must be carried through unchanged like any other id.
    const DOM_MAX: DomId = DomId { inner: usize::MAX };

    /// `NodeHierarchyItemId` stores nodes 1-based (`from_crate_internal` computes
    /// `index + 1`), so the largest node index that can survive a round-trip
    /// through a `DomNodeId` is `usize::MAX - 1`. `NodeId::new(usize::MAX)` is not
    /// representable and is deliberately never fed to `initialize_editing`.
    const MAX_ENCODABLE_NODE: usize = usize::MAX - 1;

    fn cursor(run: u32, byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    fn range(from: TextCursor, to: TextCursor) -> SelectionRange {
        SelectionRange {
            start: from,
            end: to,
        }
    }

    fn dom_node(dom: DomId, node: Option<NodeId>) -> DomNodeId {
        DomNodeId {
            dom,
            node: NodeHierarchyItemId::from_crate_internal(node),
        }
    }

    /// Build a `MultiCursorState` with an arbitrary selection list, bypassing
    /// `add_cursor`/`add_selection` (which sort + merge) so the exact ordering
    /// under test is preserved.
    fn multi_cursor_with(
        node_id: DomNodeId,
        selections: Vec<Selection>,
        key: u64,
    ) -> MultiCursorState {
        let identified: Vec<IdentifiedSelection> = selections
            .into_iter()
            .map(|selection| IdentifiedSelection {
                id: SelectionId::new(),
                selection,
            })
            .collect();
        let primary_id = identified
            .last()
            .map_or_else(SelectionId::new, |s| s.id);
        MultiCursorState {
            selections: identified,
            primary_id,
            node_id,
            contenteditable_key: key,
        }
    }

    /// `base + ms`, using the engine's own saturating instant arithmetic.
    fn plus_ms(base: &Instant, ms: u64) -> Instant {
        base.add_optional_duration(Some(&Duration::System(SystemTimeDiff::from_millis(ms))))
    }

    // ------------------------------------------------------------------
    // BlinkState
    // ------------------------------------------------------------------

    #[test]
    fn autotest_blink_new_invariants() {
        let b = BlinkState::new();
        assert!(!b.is_visible, "a fresh BlinkState starts hidden");
        assert!(b.last_input_time.is_none());
        assert!(!b.is_blink_timer_active());
        assert!(!b.blink_timer_active);
        // No input has ever been recorded, so blinking is allowed immediately.
        assert!(b.should_blink(&Instant::now()));
    }

    #[test]
    fn autotest_blink_toggle_visibility_alternates_and_returns_new_state() {
        let mut b = BlinkState::new();
        assert!(b.toggle_visibility(), "first toggle turns the caret on");
        assert!(b.is_visible);
        assert!(!b.toggle_visibility(), "second toggle turns it back off");
        assert!(!b.is_visible);

        // 1000 toggles: the return value must always equal the new field value,
        // and parity must be exactly preserved (no drift, no panic).
        let mut expected = false;
        for _ in 0..1000 {
            expected = !expected;
            let returned = b.toggle_visibility();
            assert_eq!(returned, expected);
            assert_eq!(b.is_visible, expected);
        }
        assert!(!b.is_visible, "an even number of toggles restores the state");
    }

    #[test]
    fn autotest_blink_set_visibility_is_idempotent_and_orthogonal() {
        let mut b = BlinkState::new();
        b.set_blink_timer_active(true);

        b.set_visibility(true);
        b.set_visibility(true);
        assert!(b.is_visible);
        assert!(
            b.is_blink_timer_active(),
            "visibility must not disturb the timer flag"
        );

        b.set_visibility(false);
        b.set_visibility(false);
        assert!(!b.is_visible);
        assert!(b.is_blink_timer_active());
    }

    #[test]
    fn autotest_blink_timer_active_true_false_and_idempotent() {
        let mut b = BlinkState::new();
        assert!(!b.is_blink_timer_active(), "known-false: default state");

        b.set_blink_timer_active(true);
        assert!(b.is_blink_timer_active(), "known-true: after activation");
        b.set_blink_timer_active(true);
        assert!(b.is_blink_timer_active(), "re-activation is idempotent");

        b.set_blink_timer_active(false);
        assert!(!b.is_blink_timer_active());
        b.set_blink_timer_active(false);
        assert!(!b.is_blink_timer_active(), "re-deactivation is idempotent");

        // The timer flag never leaks into visibility.
        assert!(!b.is_visible);
    }

    #[test]
    fn autotest_blink_reset_on_input_forces_solid_caret() {
        let mut b = BlinkState::new();
        b.set_blink_timer_active(true);
        b.set_visibility(false);

        let now = Instant::now();
        b.reset_blink_on_input(now.clone());

        assert!(b.is_visible, "typing must show a solid caret");
        assert_eq!(b.last_input_time.as_ref(), Some(&now));
        assert!(
            b.is_blink_timer_active(),
            "reset_blink_on_input must not stop the timer"
        );
        // Immediately after input, the blink interval has not elapsed.
        assert!(!b.should_blink(&now));
    }

    #[test]
    fn autotest_blink_reset_on_input_repeated_keeps_latest_timestamp() {
        let mut b = BlinkState::new();
        let base = Instant::now();

        // Simulate a fast typist: 500 keystrokes, 1ms apart.
        for i in 0..500u64 {
            b.reset_blink_on_input(plus_ms(&base, i));
            assert!(b.is_visible, "the caret stays solid throughout typing");
        }

        let last = plus_ms(&base, 499);
        assert_eq!(b.last_input_time.as_ref(), Some(&last));
        // The whole burst spans 499ms < 530ms, so blinking has still not resumed.
        assert!(!b.should_blink(&last));
    }

    #[test]
    fn autotest_blink_should_blink_without_input_is_true() {
        let b = BlinkState::new();
        let now = Instant::now();
        assert!(b.should_blink(&now));
        // Also true for an instant far in the past — no input means no gate at all.
        assert!(b.should_blink(&Instant::Tick(SystemTick::new(0))));
    }

    #[test]
    fn autotest_blink_should_blink_interval_boundary_is_strict() {
        let base = Instant::now();
        let mut b = BlinkState::new();
        b.reset_blink_on_input(base.clone());

        assert!(
            !b.should_blink(&base),
            "zero elapsed time must not restart the blink"
        );
        assert!(
            !b.should_blink(&plus_ms(&base, CURSOR_BLINK_INTERVAL_MS - 1)),
            "one millisecond before the interval: still solid"
        );
        assert!(
            !b.should_blink(&plus_ms(&base, CURSOR_BLINK_INTERVAL_MS)),
            "exactly at the interval: the comparison is strictly greater-than"
        );
        assert!(
            b.should_blink(&plus_ms(&base, CURSOR_BLINK_INTERVAL_MS + 1)),
            "one millisecond past the interval: blinking resumes"
        );
        // Far past the interval (one day) — no overflow, still blinking.
        assert!(b.should_blink(&plus_ms(&base, 86_400_000)));
    }

    #[test]
    fn autotest_blink_should_blink_reversed_clock_saturates_to_false() {
        // `now` is *earlier* than the recorded input (clock skew / reordered
        // events). `Instant::duration_since` saturates to zero rather than
        // panicking, so the caret stays solid instead of the call blowing up.
        let base = Instant::now();
        let mut b = BlinkState::new();
        b.reset_blink_on_input(plus_ms(&base, 10_000));

        assert!(!b.should_blink(&base));
        assert!(b.is_visible);
    }

    #[test]
    fn autotest_blink_should_blink_mismatched_clock_kinds_are_deterministic() {
        // A Tick instant compared against a System instant has no meaningful span:
        // `duration_since` saturates to `Duration::Tick(0)` and `greater_than`
        // returns false for mismatched kinds. Assert it is deterministic and
        // panic-free rather than assuming a particular blink outcome.
        let mut b = BlinkState::new();
        b.reset_blink_on_input(Instant::now());
        let tick_now = Instant::Tick(SystemTick::new(u64::MAX));
        assert_eq!(b.should_blink(&tick_now), b.should_blink(&tick_now));
        assert!(!b.should_blink(&tick_now));

        // Both endpoints Tick: the elapsed span is a Tick duration, which is never
        // "greater than" the System-typed blink interval, so a tick-only clock
        // (no_std) never resumes blinking. Deterministic, but worth knowing.
        let mut t = BlinkState::new();
        t.reset_blink_on_input(Instant::Tick(SystemTick::new(0)));
        assert!(!t.should_blink(&Instant::Tick(SystemTick::new(u64::MAX))));
    }

    #[test]
    fn autotest_blink_clear_resets_every_field_and_is_idempotent() {
        let mut b = BlinkState::new();
        b.reset_blink_on_input(Instant::now());
        b.set_blink_timer_active(true);

        b.clear();
        assert!(!b.is_visible);
        assert!(b.last_input_time.is_none());
        assert!(!b.is_blink_timer_active());

        // Clearing an already-cleared state must not panic or resurrect anything.
        b.clear();
        assert!(!b.is_visible);
        assert!(b.last_input_time.is_none());
        assert!(!b.is_blink_timer_active());
        // With no last input, blinking is unblocked again.
        assert!(b.should_blink(&Instant::now()));
    }

    // ------------------------------------------------------------------
    // TextEditManager — construction / predicates / getters
    // ------------------------------------------------------------------

    #[test]
    fn autotest_manager_new_invariants() {
        let m = TextEditManager::new();
        assert!(m.multi_cursor.is_none());
        assert!(!m.has_active_editing());
        assert!(m.get_editing_dom_id().is_none());
        assert!(m.get_editing_node_id().is_none());
        assert!(m.get_primary_cursor().is_none());
        assert!(!m.should_draw_cursor());
        assert!(m.preedit_text.is_none());
        assert_eq!(m.preedit_cursor_begin, -1, "-1 is the 'unset' IME sentinel");
        assert_eq!(m.preedit_cursor_end, -1);
        assert!(!m.display_list_dirty, "a fresh manager owes no repaint");
        assert!(m.build_cursor_locations().is_empty());
        assert!(m.build_text_selections_map().is_empty());
        assert_eq!(m, TextEditManager::default());
    }

    #[test]
    fn autotest_manager_mark_dirty_is_sticky() {
        let mut m = TextEditManager::new();
        m.mark_dirty();
        assert!(m.display_list_dirty);
        m.mark_dirty();
        assert!(m.display_list_dirty, "marking twice must not toggle it off");
    }

    #[test]
    fn autotest_manager_partial_eq_ignores_transient_state() {
        // Documented contract: only `multi_cursor` participates in equality.
        let mut a = TextEditManager::new();
        let mut b = TextEditManager::new();
        assert_eq!(a, b);

        a.set_preedit("か".to_string(), 0, 3);
        a.blink.set_visibility(true);
        a.mark_dirty();
        assert_eq!(a, b, "preedit / blink / dirty are transient visual state");

        b.initialize_editing(cursor(0, 0), DOM0, NodeId::ZERO, 1);
        assert_ne!(a, b, "a live editing session is not equal to no session");
    }

    // ------------------------------------------------------------------
    // TextEditManager — initialize_editing (numeric edges)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_initialize_editing_at_zero() {
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(0, 0), DOM0, NodeId::ZERO, 0);

        assert!(m.has_active_editing());
        assert_eq!(m.get_editing_dom_id(), Some(DOM0));
        assert_eq!(
            m.get_editing_node_id(),
            Some(NodeId::ZERO),
            "node index 0 must not be confused with the 'no node' encoding"
        );
        assert_eq!(m.get_primary_cursor(), Some(cursor(0, 0)));
        assert_eq!(
            m.multi_cursor.as_ref().map(|mc| mc.contenteditable_key),
            Some(0)
        );
        assert!(m.blink.is_visible);
        assert!(m.blink.last_input_time.is_none());
        assert!(m.should_draw_cursor());
        assert!(m.display_list_dirty);
        assert_eq!(
            m.build_cursor_locations(),
            vec![(DOM0, NodeId::ZERO, cursor(0, 0))]
        );
    }

    #[test]
    fn autotest_initialize_editing_at_integer_extremes() {
        // Max representable node index, max DomId, max contenteditable key, and a
        // cursor at the top of the u32 grapheme-coordinate space.
        let node = NodeId::new(MAX_ENCODABLE_NODE);
        let extreme_cursor = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: u32::MAX,
                start_byte_in_run: u32::MAX,
            },
            affinity: CursorAffinity::Trailing,
        };

        let mut m = TextEditManager::new();
        m.initialize_editing(extreme_cursor, DOM_MAX, node, u64::MAX);

        assert_eq!(m.get_editing_dom_id(), Some(DOM_MAX));
        assert_eq!(
            m.get_editing_node_id(),
            Some(node),
            "usize::MAX - 1 is the largest 1-based-encodable node index"
        );
        assert_eq!(m.get_primary_cursor(), Some(extreme_cursor));
        assert_eq!(
            m.multi_cursor.as_ref().map(|mc| mc.contenteditable_key),
            Some(u64::MAX),
            "the contenteditable key is opaque — u64::MAX must survive verbatim"
        );
        assert_eq!(
            m.build_cursor_locations(),
            vec![(DOM_MAX, node, extreme_cursor)]
        );
    }

    #[test]
    fn autotest_initialize_editing_overwrites_previous_session() {
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(1, 1), DOM0, NodeId::new(7), 111);
        m.initialize_editing(cursor(2, 2), DOM1, NodeId::new(9), 222);

        assert_eq!(m.get_editing_dom_id(), Some(DOM1));
        assert_eq!(m.get_editing_node_id(), Some(NodeId::new(9)));
        assert_eq!(m.get_primary_cursor(), Some(cursor(2, 2)));
        assert_eq!(
            m.build_cursor_locations().len(),
            1,
            "re-initializing replaces the cursor set, it does not accumulate"
        );
        assert_eq!(
            m.multi_cursor.as_ref().map(|mc| mc.contenteditable_key),
            Some(222)
        );
    }

    #[test]
    fn autotest_initialize_editing_clears_stale_preedit() {
        let mut m = TextEditManager::new();
        m.set_preedit("漢字".to_string(), 3, 6);
        m.initialize_editing(cursor(0, 0), DOM0, NodeId::new(4), 42);

        assert!(
            m.preedit_text.is_none(),
            "focusing a new element must drop the old composition"
        );
        assert_eq!(m.preedit_cursor_begin, -1);
        assert_eq!(m.preedit_cursor_end, -1);
    }

    // ------------------------------------------------------------------
    // TextEditManager — clear_editing
    // ------------------------------------------------------------------

    #[test]
    fn autotest_clear_editing_on_fresh_manager_is_safe() {
        let mut m = TextEditManager::new();
        m.clear_editing();
        m.clear_editing();

        assert!(!m.has_active_editing());
        assert!(!m.should_draw_cursor());
        assert!(m.build_cursor_locations().is_empty());
        assert!(
            m.display_list_dirty,
            "clear_editing marks dirty unconditionally"
        );
    }

    #[test]
    fn autotest_clear_editing_tears_down_everything() {
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(0, 5), DOM0, NodeId::new(3), 77);
        m.set_preedit("ab".to_string(), 0, 2);
        m.blink.set_blink_timer_active(true);
        m.blink.reset_blink_on_input(Instant::now());

        m.clear_editing();

        assert!(m.multi_cursor.is_none());
        assert!(!m.has_active_editing());
        assert!(m.get_editing_dom_id().is_none());
        assert!(m.get_editing_node_id().is_none());
        assert!(m.get_primary_cursor().is_none());
        assert!(!m.should_draw_cursor());
        assert!(!m.blink.is_visible);
        assert!(!m.blink.is_blink_timer_active());
        assert!(m.blink.last_input_time.is_none());
        assert!(m.preedit_text.is_none());
        assert_eq!(m.preedit_cursor_begin, -1);
        assert_eq!(m.preedit_cursor_end, -1);
        assert!(m.display_list_dirty);
        assert!(m.build_cursor_locations().is_empty());
        assert!(m.build_text_selections_map().is_empty());
    }

    // ------------------------------------------------------------------
    // TextEditManager — IME preedit (numeric edges + unicode)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_set_preedit_zero_offsets_are_not_the_unset_sentinel() {
        let mut m = TextEditManager::new();
        m.set_preedit("a".to_string(), 0, 0);

        assert_eq!(m.preedit_text.as_deref(), Some("a"));
        assert_eq!(
            m.preedit_cursor_begin, 0,
            "0 is a valid offset and must not be coerced to the -1 sentinel"
        );
        assert_eq!(m.preedit_cursor_end, 0);
        assert!(m.display_list_dirty);
    }

    #[test]
    fn autotest_set_preedit_stores_i32_extremes_verbatim() {
        let mut m = TextEditManager::new();

        m.set_preedit("x".to_string(), i32::MIN, i32::MAX);
        assert_eq!(m.preedit_cursor_begin, i32::MIN);
        assert_eq!(m.preedit_cursor_end, i32::MAX);

        // Negative (non-sentinel) values and an inverted begin > end range are
        // stored as-is: the manager performs no arithmetic on them, so there is
        // nothing to overflow. Consumers must clamp.
        m.set_preedit("x".to_string(), -42, -7);
        assert_eq!(m.preedit_cursor_begin, -42);
        assert_eq!(m.preedit_cursor_end, -7);

        m.set_preedit("x".to_string(), 10, 2);
        assert_eq!(m.preedit_cursor_begin, 10);
        assert_eq!(m.preedit_cursor_end, 2);
    }

    #[test]
    fn autotest_set_preedit_offsets_beyond_text_length_are_not_validated() {
        // A hostile / buggy IME can report offsets far outside the string. The
        // setter must not panic and must not silently rewrite them — it stores
        // them verbatim, which is the contract callers have to defend against.
        let mut m = TextEditManager::new();
        m.set_preedit("ab".to_string(), i32::MAX, i32::MAX);

        assert_eq!(m.preedit_text.as_deref(), Some("ab"));
        assert_eq!(m.preedit_cursor_begin, i32::MAX);
        assert_eq!(m.preedit_cursor_end, i32::MAX);
    }

    #[test]
    fn autotest_set_preedit_empty_text_becomes_none_but_keeps_offsets() {
        // Documented behaviour of `set_preedit`: an empty composition string maps
        // to `None`, yet the offsets are still overwritten with whatever the IME
        // passed. The result is a `None` text with non-sentinel offsets — callers
        // must key off `preedit_text`, not off the offsets.
        let mut m = TextEditManager::new();
        m.set_preedit(String::new(), 5, 9);

        assert!(m.preedit_text.is_none());
        assert_eq!(m.preedit_cursor_begin, 5);
        assert_eq!(m.preedit_cursor_end, 9);
        assert!(m.display_list_dirty);
    }

    #[test]
    fn autotest_set_preedit_preserves_unicode_verbatim() {
        let mut m = TextEditManager::new();

        for text in [
            "こんにちは",                 // CJK — the common IME case
            "👨‍👩‍👧‍👦",                        // ZWJ emoji family (one grapheme, many bytes)
            "e\u{0301}\u{0327}",          // combining acute + cedilla
            "مرحبا",                      // RTL
            "a\u{0000}b",                 // interior NUL
            "\u{FEFF}bom",                // byte-order mark
            "🇩🇪🇯🇵",                        // regional-indicator flags
        ] {
            m.set_preedit(text.to_string(), 0, 1);
            assert_eq!(
                m.preedit_text.as_deref(),
                Some(text),
                "preedit text must round-trip byte-for-byte"
            );
        }
    }

    #[test]
    fn autotest_set_preedit_huge_text_does_not_panic() {
        let huge = "あ".repeat(100_000); // 300_000 bytes
        let mut m = TextEditManager::new();
        m.set_preedit(huge.clone(), 0, 299_999);

        assert_eq!(m.preedit_text.as_deref(), Some(huge.as_str()));
        assert_eq!(m.preedit_text.as_ref().map(String::len), Some(300_000));
    }

    #[test]
    fn autotest_clear_preedit_is_idempotent_and_marks_dirty() {
        let mut m = TextEditManager::new();
        m.set_preedit("ば".to_string(), 0, 3);

        m.clear_preedit();
        assert!(m.preedit_text.is_none());
        assert_eq!(m.preedit_cursor_begin, -1);
        assert_eq!(m.preedit_cursor_end, -1);

        m.display_list_dirty = false;
        m.clear_preedit();
        assert!(m.preedit_text.is_none());
        assert_eq!(m.preedit_cursor_begin, -1);
        assert_eq!(m.preedit_cursor_end, -1);
        assert!(
            m.display_list_dirty,
            "clear_preedit marks dirty even when nothing changed"
        );
    }

    #[test]
    fn autotest_preedit_does_not_create_an_editing_session() {
        let mut m = TextEditManager::new();
        m.set_preedit("compose".to_string(), 0, 7);

        assert!(
            !m.has_active_editing(),
            "IME text alone must not fake an editing session"
        );
        assert!(!m.should_draw_cursor());
        assert!(m.get_primary_cursor().is_none());
    }

    // ------------------------------------------------------------------
    // TextEditManager — build_cursor_locations
    // ------------------------------------------------------------------

    #[test]
    fn autotest_build_cursor_locations_empty_without_session() {
        assert!(TextEditManager::new().build_cursor_locations().is_empty());
    }

    #[test]
    fn autotest_build_cursor_locations_uses_range_end_and_keeps_order() {
        let node = NodeId::new(12);
        let a = cursor(0, 0);
        let b = cursor(0, 4);
        let c = cursor(1, 8);

        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM1, Some(node)),
            vec![
                Selection::Cursor(a),
                Selection::Range(range(b, c)),
                Selection::Cursor(c),
            ],
            5,
        ));

        assert_eq!(
            m.build_cursor_locations(),
            vec![(DOM1, node, a), (DOM1, node, c), (DOM1, node, c)],
            "a Range contributes its `end` as the caret position"
        );
    }

    #[test]
    fn autotest_build_cursor_locations_with_detached_node_is_empty() {
        // A `MultiCursorState` whose node encodes "no node" must yield nothing
        // rather than panicking or fabricating NodeId(0).
        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM0, None),
            vec![Selection::Cursor(cursor(0, 0))],
            1,
        ));

        assert!(m.has_active_editing());
        assert!(m.get_editing_node_id().is_none());
        assert!(m.build_cursor_locations().is_empty());
        assert!(m.build_text_selections_map().is_empty());
    }

    #[test]
    fn autotest_build_cursor_locations_with_no_selections_is_empty() {
        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM0, Some(NodeId::ZERO)),
            Vec::new(),
            0,
        ));

        assert!(m.build_cursor_locations().is_empty());
        assert!(m.get_primary_cursor().is_none());
        assert!(m.build_text_selections_map().is_empty());
    }

    #[test]
    fn autotest_build_cursor_locations_scales_to_many_cursors() {
        let node = NodeId::new(2);
        let selections: Vec<Selection> = (0..1000u32)
            .map(|i| Selection::Cursor(cursor(0, i)))
            .collect();

        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM0, Some(node)),
            selections,
            9,
        ));

        let locations = m.build_cursor_locations();
        assert_eq!(locations.len(), 1000);
        assert_eq!(locations[0], (DOM0, node, cursor(0, 0)));
        assert_eq!(locations[999], (DOM0, node, cursor(0, 999)));
    }

    // ------------------------------------------------------------------
    // TextEditManager — build_text_selections_map
    // ------------------------------------------------------------------

    #[test]
    fn autotest_build_text_selections_map_empty_without_session() {
        assert!(TextEditManager::new().build_text_selections_map().is_empty());
    }

    #[test]
    fn autotest_build_text_selections_map_ignores_pure_cursors() {
        // Collapsed carets are not selections — nothing to paint.
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(0, 3), DOM0, NodeId::new(1), 8);
        assert!(m.build_text_selections_map().is_empty());
    }

    #[test]
    fn autotest_build_text_selections_map_single_range() {
        let node = NodeId::new(6);
        let start = cursor(0, 2);
        let end = cursor(0, 9);

        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM1, Some(node)),
            vec![Selection::Range(range(start, end))],
            3,
        ));

        let map = m.build_text_selections_map();
        assert_eq!(map.len(), 1);
        let sel = map.get(&DOM1).expect("keyed by the editing DomId");
        assert_eq!(sel.dom_id, DOM1);
        assert_eq!(sel.anchor.ifc_root_node_id, node);
        assert_eq!(sel.anchor.cursor, start);
        assert_eq!(sel.focus.ifc_root_node_id, node);
        assert_eq!(sel.focus.cursor, end);
        assert!(sel.is_forward);
        assert_eq!(sel.affected_nodes.len(), 1);
        assert_eq!(sel.affected_nodes.get(&node), Some(&range(start, end)));
    }

    #[test]
    fn autotest_build_text_selections_map_backward_range_is_reported_forward() {
        // A backward drag (start after end) is still emitted with `is_forward:
        // true` — the flag is hard-coded. Pinning the current behaviour so a
        // future direction fix has to update this test deliberately.
        let node = NodeId::new(6);
        let start = cursor(0, 9);
        let end = cursor(0, 2);

        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM0, Some(node)),
            vec![Selection::Range(range(start, end))],
            3,
        ));

        let map = m.build_text_selections_map();
        let sel = map.get(&DOM0).expect("keyed by the editing DomId");
        assert_eq!(sel.anchor.cursor, start);
        assert_eq!(sel.focus.cursor, end);
        assert!(sel.is_forward);
    }

    #[test]
    fn autotest_build_text_selections_map_multi_range_first_wins_endpoints() {
        // Documented limitation: only one range per node survives. What is NOT
        // documented is that the two halves disagree — `affected_nodes` keeps the
        // LAST range (each insert overwrites the same NodeId key) while the
        // anchor/focus endpoints come from the FIRST. Pinned deliberately.
        let node = NodeId::new(4);
        let first = range(cursor(0, 0), cursor(0, 1));
        let last = range(cursor(0, 5), cursor(0, 8));

        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM0, Some(node)),
            vec![
                Selection::Range(first),
                Selection::Cursor(cursor(0, 3)),
                Selection::Range(last),
            ],
            2,
        ));

        let map = m.build_text_selections_map();
        assert_eq!(map.len(), 1, "one entry per DomId, not per range");
        let sel = map.get(&DOM0).expect("keyed by the editing DomId");
        assert_eq!(sel.anchor.cursor, first.start, "endpoints from the FIRST range");
        assert_eq!(sel.focus.cursor, first.end);
        assert_eq!(
            sel.affected_nodes.get(&node),
            Some(&last),
            "affected_nodes keeps the LAST range — it disagrees with anchor/focus"
        );
    }

    #[test]
    fn autotest_build_text_selections_map_degenerate_and_extreme_ranges() {
        let node = NodeId::new(MAX_ENCODABLE_NODE);
        // Zero-width range (start == end) at the top of the coordinate space.
        let point = cursor(u32::MAX, u32::MAX);

        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM_MAX, Some(node)),
            vec![Selection::Range(range(point, point))],
            u64::MAX,
        ));

        let map = m.build_text_selections_map();
        let sel = map.get(&DOM_MAX).expect("keyed by the editing DomId");
        assert_eq!(sel.anchor.cursor, point);
        assert_eq!(sel.focus.cursor, point);
        assert_eq!(sel.affected_nodes.get(&node), Some(&range(point, point)));
    }

    // ------------------------------------------------------------------
    // TextEditManager — NodeIdRemap (DOM rebuild)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_remap_without_session_is_a_noop() {
        let mut m = TextEditManager::new();
        m.remap_node_ids(DOM0, &NodeIdMap::from_pairs([(NodeId::ZERO, NodeId::new(1))]));

        assert!(!m.has_active_editing());
        assert!(
            !m.display_list_dirty,
            "nothing changed, so nothing to repaint"
        );
    }

    #[test]
    fn autotest_remap_rewrites_surviving_node() {
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(0, 2), DOM0, NodeId::new(3), 55);
        m.set_preedit("ok".to_string(), 0, 2);

        m.remap_node_ids(DOM0, &NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(8))]));

        assert!(m.has_active_editing());
        assert_eq!(m.get_editing_node_id(), Some(NodeId::new(8)));
        assert_eq!(m.get_editing_dom_id(), Some(DOM0));
        assert_eq!(m.get_primary_cursor(), Some(cursor(0, 2)));
        assert_eq!(
            m.preedit_text.as_deref(),
            Some("ok"),
            "a surviving node keeps its in-flight composition"
        );
        assert_eq!(
            m.multi_cursor.as_ref().map(|mc| mc.contenteditable_key),
            Some(55),
            "the stable key must survive the rebuild"
        );
    }

    #[test]
    fn autotest_remap_drops_session_when_node_unmounted() {
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(0, 1), DOM0, NodeId::new(3), 55);
        m.set_preedit("gone".to_string(), 1, 4);
        m.display_list_dirty = false;

        // The rebuilt DOM matched some *other* node — 3 is unmounted.
        m.remap_node_ids(DOM0, &NodeIdMap::from_pairs([(NodeId::new(4), NodeId::new(4))]));

        assert!(!m.has_active_editing());
        assert!(m.multi_cursor.is_none());
        assert!(m.preedit_text.is_none());
        assert_eq!(m.preedit_cursor_begin, -1);
        assert_eq!(m.preedit_cursor_end, -1);
        assert!(m.display_list_dirty);
        assert!(m.build_cursor_locations().is_empty());
    }

    #[test]
    fn autotest_remap_with_empty_map_drops_session() {
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(0, 0), DOM0, NodeId::ZERO, 1);
        m.remap_node_ids(DOM0, &NodeIdMap::default());

        assert!(
            !m.has_active_editing(),
            "an empty map means every node was unmounted"
        );
    }

    #[test]
    fn autotest_remap_leaves_other_doms_alone() {
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(0, 0), DOM1, NodeId::new(3), 1);

        // A reconciliation of DOM0 says nothing about a cursor living in DOM1.
        m.remap_node_ids(DOM0, &NodeIdMap::default());

        assert!(m.has_active_editing());
        assert_eq!(m.get_editing_dom_id(), Some(DOM1));
        assert_eq!(m.get_editing_node_id(), Some(NodeId::new(3)));
    }

    #[test]
    fn autotest_remap_of_detached_node_drops_session() {
        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM0, None),
            vec![Selection::Cursor(cursor(0, 0))],
            1,
        ));

        m.remap_node_ids(DOM0, &NodeIdMap::from_pairs([(NodeId::ZERO, NodeId::ZERO)]));

        assert!(
            !m.has_active_editing(),
            "a cursor with no IFC root is not an editing session"
        );
        assert!(m.display_list_dirty);
    }
}
