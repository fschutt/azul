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

/// Stable identifier for a cursor/selection within a MultiCursorState.
///
/// Uses a monotonic u64 counter (not UUID) so it is `Copy` and C-API friendly.
/// Each SelectionId is unique within the lifetime of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct SelectionId {
    pub inner: u64,
}

impl SelectionId {
    /// Generate a new unique SelectionId.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        SelectionId { inner: COUNTER.fetch_add(1, Ordering::Relaxed) }
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
/// Replaces the split CursorManager + SelectionManager pattern for text editing.
/// Supports multiple simultaneous cursors/selections, each with a stable ID.
///
/// ## Invariants
///
/// - `selections` is sorted by position and non-overlapping.
/// - The **primary** selection is the last one added (highest index).
/// - After any mutation, `merge_overlapping()` is called to maintain invariants.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiCursorState {
    /// Sorted by position, non-overlapping. Primary = last added (highest index).
    pub selections: Vec<IdentifiedSelection>,
    /// The DOM node this multi-cursor state applies to.
    pub node_id: DomNodeId,
    /// Stable key that survives DOM rebuilds (from `calculate_contenteditable_key`).
    pub contenteditable_key: u64,
}

impl MultiCursorState {
    /// Create a new MultiCursorState with a single cursor.
    pub fn new_with_cursor(cursor: TextCursor, node_id: DomNodeId, contenteditable_key: u64) -> Self {
        let id = SelectionId::new();
        Self {
            selections: vec![IdentifiedSelection {
                id,
                selection: Selection::Cursor(cursor),
            }],
            node_id,
            contenteditable_key,
        }
    }

    /// Add a cursor, merging if it overlaps with existing selections.
    /// Returns the SelectionId of the new (or merged) cursor.
    #[must_use]
    pub fn add_cursor(&mut self, cursor: TextCursor) -> SelectionId {
        let id = SelectionId::new();
        self.selections.push(IdentifiedSelection {
            id,
            selection: Selection::Cursor(cursor),
        });
        self.merge_overlapping();
        id
    }

    /// Add a selection range, merging if it overlaps.
    /// Returns the SelectionId of the new (or merged) selection.
    #[must_use]
    pub fn add_selection(&mut self, range: SelectionRange) -> SelectionId {
        let id = SelectionId::new();
        self.selections.push(IdentifiedSelection {
            id,
            selection: Selection::Range(range),
        });
        self.merge_overlapping();
        id
    }

    /// Remove a selection by its stable ID. Returns true if found and removed.
    #[must_use]
    pub fn remove_selection(&mut self, id: SelectionId) -> bool {
        let len_before = self.selections.len();
        self.selections.retain(|s| s.id != id);
        self.selections.len() < len_before
    }

    /// Get the primary selection (last added = highest index).
    pub fn get_primary(&self) -> Option<&IdentifiedSelection> {
        self.selections.last()
    }

    /// Get a mutable reference to the primary selection.
    pub fn get_primary_mut(&mut self) -> Option<&mut IdentifiedSelection> {
        self.selections.last_mut()
    }

    /// Get the primary cursor position (for scroll-into-view, IME, etc.)
    pub fn get_primary_cursor(&self) -> Option<TextCursor> {
        self.get_primary().map(|s| match &s.selection {
            Selection::Cursor(c) => *c,
            Selection::Range(r) => r.end,
        })
    }

    /// Convert to a Vec<Selection> for passing to `edit_text()`.
    pub fn to_selections(&self) -> Vec<Selection> {
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
        // Don't merge here — edit_text already returns correct positions
    }

    /// Set all selections to a single cursor (e.g., on plain click without Ctrl).
    pub fn set_single_cursor(&mut self, cursor: TextCursor) {
        let id = if let Some(primary) = self.selections.last() {
            primary.id
        } else {
            SelectionId::new()
        };
        self.selections.clear();
        self.selections.push(IdentifiedSelection {
            id,
            selection: Selection::Cursor(cursor),
        });
    }

    /// Set all selections to a single range.
    pub fn set_single_range(&mut self, range: SelectionRange) {
        let id = if let Some(primary) = self.selections.last() {
            primary.id
        } else {
            SelectionId::new()
        };
        self.selections.clear();
        self.selections.push(IdentifiedSelection {
            id,
            selection: Selection::Range(range),
        });
    }

    /// Number of active cursors/selections.
    pub fn len(&self) -> usize {
        self.selections.len()
    }

    /// Whether there are no selections (should not normally happen).
    pub fn is_empty(&self) -> bool {
        self.selections.is_empty()
    }

    /// Sort selections by position and merge any that overlap.
    pub fn merge_overlapping(&mut self) {
        if self.selections.len() <= 1 {
            return;
        }

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
                    // Keep the newer ID (the one being merged in)
                    last.id = sel.id;
                    continue;
                }
            }
            merged.push(sel);
        }
        self.selections = merged;
    }

    /// Move all cursors using a movement function. Merges collisions afterward.
    ///
    /// `move_fn` takes a TextCursor and returns the new TextCursor after movement.
    /// If `extend_selection` is true, the anchor stays and only the focus moves,
    /// creating or extending a range.
    pub fn move_all_cursors(
        &mut self,
        extend_selection: bool,
        move_fn: impl Fn(&TextCursor) -> TextCursor,
    ) {
        for sel in self.selections.iter_mut() {
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
                        // Collapse to the moved end
                        let new_cursor = move_fn(&r.end);
                        sel.selection = Selection::Cursor(new_cursor);
                    }
                }
            }
        }
        self.merge_overlapping();
    }

    /// Remap the NodeId in `node_id` after DOM reconciliation.
    ///
    /// If the node was removed (not in the map), the multi-cursor state is cleared.
    pub fn remap_node_ids(
        &mut self,
        dom_id: DomId,
        node_id_map: &alloc::collections::BTreeMap<crate::dom::NodeId, crate::dom::NodeId>,
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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
/// The key is the **IFC root NodeId**, and the value is the `SelectionRange` for that IFC.
///
/// ## Example
///
/// ```text
/// <p id="1">Hello [World</p>     <- Anchor in IFC 1, partial selection
/// <p id="2">Complete line</p>    <- InBetween, fully selected
/// <p id="3">Partial] end</p>     <- Focus in IFC 3, partial selection
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TextSelection {
    /// The DOM this selection belongs to.
    pub dom_id: DomId,
    
    /// The anchor point - where the selection started (fixed during drag).
    pub anchor: SelectionAnchor,
    
    /// The focus point - where the selection currently ends (moves during drag).
    pub focus: SelectionFocus,
    
    /// Map from IFC root NodeId to the SelectionRange for that IFC.
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
    pub fn new_collapsed(
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
        
        TextSelection {
            dom_id,
            anchor,
            focus,
            affected_nodes,
            is_forward: true, // Direction doesn't matter for collapsed selection
        }
    }
    
    /// Check if this is a collapsed selection (cursor with no range).
    pub fn is_collapsed(&self) -> bool {
        self.anchor.ifc_root_node_id == self.focus.ifc_root_node_id
            && self.anchor.cursor == self.focus.cursor
    }
    
    /// Get the selection range for a specific IFC root node.
    /// Returns `None` if this node is not part of the selection.
    pub fn get_range_for_node(&self, ifc_root_node_id: &NodeId) -> Option<&SelectionRange> {
        self.affected_nodes.get(ifc_root_node_id)
    }
    
}

impl_option!(
    TextSelection,
    OptionTextSelection,
    copy = false,
    clone = false,
    [Debug, Clone, PartialEq]
);
