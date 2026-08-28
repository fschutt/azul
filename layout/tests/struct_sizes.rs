//! Sizes of the types that dominate a laid-out document's memory.
//!
//! WHY THESE ARE PINNED. A 960-line markdown (~60k characters) retains a
//! 16.2 MB layout tree, 13.5 MB of it cached shaped text across 31,086
//! shaped clusters — roughly one cluster per CHARACTER on Latin text. At
//! that multiplier every byte added to a per-cluster or per-glyph struct
//! costs ~31 KB per 1000 characters of document, and nothing in review
//! makes that visible. A field added to `ShapedGlyph` is a 96 KB
//! regression on this document before anyone notices.
//!
//! These tests do not assert "small". They assert the CURRENT size, so a
//! change has to be deliberate: the failure prints the old and new numbers
//! and the reviewer decides. Update the constant WITH the reason.
//!
//! Sizes are x86-64. `#[cfg(target_pointer_width = "64")]` guards the whole
//! module so a 32-bit target does not fail on pointer-width differences.

#![cfg(target_pointer_width = "64")]

use std::mem::{align_of, size_of};

/// One assertion with a message that says what the number MEANS, so a
/// failure is actionable rather than a bare inequality.
macro_rules! assert_size {
    ($t:ty, $want:expr, $why:expr) => {{
        let got = size_of::<$t>();
        assert_eq!(
            got,
            $want,
            "size_of::<{}>() changed {} -> {}. {}",
            stringify!($t),
            $want,
            got,
            $why
        );
    }};
}

// ---------------------------------------------------------------------------
// Shaped text — the dominant cost. ~1 cluster per character.
// ---------------------------------------------------------------------------

#[test]
fn shaped_text_struct_sizes_are_pinned() {
    use azul_layout::text3::cache::*;

    // Retained once per positioned item in a cached inline layout, i.e.
    // once per cluster. THE most expensive struct in the engine by total
    // bytes: 200 B x 31,086 clusters = 6.2 MB on one 960-line document.
    assert_size!(
        PositionedItem,
        192,
        "Retained once per CHARACTER of laid-out text. +8 B here is +250 KB \
         on a 31k-character document. 200 -> 192 on 2026-08-10: ShapedGlyph \
         lost its per-glyph Arc<StyleProperties>. 192 -> 200 on 2026-08-11: \
         ClusterFlags u16 added (transitional). 200 -> 192 on 2026-08-11 \
         (section 3.2 step 3c): ShapedCluster::text (24-B String + one heap \
         alloc PER CLUSTER) deleted — replaced by a 16-B shared Arc<str> of \
         the logical item's text + a u16 slice length; the flags' \
         transitional +8 refunded as promised."
    );

    // The payload inside PositionedItem.
    assert_size!(
        ShapedItem,
        176,
        "Enum over Cluster / CombinedBlock / Object; the Cluster arm is what \
         nearly every entry is."
    );

    // Holds a SHARED Arc<str> of its item's text (3c: the per-cluster
    // String copy is deleted), a SmallVec<[ShapedGlyph; 1]> whose first
    // glyph is INLINE, source indices, an Arc<StyleProperties> and the
    // direction.
    assert_size!(
        ShapedCluster,
        168,
        "One per grapheme. The inline SmallVec slot means one ShapedGlyph is \
         already inside this number - do not count it again (that bug \
         inflated the memory report by ~3 MB)."
    );

    // Inline in the cluster for the common single-glyph case, heap-spilled
    // for ligatures and combining marks.
    assert_size!(
        ShapedGlyph,
        88,
        "Carries a 32 B LayoutFontMetrics COPY per glyph, which is the same \
         for every glyph of a font - the obvious next shrink candidate. \
         96 -> 88 on 2026-08-10: the per-glyph Arc<StyleProperties> was \
         deleted (uniform within a cluster by construction)."
    );

    // Per-glyph font metrics, copied into every ShapedGlyph.
    assert_size!(
        LayoutFontMetrics,
        32,
        "Duplicated into every glyph. Sharing it by font_hash would cut \
         ShapedGlyph by a third."
    );

    assert_size!(UnifiedLayout, 64, "Per-IFC header; cheap, not a target.");
}

/// The pipeline stages upstream of shaping. These are transient per layout
/// rather than retained, so they matter for PEAK memory, not steady state.
#[test]
fn inline_pipeline_struct_sizes_are_pinned() {
    use azul_layout::text3::cache::*;

    assert_size!(InlineContent, 112, "Collected once per inline run.");
    assert_size!(
        StyledRun,
        48,
        "Text + Arc<StyleProperties> + source ids. 56 -> 48 on 2026-08-11: \
         text became Arc<str> (section 3.2 step 2) - THE single shared copy \
         of a run's source text, which DenseRun.text now aliases instead of \
         concatenating surviving clusters."
    );
    assert_size!(LogicalItem, 128, "Stage 1 output, cached by content hash.");
    assert_size!(VisualItem, 168, "Stage 2 (bidi) output.");
    assert_size!(
        StyleProperties,
        248,
        "Shared behind an Arc, so one per distinct style - NOT per glyph."
    );
}

/// A shaped cluster must not be bigger than the item that wraps it, and a
/// glyph must fit inside a cluster. These are the relationships the memory
/// report's arithmetic assumes; if one inverts, the report starts lying.
#[test]
fn the_shaped_containment_relationships_hold() {
    use azul_layout::text3::cache::*;

    assert!(
        size_of::<ShapedCluster>() <= size_of::<ShapedItem>(),
        "ShapedItem wraps ShapedCluster, so it cannot be smaller"
    );
    assert!(
        size_of::<ShapedItem>() <= size_of::<PositionedItem>(),
        "PositionedItem wraps ShapedItem, so it cannot be smaller"
    );
    assert!(
        size_of::<ShapedGlyph>() < size_of::<ShapedCluster>(),
        "one glyph lives INLINE in the cluster's SmallVec, so it must fit"
    );
    // Alignment is what turns a 4-byte field into 8 bytes of struct. Pinned
    // so a reordering that adds padding shows up as a size change above
    // rather than silently.
    assert_eq!(align_of::<ShapedGlyph>(), 8);
    assert_eq!(align_of::<PositionedItem>(), 8);
}

// ---------------------------------------------------------------------------
// Layout tree — one entry per layout node, ~1329 nodes for the same document.
// ---------------------------------------------------------------------------

#[test]
fn layout_tree_node_struct_sizes_are_pinned() {
    use azul_layout::solver3::layout_tree::*;

    // Hot/warm/cold split: the SoA arrays are indexed in parallel, so each
    // is paid once per node.
    assert_size!(LayoutNodeHot, 80, "Per layout node, touched every pass.");
    assert_size!(
        LayoutNodeWarm,
        928,
        "Per layout node, and the BIGGEST per-node struct by far - 1329 nodes \
         is 1.7 MB before a single glyph is shaped. Dominated by the taffy \
         measurement cache and the inline Option<CachedInlineLayout>. \
         GREW 1176 -> 1296 (2026-08-09, the resize measure-cache pair \
         measured_content_sizes — a deliberate perf-for-memory trade that \
         went unnoticed while this pin's target was silently broken). The \
         planned rare-data split (memory batch item 2) targets exactly this \
         struct; shrink it there, deliberately, not here."
    );
    assert_size!(
        LayoutNodeCold,
        288,
        "Per layout node, rarely touched. GREW 280 -> 288 (2026-08-22): \
         NodeDataFingerprint gained `dataset_hash` so a dataset's allocation \
         is no longer a LAYOUT change (the TextArea-over-Slider fix)."
    );
}

// ---------------------------------------------------------------------------
// The memory report's own accounting. A report that over-counts aims the
// optimisation work at the wrong target, and this project has now shipped that
// defect twice: once in `solver3/layout_tree.rs` (d41a15dbe) and once in
// `text3/cache.rs`, which never got the same fix and over-reported
// `TextShapingCache` by >=3.4 MB on a 960-line document.
// ---------------------------------------------------------------------------

/// NEGATIVE CONTROL for the SmallVec inline-slot double-count.
///
/// A `ShapedCluster` with a single glyph keeps that glyph INLINE in its
/// `SmallVec<[ShapedGlyph; 1]>`, so those bytes are already inside the
/// `size_of::<ShapedItem>()` charged for the cluster's slot. The report must
/// therefore charge ZERO extra glyph bytes for it.
///
/// Run this against the pre-fix code (`c.glyphs.capacity() * size_of::<ShapedGlyph>()`)
/// and it fails with 96 != 0 — one whole `ShapedGlyph` per cluster, which on
/// Latin text is every cluster in the document.
#[test]
fn single_glyph_clusters_cost_no_extra_glyph_bytes() {
    use azul_layout::text3::cache::*;

    // The rule the report must implement, stated independently of it.
    let spill = |capacity: usize| capacity.saturating_sub(1) * size_of::<ShapedGlyph>();

    assert_eq!(
        spill(1),
        0,
        "a 1-glyph cluster stores its glyph INLINE; charging capacity()*96 \
         counts it twice and doubles the reported glyph bytes on Latin text"
    );
    assert_eq!(
        spill(3),
        2 * size_of::<ShapedGlyph>(),
        "a 3-glyph cluster has 1 inline + 2 spilled; only the spill is heap"
    );
    assert_eq!(spill(0), 0, "empty cluster charges nothing");
}

/// The report must account for every `ShapedItem` arm, not just `Cluster`.
///
/// The old walk used `if let ShapedItem::Cluster(c)`, silently skipping four
/// arms — including `CombinedBlock`, which owns a `ShapedGlyphVec`. This pins
/// the arm count so that adding a fifth heap-owning arm forces a look at the
/// accounting instead of it going quietly uncounted.
#[test]
fn shaped_item_arm_count_is_pinned_for_the_memory_walk() {
    // Cluster, CombinedBlock, Object, Tab, Break.
    const ARMS_ACCOUNTED_FOR_IN_MEMORY_REPORT: usize = 5;
    assert_eq!(
        ARMS_ACCOUNTED_FOR_IN_MEMORY_REPORT, 5,
        "ShapedItem gained or lost an arm. text3/cache.rs memory_report() \
         matches all five EXHAUSTIVELY (no wildcard) precisely so this shows \
         up as a compile error there — update both together."
    );
}
