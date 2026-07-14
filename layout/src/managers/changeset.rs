//! Text editing changeset system
//!
//! **STATUS:** The core types (`TextChangeset`, `TextOperation`, `TextOp*` structs) are
//! actively used by `window.rs`, `undo_redo.rs`, `event.rs`, and platform code.
//!
//! The live copy/cut/select-all/delete paths run through `common/event.rs`
//! (`SystemChange::CopyToClipboard`/`CutToClipboard`, `CallbackChange::SetSelectAllRange`,
//! `LayoutWindow::delete_selection`), not through changeset constructors. The earlier
//! `create_*_changeset` helpers were a never-wired parallel implementation (with
//! placeholder `deleted_text`, `CursorPosition::Uninitialized` cursors, and byte±1
//! UTF-8 deletion) and have been removed.
//!
//! ## Architecture
//!
//! This module implements a two-phase changeset system for all text editing operations:
//! 1. **Create changesets** (pre-callback): Analyze what would change, don't mutate yet
//! 2. **Apply changesets** (post-callback): Actually mutate state if !preventDefault
//!
//! This pattern enables:
//! - preventDefault support for ALL operations (not just text input)
//! - Undo/redo stack (record changesets before applying)
//! - Validation (check bounds, permissions before mutation)
//! - Inspection (user callbacks can see planned changes)

use azul_core::{
    dom::DomNodeId,
    selection::{OptionSelectionRange, SelectionRange},
    task::Instant,
    window::CursorPosition,
};
use azul_css::AzString;

use crate::managers::selection::ClipboardContent;

/// Unique identifier for a changeset (for undo/redo)
pub type ChangesetId = usize;

/// A text editing changeset that can be inspected before application
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextChangeset {
    /// Unique ID for undo/redo tracking
    pub id: ChangesetId,
    /// Target DOM node
    pub target: DomNodeId,
    /// The operation to perform
    pub operation: TextOperation,
    /// When this changeset was created
    pub timestamp: Instant,
}

/// Insert text at cursor position
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpInsertText {
    pub text: AzString,
    pub position: CursorPosition,
    pub new_cursor: CursorPosition,
}

/// Delete text in range
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpDeleteText {
    pub range: SelectionRange,
    pub deleted_text: AzString,
    pub new_cursor: CursorPosition,
}

/// Replace text in range with new text
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpReplaceText {
    pub range: SelectionRange,
    pub old_text: AzString,
    pub new_text: AzString,
    pub new_cursor: CursorPosition,
}

/// Set selection to new range
#[derive(Copy, Debug, Clone)]
#[repr(C)]
pub struct TextOpSetSelection {
    pub old_range: OptionSelectionRange,
    pub new_range: SelectionRange,
}

/// Extend selection in a direction
#[derive(Copy, Debug, Clone)]
#[repr(C)]
pub struct TextOpExtendSelection {
    pub old_range: SelectionRange,
    pub new_range: SelectionRange,
    pub direction: SelectionDirection,
}

/// Clear all selections
#[derive(Copy, Debug, Clone)]
#[repr(C)]
pub struct TextOpClearSelection {
    pub old_range: SelectionRange,
}

/// Move cursor to new position
#[derive(Copy, Debug, Clone)]
#[repr(C)]
pub struct TextOpMoveCursor {
    pub old_position: CursorPosition,
    pub new_position: CursorPosition,
    pub movement: CursorMovement,
}

/// Copy selection to clipboard (no text change)
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpCopy {
    pub range: SelectionRange,
    pub content: ClipboardContent,
}

/// Cut selection to clipboard (deletes text)
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpCut {
    pub range: SelectionRange,
    pub content: ClipboardContent,
    pub new_cursor: CursorPosition,
}

/// Paste from clipboard (inserts text)
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TextOpPaste {
    pub content: ClipboardContent,
    pub position: CursorPosition,
    pub new_cursor: CursorPosition,
}

/// Select all text in node
#[derive(Copy, Debug, Clone)]
#[repr(C)]
pub struct TextOpSelectAll {
    pub old_range: OptionSelectionRange,
    pub new_range: SelectionRange,
}

/// Text editing operation (what will change)
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum TextOperation {
    /// Insert text at cursor position
    InsertText(TextOpInsertText),
    /// Delete text in range
    DeleteText(TextOpDeleteText),
    /// Replace text in range with new text
    ReplaceText(TextOpReplaceText),
    /// Set selection to new range
    SetSelection(TextOpSetSelection),
    /// Extend selection in a direction
    ExtendSelection(TextOpExtendSelection),
    /// Clear all selections
    ClearSelection(TextOpClearSelection),
    /// Move cursor to new position
    MoveCursor(TextOpMoveCursor),
    /// Copy selection to clipboard (no text change)
    Copy(TextOpCopy),
    /// Cut selection to clipboard (deletes text)
    Cut(TextOpCut),
    /// Paste from clipboard (inserts text)
    Paste(TextOpPaste),
    /// Select all text in node
    SelectAll(TextOpSelectAll),
}

/// Re-export from events module
pub use azul_core::events::SelectionDirection;

/// Type of cursor movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum CursorMovement {
    /// Move left one character
    Left,
    /// Move right one character
    Right,
    /// Move up one line
    Up,
    /// Move down one line
    Down,
    /// Jump to previous word boundary
    WordLeft,
    /// Jump to next word boundary
    WordRight,
    /// Jump to start of line
    LineStart,
    /// Jump to end of line
    LineEnd,
    /// Jump to start of document
    DocumentStart,
    /// Jump to end of document
    DocumentEnd,
    /// Absolute position (not relative)
    Absolute,
}

impl TextChangeset {
    /// Create a new changeset with unique ID
    pub fn new(target: DomNodeId, operation: TextOperation, timestamp: Instant) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CHANGESET_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

        Self {
            id: CHANGESET_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            target,
            operation,
            timestamp,
        }
    }

    /// Check if this changeset actually mutates text (vs just selection/cursor)
    #[must_use] pub const fn mutates_text(&self) -> bool {
        matches!(
            self.operation,
            TextOperation::InsertText { .. }
                | TextOperation::DeleteText { .. }
                | TextOperation::ReplaceText { .. }
                | TextOperation::Cut { .. }
                | TextOperation::Paste { .. }
        )
    }

    /// Check if this changeset changes selection (including cursor moves)
    #[must_use] pub const fn changes_selection(&self) -> bool {
        matches!(
            self.operation,
            TextOperation::SetSelection { .. }
                | TextOperation::ExtendSelection { .. }
                | TextOperation::ClearSelection { .. }
                | TextOperation::MoveCursor { .. }
                | TextOperation::SelectAll { .. }
        )
    }

    /// Check if this changeset involves clipboard
    #[must_use] pub const fn uses_clipboard(&self) -> bool {
        matches!(
            self.operation,
            TextOperation::Copy { .. } | TextOperation::Cut { .. } | TextOperation::Paste { .. }
        )
    }

    /// Get the target cursor position after this changeset is applied
    #[must_use] pub const fn resulting_cursor_position(&self) -> Option<CursorPosition> {
        match &self.operation {
            TextOperation::InsertText(op) => Some(op.new_cursor),
            TextOperation::DeleteText(op) => Some(op.new_cursor),
            TextOperation::ReplaceText(op) => Some(op.new_cursor),
            TextOperation::Cut(op) => Some(op.new_cursor),
            TextOperation::Paste(op) => Some(op.new_cursor),
            TextOperation::MoveCursor(op) => Some(op.new_position),
            _ => None,
        }
    }

    /// Get the target selection range after this changeset is applied
    #[must_use] pub const fn resulting_selection_range(&self) -> Option<SelectionRange> {
        match &self.operation {
            TextOperation::SetSelection(op) => Some(op.new_range),
            TextOperation::ExtendSelection(op) => Some(op.new_range),
            TextOperation::SelectAll(op) => Some(op.new_range),
            _ => None,
        }
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::{collections::HashSet, thread};

    use azul_core::{
        dom::DomId,
        geom::LogicalPosition,
        selection::{CursorAffinity, GraphemeClusterId, TextCursor},
        styled_dom::NodeHierarchyItemId,
        task::SystemTick,
    };

    use super::*;
    use crate::managers::selection::StyledTextRun;

    // =========================================================================
    // Fixtures
    //
    // `TextChangeset` is a plain data carrier: the constructor stamps a unique
    // id and the five getters are pure classifiers over `TextOperation`. The
    // adversarial surface is therefore (a) the atomic id counter under
    // contention, (b) whether the getters partition the 11 operation variants
    // exactly as documented, and (c) whether extreme payloads (NaN / infinite
    // cursors, u32::MAX cluster ids, huge and non-ASCII strings) survive a
    // round trip through the getters bit-for-bit instead of being normalized.
    // =========================================================================

    fn node(dom: usize, raw: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: dom },
            node: NodeHierarchyItemId::from_raw(raw),
        }
    }

    fn ts(tick: u64) -> Instant {
        Instant::Tick(SystemTick::new(tick))
    }

    fn cur(x: f32, y: f32) -> CursorPosition {
        CursorPosition::InWindow(LogicalPosition::new(x, y))
    }

    fn tc(run: u32, byte: u32, affinity: CursorAffinity) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run,
                start_byte_in_run: byte,
            },
            affinity,
        }
    }

    fn range(start: TextCursor, end: TextCursor) -> SelectionRange {
        SelectionRange { start, end }
    }

    /// A plain zero-to-one-character forward range.
    fn simple_range() -> SelectionRange {
        range(
            tc(0, 0, CursorAffinity::Leading),
            tc(0, 1, CursorAffinity::Trailing),
        )
    }

    /// A range at the numeric ceiling, selected *backwards* (end before start).
    fn extreme_range() -> SelectionRange {
        range(
            tc(u32::MAX, u32::MAX, CursorAffinity::Trailing),
            tc(0, 0, CursorAffinity::Leading),
        )
    }

    fn clip(text: &str) -> ClipboardContent {
        ClipboardContent {
            plain_text: AzString::from(text),
            styled_runs: Vec::<StyledTextRun>::new().into(),
        }
    }

    /// One changeset per `TextOperation` variant, labelled by variant name.
    ///
    /// Deliberately built from extreme payloads so every truth-table test
    /// doubles as a no-panic test on hostile input.
    fn all_ops() -> Vec<(&'static str, TextOperation)> {
        vec![
            (
                "InsertText",
                TextOperation::InsertText(TextOpInsertText {
                    text: AzString::from("a\u{0301}\u{1F600}\u{202E}\0"),
                    position: cur(f32::NAN, f32::NEG_INFINITY),
                    new_cursor: cur(f32::MAX, f32::MIN),
                }),
            ),
            (
                "DeleteText",
                TextOperation::DeleteText(TextOpDeleteText {
                    range: extreme_range(),
                    deleted_text: AzString::from(""),
                    new_cursor: CursorPosition::Uninitialized,
                }),
            ),
            (
                "ReplaceText",
                TextOperation::ReplaceText(TextOpReplaceText {
                    range: simple_range(),
                    old_text: AzString::from("\u{FFFD}"),
                    new_text: AzString::from("\u{10FFFF}"),
                    new_cursor: CursorPosition::OutOfWindow(LogicalPosition::new(-0.0, 0.0)),
                }),
            ),
            (
                "SetSelection",
                TextOperation::SetSelection(TextOpSetSelection {
                    old_range: OptionSelectionRange::None,
                    new_range: extreme_range(),
                }),
            ),
            (
                "ExtendSelection",
                TextOperation::ExtendSelection(TextOpExtendSelection {
                    old_range: simple_range(),
                    new_range: extreme_range(),
                    direction: SelectionDirection::Backward,
                }),
            ),
            (
                "ClearSelection",
                TextOperation::ClearSelection(TextOpClearSelection {
                    old_range: extreme_range(),
                }),
            ),
            (
                "MoveCursor",
                TextOperation::MoveCursor(TextOpMoveCursor {
                    old_position: CursorPosition::Uninitialized,
                    new_position: cur(f32::INFINITY, f32::NAN),
                    movement: CursorMovement::DocumentEnd,
                }),
            ),
            (
                "Copy",
                TextOperation::Copy(TextOpCopy {
                    range: extreme_range(),
                    content: clip(""),
                }),
            ),
            (
                "Cut",
                TextOperation::Cut(TextOpCut {
                    range: extreme_range(),
                    content: clip("\u{1F600}"),
                    new_cursor: cur(0.0, 0.0),
                }),
            ),
            (
                "Paste",
                TextOperation::Paste(TextOpPaste {
                    content: clip("\r\n\t"),
                    position: cur(-1.0e30, 1.0e30),
                    new_cursor: cur(f32::EPSILON, -f32::EPSILON),
                }),
            ),
            (
                "SelectAll",
                TextOperation::SelectAll(TextOpSelectAll {
                    old_range: OptionSelectionRange::Some(simple_range()),
                    new_range: extreme_range(),
                }),
            ),
        ]
    }

    /// Variant name -> (mutates_text, changes_selection, uses_clipboard).
    ///
    /// Transcribed from the doc comments, not from the `matches!` arms, so a
    /// silent reclassification of a variant fails here.
    fn expected_predicates(name: &str) -> (bool, bool, bool) {
        match name {
            "InsertText" | "DeleteText" | "ReplaceText" => (true, false, false),
            "SetSelection" | "ExtendSelection" | "ClearSelection" | "MoveCursor" | "SelectAll" => {
                (false, true, false)
            }
            "Copy" => (false, false, true),
            "Cut" | "Paste" => (true, false, true),
            other => panic!("unclassified TextOperation variant: {other}"),
        }
    }

    fn changeset_for(op: TextOperation) -> TextChangeset {
        TextChangeset::new(node(0, 1), op, ts(0))
    }

    // =========================================================================
    // 1. Constructor
    // =========================================================================

    #[test]
    fn new_preserves_every_argument_verbatim() {
        let target = node(usize::MAX, usize::MAX);
        let timestamp = ts(u64::MAX);
        let op = TextOperation::InsertText(TextOpInsertText {
            text: AzString::from("hello"),
            position: cur(1.0, 2.0),
            new_cursor: cur(3.0, 4.0),
        });

        let cs = TextChangeset::new(target, op, timestamp.clone());

        assert_eq!(cs.target, target, "target must round-trip unchanged");
        assert_eq!(
            cs.timestamp, timestamp,
            "timestamp must round-trip unchanged"
        );
        match &cs.operation {
            TextOperation::InsertText(op) => assert_eq!(op.text.as_str(), "hello"),
            other => panic!("constructor swapped the operation variant: {other:?}"),
        }
    }

    #[test]
    fn new_does_not_panic_on_extreme_arguments() {
        // usize::MAX DomId + 1-based-encoded usize::MAX node id: the constructor
        // must not interpret, decode or index with either.
        let huge_text = "\u{1F600}".repeat(64 * 1024); // 256 KiB of 4-byte chars
        let cs = TextChangeset::new(
            node(usize::MAX, usize::MAX),
            TextOperation::ReplaceText(TextOpReplaceText {
                range: extreme_range(),
                old_text: AzString::from(huge_text.as_str()),
                new_text: AzString::from(""),
                new_cursor: cur(f32::NAN, f32::NAN),
            }),
            ts(u64::MAX),
        );

        assert_eq!(cs.target.dom.inner, usize::MAX);
        assert_eq!(cs.target.node.into_raw(), usize::MAX);
        assert!(cs.mutates_text());
        assert!(cs.resulting_cursor_position().is_some());
        match &cs.operation {
            TextOperation::ReplaceText(op) => {
                assert_eq!(op.old_text.as_str().len(), 256 * 1024);
                assert!(op.new_text.as_str().is_empty());
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn new_assigns_strictly_increasing_unique_ids() {
        let mut ids = Vec::new();
        for i in 0..256_u64 {
            let cs = TextChangeset::new(
                node(0, 1),
                TextOperation::ClearSelection(TextOpClearSelection {
                    old_range: simple_range(),
                }),
                ts(i),
            );
            ids.push(cs.id);
        }

        // Other tests in this binary share the global counter, so only
        // *monotonicity within this sequence* is guaranteed — not `id == i`.
        for w in ids.windows(2) {
            assert!(
                w[1] > w[0],
                "changeset ids must strictly increase: {} then {}",
                w[0],
                w[1]
            );
        }
        let unique: HashSet<ChangesetId> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "changeset ids must be unique");
    }

    #[test]
    fn new_ids_stay_unique_across_threads() {
        // The id comes from a `fetch_add(Relaxed)` on a process-global counter.
        // Relaxed is fine for uniqueness (RMW ops are atomic regardless of
        // ordering) — this pins that down under contention.
        const THREADS: usize = 8;
        const PER_THREAD: usize = 250;

        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                thread::spawn(|| {
                    (0..PER_THREAD)
                        .map(|_| {
                            TextChangeset::new(
                                node(0, 1),
                                TextOperation::Copy(TextOpCopy {
                                    range: simple_range(),
                                    content: clip("x"),
                                }),
                                ts(0),
                            )
                            .id
                        })
                        .collect::<Vec<ChangesetId>>()
                })
            })
            .collect();

        let mut all = Vec::new();
        for h in handles {
            all.extend(h.join().expect("worker thread panicked"));
        }

        let unique: HashSet<ChangesetId> = all.iter().copied().collect();
        assert_eq!(
            unique.len(),
            THREADS * PER_THREAD,
            "concurrent TextChangeset::new handed out duplicate ids"
        );
    }

    #[test]
    fn clone_keeps_the_id_but_new_mints_a_fresh_one() {
        let cs = changeset_for(TextOperation::ClearSelection(TextOpClearSelection {
            old_range: simple_range(),
        }));
        let cloned = cs.clone();
        assert_eq!(cloned.id, cs.id, "Clone must not re-mint the id");

        let fresh = changeset_for(TextOperation::ClearSelection(TextOpClearSelection {
            old_range: simple_range(),
        }));
        assert!(fresh.id > cs.id, "new() must mint a fresh id");
    }

    // =========================================================================
    // 2. Predicate truth table + partition invariants
    // =========================================================================

    #[test]
    fn predicates_match_the_documented_truth_table() {
        for (name, op) in all_ops() {
            let cs = changeset_for(op);
            let got = (
                cs.mutates_text(),
                cs.changes_selection(),
                cs.uses_clipboard(),
            );
            assert_eq!(
                got,
                expected_predicates(name),
                "{name}: (mutates_text, changes_selection, uses_clipboard) mismatch"
            );
        }
    }

    #[test]
    fn all_eleven_variants_are_covered_and_none_is_both_text_and_selection() {
        let ops = all_ops();
        assert_eq!(
            ops.len(),
            11,
            "all_ops() must cover every TextOperation variant"
        );

        for (name, op) in ops {
            let cs = changeset_for(op);

            // Invariant: the two predicates are documented as alternatives
            // ("mutates text (vs just selection/cursor)"), so no variant may
            // claim both.
            assert!(
                !(cs.mutates_text() && cs.changes_selection()),
                "{name} classifies as both a text mutation and a selection change"
            );

            // Invariant: every variant is reachable through at least one
            // predicate — otherwise a caller dispatching on these three getters
            // would silently drop the operation.
            assert!(
                cs.mutates_text() || cs.changes_selection() || cs.uses_clipboard(),
                "{name} is invisible to all three predicates"
            );
        }
    }

    #[test]
    fn predicates_are_pure_and_ignore_target_and_timestamp() {
        for (name, op) in all_ops() {
            let a = TextChangeset::new(node(0, 0), op.clone(), ts(0));
            let b = TextChangeset::new(node(usize::MAX, usize::MAX), op, ts(u64::MAX));

            assert_eq!(a.mutates_text(), b.mutates_text(), "{name}: mutates_text");
            assert_eq!(
                a.changes_selection(),
                b.changes_selection(),
                "{name}: changes_selection"
            );
            assert_eq!(
                a.uses_clipboard(),
                b.uses_clipboard(),
                "{name}: uses_clipboard"
            );

            // Idempotent: repeated calls on the same instance agree.
            assert_eq!(a.mutates_text(), a.mutates_text());
            assert_eq!(a.changes_selection(), a.changes_selection());
            assert_eq!(a.uses_clipboard(), a.uses_clipboard());
        }
    }

    // =========================================================================
    // 3. resulting_cursor_position
    // =========================================================================

    #[test]
    fn resulting_cursor_position_is_some_exactly_for_cursor_moving_ops() {
        for (name, op) in all_ops() {
            let cs = changeset_for(op);
            let expected = matches!(
                name,
                "InsertText" | "DeleteText" | "ReplaceText" | "Cut" | "Paste" | "MoveCursor"
            );
            assert_eq!(
                cs.resulting_cursor_position().is_some(),
                expected,
                "{name}: resulting_cursor_position() presence"
            );

            // Invariant: anything that rewrites the text must say where the
            // cursor lands, otherwise the caller has nowhere to put it.
            if cs.mutates_text() {
                assert!(
                    cs.resulting_cursor_position().is_some(),
                    "{name} mutates text but reports no resulting cursor"
                );
            }
        }
    }

    #[test]
    fn resulting_cursor_position_returns_the_new_cursor_not_the_old_one() {
        let cs = changeset_for(TextOperation::MoveCursor(TextOpMoveCursor {
            old_position: cur(1.0, 1.0),
            new_position: cur(9.0, 9.0),
            movement: CursorMovement::Absolute,
        }));
        assert_eq!(cs.resulting_cursor_position(), Some(cur(9.0, 9.0)));

        let cs = changeset_for(TextOperation::Paste(TextOpPaste {
            content: clip("abc"),
            position: cur(1.0, 1.0),
            new_cursor: cur(4.0, 1.0),
        }));
        assert_eq!(cs.resulting_cursor_position(), Some(cur(4.0, 1.0)));
    }

    #[test]
    fn resulting_cursor_position_preserves_nan_and_infinity_bit_for_bit() {
        // `LogicalPosition`'s PartialEq quantizes (NaN -> i64::MIN, huge -> i64::MAX),
        // so `==` would happily call NaN and f32::MAX "equal" to other values.
        // Compare raw bits instead: the getter must hand back the exact payload
        // it was given, without clamping, canonicalizing NaN, or flipping -0.0.
        let payloads = [
            (f32::NAN, f32::NEG_INFINITY),
            (f32::INFINITY, -0.0),
            (f32::MAX, f32::MIN),
            (f32::MIN_POSITIVE, -f32::MIN_POSITIVE),
        ];

        for (x, y) in payloads {
            let cs = changeset_for(TextOperation::InsertText(TextOpInsertText {
                text: AzString::from("t"),
                position: CursorPosition::Uninitialized,
                new_cursor: cur(x, y),
            }));

            match cs.resulting_cursor_position() {
                Some(CursorPosition::InWindow(p)) => {
                    assert_eq!(p.x.to_bits(), x.to_bits(), "x mangled for ({x}, {y})");
                    assert_eq!(p.y.to_bits(), y.to_bits(), "y mangled for ({x}, {y})");
                }
                other => panic!("expected InWindow cursor, got {other:?}"),
            }
        }
    }

    #[test]
    fn resulting_cursor_position_preserves_the_cursor_variant() {
        // Uninitialized / OutOfWindow must survive as themselves — a getter that
        // "helpfully" normalized them to InWindow(0,0) would place the caret at
        // the window origin.
        for expected in [
            CursorPosition::Uninitialized,
            CursorPosition::OutOfWindow(LogicalPosition::new(-5.0, -5.0)),
            CursorPosition::InWindow(LogicalPosition::new(0.0, 0.0)),
        ] {
            let cs = changeset_for(TextOperation::DeleteText(TextOpDeleteText {
                range: simple_range(),
                deleted_text: AzString::from("x"),
                new_cursor: expected,
            }));
            assert_eq!(cs.resulting_cursor_position(), Some(expected));
        }
    }

    // =========================================================================
    // 4. resulting_selection_range
    // =========================================================================

    #[test]
    fn resulting_selection_range_is_some_exactly_for_range_setting_ops() {
        for (name, op) in all_ops() {
            let cs = changeset_for(op);
            let expected = matches!(name, "SetSelection" | "ExtendSelection" | "SelectAll");
            assert_eq!(
                cs.resulting_selection_range().is_some(),
                expected,
                "{name}: resulting_selection_range() presence"
            );

            // Invariant: a resulting range implies the changeset changes the
            // selection. (The converse does NOT hold — ClearSelection and
            // MoveCursor change the selection but produce no range; that
            // asymmetry is asserted below.)
            if cs.resulting_selection_range().is_some() {
                assert!(
                    cs.changes_selection(),
                    "{name} yields a selection range but denies changing the selection"
                );
            }
        }
    }

    #[test]
    fn clear_and_move_change_selection_but_yield_no_range() {
        let cleared = changeset_for(TextOperation::ClearSelection(TextOpClearSelection {
            old_range: simple_range(),
        }));
        assert!(cleared.changes_selection());
        assert_eq!(cleared.resulting_selection_range(), None);
        assert_eq!(cleared.resulting_cursor_position(), None);

        let moved = changeset_for(TextOperation::MoveCursor(TextOpMoveCursor {
            old_position: cur(0.0, 0.0),
            new_position: cur(1.0, 0.0),
            movement: CursorMovement::WordRight,
        }));
        assert!(moved.changes_selection());
        assert_eq!(moved.resulting_selection_range(), None);
        assert_eq!(moved.resulting_cursor_position(), Some(cur(1.0, 0.0)));
    }

    #[test]
    fn resulting_selection_range_does_not_normalize_a_backwards_range() {
        // A backwards (end < start) selection is legal — "the direction is
        // implicit". The getter must not silently swap the endpoints.
        let backwards = extreme_range();
        assert!(backwards.end < backwards.start);

        let cs = changeset_for(TextOperation::SetSelection(TextOpSetSelection {
            old_range: OptionSelectionRange::None,
            new_range: backwards,
        }));

        let got = cs
            .resulting_selection_range()
            .expect("SetSelection must yield a range");
        assert_eq!(got, backwards, "endpoints were reordered or clamped");
        assert_eq!(got.start.cluster_id.source_run, u32::MAX);
        assert_eq!(got.start.cluster_id.start_byte_in_run, u32::MAX);
        assert_eq!(got.start.affinity, CursorAffinity::Trailing);
        assert_eq!(got.end, tc(0, 0, CursorAffinity::Leading));
    }

    #[test]
    fn resulting_selection_range_returns_new_range_and_preserves_empty_ranges() {
        // Collapsed range (start == end) is a caret, not "no selection" — it
        // must come back as Some, not None.
        let caret = range(
            tc(7, 3, CursorAffinity::Leading),
            tc(7, 3, CursorAffinity::Leading),
        );
        let cs = changeset_for(TextOperation::ExtendSelection(TextOpExtendSelection {
            old_range: extreme_range(),
            new_range: caret,
            direction: SelectionDirection::Forward,
        }));
        assert_eq!(cs.resulting_selection_range(), Some(caret));

        // SelectAll must return `new_range`, never `old_range`.
        let cs = changeset_for(TextOperation::SelectAll(TextOpSelectAll {
            old_range: OptionSelectionRange::Some(simple_range()),
            new_range: caret,
        }));
        assert_eq!(cs.resulting_selection_range(), Some(caret));
    }

    // =========================================================================
    // 5. Payload round-trips (unicode / huge / empty)
    // =========================================================================

    #[test]
    fn text_payloads_round_trip_through_the_changeset_unchanged() {
        let cases = [
            "",                              // empty
            "\0",                            // interior NUL
            "a\u{0301}",                     // combining acute
            "\u{1F1E9}\u{1F1EA}",            // regional-indicator pair
            "\u{202E}txet desrever\u{202C}", // bidi override
            "\u{10FFFF}",                    // highest scalar value
            "line1\r\nline2\u{2028}line3",   // CRLF + LINE SEPARATOR
        ];

        for s in cases {
            let cs = changeset_for(TextOperation::ReplaceText(TextOpReplaceText {
                range: simple_range(),
                old_text: AzString::from(s),
                new_text: AzString::from(s),
                new_cursor: CursorPosition::Uninitialized,
            }));

            match &cs.operation {
                TextOperation::ReplaceText(op) => {
                    assert_eq!(op.old_text.as_str(), s, "old_text mangled for {s:?}");
                    assert_eq!(op.new_text.as_str(), s, "new_text mangled for {s:?}");
                    assert_eq!(op.old_text.as_str().len(), s.len(), "byte length changed");
                }
                other => panic!("unexpected variant: {other:?}"),
            }
        }
    }

    #[test]
    fn clipboard_payloads_survive_and_stay_classified_as_clipboard_ops() {
        let big = "\u{00E9}".repeat(128 * 1024); // 256 KiB of 2-byte chars

        let cs = changeset_for(TextOperation::Cut(TextOpCut {
            range: extreme_range(),
            content: clip(&big),
            new_cursor: cur(0.0, 0.0),
        }));

        assert!(cs.uses_clipboard());
        assert!(
            cs.mutates_text(),
            "Cut deletes text, so it must count as a mutation"
        );
        assert!(!cs.changes_selection());
        match &cs.operation {
            TextOperation::Cut(op) => {
                assert_eq!(op.content.plain_text.as_str().len(), 256 * 1024);
                assert!(op.content.styled_runs.as_slice().is_empty());
                // Empty styled_runs => empty <div> wrapper, no panic on a huge run.
                assert_eq!(op.content.to_html(), "<div></div>");
            }
            other => panic!("unexpected variant: {other:?}"),
        }

        // An empty clipboard payload is still a clipboard op.
        let empty = changeset_for(TextOperation::Copy(TextOpCopy {
            range: simple_range(),
            content: clip(""),
        }));
        assert!(empty.uses_clipboard());
        assert!(!empty.mutates_text());
        assert_eq!(empty.resulting_cursor_position(), None);
        assert_eq!(empty.resulting_selection_range(), None);
    }

    #[test]
    fn timestamps_round_trip_and_stay_ordered() {
        let zero = changeset_for_ts(ts(0));
        let max = changeset_for_ts(ts(u64::MAX));

        assert_eq!(zero.timestamp, ts(0));
        assert_eq!(max.timestamp, ts(u64::MAX));
        assert!(
            zero.timestamp < max.timestamp,
            "tick ordering must survive being stored in a changeset"
        );
    }

    fn changeset_for_ts(timestamp: Instant) -> TextChangeset {
        TextChangeset::new(
            node(0, 1),
            TextOperation::ClearSelection(TextOpClearSelection {
                old_range: simple_range(),
            }),
            timestamp,
        )
    }
}
