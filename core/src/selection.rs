//! Text selection and cursor positioning for inline content.
//!
//! This module provides data structures for managing text cursors and selection ranges
//! in a bidirectional (Bidi) and line-breaking aware manner. It handles:
//!
//! - **Grapheme cluster identification**: Unicode-aware character boundaries
//! - **Bidi support**: Cursor movement in mixed LTR/RTL text
//! - **Stable positions**: Selection anchors survive layout changes
//! - **Affinity tracking**: Cursor position at leading/trailing edges
//!
//! # Architecture
//!
//! Text positions are represented as:
//! - `ContentIndex`: Logical position in the original inline content array
//! - `GraphemeClusterId`: Stable identifier for a grapheme cluster (survives reordering)
//! - `TextCursor`: Precise cursor location with leading/trailing affinity
//! - `SelectionRange`: Start and end cursors defining a selection
//!
//! # Use Cases
//!
//! - Text editing: Insert/delete at cursor position
//! - Selection rendering: Highlight selected text
//! - Keyboard navigation: Move cursor by grapheme/word/line
//! - Mouse selection: Convert pixel coordinates to text positions
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

use alloc::vec::Vec;

use crate::dom::DomNodeId;

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
