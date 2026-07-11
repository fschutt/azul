//! Text selection and cursor positioning for inline content.
//!
//! This module provides data structures for managing text cursors and selection ranges
//! in a bidirectional (Bidi) and line-breaking aware manner. It handles:
//!
//! - **Grapheme cluster identification**: Unicode-aware character boundaries
//! - **Bidi support**: Cursor movement in mixed LTR/RTL text
//! - **Stable positions**: Selection anchors survive layout changes
//! - **Affinity tracking**: Cursor position at leading/trailing edges
//! - **Multi-node selection**: Browser-style selection spanning multiple DOM nodes
//!
//! # Architecture
//!
//! Text positions are represented as:
//! - `ContentIndex`: Logical position in the original inline content array
//! - `GraphemeClusterId`: Stable identifier for a grapheme cluster (survives reordering)
//! - `TextCursor`: Precise cursor location with leading/trailing affinity
//! - `SelectionRange`: Start and end cursors defining a selection
//!
//! Multi-node selection uses an Anchor/Focus model (W3C Selection API):
//! - `SelectionAnchor`: Fixed point where user started selection (mousedown)
//! - `SelectionFocus`: Movable point where selection currently ends (drag position)
//! - `TextSelection`: Complete selection state spanning potentially multiple IFC roots
//!
//! # Use Cases
//!
//! - Text editing: Insert/delete at cursor position
//! - Selection rendering: Highlight selected text across multiple nodes
//! - Keyboard navigation: Move cursor by grapheme/word/line
//! - Mouse selection: Convert pixel coordinates to text positions
//! - Drag selection: Extend selection across multiple DOM nodes
//!
//! # Examples
//!
//! ```rust,no_run
//! use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
//!
//! let cursor = TextCursor {
//!     cluster_id: GraphemeClusterId {
//!         source_run: 0,
//!         start_byte_in_run: 0,
//!     },
//!     affinity: CursorAffinity::Leading,
//! };
//! ```

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::dom::{DomId, DomNodeId, NodeId};
use crate::geom::{LogicalPosition, LogicalRect};

/// A stable, logical pointer to an item within the original `InlineContent` array.
///
/// This structure eliminates the need for string concatenation and byte-offset math
/// by tracking both the run index and the item index within that run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentIndex {
    /// The index of the `InlineContent` run in the original input array.
    pub run_index: u32,
    /// The byte index of the character or item *within* that run's string.
    pub item_index: u32,
}

/// A stable, logical identifier for a grapheme cluster.
///
/// This survives Bidi reordering and line breaking, making it ideal for tracking
/// text positions for selection and cursor logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct GraphemeClusterId {
    /// The `run_index` from the source `ContentIndex`.
    pub source_run: u32,
    /// The byte index of the start of the cluster in its original `StyledRun`.
    pub start_byte_in_run: u32,
}

/// Represents the logical position of the cursor *between* two grapheme clusters
/// or at the start/end of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(C)]
pub enum CursorAffinity {
    /// The cursor is at the leading edge of the character (left in LTR, right in RTL).
    Leading,
    /// The cursor is at the trailing edge of the character (right in LTR, left in RTL).
    Trailing,
}

/// Represents a precise cursor location in the logical text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(C)]
pub struct TextCursor {
    /// The grapheme cluster the cursor is associated with.
    pub cluster_id: GraphemeClusterId,
    /// The edge of the cluster the cursor is on.
    pub affinity: CursorAffinity,
}

impl_option!(
    TextCursor,
    OptionTextCursor,
    [Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd]
);

/// Represents a range of selected text. The direction is implicit (start can be
/// logically after end if selecting backwards).
#[derive(Debug, PartialOrd, Ord, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct SelectionRange {
    pub start: TextCursor,
    pub end: TextCursor,
}

impl_option!(
    SelectionRange,
    OptionSelectionRange,
    [Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd]
);

impl_vec!(SelectionRange, SelectionRangeVec, SelectionRangeVecDestructor, SelectionRangeVecDestructorType, SelectionRangeVecSlice, OptionSelectionRange);
impl_vec_debug!(SelectionRange, SelectionRangeVec);
impl_vec_clone!(
    SelectionRange,
    SelectionRangeVec,
    SelectionRangeVecDestructor
);
impl_vec_partialeq!(SelectionRange, SelectionRangeVec);
impl_vec_partialord!(SelectionRange, SelectionRangeVec);

/// A single selection, which can be either a blinking cursor or a highlighted range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum Selection {
    Cursor(TextCursor),
    Range(SelectionRange),
}

impl_option!(
    Selection,
    OptionSelection,
    [Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord]
);

impl_vec!(Selection, SelectionVec, SelectionVecDestructor, SelectionVecDestructorType, SelectionVecSlice, OptionSelection);
impl_vec_debug!(Selection, SelectionVec);
impl_vec_clone!(Selection, SelectionVec, SelectionVecDestructor);
impl_vec_partialeq!(Selection, SelectionVec);
impl_vec_partialord!(Selection, SelectionVec);

/// The complete selection state for a single text block, supporting multiple cursors/ranges.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct SelectionState {
    /// A list of all active selections. This list is kept sorted and non-overlapping.
    pub selections: SelectionVec,
    /// The DOM node this selection state applies to.
    pub node_id: DomNodeId,
}

impl SelectionState {
    /// Adds a new selection, merging it with any existing selections it overlaps with.
    pub fn add(&mut self, new_selection: Selection) {
        // A full implementation would handle merging overlapping ranges.
        // For now, we simply add and sort for simplicity.
        let mut selections: Vec<Selection> = self.selections.as_ref().to_vec();
        selections.push(new_selection);
        selections.sort_unstable();
        selections.dedup(); // Removes duplicate cursors
        self.selections = selections.into();
    }

}

impl_option!(
    SelectionState,
    OptionSelectionState,
    copy = false,
    clone = false,
    [Debug, Clone, PartialEq]
);

// ============================================================================
// MULTI-CURSOR SUPPORT (Sublime Text style)
// ============================================================================

/// Stable identifier for a cursor/selection within a `MultiCursorState`.
///
/// Uses a monotonic u64 counter (not UUID) so it is `Copy` and C-API friendly.
/// Each `SelectionId` is unique within the lifetime of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct SelectionId {
    pub inner: u64,
}

impl SelectionId {
    /// Generate a new unique `SelectionId`.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self { inner: COUNTER.fetch_add(1, Ordering::Relaxed) }
    }
}

/// Note: `Default` generates a new unique ID (increments global counter),
/// rather than returning a zero/sentinel value.
impl Default for SelectionId {
    fn default() -> Self {
        Self::new()
    }
}

impl_option!(
    SelectionId,
    OptionSelectionId,
    [Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord]
);

impl_vec!(SelectionId, SelectionIdVec, SelectionIdVecDestructor, SelectionIdVecDestructorType, SelectionIdVecSlice, OptionSelectionId);
impl_vec_debug!(SelectionId, SelectionIdVec);
impl_vec_clone!(SelectionId, SelectionIdVec, SelectionIdVecDestructor);
impl_vec_partialeq!(SelectionId, SelectionIdVec);
impl_vec_partialord!(SelectionId, SelectionIdVec);

/// A selection (cursor or range) paired with a stable identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct IdentifiedSelection {
    pub id: SelectionId,
    pub selection: Selection,
}

impl_option!(
    IdentifiedSelection,
    OptionIdentifiedSelection,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

impl_vec!(IdentifiedSelection, IdentifiedSelectionVec, IdentifiedSelectionVecDestructor, IdentifiedSelectionVecDestructorType, IdentifiedSelectionVecSlice, OptionIdentifiedSelection);
impl_vec_debug!(IdentifiedSelection, IdentifiedSelectionVec);
impl_vec_clone!(IdentifiedSelection, IdentifiedSelectionVec, IdentifiedSelectionVecDestructor);
impl_vec_partialeq!(IdentifiedSelection, IdentifiedSelectionVec);

/// Multi-cursor state for a contenteditable element (Sublime Text style).
///
/// Replaces the split `CursorManager` + `SelectionManager` pattern for text editing.
/// Supports multiple simultaneous cursors/selections, each with a stable ID.
///
/// ## Invariants
///
/// - `selections` is sorted by position and non-overlapping.
/// - The **primary** selection is identified by the stable `primary_id`, NOT by
///   vector position: `merge_overlapping()` re-sorts `selections` by position,
///   so "last index" is not the most-recently-added cursor.
/// - After any mutation, `merge_overlapping()` is called to maintain invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiCursorState {
    /// Sorted by position, non-overlapping. Primary is tracked via `primary_id`.
    pub selections: Vec<IdentifiedSelection>,
    /// Stable ID of the primary selection (most recently added/set). Survives the
    /// position sort in `merge_overlapping`, which would otherwise make the
    /// vector's last element (position-last) masquerade as the primary.
    pub primary_id: SelectionId,
    /// The DOM node this multi-cursor state applies to.
    pub node_id: DomNodeId,
    /// Stable key that survives DOM rebuilds (from `calculate_contenteditable_key`).
    pub contenteditable_key: u64,
}

impl MultiCursorState {
    /// Create a new `MultiCursorState` with a single cursor.
    #[must_use] pub fn new_with_cursor(cursor: TextCursor, node_id: DomNodeId, contenteditable_key: u64) -> Self {
        let id = SelectionId::new();
        Self {
            selections: vec![IdentifiedSelection {
                id,
                selection: Selection::Cursor(cursor),
            }],
            primary_id: id,
            node_id,
            contenteditable_key,
        }
    }

    /// Add a cursor, merging if it overlaps with existing selections.
    /// Returns the `SelectionId` of the new (or merged) cursor.
    #[must_use]
    pub fn add_cursor(&mut self, cursor: TextCursor) -> SelectionId {
        let id = SelectionId::new();
        self.selections.push(IdentifiedSelection {
            id,
            selection: Selection::Cursor(cursor),
        });
        self.primary_id = id;
        self.merge_overlapping();
        id
    }

    /// Add a selection range, merging if it overlaps.
    /// Returns the `SelectionId` of the new (or merged) selection.
    #[must_use]
    pub fn add_selection(&mut self, range: SelectionRange) -> SelectionId {
        let id = SelectionId::new();
        self.selections.push(IdentifiedSelection {
            id,
            selection: Selection::Range(range),
        });
        self.primary_id = id;
        self.merge_overlapping();
        id
    }

    /// Remove a selection by its stable ID. Returns true if found and removed.
    #[must_use]
    pub fn remove_selection(&mut self, id: SelectionId) -> bool {
        let len_before = self.selections.len();
        self.selections.retain(|s| s.id != id);
        let removed = self.selections.len() < len_before;
        if removed {
            // If we just removed the primary, re-point it at a surviving one.
            self.ensure_primary_valid();
        }
        removed
    }

    /// Get the primary selection (the most recently added/set, tracked by
    /// `primary_id` — NOT the vector's last element, which position-sorting
    /// reorders). Falls back to the last element if `primary_id` was somehow
    /// lost.
    #[must_use] pub fn get_primary(&self) -> Option<&IdentifiedSelection> {
        let pid = self.primary_id;
        self.selections
            .iter()
            .find(|s| s.id == pid)
            .or_else(|| self.selections.last())
    }

    /// Get a mutable reference to the primary selection (see `get_primary`).
    pub fn get_primary_mut(&mut self) -> Option<&mut IdentifiedSelection> {
        let pid = self.primary_id;
        if let Some(pos) = self.selections.iter().position(|s| s.id == pid) {
            return self.selections.get_mut(pos);
        }
        self.selections.last_mut()
    }

    /// Ensure `primary_id` names a selection that still exists; if not, adopt the
    /// last selection's id (best effort) so `get_primary` stays meaningful.
    fn ensure_primary_valid(&mut self) {
        let pid = self.primary_id;
        if !self.selections.iter().any(|s| s.id == pid) {
            if let Some(last) = self.selections.last() {
                self.primary_id = last.id;
            }
        }
    }

    /// Get the primary cursor position (for scroll-into-view, IME, etc.)
    #[must_use] pub fn get_primary_cursor(&self) -> Option<TextCursor> {
        self.get_primary().map(|s| match &s.selection {
            Selection::Cursor(c) => *c,
            Selection::Range(r) => r.end,
        })
    }

    /// Convert to a Vec<Selection> for passing to `edit_text()`.
    #[must_use] pub fn to_selections(&self) -> Vec<Selection> {
        self.selections.iter().map(|s| s.selection).collect()
    }

    /// Update selections from the result of `edit_text()`.
    ///
    /// Preserves existing IDs where possible (by index), assigns new IDs for extras.
    pub fn update_from_edit_result(&mut self, new_selections: &[Selection]) {
        let old_ids: Vec<SelectionId> = self.selections.iter().map(|s| s.id).collect();
        self.selections.clear();
        for (i, sel) in new_selections.iter().enumerate() {
            let id = old_ids.get(i).copied().unwrap_or_else(SelectionId::new);
            self.selections.push(IdentifiedSelection {
                id,
                selection: *sel,
            });
        }
        // IDs are reassigned by index; make sure primary_id still resolves.
        self.ensure_primary_valid();
        // Don't merge here — edit_text already returns correct positions
    }

    /// Set all selections to a single cursor (e.g., on plain click without Ctrl).
    pub fn set_single_cursor(&mut self, cursor: TextCursor) {
        let id = self.selections.last().map_or_else(SelectionId::new, |primary| primary.id);
        self.selections.clear();
        self.selections.push(IdentifiedSelection {
            id,
            selection: Selection::Cursor(cursor),
        });
        self.primary_id = id;
    }

    /// Set all selections to a single range.
    pub fn set_single_range(&mut self, range: SelectionRange) {
        let id = self.selections.last().map_or_else(SelectionId::new, |primary| primary.id);
        self.selections.clear();
        self.selections.push(IdentifiedSelection {
            id,
            selection: Selection::Range(range),
        });
        self.primary_id = id;
    }

    /// Number of active cursors/selections.
    #[must_use] pub const fn len(&self) -> usize {
        self.selections.len()
    }

    /// Whether there are no selections (should not normally happen).
    #[must_use] pub const fn is_empty(&self) -> bool {
        self.selections.is_empty()
    }

    /// Sort selections by position and merge any that overlap.
    pub fn merge_overlapping(&mut self) {
        if self.selections.len() <= 1 {
            return;
        }

        // Capture the primary before sorting/merging reorders and rewrites IDs.
        let primary = self.primary_id;
        let mut new_primary = primary;

        // Sort by the start position of each selection
        self.selections.sort_by(|a, b| {
            let pos_a = selection_start_pos(&a.selection);
            let pos_b = selection_start_pos(&b.selection);
            pos_a.cmp(&pos_b)
        });

        // Merge overlapping: if selection[i+1] starts at or before selection[i] ends,
        // merge them into one range (keeping the later ID as it's more recent).
        let mut merged: Vec<IdentifiedSelection> = Vec::with_capacity(self.selections.len());
        for sel in self.selections.drain(..) {
            if let Some(last) = merged.last_mut() {
                let last_end = selection_end_pos(&last.selection);
                let cur_start = selection_start_pos(&sel.selection);
                if cur_start <= last_end {
                    // Overlap — merge into one range covering both
                    let new_start = selection_start_pos(&last.selection);
                    let cur_end = selection_end_pos(&sel.selection);
                    let new_end = if cur_end > last_end { cur_end } else { last_end };
                    if new_start == new_end {
                        last.selection = Selection::Cursor(new_start);
                    } else {
                        last.selection = Selection::Range(SelectionRange {
                            start: new_start,
                            end: new_end,
                        });
                    }
                    // If either side of the merge was the primary, the merged
                    // selection inherits primary status.
                    if last.id == primary || sel.id == primary {
                        new_primary = sel.id;
                    }
                    // Keep the newer ID (the one being merged in)
                    last.id = sel.id;
                    continue;
                }
            }
            merged.push(sel);
        }
        self.selections = merged;

        // Point primary at a surviving selection (fallback: last element).
        self.primary_id = new_primary;
        self.ensure_primary_valid();
    }

    /// Move all cursors using a movement function. Merges collisions afterward.
    ///
    /// `move_fn` takes a `TextCursor` and returns the new `TextCursor` after movement.
    /// If `extend_selection` is true, the anchor stays and only the focus moves,
    /// creating or extending a range.
    pub fn move_all_cursors(
        &mut self,
        extend_selection: bool,
        move_fn: impl Fn(&TextCursor) -> TextCursor,
    ) {
        for sel in &mut self.selections {
            match &sel.selection {
                Selection::Cursor(c) => {
                    let new_cursor = move_fn(c);
                    if extend_selection {
                        if *c != new_cursor {
                            sel.selection = Selection::Range(SelectionRange {
                                start: *c,
                                end: new_cursor,
                            });
                        }
                    } else {
                        sel.selection = Selection::Cursor(new_cursor);
                    }
                }
                Selection::Range(r) => {
                    if extend_selection {
                        let new_end = move_fn(&r.end);
                        if r.start == new_end {
                            sel.selection = Selection::Cursor(r.start);
                        } else {
                            sel.selection = Selection::Range(SelectionRange {
                                start: r.start,
                                end: new_end,
                            });
                        }
                    } else {
                        // Bare arrow with an active selection collapses the caret
                        // to the selection boundary in the arrow's direction WITHOUT
                        // advancing a character (standard editor behavior). Running
                        // move_fn on the focus and using that as the caret would step
                        // one unit past the edge. We don't get the arrow direction
                        // here, so probe it: apply move_fn to the focus and compare —
                        // a forward move collapses to the max boundary, a backward
                        // move to the min boundary.
                        let (lo, hi) = if r.start <= r.end {
                            (r.start, r.end)
                        } else {
                            (r.end, r.start)
                        };
                        let probe = move_fn(&r.end);
                        let collapsed = if probe >= r.end { hi } else { lo };
                        sel.selection = Selection::Cursor(collapsed);
                    }
                }
            }
        }
        self.merge_overlapping();
    }

    /// Remap the `NodeId` in `node_id` after DOM reconciliation.
    ///
    /// If the node was removed (not in the map), the multi-cursor state is cleared.
    pub fn remap_node_ids(
        &mut self,
        dom_id: DomId,
        node_id_map: &BTreeMap<NodeId, NodeId>,
    ) {
        if self.node_id.dom != dom_id {
            return;
        }
        if let Some(old_node_id) = self.node_id.node.into_crate_internal() {
            if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                self.node_id.node = crate::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(new_node_id));
            } else {
                // Node removed — clear selections
                self.selections.clear();
            }
        }
    }
}

/// Helper: get the start position of a Selection for sorting.
fn selection_start_pos(sel: &Selection) -> TextCursor {
    match sel {
        Selection::Cursor(c) => *c,
        Selection::Range(r) => {
            if r.start <= r.end { r.start } else { r.end }
        }
    }
}

/// Helper: get the end position of a Selection for merging.
fn selection_end_pos(sel: &Selection) -> TextCursor {
    match sel {
        Selection::Cursor(c) => *c,
        Selection::Range(r) => {
            if r.end >= r.start { r.end } else { r.start }
        }
    }
}

// ============================================================================
// MULTI-NODE SELECTION (Browser-style Anchor/Focus model)
// ============================================================================

/// The anchor point of a text selection - where the user started selecting.
///
/// This is the fixed point during a drag operation. It records:
/// - The IFC root node (where the `UnifiedLayout` lives)
/// - The exact cursor position within that layout
/// - The visual bounds of the anchor character (for logical rectangle calculations)
///
/// The anchor remains constant during a drag; only the focus moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionAnchor {
    /// The IFC root node ID where selection started.
    /// This is the node that has `inline_layout_result` (e.g., `<p>`, `<div>`).
    pub ifc_root_node_id: NodeId,
    
    /// The exact cursor position within the IFC's `UnifiedLayout`.
    pub cursor: TextCursor,
    
    /// Visual bounds of the anchor character in viewport coordinates.
    /// Used for computing the logical selection rectangle during multi-line/multi-node selection.
    pub char_bounds: LogicalRect,
    
    /// The mouse position when the selection started (viewport coordinates).
    pub mouse_position: LogicalPosition,
}

/// The focus point of a text selection - where the selection currently ends.
///
/// This is the movable point during a drag operation. It updates on every mouse move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionFocus {
    /// The IFC root node ID where selection currently ends.
    /// May differ from anchor's IFC root during cross-node selection.
    pub ifc_root_node_id: NodeId,
    
    /// The exact cursor position within the IFC's `UnifiedLayout`.
    pub cursor: TextCursor,
    
    /// Current mouse position in viewport coordinates.
    pub mouse_position: LogicalPosition,
}

/// Complete selection state spanning potentially multiple DOM nodes.
///
/// This implements the W3C Selection API model with anchor/focus endpoints.
/// The selection can span multiple IFC roots (e.g., multiple `<p>` elements).
///
/// ## Storage Model
///
/// Uses `BTreeMap<NodeId, SelectionRange>` for O(log N) lookup during rendering.
/// The key is the **IFC root `NodeId`**, and the value is the `SelectionRange` for that IFC.
///
/// ## Example
///
/// ```text
/// <p id="1">Hello [World</p>     <- Anchor in IFC 1, partial selection
/// <p id="2">Complete line</p>    <- InBetween, fully selected
/// <p id="3">Partial] end</p>     <- Focus in IFC 3, partial selection
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSelection {
    /// The DOM this selection belongs to.
    pub dom_id: DomId,
    
    /// The anchor point - where the selection started (fixed during drag).
    pub anchor: SelectionAnchor,
    
    /// The focus point - where the selection currently ends (moves during drag).
    pub focus: SelectionFocus,
    
    /// Map from IFC root `NodeId` to the `SelectionRange` for that IFC.
    /// This allows O(log N) lookup during rendering.
    ///
    /// The `SelectionRange` contains the actual `TextCursor` positions for that IFC,
    /// ready to be passed to `UnifiedLayout::get_selection_rects()`.
    pub affected_nodes: BTreeMap<NodeId, SelectionRange>,
    
    /// Indicates whether anchor comes before focus in document order.
    /// True = forward selection (left-to-right), False = backward selection.
    pub is_forward: bool,
}

impl TextSelection {
    /// Create a new collapsed selection (cursor) at the given position.
    #[must_use] pub fn new_collapsed(
        dom_id: DomId,
        ifc_root_node_id: NodeId,
        cursor: TextCursor,
        char_bounds: LogicalRect,
        mouse_position: LogicalPosition,
    ) -> Self {
        let anchor = SelectionAnchor {
            ifc_root_node_id,
            cursor,
            char_bounds,
            mouse_position,
        };
        
        let focus = SelectionFocus {
            ifc_root_node_id,
            cursor,
            mouse_position,
        };
        
        // For a collapsed selection, the anchor node has a zero-width range
        let mut affected_nodes = BTreeMap::new();
        affected_nodes.insert(ifc_root_node_id, SelectionRange {
            start: cursor,
            end: cursor,
        });
        
        Self {
            dom_id,
            anchor,
            focus,
            affected_nodes,
            is_forward: true, // Direction doesn't matter for collapsed selection
        }
    }
    
    /// Check if this is a collapsed selection (cursor with no range).
    #[must_use] pub fn is_collapsed(&self) -> bool {
        self.anchor.ifc_root_node_id == self.focus.ifc_root_node_id
            && self.anchor.cursor == self.focus.cursor
    }
    
    /// Get the selection range for a specific IFC root node.
    /// Returns `None` if this node is not part of the selection.
    #[must_use] pub fn get_range_for_node(&self, ifc_root_node_id: &NodeId) -> Option<&SelectionRange> {
        self.affected_nodes.get(ifc_root_node_id)
    }

}

impl_option!(
    TextSelection,
    OptionTextSelection,
    copy = false,
    clone = false,
    [Debug, Clone, PartialEq, Eq]
);

#[cfg(test)]
mod audit_tests {
    use super::*;

    fn cursor(byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: byte },
            affinity: CursorAffinity::Leading,
        }
    }

    fn state(byte: u32) -> MultiCursorState {
        MultiCursorState::new_with_cursor(cursor(byte), DomNodeId::ROOT, 0)
    }

    #[test]
    fn primary_tracked_by_id_not_vec_position() {
        let mut mc = state(100);
        // Add a cursor at an EARLIER position; after merge_overlapping's sort it
        // becomes the vector's FIRST element, but it is the primary (last added).
        let b = mc.add_cursor(cursor(0));
        assert_eq!(mc.len(), 2);
        // The primary must be the just-added cursor at byte 0, not the
        // position-last cursor at byte 100.
        assert_eq!(mc.get_primary().unwrap().id, b);
        assert_eq!(mc.get_primary_cursor().unwrap(), cursor(0));
    }

    #[test]
    fn merge_preserves_primary() {
        let mut mc = state(5);
        let _b = mc.add_cursor(cursor(5)); // same position -> merges to one
        assert_eq!(mc.len(), 1);
        // primary_id must resolve to the surviving selection.
        let primary = mc.get_primary().unwrap();
        assert_eq!(primary.id, mc.selections[0].id);
    }

    #[test]
    fn removing_primary_repoints_it() {
        let mut mc = state(0);
        let b = mc.add_cursor(cursor(10)); // primary = b
        assert_eq!(mc.get_primary().unwrap().id, b);
        assert!(mc.remove_selection(b));
        // primary must now be a still-existing selection, not a dangling id.
        let p = mc.get_primary().unwrap();
        assert!(mc.selections.iter().any(|s| s.id == p.id));
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;
    use crate::geom::LogicalSize;
    use crate::styled_dom::NodeHierarchyItemId;

    // ---------------------------------------------------------------------
    // Fixtures
    // ---------------------------------------------------------------------

    /// Cursor in run 0 at `byte`, Leading affinity.
    fn c(byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    /// Cursor with explicit run + affinity (for ordering / boundary probes).
    fn c_full(run: u32, byte: u32, affinity: CursorAffinity) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run,
                start_byte_in_run: byte,
            },
            affinity,
        }
    }

    fn rng(a: u32, b: u32) -> SelectionRange {
        SelectionRange {
            start: c(a),
            end: c(b),
        }
    }

    fn dom_node(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    fn state(byte: u32) -> MultiCursorState {
        MultiCursorState::new_with_cursor(c(byte), DomNodeId::ROOT, 0)
    }

    /// A `MultiCursorState` with zero selections — "should not normally happen",
    /// so every getter must survive it.
    fn empty_state() -> MultiCursorState {
        MultiCursorState {
            selections: Vec::new(),
            primary_id: SelectionId::new(),
            node_id: DomNodeId::ROOT,
            contenteditable_key: 0,
        }
    }

    fn ident(id: SelectionId, sel: Selection) -> IdentifiedSelection {
        IdentifiedSelection { id, selection: sel }
    }

    // ---------------------------------------------------------------------
    // Invariant checkers (documented in `MultiCursorState`'s `## Invariants`)
    // ---------------------------------------------------------------------

    /// `selections` is sorted by position and non-overlapping.
    fn assert_sorted_nonoverlapping(mc: &MultiCursorState) {
        for w in mc.selections.windows(2) {
            let prev_end = selection_end_pos(&w[0].selection);
            let next_start = selection_start_pos(&w[1].selection);
            assert!(
                next_start > prev_end,
                "selections must be sorted and non-overlapping after merge: {:?} then {:?}",
                w[0],
                w[1]
            );
        }
    }

    /// `primary_id` must always name a selection that actually exists (or the
    /// state must be empty). A dangling `primary_id` makes `get_primary()` lie.
    fn assert_primary_resolves(mc: &MultiCursorState) {
        if mc.is_empty() {
            assert!(mc.get_primary().is_none());
            assert!(mc.get_primary_cursor().is_none());
        } else {
            let p = mc.get_primary().expect("non-empty state must have a primary");
            assert!(
                mc.selections.iter().any(|s| s.id == p.id),
                "get_primary() returned a selection not in the vec"
            );
            assert_eq!(
                mc.primary_id, p.id,
                "primary_id must name an existing selection (not fall back to last)"
            );
        }
    }

    /// All selection IDs must be distinct.
    fn assert_ids_unique(mc: &MultiCursorState) {
        for (i, a) in mc.selections.iter().enumerate() {
            for b in mc.selections.iter().skip(i + 1) {
                assert_ne!(a.id, b.id, "duplicate SelectionId in state");
            }
        }
    }

    // =====================================================================
    // SelectionId::new  (constructor)
    // =====================================================================

    #[test]
    fn selection_id_new_is_unique_and_strictly_increasing() {
        let mut prev = SelectionId::new();
        assert!(prev.inner > 0, "counter starts at 1, never the 0 sentinel");
        for _ in 0..1000 {
            let next = SelectionId::new();
            // Other tests share the global atomic, so ids may skip — but within
            // one thread they must be strictly increasing and never repeat.
            assert!(
                next.inner > prev.inner,
                "SelectionId counter must be strictly monotonic"
            );
            assert_ne!(next, prev);
            prev = next;
        }
    }

    #[test]
    fn selection_id_default_mints_a_fresh_id() {
        // Documented: Default does NOT return a zero/sentinel value.
        let a = SelectionId::default();
        let b = SelectionId::default();
        let d = SelectionId::new();
        assert_ne!(a, b);
        assert_ne!(b, d);
        assert!(a.inner > 0 && b.inner > 0);
    }

    // =====================================================================
    // SelectionState::add
    // =====================================================================

    #[test]
    fn selection_state_add_dedups_identical_cursors() {
        let mut st = SelectionState {
            selections: Vec::<Selection>::new().into(),
            node_id: DomNodeId::ROOT,
        };
        for _ in 0..100 {
            st.add(Selection::Cursor(c(42)));
        }
        assert_eq!(st.selections.as_ref().len(), 1);
        assert_eq!(st.selections.as_ref()[0], Selection::Cursor(c(42)));
    }

    #[test]
    fn selection_state_add_sorts_descending_input_ascending() {
        let mut st = SelectionState {
            selections: Vec::<Selection>::new().into(),
            node_id: DomNodeId::ROOT,
        };
        for byte in [90u32, 10, 50, 0, 70] {
            st.add(Selection::Cursor(c(byte)));
        }
        let got: Vec<Selection> = st.selections.as_ref().to_vec();
        assert_eq!(got.len(), 5);
        let want: Vec<Selection> = [0u32, 10, 50, 70, 90]
            .iter()
            .map(|b| Selection::Cursor(c(*b)))
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn selection_state_add_boundary_and_reversed_ranges_do_not_panic() {
        let mut st = SelectionState {
            selections: Vec::<Selection>::new().into(),
            node_id: DomNodeId::ROOT,
        };
        // u32::MAX bytes, max run index, both affinities, and a *reversed* range
        // (start logically after end — explicitly allowed by SelectionRange docs).
        st.add(Selection::Cursor(c_full(
            u32::MAX,
            u32::MAX,
            CursorAffinity::Trailing,
        )));
        st.add(Selection::Cursor(c_full(0, 0, CursorAffinity::Leading)));
        st.add(Selection::Range(SelectionRange {
            start: c_full(u32::MAX, u32::MAX, CursorAffinity::Trailing),
            end: c_full(0, 0, CursorAffinity::Leading),
        }));
        st.add(Selection::Range(rng(0, u32::MAX)));
        // add() only sorts + dedups; it does not normalize or merge, so all 4 stay.
        assert_eq!(st.selections.as_ref().len(), 4);
        // ... and the result is sorted.
        let got: Vec<Selection> = st.selections.as_ref().to_vec();
        let mut sorted = got.clone();
        sorted.sort_unstable();
        assert_eq!(got, sorted);
    }

    #[test]
    fn selection_state_add_cursor_and_range_at_same_pos_are_distinct() {
        let mut st = SelectionState {
            selections: Vec::<Selection>::new().into(),
            node_id: DomNodeId::ROOT,
        };
        st.add(Selection::Range(rng(5, 5)));
        st.add(Selection::Cursor(c(5)));
        // A zero-width Range and a Cursor are different `Selection` variants,
        // so dedup() cannot collapse them.
        assert_eq!(st.selections.as_ref().len(), 2);
        // Cursor variant sorts before Range variant.
        assert_eq!(st.selections.as_ref()[0], Selection::Cursor(c(5)));
    }

    // =====================================================================
    // MultiCursorState::new_with_cursor  (constructor)
    // =====================================================================

    #[test]
    fn new_with_cursor_invariants_hold() {
        let node = dom_node(7);
        let mc = MultiCursorState::new_with_cursor(c(3), node, 0xDEAD_BEEF);
        assert_eq!(mc.len(), 1);
        assert!(!mc.is_empty());
        assert_eq!(mc.selections.len(), mc.len());
        assert_eq!(mc.primary_id, mc.selections[0].id);
        assert_eq!(mc.node_id, node);
        assert_eq!(mc.contenteditable_key, 0xDEAD_BEEF);
        assert_eq!(mc.get_primary_cursor(), Some(c(3)));
        assert_eq!(mc.to_selections(), vec![Selection::Cursor(c(3))]);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn new_with_cursor_extreme_args_do_not_panic() {
        let mc = MultiCursorState::new_with_cursor(
            c_full(u32::MAX, u32::MAX, CursorAffinity::Trailing),
            dom_node(usize::MAX / 4),
            u64::MAX,
        );
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.contenteditable_key, u64::MAX);
        assert_eq!(
            mc.get_primary_cursor(),
            Some(c_full(u32::MAX, u32::MAX, CursorAffinity::Trailing))
        );
        assert_primary_resolves(&mc);
    }

    #[test]
    fn two_states_get_distinct_ids() {
        let a = state(0);
        let b = state(0);
        assert_ne!(a.primary_id, b.primary_id);
    }

    // =====================================================================
    // add_cursor / add_selection
    // =====================================================================

    #[test]
    fn add_cursor_at_same_position_merges_to_one() {
        let mut mc = state(5);
        let b = mc.add_cursor(c(5));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(5)));
        assert_eq!(mc.selections[0].id, b, "merge keeps the newer id");
        assert_primary_resolves(&mc);
        assert_ids_unique(&mc);
    }

    #[test]
    fn add_cursor_distinct_positions_stay_separate_and_sorted() {
        let mut mc = state(30);
        let _ = mc.add_cursor(c(10));
        let last = mc.add_cursor(c(20));
        assert_eq!(mc.len(), 3);
        // Sorted by position, NOT by insertion order.
        assert_eq!(mc.to_selections(), vec![
            Selection::Cursor(c(10)),
            Selection::Cursor(c(20)),
            Selection::Cursor(c(30)),
        ]);
        // Primary is the most recently added (byte 20), which is the *middle*
        // element — proving primary is tracked by id, not vec position.
        assert_eq!(mc.get_primary().unwrap().id, last);
        assert_eq!(mc.get_primary_cursor(), Some(c(20)));
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
        assert_ids_unique(&mc);
    }

    #[test]
    fn add_cursor_same_byte_different_affinity_does_not_merge() {
        // Leading < Trailing, so cur_start(Trailing) > last_end(Leading) and the
        // merge condition (`cur_start <= last_end`) is false. Two carets survive
        // at the same byte offset.
        let mut mc = MultiCursorState::new_with_cursor(
            c_full(0, 4, CursorAffinity::Leading),
            DomNodeId::ROOT,
            0,
        );
        let _ = mc.add_cursor(c_full(0, 4, CursorAffinity::Trailing));
        assert_eq!(mc.len(), 2);
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn add_selection_overlapping_ranges_merge_into_union() {
        let mut mc = empty_state();
        let _ = mc.add_selection(rng(0, 10));
        let _ = mc.add_selection(rng(5, 20));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(0, 20)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn add_selection_touching_ranges_merge() {
        // Adjacent (end == start) counts as overlapping: `cur_start <= last_end`.
        let mut mc = empty_state();
        let _ = mc.add_selection(rng(0, 10));
        let _ = mc.add_selection(rng(10, 20));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(0, 20)));
    }

    #[test]
    fn add_selection_disjoint_ranges_stay_separate() {
        let mut mc = empty_state();
        let _ = mc.add_selection(rng(0, 10));
        let _ = mc.add_selection(rng(11, 20));
        assert_eq!(mc.len(), 2);
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn add_selection_reversed_range_is_normalized_for_merging() {
        // Backwards selection: start (20) is logically after end (5).
        let mut mc = empty_state();
        let _ = mc.add_selection(SelectionRange {
            start: c(20),
            end: c(5),
        });
        // A cursor *inside* the backwards range must merge with it.
        let _ = mc.add_cursor(c(10));
        assert_eq!(mc.len(), 1);
        // The merged result is normalized to a forwards range.
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(5, 20)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn add_selection_at_u32_max_boundary_does_not_overflow() {
        let mut mc = empty_state();
        let _ = mc.add_selection(rng(u32::MAX - 1, u32::MAX));
        let _ = mc.add_cursor(c(u32::MAX));
        assert_eq!(mc.len(), 1);
        assert_eq!(
            mc.selections[0].selection,
            Selection::Range(rng(u32::MAX - 1, u32::MAX))
        );
        assert_primary_resolves(&mc);
    }

    #[test]
    fn add_cursor_stress_500_distinct_positions() {
        let mut mc = state(0);
        for i in 1..=500u32 {
            let _ = mc.add_cursor(c(i * 2));
        }
        assert_eq!(mc.len(), 501);
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
        assert_ids_unique(&mc);
    }

    #[test]
    fn add_cursor_stress_same_position_never_grows() {
        let mut mc = state(9);
        for _ in 0..300 {
            let _ = mc.add_cursor(c(9));
        }
        assert_eq!(mc.len(), 1, "identical cursors must always collapse");
        assert_primary_resolves(&mc);
    }

    // =====================================================================
    // remove_selection
    // =====================================================================

    #[test]
    fn remove_selection_unknown_id_returns_false_and_changes_nothing() {
        let mut mc = state(1);
        let before = mc.clone();
        let ghost = SelectionId::new(); // never inserted anywhere
        assert!(!mc.remove_selection(ghost));
        assert_eq!(mc, before);
    }

    #[test]
    fn remove_selection_twice_second_call_returns_false() {
        let mut mc = state(0);
        let b = mc.add_cursor(c(10));
        assert!(mc.remove_selection(b));
        assert!(!mc.remove_selection(b));
        assert_eq!(mc.len(), 1);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn remove_all_selections_leaves_a_safe_empty_state() {
        let mut mc = state(0);
        let b = mc.add_cursor(c(10));
        let a = mc.selections.iter().find(|s| s.id != b).unwrap().id;
        assert!(mc.remove_selection(a));
        assert!(mc.remove_selection(b));
        assert!(mc.is_empty());
        assert_eq!(mc.len(), 0);
        assert!(mc.get_primary().is_none());
        assert!(mc.get_primary_cursor().is_none());
        assert!(mc.to_selections().is_empty());
        // Further mutation of the empty state must not panic.
        mc.merge_overlapping();
        mc.move_all_cursors(true, |cur| *cur);
        assert!(mc.is_empty());
    }

    #[test]
    fn remove_primary_from_three_repoints_to_a_survivor() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let p = mc.add_cursor(c(20)); // primary
        assert_eq!(mc.get_primary().unwrap().id, p);
        assert!(mc.remove_selection(p));
        assert_eq!(mc.len(), 2);
        assert_primary_resolves(&mc);
    }

    // =====================================================================
    // get_primary / get_primary_mut / get_primary_cursor / to_selections / len
    // =====================================================================

    #[test]
    fn empty_state_getters_return_none_without_panicking() {
        let mut mc = empty_state();
        assert!(mc.is_empty());
        assert_eq!(mc.len(), 0);
        assert!(mc.get_primary().is_none());
        assert!(mc.get_primary_mut().is_none());
        assert!(mc.get_primary_cursor().is_none());
        assert!(mc.to_selections().is_empty());
        mc.merge_overlapping(); // early-returns on len <= 1
        mc.ensure_primary_valid(); // private: must not panic on empty vec
        assert!(mc.is_empty());
    }

    #[test]
    fn get_primary_falls_back_to_last_when_primary_id_is_dangling() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        mc.primary_id = SelectionId::new(); // dangling: names nothing in the vec
        let p = mc.get_primary().expect("must fall back, not return None");
        assert_eq!(p.id, mc.selections.last().unwrap().id);
        assert_eq!(mc.get_primary_cursor(), Some(c(10)));
    }

    #[test]
    fn ensure_primary_valid_adopts_last_id_when_dangling() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let dangling = SelectionId::new();
        mc.primary_id = dangling;
        mc.ensure_primary_valid();
        assert_ne!(mc.primary_id, dangling);
        assert_eq!(mc.primary_id, mc.selections.last().unwrap().id);
        // Idempotent: a second call on a now-valid id is a no-op.
        let fixed = mc.primary_id;
        mc.ensure_primary_valid();
        assert_eq!(mc.primary_id, fixed);
    }

    #[test]
    fn ensure_primary_valid_on_empty_leaves_id_untouched() {
        let mut mc = empty_state();
        let before = mc.primary_id;
        mc.ensure_primary_valid();
        assert_eq!(mc.primary_id, before, "nothing to adopt — id must not change");
        assert!(mc.get_primary().is_none());
    }

    #[test]
    fn get_primary_cursor_of_a_range_is_its_end_field() {
        let mut mc = empty_state();
        mc.set_single_range(rng(3, 9));
        assert_eq!(mc.get_primary_cursor(), Some(c(9)));

        // Backwards range: the raw `end` field is returned (the *focus*), even
        // though it is the lower position. This is deliberate — the caret sits
        // at the focus, not at the max boundary.
        let mut back = empty_state();
        back.set_single_range(SelectionRange {
            start: c(9),
            end: c(3),
        });
        assert_eq!(back.get_primary_cursor(), Some(c(3)));
    }

    #[test]
    fn get_primary_mut_mutation_is_visible_through_get_primary() {
        let mut mc = state(0);
        let p = mc.add_cursor(c(50));
        {
            let prim = mc.get_primary_mut().expect("primary exists");
            assert_eq!(prim.id, p);
            prim.selection = Selection::Range(rng(50, 60));
        }
        assert_eq!(
            mc.get_primary().unwrap().selection,
            Selection::Range(rng(50, 60))
        );
        assert_eq!(mc.get_primary_cursor(), Some(c(60)));
    }

    #[test]
    fn get_primary_mut_falls_back_to_last_when_dangling() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        mc.primary_id = SelectionId::new();
        let last_id = mc.selections.last().unwrap().id;
        let prim = mc.get_primary_mut().expect("fallback to last");
        assert_eq!(prim.id, last_id);
    }

    #[test]
    fn to_selections_matches_the_internal_order_and_len() {
        let mut mc = state(30);
        let _ = mc.add_cursor(c(10));
        let _ = mc.add_selection(rng(15, 20));
        let sels = mc.to_selections();
        assert_eq!(sels.len(), mc.len());
        let inner: Vec<Selection> = mc.selections.iter().map(|s| s.selection).collect();
        assert_eq!(sels, inner);
    }

    #[test]
    fn len_and_is_empty_always_agree() {
        let mut mc = empty_state();
        assert!(mc.is_empty() && mc.len() == 0);
        let _ = mc.add_cursor(c(1));
        assert!(!mc.is_empty() && mc.len() == 1);
        for i in 2..20u32 {
            let _ = mc.add_cursor(c(i * 3));
        }
        assert_eq!(mc.len(), mc.selections.len());
        assert_eq!(mc.is_empty(), mc.len() == 0);
        assert!(!mc.is_empty());
    }

    // =====================================================================
    // update_from_edit_result
    // =====================================================================

    #[test]
    fn update_from_edit_result_with_empty_slice_clears_everything() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        mc.update_from_edit_result(&[]);
        assert!(mc.is_empty());
        assert!(mc.get_primary().is_none());
        assert!(mc.get_primary_cursor().is_none());
    }

    #[test]
    fn update_from_edit_result_preserves_ids_by_index_and_mints_extras() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let old: Vec<SelectionId> = mc.selections.iter().map(|s| s.id).collect();
        assert_eq!(old.len(), 2);

        mc.update_from_edit_result(&[
            Selection::Cursor(c(1)),
            Selection::Cursor(c(2)),
            Selection::Cursor(c(3)),
            Selection::Range(rng(4, 8)),
        ]);
        assert_eq!(mc.len(), 4);
        assert_eq!(mc.selections[0].id, old[0], "id preserved by index");
        assert_eq!(mc.selections[1].id, old[1], "id preserved by index");
        assert_ids_unique(&mc);
        assert_primary_resolves(&mc);
        assert_eq!(mc.selections[3].selection, Selection::Range(rng(4, 8)));
    }

    #[test]
    fn update_from_edit_result_shrinking_keeps_primary_resolvable() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let _ = mc.add_cursor(c(20)); // primary = the byte-20 cursor
        mc.update_from_edit_result(&[Selection::Cursor(c(99))]);
        assert_eq!(mc.len(), 1);
        // The primary's id is gone (only index 0's id survived), so
        // ensure_primary_valid must have re-pointed it at the survivor.
        assert_primary_resolves(&mc);
        assert_eq!(mc.get_primary_cursor(), Some(c(99)));
    }

    #[test]
    fn update_from_edit_result_does_not_merge_overlaps() {
        // Documented: "Don't merge here — edit_text already returns correct positions"
        let mut mc = state(0);
        mc.update_from_edit_result(&[
            Selection::Range(rng(0, 10)),
            Selection::Range(rng(5, 15)),
        ]);
        assert_eq!(mc.len(), 2, "update must NOT merge");
        // ... but an explicit merge afterwards collapses them.
        mc.merge_overlapping();
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(0, 15)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn update_from_edit_result_with_1000_selections() {
        let mut mc = state(0);
        let big: Vec<Selection> = (0..1000u32).map(|i| Selection::Cursor(c(i * 4))).collect();
        mc.update_from_edit_result(&big);
        assert_eq!(mc.len(), 1000);
        assert_ids_unique(&mc);
        assert_primary_resolves(&mc);
        assert_eq!(mc.to_selections(), big);
    }

    // =====================================================================
    // set_single_cursor / set_single_range
    // =====================================================================

    #[test]
    fn set_single_cursor_collapses_all_selections() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let _ = mc.add_selection(rng(20, 30));
        mc.set_single_cursor(c(7));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(7)));
        assert_primary_resolves(&mc);
        assert_eq!(mc.get_primary_cursor(), Some(c(7)));
    }

    #[test]
    fn set_single_range_collapses_all_selections() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        mc.set_single_range(rng(u32::MAX - 2, u32::MAX));
        assert_eq!(mc.len(), 1);
        assert_eq!(
            mc.selections[0].selection,
            Selection::Range(rng(u32::MAX - 2, u32::MAX))
        );
        assert_primary_resolves(&mc);
    }

    #[test]
    fn set_single_cursor_on_empty_state_mints_a_fresh_id() {
        let mut mc = empty_state();
        let stale = mc.primary_id;
        mc.set_single_cursor(c(1));
        assert_eq!(mc.len(), 1);
        assert_ne!(mc.primary_id, stale, "no last element -> a new id is minted");
        assert_primary_resolves(&mc);
    }

    #[test]
    fn set_single_range_on_empty_state_mints_a_fresh_id() {
        let mut mc = empty_state();
        mc.set_single_range(rng(0, 0));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(0, 0)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn set_single_cursor_is_idempotent() {
        let mut mc = state(0);
        mc.set_single_cursor(c(5));
        let first = mc.clone();
        mc.set_single_cursor(c(5));
        assert_eq!(mc, first, "re-setting the same cursor must reuse the id");
    }

    // =====================================================================
    // merge_overlapping
    // =====================================================================

    #[test]
    fn merge_overlapping_on_empty_and_single_is_a_noop() {
        let mut e = empty_state();
        e.merge_overlapping();
        assert!(e.is_empty());

        let mut one = state(3);
        let before = one.clone();
        one.merge_overlapping();
        assert_eq!(one, before);
    }

    #[test]
    fn merge_overlapping_collapses_a_whole_chain() {
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..4).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Range(rng(25, 40))),
            ident(ids[1], Selection::Range(rng(0, 10))),
            ident(ids[2], Selection::Range(rng(12, 30))),
            ident(ids[3], Selection::Range(rng(5, 15))),
        ];
        mc.primary_id = ids[3];
        mc.merge_overlapping();
        // 0..10 ∪ 5..15 ∪ 12..30 ∪ 25..40 = one contiguous 0..40
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(0, 40)));
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn merge_overlapping_keeps_disjoint_selections_and_sorts_them() {
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..3).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Cursor(c(100))),
            ident(ids[1], Selection::Range(rng(0, 5))),
            ident(ids[2], Selection::Cursor(c(50))),
        ];
        mc.primary_id = ids[0];
        mc.merge_overlapping();
        assert_eq!(mc.len(), 3);
        assert_eq!(mc.to_selections(), vec![
            Selection::Range(rng(0, 5)),
            Selection::Cursor(c(50)),
            Selection::Cursor(c(100)),
        ]);
        assert_sorted_nonoverlapping(&mc);
        // The primary (byte 100) survived the sort untouched.
        assert_eq!(mc.primary_id, ids[0]);
        assert_eq!(mc.get_primary_cursor(), Some(c(100)));
    }

    #[test]
    fn merge_overlapping_zero_width_merge_yields_a_cursor_not_a_range() {
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..2).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Cursor(c(8))),
            ident(ids[1], Selection::Range(rng(8, 8))),
        ];
        mc.primary_id = ids[1];
        mc.merge_overlapping();
        assert_eq!(mc.len(), 1);
        // new_start == new_end -> collapses back to a Cursor.
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(8)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn merge_overlapping_is_idempotent() {
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..5).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Range(rng(0, 10))),
            ident(ids[1], Selection::Cursor(c(5))),
            ident(ids[2], Selection::Range(rng(30, 20))), // reversed
            ident(ids[3], Selection::Cursor(c(100))),
            ident(ids[4], Selection::Range(rng(99, 101))),
        ];
        mc.primary_id = ids[2];
        mc.merge_overlapping();
        let once = mc.clone();
        mc.merge_overlapping();
        assert_eq!(mc, once, "merge_overlapping must be a fixed point");
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn merge_overlapping_adversarial_200_selections_keeps_invariants() {
        let mut mc = empty_state();
        let mut seed: u32 = 0x1234_5678;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            seed
        };
        let mut sels = Vec::new();
        for i in 0..200u32 {
            let a = next() % 1000;
            let b = next() % 1000;
            let sel = match i % 4 {
                0 => Selection::Cursor(c(a)),
                1 => Selection::Range(SelectionRange {
                    start: c(a),
                    end: c(b),
                }), // may be reversed
                2 => Selection::Range(rng(a.min(b), a.max(b))),
                _ => Selection::Cursor(c_full(
                    0,
                    a,
                    if b % 2 == 0 {
                        CursorAffinity::Leading
                    } else {
                        CursorAffinity::Trailing
                    },
                )),
            };
            sels.push(ident(SelectionId::new(), sel));
        }
        // Throw in the absolute boundaries too.
        sels.push(ident(SelectionId::new(), Selection::Cursor(c(0))));
        sels.push(ident(SelectionId::new(), Selection::Cursor(c(u32::MAX))));
        sels.push(ident(
            SelectionId::new(),
            Selection::Range(rng(u32::MAX - 1, u32::MAX)),
        ));
        mc.primary_id = sels[7].id;
        mc.selections = sels;

        mc.merge_overlapping();

        assert!(!mc.is_empty());
        assert!(mc.len() <= 203);
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
        assert_ids_unique(&mc);
    }

    #[test]
    fn merge_overlapping_primary_inside_a_chain_still_resolves() {
        // Three cursors that all collapse into one, plus a far-away cursor.
        // The primary is the *first* link of the merge chain.
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..4).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Cursor(c(0))),
            ident(ids[1], Selection::Cursor(c(0))),
            ident(ids[2], Selection::Cursor(c(0))),
            ident(ids[3], Selection::Cursor(c(100))),
        ];
        mc.primary_id = ids[0];
        mc.merge_overlapping();

        assert_eq!(mc.len(), 2);
        // Whatever the merge does with ids, `primary_id` must never dangle.
        assert!(
            mc.selections.iter().any(|s| s.id == mc.primary_id),
            "primary_id must name a surviving selection"
        );
        assert!(mc.get_primary().is_some());
    }

    #[test]
    #[ignore = "known bug: 3+-link merge chain loses the primary; see report"]
    fn merge_overlapping_primary_should_follow_its_merge_chain() {
        // Same setup as above. `merge_overlapping` records `new_primary = sel.id`
        // when the chain's head is the primary, but the head's id is then
        // overwritten by the *next* merge, so `new_primary` points at an id that
        // no longer exists. ensure_primary_valid() then silently adopts the
        // vector's LAST element — the unrelated cursor at byte 100.
        //
        // Expected: the primary follows the merged selection it was part of (byte 0).
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..4).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Cursor(c(0))),
            ident(ids[1], Selection::Cursor(c(0))),
            ident(ids[2], Selection::Cursor(c(0))),
            ident(ids[3], Selection::Cursor(c(100))),
        ];
        mc.primary_id = ids[0];
        mc.merge_overlapping();
        assert_eq!(
            mc.get_primary_cursor(),
            Some(c(0)),
            "primary jumped to an unrelated selection after the merge"
        );
    }

    // =====================================================================
    // move_all_cursors
    // =====================================================================

    #[test]
    fn move_all_cursors_identity_leaves_positions_unchanged() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let _ = mc.add_cursor(c(20));
        let before = mc.to_selections();
        mc.move_all_cursors(false, |cur| *cur);
        assert_eq!(mc.to_selections(), before);
        assert_eq!(mc.len(), 3);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn move_all_cursors_extend_with_no_movement_keeps_a_cursor() {
        // `*c != new_cursor` is false -> the selection must stay a Cursor,
        // not degenerate into a zero-width Range.
        let mut mc = state(4);
        mc.move_all_cursors(true, |cur| *cur);
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(4)));
    }

    #[test]
    fn move_all_cursors_extend_turns_a_cursor_into_a_range() {
        let mut mc = state(10);
        mc.move_all_cursors(true, |cur| c(cur.cluster_id.start_byte_in_run + 5));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(10, 15)));
        // The anchor stayed at 10, only the focus moved.
        assert_eq!(mc.get_primary_cursor(), Some(c(15)));
    }

    #[test]
    fn move_all_cursors_bare_forward_arrow_collapses_range_to_max_boundary() {
        let mut mc = empty_state();
        mc.set_single_range(rng(3, 9));
        mc.move_all_cursors(false, |cur| c(cur.cluster_id.start_byte_in_run + 1));
        assert_eq!(mc.len(), 1);
        // Collapses to the boundary WITHOUT stepping past it (not byte 10).
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(9)));
    }

    #[test]
    fn move_all_cursors_bare_backward_arrow_collapses_range_to_min_boundary() {
        let mut mc = empty_state();
        mc.set_single_range(rng(3, 9));
        mc.move_all_cursors(false, |cur| {
            c(cur.cluster_id.start_byte_in_run.saturating_sub(1))
        });
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(3)));
    }

    #[test]
    fn move_all_cursors_collapses_a_backwards_range_by_direction_not_field_order() {
        // Backwards range (focus at 3, anchor at 9): a forward arrow must still
        // collapse to the max boundary (9), not to the `end` field (3).
        let mut mc = empty_state();
        mc.set_single_range(SelectionRange {
            start: c(9),
            end: c(3),
        });
        mc.move_all_cursors(false, |cur| c(cur.cluster_id.start_byte_in_run + 1));
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(9)));

        let mut back = empty_state();
        back.set_single_range(SelectionRange {
            start: c(9),
            end: c(3),
        });
        back.move_all_cursors(false, |cur| {
            c(cur.cluster_id.start_byte_in_run.saturating_sub(1))
        });
        assert_eq!(back.selections[0].selection, Selection::Cursor(c(3)));
    }

    #[test]
    fn move_all_cursors_extend_back_onto_the_anchor_collapses_to_a_cursor() {
        let mut mc = empty_state();
        mc.set_single_range(rng(3, 4));
        // Shrink the focus back onto the anchor: r.start == new_end.
        mc.move_all_cursors(true, |_| c(3));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(3)));
    }

    #[test]
    fn move_all_cursors_constant_move_fn_merges_everything_into_one() {
        let mut mc = state(0);
        for i in 1..5u32 {
            let _ = mc.add_cursor(c(i * 10));
        }
        assert_eq!(mc.len(), 5);
        mc.move_all_cursors(false, |_| c(7));
        assert_eq!(mc.len(), 1, "colliding cursors must be merged afterwards");
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(7)));
        assert_primary_resolves(&mc);
        assert_sorted_nonoverlapping(&mc);
    }

    #[test]
    fn move_all_cursors_saturating_at_u32_max_does_not_overflow() {
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..2).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Cursor(c(u32::MAX - 1))),
            ident(ids[1], Selection::Cursor(c(u32::MAX))),
        ];
        mc.primary_id = ids[1];
        mc.move_all_cursors(false, |cur| {
            c(cur.cluster_id.start_byte_in_run.saturating_add(1))
        });
        // Both saturate to u32::MAX and merge.
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(u32::MAX)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn move_all_cursors_on_empty_state_does_not_panic() {
        let mut mc = empty_state();
        mc.move_all_cursors(false, |cur| *cur);
        mc.move_all_cursors(true, |_| c(u32::MAX));
        assert!(mc.is_empty());
    }

    #[test]
    fn move_all_cursors_stress_keeps_invariants() {
        let mut mc = state(0);
        for i in 1..100u32 {
            let _ = mc.add_cursor(c(i * 5));
        }
        for _ in 0..10 {
            mc.move_all_cursors(false, |cur| {
                // Fold every cursor into a small window -> heavy merging.
                c(cur.cluster_id.start_byte_in_run % 7)
            });
            assert_sorted_nonoverlapping(&mc);
            assert_primary_resolves(&mc);
            assert_ids_unique(&mc);
        }
        assert!(mc.len() <= 7);
    }

    // =====================================================================
    // remap_node_ids
    // =====================================================================

    #[test]
    fn remap_node_ids_for_a_different_dom_is_a_noop() {
        let mut mc = MultiCursorState::new_with_cursor(c(1), dom_node(5), 0);
        mc.node_id.dom = DomId { inner: 7 };
        let before = mc.clone();
        let mut map = BTreeMap::new();
        map.insert(NodeId::new(5), NodeId::new(9));
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert_eq!(mc, before, "a foreign DomId must not touch this state");
    }

    #[test]
    fn remap_node_ids_rewrites_a_surviving_node() {
        let mut mc = MultiCursorState::new_with_cursor(c(1), dom_node(5), 0);
        let mut map = BTreeMap::new();
        map.insert(NodeId::new(5), NodeId::new(9));
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert_eq!(
            mc.node_id.node.into_crate_internal(),
            Some(NodeId::new(9))
        );
        assert_eq!(mc.len(), 1, "selections survive a successful remap");
        assert_primary_resolves(&mc);
    }

    #[test]
    fn remap_node_ids_clears_selections_when_the_node_was_removed() {
        let mut mc = MultiCursorState::new_with_cursor(c(1), dom_node(5), 0);
        let _ = mc.add_cursor(c(20));
        let map: BTreeMap<NodeId, NodeId> = BTreeMap::new(); // node 5 is gone
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert!(mc.is_empty(), "a removed node must drop its selections");
        assert!(mc.get_primary().is_none());
        // node_id itself is left alone (only selections are cleared).
        assert_eq!(
            mc.node_id.node.into_crate_internal(),
            Some(NodeId::new(5))
        );
    }

    #[test]
    fn remap_node_ids_with_a_none_node_is_a_noop() {
        // DomNodeId::ROOT carries NodeHierarchyItemId::NONE -> into_crate_internal()
        // is None, so neither branch runs and the selections must survive.
        let mut mc = state(3);
        let map: BTreeMap<NodeId, NodeId> = BTreeMap::new();
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.node_id.node, NodeHierarchyItemId::NONE);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn remap_node_ids_handles_large_node_indices() {
        let big = 1_000_000usize;
        let mut mc = MultiCursorState::new_with_cursor(c(1), dom_node(big), 0);
        let mut map = BTreeMap::new();
        map.insert(NodeId::new(big), NodeId::new(big * 2));
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert_eq!(
            mc.node_id.node.into_crate_internal(),
            Some(NodeId::new(big * 2))
        );
    }

    #[test]
    fn remap_node_ids_twice_is_stable() {
        let mut mc = MultiCursorState::new_with_cursor(c(1), dom_node(5), 0);
        let mut map = BTreeMap::new();
        map.insert(NodeId::new(5), NodeId::new(9));
        map.insert(NodeId::new(9), NodeId::new(9)); // identity for the new id
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert_eq!(
            mc.node_id.node.into_crate_internal(),
            Some(NodeId::new(9))
        );
        assert_eq!(mc.len(), 1);
    }

    // =====================================================================
    // selection_start_pos / selection_end_pos  (private helpers)
    // =====================================================================

    #[test]
    fn selection_pos_helpers_normalize_reversed_ranges() {
        let forward = Selection::Range(rng(3, 9));
        assert_eq!(selection_start_pos(&forward), c(3));
        assert_eq!(selection_end_pos(&forward), c(9));

        let backward = Selection::Range(SelectionRange {
            start: c(9),
            end: c(3),
        });
        assert_eq!(selection_start_pos(&backward), c(3));
        assert_eq!(selection_end_pos(&backward), c(9));

        let cursor = Selection::Cursor(c(5));
        assert_eq!(selection_start_pos(&cursor), c(5));
        assert_eq!(selection_end_pos(&cursor), c(5));
    }

    #[test]
    fn selection_pos_helpers_start_never_exceeds_end() {
        let mut seed: u32 = 0xACE1_BEEF;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            seed
        };
        let extremes = [0u32, 1, u32::MAX - 1, u32::MAX];
        let mut cases: Vec<Selection> = Vec::new();
        for a in extremes {
            for b in extremes {
                cases.push(Selection::Range(SelectionRange {
                    start: c_full(a, b, CursorAffinity::Trailing),
                    end: c_full(b, a, CursorAffinity::Leading),
                }));
                cases.push(Selection::Cursor(c_full(a, b, CursorAffinity::Leading)));
            }
        }
        for _ in 0..200 {
            cases.push(Selection::Range(SelectionRange {
                start: c(next()),
                end: c(next()),
            }));
        }
        for sel in &cases {
            assert!(
                selection_start_pos(sel) <= selection_end_pos(sel),
                "start must never sort after end: {sel:?}"
            );
        }
    }

    #[test]
    fn selection_pos_helpers_respect_affinity_ordering() {
        // Same byte, different affinity: Leading < Trailing.
        let sel = Selection::Range(SelectionRange {
            start: c_full(0, 4, CursorAffinity::Trailing),
            end: c_full(0, 4, CursorAffinity::Leading),
        });
        assert_eq!(
            selection_start_pos(&sel),
            c_full(0, 4, CursorAffinity::Leading)
        );
        assert_eq!(
            selection_end_pos(&sel),
            c_full(0, 4, CursorAffinity::Trailing)
        );
    }

    // =====================================================================
    // TextSelection
    // =====================================================================

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    #[test]
    fn new_collapsed_invariants_hold() {
        let node = NodeId::new(3);
        let sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c(7),
            rect(1.0, 2.0, 3.0, 4.0),
            LogicalPosition::new(5.0, 6.0),
        );
        assert!(sel.is_collapsed());
        assert!(sel.is_forward);
        assert_eq!(sel.dom_id, DomId::ROOT_ID);
        assert_eq!(sel.anchor.ifc_root_node_id, node);
        assert_eq!(sel.focus.ifc_root_node_id, node);
        assert_eq!(sel.anchor.cursor, c(7));
        assert_eq!(sel.focus.cursor, c(7));
        assert_eq!(sel.affected_nodes.len(), 1);
        // The collapsed node maps to a zero-width range at the cursor.
        assert_eq!(
            sel.get_range_for_node(&node),
            Some(&SelectionRange {
                start: c(7),
                end: c(7),
            })
        );
    }

    #[test]
    fn new_collapsed_with_non_finite_geometry_does_not_panic() {
        let node = NodeId::new(0);
        let sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c_full(u32::MAX, u32::MAX, CursorAffinity::Trailing),
            rect(f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX),
            LogicalPosition::new(f32::NAN, f32::NEG_INFINITY),
        );
        // Geometry is carried verbatim; only the cursors decide collapsedness.
        assert!(sel.is_collapsed());
        assert!(sel.get_range_for_node(&node).is_some());
        assert!(sel.anchor.char_bounds.origin.x.is_nan());
    }

    #[test]
    fn get_range_for_node_returns_none_for_an_unaffected_node() {
        let sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            NodeId::new(3),
            c(0),
            rect(0.0, 0.0, 0.0, 0.0),
            LogicalPosition::new(0.0, 0.0),
        );
        assert!(sel.get_range_for_node(&NodeId::new(4)).is_none());
        assert!(sel.get_range_for_node(&NodeId::new(0)).is_none());
        assert!(sel.get_range_for_node(&NodeId::new(usize::MAX)).is_none());
    }

    #[test]
    fn get_range_for_node_on_an_empty_map_returns_none() {
        let node = NodeId::new(3);
        let mut sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c(0),
            rect(0.0, 0.0, 0.0, 0.0),
            LogicalPosition::new(0.0, 0.0),
        );
        sel.affected_nodes.clear();
        assert!(sel.get_range_for_node(&node).is_none());
        assert!(sel.is_collapsed(), "collapsedness does not depend on the map");
    }

    #[test]
    fn is_collapsed_is_false_when_the_focus_cursor_moves() {
        let node = NodeId::new(3);
        let mut sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c(7),
            rect(0.0, 0.0, 1.0, 1.0),
            LogicalPosition::new(0.0, 0.0),
        );
        assert!(sel.is_collapsed());
        sel.focus.cursor = c(8);
        assert!(!sel.is_collapsed());
    }

    #[test]
    fn is_collapsed_is_false_when_the_focus_crosses_into_another_ifc() {
        let mut sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            NodeId::new(3),
            c(7),
            rect(0.0, 0.0, 1.0, 1.0),
            LogicalPosition::new(0.0, 0.0),
        );
        sel.focus.ifc_root_node_id = NodeId::new(4); // same cursor, different node
        assert!(
            !sel.is_collapsed(),
            "same cursor offset in a different IFC is not a collapsed selection"
        );
    }

    #[test]
    fn is_collapsed_only_looks_at_cursors_not_at_mouse_position() {
        let node = NodeId::new(1);
        let mut sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c(2),
            rect(0.0, 0.0, 1.0, 1.0),
            LogicalPosition::new(0.0, 0.0),
        );
        sel.focus.mouse_position = LogicalPosition::new(999.0, -999.0);
        assert!(sel.is_collapsed());
    }
}
