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

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    geom::LogicalRect,
    selection::{MultiCursorState, Selection, SelectionRange, TextCursor},
    styled_dom::NodeHierarchyItemId,
    task::{Duration, Instant},
};

/// Default cursor blink interval in milliseconds
pub const CURSOR_BLINK_INTERVAL_MS: u64 = 530;

/// Default cursor blink interval as a variant-agnostic [`Duration`].
///
/// The interval is a `Duration`, not a bare `u64` of milliseconds, so a
/// stylesheet can express it in the clockless `t` unit (`caret-animation-duration:
/// 5t`) and have it survive all the way to the comparison. `Duration`'s
/// comparisons are unit-aware, so a tick-unit interval and a wall-clock elapsed
/// value (or vice versa) still compare truthfully.
pub const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(CURSOR_BLINK_INTERVAL_MS);

/// Cursor blink animation state.
///
/// Extracted from the old `CursorManager` so it can live independently
/// on `TextEditManager` without coupling to cursor position.
#[derive(Debug, Clone)]
pub struct BlinkState {
    /// Whether the cursor is currently visible (toggled by blink timer)
    pub is_visible: bool,
    /// Timestamp of the last user input event (keyboard, mouse click in text).
    /// Used to determine whether to blink or stay solid while typing.
    pub last_input_time: Option<Instant>,
    /// Whether the cursor blink timer is currently active
    pub blink_timer_active: bool,
    /// How long the caret stays solid after input before blinking resumes, and
    /// the interval the blink timer is armed with.
    ///
    /// Defaults to [`CURSOR_BLINK_INTERVAL`]; `caret-animation-duration` on the
    /// focused node overrides it, in whichever unit the stylesheet used.
    pub blink_interval: Duration,
}

impl Default for BlinkState {
    fn default() -> Self {
        Self {
            is_visible: false,
            last_input_time: None,
            blink_timer_active: false,
            blink_interval: CURSOR_BLINK_INTERVAL,
        }
    }
}

impl BlinkState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the blink interval (from `caret-animation-duration`).
    ///
    /// Takes a [`Duration`] rather than milliseconds so a `t`-unit stylesheet
    /// value stays a frame count: a `5t` caret flips on the 5th frame exactly,
    /// on any machine, at any load.
    ///
    /// Prefer [`Self::adopt_blink_interval`] on a focus change: a RUNNING timer
    /// does not pick the new value up on its own.
    pub const fn set_blink_interval(&mut self, interval: Duration) {
        self.blink_interval = interval;
    }

    /// Adopt `interval`, reporting whether it actually CHANGED.
    ///
    /// The blink timer bakes the interval into the `Timer` once, at
    /// construction (`LayoutWindow::create_cursor_blink_timer`), so a timer
    /// that is already running keeps the PREVIOUS node's period no matter what
    /// this state says. Refocusing between two editables with different
    /// `caret-animation-duration` must therefore rebuild the timer — and only
    /// then, because rebuilding it on every focus change would restart the
    /// blink phase for nothing.
    ///
    /// This is the predicate that decides (see `HANDOFF-text-fix.md` for the
    /// `CursorBlinkTimerAction::Restart` half). A change of UNIT is a change:
    /// `5t` and `530ms` are different intervals, not one interval spelled two
    /// ways — the tick-unit caret must stay clockless.
    #[must_use]
    pub fn adopt_blink_interval(&mut self, interval: Duration) -> bool {
        let changed = self.blink_interval != interval;
        self.blink_interval = interval;
        changed
    }

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

    #[must_use]
    pub const fn is_blink_timer_active(&self) -> bool {
        self.blink_timer_active
    }

    /// Check if enough time has passed since last input to start blinking.
    ///
    /// The interval is [`Self::blink_interval`], compared unit-aware: a tick
    /// interval against wall-clock elapsed time (or the reverse) both answer
    /// truthfully. This used to build a `Duration::System` constant inline, which
    /// meant a tick-driven clock produced a `Duration::Tick` elapsed value that
    /// could never be "greater than" it — the caret stopped blinking, silently
    /// and permanently, on every clockless build.
    #[must_use]
    pub fn should_blink(&self, now: &Instant) -> bool {
        self.last_input_time.as_ref().is_none_or(|last_input| {
            now.duration_since(last_input)
                .greater_than(&self.blink_interval)
        })
    }

    /// Clear all blink state (when editing ends).
    ///
    /// The interval goes back to the default too: it was read off the node that
    /// just lost focus, and leaving it behind would apply that node's
    /// `caret-animation-duration` to the next element focused — including one
    /// that never set the property.
    pub fn clear(&mut self) {
        self.is_visible = false;
        self.last_input_time = None;
        self.blink_timer_active = false;
        self.blink_interval = CURSOR_BLINK_INTERVAL;
    }
}

/// One in-flight caret tween: the caret is gliding from `from` (what the
/// previous frame rendered) toward wherever the current layout puts it.
#[derive(Debug, Clone)]
pub struct CaretTweenTrack {
    /// Rect the tween starts from (the previously RENDERED rect, so a
    /// mid-flight retarget stays continuous).
    pub from: LogicalRect,
    /// Rect the tween is seeking. Retargeting happens ONLY when the layout's
    /// current rect stops matching this — comparing against the rendered
    /// rect would re-arm (and restart the clock) every animation tick.
    pub to: LogicalRect,
    /// When the tween (re)started.
    pub start: Instant,
}

/// One in-flight selection tween (same contract as [`CaretTweenTrack`],
/// for the whole selection band geometry).
#[derive(Debug, Clone)]
pub struct SelectionTweenTrack {
    pub from: Vec<LogicalRect>,
    /// Geometry the tween is seeking (same retarget contract as
    /// [`CaretTweenTrack::to`]).
    pub to: Vec<LogicalRect>,
    pub start: Instant,
}

/// Caret / selection tween bookkeeping, updated by the display-list
/// post-pass (`LayoutWindow::apply_text_tweens`). `tick_flag` is shared
/// with the tween timer's `RefAny` data so the timer can terminate itself
/// the tick after both tweens finish.
#[derive(Debug, Default)]
pub struct TextTweenState {
    /// DOM the tracked geometry belongs to. A caret/selection appearing on a
    /// DIFFERENT dom resets tracking (no cross-dom tween).
    pub dom_id: Option<DomId>,
    /// Node the tracked caret/selection geometry belongs to — the editing
    /// session's node, maintained by [`TextEditManager`].
    ///
    /// Without it the geometry is unattributable, and a DOM reconcile that
    /// moves or unmounts the edited node leaves `last_caret`/`last_selection`
    /// describing a rectangle that belongs to nothing: the next frame then
    /// glides the caret across the screen from a dead rect.
    pub node: Option<DomNodeId>,
    /// Focusable the tracked caret geometry currently sits inside, as
    /// [`super::super::window::LayoutWindow::find_focusable_ancestor`] reports
    /// it. `None` is a real value — text outside any focusable — and compares
    /// equal to itself.
    ///
    /// A text field is a focusable container around a contenteditable child, so
    /// the caret's own node cannot tell two fields apart from two paragraphs of
    /// one field. This can.
    pub focus_scope: Option<DomNodeId>,
    /// In-flight caret tween, if any.
    pub caret: Option<CaretTweenTrack>,
    /// Caret rect the last display-list pass RENDERED (tween target space).
    pub last_caret: Option<LogicalRect>,
    /// In-flight selection tween, if any.
    pub selection: Option<SelectionTweenTrack>,
    /// Selection rects the last display-list pass RENDERED.
    pub last_selection: Vec<LogicalRect>,
    /// In-flight focus-ring glide (ledger #29; opt-in via
    /// `SystemAnimations.focus_ring_duration_ms`).
    pub focus_ring: Option<CaretTweenTrack>,
    /// Focus-ring rect the last display-list pass RENDERED.
    pub last_focus_ring: Option<LogicalRect>,
    /// Shared "a tween is in flight" flag: written by the post-pass, read
    /// by `caret_tween_timer_callback` (via its `RefAny`) to self-terminate.
    pub tick_flag: Arc<AtomicBool>,
}

/// Cloning a manager must NOT share the original's tween-timer flag: the two
/// copies would steer one timer, and dropping either would tell that timer the
/// other's tweens had finished. So the flag is the ONE field that is not
/// shared — the clone gets its own `Arc` holding the same value. Everything
/// else is copied, because a `clone()` that quietly returned
/// `Self::default()` reported "no tween in flight, no rendered geometry" for a
/// manager that had both.
impl Clone for TextTweenState {
    fn clone(&self) -> Self {
        Self {
            dom_id: self.dom_id,
            node: self.node,
            focus_scope: self.focus_scope,
            caret: self.caret.clone(),
            last_caret: self.last_caret,
            selection: self.selection.clone(),
            last_selection: self.last_selection.clone(),
            focus_ring: self.focus_ring.clone(),
            last_focus_ring: self.last_focus_ring,
            tick_flag: Arc::new(AtomicBool::new(
                self.tick_flag.load(AtomicOrdering::Acquire),
            )),
        }
    }
}

impl TextTweenState {
    /// True while any tween is mid-flight (drives the 16ms tween timer and
    /// forces the caret solid — blinking is suppressed during animation).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.caret.is_some() || self.selection.is_some() || self.focus_ring.is_some()
    }

    /// Publish `is_active()` to the shared flag the tween timer polls.
    pub fn publish_active(&self) {
        self.tick_flag
            .store(self.is_active(), AtomicOrdering::Release);
    }

    /// Reset all tracking (focus lost / editing cleared / dom switched).
    pub fn reset(&mut self) {
        self.dom_id = None;
        self.node = None;
        self.focus_scope = None;
        self.caret = None;
        self.last_caret = None;
        self.selection = None;
        self.last_selection.clear();
        self.focus_ring = None;
        self.last_focus_ring = None;
        self.publish_active();
    }

    /// Reset only the TEXT tweens (caret + selection) — the focus ring has
    /// its own lifecycle (it runs without an editing session).
    ///
    /// `node` goes with them: it anchors the caret/selection geometry, not the
    /// ring.
    pub fn reset_text_tweens(&mut self) {
        self.node = None;
        self.focus_scope = None;
        self.caret = None;
        self.last_caret = None;
        self.selection = None;
        self.last_selection.clear();
        self.publish_active();
    }
}

/// The range selections of ONE editing session, as
/// [`TextEditManager::session_selection_ranges`] reports them.
///
/// All ranges of a session live on the same IFC root — `MultiCursorState` is
/// single-node by construction; a selection spanning several roots takes the
/// `cross_block` path instead.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSelectionRanges {
    /// DOM the session's node belongs to.
    pub dom_id: DomId,
    /// IFC root every range is expressed against.
    pub node_id: NodeId,
    /// Every range, in `MultiCursorState` order (position-sorted,
    /// non-overlapping). Never empty.
    pub ranges: Vec<SelectionRange>,
    /// The primary range: the most recently added one (Ctrl+D's newest
    /// occurrence), or the document-first range when the primary selection is
    /// a bare caret. Always an element of `ranges`.
    pub primary: SelectionRange,
}

/// Does `range` run FORWARD — anchor before focus in logical order?
///
/// `SelectionRange.start` is the ANCHOR and `.end` the FOCUS (the moving end:
/// `build_cursor_locations` reads the caret off `.end`), so a backward drag
/// arrives with `start > end`. This is the per-range form of the question
/// `LayoutWindow::set_cross_block_selection` answers for its node pair. A
/// degenerate range counts as forward, matching `TextSelection::new_collapsed`.
#[must_use]
pub fn range_is_forward(range: &SelectionRange) -> bool {
    range.start <= range.end
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
    /// Cross-block (multi-IFC-root) selection, render-ready. Precomputed by
    /// `LayoutWindow::set_cross_block_selection`; wins over `multi_cursor`
    /// in `build_text_selections_map` while set.
    pub cross_block: Option<azul_core::selection::TextSelection>,
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
    /// Caret / selection tween bookkeeping (see [`TextTweenState`]).
    pub tween: TextTweenState,
    /// Editing hosts whose text was mutated OUTSIDE the text-input record
    /// pipeline this pass (deletions, multi-cursor paste, the Enter line
    /// break). The host pass drains this and dispatches an `Input` event per
    /// host, so widget mirrors observe every committed edit, not only
    /// insertions. Filled by `LayoutWindow::record_text_edit_undo`.
    pub pending_edit_notifications: Vec<DomNodeId>,
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            multi_cursor: None,
            cross_block: None,
            blink: BlinkState::new(),
            preedit_text: None,
            preedit_cursor_begin: -1,
            preedit_cursor_end: -1,
            display_list_dirty: false,
            tween: TextTweenState::default(),
            pending_edit_notifications: Vec::new(),
        }
    }

    // === Dirty flag ===

    /// Mark that the display list needs regeneration.
    pub const fn mark_dirty(&mut self) {
        self.display_list_dirty = true;
    }

    // === Editing lifecycle ===

    /// Whether a contenteditable element is currently being edited.
    #[must_use]
    pub const fn has_active_editing(&self) -> bool {
        self.multi_cursor.is_some()
    }

    /// Get the `DomId` of the node being edited.
    #[must_use]
    pub fn get_editing_dom_id(&self) -> Option<DomId> {
        self.multi_cursor.as_ref().map(|mc| mc.node_id.dom)
    }

    /// Get the `NodeId` of the node being edited.
    #[must_use]
    pub fn get_editing_node_id(&self) -> Option<NodeId> {
        self.multi_cursor
            .as_ref()
            .and_then(|mc| mc.node_id.node.into_crate_internal())
    }

    /// Get the primary cursor position (last-added cursor).
    #[must_use]
    pub fn get_primary_cursor(&self) -> Option<TextCursor> {
        self.multi_cursor
            .as_ref()
            .and_then(MultiCursorState::get_primary_cursor)
    }

    /// Whether the cursor should be drawn (editing active AND blink visible).
    #[must_use]
    pub const fn should_draw_cursor(&self) -> bool {
        self.has_active_editing() && self.blink.is_visible
    }

    /// Initialize editing for a newly focused contenteditable element.
    ///
    /// Creates a `MultiCursorState` with a single cursor, starts the blink,
    /// and sets preedit to None.
    ///
    /// # The caret is SOLID for the first half-period, not just "visible"
    ///
    /// This used to set `is_visible = true` and, in the same breath,
    /// `last_input_time = None`. Those two statements contradict each other:
    /// `None` is the "no input has EVER been recorded" encoding, for which
    /// [`BlinkState::should_blink`] is true immediately — so the blink timer's
    /// FIRST tick (a `Timer` with an interval and no delay runs on the first
    /// pump) toggled the freshly-shown caret straight back OFF. Clicking or
    /// tabbing into a field made the caret disappear on the next frame and only
    /// reappear a blink-interval later, which reads as a glitch and matches no
    /// other toolkit.
    ///
    /// It also made every caller responsible for repairing the state this
    /// method had just broken. Two of the three did:
    /// `LayoutWindow::handle_focus_change_for_cursor_blink` calls
    /// `reset_blink_on_input` BEFORE the deferred `finalize_pending_focus_changes`
    /// gets here (so its timestamp was overwritten with `None`), and
    /// `process_mouse_click_for_selection` calls it immediately AFTER (so its
    /// caret survived). The third, `process_accessibility_action`, did not — an
    /// AT-driven focus got the broken phase.
    ///
    /// `reset_blink_on_input(now)` sets BOTH halves consistently: caret shown
    /// AND the blink phase anchored at this instant, so `should_blink` stays
    /// false for `CURSOR_BLINK_INTERVAL_MS` and the first toggle happens one
    /// half-period after focus. That is the behaviour every other toolkit ships,
    /// and it makes the callers' own `reset_blink_on_input` calls redundant
    /// rather than load-bearing.
    ///
    /// `Instant::now()` honours the thread-scoped E2E test clock, so this stays
    /// deterministic under `tick_ms`.
    /// Record which focusable the caret is about to sit inside, dropping the
    /// tracked caret/selection geometry when that is a DIFFERENT one.
    ///
    /// Call this immediately before [`Self::initialize_editing`], with
    /// `LayoutWindow::find_focusable_ancestor` of the node the caret is landing
    /// on. The geometry is what the tween glides FROM, so dropping it is what
    /// turns a glide into a jump.
    ///
    /// Node identity is the wrong test: a text field is a focusable container
    /// wrapping a contenteditable child, so every caret move inside one field
    /// would look like a "different node" while two separate fields' carets
    /// look no different from two paragraphs of the same editor. The caret
    /// should glide between paragraphs sitting next to each other — it really
    /// did travel that distance — but never between two text inputs, where it
    /// would animate out of one box, across whatever lies between, and into
    /// the other.
    pub fn enter_focus_scope(&mut self, scope: Option<DomNodeId>) {
        // A scope of None (text in no focusable at all) compares equal to
        // itself, so plain prose keeps gliding within itself.
        if self.tween.focus_scope != scope {
            self.tween.reset_text_tweens();
        }
        self.tween.focus_scope = scope;
    }

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
        // The tween now tracks THIS node's caret. The previously rendered
        // geometry is kept on purpose — that is what makes the caret glide
        // from where it was to where it landed.
        //
        // Whether that glide is WANTED is decided by focus scope, not by node
        // identity, and only the tree knows the scopes: see
        // [`Self::enter_focus_scope`], which the caller invokes first and which
        // drops this geometry when the caret crosses into a different
        // focusable.
        self.tween.node = Some(dom_node_id);
        self.blink.reset_blink_on_input(Instant::now());
        self.clear_preedit();
        self.mark_dirty();
    }

    /// End editing (focus left the contenteditable element).
    pub fn clear_editing(&mut self) {
        // Only ask for a repaint if there was something to erase.
        //
        // This used to mark dirty unconditionally, and it is reached on EVERY
        // focus change — including a Tab between two nodes that were never
        // editable. The manager then owed a repaint it had no pixels for, and
        // because `display_list_dirty` is a latch, one such focus move left the
        // window permanently "not idle": a permanent repaint request, on every
        // frame, for the rest of the window's life.
        //
        // Caught by the E2E non-interference gate, which saw `text_edit` move
        // on a focus-only step with the fingerprint otherwise identical —
        // cursor=none, preedit="" both before and after, and only `dirty`
        // flipping. Nothing changed, so nothing was owed.
        let had_cursor = self.multi_cursor.is_some();
        let had_blink = self.blink.is_visible
            || self.blink.last_input_time.is_some()
            || self.blink.blink_timer_active;

        self.multi_cursor = None;
        self.blink.clear();
        let had_preedit = self.clear_preedit_returning_changed();

        if had_cursor || had_blink || had_preedit {
            self.mark_dirty();
        }
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
        let _ = self.clear_preedit_returning_changed();
    }

    /// Clear the preedit, reporting whether anything was actually cleared.
    ///
    /// Split out so `clear_editing` can decide whether a repaint is owed
    /// without marking dirty twice — and so clearing an ALREADY-clear preedit
    /// costs nothing, which is the common case on a focus change.
    fn clear_preedit_returning_changed(&mut self) -> bool {
        let changed = self.preedit_text.is_some()
            || self.preedit_cursor_begin != -1
            || self.preedit_cursor_end != -1;
        if !changed {
            return false;
        }
        self.preedit_text = None;
        self.preedit_cursor_begin = -1;
        self.preedit_cursor_end = -1;
        self.mark_dirty();
        true
    }

    // === Convenience for building cursor_locations ===

    /// Build the Vec of cursor locations for `LayoutContext`.
    ///
    /// Returns all cursor positions from `MultiCursorState`, or empty if not editing.
    #[must_use]
    pub fn build_cursor_locations(&self) -> Vec<(DomId, NodeId, TextCursor)> {
        let Some(ref mc) = self.multi_cursor else {
            return Vec::new();
        };
        let Some(node_id) = mc.node_id.node.into_crate_internal() else {
            return Vec::new();
        };
        mc.selections
            .iter()
            .map(|s| {
                let cursor = match &s.selection {
                    Selection::Cursor(c) => *c,
                    Selection::Range(r) => r.end,
                };
                (mc.node_id.dom, node_id, cursor)
            })
            .collect()
    }

    /// Cross-block selection (spans multiple IFC roots), precomputed by
    /// `LayoutWindow::set_cross_block_selection` — the manager stores it
    /// render-ready because computing the per-IFC ranges needs layout/text
    /// access the manager does not have. Cleared by any single-node cursor
    /// interaction. When set, it wins over `multi_cursor` for rendering.
    pub fn set_cross_block_selection(&mut self, sel: azul_core::selection::TextSelection) {
        self.cross_block = Some(sel);
        self.display_list_dirty = true;
    }

    /// Clear the cross-block selection (single-node interactions do this).
    pub fn clear_cross_block_selection(&mut self) {
        if self.cross_block.take().is_some() {
            self.display_list_dirty = true;
        }
    }

    /// Take the cross-block selection (delete/apply flows consume it).
    pub const fn take_cross_block_selection(
        &mut self,
    ) -> Option<azul_core::selection::TextSelection> {
        let s = self.cross_block.take();
        if s.is_some() {
            self.display_list_dirty = true;
        }
        s
    }

    /// The active cross-block selection, if any.
    #[must_use]
    pub const fn get_cross_block_selection(&self) -> Option<&azul_core::selection::TextSelection> {
        self.cross_block.as_ref()
    }

    /// Every range selection of the current editing session.
    ///
    /// `None` when there is no session, the session's node is detached, or the
    /// session holds only bare carets — a collapsed caret is not a selection,
    /// so there is nothing to highlight.
    ///
    /// This is the COMPLETE selection data. `select_next_occurrence` (Ctrl+D)
    /// builds sessions with several ranges on one node, and the render-facing
    /// [`Self::build_text_selections_map`] can carry only one of them (see
    /// there); anything that needs all of them reads this.
    #[must_use]
    pub fn session_selection_ranges(&self) -> Option<SessionSelectionRanges> {
        let mc = self.multi_cursor.as_ref()?;
        let node_id = mc.node_id.node.into_crate_internal()?;

        let mut ranges = Vec::new();
        let mut primary = None;
        for sel in &mc.selections {
            if let Selection::Range(range) = &sel.selection {
                if sel.id == mc.primary_id {
                    primary = Some(*range);
                }
                ranges.push(*range);
            }
        }

        // The primary selection can be a bare caret while other selections are
        // ranges (a multi-cursor click adds one; an edit can collapse the
        // primary range). The document-first range then stands in, so the
        // endpoints always describe a range that is actually painted.
        let primary = primary.or_else(|| ranges.first().copied())?;

        Some(SessionSelectionRanges {
            dom_id: mc.node_id.dom,
            node_id,
            ranges,
            primary,
        })
    }

    /// Build a `TextSelection` map for the display list's `paint_selections`.
    ///
    /// Extracts Range selections from `MultiCursorState` into the format that
    /// `LayoutContext.text_selections` expects: `BTreeMap<DomId, TextSelection>`.
    /// The `affected_nodes` map uses the editing node's `NodeId` as key.
    ///
    /// `anchor`, `focus` and `is_forward` all describe the SAME range — the
    /// session's primary, which is also one of the ranges in `affected_nodes`.
    /// They used to disagree: the endpoints came from the first range,
    /// `affected_nodes` kept the last (each insert overwrote the same key), and
    /// `is_forward` was hard-coded.
    ///
    /// `affected_nodes` carries EVERY range of the session under the one node
    /// key, so a multi-range (Ctrl+D) session paints all of its occurrences and
    /// not just the primary one.
    #[must_use]
    pub fn build_text_selections_map(
        &self,
    ) -> std::collections::BTreeMap<DomId, azul_core::selection::TextSelection> {
        if let Some(cb) = &self.cross_block {
            let mut map = std::collections::BTreeMap::new();
            map.insert(cb.dom_id, cb.clone());
            return map;
        }
        use azul_core::selection::{SelectionAnchor, SelectionFocus, TextSelection};

        let mut map = std::collections::BTreeMap::new();
        let Some(session) = self.session_selection_ranges() else {
            return map;
        };
        let range = session.primary;

        let mut affected_nodes = std::collections::BTreeMap::new();
        affected_nodes.insert(session.node_id, session.ranges);

        map.insert(
            session.dom_id,
            TextSelection {
                dom_id: session.dom_id,
                anchor: SelectionAnchor {
                    ifc_root_node_id: session.node_id,
                    cursor: range.start,
                    char_bounds: LogicalRect::zero(),
                    mouse_position: azul_core::geom::LogicalPosition::zero(),
                },
                focus: SelectionFocus {
                    ifc_root_node_id: session.node_id,
                    cursor: range.end,
                    mouse_position: azul_core::geom::LogicalPosition::zero(),
                },
                affected_nodes,
                is_forward: range_is_forward(&range),
            },
        );

        map
    }
}

impl crate::managers::NodeIdRemap for TextEditManager {
    /// Remap every node-keyed piece of editing state onto the rebuilt DOM: the
    /// multi-cursor session, the caret/selection tween geometry, the
    /// cross-block selection, and the queued edit notifications.
    ///
    /// `MultiCursorState::remap_node_ids` clears the selections when the edited
    /// node is gone; here we additionally drop the whole editing session, since a
    /// cursor whose IFC root no longer exists is not an editing session.
    fn remap_node_ids(&mut self, dom: DomId, map: &crate::managers::NodeIdMap) {
        // The tween's caret/selection geometry belongs to the session's node.
        // Resolve that anchor BEFORE the session below can be dropped — and
        // fall back to the session for state that was installed by writing
        // `multi_cursor` directly (which cannot set the anchor), so a stale
        // `None` heals itself here instead of leaving the geometry orphaned.
        let tween_node = self
            .tween
            .node
            .or_else(|| self.multi_cursor.as_ref().map(|mc| mc.node_id));

        if let Some(ref mut mc) = self.multi_cursor {
            if mc.node_id.dom == dom {
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
                } else {
                    mc.remap_node_ids(dom, map.as_btree_map());
                }
            }
        }

        // The tween follows its node, and dies with it: `last_caret` /
        // `last_selection` describe a rectangle that belonged to a node which
        // is now gone, and the next display-list pass would glide the caret
        // out of it across the screen.
        if let Some(old) = tween_node {
            match map.resolve_dom_node_id(dom, old) {
                Some(new_id) => self.tween.node = Some(new_id),
                None => self.tween.reset_text_tweens(),
            }
        }

        // A cross-block selection is render-ready geometry keyed by IFC-root
        // NodeIds. Unremapped, it paints a highlight over whichever nodes
        // inherited those indices.
        if self.cross_block.as_ref().is_some_and(|cb| cb.dom_id == dom) {
            self.remap_cross_block_selection(map);
        }

        // Queued `Input` notifications name the host they belong to; a host
        // that was unmounted has no event to dispatch, and keeping the id
        // would dispatch it at the node that took its place.
        self.pending_edit_notifications.retain_mut(|node| {
            match map.resolve_dom_node_id(dom, *node) {
                Some(new_id) => {
                    *node = new_id;
                    true
                }
                None => false,
            }
        });
    }
}

impl TextEditManager {
    /// Rewrite the cross-block selection's IFC-root ids for the rebuilt DOM.
    ///
    /// The selection is dropped outright when either endpoint's root is gone:
    /// a band whose anchor or focus no longer exists has no endpoints to paint
    /// between. Interior roots that were unmounted are dropped individually.
    fn remap_cross_block_selection(&mut self, map: &crate::managers::NodeIdMap) {
        let Some(ref mut cb) = self.cross_block else {
            return;
        };
        let (Some(anchor), Some(focus)) = (
            map.resolve(cb.anchor.ifc_root_node_id),
            map.resolve(cb.focus.ifc_root_node_id),
        ) else {
            self.cross_block = None;
            self.display_list_dirty = true;
            return;
        };
        let mut changed =
            anchor != cb.anchor.ifc_root_node_id || focus != cb.focus.ifc_root_node_id;
        cb.anchor.ifc_root_node_id = anchor;
        cb.focus.ifc_root_node_id = focus;

        let before: Vec<NodeId> = cb.affected_nodes.keys().copied().collect();
        cb.affected_nodes = core::mem::take(&mut cb.affected_nodes)
            .into_iter()
            .filter_map(|(node, ranges)| map.resolve(node).map(|new| (new, ranges)))
            .collect();
        changed |= !cb.affected_nodes.keys().copied().eq(before);

        // Only owe a repaint when the painted band actually moved:
        // `display_list_dirty` is a latch, and a rebuild that renumbered
        // nothing has no pixels to redraw (see `clear_editing`).
        if changed {
            self.display_list_dirty = true;
        }
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
        let primary_id = identified.last().map_or_else(SelectionId::new, |s| s.id);
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
        // The default interval is the wall-clock one, so every existing caller
        // that never sets an interval keeps the 530ms behaviour it had.
        assert_eq!(b.blink_interval, CURSOR_BLINK_INTERVAL);
        assert_eq!(
            b.blink_interval,
            Duration::from_millis(CURSOR_BLINK_INTERVAL_MS)
        );
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
        assert!(
            !b.is_visible,
            "an even number of toggles restores the state"
        );
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
    fn autotest_blink_should_blink_mismatched_instant_kinds_are_deterministic() {
        // A Tick instant compared against a System instant has no meaningful
        // span: the two counters have no common origin, so `duration_since`
        // saturates to `Duration::Tick(0)` and nothing is ever "elapsed".
        // (This is about mismatched INSTANTS, which really are incomparable —
        // unlike mismatched DURATIONS, which are just two units of the same
        // thing and now convert.)
        let mut b = BlinkState::new();
        b.reset_blink_on_input(Instant::now());
        let tick_now = Instant::Tick(SystemTick::new(u64::MAX));
        assert_eq!(b.should_blink(&tick_now), b.should_blink(&tick_now));
        assert!(!b.should_blink(&tick_now));
    }

    /// A tick-only clock (no_std, or any clockless build) MUST resume blinking.
    ///
    /// Both endpoints are Tick, so the elapsed span is a `Duration::Tick`, which
    /// is compared against the wall-clock-typed blink interval on a canonical
    /// scale. Before that comparison was unit-aware, the answer was `false`
    /// forever and the caret on a clockless build simply never blinked again.
    #[test]
    fn autotest_blink_a_tick_only_clock_resumes_blinking_at_the_exact_frame() {
        let mut t = BlinkState::new();
        t.reset_blink_on_input(Instant::Tick(SystemTick::new(0)));

        // 530ms is 31.8 frames at 60Hz, so frame 31 is early and frame 32 blinks.
        assert!(!t.should_blink(&Instant::Tick(SystemTick::new(31))));
        assert!(t.should_blink(&Instant::Tick(SystemTick::new(32))));
        assert!(t.should_blink(&Instant::Tick(SystemTick::new(u64::MAX))));
    }

    /// The whole point of the `t` unit: a blink interval expressed in FRAMES
    /// flips on exactly the Nth frame — frame N-1 is solid, frame N blinks.
    /// There is no rounding, no clock, and nothing for a slow machine to shift.
    #[test]
    fn autotest_blink_a_tick_interval_flips_on_exactly_the_nth_frame() {
        let mut b = BlinkState::new();
        b.set_blink_interval(Duration::from_ticks(5));
        b.reset_blink_on_input(Instant::Tick(SystemTick::new(100)));

        for frame in 100..=105 {
            assert!(
                !b.should_blink(&Instant::Tick(SystemTick::new(frame))),
                "frame {frame} is within 5 frames of the input and must stay solid"
            );
        }
        assert!(
            b.should_blink(&Instant::Tick(SystemTick::new(106))),
            "frame 106 is strictly more than 5 frames past the input"
        );
    }

    /// The same tick interval, driven off a WALL-CLOCK instant: 5 frames is
    /// 83.33ms, so 83ms is solid and 84ms blinks. A `5t` stylesheet value
    /// therefore behaves identically on a desktop shell and on a clockless one.
    #[test]
    fn autotest_blink_a_tick_interval_converts_on_a_wall_clock_instant() {
        let base = Instant::now();
        let mut b = BlinkState::new();
        b.set_blink_interval(Duration::from_ticks(5));
        b.reset_blink_on_input(base.clone());

        assert!(!b.should_blink(&plus_ms(&base, 83)));
        assert!(b.should_blink(&plus_ms(&base, 84)));
    }

    /// The refocus predicate: a running blink timer holds the interval it was
    /// BUILT with, so the only safe trigger for rebuilding it is "the value
    /// actually changed". Same value ⇒ false (never restart the blink phase for
    /// nothing); different value — including a different UNIT — ⇒ true.
    #[test]
    fn autotest_blink_adopt_interval_reports_only_real_changes() {
        let mut b = BlinkState::new();

        assert!(
            !b.adopt_blink_interval(CURSOR_BLINK_INTERVAL),
            "the default adopted again is not a change"
        );

        assert!(b.adopt_blink_interval(Duration::from_millis(250)));
        assert_eq!(b.blink_interval, Duration::from_millis(250));
        assert!(
            !b.adopt_blink_interval(Duration::from_millis(250)),
            "idempotent: refocusing a node with the SAME duration must not restart the timer"
        );

        // 5 frames is 83.33ms, and 83ms is not 5 frames: the unit is part of
        // the value, so switching between them is a real change.
        assert!(b.adopt_blink_interval(Duration::from_ticks(5)));
        assert!(b.adopt_blink_interval(Duration::from_millis(83)));
        assert!(b.adopt_blink_interval(Duration::from_ticks(5)));
        assert_eq!(b.blink_interval, Duration::from_ticks(5));

        // `clear()` puts the default back, so the next focus on a node with an
        // explicit duration sees a change and rebuilds.
        b.clear();
        assert!(b.adopt_blink_interval(Duration::from_ticks(5)));
    }

    #[test]
    fn autotest_blink_clear_resets_every_field_and_is_idempotent() {
        let mut b = BlinkState::new();
        b.reset_blink_on_input(Instant::now());
        b.set_blink_timer_active(true);
        b.set_blink_interval(Duration::from_ticks(5));

        b.clear();
        assert!(!b.is_visible);
        assert!(b.last_input_time.is_none());
        assert!(!b.is_blink_timer_active());
        assert_eq!(
            b.blink_interval, CURSOR_BLINK_INTERVAL,
            "the previous node's caret-animation-duration must not leak to the next"
        );

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
        // The caret is SOLID for the first half-period after focus, so the blink
        // phase must be ANCHORED here. `None` would mean "no input ever", for
        // which `should_blink` is true immediately and the timer's first tick
        // hides the caret the user just placed. See `initialize_editing`.
        let anchored = m
            .blink
            .last_input_time
            .as_ref()
            .expect("initialize_editing must anchor the blink phase, not clear it");
        assert!(
            !m.blink.should_blink(anchored),
            "at the instant of focus, zero time has elapsed — blinking must not be allowed yet"
        );
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
        // A fresh manager has no cursor, no blink and no preedit, so clearing it
        // erases NOTHING and owes no repaint. This assertion used to read
        // `assert!(m.display_list_dirty, "clear_editing marks dirty
        // unconditionally")` — it documented the behaviour as found rather than
        // as intended, and what it documented was a bug: `clear_editing` runs on
        // every focus change, `display_list_dirty` is a LATCH, and so one Tab
        // between two never-editable nodes left the window owing a repaint on
        // every frame for the rest of its life.
        assert!(
            !m.display_list_dirty,
            "clearing an already-clear manager must not request a repaint"
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
            "こんにちは",        // CJK — the common IME case
            "👨‍👩‍👧‍👦",                // ZWJ emoji family (one grapheme, many bytes)
            "e\u{0301}\u{0327}", // combining acute + cedilla
            "مرحبا",             // RTL
            "a\u{0000}b",        // interior NUL
            "\u{FEFF}bom",       // byte-order mark
            "🇩🇪🇯🇵",              // regional-indicator flags
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
        // The FIRST clear (above) really did clear a preedit and correctly marked
        // dirty. This second one has nothing left to clear, so it must not.
        // Previously asserted the opposite, in as many words: "clear_preedit
        // marks dirty even when nothing changed".
        assert!(
            !m.display_list_dirty,
            "a no-op clear_preedit must not request a repaint"
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
        m.multi_cursor = Some(multi_cursor_with(dom_node(DOM0, Some(node)), selections, 9));

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
        assert!(TextEditManager::new()
            .build_text_selections_map()
            .is_empty());
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
        assert_eq!(sel.ranges_for_node(&node), &[range(start, end)]);
        assert_eq!(sel.get_range_for_node(&node), Some(&range(start, end)));
    }

    #[test]
    fn autotest_build_text_selections_map_backward_range_reports_is_forward_false() {
        // A backward drag anchors at 9 and puts the focus at 2, so the emitted
        // selection must say so. `is_forward` used to be hard-coded `true`.
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
        assert!(!sel.is_forward, "anchor is logically AFTER the focus");
        assert!(!range_is_forward(&range(start, end)));
        assert!(
            range_is_forward(&range(end, start)),
            "the mirrored drag is forward"
        );
        assert!(
            range_is_forward(&range(start, start)),
            "a degenerate range counts as forward, like TextSelection::new_collapsed"
        );
    }

    #[test]
    fn autotest_build_text_selections_map_multi_range_endpoints_match_the_painted_range() {
        // The two halves of the emitted `TextSelection` must agree: the
        // `anchor`/`focus` endpoints describe the PRIMARY range, and that range
        // is one of the ranges `affected_nodes` paints. They used to disagree —
        // endpoints from the FIRST range, the map from the LAST (each insert
        // overwrote the same key, so a Ctrl+D session painted ONE occurrence).
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
        assert_eq!(
            sel.anchor.cursor, last.start,
            "endpoints from the PRIMARY range"
        );
        assert_eq!(sel.focus.cursor, last.end);
        assert_eq!(sel.affected_nodes.len(), 1, "one node key, several ranges");
        assert_eq!(
            sel.ranges_for_node(&node),
            &[first, last],
            "BOTH occurrences reach the painter, in document order"
        );
        assert!(
            sel.ranges_for_node(&node).contains(&last),
            "the range the endpoints describe is one of the painted ones"
        );

        // …and no range is lost on the way: the session reports both.
        let session = m.session_selection_ranges().expect("a session with ranges");
        assert_eq!(session.dom_id, DOM0);
        assert_eq!(session.node_id, node);
        assert_eq!(session.ranges, vec![first, last], "carets are not ranges");
        assert_eq!(session.primary, last);
    }

    #[test]
    fn autotest_session_selection_ranges_falls_back_to_the_first_range() {
        // The primary selection is a bare CARET here (the fixture makes the
        // last element primary), so the endpoints stand in from the first
        // range — never from a caret, which would emit a collapsed selection
        // and paint nothing.
        let node = NodeId::new(2);
        let only = range(cursor(0, 4), cursor(0, 7));

        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM1, Some(node)),
            vec![Selection::Range(only), Selection::Cursor(cursor(0, 12))],
            9,
        ));

        let session = m
            .session_selection_ranges()
            .expect("one range is a session");
        assert_eq!(session.ranges, vec![only]);
        assert_eq!(session.primary, only);

        let map = m.build_text_selections_map();
        let sel = map.get(&DOM1).expect("keyed by the editing DomId");
        assert_eq!(sel.anchor.cursor, only.start);
        assert_eq!(sel.focus.cursor, only.end);
    }

    #[test]
    fn autotest_session_selection_ranges_is_none_without_ranges() {
        assert!(TextEditManager::new().session_selection_ranges().is_none());

        // Carets only — nothing to highlight.
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(0, 3), DOM0, NodeId::new(1), 8);
        assert!(m.session_selection_ranges().is_none());

        // A range on a DETACHED node has no IFC root to express it against.
        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM0, None),
            vec![Selection::Range(range(cursor(0, 0), cursor(0, 1)))],
            1,
        ));
        assert!(m.session_selection_ranges().is_none());
        assert!(m.build_text_selections_map().is_empty());
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
        assert_eq!(sel.ranges_for_node(&node), &[range(point, point)]);
    }

    // ------------------------------------------------------------------
    // TextEditManager — NodeIdRemap (DOM rebuild)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_remap_without_session_is_a_noop() {
        let mut m = TextEditManager::new();
        m.remap_node_ids(
            DOM0,
            &NodeIdMap::from_pairs([(NodeId::ZERO, NodeId::new(1))]),
        );

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

        m.remap_node_ids(
            DOM0,
            &NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(8))]),
        );

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
        m.remap_node_ids(
            DOM0,
            &NodeIdMap::from_pairs([(NodeId::new(4), NodeId::new(4))]),
        );

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

    // ------------------------------------------------------------------
    // TextTweenState — the tween must follow (and die with) its node
    // ------------------------------------------------------------------

    fn rect(x: f32, y: f32) -> LogicalRect {
        LogicalRect::new(
            azul_core::geom::LogicalPosition { x, y },
            azul_core::geom::LogicalSize {
                width: 2.0,
                height: 16.0,
            },
        )
    }

    fn instant() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    /// A manager editing `node` in `DOM0` with a caret tween and a selection
    /// tween both mid-flight, and both "last rendered" geometries recorded.
    fn manager_with_live_tween(node: NodeId) -> TextEditManager {
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(0, 0), DOM0, node, 7);
        m.tween.dom_id = Some(DOM0);
        m.tween.caret = Some(CaretTweenTrack {
            from: rect(10.0, 0.0),
            to: rect(40.0, 0.0),
            start: instant(),
        });
        m.tween.last_caret = Some(rect(25.0, 0.0));
        m.tween.selection = Some(SelectionTweenTrack {
            from: vec![rect(0.0, 0.0)],
            to: vec![rect(60.0, 0.0)],
            start: instant(),
        });
        m.tween.last_selection = vec![rect(30.0, 0.0)];
        m.tween.publish_active();
        m
    }

    #[test]
    fn autotest_remap_keeps_the_tween_anchored_to_a_moved_node() {
        let mut m = manager_with_live_tween(NodeId::new(3));
        assert_eq!(m.tween.node, Some(dom_node(DOM0, Some(NodeId::new(3)))));

        // A sibling was inserted ahead of it: same node, new index.
        m.remap_node_ids(
            DOM0,
            &NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(4))]),
        );

        assert_eq!(
            m.tween.node,
            Some(dom_node(DOM0, Some(NodeId::new(4)))),
            "the tween must follow the node it belongs to"
        );
        assert_eq!(m.get_editing_node_id(), Some(NodeId::new(4)));
        // The geometry is where the caret was actually RENDERED last frame, so
        // a move keeps it: that is what makes the glide continuous.
        assert_eq!(m.tween.last_caret, Some(rect(25.0, 0.0)));
        assert_eq!(m.tween.last_selection, vec![rect(30.0, 0.0)]);
        assert!(m.tween.caret.is_some());
        assert!(m.tween.selection.is_some());
        assert!(m.tween.is_active());
    }

    #[test]
    fn autotest_remap_clears_the_tween_when_the_edited_node_is_unmounted() {
        let mut m = manager_with_live_tween(NodeId::new(3));
        assert!(m.tween.tick_flag.load(AtomicOrdering::Acquire));

        // Node 3 is absent from the map => unmounted.
        m.remap_node_ids(
            DOM0,
            &NodeIdMap::from_pairs([(NodeId::new(9), NodeId::new(9))]),
        );

        assert!(m.tween.node.is_none());
        assert!(
            m.tween.caret.is_none() && m.tween.last_caret.is_none(),
            "caret geometry belonging to a deleted node must not survive"
        );
        assert!(m.tween.selection.is_none());
        assert!(m.tween.last_selection.is_empty());
        assert!(!m.tween.is_active());
        assert!(
            !m.tween.tick_flag.load(AtomicOrdering::Acquire),
            "the timer flag must be republished, or the tween timer keeps ticking"
        );
    }

    #[test]
    fn autotest_remap_clears_the_tween_of_a_session_installed_without_the_anchor() {
        // `multi_cursor` is a public field and several call sites assign it
        // directly, which cannot set `tween.node`. The remap re-derives the
        // anchor from the session so that state is not orphaned.
        let mut m = TextEditManager::new();
        m.multi_cursor = Some(multi_cursor_with(
            dom_node(DOM0, Some(NodeId::new(2))),
            vec![Selection::Cursor(cursor(0, 0))],
            1,
        ));
        m.tween.last_caret = Some(rect(11.0, 0.0));
        assert!(m.tween.node.is_none());

        m.remap_node_ids(
            DOM0,
            &NodeIdMap::from_pairs([(NodeId::new(5), NodeId::new(5))]),
        );

        assert!(m.tween.last_caret.is_none());
        assert!(m.tween.node.is_none());
    }

    #[test]
    fn autotest_remap_leaves_the_tween_of_another_dom_alone() {
        let mut m = TextEditManager::new();
        m.initialize_editing(cursor(0, 0), DOM1, NodeId::new(3), 7);
        m.tween.last_caret = Some(rect(12.0, 0.0));

        m.remap_node_ids(DOM0, &NodeIdMap::default());

        assert_eq!(m.tween.node, Some(dom_node(DOM1, Some(NodeId::new(3)))));
        assert_eq!(m.tween.last_caret, Some(rect(12.0, 0.0)));
    }

    #[test]
    fn autotest_text_tween_state_clone_copies_every_field() {
        let m = manager_with_live_tween(NodeId::new(3));
        let clone = m.tween.clone();

        assert_eq!(clone.dom_id, m.tween.dom_id);
        assert_eq!(clone.node, m.tween.node);
        assert_eq!(clone.last_caret, m.tween.last_caret);
        assert_eq!(clone.last_selection, m.tween.last_selection);
        let (a, b) = (
            clone
                .caret
                .as_ref()
                .expect("in-flight caret must be cloned"),
            m.tween.caret.as_ref().expect("original"),
        );
        assert_eq!(a.from, b.from);
        assert_eq!(a.to, b.to);
        let (a, b) = (
            clone
                .selection
                .as_ref()
                .expect("in-flight selection must be cloned"),
            m.tween.selection.as_ref().expect("original"),
        );
        assert_eq!(a.from, b.from);
        assert_eq!(a.to, b.to);
        assert!(clone.is_active(), "a clone of a running tween is running");
        assert!(clone.tick_flag.load(AtomicOrdering::Acquire));
    }

    #[test]
    fn autotest_text_tween_state_clone_does_not_share_the_timer_flag() {
        // Two managers sharing one flag would steer each other's tween timer.
        let m = manager_with_live_tween(NodeId::new(3));
        let mut clone = m.tween.clone();
        assert!(!Arc::ptr_eq(&clone.tick_flag, &m.tween.tick_flag));

        clone.reset();
        assert!(!clone.tick_flag.load(AtomicOrdering::Acquire));
        assert!(
            m.tween.tick_flag.load(AtomicOrdering::Acquire),
            "the original's timer must keep running"
        );
    }

    #[test]
    fn autotest_manager_clone_carries_the_tween() {
        let m = manager_with_live_tween(NodeId::new(3));
        let clone = m.clone();
        assert_eq!(clone.tween.node, m.tween.node);
        assert_eq!(clone.tween.last_caret, m.tween.last_caret);
        assert!(clone.tween.is_active());
    }

    // ------------------------------------------------------------------
    // Cross-block selection + queued edit notifications also carry NodeIds
    // ------------------------------------------------------------------

    fn cross_block_selection(anchor: NodeId, focus: NodeId) -> azul_core::selection::TextSelection {
        use azul_core::selection::{SelectionAnchor, SelectionFocus, TextSelection};
        let mut affected = alloc::collections::BTreeMap::new();
        affected.insert(anchor, vec![range(cursor(0, 0), cursor(0, 1))]);
        affected.insert(focus, vec![range(cursor(0, 0), cursor(0, 2))]);
        TextSelection {
            dom_id: DOM0,
            anchor: SelectionAnchor {
                ifc_root_node_id: anchor,
                cursor: cursor(0, 0),
                char_bounds: LogicalRect::zero(),
                mouse_position: azul_core::geom::LogicalPosition::zero(),
            },
            focus: SelectionFocus {
                ifc_root_node_id: focus,
                cursor: cursor(0, 2),
                mouse_position: azul_core::geom::LogicalPosition::zero(),
            },
            affected_nodes: affected,
            is_forward: true,
        }
    }

    #[test]
    fn autotest_remap_rewrites_the_cross_block_selection() {
        let mut m = TextEditManager::new();
        m.set_cross_block_selection(cross_block_selection(NodeId::new(2), NodeId::new(5)));

        m.remap_node_ids(
            DOM0,
            &NodeIdMap::from_pairs([
                (NodeId::new(2), NodeId::new(3)),
                (NodeId::new(5), NodeId::new(6)),
            ]),
        );

        let cb = m.get_cross_block_selection().expect("both roots survived");
        assert_eq!(cb.anchor.ifc_root_node_id, NodeId::new(3));
        assert_eq!(cb.focus.ifc_root_node_id, NodeId::new(6));
        assert_eq!(cb.ranges_for_node(&NodeId::new(3)).len(), 1);
        assert_eq!(cb.ranges_for_node(&NodeId::new(6)).len(), 1);
        assert!(
            cb.ranges_for_node(&NodeId::new(2)).is_empty(),
            "the old index must not still paint"
        );
    }

    #[test]
    fn autotest_remap_drops_the_cross_block_selection_when_an_endpoint_is_gone() {
        let mut m = TextEditManager::new();
        m.set_cross_block_selection(cross_block_selection(NodeId::new(2), NodeId::new(5)));

        m.remap_node_ids(
            DOM0,
            &NodeIdMap::from_pairs([(NodeId::new(2), NodeId::new(2))]),
        );

        assert!(
            m.get_cross_block_selection().is_none(),
            "a band with no focus root has nothing to paint between"
        );
    }

    #[test]
    fn autotest_remap_leaves_a_cross_block_selection_of_another_dom_alone() {
        let mut m = TextEditManager::new();
        let mut sel = cross_block_selection(NodeId::new(2), NodeId::new(5));
        sel.dom_id = DOM1;
        m.set_cross_block_selection(sel);

        m.remap_node_ids(DOM0, &NodeIdMap::default());

        let cb = m
            .get_cross_block_selection()
            .expect("other DOM is untouched");
        assert_eq!(cb.anchor.ifc_root_node_id, NodeId::new(2));
        assert_eq!(cb.focus.ifc_root_node_id, NodeId::new(5));
    }

    #[test]
    fn autotest_remap_of_a_stable_cross_block_selection_owes_no_repaint() {
        let mut m = TextEditManager::new();
        m.set_cross_block_selection(cross_block_selection(NodeId::new(2), NodeId::new(5)));
        m.display_list_dirty = false;

        m.remap_node_ids(
            DOM0,
            &NodeIdMap::from_pairs([
                (NodeId::new(2), NodeId::new(2)),
                (NodeId::new(5), NodeId::new(5)),
            ]),
        );

        assert!(m.get_cross_block_selection().is_some());
        assert!(
            !m.display_list_dirty,
            "a rebuild that renumbered nothing has no pixels to redraw"
        );
    }

    #[test]
    fn autotest_remap_rewrites_and_prunes_pending_edit_notifications() {
        let mut m = TextEditManager::new();
        m.pending_edit_notifications = vec![
            dom_node(DOM0, Some(NodeId::new(1))),
            dom_node(DOM0, Some(NodeId::new(4))),
            dom_node(DOM1, Some(NodeId::new(4))),
        ];

        m.remap_node_ids(
            DOM0,
            &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(0))]),
        );

        assert_eq!(
            m.pending_edit_notifications,
            vec![
                dom_node(DOM0, Some(NodeId::new(0))),
                dom_node(DOM1, Some(NodeId::new(4))),
            ],
            "surviving hosts are rewritten, unmounted ones dropped, other DOMs untouched"
        );
    }
}
