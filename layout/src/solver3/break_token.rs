//! Break tokens — the value-type "resume here" state for true fragmentation.
//!
//! The layout contract is LayoutNG-shaped and PURE:
//!
//! ```text
//! (node, constraint_space { remaining_extent }, break_token?) -> (fragment, break_token?)
//! ```
//!
//! Tokens are OWNED, comparable value types — nothing about pagination is
//! ever written into the node tree, the layout tree, or any cache during a
//! pass (the fossil at `paged_layout.rs:1-11` documents why the mutable
//! alternative failed). Two laws every consumer may rely on:
//!
//! 1. **Determinism**: identical input (content + constraints + incoming
//!    token) produces an identical outgoing token, comparable with `==`.
//!    Equality is structural; float fields inherit text3's rounding-tolerant
//!    `Rect` comparison, which is safe in the conservative direction — a
//!    false *inequality* merely re-lays one extra page, a false *equality*
//!    cannot arise from tolerant comparison of identical-bits passes.
//! 2. **Progress**: an outgoing token never equals the incoming token of
//!    the same fragmentainer (the page loop asserts this; violating it is
//!    the NG infinite-loop class).
//!
//! `token_fingerprint` is a FAST-PATH REJECTOR for K34 convergence checks:
//! `a == b ⇒ fingerprint(a) == fingerprint(b)` (it hashes a subset of the
//! compared fields). Convergence must NEVER be decided on fingerprints
//! alone — equal fingerprints require the full `==` before stopping
//! repagination (a collision that stopped early would ship stale pages).
//!
//! Provenance note: the token SHAPE follows public architecture prose
//! (css-break-3, the `RenderingNG` fragmentation article, the `LayoutNG`
//! README); no engine implementation source was consulted. See the design
//! doc §9.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::text3::cache::{BreakCursor, Hyphens, LineBreakStrictness, ShapedItem, WordBreak};

/// The resume state a fragmentainer boundary produced. `None` anywhere a
/// token could appear means "finished — nothing to resume".
#[derive(Debug, Clone, PartialEq)]
pub enum BreakToken {
    /// Resume a block-level box (its unfinished/unstarted children carry
    /// their own tokens).
    Block(BlockBreakToken),
    /// Resume an inline formatting context mid-flow.
    Inline(InlineBreakToken),
}

/// Resume state for one BLOCK box. Invariant (asserted by consumers, not
/// trusted): every sibling BEFORE the first entry in `children` is FINISHED
/// — `children` is the unfinished tail, in document order.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockBreakToken {
    /// Layout-tree index of the box this token resumes. Tokens never
    /// outlive their layout generation (they are regenerated per pass), so
    /// the index is same-generation by construction; `generation` exists to
    /// assert that in debug builds.
    pub node: usize,
    /// Block-size of this box already consumed by previous fragmentainers.
    /// Drives `box-decoration-break: slice` (default: no re-emitted top
    /// decoration on resume) and monolith overflow resumption.
    pub consumed_block_size: f32,
    /// The unfinished tail of this box's children, document order.
    pub children: Vec<ChildBreakEntry>,
    /// Layout-generation stamp for debug assertions (see `node`).
    pub generation: u64,
}

/// One unfinished child in a [`BlockBreakToken`].
#[derive(Debug, Clone, PartialEq)]
pub enum ChildBreakEntry {
    /// The child started in an earlier fragmentainer; resume it with this
    /// token.
    ResumeIn {
        child: usize,
        token: Box<BreakToken>,
    },
    /// The child has not started yet — a break landed before it.
    /// `forced` = the break came from `break-before: page` (or a
    /// `<pagebreak/>` node): css-break-3 §5.2 truncates margins adjoining
    /// UNFORCED breaks only, so the resume side keeps this child's top
    /// margin iff the break was forced.
    BreakBefore { child: usize, forced: bool },
}

/// Owned snapshot of text3's [`BreakCursor`] — the inline resume state.
///
/// `BreakCursor` borrows `&'a [ShapedItem]` and therefore cannot be stored
/// across passes or compared as a value; this snapshot owns exactly the
/// STATE (resume index + hyphenation remainder). The style knobs on the
/// cursor (`word_break` / `hyphens` / `line_break`) are deliberately NOT
/// part of the token: they derive from style, not from layout progress —
/// [`InlineBreakToken::resume`] takes them from the caller, who reads them
/// from the same style the original cursor did.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineBreakToken {
    /// Index of the next *full* item to process in the IFC's shaped-item
    /// sequence.
    pub next_item_index: usize,
    /// The remainder of an item split by hyphenation on the boundary line —
    /// the very first content of the resumed fragment.
    pub partial_remainder: Vec<ShapedItem>,
}

impl InlineBreakToken {
    /// Snapshot a live cursor's resume state (pure; the cursor is untouched).
    #[must_use]
    pub fn from_cursor(cursor: &BreakCursor<'_>) -> Self {
        Self {
            next_item_index: cursor.next_item_index,
            partial_remainder: cursor.partial_remainder.clone(),
        }
    }

    /// Reconstruct a cursor over `items` positioned exactly where the
    /// snapshotted one stopped. The style knobs come from the caller (they
    /// are style-derived, not layout state — see the type docs).
    #[must_use]
    pub fn resume<'a>(
        &self,
        items: &'a [ShapedItem],
        word_break: WordBreak,
        hyphens: Hyphens,
        line_break: LineBreakStrictness,
    ) -> BreakCursor<'a> {
        BreakCursor {
            items,
            next_item_index: self.next_item_index,
            partial_remainder: self.partial_remainder.clone(),
            word_break,
            hyphens,
            line_break,
        }
    }

    /// True when resuming would start from the very beginning — such a
    /// token should not exist (it encodes "no progress"); the page loop's
    /// progress guard treats it as a hard stop.
    #[must_use]
    pub const fn is_degenerate_start(&self) -> bool {
        self.next_item_index == 0 && self.partial_remainder.is_empty()
    }
}

// ---------------------------------------------------------------------------
// K30b decision helpers — pure, exhaustively unit-tested; `layout_bfc` only
// wires them (design §4.4)
// ---------------------------------------------------------------------------

/// Verdict for one normal-flow child against the fragmentainer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitDecision {
    /// The child ends within the remaining extent: place it here.
    Fits,
    /// Break BEFORE this child — it (and its later siblings) resume in the
    /// next fragmentainer.
    BreakBeforeHere,
    /// The child is the first content of this fragmentainer and no
    /// fragmentainer can ever hold it: place it OVERFLOWING (css-break
    /// monolith rule — never tear, never loop).
    MonolithOverflow,
}

/// Geometry tolerance for fit checks: sub-1/100-px overshoot is float noise,
/// not a page break.
const FIT_EPS: f32 = 0.01;

/// Decide whether a child whose border-box starts at `pen` (margins already
/// resolved) and spans `child_block_size` fits the remaining extent.
///
/// `placed_any_content`: whether this fragmentainer already holds content
/// from this box — if it does, breaking before the child always makes
/// progress; if it does not, breaking only helps when a FRESH fragmentainer
/// is actually bigger than what remains here (otherwise the child is a
/// monolith and overflows).
#[must_use]
pub fn fragment_fit(
    pen: f32,
    child_block_size: f32,
    remaining_block_extent: f32,
    next_fragmentainer_extent: f32,
    placed_any_content: bool,
) -> FitDecision {
    if pen + child_block_size <= remaining_block_extent + FIT_EPS {
        return FitDecision::Fits;
    }
    if placed_any_content {
        return FitDecision::BreakBeforeHere;
    }
    // First content of the fragmentainer overflows on its own. Progress
    // guarantee: only defer to the next fragmentainer if it is genuinely
    // roomier than what is left here AND can hold the child.
    if child_block_size <= next_fragmentainer_extent + FIT_EPS
        && next_fragmentainer_extent > remaining_block_extent + FIT_EPS
    {
        FitDecision::BreakBeforeHere
    } else {
        FitDecision::MonolithOverflow
    }
}

/// Build the outgoing token when a break lands before `breaking_child`:
/// the unfinished tail is that child plus every later in-flow sibling, all
/// as `BreakBefore` entries (block-granular v1 — `ResumeIn` entries appear
/// when nested resume lands, K30b part 2).
#[must_use]
pub fn tail_token(
    node: usize,
    consumed_block_size: f32,
    breaking_child: usize,
    later_in_flow_siblings: impl Iterator<Item = usize>,
) -> BreakToken {
    let mut children = alloc::vec![ChildBreakEntry::BreakBefore {
        child: breaking_child,
        forced: false,
    }];
    children.extend(
        later_in_flow_siblings.map(|child| ChildBreakEntry::BreakBefore {
            child,
            forced: false,
        }),
    );
    BreakToken::Block(BlockBreakToken {
        node,
        consumed_block_size,
        children,
        generation: 0,
    })
}

/// The consumer-side resume plan for a block token: which child is the
/// FIRST unfinished one (everything before it is finished and must be
/// skipped with zero side effects). `None` for a childless token —
/// defensive: such a token encodes no work and resuming from it is a
/// no-op, which the page loop's progress guard turns into a stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumePlan {
    pub first_unfinished: usize,
}

#[must_use]
pub fn resume_plan(token: &BlockBreakToken) -> Option<ResumePlan> {
    token.children.first().map(|entry| ResumePlan {
        first_unfinished: match entry {
            ChildBreakEntry::ResumeIn { child, .. }
            | ChildBreakEntry::BreakBefore { child, .. } => *child,
        },
    })
}

// ---------------------------------------------------------------------------
// Fingerprints — fast-path rejector for K34 convergence
// ---------------------------------------------------------------------------

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[inline]
fn fnv(hash: u64, byte: u8) -> u64 {
    (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
}

#[inline]
fn fnv_u64(mut hash: u64, value: u64) -> u64 {
    for b in value.to_le_bytes() {
        hash = fnv(hash, b);
    }
    hash
}

/// 64-bit structural fingerprint. Law: `a == b ⇒ fingerprint(a) ==
/// fingerprint(b)` — guaranteed because it hashes a SUBSET of the fields
/// `PartialEq` compares (float geometry inside `partial_remainder` items is
/// summarized by count + source indices, never by the tolerant-compared
/// floats themselves, so the law survives the rounding tolerance).
/// Convergence checks use it to reject fast and MUST confirm with `==`.
#[must_use]
pub fn token_fingerprint(token: &BreakToken) -> u64 {
    fingerprint_into(FNV_OFFSET, token)
}

fn fingerprint_into(mut h: u64, token: &BreakToken) -> u64 {
    match token {
        BreakToken::Block(b) => {
            h = fnv(h, 0x01);
            h = fnv_u64(h, b.node as u64);
            // consumed_block_size participates in PartialEq as an exact
            // f32 compare, so its bits are a valid fingerprint component.
            h = fnv_u64(h, u64::from(b.consumed_block_size.to_bits()));
            h = fnv_u64(h, b.children.len() as u64);
            for entry in &b.children {
                match entry {
                    ChildBreakEntry::ResumeIn { child, token } => {
                        h = fnv(h, 0x02);
                        h = fnv_u64(h, *child as u64);
                        h = fingerprint_into(h, token);
                    }
                    ChildBreakEntry::BreakBefore { child, .. } => {
                        h = fnv(h, 0x03);
                        h = fnv_u64(h, *child as u64);
                    }
                }
            }
            h
        }
        BreakToken::Inline(t) => {
            h = fnv(h, 0x04);
            h = fnv_u64(h, t.next_item_index as u64);
            h = fnv_u64(h, t.partial_remainder.len() as u64);
            // Source indices are integer-exact fields of the compared items
            // (Rect floats are deliberately excluded — they compare with a
            // rounding tolerance, hashing them would break the law).
            for item in &t.partial_remainder {
                if let Some(src) = shaped_item_source(item) {
                    h = fnv_u64(h, u64::from(src.run_index));
                    h = fnv_u64(h, u64::from(src.item_index));
                }
            }
            h
        }
    }
}

// Returns Option to mirror the other `*_source` accessors and to stay
// source-compatible if a future `ShapedItem` variant has no content index.
#[allow(clippy::unnecessary_wraps)]
const fn shaped_item_source(item: &ShapedItem) -> Option<azul_core::selection::ContentIndex> {
    match item {
        ShapedItem::Cluster(c) => Some(c.source_content_index),
        ShapedItem::CombinedBlock { source, .. }
        | ShapedItem::Object { source, .. }
        | ShapedItem::Tab { source, .. }
        | ShapedItem::Break { source, .. } => Some(*source),
    }
}

// ---------------------------------------------------------------------------
// Property tests (K30a exit gate; see design doc §6.2)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod break_token_laws {
    use azul_core::selection::ContentIndex;

    use super::*;
    use crate::text3::cache::Rect;

    fn tab(run: u32, item: u32, w: f32) -> ShapedItem {
        ShapedItem::Tab {
            source: ContentIndex {
                run_index: run,
                item_index: item,
            },
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: w,
                height: 16.0,
            },
        }
    }

    fn inline(next: usize, remainder: Vec<ShapedItem>) -> BreakToken {
        BreakToken::Inline(InlineBreakToken {
            next_item_index: next,
            partial_remainder: remainder,
        })
    }

    fn block(node: usize, consumed: f32, children: Vec<ChildBreakEntry>) -> BreakToken {
        BreakToken::Block(BlockBreakToken {
            node,
            consumed_block_size: consumed,
            children,
            generation: 1,
        })
    }

    // -- Eq laws ----------------------------------------------------------

    #[test]
    fn equality_is_structural_and_reflexive() {
        let t = block(
            7,
            120.5,
            vec![
                ChildBreakEntry::BreakBefore {
                    child: 3,
                    forced: false,
                },
                ChildBreakEntry::ResumeIn {
                    child: 2,
                    token: Box::new(inline(4, vec![tab(0, 9, 12.0)])),
                },
            ],
        );
        assert_eq!(t, t.clone());
        // Any structural difference breaks equality: node…
        let mut o = t.clone();
        if let BreakToken::Block(b) = &mut o {
            b.node = 8;
        }
        assert_ne!(t, o);
        // …consumed size…
        let mut o = t.clone();
        if let BreakToken::Block(b) = &mut o {
            b.consumed_block_size += 0.5;
        }
        assert_ne!(t, o);
        // …child order (document order is semantic)…
        let mut o = t.clone();
        if let BreakToken::Block(b) = &mut o {
            b.children.reverse();
        }
        assert_ne!(t, o);
        // …and nested inline state.
        let mut o = t.clone();
        if let BreakToken::Block(b) = &mut o {
            if let ChildBreakEntry::ResumeIn { token, .. } = &mut b.children[1] {
                **token = inline(5, vec![tab(0, 9, 12.0)]);
            }
        }
        assert_ne!(t, o);
    }

    #[test]
    fn fingerprint_law_equal_tokens_have_equal_fingerprints() {
        let cases = [
            inline(0, vec![]),
            inline(3, vec![tab(1, 2, 8.0)]),
            block(0, 0.0, vec![]),
            block(
                5,
                33.25,
                vec![ChildBreakEntry::ResumeIn {
                    child: 1,
                    token: Box::new(inline(2, vec![])),
                }],
            ),
        ];
        for t in &cases {
            assert_eq!(
                token_fingerprint(t),
                token_fingerprint(&t.clone()),
                "fingerprint must be a pure function of compared fields: {t:?}"
            );
        }
        // And it actually discriminates the obvious cases (not a constant).
        assert_ne!(
            token_fingerprint(&cases[0]),
            token_fingerprint(&cases[2]),
            "inline(0) vs block(0) must not collide on the variant tag"
        );
        assert_ne!(
            token_fingerprint(&inline(1, vec![])),
            token_fingerprint(&inline(2, vec![]))
        );
    }

    #[test]
    fn fingerprint_survives_the_tolerant_rect_compare() {
        // text3's Rect PartialEq is rounding-tolerant: two tokens whose
        // remainder Rects differ inside the tolerance are EQUAL — the
        // fingerprint must agree (law: a == b ⇒ fp(a) == fp(b)). This is
        // exactly why Rect floats are excluded from the fingerprint.
        let a = inline(3, vec![tab(1, 2, 8.0)]);
        let b = inline(3, vec![tab(1, 2, 8.000001)]);
        if a == b {
            assert_eq!(token_fingerprint(&a), token_fingerprint(&b));
        } else {
            // If the tolerance ever tightens to bit-exact this branch keeps
            // the test meaningful instead of vacuous.
            assert_ne!(a, b);
        }
    }

    // -- Cursor bridge ----------------------------------------------------

    #[test]
    fn cursor_snapshot_resume_round_trips() {
        let items = vec![tab(0, 0, 10.0), tab(0, 1, 10.0), tab(0, 2, 10.0)];
        let mut cursor = BreakCursor::new(&items);
        cursor.next_item_index = 2;
        cursor.partial_remainder = vec![tab(0, 1, 4.0)];

        let token = InlineBreakToken::from_cursor(&cursor);
        let resumed = token.resume(&items, cursor.word_break, cursor.hyphens, cursor.line_break);

        assert_eq!(resumed.next_item_index, cursor.next_item_index);
        assert_eq!(resumed.partial_remainder, cursor.partial_remainder);
        assert!(!resumed.is_at_start());
        // And the snapshot round-trips through the snapshot again.
        assert_eq!(InlineBreakToken::from_cursor(&resumed), token);
    }

    #[test]
    fn degenerate_start_token_is_detected() {
        assert!(InlineBreakToken {
            next_item_index: 0,
            partial_remainder: vec![],
        }
        .is_degenerate_start());
        assert!(!InlineBreakToken {
            next_item_index: 0,
            partial_remainder: vec![tab(0, 0, 1.0)],
        }
        .is_degenerate_start());
        assert!(!InlineBreakToken {
            next_item_index: 1,
            partial_remainder: vec![],
        }
        .is_degenerate_start());
    }

    // -- K30b decision helpers ---------------------------------------------

    #[test]
    fn fragment_fit_truth_table() {
        use FitDecision::*;
        // Fits exactly / with epsilon slack.
        assert_eq!(fragment_fit(0.0, 100.0, 100.0, 100.0, false), Fits);
        assert_eq!(fragment_fit(50.0, 50.005, 100.0, 100.0, true), Fits);
        // Mid-page overflow with content already placed: break before.
        assert_eq!(
            fragment_fit(80.0, 40.0, 100.0, 100.0, true),
            BreakBeforeHere
        );
        // First content, uniform pages, taller than a page: monolith.
        assert_eq!(
            fragment_fit(0.0, 250.0, 100.0, 100.0, false),
            MonolithOverflow
        );
        // First content, but the NEXT page is roomier and holds it: defer.
        assert_eq!(
            fragment_fit(0.0, 250.0, 100.0, 300.0, false),
            BreakBeforeHere
        );
        // First content, next page roomier but STILL too small: monolith
        // (deferring would just move the overflow, not fix it).
        assert_eq!(
            fragment_fit(0.0, 400.0, 100.0, 300.0, false),
            MonolithOverflow
        );
        // Progress guarantee: never break-before on first content when the
        // next fragmentainer is the same size (would loop forever).
        assert_eq!(
            fragment_fit(0.0, 100.02, 100.0, 100.0, false),
            MonolithOverflow
        );
    }

    #[test]
    fn tail_token_lists_the_breaking_child_then_later_siblings_in_order() {
        let t = tail_token(4, 320.0, 7, [9, 12].into_iter());
        let BreakToken::Block(b) = &t else {
            panic!("block token expected")
        };
        assert_eq!(b.node, 4);
        assert_eq!(b.consumed_block_size, 320.0);
        assert_eq!(
            b.children,
            vec![
                ChildBreakEntry::BreakBefore {
                    child: 7,
                    forced: false
                },
                ChildBreakEntry::BreakBefore {
                    child: 9,
                    forced: false
                },
                ChildBreakEntry::BreakBefore {
                    child: 12,
                    forced: false
                },
            ]
        );
        // Consumer side: resume starts exactly at the breaking child.
        assert_eq!(resume_plan(b).unwrap().first_unfinished, 7);
    }

    #[test]
    fn resume_plan_is_none_for_a_childless_token() {
        let empty = BlockBreakToken {
            node: 1,
            consumed_block_size: 0.0,
            children: vec![],
            generation: 0,
        };
        assert!(resume_plan(&empty).is_none());
    }

    // -- Progress guard shape ----------------------------------------------

    #[test]
    fn progress_is_observable_via_equality() {
        // The page loop's no-progress guard is `outgoing == incoming`; pin
        // that "one more child finished" and "one more item consumed" are
        // both visible to it.
        let before = block(
            0,
            0.0,
            vec![
                ChildBreakEntry::ResumeIn {
                    child: 1,
                    token: Box::new(inline(2, vec![])),
                },
                ChildBreakEntry::BreakBefore {
                    child: 2,
                    forced: false,
                },
            ],
        );
        let after_child_finished = block(
            0,
            0.0,
            vec![ChildBreakEntry::ResumeIn {
                child: 2,
                token: Box::new(inline(0, vec![tab(0, 0, 1.0)])),
            }],
        );
        assert_ne!(before, after_child_finished);

        let after_items_consumed = block(
            0,
            0.0,
            vec![
                ChildBreakEntry::ResumeIn {
                    child: 1,
                    token: Box::new(inline(3, vec![])),
                },
                ChildBreakEntry::BreakBefore {
                    child: 2,
                    forced: false,
                },
            ],
        );
        assert_ne!(before, after_items_consumed);
    }
}
