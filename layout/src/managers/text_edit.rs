//! Unified text editing manager
//!
//! Single source of truth for all text editing state. `MultiCursorState` is
//! the primary cursor/selection system. `BlinkState` handles the caret blink
//! animation. `SelectionManager` handles non-editable drag-select only.
//!
//! Every mutation that affects visual output sets `display_list_dirty = true`,
//! ensuring the display list is always regenerated.

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    selection::{
        CursorAffinity, GraphemeClusterId, MultiCursorState, Selection, TextCursor,
    },
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
pub struct BlinkState {
    /// Whether the cursor is currently visible (toggled by blink timer)
    pub is_visible: bool,
    /// Timestamp of the last user input event (keyboard, mouse click in text).
    /// Used to determine whether to blink or stay solid while typing.
    pub last_input_time: Option<Instant>,
    /// Whether the cursor blink timer is currently active
    pub blink_timer_active: bool,
}

impl Default for BlinkState {
    fn default() -> Self {
        Self {
            is_visible: false,
            last_input_time: None,
            blink_timer_active: false,
        }
    }
}

impl BlinkState {
    pub fn new() -> Self { Self::default() }

    /// Reset blink on user input — cursor stays solid until blink interval elapses.
    pub fn reset_blink_on_input(&mut self, now: Instant) {
        self.is_visible = true;
        self.last_input_time = Some(now);
    }

    /// Toggle cursor visibility (called by blink timer callback).
    pub fn toggle_visibility(&mut self) -> bool {
        self.is_visible = !self.is_visible;
        self.is_visible
    }

    pub fn set_visibility(&mut self, visible: bool) {
        self.is_visible = visible;
    }

    pub fn set_blink_timer_active(&mut self, active: bool) {
        self.blink_timer_active = active;
    }

    pub fn is_blink_timer_active(&self) -> bool {
        self.blink_timer_active
    }

    /// Check if enough time has passed since last input to start blinking.
    pub fn should_blink(&self, now: &Instant) -> bool {
        use azul_core::task::{Duration, SystemTimeDiff};
        match &self.last_input_time {
            Some(last_input) => {
                let elapsed = now.duration_since(last_input);
                let blink_interval = Duration::System(SystemTimeDiff::from_millis(CURSOR_BLINK_INTERVAL_MS));
                elapsed.greater_than(&blink_interval)
            }
            None => true,
        }
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
/// `selection_manager` handles non-editable text drag-select only.
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
    /// Byte offset of cursor within preedit text (from IME), or -1 if unset
    pub preedit_cursor_begin: i32,
    /// Byte offset of cursor end within preedit text (from IME), or -1 if unset
    pub preedit_cursor_end: i32,
    /// Set to true by any mutation that changes visual output.
    pub display_list_dirty: bool,
}

impl Default for TextEditManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for TextEditManager {
    fn eq(&self, other: &Self) -> bool {
        self.multi_cursor == other.multi_cursor
    }
}

impl TextEditManager {
    /// Create a new text edit manager with no active editing state
    pub fn new() -> Self {
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

    /// Check and clear the display_list_dirty flag.
    pub fn take_display_list_dirty(&mut self) -> bool {
        let v = self.display_list_dirty;
        self.display_list_dirty = false;
        v
    }

    /// Mark that the display list needs regeneration.
    pub fn mark_dirty(&mut self) {
        self.display_list_dirty = true;
    }

    // === Editing lifecycle ===

    /// Whether a contenteditable element is currently being edited.
    pub fn has_active_editing(&self) -> bool {
        self.multi_cursor.is_some()
    }

    /// Get the DomId of the node being edited.
    pub fn get_editing_dom_id(&self) -> Option<DomId> {
        self.multi_cursor.as_ref().map(|mc| mc.node_id.dom)
    }

    /// Get the NodeId of the node being edited.
    pub fn get_editing_node_id(&self) -> Option<NodeId> {
        self.multi_cursor.as_ref()
            .and_then(|mc| mc.node_id.node.into_crate_internal())
    }

    /// Get the primary cursor position (last-added cursor).
    pub fn get_primary_cursor(&self) -> Option<TextCursor> {
        self.multi_cursor.as_ref().and_then(|mc| mc.get_primary_cursor())
    }

    /// Whether the cursor should be drawn (editing active AND blink visible).
    pub fn should_draw_cursor(&self) -> bool {
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
    }

    // === Convenience for building cursor_locations ===

    /// Build the Vec of cursor locations for LayoutContext.
    ///
    /// Returns all cursor positions from MultiCursorState, or empty if not editing.
    pub fn build_cursor_locations(&self) -> Vec<(DomId, NodeId, TextCursor)> {
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

    /// Build a TextSelection map for the display list's `paint_selections`.
    ///
    /// Extracts Range selections from MultiCursorState into the format that
    /// `LayoutContext.text_selections` expects: `BTreeMap<DomId, TextSelection>`.
    /// The `affected_nodes` map uses the editing node's NodeId as key.
    pub fn build_text_selections_map(&self) -> std::collections::BTreeMap<DomId, azul_core::selection::TextSelection> {
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
