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
        200,
        "Retained once per CHARACTER of laid-out text. +8 B here is +250 KB \
         on a 31k-character document."
    );

    // The payload inside PositionedItem.
    assert_size!(
        ShapedItem,
        184,
        "Enum over Cluster / CombinedBlock / Object; the Cluster arm is what \
         nearly every entry is."
    );

    // Holds its own `String` copy of the cluster text (usually one char),
    // a SmallVec<[ShapedGlyph; 1]> whose first glyph is INLINE, source
    // indices, an Arc<StyleProperties> and the direction.
    assert_size!(
        ShapedCluster,
        176,
        "One per grapheme. The inline SmallVec slot means one ShapedGlyph is \
         already inside this number - do not count it again (that bug \
         inflated the memory report by ~3 MB)."
    );

    // Inline in the cluster for the common single-glyph case, heap-spilled
    // for ligatures and combining marks.
    assert_size!(
        ShapedGlyph,
        96,
        "Carries a 32 B LayoutFontMetrics COPY per glyph, which is the same \
         for every glyph of a font - the obvious shrink candidate."
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
    assert_size!(StyledRun, 56, "Text + Arc<StyleProperties> + source ids.");
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
        1176,
        "Per layout node, and the BIGGEST per-node struct by far - 1329 nodes \
         is 1.5 MB before a single glyph is shaped. Dominated by the taffy \
         measurement cache and the inline Option<CachedInlineLayout>."
    );
    assert_size!(LayoutNodeCold, 280, "Per layout node, rarely touched.");

}
