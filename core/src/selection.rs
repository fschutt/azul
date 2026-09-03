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

impl_vec!(
    SelectionRange,
    SelectionRangeVec,
    SelectionRangeVecDestructor,
    SelectionRangeVecDestructorType,
    SelectionRangeVecSlice,
    OptionSelectionRange
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

impl_option!(
    Selection,
    OptionSelection,
    [Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord]
);

impl_vec!(
    Selection,
    SelectionVec,
    SelectionVecDestructor,
    SelectionVecDestructorType,
    SelectionVecSlice,
    OptionSelection
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
        Self {
            inner: COUNTER.fetch_add(1, Ordering::Relaxed),
        }
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

impl_vec!(
    SelectionId,
    SelectionIdVec,
    SelectionIdVecDestructor,
    SelectionIdVecDestructorType,
    SelectionIdVecSlice,
    OptionSelectionId
);
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
    /// WHOSE selection this is (U1).
    ///
    /// [`SelectionOwner::LOCAL`] for the person at this machine, which is
    /// every selection the engine creates itself. An app running a shared
    /// editing session injects the other participants' cursors with their own
    /// owners, and the paint path colours by this rather than by the node's
    /// `caret-color` - one colour per participant is the whole point.
    pub owner: SelectionOwner,
}

impl_option!(
    IdentifiedSelection,
    OptionIdentifiedSelection,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

impl_vec!(
    IdentifiedSelection,
    IdentifiedSelectionVec,
    IdentifiedSelectionVecDestructor,
    IdentifiedSelectionVecDestructorType,
    IdentifiedSelectionVecSlice,
    OptionIdentifiedSelection
);
impl_vec_debug!(IdentifiedSelection, IdentifiedSelectionVec);
impl_vec_clone!(
    IdentifiedSelection,
    IdentifiedSelectionVec,
    IdentifiedSelectionVecDestructor
);
impl_vec_partialeq!(IdentifiedSelection, IdentifiedSelectionVec);

/// WHO a cursor or selection belongs to (U1).
///
/// A 128-bit id rather than an index, because it has to survive a NETWORK: in a
/// shared editing session the participants are decided elsewhere - a server, a
/// CRDT peer id, a user account - and an engine-allocated number could not be
/// agreed on by two machines. [`SelectionId`] is that engine-allocated number
/// and stays local; this is the app's, and the two are separate fields for
/// exactly that reason.
///
/// Split into two `u64`s rather than a `u128` because this crosses the C ABI,
/// where `u128` had no stable layout until recently and still surprises
/// bindings.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct SelectionOwner {
    pub high: u64,
    pub low: u64,
}

impl SelectionOwner {
    /// The person at this machine.
    ///
    /// All-zero, so a `Default` selection is a local one and every existing
    /// construction site keeps meaning what it meant.
    pub const LOCAL: Self = Self { high: 0, low: 0 };

    #[must_use]
    pub const fn new(high: u64, low: u64) -> Self {
        Self { high, low }
    }

    /// Is this the local participant?
    #[must_use]
    pub const fn is_local(self) -> bool {
        self.high == 0 && self.low == 0
    }
}

/// Multi-cursor state for a contenteditable element (Sublime Text style).
///
/// Replaces the split `CursorManager` + `SelectionManager` pattern for text editing.
/// Supports multiple simultaneous cursors/selections, each with a stable ID.
///
/// ## Invariants
///
/// - `selections` is sorted by owner, then by position, and non-overlapping
///   within one owner.
/// - The **primary** selection is identified by the stable `primary_id`, NOT by
///   vector position: `merge_overlapping()` re-sorts `selections` by position,
///   so "last index" is not the most-recently-added cursor.
/// - After any mutation, `merge_overlapping()` is called to maintain invariants.
///
/// ## Who a selection belongs to (U3)
///
/// A selection is identified by an OWNER-SCOPED id: `(owner, id)`. The engine
/// acts on the [`SelectionOwner::LOCAL`] set and on nothing else:
///
/// - the PRIMARY is always local - `get_primary` never answers with a peer's
///   selection, and `primary_id` is re-pointed only at a local one;
/// - the EDIT SET (`to_selections`, what typing / Backspace / paste apply
///   to) is the local set, and `update_from_edit_result` writes back to it
///   alone, leaving every peer's entry untouched;
/// - a plain click (`set_single_cursor` / `set_single_range`) collapses the
///   LOCAL set to one and keeps the peers in view;
/// - cursor movement (`move_all_cursors*`) moves local carets only;
/// - the platform's idea of "the selection" (`selectedTextRange`, the IME's
///   marked range, the Android selection bridge) is the local primary.
///
/// Peers' selections are DISPLAY-ONLY SNAPSHOTS: they enter through
/// `set_owner_selections`, leave through `remove_owner`, and are painted in
/// their owner's colour. They are not shifted by a local edit either - the
/// sync layer that carried the snapshot here is the one that knows how the
/// edit moved the peer's caret, and it replaces the snapshot. Anything that
/// walks `selections` directly and means "what is the user doing" must go
/// through [`Self::local_selections`]; walking the whole list is right only
/// for painting.
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
    #[must_use]
    pub fn new_with_cursor(
        cursor: TextCursor,
        node_id: DomNodeId,
        contenteditable_key: u64,
    ) -> Self {
        let id = SelectionId::new();
        Self {
            selections: vec![IdentifiedSelection {
                id,
                selection: Selection::Cursor(cursor),
                owner: SelectionOwner::LOCAL,
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
            owner: SelectionOwner::LOCAL,
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
            owner: SelectionOwner::LOCAL,
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

    /// The LOCAL participant's selections - the ones the engine edits, moves
    /// and reports (U3). Peers' snapshots are excluded.
    pub fn local_selections(&self) -> impl Iterator<Item = &IdentifiedSelection> {
        self.selections.iter().filter(|s| s.owner.is_local())
    }

    /// Mutable [`Self::local_selections`].
    pub fn local_selections_mut(&mut self) -> impl Iterator<Item = &mut IdentifiedSelection> {
        self.selections.iter_mut().filter(|s| s.owner.is_local())
    }

    /// How many carets the LOCAL user is typing into. This - not [`Self::len`]
    /// - is the count a "one line per cursor" paste or a "multi-cursor mode"
    /// decision wants; a peer's caret is not somewhere the local user types.
    #[must_use]
    pub fn local_len(&self) -> usize {
        self.local_selections().count()
    }

    /// Get the primary selection (the most recently added/set, tracked by
    /// `primary_id` — NOT the vector's last element, which position-sorting
    /// reorders). Falls back to the last LOCAL selection if `primary_id` was
    /// somehow lost - never to a peer's: the list is owner-sorted, so a plain
    /// `last()` was a peer's entry whenever one existed, and everything that
    /// reads "the selection" (IME, the platform selection, copy) would have
    /// been answering with someone else's.
    #[must_use]
    pub fn get_primary(&self) -> Option<&IdentifiedSelection> {
        let pid = self.primary_id;
        self.selections
            .iter()
            .find(|s| s.id == pid && s.owner.is_local())
            .or_else(|| self.local_selections().last())
    }

    /// Get a mutable reference to the primary selection (see `get_primary`).
    pub fn get_primary_mut(&mut self) -> Option<&mut IdentifiedSelection> {
        let pid = self.primary_id;
        if let Some(pos) = self
            .selections
            .iter()
            .position(|s| s.id == pid && s.owner.is_local())
        {
            return self.selections.get_mut(pos);
        }
        self.local_selections_mut().last()
    }

    /// Ensure `primary_id` names a LOCAL selection that still exists; if not,
    /// adopt the last local one (best effort) so `get_primary` stays
    /// meaningful. With no local selection left, `primary_id` is left dangling
    /// on purpose and `get_primary` answers `None` - a peer's caret must not
    /// become "the selection" because the local one went away.
    fn ensure_primary_valid(&mut self) {
        let pid = self.primary_id;
        if !self.selections.iter().any(|s| s.id == pid && s.owner.is_local()) {
            if let Some(last) = self.local_selections().last() {
                self.primary_id = last.id;
            }
        }
    }

    /// Get the primary cursor position (for scroll-into-view, IME, etc.)
    #[must_use]
    pub fn get_primary_cursor(&self) -> Option<TextCursor> {
        self.get_primary().map(|s| match &s.selection {
            Selection::Cursor(c) => *c,
            Selection::Range(r) => r.end,
        })
    }

    /// The EDIT SET: the LOCAL selections, as a `Vec<Selection>` for
    /// `edit_text()`. Peers' carets are excluded (U3) - with them included, a
    /// local keystroke was applied at every peer's caret as well, because
    /// multi-cursor editing inserts at every selection it is handed.
    #[must_use]
    pub fn to_selections(&self) -> Vec<Selection> {
        self.local_selections().map(|s| s.selection).collect()
    }

    /// Update the LOCAL selections from the result of `edit_text()`.
    ///
    /// Preserves existing local IDs where possible (by index among the local
    /// entries), assigns new IDs for extras. Peers' entries are carried over
    /// UNTOUCHED, owner and id included: rebuilding the whole list as local
    /// used to absorb every peer into the local user after the first
    /// keystroke.
    pub fn update_from_edit_result(&mut self, new_selections: &[Selection]) {
        let old_ids: Vec<SelectionId> = self.local_selections().map(|s| s.id).collect();
        let peers: Vec<IdentifiedSelection> = self
            .selections
            .iter()
            .filter(|s| !s.owner.is_local())
            .copied()
            .collect();
        self.selections.clear();
        for (i, sel) in new_selections.iter().enumerate() {
            let id = old_ids.get(i).copied().unwrap_or_else(SelectionId::new);
            self.selections.push(IdentifiedSelection {
                id,
                selection: *sel,
                owner: SelectionOwner::LOCAL,
            });
        }
        // Owner-sorted order: LOCAL is all-zero and sorts first, so the peers
        // go back behind the rebuilt local set.
        self.selections.extend(peers);
        // IDs are reassigned by index; make sure primary_id still resolves.
        self.ensure_primary_valid();
        // Don't merge here — edit_text already returns correct positions
    }

    /// The id a collapsed local set keeps: the local primary's when there is
    /// one, so a caller tracking it sees the same selection continue.
    fn surviving_local_id(&self) -> SelectionId {
        self.get_primary().map_or_else(SelectionId::new, |primary| primary.id)
    }

    /// Collapse the LOCAL selections to a single cursor (a plain click without
    /// Ctrl). Peers' selections stay: a click is not a message to them.
    pub fn set_single_cursor(&mut self, cursor: TextCursor) {
        let id = self.surviving_local_id();
        self.selections.retain(|s| !s.owner.is_local());
        self.selections.insert(
            0,
            IdentifiedSelection {
                id,
                selection: Selection::Cursor(cursor),
                owner: SelectionOwner::LOCAL,
            },
        );
        self.primary_id = id;
    }

    /// Collapse the LOCAL selections to a single range. Peers stay, as above.
    pub fn set_single_range(&mut self, range: SelectionRange) {
        let id = self.surviving_local_id();
        self.selections.retain(|s| !s.owner.is_local());
        self.selections.insert(
            0,
            IdentifiedSelection {
                id,
                selection: Selection::Range(range),
                owner: SelectionOwner::LOCAL,
            },
        );
        self.primary_id = id;
    }

    /// Number of selections of EVERY owner - what is painted. For how many
    /// carets the local user types into, see [`Self::local_len`].
    #[must_use]
    pub const fn len(&self) -> usize {
        self.selections.len()
    }

    /// Whether there are no selections (should not normally happen).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
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

        // BY OWNER FIRST, then by position (U1). Two participants' cursors
        // must never merge: in a shared editing session that would silently
        // delete someone from the document - their caret absorbed into another
        // person's and repainted in that person's colour. Sorting by owner
        // groups each participant's selections so the adjacency check below
        // only ever compares two of the same owner's.
        self.selections.sort_by(|a, b| {
            a.owner.cmp(&b.owner).then_with(|| {
                let pos_a = selection_start_pos(&a.selection);
                let pos_b = selection_start_pos(&b.selection);
                pos_a.cmp(&pos_b)
            })
        });

        // Merge overlapping: if selection[i+1] starts at or before selection[i] ends,
        // merge them into one range (keeping the later ID as it's more recent).
        let mut merged: Vec<IdentifiedSelection> = Vec::with_capacity(self.selections.len());
        for sel in self.selections.drain(..) {
            if let Some(last) = merged.last_mut() {
                let last_end = selection_end_pos(&last.selection);
                let cur_start = selection_start_pos(&sel.selection);
                // SAME OWNER ONLY - see the sort above.
                if last.owner == sel.owner && cur_start <= last_end {
                    // Overlap — merge into one range covering both
                    let new_start = selection_start_pos(&last.selection);
                    let cur_end = selection_end_pos(&sel.selection);
                    let new_end = if cur_end > last_end {
                        cur_end
                    } else {
                        last_end
                    };
                    if new_start == new_end {
                        last.selection = Selection::Cursor(new_start);
                    } else {
                        last.selection = Selection::Range(SelectionRange {
                            start: new_start,
                            end: new_end,
                        });
                    }
                    // If either side of the merge — or the accumulator that has
                    // already absorbed the primary earlier in the chain — was the
                    // primary, the merged selection inherits primary status.
                    // `last.id == new_primary` carries the primary across a 3+-link
                    // chain: without it, `new_primary` would keep pointing at an
                    // intermediate id that the next merge overwrites, and
                    // `ensure_primary_valid` would then adopt an unrelated tail.
                    let inherits_primary =
                        last.id == primary || sel.id == primary || last.id == new_primary;
                    // Keep the newer ID (the one being merged in)
                    last.id = sel.id;
                    if inherits_primary {
                        new_primary = sel.id;
                    }
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

    /// Replace everything ONE participant owns (U1).
    ///
    /// The injection point for a shared editing session: a peer's cursor
    /// arrives over the network, and this makes it the whole of what that peer
    /// has selected. Replacing rather than merging is deliberate - a remote
    /// participant's state is a SNAPSHOT, and adding to it would leave stale
    /// carets behind whenever a message was missed.
    ///
    /// Refuses to touch [`SelectionOwner::LOCAL`]: the local caret is the
    /// engine's, and letting an app overwrite it through this door would make
    /// every text-editing invariant the engine maintains someone else's
    /// problem. Returns `false` in that case.
    pub fn set_owner_selections(
        &mut self,
        owner: SelectionOwner,
        selections: &[Selection],
    ) -> bool {
        if owner.is_local() {
            return false;
        }
        self.selections.retain(|s| s.owner != owner);
        for selection in selections {
            self.selections.push(IdentifiedSelection {
                id: SelectionId::new(),
                selection: *selection,
                owner,
            });
        }
        // NOT `merge_overlapping` here: that call is about keeping the LOCAL
        // caret set sane after a movement, and a remote snapshot is already
        // whatever the peer says it is. Merging it would also renumber ids the
        // caller may be tracking.
        self.ensure_primary_valid();
        true
    }

    /// Forget a participant - they left, or their connection dropped.
    ///
    /// Returns how many selections went. `LOCAL` is refused for the same
    /// reason as above; removing it would leave the document with no caret.
    pub fn remove_owner(&mut self, owner: SelectionOwner) -> usize {
        if owner.is_local() {
            return 0;
        }
        let before = self.selections.len();
        self.selections.retain(|s| s.owner != owner);
        self.ensure_primary_valid();
        before - self.selections.len()
    }

    /// Every participant with a selection right now, `LOCAL` included.
    #[must_use]
    pub fn owners(&self) -> Vec<SelectionOwner> {
        let mut out: Vec<SelectionOwner> = self.selections.iter().map(|s| s.owner).collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Move all cursors using a movement function. Merges collisions afterward.
    ///
    /// `move_fn` takes a `TextCursor` and returns the new `TextCursor` after movement.
    /// If `extend_selection` is true, the anchor stays and only the focus moves,
    /// creating or extending a range.
    ///
    /// A bare (non-extending) move over an active range COLLAPSES to the range
    /// boundary — the arrow-key rule. Use [`Self::move_all_cursors_with`] for
    /// steps where that is wrong (Home/End, document jumps).
    pub fn move_all_cursors(
        &mut self,
        extend_selection: bool,
        move_fn: impl Fn(&TextCursor) -> TextCursor,
    ) {
        self.move_all_cursors_with(extend_selection, true, move_fn);
    }

    /// [`Self::move_all_cursors`], with control over what a bare move does to
    /// an active range.
    ///
    /// `collapse_range_to_boundary` is the arrow-key rule: Left/Right with a
    /// selection put the caret on the selection's edge and go no further.
    /// Every OTHER step — Home/End, Ctrl+Home/End, a visual line, a word — is a
    /// MOVEMENT and must be performed: collapsing them to the nearest edge is
    /// how pressing End with text selected used to leave the caret sitting at
    /// the end of the selection instead of the end of the line.
    pub fn move_all_cursors_with(
        &mut self,
        extend_selection: bool,
        collapse_range_to_boundary: bool,
        move_fn: impl Fn(&TextCursor) -> TextCursor,
    ) {
        // LOCAL carets only (U3): an arrow key is the local user's, and moving
        // a peer's caret with it would show the peer somewhere they are not.
        for sel in self.selections.iter_mut().filter(|s| s.owner.is_local()) {
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
                    } else if collapse_range_to_boundary {
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
                    } else {
                        // Home / End / Ctrl+Home / Ctrl+End / a visual line step:
                        // the caret goes where the step points, measured from the
                        // focus. The boundary collapse above would strand it on
                        // the selection's edge instead.
                        sel.selection = Selection::Cursor(move_fn(&r.end));
                    }
                }
            }
        }
        self.merge_overlapping();
    }

    /// Remap the `NodeId` in `node_id` after DOM reconciliation.
    ///
    /// If the node was removed (not in the map), the multi-cursor state is cleared.
    pub fn remap_node_ids(&mut self, dom_id: DomId, node_id_map: &BTreeMap<NodeId, NodeId>) {
        if self.node_id.dom != dom_id {
            return;
        }
        if let Some(old_node_id) = self.node_id.node.into_crate_internal() {
            if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                self.node_id.node =
                    crate::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(new_node_id));
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
            if r.start <= r.end {
                r.start
            } else {
                r.end
            }
        }
    }
}

/// Helper: get the end position of a Selection for merging.
fn selection_end_pos(sel: &Selection) -> TextCursor {
    match sel {
        Selection::Cursor(c) => *c,
        Selection::Range(r) => {
            if r.end >= r.start {
                r.end
            } else {
                r.start
            }
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
/// Uses `BTreeMap<NodeId, Vec<SelectionRange>>` for O(log N) lookup during rendering.
/// The key is the **IFC root `NodeId`**, and the value is every `SelectionRange`
/// that IFC contributes.
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

    /// Map from IFC root `NodeId` to the `SelectionRange`s for that IFC.
    /// This allows O(log N) lookup during rendering.
    ///
    /// Each `SelectionRange` contains the actual `TextCursor` positions for that IFC,
    /// ready to be passed to `UnifiedLayout::get_selection_rects()`.
    ///
    /// A node carries SEVERAL ranges when a multi-cursor session selects several
    /// occurrences in it (Ctrl+D); the ranges are disjoint and in document order.
    pub affected_nodes: BTreeMap<NodeId, Vec<SelectionRange>>,

    /// OTHER PARTICIPANTS' ranges on the same nodes, with whose they are
    /// (U1-a).
    ///
    /// Separate from `affected_nodes` rather than mixed into it, because the
    /// two are painted differently and mean different things: that one is the
    /// LOCAL user's selection and takes the node's `::selection` colour, while
    /// these take their owner's. Mixing them made a remote participant's range
    /// look like the local user's own, which is worse than not showing it.
    ///
    /// Empty for a single-user app, which is every app until one injects a
    /// remote owner.
    pub remote_ranges: BTreeMap<NodeId, Vec<(SelectionOwner, SelectionRange)>>,

    /// Indicates whether anchor comes before focus in document order.
    /// True = forward selection (left-to-right), False = backward selection.
    pub is_forward: bool,
}

impl TextSelection {
    /// Create a new collapsed selection (cursor) at the given position.
    #[must_use]
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
        affected_nodes.insert(
            ifc_root_node_id,
            vec![SelectionRange {
                start: cursor,
                end: cursor,
            }],
        );

        Self {
            remote_ranges: BTreeMap::new(),
            dom_id,
            anchor,
            focus,
            affected_nodes,
            is_forward: true, // Direction doesn't matter for collapsed selection
        }
    }

    /// Check if this is a collapsed selection (cursor with no range).
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor.ifc_root_node_id == self.focus.ifc_root_node_id
            && self.anchor.cursor == self.focus.cursor
    }

    /// Get the FIRST selection range for a specific IFC root node.
    /// Returns `None` if this node is not part of the selection.
    ///
    /// A multi-range node has more; [`Self::ranges_for_node`] returns all of them.
    #[must_use]
    pub fn get_range_for_node(&self, ifc_root_node_id: &NodeId) -> Option<&SelectionRange> {
        self.affected_nodes
            .get(ifc_root_node_id)
            .and_then(|r| r.first())
    }

    /// Every range this IFC root contributes (empty slice when unaffected).
    #[must_use]
    pub fn ranges_for_node(&self, ifc_root_node_id: &NodeId) -> &[SelectionRange] {
        self.affected_nodes
            .get(ifc_root_node_id)
            .map_or(&[], Vec::as_slice)
    }
}

impl_option!(
    TextSelection,
    OptionTextSelection,
    copy = false,
    clone = false,
    [Debug, Clone, PartialEq, Eq]
);

// ============================================================================
// App-facing document coordinates (the CallbackInfo selection/sync API)
// ============================================================================

/// A position in a node's TEXT CONTENT, app-facing: `text_byte` indexes the
/// flattened text of `node` — the exact string
/// `CallbackInfo::get_node_text_content(node)` returns (overlay-first, so it
/// sees uncommitted typing). The engine resolves cluster ids and affinity
/// BEFORE handing this out: the byte always lies on a grapheme-cluster
/// boundary of that string (a ZWJ emoji family or a decomposed `é` is never
/// split), and is in LOGICAL order — bidi visual reordering does not affect
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct DocumentPosition {
    pub node: DomNodeId,
    pub text_byte: u32,
}

impl_option!(
    DocumentPosition,
    OptionDocumentPosition,
    [Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord]
);

/// A selected byte span `[start_byte, end_byte)` of `node`'s text content,
/// in the coordinates of [`DocumentPosition`]. Always LOGICAL and
/// NORMALIZED: `start_byte <= end_byte` regardless of drag direction or
/// script direction (an RTL selection is still a forward byte span). A
/// cross-block selection yields one span per affected node, in document
/// order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct DocumentSelectionSpan {
    pub node: DomNodeId,
    pub start_byte: u32,
    pub end_byte: u32,
}

impl_option!(
    DocumentSelectionSpan,
    OptionDocumentSelectionSpan,
    [Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord]
);

impl_vec!(
    DocumentSelectionSpan,
    DocumentSelectionSpanVec,
    DocumentSelectionSpanVecDestructor,
    DocumentSelectionSpanVecDestructorType,
    DocumentSelectionSpanVecSlice,
    OptionDocumentSelectionSpan
);
impl_vec_debug!(DocumentSelectionSpan, DocumentSelectionSpanVec);
impl_vec_clone!(
    DocumentSelectionSpan,
    DocumentSelectionSpanVec,
    DocumentSelectionSpanVecDestructor
);
impl_vec_partialeq!(DocumentSelectionSpan, DocumentSelectionSpanVec);
impl_vec_partialord!(DocumentSelectionSpan, DocumentSelectionSpanVec);

/// One un-synced character-level edit: `node`'s effective text is now
/// `text` (revision-stamped). The app folds it into its model and acks the
/// highest revision it saw via `CallbackInfo::mark_text_revision_synced` —
/// the character-path counterpart of the structural DocumentEdit loop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct DocumentTextEdit {
    pub node: DomNodeId,
    pub text: azul_css::corety::AzString,
    pub revision: u64,
}

impl_option!(
    DocumentTextEdit,
    OptionDocumentTextEdit,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord]
);

impl_vec!(
    DocumentTextEdit,
    DocumentTextEditVec,
    DocumentTextEditVecDestructor,
    DocumentTextEditVecDestructorType,
    DocumentTextEditVecSlice,
    OptionDocumentTextEdit
);
impl_vec_debug!(DocumentTextEdit, DocumentTextEditVec);
impl_vec_clone!(DocumentTextEdit, DocumentTextEditVec, DocumentTextEditVecDestructor);
impl_vec_partialeq!(DocumentTextEdit, DocumentTextEditVec);
impl_vec_partialord!(DocumentTextEdit, DocumentTextEditVec);

#[cfg(test)]
#[path = "selection_test.rs"]
mod selection_test;
