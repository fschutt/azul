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

impl_vec!(
    SelectionRange,
    SelectionRangeVec,
    SelectionRangeVecDestructor,
    SelectionRangeVecDestructorType
);
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

impl_vec!(
    Selection,
    SelectionVec,
    SelectionVecDestructor,
    SelectionVecDestructorType
);
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

    /// Clears all selections and replaces them with a single cursor.
    pub fn set_cursor(&mut self, cursor: TextCursor) {
        self.selections = vec![Selection::Cursor(cursor)].into();
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

/// Type of selection for a specific node within a multi-node selection.
///
/// This helps the renderer determine how to highlight each node:
/// - `Anchor`: Selection starts in this node
/// - `Focus`: Selection ends in this node  
/// - `InBetween`: Entire node is selected (between anchor and focus)
/// - `AnchorAndFocus`: Both anchor and focus are in this single node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeSelectionType {
    /// This is the anchor node (selection started here) - partial selection from anchor to end
    Anchor,
    /// This is the focus node (selection ends here) - partial selection from start to focus
    Focus,
    /// This node is between anchor and focus - fully selected
    InBetween,
    /// Anchor and focus are in the same node - partial selection between cursors
    AnchorAndFocus,
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
    
    /// Check if a specific IFC root node is part of this selection.
    pub fn contains_node(&self, ifc_root_node_id: &NodeId) -> bool {
        self.affected_nodes.contains_key(ifc_root_node_id)
    }
    
    /// Get the selection type for a specific node.
    pub fn get_node_selection_type(&self, ifc_root_node_id: &NodeId) -> Option<NodeSelectionType> {
        if !self.affected_nodes.contains_key(ifc_root_node_id) {
            return None;
        }
        
        let is_anchor = *ifc_root_node_id == self.anchor.ifc_root_node_id;
        let is_focus = *ifc_root_node_id == self.focus.ifc_root_node_id;
        
        Some(match (is_anchor, is_focus) {
            (true, true) => NodeSelectionType::AnchorAndFocus,
            (true, false) => NodeSelectionType::Anchor,
            (false, true) => NodeSelectionType::Focus,
            (false, false) => NodeSelectionType::InBetween,
        })
    }
}

impl_option!(
    TextSelection,
    OptionTextSelection,
    copy = false,
    clone = false,
    [Debug, Clone, PartialEq]
);
