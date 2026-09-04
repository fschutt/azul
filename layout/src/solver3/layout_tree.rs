//! Layout tree construction from a styled DOM, including anonymous box generation
use std::{
    cell::Cell,
    collections::BTreeMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

use azul_core::{
    diff::NodeDataFingerprint,
    spaces::{ContentInset, Inclusivity},
};

use crate::text3::cache::UnifiedConstraints;

thread_local! {
    /// Per-thread counter for IFC IDs, reset to 0 at the start of each layout
    /// pass (see [`IfcId::reset_counter`]).
    ///
    /// This was previously a process-global `AtomicU32`. Two `layout_document`
    /// calls running concurrently on different threads (the `Sync` layout bound
    /// permits this) shared that single counter, so their IFC IDs interleaved and
    /// collided. A thread-local counter gives each pass its own sequence — a single
    /// pass is single-threaded (`LayoutContext` holds non-`Sync` `RefCell` caches),
    /// so IDs stay deterministic and stable across frames while never colliding
    /// across concurrent passes.
    static IFC_ID_COUNTER: Cell<u32> = const { Cell::new(0) };
}

/// Unique identifier for an Inline Formatting Context (IFC).
///
/// An IFC represents a region where inline content (text, inline-blocks, images)
/// is laid out together. One IFC can contain content from multiple DOM nodes
/// (e.g., `<p>Hello <span>world</span>!</p>` is one IFC with 3 text runs).
///
/// The ID is generated using a per-thread counter that resets at the start
/// of each layout pass. This ensures:
/// - IDs are unique within a layout pass
/// - The same logical IFC gets the same ID across frames (for selection stability)
/// - Concurrent `layout_document` passes on different threads can't collide
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IfcId(pub u32);

impl IfcId {
    /// Generate a new unique IFC ID (within the current thread's layout pass).
    #[must_use]
    pub fn unique() -> Self {
        IFC_ID_COUNTER.with(|c| {
            let v = c.get();
            c.set(v.wrapping_add(1));
            Self(v)
        })
    }

    /// Reset the IFC ID counter. Called at the start of each layout pass.
    pub fn reset_counter() {
        IFC_ID_COUNTER.with(|c| c.set(0));
    }
}

/// Tracks a layout node's membership in an Inline Formatting Context.
///
/// Text nodes don't store their own `inline_layout_result` - instead, they
/// participate in their parent's IFC. This struct provides the link from
/// a text node back to its IFC's layout data.
///
/// # Architecture
///
/// ```text
/// DOM:  <p>Hello <span>world</span>!</p>
///
/// Layout Tree:
/// ├── LayoutNode (p) - IFC root
/// │   └── inline_layout_result: Some(UnifiedLayout)
/// │   └── ifc_id: IfcId(5)
/// │
/// ├── LayoutNode (::text "Hello ")
/// │   └── ifc_membership: Some(IfcMembership { ifc_id: 5, run_index: 0 })
/// │
/// ├── LayoutNode (span)
/// │   └── ifc_membership: Some(IfcMembership { ifc_id: 5, run_index: 1 })
/// │   └── LayoutNode (::text "world")
/// │       └── ifc_membership: Some(IfcMembership { ifc_id: 5, run_index: 1 })
/// │
/// └── LayoutNode (::text "!")
///     └── ifc_membership: Some(IfcMembership { ifc_id: 5, run_index: 2 })
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IfcMembership {
    /// The IFC ID this node's content was laid out in.
    pub ifc_id: IfcId,
    /// The index of the IFC root `LayoutNode` in the layout tree.
    /// Used to quickly find the node with `inline_layout_result`.
    pub ifc_root_layout_index: usize,
    /// Which run index within the IFC corresponds to this node's text.
    /// Maps to `ContentIndex::run_index` in the shaped items.
    pub run_index: u32,
}

use azul_core::{
    dom::{FormattingContext, NodeData, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    styled_dom::StyledDom,
};
use azul_css::{
    codegen::format::GetHash,
    corety::LayoutDebugMessage,
    css::CssPropertyValue,
    props::{
        basic::{
            pixel::DEFAULT_FONT_SIZE, PhysicalSize, PixelValue, PropertyContext, ResolutionContext,
        },
        layout::{
            LayoutDisplay, LayoutFloat, LayoutHeight, LayoutMaxHeight, LayoutMaxWidth,
            LayoutMinHeight, LayoutMinWidth, LayoutOverflow, LayoutPosition, LayoutWidth,
            LayoutWritingMode,
        },
        property::{CssProperty, CssPropertyType},
        style::{StyleTextAlign, StyleWhiteSpace},
    },
};
use taffy::{Cache as TaffyCache, Layout, LayoutInput, LayoutOutput};

#[cfg(feature = "text_layout")]
use crate::text3;
use crate::{
    debug_log,
    font::parsed::ParsedFont,
    font_traits::{FontLoaderTrait, ParsedFontTrait, UnifiedLayout},
    solver3::{
        geometry::{BoxProps, IntrinsicSizes, PositionedRectangle},
        getters::{
            get_css_height, get_css_max_height, get_css_max_width, get_css_min_height,
            get_css_min_width, get_css_width, get_direction_property as get_direction,
            get_display_property, get_float, get_overflow_x, get_overflow_y, get_position,
            get_text_align, get_text_orientation_property as get_text_orientation,
            get_white_space_property, get_writing_mode, MultiValue,
        },
        scrollbar::ScrollbarRequirements,
        LayoutContext, Result,
    },
    text3::cache::AvailableSpace,
};

/// Represents the invalidation state of a layout node.
///
/// The states are ordered by severity, allowing for easy "upgrading" of the dirty state.
/// A node marked for `Layout` does not also need to be marked for `Paint`.
///
/// Because this enum derives `PartialOrd` and `Ord`, you can directly compare variants:
///
/// - `DirtyFlag::Layout > DirtyFlag::Paint` is `true`
/// - `DirtyFlag::Paint >= DirtyFlag::None` is `true`
/// - `DirtyFlag::Paint < DirtyFlag::Layout` is `true`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum DirtyFlag {
    /// The node's layout is valid and no repaint is needed. This is the "clean" state.
    #[default]
    None,
    /// The node's geometry is valid, but its appearance (e.g., color) has changed.
    /// Requires a display list update only.
    Paint,
    /// The node's geometry (size or position) is invalid.
    /// Requires a full layout pass and a display list update.
    Layout,
}

/// A hash that represents the content and style of a node PLUS all of its descendants.
/// If two `SubtreeHashes` are equal, their entire subtrees are considered identical for layout
/// purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct SubtreeHash(pub u64);

/// Per-item metrics cached from the last IFC layout.
///
/// These metrics enable incremental IFC relayout (Phase 2 optimization):
/// when a single inline item changes, we can check whether its advance width
/// changed and potentially skip full line-breaking for unaffected lines.
///
/// Index in `CachedInlineLayout::item_metrics` matches the item order in
/// `UnifiedLayout::items`.
#[derive(Copy, Debug, Clone)]
pub struct InlineItemMetrics {
    /// The DOM `NodeId` of the source node for this item (for dirty checking).
    /// `None` for generated content (list markers, hyphens, etc.)
    pub source_node_id: Option<NodeId>,
    /// Advance width of this item (glyph run width, inline-block width, etc.)
    pub advance_width: f32,
    /// Advance height contribution from this item to its line box.
    pub line_height_contribution: f32,
    /// Whether this item can participate in line breaking.
    /// `false` for items inside `white-space: nowrap` or `white-space: pre`.
    pub can_break: bool,
    /// Which line this item was placed on (0-indexed).
    pub line_index: u32,
    /// X offset within its line.
    pub x_offset: f32,
}

/// Cached inline layout result with the constraints used to compute it.
///
/// This structure solves a fundamental architectural problem: inline layouts
/// (text wrapping, inline-block positioning) depend on the available width.
/// Different layout phases may compute the layout with different widths:
///
/// 1. **Min-content measurement**: width = `MinContent` (effectively 0)
/// 2. **Max-content measurement**: width = `MaxContent` (effectively infinite)
/// Cached output of `collect_and_measure_inline_content` (see
/// `LayoutNodeWarm::inline_content_cache`).
#[derive(Debug, Clone)]
pub struct CachedInlineContent {
    /// The collected inline content (text runs, atomics, markers).
    pub content: Vec<crate::text3::cache::InlineContent>,
    /// `ContentIndex` -> child layout-node index, as collection built it.
    pub child_map: std::collections::HashMap<crate::text3::cache::ContentIndex, usize>,
    /// Validity key: the fingerprints of the IFC root and its descendants,
    /// folded in tree order. Anything that changes text, style or structure
    /// changes a fingerprint, so an equal key means an identical collection.
    pub subtree_fingerprint: u64,
    /// Hash of `content` alone, computed ONCE when the collection is (re)built.
    ///
    /// The per-visit "current content hash" used to re-hash the full inline
    /// content on EVERY `layout_ifc` entry — 2 292 hashes × ~12.7 µs ≈ 29 ms
    /// per resize relayout on big.md, hashing bytes that by definition had not
    /// changed (an equal `subtree_fingerprint` is what admitted the cached
    /// collection in the first place). Visits now fold only the seven
    /// constraint-level container properties on top of this base.
    pub content_hash_base: u64,
}

/// 3. **Final layout**: width = `Definite(actual_column_width)`
///
/// Without tracking which constraints were used, a cached result from phase 1
/// would incorrectly be reused in phase 3, causing text to wrap at the wrong
/// positions (the root cause of table cell width bugs).
///
/// By storing the constraints alongside the result, we can:
/// - Invalidate the cache when constraints change
/// - Keep multiple cached results for different constraint types if needed
/// - Ensure the final render always uses a layout computed with correct widths
#[derive(Debug, Clone)]
pub struct CachedInlineLayout {
    /// The computed inline layout.
    ///
    /// (d6h RETIREMENT) Under `AZ_DENSE_TEXT=1`, when the dense view is
    /// retained AND covers every item (pure clusters), this holds a
    /// SHARED EMPTY sentinel instead of the real thing — the ~192 B/item
    /// sparse retention is the campaign's target. Every production
    /// reader is dense-first by d1-d6h; `verify` mode retains BOTH and
    /// A/B-asserts; flag-off is unchanged. `overflow` (below) carries
    /// the one field the sentinel loses.
    pub layout: Arc<UnifiedLayout>,
    /// (d6h) The real layout's overflow info, captured at store time —
    /// the incremental-relayout guard reads it after the sentinel swap.
    pub overflow: crate::text3::cache::OverflowInfo,
    /// The available width constraint used to compute this layout.
    /// This is the key for cache validity checking.
    /// +spec:writing-modes:1dcba2 - "available width" (CSS2.1) = auto size in inline axis
    pub available_width: AvailableSpace,
    /// Whether this layout was computed with float exclusions.
    /// Float-aware layouts should not be overwritten by non-float layouts.
    pub has_floats: bool,
    /// The full constraints used to compute this layout.
    /// Used for quick relayout after text edits without rebuilding from CSS.
    pub constraints: Option<UnifiedConstraints>,
    /// Glyph runs derived from `layout` at STORE time (a pure function of
    /// it — see `get_glyph_runs_simple`). The display-list generator used to
    /// re-derive these on EVERY paint of every IFC (the run-grouping walk was
    /// the bulk of `dl_inline_text`'s 6.8 ms/resize on big.md); computing them
    /// once alongside the layout makes every later paint a lookup. Rides
    /// `GlyphSwap` reuse for free — a reused layout is a reused run list.
    ///
    /// (#25) Stored COMPACT — 8 B/glyph instead of the 20 B `GlyphInstance`
    /// (y and size are per-run-uniform in the simple-run contract; deviants
    /// ride the exception table). Paint expands into the same offset Vec it
    /// always built; `AZ_DENSE_TEXT=verify` roundtrip-gates the encoding.
    pub glyph_runs: Arc<Vec<crate::text3::glyphs::CompactGlyphRun>>,
    /// Per-item metrics for incremental IFC relayout (Phase 2).
    ///
    /// Each entry corresponds to one `PositionedItem` in `layout.items`.
    /// These metrics enable the IFC relayout decision tree:
    /// - Check if a dirty node's `advance_width` changed → skip repositioning if not
    /// - Use `can_break` + `line_index` for the nowrap fast path
    /// - Use `x_offset` for shifting subsequent items without full line-breaking
    pub item_metrics: Vec<InlineItemMetrics>,
    /// Cached line break boundaries for incremental relayout.
    /// Enables checking if a width change fits on the same line without
    /// re-running the full line-breaking algorithm.
    pub line_breaks: Option<crate::text3::cache::CachedLineBreaks>,
    /// §3.2 step (d5): the ONE cache-stable Arc the display list carries
    /// as its `TextLayout` payload. `TextPayload{dense,sparse}` when the
    /// dense view is retained, else the `layout` Arc itself (identical
    /// to the pre-d5 payload). Built at store time so damage `ptr_eq`
    /// stays stable across paints.
    pub payload: Arc<dyn std::any::Any + Send + Sync>,
    /// §3.2 step (d1): the dense view of `layout`, retained when
    /// `AZ_DENSE_TEXT` is active (`glyph_runs` derive from THIS instance).
    /// Readers migrate from `layout` onto it one by one; the sparse form
    /// retires when the last reader has moved (plan §10.4).
    pub dense: Option<Arc<crate::text3::dense::DenseText>>,
    /// Hash of the `InlineContent` this layout was shaped from. The Phase 2d
    /// fast-path reuse in fc.rs keys cache validity on WIDTH only; without this,
    /// a same-width `RefreshDom` whose text CHANGED would reuse the stale shaped
    /// layout (#11 stale display list). 0 = unknown ⇒ never fast-path-reuse.
    pub inline_content_hash: u64,
    /// The STYLE environment this layout was shaped under (fonts, sizes,
    /// spacing — not paint-only fields), written only by the fast reshape
    /// path (`LayoutWindow::shaping_style_fingerprint`). The incremental
    /// patcher pastes new glyphs onto CACHED pen positions, which is only
    /// sound when both were shaped under the same style; 0 = unknown
    /// (full-layout caches) ⇒ it re-runs stage 4 instead. Pinned by the
    /// 2026-08-29 `TextInput` bug: a 16px-seeded reshape met an 11px cache
    /// and pasted large glyphs onto short advances every other frame.
    pub shaping_style_fp: u64,
}

/// §3.2 step (d5): the display list's `TextLayout` payload when the dense
/// view is retained — BOTH forms behind one cache-stable Arc, so
/// `TextLayout` damage diffing (`Arc::ptr_eq`) keeps firing exactly as
/// before. Downcasters try this first and fall back to a bare
/// `UnifiedLayout` payload (the flag-off path). The `sparse` half is
/// what current consumers read; it empties at the d6 retirement, which
/// is when the consumers move onto `dense`.
#[derive(Debug)]
pub struct TextPayload {
    pub dense: Arc<crate::text3::dense::DenseText>,
    pub sparse: Arc<UnifiedLayout>,
}

/// §3.2 flip step (a): the paint-run computation, routed by `AZ_DENSE_TEXT`.
///
/// - unset/`0` — the sparse reference walker (production default).
/// - `1`      — build [`crate::text3::dense::DenseText`] and walk THAT
///   (`get_glyph_runs_simple_dense`), the compact model's first production
///   consumer.
/// - `verify` — dense path PLUS an A/B assert against the reference; the
///   e2e corpus run with this set is the flip proof over real layouts.
///
/// Layouts containing combined blocks (tate-chu-yoko) fall back to the
/// reference under the flag: the dense model keeps atomics on the sparse
/// side by design and the twin would silently DROP their glyphs.
fn compute_glyph_runs(
    layout: &UnifiedLayout,
) -> (
    Vec<crate::text3::glyphs::SimpleGlyphRun>,
    Option<Arc<crate::text3::dense::DenseText>>,
) {
    compute_glyph_runs_with_mode(layout, dense_text_mode())
}

/// The process-wide `AZ_DENSE_TEXT` mode: 0 off, 1 on, 2 verify.
///
/// (d7) DEFAULT ON: the dense model is the stored form in production —
/// the retirement (d6h) plateaued at 65.2 MB heap vs 73.6 flag-off on
/// the 960-line corpus, with the full battery + flag-on corpus +
/// flag-on reftests green. `AZ_DENSE_TEXT=0` is the escape hatch;
/// `verify` retains both forms and A/B-asserts every dense-first arm.
pub(crate) fn dense_text_mode() -> u8 {
    static MODE: std::sync::OnceLock<u8> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("AZ_DENSE_TEXT").as_deref() {
        Ok("0" | "off") => 0,
        Ok("verify") => 2,
        _ => 1,
    })
}

/// #25: converts the transient builder output into the stored compact form.
/// Under `AZ_DENSE_TEXT=verify` every run is roundtripped and compared
/// BIT-EXACTLY against its original — the same proof obligation the dense
/// campaign carried: the stored form must be an encoding, never a rewrite.
fn compact_glyph_runs(
    runs: Vec<crate::text3::glyphs::SimpleGlyphRun>,
) -> Vec<crate::text3::glyphs::CompactGlyphRun> {
    let verify = dense_text_mode() == 2;
    runs.into_iter()
        .map(|r| {
            if verify {
                let compact = crate::text3::glyphs::CompactGlyphRun::from(r.clone());
                assert!(
                    crate::text3::glyphs::simple_runs_bit_equal(&compact.expand(), &r),
                    "#25 compact glyph-run roundtrip diverged from the original"
                );
                compact
            } else {
                crate::text3::glyphs::CompactGlyphRun::from(r)
            }
        })
        .collect()
}

/// Testable core of [`compute_glyph_runs`] — `mode` injected so the d1
/// retention pin can exercise the dense path without env-var races.
fn compute_glyph_runs_with_mode(
    layout: &UnifiedLayout,
    mode: u8,
) -> (
    Vec<crate::text3::glyphs::SimpleGlyphRun>,
    Option<Arc<crate::text3::dense::DenseText>>,
) {
    use crate::text3::cache::ShapedItem;
    if mode == 0
        || layout
            .items
            .iter()
            .any(|it| matches!(it.item, ShapedItem::CombinedBlock { .. }))
    {
        return (crate::text3::glyphs::get_glyph_runs_simple(layout), None);
    }
    // §3.2 step (d1): the dense view is RETAINED on the cache entry, not
    // rebuilt-and-dropped per walk — the readers migrate onto it one by
    // one, and the sparse `layout` retires when the last one has.
    let dense = crate::text3::dense::DenseText::from_unified(layout);
    let ours = crate::text3::dense::get_glyph_runs_simple_dense(&dense);
    if mode == 2 {
        let reference = crate::text3::glyphs::get_glyph_runs_simple(layout);
        assert_eq!(
            reference.len(),
            ours.len(),
            "AZ_DENSE_TEXT=verify: run count diverged"
        );
        for (i, (r, o)) in reference.iter().zip(ours.iter()).enumerate() {
            assert!(
                r.color == o.color
                    && r.background_color == o.background_color
                    && r.background_content == o.background_content
                    && r.border == o.border
                    && r.font_hash == o.font_hash
                    && (r.font_size_px - o.font_size_px).abs() < 0.01
                    && r.text_decoration == o.text_decoration
                    && r.source_node_id == o.source_node_id
                    && r.glyphs.len() == o.glyphs.len()
                    && r.glyphs.iter().zip(o.glyphs.iter()).all(|(a, b)| {
                        a.index == b.index
                            && (a.point.x - b.point.x).abs() < 0.01
                            && (a.point.y - b.point.y).abs() < 0.01
                    }),
                "AZ_DENSE_TEXT=verify: run {i} diverged from the reference\n  \
                 color {:?}/{:?} bg {:?}/{:?} font_hash {:?}/{:?} size {}/{} \
                 deco {:?}/{:?} node {:?}/{:?}\n  glyph count {} vs {}\n  \
                 ref glyphs {:?}\n  our glyphs {:?}",
                r.color,
                o.color,
                r.background_color,
                o.background_color,
                r.font_hash,
                o.font_hash,
                r.font_size_px,
                o.font_size_px,
                r.text_decoration,
                o.text_decoration,
                r.source_node_id,
                o.source_node_id,
                r.glyphs.len(),
                o.glyphs.len(),
                r.glyphs
                    .iter()
                    .map(|g| (g.index, g.point.x, g.point.y))
                    .collect::<Vec<_>>(),
                o.glyphs
                    .iter()
                    .map(|g| (g.index, g.point.x, g.point.y))
                    .collect::<Vec<_>>(),
            );
        }
    }
    (ours, Some(Arc::new(dense)))
}

/// §3.2 step (d2): per-item metrics FROM the dense view. Returns None
/// unless the layout was PURE clusters (`dense.clusters.len()` must
/// equal `layout.items.len()` — the dense builder silently skips
/// objects/tabs/breaks/combined blocks, and `item_metrics` is
/// index-aligned with ALL items, so a shorter vec would silently
/// misalign the incremental-relayout decision tree).
///
/// Field-for-field vs the sparse extraction (the d2 gate pins this):
/// `advance_width` = ClusterCompact.advance (BASE advance == bounds().width
/// since the d2 redefinition), `line_height_contribution` = the run's
/// resolved line height (== ascent+descent by the half-leading identity),
/// `can_break` = true (clusters are never `ShapedItem::Break`), `line_index` =
/// `LineRecord.source_index`, `x_offset` = ClusterCompact.x.
fn extract_item_metrics_dense(
    dense: &crate::text3::dense::DenseText,
    item_count: usize,
) -> Option<Vec<InlineItemMetrics>> {
    if dense.clusters.len() != item_count {
        return None;
    }
    let mut out = Vec::with_capacity(dense.clusters.len());
    let mut line_iter = dense.lines.iter().peekable();
    for (ci, run) in dense
        .runs
        .iter()
        .flat_map(|r| (r.clusters.start..r.clusters.end).map(move |i| (i, r)))
    {
        let c = &dense.clusters[ci as usize];
        while let Some(l) = line_iter.peek() {
            if ci >= l.clusters.1 {
                line_iter.next();
            } else {
                break;
            }
        }
        let line_index = line_iter.peek().map_or(0, |l| l.source_index);
        let m = &run.font_metrics;
        let lh = if m.units_per_em == 0 {
            0.0
        } else {
            run.style
                .line_height
                .resolve_with_metrics(run.style.font_size_px, m)
        };
        out.push(InlineItemMetrics {
            source_node_id: (run.source_node != u32::MAX)
                .then(|| NodeId::new(run.source_node as usize)),
            advance_width: c.advance,
            line_height_contribution: lh,
            can_break: true,
            line_index,
            x_offset: c.x,
        });
    }
    Some(out)
}

impl CachedInlineLayout {
    /// Creates a new cached inline layout.
    #[must_use]
    pub fn new(
        layout: Arc<UnifiedLayout>,
        available_width: AvailableSpace,
        has_floats: bool,
    ) -> Self {
        let (runs, dense) = compute_glyph_runs(&layout);
        let glyph_runs = Arc::new(compact_glyph_runs(runs));
        let item_metrics = Self::choose_item_metrics(&layout, dense.as_deref());
        let (layout, overflow) = Self::retire_sparse(layout, dense.as_deref());
        // Payload built from the STORED form — under the retirement the
        // sparse half is the sentinel and consumers expand from dense.
        let payload = Self::build_payload(&layout, dense.as_ref());
        Self {
            layout,
            overflow,
            available_width,
            has_floats,
            constraints: None,
            payload,
            glyph_runs,
            item_metrics,
            line_breaks: None,
            dense,
            inline_content_hash: 0,
            shaping_style_fp: 0,
        }
    }

    /// Creates a new cached inline layout with full constraints.
    #[must_use]
    pub fn new_with_constraints(
        layout: Arc<UnifiedLayout>,
        available_width: AvailableSpace,
        has_floats: bool,
        constraints: UnifiedConstraints,
    ) -> Self {
        let available_width_px = match available_width {
            AvailableSpace::Definite(w) => w,
            _ => f32::MAX,
        };
        let line_breaks = Some(crate::text3::cache::extract_line_breaks(
            &layout.items,
            available_width_px,
        ));
        let (runs, dense) = compute_glyph_runs(&layout);
        let glyph_runs = Arc::new(compact_glyph_runs(runs));
        let item_metrics = Self::choose_item_metrics(&layout, dense.as_deref());
        let (layout, overflow) = Self::retire_sparse(layout, dense.as_deref());
        // Payload built from the STORED form — under the retirement the
        // sparse half is the sentinel and consumers expand from dense.
        let payload = Self::build_payload(&layout, dense.as_ref());
        Self {
            layout,
            overflow,
            available_width,
            has_floats,
            constraints: Some(constraints),
            payload,
            glyph_runs,
            item_metrics,
            line_breaks,
            dense,
            inline_content_hash: 0,
            shaping_style_fp: 0,
        }
    }

    /// (d6h) The stored layout, MATERIALIZED if it is the retirement
    /// sentinel: dense expands (exact by the d6h expansion gate) into a
    /// transient sparse copy. The universal accessor for readers that
    /// measure or walk content; flag-off / verify / non-pure entries
    /// return the stored Arc untouched.
    #[must_use]
    pub fn materialized(&self) -> Arc<UnifiedLayout> {
        if self.layout.items.is_empty() {
            if let Some(d) = self.dense.as_deref() {
                if !d.clusters.is_empty() {
                    return Arc::new(UnifiedLayout {
                        items: d.to_unified_items(),
                        overflow: self.overflow.clone(),
                    });
                }
            }
        }
        self.layout.clone()
    }

    /// (d6h) THE RETIREMENT: under `AZ_DENSE_TEXT=1`, a pure-cluster
    /// layout whose dense view is retained stores a SHARED EMPTY
    /// sentinel in `layout` — the per-item sparse retention (the
    /// campaign's ~192 B/cluster target) drops here. The real overflow
    /// info is captured first (the incremental guard reads it).
    /// `verify` mode (2) retains BOTH so every dense-first arm keeps
    /// A/B-asserting; flag-off (0) is byte-for-byte unchanged.
    fn retire_sparse(
        layout: Arc<UnifiedLayout>,
        dense: Option<&crate::text3::dense::DenseText>,
    ) -> (Arc<UnifiedLayout>, crate::text3::cache::OverflowInfo) {
        let overflow = layout.overflow.clone();
        // Empty layouts are skipped: nothing to save, and the identity
        // swap would be pure Arc churn (unit fixtures rely on ptr_eq).
        let fully_covered = !layout.items.is_empty()
            && dense.is_some_and(|d| d.clusters.len() == layout.items.len());
        if dense_text_mode() == 1 && fully_covered {
            static EMPTY: std::sync::OnceLock<Arc<UnifiedLayout>> = std::sync::OnceLock::new();
            let sentinel = EMPTY
                .get_or_init(|| {
                    Arc::new(UnifiedLayout {
                        items: Vec::new(),
                        overflow: crate::text3::cache::OverflowInfo::default(),
                    })
                })
                .clone();
            return (sentinel, overflow);
        }
        (layout, overflow)
    }

    /// (d5) The cache-stable DL payload — see the `payload` field.
    fn build_payload(
        layout: &Arc<UnifiedLayout>,
        dense: Option<&Arc<crate::text3::dense::DenseText>>,
    ) -> Arc<dyn std::any::Any + Send + Sync> {
        if let Some(d) = dense {
            Arc::new(TextPayload {
                dense: d.clone(),
                sparse: layout.clone(),
            })
        } else {
            let shared: Arc<dyn std::any::Any + Send + Sync> = layout.clone();
            shared
        }
    }

    /// (d2) Metrics source selection: dense when retained AND the layout
    /// is pure clusters; sparse otherwise. Under `AZ_DENSE_TEXT=verify` the
    /// dense result is A/B-asserted against the sparse extraction.
    fn choose_item_metrics(
        layout: &UnifiedLayout,
        dense: Option<&crate::text3::dense::DenseText>,
    ) -> Vec<InlineItemMetrics> {
        let Some(d) = dense else {
            return Self::extract_item_metrics(layout);
        };
        let Some(from_dense) = extract_item_metrics_dense(d, layout.items.len()) else {
            return Self::extract_item_metrics(layout);
        };
        let verify = std::env::var("AZ_DENSE_TEXT").as_deref() == Ok("verify");
        if verify {
            let reference = Self::extract_item_metrics(layout);
            assert_eq!(
                reference.len(),
                from_dense.len(),
                "d2 verify: metrics count"
            );
            for (i, (r, o)) in reference.iter().zip(from_dense.iter()).enumerate() {
                assert!(
                    r.source_node_id == o.source_node_id
                        && (r.advance_width - o.advance_width).abs() < 0.01
                        && (r.line_height_contribution - o.line_height_contribution).abs() < 0.01
                        && r.can_break == o.can_break
                        && r.line_index == o.line_index
                        && (r.x_offset - o.x_offset).abs() < 0.01,
                    "d2 verify: metrics @{i} diverged (ref {r:?} vs dense {o:?})"
                );
            }
        }
        from_dense
    }

    /// Extracts per-item metrics from a computed `UnifiedLayout`.
    ///
    /// This is called automatically by the constructors. The metrics
    /// enable incremental IFC relayout in Phase 2c/2d by providing
    /// cached advance widths, line assignments, and break information
    /// for each positioned item.
    #[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
    fn extract_item_metrics(layout: &UnifiedLayout) -> Vec<InlineItemMetrics> {
        use crate::text3::cache::{get_item_vertical_metrics_approx, ShapedItem};

        layout
            .items
            .iter()
            .map(|positioned_item| {
                let bounds = positioned_item.item.bounds();
                let (ascent, descent) = get_item_vertical_metrics_approx(&positioned_item.item);

                let source_node_id = match &positioned_item.item {
                    ShapedItem::Cluster(c) => c.source_node_id,
                    // Objects (inline-blocks, images) and other generated items
                    // don't expose source_node_id directly on ShapedItem.
                    // Phase 2c will refine this via the ContentIndex mapping.
                    ShapedItem::Object { .. }
                    | ShapedItem::CombinedBlock { .. }
                    | ShapedItem::Tab { .. }
                    | ShapedItem::Break { .. } => None,
                };

                // For Phase 2a, default can_break = true for all items.
                // Phase 2c will refine this by checking the white-space property
                // on the IFC root's style or the item's own style context.
                // (Note: text3::StyleProperties doesn't carry white-space;
                //  that's resolved at the IFC/BFC boundary level.)
                let can_break = !matches!(&positioned_item.item, ShapedItem::Break { .. });

                InlineItemMetrics {
                    source_node_id,
                    advance_width: bounds.width,
                    line_height_contribution: ascent + descent,
                    can_break,
                    line_index: positioned_item.line_index as u32,
                    x_offset: positioned_item.position.x,
                }
            })
            .collect()
    }

    /// Checks if this cached layout is valid for the given constraints.
    ///
    /// A cached layout is valid if:
    /// 1. The available width matches (definite widths must be equal, or both are the same
    ///    indefinite type)
    /// 2. OR the new request doesn't have floats but the cached one does (keep float-aware layout)
    ///
    /// The second condition preserves float-aware layouts, which are more "correct" than
    /// non-float layouts and shouldn't be overwritten.
    #[must_use]
    pub fn is_valid_for(&self, new_width: AvailableSpace, new_has_floats: bool) -> bool {
        // A cached layout with NO floats must not be reused when the new request DOES
        // have floats: the line boxes have to re-wrap around the float exclusions.
        // Without this, the two-pass IFC path (no-float sizing pass, then float-aware
        // re-layout) reuses the pass-1 no-float line breaks and text ignores the float
        // entirely (#19). Mirrors should_replace_with()'s gain-float branch so the two
        // stay consistent.
        if new_has_floats && !self.has_floats {
            return false;
        }

        // If we have a float-aware layout and the new request doesn't have floats,
        // keep the float-aware layout (it's more accurate)
        if self.has_floats && !new_has_floats {
            // But only if the width constraint type matches
            return self.width_constraint_matches(new_width);
        }

        // Otherwise, require exact width match
        self.width_constraint_matches(new_width)
    }

    /// Tolerance for comparing definite layout widths (in logical pixels).
    /// Sub-pixel differences below this threshold are treated as identical
    /// to avoid unnecessary relayout from floating-point rounding.
    const LAYOUT_WIDTH_EPSILON: f32 = 0.1;

    /// Checks if the width constraint matches.
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    fn width_constraint_matches(&self, new_width: AvailableSpace) -> bool {
        match (self.available_width, new_width) {
            // Definite widths must match within a small epsilon
            (AvailableSpace::Definite(old), AvailableSpace::Definite(new)) => {
                (old - new).abs() < Self::LAYOUT_WIDTH_EPSILON
            }
            // MinContent matches MinContent
            (AvailableSpace::MinContent, AvailableSpace::MinContent) => true,
            // MaxContent matches MaxContent
            (AvailableSpace::MaxContent, AvailableSpace::MaxContent) => true,
            // Different constraint types don't match
            _ => false,
        }
    }

    /// Determines if this cached layout should be replaced by a new layout.
    ///
    /// Returns true if the new layout should replace this one.
    #[must_use]
    pub fn should_replace_with(&self, new_width: AvailableSpace, new_has_floats: bool) -> bool {
        // Always replace if we gain float information
        if new_has_floats && !self.has_floats {
            return true;
        }

        // Replace if width constraint changed
        !self.width_constraint_matches(new_width)
    }

    /// Returns a reference to the inner `UnifiedLayout`.
    ///
    /// This is a convenience method for code that only needs the layout data
    /// and doesn't care about the caching metadata.
    #[inline]
    #[must_use]
    pub const fn get_layout(&self) -> &Arc<UnifiedLayout> {
        &self.layout
    }

    /// Returns a clone of the inner Arc<UnifiedLayout>.
    ///
    /// This is useful for APIs that need to return an owned reference
    /// to the layout without exposing the caching metadata.
    #[inline]
    #[must_use]
    pub fn clone_layout(&self) -> Arc<UnifiedLayout> {
        self.layout.clone()
    }
}

/// A layout tree node representing the CSS box model.
///
/// ## Memory Layout Optimization (`#[repr(C)]`)
///
/// Fields are ordered by access frequency (hottest first) to maximize CPU
/// cache line utilization during tree traversal. With `#[repr(C)]`, the
/// compiler preserves this ordering. The 6 hottest fields (~140 bytes)
/// occupy the first 2-3 cache lines (64 bytes each), which are loaded
/// first by the hardware prefetcher.
///
/// | Tier   | Fields                                  | ~Bytes | Accesses |
/// |--------|-----------------------------------------|--------|----------|
/// | HOT    | `box_props`, `dom_node_id`, children,       |  ~140  |  410+    |
/// |        | `used_size`, `formatting_context`, parent    |        |          |
/// | WARM   | `intrinsic_sizes..computed_style`          |  ~220  |  ~80     |
/// | COLD   | `dirty_flag..is_anonymous`                 |  ~190  |  ~20     |
///
/// Note: An absolute position is a final paint-time value and shouldn't be
/// cached on the node itself, as it can change even if the node's
/// layout is clean (e.g., if a sibling changes size). We will calculate
/// it in a separate map.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct LayoutNode {
    // ── HOT tier: accessed on every node in every layout pass ────────────
    // These fields should fit in the first 2-3 cache lines (~128-192 bytes).
    /// The resolved box model properties (margin, border, padding)
    /// in logical pixels. Cached after first resolution.
    /// (148 accesses — hottest field)
    pub box_props: BoxProps,
    /// Reference back to the original DOM node (None for anonymous boxes)
    /// (111 accesses)
    pub dom_node_id: Option<NodeId>,
    /// Children indices in the layout tree
    /// (53 accesses)
    pub children: Vec<usize>,
    /// The size used during the last layout pass.
    /// (43 accesses)
    pub used_size: Option<LogicalSize>,
    /// The formatting context this node establishes or participates in.
    /// (30 accesses)
    pub formatting_context: FormattingContext,
    /// Parent index (None for root)
    /// (25 accesses)
    pub parent: Option<usize>,

    // ── WARM tier: frequently accessed but not on every node ─────────────
    /// Cached intrinsic sizes (min-content, max-content, etc.)
    /// (16 accesses — sizing pass only)
    pub intrinsic_sizes: Option<IntrinsicSizes>,
    // +spec:display-property:af3a89 - alignment baseline for inline-level boxes
    /// The baseline of this box, if applicable, measured from its content-box top edge.
    /// (14 accesses — IFC/table alignment)
    pub baseline: Option<f32>,
    /// Cached inline layout result with the constraints used to compute it.
    ///
    /// This field stores both the computed layout AND the constraints (available width,
    /// float state) under which it was computed. This is essential for correctness:
    ///
    /// - Table cells are measured multiple times with different widths
    /// - Min-content/max-content intrinsic sizing uses special constraint values
    /// - The final layout must use the actual available width, not a measurement width
    ///
    /// By tracking the constraints, we avoid the bug where a min-content measurement
    /// (with width=0) would be incorrectly reused for final rendering.
    /// (13 accesses — IFC roots / table cells)
    /// BOXED (rare-data split, 2026-08-11): populated only on IFC roots /
    /// table cells; the 296-B inline Option was paid by EVERY node.
    pub inline_layout_result: Option<Box<CachedInlineLayout>>,
    /// See [`LayoutNodeWarm::inline_content_cache`]. BOXED likewise (88 B).
    pub inline_content_cache: Option<Box<CachedInlineContent>>,
    /// Cached scrollbar information (calculated during layout)
    /// Used to determine if scrollbars appeared/disappeared requiring reflow
    /// (12 accesses — scrollable containers only)
    pub scrollbar_info: Option<ScrollbarRequirements>,
    /// The position of this node *relative to its parent's content box*.
    /// (9 accesses — positioning pass)
    pub relative_position: Option<LogicalPosition>,
    /// The actual content size (children overflow size) for scrollable containers.
    /// This is the size of all content that might need to be scrolled, which can
    /// be larger than `used_size` when content overflows the container.
    /// (7 accesses — scrollable containers)
    pub overflow_content_size: Option<LogicalSize>,
    /// Cache for Taffy layout computations for this node.
    /// (6 accesses — Taffy bridge)
    pub taffy_cache: TaffyCache,
    /// Pure min-content / max-content measure results from the taffy bridge
    /// (`(min, max)` outer sizes), keyed by NOTHING — a pure content
    /// measurement is viewport-independent, so it stays valid until the
    /// node's content changes. Invalidated wherever `taffy_cache` is
    /// (the intrinsic-dirty ancestor closure). WHY separate from
    /// `taffy_cache`: taffy's slot classes collide under the flex
    /// algorithm's candidate-width probing (Definite(0)/Definite(w1)/…
    /// share a slot and evict each other within one pass), so min/max
    /// entries did not survive to the NEXT pass — 312 full min/max-content
    /// subtree re-layouts per steady resize on big.md, each re-breaking
    /// every IFC line in the subtree AND evicting the Definite entry of
    /// the single-slot IFC cache on the way (the 429 `text_layout_flow`
    /// calls). (2 accesses — Taffy bridge measure path)
    pub measured_content_sizes: (Option<LayoutOutput>, Option<LayoutOutput>),
    /// Pre-computed CSS properties needed during layout.
    /// Computed once during layout tree build to avoid repeated style lookups.
    /// (5 accesses — cache.rs only)
    pub computed_style: ComputedLayoutStyle,
    /// Pseudo-element type (`::marker`, `::before`, `::after`) if this node is a pseudo-element
    /// (5 accesses — pseudo-elements only)
    pub pseudo_element: Option<PseudoElement>,
    /// Escaped top margin (CSS 2.1 margin collapsing)
    /// If this BFC's first child's top margin "escaped" the BFC, this contains
    /// the collapsed margin that should be applied by the parent.
    /// (4 accesses — BFC margin collapsing)
    pub escaped_top_margin: Option<f32>,
    /// Escaped bottom margin (CSS 2.1 margin collapsing)\
    /// If this BFC's last child's bottom margin "escaped" the BFC, this contains
    /// the collapsed margin that should be applied by the parent.
    /// (4 accesses)
    pub escaped_bottom_margin: Option<f32>,
    /// Parent's formatting context (needed to determine if stretch applies)
    /// (4 accesses — flex/grid children)
    pub parent_formatting_context: Option<FormattingContext>,
    /// If this node participates in an IFC (is inline content like text),
    /// stores the reference back to the IFC root and the run index.
    /// This allows text nodes to find their layout data in the parent's IFC.
    /// (3 accesses — text nodes only)
    pub ifc_membership: Option<IfcMembership>,
    /// The layout tree index of this node's containing block.
    /// - For abs-pos elements: nearest positioned (non-static) ancestor
    /// - For fixed elements: root / None (viewport)
    /// - For normal-flow: parent (None = implicit)
    ///   Used for clip exemption: abs-pos elements whose containing block
    ///   is above an overflow clipper should not be clipped.
    pub containing_block_index: Option<usize>,

    // ── COLD tier: construction / reconciliation / debugging only ────────
    /// Type of anonymous box (if applicable)
    /// (2 accesses)
    pub anonymous_type: Option<AnonymousBoxType>,
    /// O3-render: when this node is one PART of a structural-split PREVIEW,
    /// the flat byte range of the source node's inline content this part
    /// displays (`u32::MAX` end = to the end). The IFC build slices its
    /// collected content by this range. `None` = the node shows everything
    /// (the universal case outside previews).
    pub preview_byte_range: Option<(u32, u32)>,
    /// Multi-field fingerprint of this node's data (style, text, etc.)
    /// for granular change detection during reconciliation.
    /// (2 accesses — reconciliation only)
    pub node_data_fingerprint: NodeDataFingerprint,
    /// A hash of this node's data and all of its descendants. Used for
    /// fast reconciliation.
    /// (9 accesses — all in cache.rs reconciliation)
    pub subtree_hash: SubtreeHash,
    /// Dirty flags to track what needs recalculation.
    /// (7 accesses — reconciliation setup)
    pub dirty_flag: DirtyFlag,
    /// Unresolved box model properties (raw CSS values).
    /// These are resolved lazily during layout when containing block is known.
    /// (1 access — initial resolution only)
    pub unresolved_box_props: crate::solver3::geometry::UnresolvedBoxProps,
    /// If this node is an IFC root, stores the IFC ID.
    /// Used to identify which IFC this node's `inline_layout_result` belongs to.
    /// (1 access — IFC creation only)
    pub ifc_id: Option<IfcId>,
}

/// Pre-computed CSS properties needed during layout.
///
/// This struct stores resolved CSS values that are frequently accessed during
/// layout calculations. By computing these once during layout tree construction,
/// we avoid O(n * m) style lookups where n = nodes and m = layout passes.
///
/// All values are resolved to their final form (no 'inherit', 'initial', etc.)
#[derive(Debug, Clone, Default)]
pub struct ComputedLayoutStyle {
    /// CSS `display` property
    pub display: LayoutDisplay,
    /// CSS `position` property
    pub position: LayoutPosition,
    /// CSS `float` property
    pub float: LayoutFloat,
    /// CSS `overflow-x` property
    pub overflow_x: LayoutOverflow,
    /// CSS `overflow-y` property
    pub overflow_y: LayoutOverflow,
    /// CSS `writing-mode` property
    pub writing_mode: azul_css::props::layout::LayoutWritingMode,
    /// CSS `direction` property (ltr/rtl)
    pub direction: azul_css::props::style::StyleDirection,
    /// CSS `text-orientation` property (for vertical writing modes)
    pub text_orientation: azul_css::props::style::effects::StyleTextOrientation,
    /// CSS `width` property (None = auto)
    pub width: Option<azul_css::props::layout::LayoutWidth>,
    /// CSS `height` property (None = auto)
    pub height: Option<azul_css::props::layout::LayoutHeight>,
    /// CSS `min-width` property
    pub min_width: Option<azul_css::props::layout::LayoutMinWidth>,
    /// CSS `min-height` property
    pub min_height: Option<azul_css::props::layout::LayoutMinHeight>,
    /// CSS `max-width` property
    pub max_width: Option<azul_css::props::layout::LayoutMaxWidth>,
    /// CSS `max-height` property
    pub max_height: Option<azul_css::props::layout::LayoutMaxHeight>,
    /// CSS `text-align` property
    pub text_align: StyleTextAlign,
}

// Note: LayoutNode methods that cross hot/warm/cold boundaries have been
// moved to LayoutTree methods (resolve_box_props, get_content_size).

/// CSS pseudo-elements that can be generated
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoElement {
    /// `::marker` pseudo-element for list items
    Marker,
    /// `::before` pseudo-element
    Before,
    /// `::after` pseudo-element
    After,
}

// +spec:display-property:b7f4bf - anonymous inline/block boxes are both called "anonymous boxes"
/// Types of anonymous boxes that can be generated
// +spec:display-property:ae4f16 - anonymous boxes are treated as descendants alongside pseudo-elements
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnonymousBoxType {
    /// Anonymous block box wrapping inline content
    InlineWrapper,
    /// O3-render: the SECOND (or later) part of a structural-split preview —
    /// a box with a real `dom_node_id` (the split node, via the
    /// `dom_to_layout` multimap) that displays a byte-slice of its content.
    /// Exists only between recording a split and the app's re-render.
    SplitPreviewPart,
    /// Anonymous box for a list item marker (bullet or number)
    /// DEPRECATED: Use `PseudoElement::Marker` instead
    ListItemMarker,
    /// Anonymous table wrapper
    TableWrapper,
    /// Anonymous table row group (tbody)
    TableRowGroup,
    /// Anonymous table row
    TableRow,
    /// Anonymous table cell
    TableCell,
}

// =============================================================================
// SoA (struct-of-arrays) layout node split for cache performance
// =============================================================================

/// Hot layout node fields — accessed on every node in every layout pass.
///
/// Stored in a separate `Vec` for cache locality. At ~100 bytes per node,
/// 1000 nodes fit in ~100 KB (L2 cache), vs ~550 KB with the monolithic struct.
// ~100B per-node hot type stored/moved in Vecs across every layout pass; kept
// non-Copy on purpose so it isn't silently bulk-copied (Copy would mask the
// cost and churn the many `.clone()` call sites).
#[allow(missing_copy_implementations)]
#[derive(Debug, Clone)]
pub struct LayoutNodeHot {
    /// The resolved box model properties (margin, border, padding)
    /// Stored in packed i16×10 encoding to reduce cache footprint.
    /// Use `box_props.unpack()` to get f32 `ResolvedBoxProps` for computation.
    pub box_props: crate::solver3::geometry::PackedBoxProps,
    /// Reference back to the original DOM node (None for anonymous boxes)
    pub dom_node_id: Option<NodeId>,
    /// The size used during the last layout pass.
    pub used_size: Option<LogicalSize>,
    /// The formatting context this node establishes or participates in.
    pub formatting_context: FormattingContext,
    /// Parent index (None for root)
    pub parent: Option<usize>,
}

/// Warm layout node fields — accessed frequently but not on every node.
///
/// Stored in a separate `Vec`. These fields are accessed during specific
/// layout phases (sizing, IFC, table alignment) but not during the main
/// constraint-solving loop.
#[derive(Debug, Clone, Default)]
pub struct LayoutNodeWarm {
    /// Cached intrinsic sizes (min-content, max-content, etc.)
    pub intrinsic_sizes: Option<IntrinsicSizes>,
    /// The baseline of this box, measured from its content-box top edge.
    pub baseline: Option<f32>,
    /// Cached inline layout result with the constraints used to compute it.
    /// BOXED (rare-data split, 2026-08-11): only IFC roots populate this,
    /// but the 296-B inline Option was paid by EVERY node. 8 B here now.
    pub inline_layout_result: Option<Box<CachedInlineLayout>>,
    /// Cached result of `collect_and_measure_inline_content` for this IFC
    /// root, valid while the subtree's fingerprints are unchanged.
    ///
    /// Collection resolves the FULL cascade (`get_style_properties`) for
    /// every text run and inline span. That ran on every layout — even one
    /// that went on to reuse the cached line layout — and measured 2 ms per
    /// IFC, 32 ms per pagination: the single largest remaining cost after
    /// the line-breaker and font-signature fixes.
    /// BOXED (rare-data split): IFC roots only; was 88 B inline per node.
    pub inline_content_cache: Option<Box<CachedInlineContent>>,
    /// Cached scrollbar information
    pub scrollbar_info: Option<ScrollbarRequirements>,
    /// The position relative to parent's content box.
    pub relative_position: Option<LogicalPosition>,
    /// The actual content size for scrollable containers.
    pub overflow_content_size: Option<LogicalSize>,
    /// Cache for Taffy layout computations.
    pub taffy_cache: TaffyCache,
    /// Pure min/max-content measure results — see the facade field's doc.
    pub measured_content_sizes: (Option<LayoutOutput>, Option<LayoutOutput>),
    /// Pre-computed CSS properties needed during layout.
    pub computed_style: ComputedLayoutStyle,
    /// Pseudo-element type if this node is a pseudo-element
    pub pseudo_element: Option<PseudoElement>,
    /// Escaped top margin (CSS 2.1 margin collapsing)
    pub escaped_top_margin: Option<f32>,
    /// Escaped bottom margin (CSS 2.1 margin collapsing)
    pub escaped_bottom_margin: Option<f32>,
    /// Parent's formatting context
    pub parent_formatting_context: Option<FormattingContext>,
    /// IFC membership for text nodes
    pub ifc_membership: Option<IfcMembership>,
    /// Containing block index for clip exemption
    pub containing_block_index: Option<usize>,
}

/// Cold layout node fields — construction / reconciliation / debugging only.
///
/// Stored in a separate `Vec`. These fields are rarely accessed during layout;
/// mostly used during tree construction, reconciliation, and dirty tracking.
#[derive(Debug, Clone, Default)]
pub struct LayoutNodeCold {
    /// Type of anonymous box (if applicable)
    pub anonymous_type: Option<AnonymousBoxType>,
    /// O3-render: when this node is one PART of a structural-split PREVIEW,
    /// the flat byte range of the source node's inline content this part
    /// displays (`u32::MAX` end = to the end). The IFC build slices its
    /// collected content by this range. `None` = the node shows everything
    /// (the universal case outside previews).
    pub preview_byte_range: Option<(u32, u32)>,
    /// Multi-field fingerprint for granular change detection.
    pub node_data_fingerprint: NodeDataFingerprint,
    /// Hash of this node's data + all descendants.
    pub subtree_hash: SubtreeHash,
    /// Dirty flags for recalculation tracking.
    pub dirty_flag: DirtyFlag,
    /// Unresolved box model properties (raw CSS values).
    pub unresolved_box_props: crate::solver3::geometry::UnresolvedBoxProps,
    /// IFC ID if this node is an IFC root.
    pub ifc_id: Option<IfcId>,
}

impl LayoutNode {
    /// Split this full layout node into hot/warm/cold components.
    /// Used during `LayoutTreeBuilder::build()` to create the `SoA` layout.
    #[must_use]
    pub fn split(self) -> (LayoutNodeHot, LayoutNodeWarm, LayoutNodeCold) {
        (
            LayoutNodeHot {
                box_props: crate::solver3::geometry::PackedBoxProps::pack(&self.box_props),
                dom_node_id: self.dom_node_id,
                used_size: self.used_size,
                formatting_context: self.formatting_context,
                parent: self.parent,
            },
            LayoutNodeWarm {
                intrinsic_sizes: self.intrinsic_sizes,
                baseline: self.baseline,
                inline_layout_result: self.inline_layout_result,
                inline_content_cache: self.inline_content_cache,
                scrollbar_info: self.scrollbar_info,
                relative_position: self.relative_position,
                overflow_content_size: self.overflow_content_size,
                taffy_cache: self.taffy_cache,
                measured_content_sizes: self.measured_content_sizes,
                computed_style: self.computed_style,
                pseudo_element: self.pseudo_element,
                escaped_top_margin: self.escaped_top_margin,
                escaped_bottom_margin: self.escaped_bottom_margin,
                parent_formatting_context: self.parent_formatting_context,
                ifc_membership: self.ifc_membership,
                containing_block_index: self.containing_block_index,
            },
            LayoutNodeCold {
                anonymous_type: self.anonymous_type,
                preview_byte_range: self.preview_byte_range,
                node_data_fingerprint: self.node_data_fingerprint,
                subtree_hash: self.subtree_hash,
                dirty_flag: self.dirty_flag,
                unresolved_box_props: self.unresolved_box_props,
                ifc_id: self.ifc_id,
            },
        )
    }
}

/// How far [`LayoutTree::get_ifc_root_layout_index`] walks up looking for the
/// box that owns an inline descendant's layout. Inline nesting is shallow in
/// practice; the bound only stops a malformed tree from spinning.
const IFC_ANCESTOR_WALK_LIMIT: usize = 64;

/// A position in the LAYOUT tree — distinct from a DOM [`NodeId`] BY TYPE.
///
/// The layout tree interleaves anonymous boxes and splits during
/// construction, so layout indices and DOM indices diverge for any real DOM.
/// Conflating them (`node_id.index()` used as a layout index) froze CPU-path
/// scrolling once (`build_scroll_offset_map`) and is exactly the mistake this
/// newtype turns into a compile error: the ONLY sanctioned ways to obtain a
/// `LayoutNodeId` from a DOM node are [`LayoutTree::dom_to_layout`] and the
/// scroll-id tables. Solver-interior code that produces indices locally wraps
/// them with [`LayoutNodeId::new`] — an explicit, greppable assertion that
/// the value really is a layout index.
///
/// The raw `SoA` vectors (`nodes`, `warm`, `cold`, `children_arena`) stay
/// `usize`-indexed for interior arithmetic; the typed boundary is the
/// accessor surface (`get`/`warm`/`cold`/`get_content_size`/...), which is
/// what map-derived consumer code calls.
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutNodeId(pub usize);

impl LayoutNodeId {
    #[inline]
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }
    /// The raw vector index. Greppable by design: every `.index()` is a
    /// deliberate exit from the typed space.
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

impl core::fmt::Display for LayoutNodeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "L{}", self.0)
    }
}

/// The complete layout tree structure.
///
/// Uses a struct-of-arrays (`SoA`) layout for cache performance:
/// - `nodes` (hot): accessed on every node in every layout pass
/// - `warm`: accessed during specific layout phases
/// - `cold`: construction / reconciliation only
#[derive(Debug, Clone)]
pub struct LayoutTree {
    /// Hot layout data — box props, parent, `used_size`, formatting context
    pub nodes: Vec<LayoutNodeHot>,
    /// Warm layout data — intrinsic sizes, baseline, inline layout, etc.
    pub warm: Vec<LayoutNodeWarm>,
    /// Cold layout data — dirty flags, fingerprints, reconciliation data
    pub cold: Vec<LayoutNodeCold>,
    /// Root node index
    pub root: usize,
    /// Mapping from DOM node IDs to layout node indices
    // BTreeMap (not HashMap): std HashMap's RandomState hasher needs an RNG seed
    // that isn't available in the remill-lifted wasm (no getrandom), so inserts
    // silently no-op there — dom_to_layout came back empty (node mapping lost,
    // get_node_size/position returned None → 0-rects). BTreeMap is deterministic,
    // matches the rest of azul-core, and lifts reliably (M12.7).
    pub dom_to_layout: BTreeMap<NodeId, Vec<LayoutNodeId>>,
    /// Flat arena holding all children indices contiguously.
    pub children_arena: Vec<usize>,
    /// Per-node (start, len) into `children_arena`. Indexed by node index.
    pub children_offsets: Vec<(u32, u32)>,
    /// Per-node bit: this node or any descendant establishes a shrink-to-fit
    /// (STF) context whose sizing algorithm reads children's intrinsic sizes
    /// (flex/grid/table/inline-block containers, floats, or abspos elements).
    ///
    /// If `subtree_needs_intrinsic[i]` is false AND no ancestor of `i` is STF
    /// either, the intrinsic sizing pass can skip the entire subtree — nothing
    /// will ever read those values. This is the static-DOM optimization from
    /// §58 Win #3 (the "safely re-enabled Fix C").
    ///
    /// Computed once at tree build time in `generate_layout_tree`. An empty
    /// vec means "assume every subtree needs intrinsics" (safe fallback for
    /// code paths that construct `LayoutTree` without going through the
    /// builder — currently none, but preserves the invariant for tests).
    pub subtree_needs_intrinsic: Vec<bool>,
}

/// Approximate per-field heap-byte breakdown of a [`LayoutTree`].
#[derive(Copy, Debug, Clone, Default)]
pub struct LayoutTreeMemoryReport {
    pub node_count: usize,
    pub hot_bytes: usize,
    pub warm_bytes: usize,
    pub warm_inline_layout_bytes: usize,
    /// Shaped clusters retained across the whole tree. A cluster is one
    /// grapheme, so on Latin text this is roughly the CHARACTER COUNT of
    /// everything laid out — the number that explains `warm.inline`.
    pub shaped_cluster_count: usize,
    /// Shaped glyphs retained. Differs from the cluster count only for
    /// ligatures and combining marks.
    pub shaped_glyph_count: usize,
    /// Bytes of per-cluster heap `String` (each cluster owns a copy of its
    /// own text). Called out separately because it is pure duplication of
    /// text the DOM already holds.
    pub shaped_cluster_text_bytes: usize,
    /// Distinct `Arc<StyleProperties>` allocations the retained glyphs point
    /// at, by pointer identity.
    ///
    /// A document has a handful of distinct text styles (body, headings,
    /// code), so a healthy number here is single digits no matter how many
    /// glyphs there are. A count that tracks the GLYPH count means the
    /// sharing predicate has silently stopped sharing — Stylo hit exactly
    /// this, ending up with 109k `ComputedValues` where 2,200 were expected
    /// because one pseudo-element rule defeated the predicate. At 248 B per
    /// `StyleProperties` that failure is invisible and expensive.
    pub distinct_glyph_style_arcs: usize,
    /// Paint-run cache (`CachedInlineLayout::glyph_runs`): heap bytes of the
    /// retained `Vec<SimpleGlyphRun>` lists, each distinct Arc counted once
    /// (`GlyphSwap` reuse shares them). This is the store-time derivation that
    /// makes every paint a lookup — the retained cost of that trade.
    pub glyph_run_bytes: usize,
    /// Distinct `SimpleGlyphRun`s behind `glyph_run_bytes`.
    pub glyph_run_count: usize,
    /// Total `GlyphInstance`s retained across those runs (the 20 B/each bulk).
    pub glyph_instance_count: usize,
    pub warm_taffy_cache_bytes: usize,
    pub cold_bytes: usize,
    pub dom_to_layout_bytes: usize,
    pub children_arena_bytes: usize,
    pub children_offsets_bytes: usize,
}

impl LayoutTreeMemoryReport {
    #[must_use]
    pub const fn total_bytes(&self) -> usize {
        self.hot_bytes
            + self.warm_bytes
            + self.warm_inline_layout_bytes
            + self.glyph_run_bytes
            + self.warm_taffy_cache_bytes
            + self.cold_bytes
            + self.dom_to_layout_bytes
            + self.children_arena_bytes
            + self.children_offsets_bytes
    }
}

/// Iterator over a node's ancestor chain — see [`LayoutTree::ancestor_chain`].
///
/// Yields layout indices child-first (nearest ancestor first).
#[derive(Debug)]
pub struct AncestorChain<'a> {
    nodes: &'a [LayoutNodeHot],
    cursor: Option<usize>,
    budget: usize,
}

impl Iterator for AncestorChain<'_> {
    type Item = LayoutNodeId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.budget == 0 {
            return None;
        }
        let idx = self.cursor?;
        self.budget -= 1;
        self.cursor = self.nodes.get(idx).and_then(|n| n.parent);
        Some(LayoutNodeId::new(idx))
    }
}

impl LayoutTree {
    /// Approximate heap bytes retained by this `LayoutTree`.
    #[must_use]
    pub fn memory_report(&self) -> LayoutTreeMemoryReport {
        let mut report = LayoutTreeMemoryReport {
            node_count: self.nodes.len(),
            hot_bytes: self.nodes.capacity() * size_of::<LayoutNodeHot>(),
            warm_bytes: self.warm.capacity() * size_of::<LayoutNodeWarm>(),
            cold_bytes: self.cold.capacity() * size_of::<LayoutNodeCold>(),
            children_arena_bytes: self.children_arena.capacity() * size_of::<usize>(),
            children_offsets_bytes: self.children_offsets.capacity() * size_of::<(u32, u32)>(),
            dom_to_layout_bytes: 0,
            warm_inline_layout_bytes: 0,
            shaped_cluster_count: 0,
            shaped_glyph_count: 0,
            shaped_cluster_text_bytes: 0,
            distinct_glyph_style_arcs: 0,
            glyph_run_bytes: 0,
            glyph_run_count: 0,
            glyph_instance_count: 0,
            warm_taffy_cache_bytes: 0,
        };
        // HashMap<NodeId, Vec<usize>> — approximate: (key + Vec-header) per entry
        // plus heap for each inner Vec.
        let entries = self.dom_to_layout.len();
        report.dom_to_layout_bytes = entries * (size_of::<NodeId>() + size_of::<Vec<usize>>());
        for v in self.dom_to_layout.values() {
            report.dom_to_layout_bytes += v.capacity() * size_of::<usize>();
        }
        // Inline layout data lives behind Arc — count Arc heap-shares once
        // per node that has a cached layout. Counted conservatively.
        let mut text_arcs: alloc::collections::BTreeSet<usize> =
            alloc::collections::BTreeSet::new();
        let mut style_arcs: alloc::collections::BTreeSet<usize> =
            alloc::collections::BTreeSet::new();
        let mut run_arcs: alloc::collections::BTreeSet<usize> = alloc::collections::BTreeSet::new();
        for w in &self.warm {
            if let Some(cached) = &w.inline_layout_result {
                // The Box allocation itself — CachedInlineLayout is boxed off
                // LayoutNodeWarm, so `warm_bytes` above does NOT include it.
                // This also covers the INLINE `Option<UnifiedConstraints>`.
                report.warm_inline_layout_bytes += size_of::<CachedInlineLayout>();
                // Heap Vecs hanging off the inline constraints.
                if let Some(c) = &cached.constraints {
                    report.warm_inline_layout_bytes += c.shape_boundaries.capacity()
                        * size_of::<crate::text3::cache::ShapeBoundary>()
                        + c.shape_exclusions.capacity()
                            * size_of::<crate::text3::cache::ShapeBoundary>();
                }
                // Cached line breaks (edit fast-path).
                if let Some(lb) = &cached.line_breaks {
                    report.warm_inline_layout_bytes += lb.line_ranges.capacity()
                        * size_of::<(usize, usize)>()
                        + lb.line_widths.capacity() * size_of::<f32>();
                }
                // The payload Arc: when it is a TextPayload wrapper, that
                // allocation holds two Arc CLONES of buffers counted
                // elsewhere — only its own header is new bytes. When it is
                // the layout Arc itself (flag-off), nothing new to count.
                if cached.payload.downcast_ref::<TextPayload>().is_some() {
                    report.warm_inline_layout_bytes += size_of::<TextPayload>();
                }
                // Paint-run cache — its own report line (see the field doc).
                // Distinct Arcs only: GlyphSwap reuse clones the Arc.
                if run_arcs.insert(Arc::as_ptr(&cached.glyph_runs).cast::<u8>() as usize) {
                    report.glyph_run_bytes += cached.glyph_runs.capacity()
                        * size_of::<crate::text3::glyphs::CompactGlyphRun>();
                    for run in cached.glyph_runs.iter() {
                        report.glyph_run_bytes += run.glyphs.retained_bytes()
                            + run.background_content.capacity()
                                * size_of::<azul_css::props::style::StyleBackgroundContent>();
                        report.glyph_run_count += 1;
                        report.glyph_instance_count += run.glyphs.len();
                    }
                }
                // Arc<UnifiedLayout> — count the UnifiedLayout header + its items.
                report.warm_inline_layout_bytes += size_of::<UnifiedLayout>();
                report.warm_inline_layout_bytes += cached.layout.items.capacity()
                    * size_of::<crate::text3::cache::PositionedItem>();
                report.warm_inline_layout_bytes +=
                    cached.item_metrics.capacity() * size_of::<InlineItemMetrics>();
                // (d1): the retained dense view, when AZ_DENSE_TEXT keeps it.
                // Counted per-field so the sparse-vs-dense split is honest
                // during the reader migration (both alive = the transitional
                // cost; the sparse side retires at the end of (d)).
                if let Some(d) = &cached.dense {
                    use crate::text3::dense as dn;
                    report.warm_inline_layout_bytes += size_of::<dn::DenseText>()
                        + d.clusters.capacity() * size_of::<dn::ClusterCompact>()
                        + d.runs.capacity() * size_of::<dn::DenseRun>()
                        + d.lines.capacity() * size_of::<dn::LineRecord>()
                        + d.details.capacity() * size_of::<dn::ClusterDetail>()
                        + d.detail_glyphs.capacity() * size_of::<dn::DetailGlyph>();
                }
                // Glyphs and per-cluster text.
                //
                // ONLY the SPILLED glyphs are counted here. `glyphs` is a
                // `SmallVec<[ShapedGlyph; 1]>`, so its first element lives
                // INSIDE the ShapedCluster — which is already counted as part
                // of `PositionedItem` above. Multiplying `capacity()` (which
                // includes the inline slot) by `size_of::<ShapedGlyph>()`
                // double-counted one 96-byte glyph for EVERY cluster: on a
                // 31k-cluster document that inflated the report by ~3 MB, and
                // a memory report that over-reports sends the optimisation
                // work at the wrong target.
                for item in &cached.layout.items {
                    if let crate::text3::cache::ShapedItem::Cluster(c) = &item.item {
                        let spilled = c.glyphs.capacity().saturating_sub(1);
                        report.warm_inline_layout_bytes +=
                            spilled * size_of::<crate::text3::cache::ShapedGlyph>();
                        // 3c: per-cluster text is a SHARED Arc slice now —
                        // attribute each distinct source Arc's buffer ONCE.
                        if text_arcs.insert(Arc::as_ptr(&c.source_text).cast::<u8>() as usize) {
                            report.warm_inline_layout_bytes += c.source_text.len();
                            report.shaped_cluster_text_bytes += c.source_text.len();
                        }
                        report.shaped_cluster_count += 1;
                        report.shaped_glyph_count += c.glyphs.len();
                        style_arcs.insert(Arc::as_ptr(&c.style) as usize);
                    }
                }
            }
            // Taffy cache — each slot is an Option, ~50 B empty
            report.warm_taffy_cache_bytes += size_of::<TaffyCache>();
        }
        report.distinct_glyph_style_arcs = style_arcs.len();
        report
    }

    /// Returns the children of node `index` as a contiguous slice from the arena.
    #[inline]
    #[must_use]
    pub fn children(&self, index: usize) -> &[usize] {
        if let Some(&(start, len)) = self.children_offsets.get(index) {
            &self.children_arena[(start as usize)..((start as usize) + (len as usize))]
        } else {
            &[]
        }
    }

    /// Get hot layout data for a node (`box_props`, `dom_node_id`, `used_size`, etc.)
    #[inline]
    #[must_use]
    pub fn get(&self, index: LayoutNodeId) -> Option<&LayoutNodeHot> {
        self.nodes.get(index.index())
    }

    /// The left/top step from a node's BORDER box (what `calculated_positions`
    /// and `used_size` describe) to its CONTENT box (what inline layout, and
    /// therefore every caret/selection/hit-test coordinate, is measured in).
    ///
    /// THE single source of `padding.left + border.left` / `padding.top +
    /// border.top` for the pointer pipeline. The display-list builder already
    /// went through `BorderBoxRect::to_content_box` for painting; the two
    /// hit-test producers each open-coded (or, on the `WebRender` side, simply
    /// omitted) this term, which is exactly how they came to disagree.
    ///
    /// An unknown index yields [`ContentInset::ZERO`], matching the
    /// "unpadded, unbordered" default rather than skewing the point.
    #[inline]
    #[must_use]
    pub fn content_inset(&self, index: LayoutNodeId) -> ContentInset {
        self.get(index).map_or(ContentInset::ZERO, |node| {
            let bp = node.box_props.unpack();
            ContentInset::new(
                bp.padding.left + bp.border.left,
                bp.padding.top + bp.border.top,
            )
        })
    }

    /// THE ancestor walk: `index` (only when `inclusivity` says so) followed by
    /// every parent up to the root.
    ///
    /// Five separate hand-rolled versions of this loop used to exist, and each
    /// encoded "does this include the node itself?" in its starting value —
    /// `let mut cursor = Some(idx)` vs `let mut cur = node.parent`. That is
    /// invisible at the call site, which is how `accumulated_scroll_for_node`
    /// (self-inclusive) and [`node_rect_to_screen`] (ancestors-only) came to
    /// look like the same helper under two names, and how the auto-scroll
    /// timer ended up unable to target the caret's own scroll box. Taking
    /// [`Inclusivity`] as an argument forces every call site to say which it
    /// means.
    ///
    /// [`node_rect_to_screen`]: crate::headless::node_rect_to_screen
    ///
    /// The walk is bounded by `nodes.len()`: no acyclic path can be longer, so
    /// a malformed (cyclic) `parent` chain terminates instead of spinning,
    /// while a legitimately deep tree is never truncated.
    #[inline]
    #[must_use]
    pub fn ancestor_chain(
        &self,
        index: LayoutNodeId,
        inclusivity: Inclusivity,
    ) -> AncestorChain<'_> {
        let start = if inclusivity.includes_self() {
            Some(index.index())
        } else {
            self.nodes.get(index.index()).and_then(|n| n.parent)
        };
        AncestorChain {
            nodes: &self.nodes,
            cursor: start,
            budget: self.nodes.len(),
        }
    }

    /// Get mutable hot layout data for a node.
    #[inline]
    pub fn get_mut(&mut self, index: LayoutNodeId) -> Option<&mut LayoutNodeHot> {
        self.nodes.get_mut(index.index())
    }

    /// Get warm layout data for a node (`intrinsic_sizes`, baseline, `inline_layout`, etc.)
    #[inline]
    #[must_use]
    pub fn warm(&self, index: LayoutNodeId) -> Option<&LayoutNodeWarm> {
        self.warm.get(index.index())
    }

    /// Get mutable warm layout data for a node.
    #[inline]
    pub fn warm_mut(&mut self, index: LayoutNodeId) -> Option<&mut LayoutNodeWarm> {
        self.warm.get_mut(index.index())
    }

    /// Get cold layout data for a node (`dirty_flag`, `subtree_hash`, fingerprint, etc.)
    #[inline]
    #[must_use]
    pub fn cold(&self, index: LayoutNodeId) -> Option<&LayoutNodeCold> {
        self.cold.get(index.index())
    }

    /// Get mutable cold layout data for a node.
    #[inline]
    pub fn cold_mut(&mut self, index: LayoutNodeId) -> Option<&mut LayoutNodeCold> {
        self.cold.get_mut(index.index())
    }

    fn root_node(&self) -> &LayoutNodeHot {
        &self.nodes[self.root]
    }

    /// Reconstruct a full `LayoutNode` from the split hot/warm/cold arrays.
    ///
    /// Used when passing node data to `LayoutTreeBuilder::clone_node_from_old()`.
    #[must_use]
    pub fn get_full_node(&self, index: usize) -> Option<LayoutNode> {
        let hot = self.nodes.get(index)?;
        let warm = self.warm.get(index).cloned().unwrap_or_default();
        let cold = self.cold.get(index).cloned().unwrap_or_default();
        let children = self.children(index).to_vec();
        Some(LayoutNode {
            box_props: hot.box_props.unpack(),
            dom_node_id: hot.dom_node_id,
            children,
            used_size: hot.used_size,
            formatting_context: hot.formatting_context,
            parent: hot.parent,
            intrinsic_sizes: warm.intrinsic_sizes,
            baseline: warm.baseline,
            inline_layout_result: warm.inline_layout_result,
            inline_content_cache: warm.inline_content_cache,
            scrollbar_info: warm.scrollbar_info,
            relative_position: warm.relative_position,
            overflow_content_size: warm.overflow_content_size,
            taffy_cache: warm.taffy_cache,
            measured_content_sizes: warm.measured_content_sizes,
            computed_style: warm.computed_style,
            pseudo_element: warm.pseudo_element,
            escaped_top_margin: warm.escaped_top_margin,
            escaped_bottom_margin: warm.escaped_bottom_margin,
            parent_formatting_context: warm.parent_formatting_context,
            ifc_membership: warm.ifc_membership,
            containing_block_index: warm.containing_block_index,
            anonymous_type: cold.anonymous_type,
            preview_byte_range: cold.preview_byte_range,
            node_data_fingerprint: cold.node_data_fingerprint,
            subtree_hash: cold.subtree_hash,
            dirty_flag: cold.dirty_flag,
            unresolved_box_props: cold.unresolved_box_props,
            ifc_id: cold.ifc_id,
        })
    }

    /// Re-resolve box properties for a node with the actual containing block size.
    pub(crate) fn resolve_box_props(
        &mut self,
        node_index: usize,
        containing_block: LogicalSize,
        viewport_size: LogicalSize,
        element_font_size: f32,
        root_font_size: f32,
    ) {
        let params = crate::solver3::geometry::ResolutionParams {
            containing_block,
            viewport_size,
            element_font_size,
            root_font_size,
        };
        if let (Some(hot), Some(cold)) = (self.nodes.get_mut(node_index), self.cold.get(node_index))
        {
            hot.box_props = crate::solver3::geometry::PackedBoxProps::pack(
                &cold.unresolved_box_props.resolve(&params),
            );
        }
    }

    /// Marks a node and its ancestors as dirty with the given flag.
    pub fn mark_dirty(&mut self, start_index: usize, flag: DirtyFlag) {
        if flag == DirtyFlag::None {
            return;
        }

        let mut current_index = Some(start_index);
        while let Some(index) = current_index {
            let Some(cold) = self.cold.get_mut(index) else {
                break;
            };
            if cold.dirty_flag >= flag {
                break;
            }
            cold.dirty_flag = flag;
            current_index = self.nodes.get(index).and_then(|n| n.parent);
        }
    }

    /// Marks a node and its entire subtree of descendants with the given dirty flag.
    fn mark_subtree_dirty(&mut self, start_index: usize, flag: DirtyFlag) {
        if flag == DirtyFlag::None {
            return;
        }

        let mut stack = vec![start_index];
        while let Some(index) = stack.pop() {
            let children = self.children(index).to_vec();
            if let Some(cold) = self.cold.get_mut(index) {
                if cold.dirty_flag < flag {
                    cold.dirty_flag = flag;
                }
                stack.extend_from_slice(&children);
            }
        }
    }

    /// Resets the dirty flags of all nodes in the tree to `None` after layout is complete.
    fn clear_all_dirty_flags(&mut self) {
        for cold in &mut self.cold {
            cold.dirty_flag = DirtyFlag::None;
        }
    }

    /// Get inline layout for a node, navigating through IFC membership if needed.
    /// (d4) The dense view for a node, resolved through the SAME IFC
    /// membership walk as [`Self::get_inline_layout_for_node`]. `None`
    /// when no dense view is retained (flag off) or the node has no IFC.
    #[must_use]
    pub fn get_dense_for_node(
        &self,
        layout_index: usize,
    ) -> Option<&Arc<crate::text3::dense::DenseText>> {
        let warm = self.warm.get(layout_index)?;
        if let Some(cached) = &warm.inline_layout_result {
            return cached.dense.as_ref();
        }
        if let Some(ifc_membership) = &warm.ifc_membership {
            let ifc_root_warm = self.warm.get(ifc_membership.ifc_root_layout_index)?;
            if let Some(cached) = &ifc_root_warm.inline_layout_result {
                return cached.dense.as_ref();
            }
        }
        None
    }

    /// (d6h) The stored inline layout for a node, MATERIALIZED when it
    /// is the retirement sentinel: dense expands (exact by the d6h
    /// expansion gate) into a transient copy for geometry readers —
    /// the caret and selection painters. Flag-off / verify / non-pure
    /// layouts return the stored Arc untouched.
    #[must_use]
    pub fn materialized_inline_layout_for_node(
        &self,
        layout_index: usize,
    ) -> Option<Arc<UnifiedLayout>> {
        let warm = self.warm(LayoutNodeId::new(layout_index))?;
        let cached = warm.inline_layout_result.as_ref()?;
        if cached.layout.items.is_empty() {
            if let Some(d) = cached.dense.as_deref() {
                if !d.clusters.is_empty() {
                    return Some(Arc::new(UnifiedLayout {
                        items: d.to_unified_items(),
                        overflow: cached.overflow.clone(),
                    }));
                }
            }
        }
        Some(cached.layout.clone())
    }

    #[must_use]
    pub fn get_inline_layout_for_node(&self, layout_index: usize) -> Option<&Arc<UnifiedLayout>> {
        let warm = self.warm.get(layout_index)?;

        // First, check if this node has its own inline_layout_result (it's an IFC root)
        if let Some(cached) = &warm.inline_layout_result {
            return Some(cached.get_layout());
        }

        // For text nodes, check if they have ifc_membership pointing to the IFC root
        if let Some(ifc_membership) = &warm.ifc_membership {
            let ifc_root_warm = self.warm.get(ifc_membership.ifc_root_layout_index)?;
            if let Some(cached) = &ifc_root_warm.inline_layout_result {
                return Some(cached.get_layout());
            }
        }

        None
    }

    /// Return the layout index of the IFC root that owns `layout_index`'s inline content.
    /// If the node IS an IFC root (has its own `inline_layout_result`) or has no
    /// `ifc_membership`, returns `layout_index` unchanged. Inline text nodes never get
    /// their own box position (it stays the `f32::MIN` sentinel) — their geometry lives
    /// in the IFC root's content box, so selection/inline painting must anchor to the
    /// IFC root's position, not the text node's. See `get_inline_layout_for_node`.
    #[must_use]
    pub fn get_ifc_root_layout_index(&self, layout_index: usize) -> usize {
        if let Some(warm) = self.warm.get(layout_index) {
            if warm.inline_layout_result.is_none() {
                if let Some(ifc_membership) = &warm.ifc_membership {
                    return ifc_membership.ifc_root_layout_index;
                }
                // No membership recorded. `ifc_membership` is only ever
                // assigned to text nodes the IFC root's own walk collected
                // directly; `collect_inline_span_recursive` assigns none, so
                // anything under a `<span>` carries nothing and ownership used
                // to resolve to the text node ITSELF. That is why the caret in
                // `p > span > text` was never painted, and why keyboard focus
                // into any editable containing a span produced a session whose
                // caret never appeared.
                //
                // The nearest ancestor that OWNS inline layout is the IFC root
                // by definition, so walk up to it. Nodes outside any IFC have
                // no such ancestor and fall through to the identity below.
                let mut cursor = layout_index;
                for _ in 0..IFC_ANCESTOR_WALK_LIMIT {
                    let Some(parent) = self.nodes.get(cursor).and_then(|n| n.parent) else {
                        break;
                    };
                    match self.warm.get(parent) {
                        Some(w) if w.inline_layout_result.is_some() => return parent,
                        Some(_) => cursor = parent,
                        None => break,
                    }
                }
            }
        }
        layout_index
    }

    /// Get the content size of a node (for scrollbar calculations).
    #[must_use]
    pub fn get_content_size(&self, index: LayoutNodeId) -> LogicalSize {
        let Some(warm) = self.warm.get(index.index()) else {
            return LogicalSize::default();
        };

        if let Some(content_size) = warm.overflow_content_size {
            return content_size;
        }

        let Some(hot) = self.nodes.get(index.index()) else {
            return LogicalSize::default();
        };

        let mut content_size = hot.used_size.unwrap_or_default();

        if let Some(ref cached_layout) = warm.inline_layout_result {
            let text_layout = &cached_layout.layout;
            let mut max_x: f32 = 0.0;
            let mut max_y: f32 = 0.0;
            for positioned_item in &text_layout.items {
                let item_bounds = positioned_item.item.bounds();
                max_x = max_x.max(positioned_item.position.x + item_bounds.width);
                max_y = max_y.max(positioned_item.position.y + item_bounds.height);
            }
            content_size.width = content_size.width.max(max_x);
            content_size.height = content_size.height.max(max_y);
        }

        content_size
    }
}

/// Generate layout tree from styled DOM with proper anonymous box generation
/// # Errors
///
/// Returns a `LayoutError` if the layout tree cannot be built.
pub fn generate_layout_tree<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
) -> Result<LayoutTree> {
    let mut builder = LayoutTreeBuilder::new(ctx.viewport_size);
    let root_id = ctx
        .styled_dom
        .root
        .into_crate_internal()
        .unwrap_or(NodeId::ZERO);
    let root_index = builder.process_node(ctx.styled_dom, root_id, None, ctx.debug_messages)?;
    builder.apply_split_previews(ctx.content_overlay, ctx.styled_dom);
    let mut layout_tree = builder.build(root_index);

    // Pre-compute the STF (shrink-to-fit) subtree bitmap. This is static-DOM
    // information: whether a subtree establishes any shrink-to-fit context
    // depends only on the DOM structure + formatting context, both of which
    // are frozen from here until the next layout-tree rebuild. The intrinsic
    // sizing pass reads this to skip subtrees whose intrinsics are never
    // consumed (§58 Win #3).
    layout_tree.subtree_needs_intrinsic =
        compute_subtree_needs_intrinsic(ctx.styled_dom, &layout_tree);

    debug_log!(
        ctx,
        "Generated layout tree with {} nodes (incl. anonymous)",
        layout_tree.nodes.len()
    );

    Ok(layout_tree)
}

/// Returns true if `(dom_node_id, fc)` establishes a formatting context whose
/// sizing algorithm reads children's intrinsic sizes. Covers:
/// - flex containers (flex item sizing uses child min/max-content),
/// - grid containers (grid-track sizing likewise),
/// - tables and table cells,
/// - inline-block (its own width may be shrink-to-fit),
/// - floats and abspos elements (their `auto` width resolves to shrink-to-fit).
///
/// A `FormattingContext::Block` with a definite CSS width is NOT shrink-to-fit —
/// its inner layout gets the width top-down, so descendant intrinsics don't
/// feed back up. That's the path Fix C short-circuits.
pub(crate) fn is_shrink_to_fit_context(
    styled_dom: &StyledDom,
    dom_node_id: Option<NodeId>,
    fc: FormattingContext,
) -> bool {
    use crate::solver3::getters::{get_float, MultiValue};
    use crate::solver3::positioning::get_position_type;
    use azul_css::props::layout::{LayoutFloat, LayoutPosition};

    match fc {
        FormattingContext::Flex
        | FormattingContext::Grid
        | FormattingContext::Table
        | FormattingContext::InlineBlock => return true,
        _ => {}
    }
    let Some(dom_id) = dom_node_id else {
        return false;
    };
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let float_val = match get_float(styled_dom, dom_id, node_state) {
        MultiValue::Exact(v) => v,
        _ => LayoutFloat::None,
    };
    if float_val != LayoutFloat::None {
        return true;
    }
    let pos = get_position_type(styled_dom, Some(dom_id));
    if pos == LayoutPosition::Absolute || pos == LayoutPosition::Fixed {
        // Abspos only becomes shrink-to-fit when width is `auto`.
        // Being conservative: treat as STF whenever abspos so we still
        // compute intrinsics for the auto-width case. Misses no work.
        return true;
    }
    false
}

/// Per-node bitmap of "this node or any descendant establishes a shrink-to-fit
/// context." Post-order walk: `out[i] = self_stf(i) || any(out[child_of_i])`.
/// Layout tree nodes are built top-down (pre-order), so iterating from the end
/// visits children before parents.
fn compute_subtree_needs_intrinsic(styled_dom: &StyledDom, tree: &LayoutTree) -> Vec<bool> {
    let n = tree.nodes.len();
    let mut out = vec![false; n];
    for idx in (0..n).rev() {
        let hot = &tree.nodes[idx];
        let self_stf =
            is_shrink_to_fit_context(styled_dom, hot.dom_node_id, hot.formatting_context);
        let mut any = self_stf;
        if !any {
            for &child in tree.children(idx) {
                if out.get(child).copied().unwrap_or(false) {
                    any = true;
                    break;
                }
            }
        }
        out[idx] = any;
    }
    out
}

/// Incrementally builds a [`LayoutTree`] from a [`StyledDom`].
///
/// Usage: create via [`LayoutTreeBuilder::new`], call [`process_node`](Self::process_node)
/// on the root DOM node, then call [`build`](Self::build) to produce the final
/// SoA-split `LayoutTree`. During `process_node`, anonymous boxes are generated
/// as required by CSS 2.2 §9.2.1.1 (inline wrappers) and §17.2.1 (table fixup).
#[derive(Debug)]
pub struct LayoutTreeBuilder {
    nodes: Vec<LayoutNode>,
    dom_to_layout: BTreeMap<NodeId, Vec<usize>>,
    viewport_size: LogicalSize,
}

impl LayoutTreeBuilder {
    #[must_use]
    pub const fn new(viewport_size: LogicalSize) -> Self {
        Self {
            nodes: Vec::new(),
            dom_to_layout: BTreeMap::new(),
            viewport_size,
        }
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&LayoutNode> {
        self.nodes.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut LayoutNode> {
        self.nodes.get_mut(index)
    }

    // +spec:display-property:2188b7 - builds box tree: each element's principal box is child of nearest ancestor's principal box, with anonymous boxes for tables/inline wrapping
    /// Main entry point for recursively building the layout tree.
    /// This function dispatches to specialized handlers based on the node's
    /// `display` property to correctly generate anonymous boxes.
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn process_node(
        &mut self,
        styled_dom: &StyledDom,
        dom_id: NodeId,
        parent_idx: Option<usize>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<usize> {
        let node_data = &styled_dom.node_data.as_container()[dom_id];
        let node_idx = self.create_node_from_dom(styled_dom, dom_id, parent_idx, debug_messages);
        let raw_display = get_display_type(styled_dom, dom_id);

        // +spec:display-property:042f56 - replaced elements with layout-internal display use inline
        // CSS Display 3 §2.4: "When the display property of a replaced element computes to
        // one of the layout-internal values, it is handled as having a used value of inline."
        let raw_display = if raw_display.is_layout_internal() && is_replaced_element(node_data) {
            LayoutDisplay::Inline
        } else {
            raw_display
        };

        // +spec:display-property:0b40af - display/position/float interaction per CSS 2.2 §9.7
        // +spec:display-property:ba53ba - float!=none or position!=static causes display to blockify
        // +spec:positioning:69468c - absolute/fixed blockifies the box, float computes to none
        // +spec:table-layout:cfc60a - CSS 2.2 §9.7: display/position/float interaction
        // Blockification rules (CSS Display 3 §2.7 / §2.8):
        // 1. Root element → blockify
        // 2. position:absolute or position:fixed → float computes to 'none', blockify
        // 3. float is not 'none' → blockify
        // 4. Flex/Grid children → blockify
        let node_position = self
            .nodes
            .get(node_idx)
            .map(|n| n.computed_style.position)
            .unwrap_or_default();
        let node_float = self
            .nodes
            .get(node_idx)
            .map(|n| n.computed_style.float)
            .unwrap_or_default();
        let is_absolute_or_fixed = matches!(
            node_position,
            LayoutPosition::Absolute | LayoutPosition::Fixed
        );
        let is_floated = node_float != LayoutFloat::None;
        let is_root = parent_idx.is_none();

        // Per CSS 2.2 §9.7: if position is absolute or fixed, float computes to 'none'
        if is_absolute_or_fixed && is_floated {
            if let Some(node) = self.nodes.get_mut(node_idx) {
                node.computed_style.float = LayoutFloat::None;
            }
        }

        let is_flex_grid_child = parent_idx
            .and_then(|p| {
                self.nodes.get(p).map(|n| {
                    matches!(
                        n.formatting_context,
                        FormattingContext::Flex | FormattingContext::Grid
                    )
                })
            })
            .unwrap_or(false);

        let display_type = crate::solver3::getters::get_computed_display(
            raw_display,
            is_absolute_or_fixed,
            is_floated,
            is_root,
            is_flex_grid_child,
        );

        // If blockification changed the display type, update the node's formatting context
        if display_type != raw_display {
            if let Some(node) = self.nodes.get_mut(node_idx) {
                node.computed_style.display = display_type;
                node.formatting_context =
                    determine_formatting_context_for_display(styled_dom, dom_id, display_type);
            }
        }

        // Compute containing block index for abs-pos clip exemption
        if is_absolute_or_fixed {
            let cb_index = if matches!(node_position, LayoutPosition::Fixed) {
                // Fixed elements: containing block is the root (viewport)
                None
            } else {
                // Absolute elements: containing block is nearest positioned ancestor
                let mut ancestor = parent_idx;
                loop {
                    match ancestor {
                        Some(idx) => {
                            let pos = self
                                .nodes
                                .get(idx)
                                .map(|n| n.computed_style.position)
                                .unwrap_or_default();
                            if pos.is_positioned() {
                                break Some(idx);
                            }
                            ancestor = self.nodes.get(idx).and_then(|n| n.parent);
                        }
                        None => break None, // root
                    }
                }
            };
            if let Some(node) = self.nodes.get_mut(node_idx) {
                node.containing_block_index = cb_index;
            }
        }

        if parent_idx.is_none() {
            if let Some(node) = self.nodes.get_mut(node_idx) {
                if let FormattingContext::Block {
                    ref mut establishes_new_context,
                } = node.formatting_context
                {
                    *establishes_new_context = true;
                }
            }
        }

        // +spec:display-property:1f4039 - list-item generates ::marker pseudo-element + principal box
        // +spec:display-property:2bb592 - list-item generates ::marker pseudo-element with list-style content
        // +spec:display-property:3b507e - list-item generates ::marker pseudo-element
        // +spec:display-property:a48f00 - additional boxes (marker, table wrapper) placed w.r.t. principal box
        // +spec:display-property:998063 - list-item generates principal block box + marker box
        // If this is a list-item, inject a ::marker pseudo-element as its first child
        // +spec:display-property:a42905 - list-item generates ::marker pseudo-element with list-style content, principal box outer=block inner=flow
        if display_type == LayoutDisplay::ListItem {
            self.create_marker_pseudo_element(styled_dom, dom_id, node_idx);
        }

        // +spec:display-contents:376f2e - display:contents removes principal box, children render normally
        // +spec:display-contents:3c7066 - display:contents strips element from formatting tree, hoists children
        // +spec:display-contents:3f4884 - replaced elements / form controls not specially handled yet (spec note: use display:none instead)
        // +spec:display-contents:4f9129 - semantic container role preserved: children promoted but DOM structure unchanged
        // +spec:display-contents:7558e8 - display:contents is rendering-time only; DOM relationships unaffected
        // +spec:display-contents:a079e3 - display:contents generates no box; children promoted to nearest non-contents ancestor (writing-mode parent lookup skips these)
        // +spec:display-contents:e202d5 - display:contents removes principal box, children render as normal
        // +spec:display-contents:6bbdf4 - display:contents preserves semantic container role (visibility context)
        // +spec:display-property:d7a8de - display:none/contents elements generate no box; anonymous box generation ignores them
        // +spec:display-property:dc2132 - display:none and display:contents control box generation
        // display:contents - element generates no box; promote children to parent
        // +spec:display-contents:61992e - element itself generates no boxes, children promoted to parent
        // +spec:display-contents:af8feb - treated as if replaced in element tree by its contents
        // +spec:display-contents:353e71 - display:contents box generation behavior
        // +spec:display-contents:b0a76b - display:contents generates no box; children promoted to parent
        // +spec:display-property:e370af - display:contents generates no box; children promoted to parent
        //
        // +spec:display-contents:852a59 - display:contents computes to display:none for replaced elements
        // +spec:display-contents:4a524e - display:contents computes to display:none on replaced elements
        // +spec:replaced-elements:af1e68 - display:contents on replaced elements has no effect (element renders normally)
        // Per CSS Display 3 §2.5 / Appendix B: replaced elements (img, canvas, embed, object,
        // audio, iframe, video, input, textarea, select, br, wbr, meter, progress)
        // and similar cannot be "un-boxed" — display:contents becomes display:none.
        if display_type == LayoutDisplay::Contents && is_replaced_element(node_data) {
            // Treat as display:none — remove node from parent and skip children
            if let Some(parent) = parent_idx {
                if let Some(p) = self.nodes.get_mut(parent) {
                    p.children.retain(|&c| c != node_idx);
                }
            }
            if let Some(node) = self.nodes.get_mut(node_idx) {
                node.computed_style.display = LayoutDisplay::None;
                node.formatting_context = FormattingContext::None;
            }
            return Ok(node_idx);
        }

        if display_type == LayoutDisplay::Contents {
            // Remove the node we just created — it shouldn't generate a box
            if let Some(parent) = parent_idx {
                if let Some(p) = self.nodes.get_mut(parent) {
                    p.children.retain(|&c| c != node_idx);
                }
            }
            // Process children as if they belong to the parent (or root if no parent)
            let effective_parent = parent_idx.unwrap_or(node_idx);
            for child_dom_id in layout_children(styled_dom, dom_id) {
                self.process_node(
                    styled_dom,
                    child_dom_id,
                    Some(effective_parent),
                    debug_messages,
                )?;
            }
            return Ok(node_idx);
        }

        match display_type {
            LayoutDisplay::Block
            | LayoutDisplay::InlineBlock
            | LayoutDisplay::FlowRoot
            | LayoutDisplay::ListItem => {
                self.process_block_children(styled_dom, dom_id, node_idx, debug_messages)?;
            }
            // +spec:table-layout:d52e09 - display:table/inline-table cause element to behave like a table element
            // +spec:table-layout:360da0 - table display values cause table formatting behavior
            LayoutDisplay::Table | LayoutDisplay::InlineTable => {
                self.process_table_children(styled_dom, dom_id, node_idx, debug_messages)?;
            }
            LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup => {
                self.process_table_row_group_children(
                    styled_dom,
                    dom_id,
                    node_idx,
                    debug_messages,
                )?;
            }
            LayoutDisplay::TableRow => {
                self.process_table_row_children(styled_dom, dom_id, node_idx, debug_messages)?;
            }
            LayoutDisplay::TableColumn => {
                // +spec:table-layout:77974f - Stage 1: all children of table-column treated as display:none
                // +spec:table-layout:c8dc69 - Stage 1: remove irrelevant boxes from table-column
                // CSS 2.2 §17.2.1: "All child boxes of a 'table-column' parent are
                // treated as if they had 'display: none'." - skip all children.
            }
            LayoutDisplay::TableColumnGroup => {
                // CSS 2.2 §17.2.1: "If a child C of a 'table-column-group' parent is not
                // a 'table-column' box, then it is treated as if it had 'display: none'."
                for child_dom_id in layout_children(styled_dom, dom_id) {
                    let child_display = get_display_type(styled_dom, child_dom_id);
                    if child_display == LayoutDisplay::TableColumn {
                        self.process_node(
                            styled_dom,
                            child_dom_id,
                            Some(node_idx),
                            debug_messages,
                        )?;
                    }
                    // Non-table-column children are suppressed (treated as display:none)
                }
            }
            // Inline, TableCell, etc., have their children processed as part of their
            // formatting context layout and don't require anonymous box generation at this stage.
            // of table-internal display values is handled via blockify_flex_item_if_table_internal
            _ => {
                // +spec:display-contents:34008d - display:none elements generate no boxes; excluded from formatting structure
                // +spec:display-property:1f38b2 - display:none creates no box at all, filter from layout tree
                // +spec:display-property:eb53f7 - display:none suppresses box generation; visibility:hidden boxes still affect layout
                // Filter out display: none children - they don't participate in layout
                // +spec:display-property:d1600a - display:none suppresses box generation; visibility:hidden boxes still affect layout
                // ALSO filter out whitespace-only text nodes for Flex/Grid/etc containers
                // to prevent them from becoming unwanted anonymous items.
                let children: Vec<NodeId> = layout_children(styled_dom, dom_id)
                    .into_iter()
                    // +spec:display-property:9f02c6 - display:none elements generate no boxes
                    .filter(|&child_id| {
                        // +spec:display-property:3b507e - display:none excludes subtree from box tree
                        if get_display_type(styled_dom, child_id) == LayoutDisplay::None {
                            return false;
                        }
                        // Check for whitespace-only text
                        let node_data = &styled_dom.node_data.as_container()[child_id];
                        if let NodeType::Text(text) = node_data.get_node_type() {
                            // Skip if text is empty or just whitespace
                            return !text.as_str().trim().is_empty();
                        }
                        true
                    })
                    .collect();

                let is_flex_or_grid = matches!(
                    display_type,
                    LayoutDisplay::Flex
                        | LayoutDisplay::InlineFlex
                        | LayoutDisplay::Grid
                        | LayoutDisplay::InlineGrid
                );

                for child_dom_id in children {
                    // +spec:display-property:934c84 - table wrapper box generation: display:table/inline-table generates a principal block container (table wrapper box) that establishes BFC and contains the table box + caption boxes
                    // +spec:width-calculation:59d456 - table wrapper box is block-level, establishes BFC (CSS 2.2 §17.4)
                    // the table wrapper box becomes the flex item; align-self applies to the
                    // wrapper, flex longhands apply to the inner table box, caption contents
                    // contribute to wrapper min/max-content sizes
                    let child_display = get_display_type(styled_dom, child_dom_id);
                    if is_flex_or_grid && child_display.creates_table_context() {
                        let wrapper_idx = self.create_anonymous_node(
                            node_idx,
                            AnonymousBoxType::TableWrapper,
                            FormattingContext::Block {
                                establishes_new_context: true,
                            },
                        );
                        self.process_node(
                            styled_dom,
                            child_dom_id,
                            Some(wrapper_idx),
                            debug_messages,
                        )?;
                    } else {
                        let child_idx = self.process_node(
                            styled_dom,
                            child_dom_id,
                            Some(node_idx),
                            debug_messages,
                        )?;
                        // table-internal flex items are blockified, preventing anonymous table
                        // box generation (e.g. two display:table-cell flex items become two
                        // separate display:block flex items)
                        if is_flex_or_grid {
                            blockify_flex_item_if_table_internal(&mut self.nodes, child_idx);
                        }
                    }
                }
            }
        }
        Ok(node_idx)
    }

    // +spec:display-property:5572e7 - Anonymous block boxes: wrap inline runs when block container has mixed block/inline children
    // +spec:display-property:090043 - Anonymous block box properties inherited from enclosing non-anonymous box; non-inherited props get initial values
    // +spec:display-property:7b9f7a - Block-level vs inline-level classification and anonymous block box creation
    // +spec:display-property:078fe5 - Anonymous block boxes wrapping inline content in mixed block/inline contexts
    // +spec:display-property:8d8ef3 - block container anonymous box generation: wraps inline runs in anonymous block boxes to ensure block containers contain only block-level or only inline-level boxes
    // +spec:display-property:1fe2be - inline box construction with anonymous text interspersed with inline elements
    // +spec:display-property:be80e3 - Anonymous inline boxes: text in block containers treated as anonymous inlines, whitespace-only runs collapsed
    /// Handles children of a block-level element, creating anonymous block
    /// wrappers for consecutive runs of inline-level children if necessary.
    // +spec:display-property:b73c50 - blockify inline content by wrapping in anonymous block containers
    fn process_block_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        // Filter out display: none children - they don't participate in layout
        let children: Vec<NodeId> = layout_children(styled_dom, parent_dom_id)
            .into_iter()
            .filter(|&child_id| get_display_type(styled_dom, child_id) != LayoutDisplay::None)
            .collect();

        // Debug: log which children we found
        if let Some(msgs) = debug_messages.as_mut() {
            msgs.push(LayoutDebugMessage::info(format!(
                "[process_block_children] DOM node {} has {} children: {:?}",
                parent_dom_id.index(),
                children.len(),
                children.iter().map(NodeId::index).collect::<Vec<_>>()
            )));
        }

        let has_block_child = children.iter().any(|&id| is_block_level(styled_dom, id));

        if let Some(msgs) = debug_messages.as_mut() {
            msgs.push(LayoutDebugMessage::info(format!(
                "[process_block_children] has_block_child={}, children display types: {:?}",
                has_block_child,
                children
                    .iter()
                    .map(|c| {
                        let dt = get_display_type(styled_dom, *c);
                        let is_block = is_block_level(styled_dom, *c);
                        format!("{}:{:?}(block={})", c.index(), dt, is_block)
                    })
                    .collect::<Vec<_>>()
            )));
        }

        if !has_block_child {
            // All children are inline, no anonymous boxes needed.
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[process_block_children] All inline, processing {} children directly",
                    children.len()
                )));
            }
            for child_id in children {
                self.process_node(styled_dom, child_id, Some(parent_idx), debug_messages)?;
            }
            return Ok(());
        }

        // Mixed block and inline content requires anonymous wrappers.
        let mut inline_run = Vec::new();

        for child_id in children {
            if is_block_level(styled_dom, child_id) {
                // +spec:display-contents:02a534 - contiguous text sequences with no text don't generate boxes
                // End the current inline run — but skip if all nodes are whitespace-only text.
                // +spec:display-property:7d1570 - whitespace-only text that would be collapsed does not generate anonymous inline boxes
                // +spec:white-space-processing:b32f69 - whitespace-only inline runs between blocks don't generate anonymous inline boxes
                // CSS 2.1 §9.2.2.1: "White space content that would subsequently be collapsed
                // away according to the 'white-space' property does not generate any anonymous
                // inline boxes."
                if !inline_run.is_empty() {
                    self.flush_inline_run(styled_dom, parent_idx, &mut inline_run, debug_messages)?;
                }
                // Process the block-level child directly
                if let Some(msgs) = debug_messages.as_mut() {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[process_block_children] Processing block child DOM {}",
                        child_id.index()
                    )));
                }
                self.process_node(styled_dom, child_id, Some(parent_idx), debug_messages)?;
            } else {
                inline_run.push(child_id);
            }
        }
        // Process any remaining inline children at the end — skip if all whitespace
        if !inline_run.is_empty() {
            self.flush_inline_run(styled_dom, parent_idx, &mut inline_run, debug_messages)?;
        }

        Ok(())
    }

    // +spec:table-layout:6bb84e - Anonymous table object generation (stages 1-3: remove irrelevant boxes, generate missing child wrappers, generate missing parents)
    // +spec:table-layout:77974f - Stage 2: generate missing child wrappers for table/inline-table
    // +spec:table-layout:c8dc69 - Stage 2: wrap non-proper children in anonymous table-row
    // +spec:display-property:6f8f13 - anonymous table object generation (§17.2.1): suppress table-column/table-column-group children, wrap non-proper children in anonymous rows/cells
    fn process_table_level_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
        is_expected_child: fn(LayoutDisplay) -> bool,
        anon_type: AnonymousBoxType,
        anon_fc: FormattingContext,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        let parent_display = get_display_type(styled_dom, parent_dom_id);
        let mut non_matching_children = Vec::new();

        for child_id in layout_children(styled_dom, parent_dom_id) {
            if should_skip_for_table_structure(styled_dom, child_id, parent_display) {
                continue;
            }

            let child_display = get_display_type(styled_dom, child_id);

            if is_expected_child(child_display) {
                if !non_matching_children.is_empty() {
                    let anon_idx = self.create_anonymous_node(parent_idx, anon_type, anon_fc);
                    #[allow(clippy::iter_with_drain)]
                    // accumulator Vec reused across runs; drain(..) empties it while retaining the allocation
                    for np_id in non_matching_children.drain(..) {
                        self.process_node(styled_dom, np_id, Some(anon_idx), debug_messages)?;
                    }
                }
                self.process_node(styled_dom, child_id, Some(parent_idx), debug_messages)?;
            } else {
                non_matching_children.push(child_id);
            }
        }

        if !non_matching_children.is_empty() {
            let anon_idx = self.create_anonymous_node(parent_idx, anon_type, anon_fc);
            for np_id in non_matching_children {
                self.process_node(styled_dom, np_id, Some(anon_idx), debug_messages)?;
            }
        }

        Ok(())
    }

    fn process_table_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        self.process_table_level_children(
            styled_dom,
            parent_dom_id,
            parent_idx,
            is_proper_table_child,
            AnonymousBoxType::TableRow,
            FormattingContext::TableRow,
            debug_messages,
        )
    }

    fn process_table_row_group_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        self.process_table_level_children(
            styled_dom,
            parent_dom_id,
            parent_idx,
            |d| d == LayoutDisplay::TableRow,
            AnonymousBoxType::TableRow,
            FormattingContext::TableRow,
            debug_messages,
        )
    }

    fn process_table_row_children(
        &mut self,
        styled_dom: &StyledDom,
        parent_dom_id: NodeId,
        parent_idx: usize,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        self.process_table_level_children(
            styled_dom,
            parent_dom_id,
            parent_idx,
            |d| d == LayoutDisplay::TableCell,
            AnonymousBoxType::TableCell,
            FormattingContext::Block {
                establishes_new_context: true,
            },
            debug_messages,
        )
    }
    // +spec:display-property:7d1570 - whitespace-only text that would be collapsed does not generate anonymous inline boxes
    // +spec:white-space-processing:b32f69 - whitespace-only inline runs between blocks don't generate anonymous inline boxes
    fn flush_inline_run(
        &mut self,
        styled_dom: &StyledDom,
        parent_idx: usize,
        inline_run: &mut Vec<NodeId>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        let all_whitespace = inline_run
            .iter()
            .all(|id| is_whitespace_only_text(styled_dom, *id));
        if all_whitespace {
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[process_block_children] Skipping whitespace-only inline run: {:?}",
                    inline_run
                        .iter()
                        .map(|c: &NodeId| c.index())
                        .collect::<Vec<_>>()
                )));
            }
            inline_run.clear();
        } else {
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[process_block_children] Creating anon wrapper for inline run: {:?}",
                    inline_run
                        .iter()
                        .map(|c: &NodeId| c.index())
                        .collect::<Vec<_>>()
                )));
            }
            let anon_idx = self.create_anonymous_node(
                parent_idx,
                AnonymousBoxType::InlineWrapper,
                FormattingContext::Block {
                    establishes_new_context: true,
                },
            );
            for inline_child_id in inline_run.drain(..) {
                self.process_node(styled_dom, inline_child_id, Some(anon_idx), debug_messages)?;
            }
        }
        Ok(())
    }

    // +spec:display-property:52f497 - anonymous inline boxes inherit inheritable properties from block parent; non-inherited properties use initial values (dom_node_id: None + BoxProps::default())
    /// CSS 2.2 Section 17.2.1 - Anonymous box generation:
    /// "In this process, inline-level boxes are wrapped in anonymous boxes as needed
    /// to satisfy the constraints of the table model."
    ///
    // +spec:display-property:ee83bf - Anonymous box generation: boxes not associated with elements, inheriting through box tree parentage
    /// Helper to create an anonymous node in the tree.
    /// Anonymous boxes don't have a corresponding DOM node and are used to enforce
    /// the CSS box model structure (e.g., wrapping inline content in blocks,
    /// or creating missing table structural elements).
    // +spec:display-property:6ff51a - anonymous block boxes have no styles (box_props default), so parent element properties still apply to its content
    pub fn create_anonymous_node(
        &mut self,
        parent: usize,
        anon_type: AnonymousBoxType,
        fc: FormattingContext,
    ) -> usize {
        let index = self.nodes.len();

        // +spec:display-property:e67146 - Anonymous boxes inherit from enclosing non-anonymous box; non-inherited props use initial values
        let parent_fc = self.nodes.get(parent).map(|n| n.formatting_context);

        self.nodes.push(LayoutNode {
            // ── HOT ──
            box_props: BoxProps::default(),
            dom_node_id: None,
            children: Vec::new(),
            used_size: None,
            formatting_context: fc,
            parent: Some(parent),
            // ── WARM ──
            intrinsic_sizes: None,
            baseline: None,
            inline_layout_result: None,
            inline_content_cache: None,
            scrollbar_info: None,
            relative_position: None,
            overflow_content_size: None,
            taffy_cache: TaffyCache::new(),
            measured_content_sizes: (None, None),
            computed_style: ComputedLayoutStyle::default(),
            pseudo_element: None,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
            parent_formatting_context: parent_fc,
            ifc_membership: None,
            containing_block_index: None,
            // ── COLD ──
            anonymous_type: Some(anon_type),
            preview_byte_range: None,
            node_data_fingerprint: NodeDataFingerprint::default(),
            subtree_hash: SubtreeHash(0),
            dirty_flag: DirtyFlag::Layout,
            unresolved_box_props: crate::solver3::geometry::UnresolvedBoxProps::default(),
            ifc_id: None,
        });

        self.nodes[parent].children.push(index);
        index
    }

    /// Creates a `::marker` pseudo-element as the first child of a list-item.
    ///
    /// Per CSS Lists Module Level 3, Section 3.1:
    /// "For elements with display: list-item, user agents must generate a
    /// `::marker` pseudo-element as the first child of the principal box."
    ///
    /// The `::marker` references the same DOM node as its parent list-item,
    /// but is marked as a pseudo-element for proper counter resolution and styling.
    pub fn create_marker_pseudo_element(
        &mut self,
        styled_dom: &StyledDom,
        list_item_dom_id: NodeId,
        list_item_idx: usize,
    ) -> usize {
        let index = self.nodes.len();

        // The marker references the same DOM node as the list-item
        // This is important for style resolution (the marker inherits from the list-item)
        let parent_fc = self.nodes.get(list_item_idx).map(|n| n.formatting_context);
        self.nodes.push(LayoutNode {
            // ── HOT ──
            box_props: BoxProps::default(),
            dom_node_id: Some(list_item_dom_id),
            children: Vec::new(),
            used_size: None,
            formatting_context: FormattingContext::Inline,
            parent: Some(list_item_idx),
            // ── WARM ──
            intrinsic_sizes: None,
            baseline: None,
            inline_layout_result: None,
            inline_content_cache: None,
            scrollbar_info: None,
            relative_position: None,
            overflow_content_size: None,
            taffy_cache: TaffyCache::new(),
            measured_content_sizes: (None, None),
            computed_style: ComputedLayoutStyle::default(),
            pseudo_element: Some(PseudoElement::Marker),
            escaped_top_margin: None,
            escaped_bottom_margin: None,
            parent_formatting_context: parent_fc,
            ifc_membership: None,
            containing_block_index: None,
            // ── COLD ──
            anonymous_type: None,
            preview_byte_range: None,
            node_data_fingerprint: NodeDataFingerprint::default(),
            subtree_hash: SubtreeHash(0),
            dirty_flag: DirtyFlag::Layout,
            unresolved_box_props: crate::solver3::geometry::UnresolvedBoxProps::default(),
            ifc_id: None,
        });

        // Insert as FIRST child (per spec)
        self.nodes[list_item_idx].children.insert(0, index);

        // Register with DOM mapping for counter resolution
        self.dom_to_layout
            .entry(list_item_dom_id)
            .or_default()
            .push(index);

        index
    }

    // M12.7: returns `usize`, NOT `Result<usize>` — this fn has no error path
    // (always `Ok(index)`). The `Result` forced callers to use `?`, whose lifted
    // discriminant decode mis-reads the Ok as Err (the rc=5 root cause: reconcile
    // reaches this fn but returns Err before its own Ok). Dropping the Result
    // removes that mis-lifting `?`.
    /// Apply CSS Display 3 §2.7/§2.8 blockification to a freshly-created node:
    /// a flex/grid item (or root / abs-pos / floated box) whose specified display
    /// is inline-level computes to its block-level equivalent.
    ///
    /// `process_node` (the full tree build) does this inline, but the INCREMENTAL
    /// tree builder (`cache.rs` reconcile → `create_node_from_dom`) bypassed it.
    /// Without it, a replaced inline flex item — e.g. an `<img>` canvas with
    /// `flex-grow: 1` (`AzulPaint`) — stayed inline, so its flex-grow was ignored
    /// and it was laid out 300×0 (the replaced-element default width, 0 height).
    /// Must be called AFTER the node is created and AFTER its parent's
    /// formatting context is known (the build is top-down, so the parent exists).
    pub fn blockify_node_display(
        &mut self,
        styled_dom: &StyledDom,
        dom_id: NodeId,
        node_idx: usize,
        parent_idx: Option<usize>,
    ) {
        let node_data = &styled_dom.node_data.as_container()[dom_id];
        // CSS Display 3 §2.4: a replaced element with a layout-internal display
        // value uses 'inline' — so it's inline-level and thus blockifiable.
        let raw_display = {
            let d = get_display_type(styled_dom, dom_id);
            if d.is_layout_internal() && is_replaced_element(node_data) {
                LayoutDisplay::Inline
            } else {
                d
            }
        };
        let (position, float) = self
            .nodes
            .get(node_idx)
            .map(|n| (n.computed_style.position, n.computed_style.float))
            .unwrap_or_default();
        let is_absolute_or_fixed =
            matches!(position, LayoutPosition::Absolute | LayoutPosition::Fixed);
        let is_floated = float != LayoutFloat::None;
        let is_root = parent_idx.is_none();
        let is_flex_grid_child = parent_idx.and_then(|p| self.nodes.get(p)).is_some_and(|n| {
            matches!(
                n.formatting_context,
                FormattingContext::Flex | FormattingContext::Grid
            )
        });
        let display_type = crate::solver3::getters::get_computed_display(
            raw_display,
            is_absolute_or_fixed,
            is_floated,
            is_root,
            is_flex_grid_child,
        );
        if display_type != raw_display {
            if let Some(node) = self.nodes.get_mut(node_idx) {
                node.computed_style.display = display_type;
                node.formatting_context =
                    determine_formatting_context_for_display(styled_dom, dom_id, display_type);
            }
        }
    }

    #[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
    pub fn create_node_from_dom(
        &mut self,
        styled_dom: &StyledDom,
        dom_id: NodeId,
        parent: Option<usize>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> usize {
        let index = self.nodes.len();
        // as IT sees it). If this is 0 but build() sees 0 nodes, the push is lost
        // between here and build (builder &mut threading); if garbage, len mis-reads.
        {
            let _ = (0xCE00_0000u32 | (index as u32 & 0xffff));
        }
        let parent_fc = parent.and_then(|p| self.nodes.get(p).map(|n| n.formatting_context));
        // this is reached but step A is NOT, collect_box_props diverges; if this is
        // NOT reached, the parent Option discriminant mis-lifts (None→Some garbage).
        {
            let _ = (0xCD00_0001u32 | (u32::from(parent_fc.is_some()) << 8));
        }
        let collected = collect_box_props(styled_dom, dom_id, debug_messages, self.viewport_size);
        {
            let _ = (0xCA00_0001u32);
        }
        self.nodes.push(LayoutNode {
            // ── HOT ──
            box_props: collected.resolved,
            dom_node_id: Some(dom_id),
            children: Vec::new(),
            used_size: None,
            formatting_context: determine_formatting_context(styled_dom, dom_id),
            parent,
            // ── WARM ──
            intrinsic_sizes: None,
            baseline: None,
            inline_layout_result: None,
            inline_content_cache: None,
            scrollbar_info: None,
            relative_position: None,
            overflow_content_size: None,
            taffy_cache: TaffyCache::new(),
            measured_content_sizes: (None, None),
            // +spec:overflow:8f9f7e - viewport overflow propagation: visible→auto, clip→hidden
            computed_style: {
                let mut style = compute_layout_style(styled_dom, dom_id);
                if parent.is_none() {
                    // CSS Overflow 3 §3.3: If visible is applied to the viewport,
                    // it must be interpreted as auto. If clip is applied to the
                    // viewport, it must be interpreted as hidden.
                    use azul_css::props::layout::LayoutOverflow;
                    if style.overflow_x == LayoutOverflow::Visible {
                        style.overflow_x = LayoutOverflow::Auto;
                    } else if style.overflow_x == LayoutOverflow::Clip {
                        style.overflow_x = LayoutOverflow::Hidden;
                    }
                    if style.overflow_y == LayoutOverflow::Visible {
                        style.overflow_y = LayoutOverflow::Auto;
                    } else if style.overflow_y == LayoutOverflow::Clip {
                        style.overflow_y = LayoutOverflow::Hidden;
                    }
                }
                style
            },
            pseudo_element: None,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
            parent_formatting_context: parent_fc,
            ifc_membership: None,
            containing_block_index: None,
            // ── COLD ──
            anonymous_type: None,
            preview_byte_range: None,
            node_data_fingerprint: NodeDataFingerprint::compute(
                &styled_dom.node_data.as_container()[dom_id],
                styled_dom
                    .styled_nodes
                    .as_container()
                    .get(dom_id)
                    .map(|n| &n.styled_node_state),
            ),
            subtree_hash: SubtreeHash(0),
            dirty_flag: DirtyFlag::Layout,
            unresolved_box_props: collected.unresolved,
            ifc_id: None,
        });
        {
            let _ = (0xCB00_0001u32 | ((self.nodes.len() as u32 & 0xff) << 8));
        }
        if let Some(p) = parent {
            self.nodes[p].children.push(index);
        }
        self.dom_to_layout.entry(dom_id).or_default().push(index);
        // DEBUG (2026-06-02 children-None tree-build): count create_node_from_dom
        // calls @0x40500 + record each dom_id into a 14-slot ring @0x40504. REVERT
        // before commit. Runs only in lifted wasm (server lifts, never runs natively).
        unsafe {
            let c = crate::az_mark_read(0x40500);
            crate::az_mark(0x60500_u32, (c.wrapping_add(1)));
            if (c as usize) < 14 {
                crate::az_mark(
                    (0x40504 + (c as usize) * 4) as u32,
                    (0xDD00_0000 | (dom_id.index() as u32 & 0xffff)),
                );
            }
        }
        index
    }

    /// O3-render: materialize pending SPLIT previews as layout nodes — the
    /// "fake structural edit" becomes VISIBLE. For each recorded split whose
    /// node is an IFC leaf (no layout children; the `<p>text…</p>` shape),
    /// the node becomes part 1 (content byte range `[0, at)`), and a SECOND
    /// node with the SAME `dom_node_id` (the `dom_to_layout` multimap seat)
    /// and `AnonymousBoxType::SplitPreviewPart` shows `[at, ∞)` — a real
    /// layout identity like any anonymous box, NO DOM mutation, no stale
    /// `NodeIds`. Runs as a post-pass just before `build()` on BOTH tree-build
    /// paths (full + reconcile).
    ///
    /// v1 renders text-byte splits; element-children splits stay
    /// recorded-only (the child-routing per part is the staged follow-up).
    pub fn apply_split_previews(
        &mut self,
        overlay: Option<&crate::overlay::ContentOverlay>,
        styled_dom: &StyledDom,
    ) {
        let Some(overlay) = overlay else { return };
        let pending = overlay.pending_structure(styled_dom.dom_id);
        if pending.is_empty() {
            return;
        }
        let node_data = styled_dom.node_data.as_container();
        let hierarchy = styled_dom.node_hierarchy.as_container();

        for entry in pending {
            let (node, at) = match &entry.preview {
                crate::overlay::StructuralPreview::Split { node, at, .. } => (node, at),
                crate::overlay::StructuralPreview::Remove { parent, start, end }
                | crate::overlay::StructuralPreview::Replace {
                    parent, start, end, ..
                } => {
                    // Removal side renders NOW: the range's layout children
                    // detach from the tree (unreachable from the root, so
                    // never positioned nor painted). Replace's PENDING
                    // content is the staged follow-up — it needs the styled
                    // materialization of a raw `Dom` fragment; until then a
                    // Replace previews as its removal half.
                    self.apply_range_suppression_preview(
                        *parent,
                        *start,
                        *end,
                        &ordinal_map(&hierarchy, *parent),
                    );
                    continue;
                }
                crate::overlay::StructuralPreview::Merge { first, second } => {
                    // Backspace-at-start UX: `second` disappears from its
                    // parent, its layout children move (subtrees wholesale)
                    // onto the END of `first` — the inverse mechanics of the
                    // element split above.
                    self.apply_merge_preview(*first, *second);
                    continue;
                }
                crate::overlay::StructuralPreview::Insert { .. } => {
                    // Staged: rendering a pending `Dom` needs a styled
                    // sub-mount (fragment styling against the retained author
                    // css). Resolvers already expose it (children_for_node);
                    // the box tree does not materialize it yet.
                    continue;
                }
            };
            let Some(indices) = self.dom_to_layout.get(node) else {
                continue;
            };
            if indices.len() != 1 {
                continue; // already split (or exotic mapping)
            }
            let idx = indices[0];

            // Child-boundary split (no text byte): a container splits BETWEEN
            // its children — a `<ul>` between `<li>`s, a section between
            // paragraphs. Children before `at.child_index` stay in part 1;
            // the rest MOVE (wholesale, subtrees intact) to the preview part.
            let Some(byte) = at.text_byte.into_option() else {
                // Child-boundary split: layout children route to a part by
                // their DOM position.
                self.apply_element_split_preview(
                    idx,
                    *node,
                    at.child_index,
                    &ordinal_map(&hierarchy, *node),
                );
                continue;
            };
            // v1 gate: plain-text IFC roots — every layout child must be
            // text-backed (or anonymous inline machinery). Element children
            // need per-part routing (the staged follow-up).
            let all_text_children = self.nodes[idx].children.iter().all(|&c| {
                self.nodes.get(c).is_none_or(|n| {
                    n.dom_node_id.is_none_or(|d| {
                        node_data
                            .get(d)
                            .is_some_and(|nd| matches!(nd.get_node_type(), NodeType::Text(_)))
                    })
                })
            });
            if !all_text_children {
                continue;
            }

            // Flat byte offset of the split within the node's concatenated
            // direct-text content (the same flattening fc collects).
            let mut flat_at: u32 = 0;
            let mut child = hierarchy.get(*node).and_then(|h| h.first_child_id(*node));
            let mut i: u32 = 0;
            while let Some(c) = child {
                if i == at.child_index {
                    break;
                }
                if let Some(nd) = node_data.get(c) {
                    if let NodeType::Text(t) = nd.get_node_type() {
                        flat_at += u32::try_from(t.as_str().len()).unwrap_or(u32::MAX);
                    }
                }
                i += 1;
                child = hierarchy
                    .get(c)
                    .and_then(azul_core::styled_dom::NodeHierarchyItem::next_sibling_id);
            }
            flat_at += byte;

            self.nodes[idx].preview_byte_range = Some((0, flat_at));
            self.nodes[idx].inline_layout_result = None;
            self.nodes[idx].dirty_flag = DirtyFlag::Layout;

            let part2_idx = self.nodes.len();
            let mut part2 = self.nodes[idx].clone();
            part2.children = Vec::new();
            part2.preview_byte_range = Some((flat_at, u32::MAX));
            part2.anonymous_type = Some(AnonymousBoxType::SplitPreviewPart);
            part2.inline_layout_result = None;
            part2.taffy_cache = TaffyCache::new();
            part2.measured_content_sizes = (None, None);
            part2.dirty_flag = DirtyFlag::Layout;
            let parent = part2.parent;
            self.nodes.push(part2);
            if let Some(p) = parent {
                let pos = self.nodes[p]
                    .children
                    .iter()
                    .position(|&c| c == idx)
                    .map_or(self.nodes[p].children.len(), |i| i + 1);
                self.nodes[p].children.insert(pos, part2_idx);
                self.nodes[p].dirty_flag = DirtyFlag::Layout;
            }
            self.dom_to_layout.entry(*node).or_default().push(part2_idx);
        }
    }

    /// Child-boundary split preview: partition `idx`'s layout children by DOM
    /// ordinal — `< split_index` stays, the rest MOVE to a fresh
    /// `SplitPreviewPart` sibling that shares `dom_node`'s id (the anon-box
    /// model: rendering knows, callbacks stay blind). Subtrees move
    /// wholesale; nothing is byte-sliced.
    ///
    /// Anonymous wrappers (mixed inline/block content puts inline runs inside
    /// anonymous blocks, so layout children are NOT 1:1 with DOM children)
    /// route through their wrapped runs' ordinals: a wrapper wholly on one
    /// side moves wholesale; a wrapper STRADDLING the split point splits
    /// itself — its second half becomes a fresh anonymous wrapper inside the
    /// preview part, mirroring how the builder would have wrapped the
    /// post-apply trees.
    fn apply_element_split_preview(
        &mut self,
        idx: usize,
        dom_node: NodeId,
        split_index: u32,
        ordinal_of: &BTreeMap<NodeId, u32>,
    ) {
        let children = self.nodes[idx].children.clone();
        let mut keep: Vec<usize> = Vec::new();
        let mut moved: Vec<usize> = Vec::new();
        for c in children {
            if let Some(d) = self.nodes[c].dom_node_id {
                // A layout child mapping to something that is NOT a direct
                // DOM child (unexpected) degrades to "stays" — part 1 keeps
                // rendering it rather than dropping content from a preview.
                match ordinal_of.get(&d) {
                    Some(&o) if o >= split_index => moved.push(c),
                    _ => keep.push(c),
                }
                continue;
            }
            // Anonymous wrapper: side = the ordinal span of its wrapped runs.
            match self.wrapper_ordinal_span(c, ordinal_of) {
                Some((_, hi)) if hi < split_index => keep.push(c),
                Some((lo, _)) if lo >= split_index => moved.push(c),
                Some(_) => {
                    // Straddling: the wrapper itself splits at the boundary.
                    keep.push(c);
                    if let Some(second) = self.split_anonymous_wrapper(c, split_index, ordinal_of) {
                        moved.push(second);
                    }
                }
                // Empty / opaque wrapper (no mapped runs): keep in part 1.
                None => keep.push(c),
            }
        }
        if moved.is_empty() {
            return; // split at/after the end: nothing to preview
        }

        self.nodes[idx].children = keep;
        self.nodes[idx].inline_layout_result = None;
        self.nodes[idx].dirty_flag = DirtyFlag::Layout;

        let part2_idx = self.nodes.len();
        let mut part2 = self.nodes[idx].clone();
        part2.children.clone_from(&moved);
        part2.anonymous_type = Some(AnonymousBoxType::SplitPreviewPart);
        part2.inline_layout_result = None;
        part2.taffy_cache = TaffyCache::new();
        part2.measured_content_sizes = (None, None);
        part2.dirty_flag = DirtyFlag::Layout;
        let parent = part2.parent;
        self.nodes.push(part2);

        for &m in &moved {
            self.nodes[m].parent = Some(part2_idx);
            self.nodes[m].dirty_flag = DirtyFlag::Layout;
        }
        if let Some(p) = parent {
            let pos = self.nodes[p]
                .children
                .iter()
                .position(|&c| c == idx)
                .map_or(self.nodes[p].children.len(), |i| i + 1);
            self.nodes[p].children.insert(pos, part2_idx);
            self.nodes[p].dirty_flag = DirtyFlag::Layout;
        }
        self.dom_to_layout
            .entry(dom_node)
            .or_default()
            .push(part2_idx);
    }

    /// The min/max DOM ordinal among an anonymous wrapper's DIRECT layout
    /// children that map to real DOM children of the node being edited.
    /// `None` = the wrapper wraps nothing addressable (empty, or generated
    /// content only).
    fn wrapper_ordinal_span(
        &self,
        wrapper: usize,
        ordinal_of: &BTreeMap<NodeId, u32>,
    ) -> Option<(u32, u32)> {
        let mut span: Option<(u32, u32)> = None;
        for &c in &self.nodes[wrapper].children {
            let Some(o) = self.nodes[c]
                .dom_node_id
                .and_then(|d| ordinal_of.get(&d).copied())
            else {
                continue;
            };
            span = Some(match span {
                None => (o, o),
                Some((lo, hi)) => (lo.min(o), hi.max(o)),
            });
        }
        span
    }

    /// Split a straddling anonymous wrapper at `split_index`: runs with
    /// ordinal `>= split_index` move into a NEW wrapper of the same shape
    /// (returned; the caller seats it in the preview part). Runs that map to
    /// nothing stay in the first half. Returns `None` when nothing moves.
    fn split_anonymous_wrapper(
        &mut self,
        wrapper: usize,
        split_index: u32,
        ordinal_of: &BTreeMap<NodeId, u32>,
    ) -> Option<usize> {
        let children = core::mem::take(&mut self.nodes[wrapper].children);
        let (keep, moved): (Vec<usize>, Vec<usize>) = children.into_iter().partition(|&c| {
            self.nodes[c]
                .dom_node_id
                .and_then(|d| ordinal_of.get(&d).copied())
                .is_none_or(|o| o < split_index)
        });
        self.nodes[wrapper].children = keep;
        self.nodes[wrapper].inline_layout_result = None;
        self.nodes[wrapper].taffy_cache = TaffyCache::new();
        self.nodes[wrapper].measured_content_sizes = (None, None);
        self.nodes[wrapper].dirty_flag = DirtyFlag::Layout;
        if moved.is_empty() {
            return None;
        }

        let second_idx = self.nodes.len();
        let mut second = self.nodes[wrapper].clone();
        second.children = moved;
        second.inline_layout_result = None;
        second.taffy_cache = TaffyCache::new();
        second.measured_content_sizes = (None, None);
        second.dirty_flag = DirtyFlag::Layout;
        self.nodes.push(second);
        let moved_children = self.nodes[second_idx].children.clone();
        for m in moved_children {
            self.nodes[m].parent = Some(second_idx);
            self.nodes[m].dirty_flag = DirtyFlag::Layout;
        }
        // The caller re-parents `second_idx` into the preview part; anonymous
        // wrappers have no dom_node_id, so dom_to_layout needs no entry.
        Some(second_idx)
    }

    /// Remove/Replace preview: detach the layout children of `parent_dom`
    /// whose DOM ordinal falls in `[start, end)`. Detached subtrees stay in
    /// the node arena but are unreachable from the root — never positioned,
    /// never painted (the same fate as any orphaned builder node).
    ///
    /// Anonymous wrappers route through their wrapped runs: wholly inside the
    /// range → the wrapper detaches; straddling → only the wrapper's in-range
    /// runs detach (the wrapper stays for the survivors).
    fn apply_range_suppression_preview(
        &mut self,
        parent_dom: NodeId,
        start: u32,
        end: u32,
        ordinal_of: &BTreeMap<NodeId, u32>,
    ) {
        let Some(indices) = self.dom_to_layout.get(&parent_dom) else {
            return;
        };
        if indices.len() != 1 {
            return; // exotic mapping (already part-split); one preview at a time
        }
        let idx = indices[0];
        let children = self.nodes[idx].children.clone();
        let mut kept: Vec<usize> = Vec::new();
        let mut changed = false;
        for c in children {
            if let Some(d) = self.nodes[c].dom_node_id {
                let in_range = ordinal_of.get(&d).is_some_and(|&o| o >= start && o < end);
                if in_range {
                    changed = true;
                } else {
                    kept.push(c);
                }
                continue;
            }
            match self.wrapper_ordinal_span(c, ordinal_of) {
                Some((lo, hi)) if lo >= start && hi < end => changed = true, // wholly removed
                Some((lo, hi)) if hi >= start && lo < end => {
                    // Straddling: filter the wrapper's own runs in place.
                    let before = self.nodes[c].children.len();
                    let nodes = &self.nodes;
                    let survivors: Vec<usize> = self.nodes[c]
                        .children
                        .iter()
                        .copied()
                        .filter(|&r| {
                            nodes[r]
                                .dom_node_id
                                .and_then(|d| ordinal_of.get(&d).copied())
                                .is_none_or(|o| o < start || o >= end)
                        })
                        .collect();
                    if survivors.len() != before {
                        self.nodes[c].children = survivors;
                        self.nodes[c].inline_layout_result = None;
                        self.nodes[c].taffy_cache = TaffyCache::new();
                        self.nodes[c].measured_content_sizes = (None, None);
                        self.nodes[c].dirty_flag = DirtyFlag::Layout;
                        changed = true;
                    }
                    kept.push(c);
                }
                _ => kept.push(c), // wholly outside, or opaque: keep
            }
        }
        if changed {
            self.nodes[idx].children = kept;
            self.nodes[idx].inline_layout_result = None;
            self.nodes[idx].dirty_flag = DirtyFlag::Layout;
        }
    }

    /// Merge preview: `second_dom` disappears from its parent; its layout
    /// children move (subtrees wholesale) onto the END of `first_dom`.
    fn apply_merge_preview(&mut self, first_dom: NodeId, second_dom: NodeId) {
        // Unmapped or exotic multi-part mappings are staged.
        let ((Some(&[first_idx]), Some(&[second_idx]))) = (
            self.dom_to_layout.get(&first_dom).map(Vec::as_slice),
            self.dom_to_layout.get(&second_dom).map(Vec::as_slice),
        ) else {
            return;
        };

        let moved = core::mem::take(&mut self.nodes[second_idx].children);
        for &m in &moved {
            self.nodes[m].parent = Some(first_idx);
            self.nodes[m].dirty_flag = DirtyFlag::Layout;
        }
        self.nodes[first_idx].children.extend(moved);
        self.nodes[first_idx].inline_layout_result = None;
        self.nodes[first_idx].dirty_flag = DirtyFlag::Layout;

        // Detach `second` itself from its parent (unreachable = unpainted).
        if let Some(p) = self.nodes[second_idx].parent {
            self.nodes[p].children.retain(|&c| c != second_idx);
            self.nodes[p].dirty_flag = DirtyFlag::Layout;
        }
    }

    /// Reuse an old node's cached layout under a NEW DOM identity.
    ///
    /// `new_dom_id` is what the node IS; the clone only carries what it
    /// COSTS (used size, taffy cache, inline layout result). Reconciliation
    /// matches an old child to a new one by DOM id where it can, but falls
    /// back to POSITION when the id is absent from the old child list — and
    /// a positional match pairs nodes with different ids. Copying the old
    /// id through then produced a layout node claiming to be a DOM node
    /// that has moved, or (when the DOM shrank) one that no longer exists:
    /// `compute_counters` panicked indexing `node_data[52]` on a 37-node
    /// `StyledDom`, and every quieter consumer — style lookups, node rects,
    /// damage attribution — silently read a DIFFERENT node's data.
    ///
    /// Passing `None` keeps the node anonymous (no DOM identity), which is
    /// correct for pure layout wrappers.
    pub fn clone_node_from_old(
        &mut self,
        old_node: &LayoutNode,
        parent: Option<usize>,
        new_dom_id: Option<NodeId>,
    ) -> usize {
        let index = self.nodes.len();
        let mut new_node = old_node.clone();
        new_node.parent = parent;
        new_node.parent_formatting_context =
            parent.and_then(|p| self.nodes.get(p).map(|n| n.formatting_context));
        new_node.children = Vec::new();
        new_node.dirty_flag = DirtyFlag::None;
        // The measurement cache does NOT survive the clone.
        //
        // A clone is taken because the node's own data is unchanged — but
        // "unchanged" says nothing about its SURROUNDINGS. Its cached
        // (available space -> computed size) entries were measured against
        // the previous frame's siblings, and a clone is taken precisely when
        // a sibling changed enough to re-lay the parent out. `layout_document`
        // clears this cache for `intrinsic_dirty` nodes; the clean clones
        // beside them kept theirs.
        //
        // Symptom: clicking ribbon tab 1 left tab 2 — the tab that never
        // changed state — drawing its label 1.5px lower than a fresh render
        // of the same state, with an IDENTICAL header box. The cached entry
        // was answering with a measurement taken while tab 0 was active.
        //
        // What the clone is FOR is still preserved: the tree structure, the
        // fingerprint, and `inline_layout_result` — so the text is not
        // re-shaped, only re-measured. Measured flat on
        // layout/tests/frame_perf.rs (idle 19.21 vs 19.70 ms, cold 75.96 vs
        // 74.57 ms, edit 22.72 vs 25.39 ms — all inside run-to-run noise).
        new_node.taffy_cache.clear();
        new_node.measured_content_sizes = (None, None);
        new_node.dom_node_id = new_dom_id;
        self.nodes.push(new_node);
        if let Some(p) = parent {
            self.nodes[p].children.push(index);
        }
        if let Some(dom_id) = new_dom_id {
            self.dom_to_layout.entry(dom_id).or_default().push(index);
        }
        index
    }

    #[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
    #[must_use]
    pub fn build(self, root_idx: usize) -> LayoutTree {
        let nodes = self.nodes;
        let node_count = nodes.len();

        // Flatten per-node children Vecs into a single contiguous arena.
        let total_children: usize = nodes.iter().map(|n| n.children.len()).sum();
        let mut arena = Vec::with_capacity(total_children);
        let mut offsets = Vec::with_capacity(node_count);

        // Split monolithic LayoutNodes into hot/warm/cold SoA arrays
        let mut hot_nodes = Vec::with_capacity(node_count);
        let mut warm_nodes = Vec::with_capacity(node_count);
        let mut cold_nodes = Vec::with_capacity(node_count);

        for node in nodes {
            // Flatten children into arena first
            let start = arena.len() as u32;
            let len = node.children.len() as u32;
            arena.extend_from_slice(&node.children);
            offsets.push((start, len));

            // Split into hot/warm/cold
            let (hot, warm, cold) = node.split();
            hot_nodes.push(hot);
            warm_nodes.push(warm);
            cold_nodes.push(cold);
        }

        // discriminant). If len>0 but calculate_intrinsic_recursive's
        // `tree.get(root).ok_or(InvalidTree)?` still errors, that `?`/null-check
        // mis-discriminates Some→None. If len==0, build's input was empty.
        // if build>0 but get_node_size sees 0, the tree.clone() (hashbrown) drops the map.

        LayoutTree {
            nodes: hot_nodes,
            warm: warm_nodes,
            cold: cold_nodes,
            root: root_idx,
            dom_to_layout: self
                .dom_to_layout
                .into_iter()
                .map(|(k, v)| (k, v.into_iter().map(LayoutNodeId::new).collect()))
                .collect(),
            children_arena: arena,
            children_offsets: offsets,
            // Populated by `generate_layout_tree` after the tree is built,
            // since the computation needs styled_dom for float/position lookup.
            subtree_needs_intrinsic: Vec::new(),
        }
    }
}

/// DOM ordinal per direct child of `parent` — the coordinate structural
/// previews use to route layout children (`NodePosition.child_index` space).
fn ordinal_map(
    hierarchy: &azul_core::id::NodeDataContainerRef<'_, azul_core::styled_dom::NodeHierarchyItem>,
    parent: NodeId,
) -> BTreeMap<NodeId, u32> {
    let mut ordinal_of = BTreeMap::new();
    let mut c = hierarchy.get(parent).and_then(|h| h.first_child_id(parent));
    let mut ord: u32 = 0;
    while let Some(cc) = c {
        ordinal_of.insert(cc, ord);
        ord += 1;
        c = hierarchy
            .get(cc)
            .and_then(azul_core::styled_dom::NodeHierarchyItem::next_sibling_id);
    }
    ordinal_of
}

// +spec:display-property:697082 - outer display type determines principal box's role in flow layout (block vs inline)
// +spec:display-property:0d251b - Block-level elements: display 'block', 'list-item', 'table' generate block-level boxes
// +spec:display-property:9464be - block-level vs block container distinction: not all block-level boxes are block containers (e.g. replaced elements, flex containers)
#[must_use]
pub fn is_block_level(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    matches!(
        get_display_type(styled_dom, node_id),
        LayoutDisplay::Block
            | LayoutDisplay::FlowRoot
            | LayoutDisplay::Flex
            | LayoutDisplay::Grid
            | LayoutDisplay::Table
            | LayoutDisplay::TableCaption
            | LayoutDisplay::TableRow
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
            | LayoutDisplay::TableCell
            | LayoutDisplay::ListItem
    )
}

// +spec:display-property:23f111 - Inline-level elements: inline, inline-block, inline-table, inline-flex, inline-grid
/// Checks if a node is inline-level (including text nodes).
/// According to CSS spec, inline-level content includes:
///
/// - Elements with display: inline, inline-block, inline-table, inline-flex, inline-grid
/// - Text nodes
/// - Generated content
fn is_inline_level(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    // Text nodes are always inline-level
    let node_data = &styled_dom.node_data.as_container()[node_id];
    if matches!(node_data.get_node_type(), NodeType::Text(_)) {
        return true;
    }

    // Check the display property
    matches!(
        get_display_type(styled_dom, node_id),
        LayoutDisplay::Inline
            | LayoutDisplay::InlineBlock
            | LayoutDisplay::InlineTable
            | LayoutDisplay::InlineFlex
            | LayoutDisplay::InlineGrid
    )
}

// +spec:display-property:c2520b - Block containers with only inline-level children establish IFC; mixed content gets anonymous block wrappers
/// Checks if a block container has only inline-level children.
/// According to CSS 2.2 Section 9.4.2: "An inline formatting context is established
/// by a block container box that contains no block-level boxes."
// +spec:display-property:75d642 - block container with only inline-level content establishes IFC
// +spec:display-property:c188d6 - IFC: all inline content within a containing block flows together as continuous text
pub(crate) fn has_only_inline_children(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let Some(node_hier) = hierarchy.get(node_id) else {
        return false;
    };

    // Get the first child
    let mut current_child = node_hier.first_child_id(node_id);

    // If there are no children, it's not an IFC (it's empty)
    if current_child.is_none() {
        return false;
    }

    // Check all children
    while let Some(child_id) = current_child {
        let is_inline = is_inline_level(styled_dom, child_id);

        if !is_inline {
            // Found a block-level child
            return false;
        }

        // Move to next sibling
        if let Some(child_hier) = hierarchy.get(child_id) {
            current_child = child_hier.next_sibling_id();
        } else {
            break;
        }
    }

    // All children are inline-level
    true
}

/// Pre-computes all CSS properties needed during layout for a single node.
///
/// This is called once per node during layout tree construction, avoiding
/// repeated style lookups during the actual layout pass (O(n) vs O(n²)).
fn compute_layout_style(styled_dom: &StyledDom, dom_id: NodeId) -> ComputedLayoutStyle {
    let styled_node_state = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| n.styled_node_state)
        .unwrap_or_default();

    // Get display property
    let display = match get_display_property(styled_dom, Some(dom_id)) {
        MultiValue::Exact(d) => d,
        MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => LayoutDisplay::Block,
    };

    // Get position property
    let position = get_position(styled_dom, dom_id, &styled_node_state).unwrap_or_default();

    // Get float property
    let float = get_float(styled_dom, dom_id, &styled_node_state).unwrap_or_default();

    // Get overflow properties
    // +spec:overflow:48890c - overflow:hidden treated as overflow:clip on replaced elements
    let is_replaced = matches!(
        styled_dom.node_data.as_container()[dom_id].get_node_type(),
        NodeType::Image(_) | NodeType::VirtualView
    );
    let overflow_x = {
        let v = get_overflow_x(styled_dom, dom_id, &styled_node_state).unwrap_or_default();
        if is_replaced && v == LayoutOverflow::Hidden {
            LayoutOverflow::Clip
        } else {
            v
        }
    };
    let overflow_y = {
        let v = get_overflow_y(styled_dom, dom_id, &styled_node_state).unwrap_or_default();
        if is_replaced && v == LayoutOverflow::Hidden {
            LayoutOverflow::Clip
        } else {
            v
        }
    };

    // Get writing mode, direction, and text-orientation
    // +spec:writing-modes:2af307 - Propagate used writing-mode from <body> to <html> root
    let writing_mode = {
        let own_wm = get_writing_mode(styled_dom, dom_id, &styled_node_state).unwrap_or_default();
        let nd = &styled_dom.node_data.as_container()[dom_id];
        if matches!(nd.node_type, NodeType::Html) {
            // If root <html>, propagate writing-mode from first <body> child
            styled_dom
                .node_hierarchy
                .as_container()
                .get(dom_id)
                .and_then(|node| node.first_child_id(dom_id))
                .and_then(|child_id| {
                    let child_data = &styled_dom.node_data.as_container()[child_id];
                    if matches!(child_data.node_type, NodeType::Body) {
                        let child_state =
                            &styled_dom.styled_nodes.as_container()[child_id].styled_node_state;
                        Some(
                            get_writing_mode(styled_dom, child_id, child_state).unwrap_or_default(),
                        )
                    } else {
                        None
                    }
                })
                .unwrap_or(own_wm)
        } else {
            own_wm
        }
    };
    let direction = get_direction(styled_dom, dom_id, &styled_node_state).unwrap_or_default();
    let text_orientation =
        get_text_orientation(styled_dom, dom_id, &styled_node_state).unwrap_or_default();

    // Get text-align
    let text_align = get_text_align(styled_dom, dom_id, &styled_node_state).unwrap_or_default();

    // Get explicit width/height (None = auto)
    let width = match get_css_width(styled_dom, dom_id, &styled_node_state) {
        MultiValue::Exact(w) => Some(w),
        _ => None,
    };
    let height = match get_css_height(styled_dom, dom_id, &styled_node_state) {
        MultiValue::Exact(h) => Some(h),
        _ => None,
    };

    // Get min/max constraints
    let min_width = match get_css_min_width(styled_dom, dom_id, &styled_node_state) {
        MultiValue::Exact(v) => Some(v),
        _ => None,
    };
    let min_height = match get_css_min_height(styled_dom, dom_id, &styled_node_state) {
        MultiValue::Exact(v) => Some(v),
        _ => None,
    };
    let max_width = match get_css_max_width(styled_dom, dom_id, &styled_node_state) {
        MultiValue::Exact(v) => Some(v),
        _ => None,
    };
    let max_height = match get_css_max_height(styled_dom, dom_id, &styled_node_state) {
        MultiValue::Exact(v) => Some(v),
        _ => None,
    };

    ComputedLayoutStyle {
        display,
        position,
        float,
        overflow_x,
        overflow_y,
        writing_mode,
        direction,
        text_orientation,
        width,
        height,
        min_width,
        min_height,
        max_width,
        max_height,
        text_align,
    }
}

// hash_node_data() removed — replaced by NodeDataFingerprint::compute()

/// Helper function to get element's computed font-size
pub(crate) fn get_element_font_size(styled_dom: &StyledDom, dom_id: NodeId) -> f32 {
    {
        let _ = (0xC3_000001u32);
    } // 2-arg wrapper entered
    let node_state = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| &n.styled_node_state)
        .copied()
        .unwrap_or_default();
    {
        let _ = (0xC3_000002u32);
    } // after node_state (clone); next = 3-arg call

    crate::solver3::getters::get_element_font_size(styled_dom, dom_id, &node_state)
}

/// Helper function to get parent's computed font-size
fn get_parent_font_size(styled_dom: &StyledDom, dom_id: NodeId) -> f32 {
    styled_dom
        .node_hierarchy
        .as_container()
        .get(dom_id)
        .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
        .map_or(DEFAULT_FONT_SIZE, |parent_id| {
            get_element_font_size(styled_dom, parent_id)
        })
}

/// Helper function to get root element's font-size
pub(crate) fn get_root_font_size(styled_dom: &StyledDom) -> f32 {
    // Root is always NodeId(0) in Azul
    get_element_font_size(styled_dom, NodeId::new(0))
}

/// Create a `ResolutionContext` for a given node
fn create_resolution_context(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    containing_block_size: Option<PhysicalSize>,
    viewport_size: LogicalSize,
) -> ResolutionContext {
    {
        let _ = (0xC1_000001u32);
    } // create_resolution_context entered
    let element_font_size = get_element_font_size(styled_dom, dom_id);
    {
        let _ = (0xC1_000002u32);
    } // after get_element_font_size
    let parent_font_size = get_parent_font_size(styled_dom, dom_id);
    {
        let _ = (0xC1_000003u32);
    } // after get_parent_font_size
    let root_font_size = get_root_font_size(styled_dom);
    {
        let _ = (0xC1_000004u32);
    } // after get_root_font_size

    ResolutionContext {
        vertical_writing_mode: false,
        element_font_size,
        parent_font_size,
        root_font_size,
        // +spec:box-model:ec6466 - percentage margins/padding resolve to 0 when containing block is unknown (intrinsic sizing), breaking cyclic dependencies per css-sizing-3 §5.2.1
        containing_block_size: containing_block_size.unwrap_or(PhysicalSize::new(0.0, 0.0)),
        element_size: None, // Not yet laid out
        viewport_size: PhysicalSize::new(viewport_size.width, viewport_size.height),
    }
}

/// Result of collecting box properties from the styled DOM.
struct CollectedBoxProps {
    unresolved: crate::solver3::geometry::UnresolvedBoxProps,
    resolved: BoxProps,
}

/// Collects box properties from the styled DOM and returns both unresolved and resolved forms.
///
/// The unresolved form stores the raw CSS values for later re-resolution when
/// the containing block size is known. The resolved form is an initial resolution
/// using `viewport_size` for viewport-relative units.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn collect_box_props(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    viewport_size: LogicalSize,
) -> CollectedBoxProps {
    use crate::solver3::geometry::{UnresolvedBoxProps, UnresolvedEdge, UnresolvedMargin};
    #[allow(clippy::wildcard_imports)]
    // widget/render module pulls in the css property/value types it builds with
    use crate::solver3::getters::*;
    use azul_css::props::style::border::BorderStyle;
    // before create_node step A is the diverging call.
    {
        let _ = (0xC0_000001u32);
    } // entered

    let node_data = &styled_dom.node_data.as_container()[dom_id];

    // Get styled node state
    let node_state = styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| &n.styled_node_state)
        .copied()
        .unwrap_or_default();
    {
        let _ = (0xC0_000002u32);
    } // after node_state (clone)

    // Create resolution context for this element
    // Note: containing_block_size is None here because we don't have it yet
    // This is fine for initial resolution - will be re-resolved during layout
    let context = create_resolution_context(styled_dom, dom_id, None, viewport_size);
    {
        let _ = (0xC0_000003u32);
    } // after create_resolution_context

    // Read margin values from styled_dom
    let margin_top_mv = get_css_margin_top(styled_dom, dom_id, &node_state);
    {
        let _ = (0xC0_000004u32);
    } // after get_css_margin_top
    let margin_right_mv = get_css_margin_right(styled_dom, dom_id, &node_state);
    let margin_bottom_mv = get_css_margin_bottom(styled_dom, dom_id, &node_state);
    let margin_left_mv = get_css_margin_left(styled_dom, dom_id, &node_state);

    // Convert MultiValue to UnresolvedMargin
    let to_unresolved_margin = |mv: &MultiValue<PixelValue>| -> UnresolvedMargin {
        match mv {
            MultiValue::Auto => UnresolvedMargin::Auto,
            MultiValue::Exact(pv) => UnresolvedMargin::Length(*pv),
            _ => UnresolvedMargin::Zero,
        }
    };

    // Build unresolved margins
    let unresolved_margin = UnresolvedEdge {
        top: to_unresolved_margin(&margin_top_mv),
        right: to_unresolved_margin(&margin_right_mv),
        bottom: to_unresolved_margin(&margin_bottom_mv),
        left: to_unresolved_margin(&margin_left_mv),
    };
    {
        let _ = (0xC0_000005u32);
    } // after margin block

    // Read padding values
    let padding_top_mv = get_css_padding_top(styled_dom, dom_id, &node_state);
    let padding_right_mv = get_css_padding_right(styled_dom, dom_id, &node_state);
    let padding_bottom_mv = get_css_padding_bottom(styled_dom, dom_id, &node_state);
    let padding_left_mv = get_css_padding_left(styled_dom, dom_id, &node_state);

    // Convert MultiValue to PixelValue (default to 0px)
    let to_pixel_value = |mv: MultiValue<PixelValue>| -> PixelValue {
        match mv {
            MultiValue::Exact(pv) => pv,
            _ => PixelValue::const_px(0),
        }
    };

    // Build unresolved padding
    let unresolved_padding = UnresolvedEdge {
        top: to_pixel_value(padding_top_mv),
        right: to_pixel_value(padding_right_mv),
        bottom: to_pixel_value(padding_bottom_mv),
        left: to_pixel_value(padding_left_mv),
    };
    {
        let _ = (0xC0_000056u32);
    } // after padding getters+values, before get_display_type

    // +spec:table-layout:038f9d - padding does not apply to table-row-group, table-header-group, table-footer-group, table-row, table-column-group, table-column
    // Non-cell internal table elements (rows, row groups, columns, column groups) do not have padding.
    // 0xC0_57<dt> the CALL returned (dt = LayoutDisplay discriminant) and the MATCH below
    // diverges; if it stays 0x56, get_display_type (the enum extraction) itself diverges.
    // M12.7 NOTE: get_display_type RETURNS a valid dt here (captured =2), but the code
    // immediately after diverges — and replacing the `match` below with a branchless
    // bitmask test did NOT help (so it's NOT the multi-way-branch codegen). So the
    // get_display_type CALL corrupts the caller frame / control flow (same class as
    // create_node's return 0→48704), specific to ENUM-returning getters (pixel getters
    // like get_css_margin_* lift fine). Remill-level. The match is kept (original).
    let unresolved_padding = match get_display_type(styled_dom, dom_id) {
        LayoutDisplay::TableRow
        | LayoutDisplay::TableRowGroup
        | LayoutDisplay::TableHeaderGroup
        | LayoutDisplay::TableFooterGroup
        | LayoutDisplay::TableColumn
        | LayoutDisplay::TableColumnGroup => UnresolvedEdge {
            top: PixelValue::const_px(0),
            right: PixelValue::const_px(0),
            bottom: PixelValue::const_px(0),
            left: PixelValue::const_px(0),
        },
        _ => unresolved_padding,
    };
    {
        let _ = (0xC0_000006u32);
    } // after padding block

    // Read border values
    let border_top_mv = get_css_border_top_width(styled_dom, dom_id, &node_state);
    let border_right_mv = get_css_border_right_width(styled_dom, dom_id, &node_state);
    let border_bottom_mv = get_css_border_bottom_width(styled_dom, dom_id, &node_state);
    let border_left_mv = get_css_border_left_width(styled_dom, dom_id, &node_state);

    // +spec:box-model:17c0e0 - computed border-width is 0 if border-style is none or hidden
    // +spec:box-model:5d2b66 - border-style none/hidden means no border
    // CSS 2.2 §8.5.1: "Computed value: absolute length; '0' if the border style is 'none' or 'hidden'"
    let style_zeroes_width = |s: BorderStyle| matches!(s, BorderStyle::None | BorderStyle::Hidden);

    // Read border styles to check if widths should be zeroed.
    // FAST PATH: compact cache returns styles directly for normal state — no
    // cascade walks. Prior code here did 4 cascade walks × 586 nodes.
    let (bs_top, bs_right, bs_bottom, bs_left) = {
        let cache_ptr = &styled_dom.css_property_cache.ptr;
        if node_state.is_normal() {
            cache_ptr.compact_cache.as_ref().map_or_else(
                || {
                    (
                        cache_ptr
                            .get_border_top_style(node_data, &dom_id, &node_state)
                            .and_then(|v| v.get_property())
                            .map_or(BorderStyle::None, |s| s.inner),
                        cache_ptr
                            .get_border_right_style(node_data, &dom_id, &node_state)
                            .and_then(|v| v.get_property())
                            .map_or(BorderStyle::None, |s| s.inner),
                        cache_ptr
                            .get_border_bottom_style(node_data, &dom_id, &node_state)
                            .and_then(|v| v.get_property())
                            .map_or(BorderStyle::None, |s| s.inner),
                        cache_ptr
                            .get_border_left_style(node_data, &dom_id, &node_state)
                            .and_then(|v| v.get_property())
                            .map_or(BorderStyle::None, |s| s.inner),
                    )
                },
                |cc| {
                    let idx = dom_id.index();
                    (
                        cc.get_border_top_style(idx),
                        cc.get_border_right_style(idx),
                        cc.get_border_bottom_style(idx),
                        cc.get_border_left_style(idx),
                    )
                },
            )
        } else {
            (
                cache_ptr
                    .get_border_top_style(node_data, &dom_id, &node_state)
                    .and_then(|v| v.get_property())
                    .map_or(BorderStyle::None, |s| s.inner),
                cache_ptr
                    .get_border_right_style(node_data, &dom_id, &node_state)
                    .and_then(|v| v.get_property())
                    .map_or(BorderStyle::None, |s| s.inner),
                cache_ptr
                    .get_border_bottom_style(node_data, &dom_id, &node_state)
                    .and_then(|v| v.get_property())
                    .map_or(BorderStyle::None, |s| s.inner),
                cache_ptr
                    .get_border_left_style(node_data, &dom_id, &node_state)
                    .and_then(|v| v.get_property())
                    .map_or(BorderStyle::None, |s| s.inner),
            )
        }
    };

    // Build unresolved border, zeroing width when style is none or hidden
    let unresolved_border = UnresolvedEdge {
        top: if style_zeroes_width(bs_top) {
            PixelValue::const_px(0)
        } else {
            to_pixel_value(border_top_mv)
        },
        right: if style_zeroes_width(bs_right) {
            PixelValue::const_px(0)
        } else {
            to_pixel_value(border_right_mv)
        },
        bottom: if style_zeroes_width(bs_bottom) {
            PixelValue::const_px(0)
        } else {
            to_pixel_value(border_bottom_mv)
        },
        left: if style_zeroes_width(bs_left) {
            PixelValue::const_px(0)
        } else {
            to_pixel_value(border_left_mv)
        },
    };
    {
        let _ = (0xC0_000007u32);
    } // after border block (incl is_normal/compact_cache fast-path)

    // +spec:box-model:8538a9 - Internal table elements do not have margins (CSS 2.2 §17.5)
    // "These boxes have content and borders and cells have padding as well.
    //  Internal table elements do not have margins."
    // +spec:box-model:b4923a - Internal table elements do not have margins (CSS 2.2 § 17.5)
    // +spec:box-model:0a9f8e - Internal table elements do not have margins (CSS 2.2 § 17.5)
    let display_type = get_display_type(styled_dom, dom_id);
    let unresolved_margin = match display_type {
        LayoutDisplay::TableRow
        | LayoutDisplay::TableRowGroup
        | LayoutDisplay::TableHeaderGroup
        | LayoutDisplay::TableFooterGroup
        | LayoutDisplay::TableCell
        | LayoutDisplay::TableColumn
        | LayoutDisplay::TableColumnGroup => UnresolvedEdge {
            top: UnresolvedMargin::Zero,
            right: UnresolvedMargin::Zero,
            bottom: UnresolvedMargin::Zero,
            left: UnresolvedMargin::Zero,
        },
        // +spec:box-model:1197a5 - height property does not apply to non-replaced inline elements; vertical margins zeroed
        // +spec:replaced-elements:f07118 - non-replaced elements have rendering dictated by CSS model
        // "These properties apply to all elements, but vertical margins will not have
        //  any effect on non-replaced inline elements."
        LayoutDisplay::Inline => {
            let is_replaced = matches!(
                node_data.get_node_type(),
                NodeType::Image(_) | NodeType::VirtualView
            );
            if is_replaced {
                unresolved_margin
            } else {
                UnresolvedEdge {
                    top: UnresolvedMargin::Zero,
                    bottom: UnresolvedMargin::Zero,
                    ..unresolved_margin
                }
            }
        }
        _ => unresolved_margin,
    };

    // Build the UnresolvedBoxProps
    let unresolved = UnresolvedBoxProps {
        margin: unresolved_margin,
        padding: unresolved_padding,
        border: unresolved_border,
        // Percentage basis axis for margin/padding (writing-modes-4 §7.2).
        vertical_writing_mode: matches!(
            get_writing_mode(styled_dom, dom_id, &node_state),
            MultiValue::Exact(
                azul_css::props::layout::wrapping::LayoutWritingMode::VerticalRl
                    | azul_css::props::layout::wrapping::LayoutWritingMode::VerticalLr
            )
        ),
    };

    // Create initial resolution params (with viewport as containing block for now)
    let params = crate::solver3::geometry::ResolutionParams {
        containing_block: viewport_size,
        viewport_size,
        element_font_size: context.parent_font_size,
        root_font_size: context.root_font_size,
    };

    // Resolve to get initial box_props
    let resolved = unresolved.resolve(&params);

    if let Some(msgs) = debug_messages.as_mut() {
        msgs.push(LayoutDebugMessage::box_props(format!(
            "[BOX] node[{}] {:?} pad=[{:.1} {:.1} {:.1} {:.1}] mar=[{:.1} {:.1} {:.1} {:.1}] bor=[{:.1} {:.1} {:.1} {:.1}]",
            dom_id.index(), node_data.node_type,
            resolved.padding.top, resolved.padding.right, resolved.padding.bottom, resolved.padding.left,
            resolved.margin.top, resolved.margin.right, resolved.margin.bottom, resolved.margin.left,
            resolved.border.top, resolved.border.right, resolved.border.bottom, resolved.border.left,
        )));

        let has_vh = match &unresolved_margin.top {
            UnresolvedMargin::Length(pv) => pv.metric == azul_css::props::basic::SizeMetric::Vh,
            _ => false,
        };
        if has_vh || resolved.margin.top > 0.0 || resolved.margin.left > 0.0 {
            msgs.push(LayoutDebugMessage::box_props(format!(
                "NodeId {:?} ({:?}): unresolved_margin_top={:?}, resolved_margin_top={:.2}, viewport_size={:?}",
                dom_id, node_data.node_type,
                unresolved_margin.top,
                resolved.margin.top,
                viewport_size
            )));
        }

        msgs.push(LayoutDebugMessage::box_props(format!(
            "NodeId {:?} ({:?}): margin_auto: left={}, right={}, top={}, bottom={} | margin_left={:?}",
            dom_id, node_data.node_type,
            resolved.margin_auto.left, resolved.margin_auto.right,
            resolved.margin_auto.top, resolved.margin_auto.bottom,
            unresolved_margin.left
        )));

        if matches!(node_data.node_type, NodeType::Body) {
            msgs.push(LayoutDebugMessage::box_props(format!(
                "Body margin resolved: top={:.2}, right={:.2}, bottom={:.2}, left={:.2}",
                resolved.margin.top,
                resolved.margin.right,
                resolved.margin.bottom,
                resolved.margin.left
            )));
        }
    }

    CollectedBoxProps {
        unresolved,
        resolved,
    }
}

/// CSS 2.2 Section 17.2.1 - Anonymous box generation, Stage 1:
///
/// "Remove all irrelevant boxes. These are boxes that do not contain table-related boxes
/// and do not themselves have 'display' set to a table-related value. In this context,
/// 'irrelevant boxes' means anonymous inline boxes that contain only white space."
///
/// Checks if a DOM node is whitespace-only text (for table anonymous box generation).
/// Returns true if the node is a text node containing only whitespace characters
/// that would be collapsed away by the white-space property.
// according to the 'white-space' property does not generate any anonymous inline boxes (CSS2§9.2.2.1)
#[must_use]
pub fn is_whitespace_only_text(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    let binding = styled_dom.node_data.as_container();
    let node_data = binding.get(node_id);
    if let Some(data) = node_data {
        if let NodeType::Text(text) = data.get_node_type() {
            // Check if the text contains only CSS document white space characters
            // Per CSS Text 3 §4.1: document white space = U+0020, U+0009, segment breaks
            if !text
                .chars()
                .all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C'))
            {
                return false;
            }
            // Per CSS2§9.2.2.1: "White space content that would subsequently be
            // collapsed away according to the 'white-space' property does not
            // generate any anonymous inline boxes."
            // For white-space: pre / pre-wrap / break-spaces, whitespace is preserved
            // and should NOT be treated as collapsible.
            let white_space = styled_dom.styled_nodes.as_container().get(node_id).map_or(
                StyleWhiteSpace::Normal,
                |n| match get_white_space_property(styled_dom, node_id, &n.styled_node_state) {
                    MultiValue::Exact(ws) => ws,
                    _ => StyleWhiteSpace::Normal,
                },
            );
            return match white_space {
                // These values collapse whitespace — whitespace-only text is collapsible
                StyleWhiteSpace::Normal | StyleWhiteSpace::Nowrap | StyleWhiteSpace::PreLine => {
                    true
                }
                // These values preserve whitespace — whitespace-only text is NOT collapsible
                StyleWhiteSpace::Pre | StyleWhiteSpace::PreWrap | StyleWhiteSpace::BreakSpaces => {
                    false
                }
            };
        }
    }

    false
}

/// Anonymous box generation: is this node a whitespace-only text child that
/// must NOT generate a box for this kind of parent?
///
/// - CSS 2.2 section 17.2.1 (tables): whitespace between table structural
///   elements is "irrelevant" and generates nothing.
/// - css-flexbox-1 section 4: "an anonymous flex item that contains only
///   white space is not rendered".
/// - css-grid-1 section 6: same rule for grid items.
///
/// Without the flex/grid arms, every newline between `<div>` children of a
/// grid container became an anonymous grid ITEM, occupying auto-placement
/// cells: grid-minmax-fr-001 rendered its 9 items in a checkerboard with
/// phantom gaps and overflow rows.
fn should_skip_for_table_structure(
    styled_dom: &StyledDom,
    node_id: NodeId,
    parent_display: LayoutDisplay,
) -> bool {
    matches!(
        parent_display,
        LayoutDisplay::Table
            | LayoutDisplay::InlineTable
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
            | LayoutDisplay::TableRow
            | LayoutDisplay::Grid
            | LayoutDisplay::InlineGrid
            | LayoutDisplay::Flex
            | LayoutDisplay::InlineFlex
    ) && is_whitespace_only_text(styled_dom, node_id)
}

/// Returns true if the given display type is a "proper table child" of a table/inline-table box.
/// Per CSS 2.2 §17.2.1, proper table children are: table-row-group, table-header-group,
/// table-footer-group, table-row, table-column-group, table-column, table-caption.
const fn is_proper_table_child(display: LayoutDisplay) -> bool {
    matches!(
        display,
        LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
            | LayoutDisplay::TableRow
            | LayoutDisplay::TableColumnGroup
            | LayoutDisplay::TableColumn
            | LayoutDisplay::TableCaption
    )
}

// Determines the display type of a node based on its tag and CSS properties.
// Delegates to getters::get_display_property which uses the compact cache fast path.
// M12.7 ROOT: get_display_type (and every layout enum getter) mis-lifts to wasm via the
// remill enum-return/decode path — the geometry-chain blocker. FOUR Rust workarounds all
// FAILED to advance (none reached collect_box_props past get_display_type):
//   1. skip the get_css_property! enum compact-cache fast path  → no change
//   2. replace the LayoutDisplay `match` with a branchless bitmask → no change
//   3. #[inline(never)] (wrap the call w/ enforce_sp_preservation) → made it diverge earlier
//   4. bypass MultiValue<LayoutDisplay> by reading cc.get_display() directly → diverges earlier
// So it is NOT the match codegen, NOT the MultiValue wrapper, NOT a frame/SP issue — it is
// the lift of a fn RETURNING a small fieldless enum (LayoutDisplay) corrupting control flow
// (pixel/i16-returning getters lift fine). Needs the remill m12-q-reg-x8-sret fork's
// enum-return handling — not fixable in Rust. (Original kept.)
#[must_use]
pub fn get_display_type(styled_dom: &StyledDom, node_id: NodeId) -> LayoutDisplay {
    use crate::solver3::getters::get_display_property;

    // A `<transient-window>` takes part in its PARENT's layout only while it
    // is docked INLINE (`dock="inline"`, not torn off - see
    // `transient::TransientDocks`). Otherwise it is nothing here: closed, it
    // is nothing at all; open as a popup or torn off, its subtree is laid out
    // as the root of its OWN window, and laying it out here as well would
    // render the content twice. `display: none` is the one choke point every
    // child-collection honours, which is why it is decided here and not in
    // each of them.
    if let Some(NodeType::TransientWindow(cfg)) = styled_dom
        .node_data
        .as_container()
        .get(node_id)
        .map(NodeData::get_node_type)
    {
        if !transient_node_is_inline(node_id, cfg) {
            return LayoutDisplay::None;
        }
    }

    get_display_property(styled_dom, Some(node_id)).unwrap_or(LayoutDisplay::Inline)
}

// ---------------------------------------------------------------------------
// Inline-docked transient windows: the per-pass dock set
// ---------------------------------------------------------------------------

thread_local! {
    /// The docking state of the layout pass running on this thread (set by
    /// [`with_transient_docks`] for exactly the duration of a solver call).
    /// `None` outside a pass: nodes are then judged by their config alone.
    static TRANSIENT_DOCKS: core::cell::RefCell<Option<crate::transient::TransientDocks>> =
        const { core::cell::RefCell::new(None) };
}

/// Run `f` with `docks` as the current pass's docking state. Every solver
/// entry (`LayoutWindow`'s `layout_document` call) wraps itself in this so
/// the 25 `get_display_type` call sites and the five child-collection sites
/// agree on which `<transient-window>` is inline and where it is grafted.
/// Nests safely (the previous value is restored) and is per thread, so
/// parallel headless layouts do not see each other's docks.
pub fn with_transient_docks<R>(
    docks: crate::transient::TransientDocks,
    f: impl FnOnce() -> R,
) -> R {
    let previous = TRANSIENT_DOCKS.with(|d| d.replace(Some(docks)));
    struct Restore(Option<crate::transient::TransientDocks>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let prev = self.0.take();
            TRANSIENT_DOCKS.with(|d| *d.borrow_mut() = prev);
        }
    }
    let _restore = Restore(previous);
    f()
}

/// Is the `<transient-window>` `node` laid out inline this pass?
fn transient_node_is_inline(
    node: NodeId,
    cfg: &azul_core::transient::TransientWindowConfig,
) -> bool {
    TRANSIENT_DOCKS.with(|d| match d.borrow().as_ref() {
        Some(docks) => docks.is_inline(node, cfg),
        None => cfg.dock == azul_core::transient::TransientDock::Inline && cfg.open && !cfg.torn,
    })
}

/// The children `parent` lays out, in order: its DOM children, minus any
/// inline transient window that was dropped onto another zone, plus the
/// inline transient windows dropped onto `parent` itself (appended last).
/// THE child-collection helper: every site that used to iterate
/// `az_children` directly goes through here, so a grafted panel is a child
/// of its zone for block, flex, grid, table and `display: contents`
/// processing alike.
#[must_use]
pub fn layout_children(styled_dom: &StyledDom, parent: NodeId) -> Vec<NodeId> {
    let hierarchy = styled_dom.node_hierarchy.as_container();
    if hierarchy.get(parent).is_none() {
        return Vec::new(); // an id past the arena: no children, no panic
    }
    TRANSIENT_DOCKS.with(|d| {
        let borrow = d.borrow();
        match borrow.as_ref() {
            Some(docks) if !docks.inline.is_empty() => {
                let mut out: Vec<NodeId> = parent
                    .az_children(&hierarchy)
                    .filter(|c| !docks.moved_away_from(*c, parent))
                    .collect();
                out.extend(docks.grafted_onto(parent, styled_dom));
                out
            }
            _ => parent.az_children(&hierarchy).collect(),
        }
    })
}

// +spec:display-contents:95faa5 - blockification has no effect on none/contents (other => other)
// +spec:display-property:f68848 - Automatic box type transformations: blockification of computed display values
/// Blockify a display type per CSS Display 3 §2.7.
// +spec:display-property:760c5f - blockification sets computed outer display type to block
/// +spec:display-property:d50f70 - blockification affects computed values, determining principal box type only
/// // +spec:inline-block:692e44 - blockification of inline-block per CSS2 compatibility
// +spec:display-property:c3aca2 - inline-block blockifies to block, not flow-root
// +spec:display-property:ee2d65 - blockification of inline-level display types (CSS Display 3 §2.7)
// +spec:display-property:e4a8b7 - layout-internal boxes blockified to flow (block container)
/// CSS Flexbox §3: flex items with table-internal display values
/// (table-cell, table-row, table-row-group, table-header-group, table-footer-group,
/// table-column, table-column-group, table-caption) are blockified to display:block
/// before anonymous table box generation can occur. E.g. two consecutive
/// display:table-cell flex items become two separate display:block flex items.
fn blockify_flex_item_if_table_internal(nodes: &mut [LayoutNode], node_idx: usize) {
    if let Some(node) = nodes.get_mut(node_idx) {
        let is_table_internal = matches!(
            node.formatting_context,
            FormattingContext::TableCell
                | FormattingContext::TableRow
                | FormattingContext::TableRowGroup
                | FormattingContext::TableColumnGroup
                | FormattingContext::TableCaption
                | FormattingContext::Table
        );
        if is_table_internal {
            node.formatting_context = FormattingContext::Block {
                establishes_new_context: true,
            };
        }
    }
}

/// Returns true if the node is a replaced element per CSS Display 3 Appendix B.
/// Replaced elements (img, canvas, embed, object, audio, video, input, textarea,
/// select, br, wbr, meter, progress, virtual views) cannot be un-boxed by
/// `display: contents` and always establish an independent formatting context.
const fn is_replaced_element(node_data: &NodeData) -> bool {
    matches!(
        node_data.get_node_type(),
        NodeType::Image(_)
            | NodeType::VirtualView
            | NodeType::Br
            | NodeType::Wbr
            | NodeType::Meter
            | NodeType::Progress
            | NodeType::Canvas
            | NodeType::Embed
            | NodeType::Object
            | NodeType::Audio
            | NodeType::Video
            | NodeType::Input
            | NodeType::TextArea
            | NodeType::Select
    )
}

// +spec:display-property:285fe7 - block box establishing a BFC (block-level block container with new BFC)
/// **Corrected:** Checks for all conditions that create a new Block Formatting Context.
/// A BFC contains floats and prevents margin collapse.
fn establishes_new_block_formatting_context(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    let display = get_display_type(styled_dom, node_id);
    if matches!(
        display,
        LayoutDisplay::InlineBlock
            | LayoutDisplay::TableCell
            | LayoutDisplay::TableCaption
            | LayoutDisplay::FlowRoot
    ) {
        return true;
    }

    if let Some(styled_node) = styled_dom.styled_nodes.as_container().get(node_id) {
        // Only an EXPLICIT overflow of hidden/scroll/auto establishes a BFC. The
        // initial value is `visible` (no BFC), so an unset overflow — which the
        // slow cascade path returns as the `MultiValue::Auto` "not set" sentinel —
        // must NOT trigger one (`!is_visible_or_clip()` wrongly did, since the
        // sentinel is neither visible nor clip).
        let overflow_x = get_overflow_x(styled_dom, node_id, &styled_node.styled_node_state);
        let overflow_y = get_overflow_y(styled_dom, node_id, &styled_node.styled_node_state);
        if overflow_x.establishes_bfc() || overflow_y.establishes_bfc() {
            return true;
        }

        let position = get_position(styled_dom, node_id, &styled_node.styled_node_state);
        if position.is_absolute_or_fixed() {
            return true;
        }

        let float = get_float(styled_dom, node_id, &styled_node.styled_node_state);
        if !float.is_none() {
            return true;
        }
    }

    // CSS Writing Modes 4 § 3.2: block container with different writing-mode than parent establishes BFC
    if let Some(styled_node) = styled_dom.styled_nodes.as_container().get(node_id) {
        let hierarchy = styled_dom.node_hierarchy.as_container();
        if let Some(parent_dom_id) = hierarchy[node_id].parent_id() {
            let parent_state =
                &styled_dom.styled_nodes.as_container()[parent_dom_id].styled_node_state;
            let child_wm = get_writing_mode(styled_dom, node_id, &styled_node.styled_node_state)
                .unwrap_or_default();
            let parent_wm =
                get_writing_mode(styled_dom, parent_dom_id, parent_state).unwrap_or_default();
            if child_wm != parent_wm {
                return true;
            }
        }
    }

    // +spec:replaced-elements:4f494d - replaced elements always establish an independent formatting context
    let node_data = &styled_dom.node_data.as_container()[node_id];
    if is_replaced_element(node_data) {
        return true;
    }

    // The root element (<html>) also establishes a BFC.
    if styled_dom.root.into_crate_internal() == Some(node_id) {
        return true;
    }

    false
}

// +spec:display-property:0d93f1 - maps display value to box generation (principal box, none, or contents)
/// Like `determine_formatting_context`, but uses an explicit (possibly blockified) display type
/// instead of reading it from the DOM. Used when blockification changes the display.
// +spec:display-property:80f43f - inner display type defines formatting context for non-replaced elements
// +spec:display-property:46e71c - Maps outer display (block/inline) and inner display (flow/flow-root/table/flex/grid) to FormattingContext
// +spec:display-property:aa582d - maps display types to formatting contexts (inline-level, block-level, atomic inline, block container)
#[allow(clippy::match_same_arms)]
// enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
/// A CHILDLESS block that is (or sits inside) a `contenteditable` editing host
/// still establishes an inline formatting context.
///
/// `layout_ifc` keeps ONE line box — the editing-host strut — for an IFC root
/// with no inline content that is editable: the line the caret stands on, the
/// height `display_list::empty_editable_caret_rect` paints into, and the box a
/// click has to land in. A block with NO CHILDREN AT ALL never reached that
/// code, because `has_only_inline_children` answers false for "no children"
/// (an empty box is neither inline nor block content), so the box became a
/// Block FC and collapsed to zero height.
///
/// A brand-new document is exactly one such box: an empty `<p>` inside the
/// editable content root. With no line box there was nothing to click, nothing
/// to paint a caret on, and no way to start typing — you could only get a
/// caret into a document that already had text in it.
///
/// Non-editable empty blocks are untouched: they keep collapsing to nothing.
fn is_empty_editing_host_line(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let has_children = hierarchy
        .get(node_id)
        .and_then(|h| h.first_child_id(node_id))
        .is_some();
    !has_children && crate::solver3::getters::is_node_contenteditable_inherited(styled_dom, node_id)
}

fn determine_formatting_context_for_display(
    styled_dom: &StyledDom,
    node_id: NodeId,
    display_type: LayoutDisplay,
) -> FormattingContext {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    if matches!(node_data.get_node_type(), NodeType::Text(_)) {
        // [g147h az-web-lift DIAG] CONSTANT marker of the COMPUTED FC per DOM node_id (0x60B60+slot),
        // written WITHOUT reading the stored field. 1=text→Inline, 2=block-with-inline→Inline, 4=Block.
        // For the divs (node_id 1,3): 2 ⇒ computed Inline correctly (bug is store/clone/read); 4 ⇒
        // has_only_inline_children mis-lifted to false (computed Block).
        #[cfg(feature = "web_lift")]
        unsafe {
            crate::az_mark(
                (0x60B60 + (node_id.index() & 7) * 4) as u32,
                (0xC0DE0001) as u32,
            );
        }
        return FormattingContext::Inline;
    }
    // +spec:display-property:2a8d62 - block containers with inline-level content establish an IFC
    match display_type {
        // +spec:display-property:37bcf3 - inline outer display type generates an inline box
        // +spec:display-property:30a935 - outer display without inner defaults to flow (block/inline both use flow context)
        LayoutDisplay::Inline => FormattingContext::Inline,
        // +spec:block-formatting-context:97b03b - flow-root always establishes a new BFC; block/list-item may establish one based on other conditions
        // +spec:display-property:0bac26 - list-item limited to flow layout inner types (block/flow-root)
        // +spec:display-property:0beffc - block container with only inline children establishes IFC
        // +spec:display-property:7c49c1 - block container with only inline children establishes an IFC
        // +spec:display-property:90ba2a - flow-root always establishes a new BFC
        LayoutDisplay::FlowRoot => FormattingContext::Block {
            establishes_new_context: true,
        },
        LayoutDisplay::Block | LayoutDisplay::ListItem => {
            if has_only_inline_children(styled_dom, node_id)
                || is_empty_editing_host_line(styled_dom, node_id)
            {
                #[cfg(feature = "web_lift")]
                unsafe {
                    crate::az_mark(
                        (0x60B60 + (node_id.index() & 7) * 4) as u32,
                        (0xC0DE0002) as u32,
                    );
                }
                FormattingContext::Inline
            } else {
                #[cfg(feature = "web_lift")]
                unsafe {
                    crate::az_mark(
                        (0x60B60 + (node_id.index() & 7) * 4) as u32,
                        (0xC0DE0004) as u32,
                    );
                }
                FormattingContext::Block {
                    establishes_new_context: establishes_new_block_formatting_context(
                        styled_dom, node_id,
                    ),
                }
            }
        }
        LayoutDisplay::InlineBlock => FormattingContext::InlineBlock,
        // +spec:display-property:723fe8 - CSS 2.2 §17.2 table model: display types map to formatting contexts, table-column/column-group not rendered, anonymous table objects generated
        // +spec:table-layout:023714 - map display values to table formatting contexts per CSS 2.2 §17.2
        // +spec:table-layout:6c5039 - row-primary table model: rows/cells/captions/columns mapped here
        // +spec:table-layout:75eea9 - display property values for table elements (table, tr, td, etc.)
        // +spec:table-layout:3ee121 - layout-internal display types map to table formatting context
        // +spec:display-property:b02b7f - table display types map to table formatting contexts;
        // table-column/table-column-group not rendered (treated as display:none for box generation)
        LayoutDisplay::Table | LayoutDisplay::InlineTable => FormattingContext::Table,
        LayoutDisplay::TableRowGroup
        | LayoutDisplay::TableHeaderGroup
        | LayoutDisplay::TableFooterGroup => FormattingContext::TableRowGroup,
        LayoutDisplay::TableRow => FormattingContext::TableRow,
        LayoutDisplay::TableCell => FormattingContext::TableCell,
        // +spec:display-property:da3fc7 - display:none/contents generate no boxes (no inner/outer display types)
        // +spec:display-property:e370af - display:none generates no boxes or text sequences
        LayoutDisplay::None => FormattingContext::None,
        LayoutDisplay::Flex | LayoutDisplay::InlineFlex => FormattingContext::Flex,
        LayoutDisplay::TableColumnGroup => FormattingContext::TableColumnGroup,
        LayoutDisplay::TableCaption => FormattingContext::TableCaption,
        LayoutDisplay::Grid | LayoutDisplay::InlineGrid => FormattingContext::Grid,
        // table-column elements are used only for column styling, not for generating boxes
        LayoutDisplay::TableColumn => FormattingContext::None,
        // +spec:display-contents:584072 - no special behavior for legend/HTML elements; contents handled normally
        // display:contents - element generates no box, children are promoted to parent
        LayoutDisplay::Contents => FormattingContext::Contents,
        // +spec:display-property:b89b80 - run-in box falls back to block (merging into next block not implemented)
        // +spec:display-property:ccd4e6 - run-in falls back to block; reparenting not implemented
        // These less common display types default to block behavior
        // +spec:display-property:7d77f5 - run-in treated as block (run-in sequencing fixup not yet implemented)
        // +spec:display-property:0c30c4 - run-in boxes fall back to block (run-in reparenting not implemented, matches browser behavior)
        // +spec:display-property:2f5c52 - run-in treated as block (full run-in merging not implemented)
        LayoutDisplay::RunIn | LayoutDisplay::Marker => FormattingContext::Block {
            establishes_new_context: true,
        },
    }
}

/// The logic now correctly identifies all BFC roots.
fn determine_formatting_context(styled_dom: &StyledDom, node_id: NodeId) -> FormattingContext {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    // [g147j az-web-lift DIAG] OUTER determine_ entry (0x60BB0+slot): 1=Text early-exit,
    // 0x10|disc = went through for_display and returned that repr(C,u8) discriminant.
    // Discriminates "never called during the lifted build" (slot stays 0) vs "called but
    // the for_display match mis-routes" (here=0x10|x while the g147h inner markers stay 0)
    // vs "value correct at build, corrupted later" (here says Inline, dispatch reads garbage).
    if matches!(node_data.get_node_type(), NodeType::Text(_)) {
        #[cfg(feature = "web_lift")]
        unsafe {
            crate::az_mark(0x60BB0 + (node_id.index() & 7) as u32 * 4, 0xC0DE0001);
        }
        return FormattingContext::Inline;
    }
    let display_type = get_display_type(styled_dom, node_id);
    let fc = determine_formatting_context_for_display(styled_dom, node_id, display_type);
    #[cfg(feature = "web_lift")]
    unsafe {
        let disc: u8 = core::ptr::read_volatile((&fc) as *const FormattingContext as *const u8);
        crate::az_mark(
            0x60BB0 + (node_id.index() & 7) as u32 * 4,
            0xC0DE0010 | disc as u32,
        );
    }
    fc
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use azul_core::{
        dom::{Dom, IdOrClass},
        resources::{ImageRef, RawImageFormat},
        selection::ContentIndex,
    };

    use super::*;
    use crate::{
        solver3::geometry::{EdgeSizes, PackedBoxProps},
        text3::{
            cache::{
                BidiDirection, BreakType, ClearType, ClusterFlags, GlyphKind, GraphemeClusterId,
                InlineBreak, LayoutFontMetrics, OverflowInfo, Point, PositionedItem, Rect,
                ShapedCluster, ShapedGlyph, ShapedItem, StyleProperties,
            },
            script::Script,
        },
    };

    // ==================================================================
    // Fixtures
    // ==================================================================

    const VIEWPORT: LogicalSize = LogicalSize {
        width: 800.0,
        height: 600.0,
    };

    fn styled(dom: Dom, css_str: &str) -> StyledDom {
        let mut dom = dom;
        let (css, _warnings) = azul_css::parser2::new_from_str(css_str);
        StyledDom::create(&mut dom, css)
    }

    fn div_class(class: &str) -> Dom {
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
    }

    /// Runs the real build pipeline (`process_node` → `build` → STF bitmap) —
    /// i.e. everything `generate_layout_tree` does minus the `LayoutContext`
    /// (which would need a system-font `FontManager`).
    fn build_tree(styled_dom: &StyledDom) -> LayoutTree {
        let mut builder = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs: Option<Vec<LayoutDebugMessage>> = None;
        let root_id = styled_dom
            .root
            .into_crate_internal()
            .unwrap_or(NodeId::ZERO);
        let root_index = builder
            .process_node(styled_dom, root_id, None, &mut msgs)
            .expect("process_node on a well-formed DOM");
        let mut tree = builder.build(root_index);
        tree.subtree_needs_intrinsic = compute_subtree_needs_intrinsic(styled_dom, &tree);
        tree
    }

    /// `body(0) > [ .block(1) > [ text "hello"(2), .inline(3) > text "world"(4) ],
    ///              .mixed(5) > [ text " \n\t"(6), .block2(7), text "tail"(8) ] ]`
    ///
    /// The `.mixed` subtree is the interesting one: a whitespace-only inline run
    /// followed by a block sibling and a real inline run — exactly the CSS 2.1
    /// §9.2.2.1 anonymous-box case.
    fn mixed_dom() -> StyledDom {
        styled(
            Dom::create_body()
                .with_child(
                    div_class("block")
                        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                            "hello",
                        ))
                        .with_child(div_class("inline").with_child(
                            Dom::create_text_do_not_use_without_block_level_wrapper("world"),
                        )),
                )
                .with_child(
                    div_class("mixed")
                        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                            " \n\t",
                        ))
                        .with_child(div_class("block2"))
                        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                            "tail",
                        )),
                ),
            ".block { display: block; } .inline { display: inline; } .mixed { display: block; } \
             .block2 { display: block; }",
        )
    }

    /// Locates a DOM node by its exact text content. Keeps the tests structural
    /// instead of hard-coding `CompactDom` pre-order indices.
    fn text_node(styled_dom: &StyledDom, needle: &str) -> NodeId {
        let container = styled_dom.node_data.as_container();
        for i in 0..styled_dom.node_data.len() {
            let id = NodeId::new(i);
            if let NodeType::Text(text) = container[id].get_node_type() {
                if text.as_str() == needle {
                    return id;
                }
            }
        }
        panic!("no text node with content {needle:?}");
    }

    fn empty_layout() -> Arc<UnifiedLayout> {
        Arc::new(UnifiedLayout {
            items: Vec::new(),
            overflow: OverflowInfo::default(),
        })
    }

    fn layout_of(items: Vec<PositionedItem>) -> Arc<UnifiedLayout> {
        Arc::new(UnifiedLayout {
            items,
            overflow: OverflowInfo::default(),
        })
    }

    fn tab_item(width: f32, height: f32, x: f32, line_index: usize) -> PositionedItem {
        PositionedItem {
            item: ShapedItem::Tab {
                source: ContentIndex {
                    run_index: 0,
                    item_index: 0,
                },
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                },
            },
            position: Point { x, y: 0.0 },
            line_index,
        }
    }

    /// A positioned text cluster with `n_glyphs` degenerate glyphs, all on
    /// one shared default style/font — so same-line clusters MERGE into one
    /// paint run (the report fixtures need runs that actually hold glyphs;
    /// `tab_item` yields none).
    fn glyph_cluster_item(
        text: &str,
        n_glyphs: usize,
        x: f32,
        line_index: usize,
    ) -> PositionedItem {
        let glyphs: Vec<ShapedGlyph> = (0..n_glyphs)
            .map(|i| ShapedGlyph {
                kind: GlyphKind::Character,
                glyph_id: u16::try_from(i + 1).unwrap(),
                cluster_offset: 0,
                advance: 6.0,
                kerning: 0.0,
                offset: Point { x: 0.0, y: 0.0 },
                vertical_advance: 0.0,
                vertical_offset: Point { x: 0.0, y: 0.0 },
                script: Script::Latin,
                font_hash: 0xF0F0,
                font_metrics: LayoutFontMetrics {
                    ascent: 0.0,
                    descent: 0.0,
                    line_gap: 0.0,
                    units_per_em: 0,
                    x_height: None,
                    cap_height: None,
                },
            })
            .collect();
        PositionedItem {
            item: ShapedItem::Cluster(ShapedCluster {
                flags: ClusterFlags::classify(text),
                source_text: Arc::from(text),
                source_byte_len: u16::try_from(text.len()).unwrap(),
                source_cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                source_content_index: ContentIndex {
                    run_index: 0,
                    item_index: 0,
                },
                source_node_id: None,
                glyphs: glyphs.into_iter().collect(),
                advance: 6.0,
                direction: BidiDirection::Ltr,
                style: Arc::new(StyleProperties::default()),
                marker_position_outside: None,
                is_first_fragment: true,
                is_last_fragment: true,
            }),
            position: Point { x, y: 0.0 },
            line_index,
        }
    }

    fn break_item(line_index: usize) -> PositionedItem {
        PositionedItem {
            item: ShapedItem::Break {
                source: ContentIndex {
                    run_index: 0,
                    item_index: 0,
                },
                break_info: InlineBreak {
                    break_type: BreakType::Hard,
                    clear: ClearType::None,
                    content_index: 0,
                },
            },
            position: Point { x: 0.0, y: 0.0 },
            line_index,
        }
    }

    fn hot(parent: Option<usize>) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: PackedBoxProps::default(),
            dom_node_id: None,
            used_size: None,
            formatting_context: FormattingContext::Block {
                establishes_new_context: false,
            },
            parent,
        }
    }

    /// Hand-assembles a `LayoutTree` from raw hot nodes + child lists so the
    /// index/cycle edge cases the builder can never produce are still reachable.
    fn raw_tree(nodes: Vec<LayoutNodeHot>, child_lists: &[Vec<usize>]) -> LayoutTree {
        let n = nodes.len();
        let mut children_arena: Vec<usize> = Vec::new();
        let mut children_offsets: Vec<(u32, u32)> = Vec::with_capacity(n);
        for cl in child_lists {
            let start = u32::try_from(children_arena.len()).unwrap();
            children_arena.extend_from_slice(cl);
            children_offsets.push((start, u32::try_from(cl.len()).unwrap()));
        }
        while children_offsets.len() < n {
            children_offsets.push((0, 0));
        }
        LayoutTree {
            nodes,
            warm: vec![LayoutNodeWarm::default(); n],
            cold: vec![LayoutNodeCold::default(); n],
            root: 0,
            dom_to_layout: BTreeMap::new(),
            children_arena,
            children_offsets,
            subtree_needs_intrinsic: Vec::new(),
        }
    }

    const ALL_DISPLAYS: [LayoutDisplay; 23] = [
        LayoutDisplay::None,
        LayoutDisplay::Block,
        LayoutDisplay::Inline,
        LayoutDisplay::InlineBlock,
        LayoutDisplay::Flex,
        LayoutDisplay::InlineFlex,
        LayoutDisplay::Table,
        LayoutDisplay::InlineTable,
        LayoutDisplay::TableRowGroup,
        LayoutDisplay::TableHeaderGroup,
        LayoutDisplay::TableFooterGroup,
        LayoutDisplay::TableRow,
        LayoutDisplay::TableColumnGroup,
        LayoutDisplay::TableColumn,
        LayoutDisplay::TableCell,
        LayoutDisplay::TableCaption,
        LayoutDisplay::FlowRoot,
        LayoutDisplay::ListItem,
        LayoutDisplay::RunIn,
        LayoutDisplay::Marker,
        LayoutDisplay::Grid,
        LayoutDisplay::InlineGrid,
        LayoutDisplay::Contents,
    ];

    // ==================================================================
    // IfcId — thread-local counter (numeric / overflow)
    // ==================================================================

    #[test]
    fn ifcid_unique_hands_out_a_fresh_id_per_call_after_reset() {
        IfcId::reset_counter();
        assert_eq!(IfcId::unique(), IfcId(0));
        assert_eq!(IfcId::unique(), IfcId(1));
        assert_eq!(IfcId::unique(), IfcId(2));
        IfcId::reset_counter();
        assert_eq!(
            IfcId::unique(),
            IfcId(0),
            "reset_counter must restart the sequence, not continue it"
        );
        IfcId::reset_counter();
    }

    #[test]
    fn ifcid_reset_counter_is_idempotent() {
        IfcId::reset_counter();
        IfcId::reset_counter();
        IfcId::reset_counter();
        assert_eq!(IfcId::unique(), IfcId(0));
        IfcId::reset_counter();
    }

    #[test]
    fn ifcid_unique_wraps_at_u32_max_instead_of_panicking() {
        // `wrapping_add` is deliberate: a layout pass with 2^32 IFCs is not a
        // thing, but a debug-mode overflow panic in the middle of layout is.
        IFC_ID_COUNTER.with(|c| c.set(u32::MAX));
        assert_eq!(IfcId::unique(), IfcId(u32::MAX));
        assert_eq!(
            IfcId::unique(),
            IfcId(0),
            "wraps rather than overflow-panics"
        );
        assert_eq!(IfcId::unique(), IfcId(1));
        IfcId::reset_counter();
    }

    // ==================================================================
    // CachedInlineLayout — constructors + metrics extraction
    // ==================================================================

    #[test]
    fn cached_inline_layout_new_keeps_the_args_it_was_given() {
        let arc = empty_layout();
        let c = CachedInlineLayout::new(Arc::clone(&arc), AvailableSpace::Definite(123.5), true);
        assert!(Arc::ptr_eq(&c.layout, &arc));
        assert_eq!(c.available_width, AvailableSpace::Definite(123.5));
        assert!(c.has_floats);
        assert!(c.constraints.is_none(), "new() carries no constraints");
        assert!(c.line_breaks.is_none(), "new() computes no line breaks");
        assert_eq!(
            c.inline_content_hash, 0,
            "0 = unknown ⇒ never fast-path-reuse"
        );
        assert!(
            c.item_metrics.is_empty(),
            "an empty layout has no item metrics"
        );
    }

    #[test]
    fn cached_inline_layout_new_survives_extreme_widths() {
        for w in [
            AvailableSpace::Definite(0.0),
            AvailableSpace::Definite(-1.0),
            AvailableSpace::Definite(f32::MAX),
            AvailableSpace::Definite(f32::MIN),
            AvailableSpace::Definite(f32::INFINITY),
            AvailableSpace::Definite(f32::NEG_INFINITY),
            AvailableSpace::Definite(f32::NAN),
            AvailableSpace::MinContent,
            AvailableSpace::MaxContent,
        ] {
            let c = CachedInlineLayout::new(empty_layout(), w, false);
            assert!(c.item_metrics.is_empty());
            assert!(c.layout.items.is_empty());
        }
    }

    #[test]
    fn extract_item_metrics_mirrors_every_positioned_item() {
        let layout = layout_of(vec![
            tab_item(12.0, 20.0, 5.0, 3),
            tab_item(0.0, 0.0, 0.0, 0),
        ]);
        let m = CachedInlineLayout::extract_item_metrics(&layout);
        assert_eq!(m.len(), 2, "one metric entry per PositionedItem, in order");

        assert_eq!(m[0].advance_width, 12.0);
        assert_eq!(m[0].x_offset, 5.0);
        assert_eq!(m[0].line_index, 3);
        assert!(m[0].can_break, "a Tab is breakable");
        assert!(
            m[0].source_node_id.is_none(),
            "non-Cluster items expose no source_node_id"
        );
        // Tab metrics are the fallback ascent/descent split of the item height.
        assert!(
            (m[0].line_height_contribution - 20.0).abs() < 1e-3,
            "ascent+descent should reconstruct the height, got {}",
            m[0].line_height_contribution
        );

        assert_eq!(m[1].advance_width, 0.0);
        assert_eq!(m[1].line_index, 0);
    }

    #[test]
    fn extract_item_metrics_marks_break_items_as_unbreakable_and_zero_sized() {
        let layout = layout_of(vec![break_item(7)]);
        let m = CachedInlineLayout::extract_item_metrics(&layout);
        assert_eq!(m.len(), 1);
        assert!(
            !m[0].can_break,
            "ShapedItem::Break is the one non-breakable item"
        );
        assert_eq!(m[0].advance_width, 0.0, "a break has no visual geometry");
        assert_eq!(m[0].line_height_contribution, 0.0);
        assert_eq!(m[0].line_index, 7);
    }

    #[test]
    fn extract_item_metrics_on_an_empty_layout_is_empty_not_a_panic() {
        assert!(CachedInlineLayout::extract_item_metrics(&empty_layout()).is_empty());
    }

    #[test]
    fn extract_item_metrics_does_not_choke_on_non_finite_item_bounds() {
        let layout = layout_of(vec![
            tab_item(
                f32::INFINITY,
                f32::NAN,
                f32::NEG_INFINITY,
                u32::MAX as usize,
            ),
            tab_item(f32::MAX, f32::MAX, f32::MIN, 0),
        ]);
        let m = CachedInlineLayout::extract_item_metrics(&layout);
        assert_eq!(m.len(), 2);
        assert!(m[0].advance_width.is_infinite());
        assert!(
            m[0].line_height_contribution.is_nan(),
            "NaN in, NaN out — but no panic"
        );
        assert_eq!(m[1].advance_width, f32::MAX);
    }

    #[test]
    fn extract_item_metrics_truncates_a_huge_line_index_into_u32() {
        // `line_index` is a usize on PositionedItem but a u32 in the metrics —
        // the cast is `as`, so it wraps rather than panicking.
        let huge = (u32::MAX as usize) + 5;
        let m = CachedInlineLayout::extract_item_metrics(&layout_of(vec![tab_item(
            1.0, 1.0, 0.0, huge,
        )]));
        assert_eq!(
            m[0].line_index, 4,
            "wrapping `as u32` truncation, not a panic"
        );
    }

    #[test]
    fn cached_inline_layout_new_with_constraints_records_constraints_and_line_breaks() {
        let arc = layout_of(vec![tab_item(10.0, 20.0, 0.0, 0)]);
        let c = CachedInlineLayout::new_with_constraints(
            Arc::clone(&arc),
            AvailableSpace::Definite(200.0),
            false,
            UnifiedConstraints::default(),
        );
        assert!(c.constraints.is_some());
        let lb = c
            .line_breaks
            .expect("new_with_constraints computes line breaks");
        assert_eq!(lb.available_width, 200.0);
        assert_eq!(c.item_metrics.len(), 1);
    }

    #[test]
    fn new_with_constraints_treats_indefinite_widths_as_f32_max() {
        for w in [AvailableSpace::MinContent, AvailableSpace::MaxContent] {
            let c = CachedInlineLayout::new_with_constraints(
                empty_layout(),
                w,
                false,
                UnifiedConstraints::default(),
            );
            let lb = c.line_breaks.expect("line breaks");
            assert_eq!(
                lb.available_width,
                f32::MAX,
                "indefinite width collapses to f32::MAX for break extraction"
            );
            assert_eq!(
                c.available_width, w,
                "but the cache key keeps the real variant"
            );
        }
    }

    // ==================================================================
    // CachedInlineLayout — width matching / validity (predicates)
    // ==================================================================

    fn cached(width: AvailableSpace, has_floats: bool) -> CachedInlineLayout {
        CachedInlineLayout::new(empty_layout(), width, has_floats)
    }

    /// d1 retention wiring: mode 0 stores NO dense view; mode 1 (and 2)
    /// store one, and the runs it derives equal the reference's. The
    /// SEMANTIC equality over real shaped content is pinned by
    /// text3_dense_equivalence + the e2e corpus under AZ_DENSE_TEXT=verify;
    /// this pins the CACHE wiring (someone dropping the retention or the
    /// tuple plumbing goes red here).
    #[test]
    fn dense_view_is_retained_exactly_when_the_flag_asks() {
        let l = empty_layout();
        let (runs0, d0) = compute_glyph_runs_with_mode(&l, 0);
        assert!(d0.is_none(), "mode 0 must not retain a dense view");
        let (runs1, d1) = compute_glyph_runs_with_mode(&l, 1);
        assert!(d1.is_some(), "mode 1 must retain the dense view");
        let (runs2, d2) = compute_glyph_runs_with_mode(&l, 2);
        assert!(d2.is_some(), "verify mode must retain the dense view");
        assert_eq!(runs0.len(), runs1.len());
        assert_eq!(runs0.len(), runs2.len());
    }

    #[test]
    fn width_constraint_matches_definite_widths_within_the_epsilon() {
        let c = cached(AvailableSpace::Definite(100.0), false);
        assert!(c.width_constraint_matches(AvailableSpace::Definite(100.0)));
        assert!(
            c.width_constraint_matches(AvailableSpace::Definite(100.09)),
            "sub-0.1px drift must not force a relayout"
        );
        // 100.1 is NOT exactly 0.1 away: the f32 literal is 100.0999984741211, so the
        // real diff is ~0.09999847, which genuinely IS < 0.1. Binary floats, not
        // decimal. (Pick a literal that clears the epsilon after rounding to test the
        // miss branch -- see below.)
        assert!(c.width_constraint_matches(AvailableSpace::Definite(100.1)));
        assert!(
            !c.width_constraint_matches(AvailableSpace::Definite(100.2)),
            "the epsilon is strict (`< 0.1`), so a 0.2 drift must miss"
        );
        assert!(!c.width_constraint_matches(AvailableSpace::Definite(0.0)));
    }

    #[test]
    fn width_constraint_matches_only_pairs_like_with_like() {
        let min = cached(AvailableSpace::MinContent, false);
        let max = cached(AvailableSpace::MaxContent, false);
        let def = cached(AvailableSpace::Definite(50.0), false);

        assert!(min.width_constraint_matches(AvailableSpace::MinContent));
        assert!(max.width_constraint_matches(AvailableSpace::MaxContent));
        assert!(!min.width_constraint_matches(AvailableSpace::MaxContent));
        assert!(!max.width_constraint_matches(AvailableSpace::MinContent));
        assert!(!min.width_constraint_matches(AvailableSpace::Definite(50.0)));
        assert!(!def.width_constraint_matches(AvailableSpace::MinContent));
        assert!(!def.width_constraint_matches(AvailableSpace::MaxContent));
    }

    #[test]
    fn width_constraint_matches_is_false_for_nan_widths_rather_than_panicking() {
        // (NaN - NaN).abs() is NaN, and `NaN < eps` is false — so a NaN-width
        // cache entry never validates. Deterministic (always relayout), not a panic.
        let c = cached(AvailableSpace::Definite(f32::NAN), false);
        assert!(!c.width_constraint_matches(AvailableSpace::Definite(f32::NAN)));
        assert!(!c.width_constraint_matches(AvailableSpace::Definite(0.0)));

        let good = cached(AvailableSpace::Definite(10.0), false);
        assert!(!good.width_constraint_matches(AvailableSpace::Definite(f32::NAN)));
    }

    #[test]
    fn width_constraint_matches_is_false_for_an_infinite_width_against_itself() {
        // inf - inf == NaN, so an infinite cached width never matches — the cache
        // simply always misses. Surprising, but safe and deterministic.
        let c = cached(AvailableSpace::Definite(f32::INFINITY), false);
        assert!(!c.width_constraint_matches(AvailableSpace::Definite(f32::INFINITY)));
        assert!(!c.is_valid_for(AvailableSpace::Definite(f32::INFINITY), false));
        assert!(c.should_replace_with(AvailableSpace::Definite(f32::INFINITY), false));
    }

    #[test]
    fn width_constraint_matches_handles_huge_finite_widths() {
        let c = cached(AvailableSpace::Definite(f32::MAX), false);
        assert!(c.width_constraint_matches(AvailableSpace::Definite(f32::MAX)));
        assert!(!c.width_constraint_matches(AvailableSpace::Definite(f32::MIN)));
    }

    #[test]
    fn is_valid_for_rejects_a_no_float_cache_when_the_request_gains_floats() {
        // A cached layout with no floats must be invalidated once the new request
        // carries floats, so the text re-wraps around them (#19); every other float
        // combination reduces to width_constraint_matches. This mirrors
        // should_replace_with()'s gain-float branch (they must stay consistent).
        let widths = [
            AvailableSpace::Definite(0.0),
            AvailableSpace::Definite(100.0),
            AvailableSpace::MinContent,
            AvailableSpace::MaxContent,
        ];
        for cached_floats in [false, true] {
            for cached_w in widths {
                let c = cached(cached_w, cached_floats);
                for new_w in widths {
                    let width_ok = c.width_constraint_matches(new_w);
                    // No-float request: pure width match (a float-aware cache is
                    // still kept for a no-float request, gated on width).
                    assert_eq!(c.is_valid_for(new_w, false), width_ok);
                    // Float request: a no-float cache is always rejected so the text
                    // re-wraps; a float-aware cache stays gated on width.
                    let expected_with_floats = if cached_floats { width_ok } else { false };
                    assert_eq!(c.is_valid_for(new_w, true), expected_with_floats);
                    // is_valid_for and should_replace_with must stay opposites on the
                    // gain-float axis.
                    if !cached_floats {
                        assert!(c.should_replace_with(new_w, true));
                    }
                }
            }
        }
    }

    #[test]
    fn is_valid_for_returns_the_expected_true_and_false() {
        let c = cached(AvailableSpace::Definite(300.0), false);
        assert!(c.is_valid_for(AvailableSpace::Definite(300.0), false));
        assert!(!c.is_valid_for(AvailableSpace::Definite(299.0), false));
    }

    #[test]
    fn should_replace_with_always_replaces_when_float_info_is_gained() {
        // Even at an identical width: a float-aware layout strictly dominates.
        let c = cached(AvailableSpace::Definite(300.0), false);
        assert!(c.should_replace_with(AvailableSpace::Definite(300.0), true));
        assert!(c.should_replace_with(AvailableSpace::MinContent, true));
    }

    #[test]
    fn should_replace_with_keeps_a_float_aware_layout_at_a_matching_width() {
        let c = cached(AvailableSpace::Definite(300.0), true);
        assert!(
            !c.should_replace_with(AvailableSpace::Definite(300.0), false),
            "a non-float layout must not overwrite a float-aware one at the same width"
        );
        assert!(
            c.should_replace_with(AvailableSpace::Definite(100.0), false),
            "…but a width change still forces a replace"
        );
    }

    #[test]
    fn should_replace_with_is_the_negation_of_is_valid_for_when_floats_are_unchanged() {
        let widths = [
            AvailableSpace::Definite(0.0),
            AvailableSpace::Definite(42.0),
            AvailableSpace::MinContent,
            AvailableSpace::MaxContent,
        ];
        for floats in [false, true] {
            for cached_w in widths {
                let c = cached(cached_w, floats);
                for new_w in widths {
                    assert_eq!(
                        c.should_replace_with(new_w, floats),
                        !c.is_valid_for(new_w, floats),
                        "cached={cached_w:?} new={new_w:?} floats={floats}"
                    );
                }
            }
        }
    }

    // ==================================================================
    // CachedInlineLayout — getters
    // ==================================================================

    #[test]
    fn get_layout_and_clone_layout_hand_back_the_very_same_arc() {
        let arc = layout_of(vec![tab_item(1.0, 2.0, 0.0, 0)]);
        let c = CachedInlineLayout::new(Arc::clone(&arc), AvailableSpace::MaxContent, false);
        assert!(Arc::ptr_eq(c.get_layout(), &arc));

        let cloned = c.clone_layout();
        assert!(
            Arc::ptr_eq(&cloned, &arc),
            "clone_layout must not deep-copy"
        );
        assert_eq!(
            Arc::strong_count(&arc),
            4,
            "the original + the cache's + the clone + the DL payload (d5: \
             the payload is built ONCE at store time and holds the layout \
             Arc so TextLayout damage ptr_eq stays stable; 3 -> 4 on \
             2026-08-11)"
        );
        assert_eq!(c.get_layout().items.len(), 1);
    }

    #[test]
    fn get_layout_works_on_an_empty_extreme_instance() {
        let c = cached(AvailableSpace::Definite(f32::NAN), true);
        assert!(c.get_layout().items.is_empty());
        assert!(c.clone_layout().items.is_empty());
    }

    // ==================================================================
    // LayoutNode::split / LayoutTree::get_full_node — round-trip
    // ==================================================================

    #[test]
    fn get_full_node_then_split_round_trips_through_the_soa_arrays() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        assert!(tree.nodes.len() >= 2);

        for i in 0..tree.nodes.len() {
            let full = tree.get_full_node(i).expect("in-range node");
            let (h, w, c) = full.split();

            let hot = tree.get(LayoutNodeId::new(i)).unwrap();
            assert_eq!(h.dom_node_id, hot.dom_node_id, "node {i}");
            assert_eq!(h.parent, hot.parent, "node {i}");
            assert_eq!(h.used_size, hot.used_size, "node {i}");
            assert_eq!(h.formatting_context, hot.formatting_context, "node {i}");
            // box_props survive a pack → unpack → pack round-trip bit-for-bit.
            assert_eq!(h.box_props.margin, hot.box_props.margin, "node {i}");
            assert_eq!(h.box_props.padding, hot.box_props.padding, "node {i}");
            assert_eq!(h.box_props.border, hot.box_props.border, "node {i}");

            let warm = tree.warm(LayoutNodeId::new(i)).unwrap();
            assert_eq!(w.pseudo_element, warm.pseudo_element, "node {i}");
            assert_eq!(w.baseline, warm.baseline, "node {i}");
            assert_eq!(
                w.computed_style.display, warm.computed_style.display,
                "node {i}"
            );

            let cold = tree.cold(LayoutNodeId::new(i)).unwrap();
            assert_eq!(c.anonymous_type, cold.anonymous_type, "node {i}");
            assert_eq!(c.dirty_flag, cold.dirty_flag, "node {i}");
            assert_eq!(c.subtree_hash, cold.subtree_hash, "node {i}");
            assert_eq!(c.ifc_id, cold.ifc_id, "node {i}");
        }
    }

    #[test]
    fn get_full_node_restores_the_children_from_the_arena() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        for i in 0..tree.nodes.len() {
            let full = tree.get_full_node(i).unwrap();
            assert_eq!(full.children, tree.children(i).to_vec(), "node {i}");
        }
    }

    #[test]
    fn get_full_node_is_none_out_of_range() {
        let tree = build_tree(&mixed_dom());
        assert!(tree.get_full_node(tree.nodes.len()).is_none());
        assert!(tree.get_full_node(usize::MAX).is_none());
    }

    // ==================================================================
    // LayoutTree — index-taking accessors (numeric / min-max / overflow)
    // ==================================================================

    #[test]
    fn tree_accessors_return_none_for_every_out_of_range_index() {
        let mut tree = build_tree(&mixed_dom());
        let n = tree.nodes.len();
        for idx in [n, n + 1, usize::MAX, usize::MAX - 1, usize::MAX / 2] {
            assert!(tree.get(LayoutNodeId::new(idx)).is_none(), "get({idx})");
            assert!(tree.warm(LayoutNodeId::new(idx)).is_none(), "warm({idx})");
            assert!(tree.cold(LayoutNodeId::new(idx)).is_none(), "cold({idx})");
            assert!(
                tree.get_mut(LayoutNodeId::new(idx)).is_none(),
                "get_mut({idx})"
            );
            assert!(
                tree.warm_mut(LayoutNodeId::new(idx)).is_none(),
                "warm_mut({idx})"
            );
            assert!(
                tree.cold_mut(LayoutNodeId::new(idx)).is_none(),
                "cold_mut({idx})"
            );
            assert!(tree.get_inline_layout_for_node(idx).is_none());
        }
    }

    #[test]
    fn tree_accessors_all_resolve_at_index_zero() {
        let mut tree = build_tree(&mixed_dom());
        assert!(tree.get(LayoutNodeId::new(0)).is_some());
        assert!(tree.warm(LayoutNodeId::new(0)).is_some());
        assert!(tree.cold(LayoutNodeId::new(0)).is_some());
        assert!(tree.get_mut(LayoutNodeId::new(0)).is_some());
        assert!(tree.warm_mut(LayoutNodeId::new(0)).is_some());
        assert!(tree.cold_mut(LayoutNodeId::new(0)).is_some());
        assert_eq!(
            tree.get(LayoutNodeId::new(0)).unwrap().parent,
            None,
            "index 0 is the root"
        );
    }

    #[test]
    fn children_of_an_out_of_range_index_is_an_empty_slice() {
        let tree = build_tree(&mixed_dom());
        assert!(tree.children(tree.nodes.len()).is_empty());
        assert!(tree.children(usize::MAX).is_empty());
        assert!(tree.children(usize::MAX - 1).is_empty());
    }

    #[test]
    fn children_arena_slices_agree_with_the_parent_pointers() {
        let tree = build_tree(&mixed_dom());
        let n = tree.nodes.len();
        let mut seen: Vec<usize> = Vec::new();
        for i in 0..n {
            for &child in tree.children(i) {
                assert!(child < n, "child {child} of {i} is out of range");
                assert_eq!(
                    tree.get(LayoutNodeId::new(child)).unwrap().parent,
                    Some(i),
                    "child {child} does not point back at parent {i}"
                );
                seen.push(child);
            }
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(
            seen.len(),
            n - 1,
            "every node but the root is someone's child"
        );
        assert!(!seen.contains(&tree.root), "the root is nobody's child");
    }

    #[test]
    fn children_offsets_stay_inside_the_arena() {
        let tree = build_tree(&mixed_dom());
        assert_eq!(tree.children_offsets.len(), tree.nodes.len());
        let total: usize = tree
            .children_offsets
            .iter()
            .map(|&(_, len)| len as usize)
            .sum();
        assert_eq!(total, tree.children_arena.len());
        for &(start, len) in &tree.children_offsets {
            assert!((start as usize) + (len as usize) <= tree.children_arena.len());
        }
    }

    #[test]
    fn get_content_size_is_default_for_an_out_of_range_index() {
        let tree = build_tree(&mixed_dom());
        assert_eq!(
            tree.get_content_size(LayoutNodeId::new(usize::MAX)),
            LogicalSize::default()
        );
        assert_eq!(
            tree.get_content_size(LayoutNodeId::new(tree.nodes.len())),
            LogicalSize::default()
        );
    }

    #[test]
    fn get_content_size_prefers_the_explicit_overflow_content_size() {
        let mut tree = raw_tree(vec![hot(None)], &[vec![]]);
        tree.nodes[0].used_size = Some(LogicalSize::new(10.0, 10.0));
        tree.warm[0].overflow_content_size = Some(LogicalSize::new(999.0, 888.0));
        assert_eq!(
            tree.get_content_size(LayoutNodeId::new(0)),
            LogicalSize::new(999.0, 888.0)
        );
    }

    #[test]
    fn get_content_size_grows_the_used_size_to_cover_the_inline_items() {
        let mut tree = raw_tree(vec![hot(None)], &[vec![]]);
        tree.nodes[0].used_size = Some(LogicalSize::new(10.0, 10.0));
        tree.warm[0].inline_layout_result = Some(Box::new(CachedInlineLayout::new(
            layout_of(vec![tab_item(30.0, 40.0, 25.0, 0)]),
            AvailableSpace::MaxContent,
            false,
        )));
        // item spans x ∈ [25, 55], y ∈ [0, 40]  →  content must cover 55 × 40.
        let cs = tree.get_content_size(LayoutNodeId::new(0));
        assert_eq!(cs.width, 55.0);
        assert_eq!(cs.height, 40.0);
    }

    #[test]
    fn get_content_size_never_shrinks_below_the_used_size() {
        let mut tree = raw_tree(vec![hot(None)], &[vec![]]);
        tree.nodes[0].used_size = Some(LogicalSize::new(500.0, 500.0));
        tree.warm[0].inline_layout_result = Some(Box::new(CachedInlineLayout::new(
            layout_of(vec![tab_item(1.0, 1.0, 0.0, 0)]),
            AvailableSpace::MaxContent,
            false,
        )));
        assert_eq!(
            tree.get_content_size(LayoutNodeId::new(0)),
            LogicalSize::new(500.0, 500.0)
        );
    }

    #[test]
    fn get_content_size_of_a_node_with_no_used_size_is_zero() {
        let tree = raw_tree(vec![hot(None)], &[vec![]]);
        assert_eq!(
            tree.get_content_size(LayoutNodeId::new(0)),
            LogicalSize::default()
        );
    }

    // ==================================================================
    // LayoutTree — IFC navigation
    // ==================================================================

    #[test]
    fn get_ifc_root_layout_index_returns_the_input_unchanged_when_out_of_range() {
        let tree = build_tree(&mixed_dom());
        // Documented contract: no membership ⇒ identity. That must hold for
        // garbage indices too, and it must not panic.
        assert_eq!(tree.get_ifc_root_layout_index(usize::MAX), usize::MAX);
        assert_eq!(tree.get_ifc_root_layout_index(0), 0);
    }

    #[test]
    fn get_ifc_root_layout_index_follows_membership_only_for_non_ifc_roots() {
        let mut tree = raw_tree(vec![hot(None), hot(Some(0))], &[vec![1], vec![]]);
        tree.warm[0].inline_layout_result = Some(Box::new(CachedInlineLayout::new(
            empty_layout(),
            AvailableSpace::MaxContent,
            false,
        )));
        tree.warm[1].ifc_membership = Some(IfcMembership {
            ifc_id: IfcId(0),
            ifc_root_layout_index: 0,
            run_index: 0,
        });

        assert_eq!(
            tree.get_ifc_root_layout_index(1),
            0,
            "a text node anchors to its IFC root"
        );
        assert_eq!(
            tree.get_ifc_root_layout_index(0),
            0,
            "the IFC root itself is its own anchor"
        );

        // A node that owns an inline_layout_result must NOT be redirected, even
        // if it also carries a (stale) membership.
        tree.warm[0].ifc_membership = Some(IfcMembership {
            ifc_id: IfcId(9),
            ifc_root_layout_index: 1,
            run_index: 0,
        });
        assert_eq!(tree.get_ifc_root_layout_index(0), 0);
    }

    #[test]
    fn get_inline_layout_for_node_walks_membership_then_gives_up_cleanly() {
        let mut tree = raw_tree(vec![hot(None), hot(Some(0)), hot(Some(0))], &[vec![1, 2]]);
        let arc = empty_layout();
        tree.warm[0].inline_layout_result = Some(Box::new(CachedInlineLayout::new(
            Arc::clone(&arc),
            AvailableSpace::MaxContent,
            false,
        )));
        tree.warm[1].ifc_membership = Some(IfcMembership {
            ifc_id: IfcId(0),
            ifc_root_layout_index: 0,
            run_index: 0,
        });
        // Node 2 has neither its own layout nor a membership.
        assert!(Arc::ptr_eq(
            tree.get_inline_layout_for_node(0).unwrap(),
            &arc
        ));
        assert!(Arc::ptr_eq(
            tree.get_inline_layout_for_node(1).unwrap(),
            &arc
        ));
        assert!(tree.get_inline_layout_for_node(2).is_none());
    }

    #[test]
    fn get_inline_layout_for_node_is_none_when_membership_dangles() {
        // A membership pointing at a bogus root index must return None, not panic.
        let mut tree = raw_tree(vec![hot(None)], &[vec![]]);
        tree.warm[0].ifc_membership = Some(IfcMembership {
            ifc_id: IfcId(3),
            ifc_root_layout_index: usize::MAX,
            run_index: 0,
        });
        assert!(tree.get_inline_layout_for_node(0).is_none());

        // …and when the referenced root exists but has no cached layout.
        let mut tree = raw_tree(vec![hot(None), hot(Some(0))], &[vec![1], vec![]]);
        tree.warm[1].ifc_membership = Some(IfcMembership {
            ifc_id: IfcId(3),
            ifc_root_layout_index: 0,
            run_index: 0,
        });
        assert!(tree.get_inline_layout_for_node(1).is_none());
    }

    // ==================================================================
    // LayoutTree — dirty flags
    // ==================================================================

    /// `0 → 1 → 2` chain plus a sibling `3` under `1`.
    fn dirty_tree() -> LayoutTree {
        raw_tree(
            vec![hot(None), hot(Some(0)), hot(Some(1)), hot(Some(1))],
            &[vec![1], vec![2, 3], vec![], vec![]],
        )
    }

    #[test]
    fn mark_dirty_walks_up_to_the_root() {
        let mut tree = dirty_tree();
        tree.mark_dirty(2, DirtyFlag::Layout);
        assert_eq!(
            tree.cold(LayoutNodeId::new(2)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
        assert_eq!(
            tree.cold(LayoutNodeId::new(1)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
        assert_eq!(
            tree.cold(LayoutNodeId::new(0)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
        assert_eq!(
            tree.cold(LayoutNodeId::new(3)).unwrap().dirty_flag,
            DirtyFlag::None,
            "the sibling is untouched"
        );
    }

    #[test]
    fn mark_dirty_with_flag_none_is_a_no_op() {
        let mut tree = dirty_tree();
        tree.mark_dirty(2, DirtyFlag::None);
        assert!(tree.cold.iter().all(|c| c.dirty_flag == DirtyFlag::None));
    }

    #[test]
    fn mark_dirty_never_downgrades_an_existing_flag() {
        let mut tree = dirty_tree();
        tree.mark_dirty(2, DirtyFlag::Layout);
        tree.mark_dirty(2, DirtyFlag::Paint);
        assert_eq!(
            tree.cold(LayoutNodeId::new(2)).unwrap().dirty_flag,
            DirtyFlag::Layout,
            "Layout > Paint — a Paint request must not weaken it"
        );
    }

    #[test]
    fn mark_dirty_upgrades_paint_to_layout_and_keeps_propagating() {
        let mut tree = dirty_tree();
        tree.mark_dirty(2, DirtyFlag::Paint);
        assert_eq!(
            tree.cold(LayoutNodeId::new(0)).unwrap().dirty_flag,
            DirtyFlag::Paint
        );
        tree.mark_dirty(2, DirtyFlag::Layout);
        assert_eq!(
            tree.cold(LayoutNodeId::new(2)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
        assert_eq!(
            tree.cold(LayoutNodeId::new(0)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
    }

    #[test]
    fn mark_dirty_stops_early_when_an_ancestor_is_already_at_least_as_dirty() {
        let mut tree = dirty_tree();
        tree.mark_dirty(3, DirtyFlag::Layout); // marks 3, 1, 0
        tree.mark_dirty(2, DirtyFlag::Layout); // marks 2, then stops at 1
        assert_eq!(
            tree.cold(LayoutNodeId::new(2)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
        assert_eq!(
            tree.cold(LayoutNodeId::new(1)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
    }

    #[test]
    fn mark_dirty_out_of_range_is_a_silent_no_op() {
        let mut tree = dirty_tree();
        tree.mark_dirty(usize::MAX, DirtyFlag::Layout);
        tree.mark_dirty(tree.nodes.len(), DirtyFlag::Layout);
        assert!(tree.cold.iter().all(|c| c.dirty_flag == DirtyFlag::None));
    }

    #[test]
    fn mark_dirty_terminates_on_a_cyclic_parent_chain() {
        // Not reachable through the builder, but the `>= flag` early-out is the
        // only thing standing between a corrupted parent pointer and a hang.
        let mut tree = raw_tree(vec![hot(Some(1)), hot(Some(0))], &[vec![], vec![]]);
        tree.mark_dirty(0, DirtyFlag::Layout);
        assert_eq!(
            tree.cold(LayoutNodeId::new(0)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
        assert_eq!(
            tree.cold(LayoutNodeId::new(1)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
    }

    #[test]
    fn mark_dirty_terminates_when_a_node_is_its_own_parent() {
        let mut tree = raw_tree(vec![hot(Some(0))], &[vec![]]);
        tree.mark_dirty(0, DirtyFlag::Layout);
        assert_eq!(
            tree.cold(LayoutNodeId::new(0)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
    }

    #[test]
    fn mark_subtree_dirty_marks_descendants_but_not_ancestors_or_siblings() {
        let mut tree = dirty_tree();
        tree.mark_subtree_dirty(1, DirtyFlag::Layout);
        assert_eq!(
            tree.cold(LayoutNodeId::new(1)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
        assert_eq!(
            tree.cold(LayoutNodeId::new(2)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
        assert_eq!(
            tree.cold(LayoutNodeId::new(3)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
        assert_eq!(
            tree.cold(LayoutNodeId::new(0)).unwrap().dirty_flag,
            DirtyFlag::None,
            "mark_subtree_dirty walks DOWN only"
        );
    }

    #[test]
    fn mark_subtree_dirty_with_none_or_a_bad_index_is_a_no_op() {
        let mut tree = dirty_tree();
        tree.mark_subtree_dirty(0, DirtyFlag::None);
        tree.mark_subtree_dirty(usize::MAX, DirtyFlag::Layout);
        assert!(tree.cold.iter().all(|c| c.dirty_flag == DirtyFlag::None));
    }

    #[test]
    fn mark_subtree_dirty_does_not_downgrade() {
        let mut tree = dirty_tree();
        tree.mark_subtree_dirty(0, DirtyFlag::Layout);
        tree.mark_subtree_dirty(0, DirtyFlag::Paint);
        assert!(tree.cold.iter().all(|c| c.dirty_flag == DirtyFlag::Layout));
    }

    #[test]
    fn clear_all_dirty_flags_resets_every_node() {
        let mut tree = dirty_tree();
        tree.mark_subtree_dirty(0, DirtyFlag::Layout);
        assert!(tree.cold.iter().any(|c| c.dirty_flag != DirtyFlag::None));
        tree.clear_all_dirty_flags();
        assert!(tree.cold.iter().all(|c| c.dirty_flag == DirtyFlag::None));
    }

    #[test]
    fn clear_all_dirty_flags_on_an_empty_tree_does_not_panic() {
        let mut tree = raw_tree(Vec::new(), &[]);
        tree.clear_all_dirty_flags();
        assert!(tree.cold.is_empty());
    }

    // ==================================================================
    // LayoutTree — memory report (getters / numeric)
    // ==================================================================

    #[test]
    fn memory_report_total_is_the_sum_of_its_parts() {
        let tree = build_tree(&mixed_dom());
        let r = tree.memory_report();
        assert_eq!(r.node_count, tree.nodes.len());
        assert_eq!(
            r.total_bytes(),
            r.hot_bytes
                + r.warm_bytes
                + r.warm_inline_layout_bytes
                + r.warm_taffy_cache_bytes
                + r.cold_bytes
                + r.dom_to_layout_bytes
                + r.children_arena_bytes
                + r.children_offsets_bytes
        );
        assert!(r.hot_bytes >= r.node_count * size_of::<LayoutNodeHot>());
        assert!(r.total_bytes() > 0, "a non-empty tree retains something");
    }

    #[test]
    fn memory_report_of_an_empty_tree_is_all_zero() {
        let tree = raw_tree(Vec::new(), &[]);
        let r = tree.memory_report();
        assert_eq!(r.node_count, 0);
        assert_eq!(r.total_bytes(), 0);
    }

    #[test]
    fn memory_report_total_bytes_default_is_zero() {
        assert_eq!(LayoutTreeMemoryReport::default().total_bytes(), 0);
    }

    #[test]
    fn memory_report_total_bytes_at_the_usize_boundary_does_not_overflow() {
        // Nine byte-fields, each usize::MAX / 9 → usize::MAX - 6 (MAX ≡ 6 mod 9).
        // One notch further and `total_bytes`'s plain `+` chain would
        // overflow-panic in debug.
        let ninth = usize::MAX / 9;
        let r = LayoutTreeMemoryReport {
            node_count: 0,
            hot_bytes: ninth,
            warm_bytes: ninth,
            warm_inline_layout_bytes: ninth,
            glyph_run_bytes: ninth,
            warm_taffy_cache_bytes: ninth,
            cold_bytes: ninth,
            dom_to_layout_bytes: ninth,
            children_arena_bytes: ninth,
            children_offsets_bytes: ninth,
            // Counters, not byte totals — `total_bytes` must not add them.
            shaped_cluster_count: 0,
            shaped_glyph_count: 0,
            shaped_cluster_text_bytes: 0,
            distinct_glyph_style_arcs: 0,
            glyph_run_count: 0,
            glyph_instance_count: 0,
        };
        assert_eq!(r.total_bytes(), ninth * 9);
        assert_eq!(r.total_bytes(), usize::MAX - 6);
    }

    #[test]
    fn memory_report_counts_a_cached_inline_layout() {
        let mut tree = raw_tree(vec![hot(None)], &[vec![]]);
        let bare = tree.memory_report().warm_inline_layout_bytes;
        assert_eq!(bare, 0);

        tree.warm[0].inline_layout_result = Some(Box::new(CachedInlineLayout::new(
            layout_of(vec![tab_item(1.0, 1.0, 0.0, 0)]),
            AvailableSpace::MaxContent,
            false,
        )));
        assert!(
            tree.memory_report().warm_inline_layout_bytes >= size_of::<UnifiedLayout>(),
            "the UnifiedLayout header must at least be counted"
        );
        // The Box allocation itself is retained heap the `warm` line can't
        // see (CachedInlineLayout is boxed off LayoutNodeWarm).
        assert!(
            tree.memory_report().warm_inline_layout_bytes >= size_of::<CachedInlineLayout>(),
            "the boxed CachedInlineLayout struct must be counted"
        );
    }

    #[test]
    fn memory_report_itemizes_the_glyph_run_cache() {
        use crate::text3::glyphs::CompactGlyphRun;

        let mut tree = raw_tree(vec![hot(None), hot(Some(0))], &[vec![1], vec![]]);
        let r0 = tree.memory_report();
        assert_eq!(
            (
                r0.glyph_run_bytes,
                r0.glyph_run_count,
                r0.glyph_instance_count
            ),
            (0, 0, 0)
        );

        // Two same-style same-baseline clusters (2 + 1 glyphs) merge into ONE
        // paint run of 3 instances — the run-merging law, pinned through the
        // report counters.
        tree.warm[0].inline_layout_result = Some(Box::new(CachedInlineLayout::new(
            layout_of(vec![
                glyph_cluster_item("a", 2, 0.0, 0),
                glyph_cluster_item("b", 1, 12.0, 0),
            ]),
            AvailableSpace::MaxContent,
            false,
        )));
        let r1 = tree.memory_report();
        assert_eq!(r1.glyph_run_count, 1, "same style+font+baseline must merge");
        assert_eq!(r1.glyph_instance_count, 3);
        assert!(
            r1.glyph_run_bytes >= size_of::<CompactGlyphRun>() + 3 * size_of::<(u32, f32)>(),
            "bytes must cover the run header and all three compact entries \
             (got {}, header {})",
            r1.glyph_run_bytes,
            size_of::<CompactGlyphRun>(),
        );

        // Arc-dedup law: a second node SHARING the same run list (GlyphSwap
        // reuse clones the Arc) must add ZERO bytes and ZERO counts.
        let shared = tree.warm[0]
            .inline_layout_result
            .as_ref()
            .unwrap()
            .glyph_runs
            .clone();
        let mut second = CachedInlineLayout::new(
            layout_of(vec![glyph_cluster_item("c", 1, 0.0, 0)]),
            AvailableSpace::MaxContent,
            false,
        );
        second.glyph_runs = shared;
        tree.warm[1].inline_layout_result = Some(Box::new(second));
        let r2 = tree.memory_report();
        assert_eq!(
            (
                r2.glyph_run_bytes,
                r2.glyph_run_count,
                r2.glyph_instance_count
            ),
            (
                r1.glyph_run_bytes,
                r1.glyph_run_count,
                r1.glyph_instance_count
            ),
            "a shared Arc must not be double-counted"
        );

        // A DISTINCT run list does count.
        let third = CachedInlineLayout::new(
            layout_of(vec![glyph_cluster_item("d", 1, 0.0, 0)]),
            AvailableSpace::MaxContent,
            false,
        );
        tree.warm[1].inline_layout_result = Some(Box::new(third));
        let r3 = tree.memory_report();
        assert_eq!(r3.glyph_run_count, 2);
        assert_eq!(r3.glyph_instance_count, 4);
        assert!(r3.glyph_run_bytes > r1.glyph_run_bytes);
    }

    #[test]
    fn root_node_returns_the_hot_node_at_the_root_index() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        let root = tree.root_node();
        assert_eq!(root.parent, None);
        assert_eq!(root.dom_node_id, sd.root.into_crate_internal());
    }

    // ==================================================================
    // LayoutTree::resolve_box_props (numeric / NaN-inf)
    // ==================================================================

    #[test]
    fn resolve_box_props_out_of_range_is_a_no_op() {
        let mut tree = build_tree(&mixed_dom());
        tree.resolve_box_props(usize::MAX, VIEWPORT, VIEWPORT, 16.0, 16.0);
        tree.resolve_box_props(tree.nodes.len(), VIEWPORT, VIEWPORT, 16.0, 16.0);
    }

    #[test]
    fn resolve_box_props_keeps_the_stored_props_finite_for_nan_and_inf_inputs() {
        let sd = styled(
            Dom::create_body().with_child(div_class("m")),
            ".m { margin: 50%; padding: 10em; border: 1px solid black; }",
        );
        let mut tree = build_tree(&sd);

        for (cb, vp, efs, rfs) in [
            (
                LogicalSize::new(f32::NAN, f32::NAN),
                LogicalSize::new(f32::NAN, f32::NAN),
                f32::NAN,
                f32::NAN,
            ),
            (
                LogicalSize::new(f32::INFINITY, f32::INFINITY),
                LogicalSize::new(f32::INFINITY, f32::INFINITY),
                f32::INFINITY,
                f32::INFINITY,
            ),
            (
                LogicalSize::new(f32::NEG_INFINITY, 0.0),
                LogicalSize::new(0.0, f32::NEG_INFINITY),
                f32::NEG_INFINITY,
                0.0,
            ),
            (
                LogicalSize::new(f32::MAX, f32::MAX),
                LogicalSize::new(f32::MAX, f32::MAX),
                f32::MAX,
                f32::MAX,
            ),
            (
                LogicalSize::new(0.0, 0.0),
                LogicalSize::new(0.0, 0.0),
                0.0,
                0.0,
            ),
        ] {
            tree.resolve_box_props(1, cb, vp, efs, rfs);
            let bp = tree.get(LayoutNodeId::new(1)).unwrap().box_props.unpack();
            for v in [
                bp.margin.top,
                bp.margin.right,
                bp.margin.bottom,
                bp.margin.left,
                bp.padding.top,
                bp.padding.left,
                bp.border.top,
                bp.border.left,
            ] {
                assert!(
                    v.is_finite(),
                    "the i16×10 packing must launder NaN/inf into a finite value, got {v}"
                );
                assert!(
                    (-3277.0..=3277.0).contains(&v),
                    "packed edges are clamped to ±3276.8px, got {v}"
                );
            }
        }
    }

    #[test]
    fn resolve_box_props_resolves_percentages_against_the_containing_block() {
        let sd = styled(
            Dom::create_body().with_child(div_class("m")),
            ".m { margin-left: 50%; }",
        );
        let mut tree = build_tree(&sd);
        tree.resolve_box_props(1, LogicalSize::new(200.0, 100.0), VIEWPORT, 16.0, 16.0);
        let bp = tree.get(LayoutNodeId::new(1)).unwrap().box_props.unpack();
        assert!(
            (bp.margin.left - 100.0).abs() < 0.2,
            "50% of a 200px containing block ≈ 100px, got {}",
            bp.margin.left
        );
    }

    // ==================================================================
    // Anonymous box generation via the real builder
    // ==================================================================

    #[test]
    fn a_whitespace_only_inline_run_generates_no_anonymous_box() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        let ws = text_node(&sd, " \n\t");
        assert!(
            !tree.dom_to_layout.contains_key(&ws),
            "CSS 2.1 §9.2.2.1: collapsible whitespace generates no box"
        );
        assert!(
            tree.nodes.iter().all(|n| n.dom_node_id != Some(ws)),
            "…and no layout node references it"
        );
    }

    #[test]
    fn a_real_inline_run_next_to_a_block_sibling_gets_exactly_one_anonymous_wrapper() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        let wrappers: Vec<usize> = (0..tree.nodes.len())
            .filter(|&i| {
                tree.cold(LayoutNodeId::new(i)).unwrap().anonymous_type
                    == Some(AnonymousBoxType::InlineWrapper)
            })
            .collect();
        assert_eq!(
            wrappers.len(),
            1,
            "only the trailing `tail` run needs wrapping"
        );

        let w = wrappers[0];
        assert_eq!(
            tree.get(LayoutNodeId::new(w)).unwrap().dom_node_id,
            None,
            "anon boxes have no DOM node"
        );
        assert_eq!(
            tree.cold(LayoutNodeId::new(w)).unwrap().dirty_flag,
            DirtyFlag::Layout
        );
        let tail = text_node(&sd, "tail");
        let kids = tree.children(w);
        assert_eq!(kids.len(), 1);
        assert_eq!(
            tree.get(LayoutNodeId::new(kids[0])).unwrap().dom_node_id,
            Some(tail)
        );
    }

    #[test]
    fn an_all_inline_block_container_gets_no_anonymous_wrapper() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        // `.block` (DOM 1) holds only inline children — the all-inline fast path
        // must hand them straight to the parent, with no wrapper in between.
        let block_idx = (0..tree.nodes.len())
            .find(|&i| tree.get(LayoutNodeId::new(i)).unwrap().dom_node_id == Some(NodeId::new(1)))
            .expect("the .block layout node");
        let kids = tree.children(block_idx);
        assert_eq!(kids.len(), 2, "the text run and the inline div, unwrapped");
        assert!(
            kids.iter().all(|&c| tree
                .cold(LayoutNodeId::new(c))
                .unwrap()
                .anonymous_type
                .is_none()),
            "an all-inline block container needs no anonymous wrapper"
        );
        assert_eq!(
            tree.get(LayoutNodeId::new(block_idx))
                .unwrap()
                .formatting_context,
            FormattingContext::Inline,
            "it establishes an IFC instead"
        );
    }

    #[test]
    fn the_marker_pseudo_element_is_inserted_as_the_first_child_of_a_list_item() {
        let sd = styled(
            Dom::create_body().with_child(div_class("li").with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper("item"),
            )),
            ".li { display: list-item; }",
        );
        let tree = build_tree(&sd);

        let marker = (0..tree.nodes.len())
            .find(|&i| {
                tree.warm(LayoutNodeId::new(i)).unwrap().pseudo_element
                    == Some(PseudoElement::Marker)
            })
            .expect("display:list-item must generate a ::marker");
        let li = tree
            .get(LayoutNodeId::new(marker))
            .unwrap()
            .parent
            .expect("marker has a parent");
        assert_eq!(
            tree.children(li)[0],
            marker,
            "CSS Lists 3 §3.1: ::marker is the FIRST child"
        );
        assert_eq!(
            tree.get(LayoutNodeId::new(marker)).unwrap().dom_node_id,
            tree.get(LayoutNodeId::new(li)).unwrap().dom_node_id,
            "the marker shares the list-item's DOM node for counter/style resolution"
        );
        assert_eq!(
            tree.get(LayoutNodeId::new(marker))
                .unwrap()
                .formatting_context,
            FormattingContext::Inline
        );
        assert!(
            tree.dom_to_layout[&tree
                .get(LayoutNodeId::new(li))
                .unwrap()
                .dom_node_id
                .unwrap()]
                .contains(&LayoutNodeId::new(marker)),
            "the marker is registered in dom_to_layout for counter resolution"
        );
    }

    #[test]
    fn display_none_children_never_reach_the_layout_tree() {
        let sd = styled(
            Dom::create_body()
                .with_child(div_class("gone"))
                .with_child(div_class("here")),
            ".gone { display: none; } .here { display: block; }",
        );
        let tree = build_tree(&sd);
        assert_eq!(
            tree.children(tree.root).len(),
            1,
            "display:none generates no box"
        );
    }

    #[test]
    fn display_contents_promotes_its_children_to_the_grandparent() {
        let sd = styled(
            Dom::create_body().with_child(div_class("c").with_child(div_class("kid"))),
            ".c { display: contents; } .kid { display: block; }",
        );
        let tree = build_tree(&sd);
        // The `.c` box is removed from its parent's child list; `.kid` is hoisted.
        let root_kids = tree.children(tree.root);
        assert!(
            root_kids.iter().any(|&i| tree
                .warm(LayoutNodeId::new(i))
                .unwrap()
                .computed_style
                .display
                == LayoutDisplay::Block),
            "the promoted child must be a direct child of the root"
        );
    }

    #[test]
    fn a_table_with_a_bare_cell_gets_an_anonymous_row() {
        let sd = styled(
            Dom::create_body().with_child(div_class("t").with_child(div_class("cell"))),
            ".t { display: table; } .cell { display: table-cell; }",
        );
        let tree = build_tree(&sd);
        assert!(
            (0..tree.nodes.len()).any(|i| {
                tree.cold(LayoutNodeId::new(i)).unwrap().anonymous_type
                    == Some(AnonymousBoxType::TableRow)
            }),
            "CSS 2.2 §17.2.1 stage 2: a non-proper table child is wrapped in an anonymous row"
        );
    }

    #[test]
    fn whitespace_between_table_rows_is_dropped_not_wrapped() {
        let sd = styled(
            Dom::create_body().with_child(
                div_class("t")
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "   ",
                    ))
                    .with_child(div_class("row")),
            ),
            ".t { display: table; } .row { display: table-row; }",
        );
        let tree = build_tree(&sd);
        let ws = text_node(&sd, "   ");
        assert!(
            !tree.dom_to_layout.contains_key(&ws),
            "stage 1: irrelevant (whitespace) boxes are removed"
        );
        assert!(
            !(0..tree.nodes.len()).any(|i| tree.cold(LayoutNodeId::new(i)).unwrap().anonymous_type
                == Some(AnonymousBoxType::TableRow)),
            "and no anonymous row is generated for it"
        );
    }

    #[test]
    fn table_column_children_are_suppressed_entirely() {
        let sd = styled(
            Dom::create_body().with_child(div_class("col").with_child(div_class("kid"))),
            ".col { display: table-column; } .kid { display: block; }",
        );
        let tree = build_tree(&sd);
        // CSS 2.2 §17.2.1: all children of a table-column are display:none.
        let col = tree.children(tree.root)[0];
        assert!(tree.children(col).is_empty());
    }

    // ==================================================================
    // LayoutTreeBuilder (constructor / numeric / boundary)
    // ==================================================================

    #[test]
    fn builder_new_starts_completely_empty() {
        let b = LayoutTreeBuilder::new(VIEWPORT);
        assert!(b.get(0).is_none());
        assert!(b.get(usize::MAX).is_none());
        assert!(b.nodes.is_empty());
        assert!(b.dom_to_layout.is_empty());
        assert_eq!(b.viewport_size, VIEWPORT);
    }

    #[test]
    fn builder_new_accepts_degenerate_viewports() {
        for vp in [
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(-1.0, -1.0),
            LogicalSize::new(f32::MAX, f32::MAX),
            LogicalSize::new(f32::NAN, f32::INFINITY),
        ] {
            let b = LayoutTreeBuilder::new(vp);
            assert!(b.nodes.is_empty());
        }
    }

    #[test]
    fn builder_get_and_get_mut_are_none_out_of_range() {
        let sd = mixed_dom();
        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs = None;
        let root = b.create_node_from_dom(&sd, NodeId::ZERO, None, &mut msgs);
        assert_eq!(root, 0);
        assert!(b.get(0).is_some());
        assert!(b.get_mut(0).is_some());
        for idx in [1, usize::MAX, usize::MAX - 1] {
            assert!(b.get(idx).is_none(), "get({idx})");
            assert!(b.get_mut(idx).is_none(), "get_mut({idx})");
        }
    }

    #[test]
    fn create_anonymous_node_wires_up_parent_children_and_cold_defaults() {
        let sd = mixed_dom();
        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs = None;
        let root = b.create_node_from_dom(&sd, NodeId::ZERO, None, &mut msgs);
        let root_fc = b.get(root).unwrap().formatting_context;

        let anon = b.create_anonymous_node(
            root,
            AnonymousBoxType::TableCell,
            FormattingContext::TableCell,
        );
        assert_eq!(anon, 1, "anon nodes are appended");

        let n = b.get(anon).unwrap();
        assert_eq!(n.dom_node_id, None, "anonymous ⇒ no DOM node");
        assert_eq!(n.anonymous_type, Some(AnonymousBoxType::TableCell));
        assert_eq!(n.formatting_context, FormattingContext::TableCell);
        assert_eq!(n.parent, Some(root));
        assert_eq!(n.parent_formatting_context, Some(root_fc));
        assert_eq!(n.dirty_flag, DirtyFlag::Layout, "a fresh box needs layout");
        assert!(n.children.is_empty());
        assert_eq!(n.subtree_hash, SubtreeHash(0));
        assert!(n.ifc_id.is_none());
        assert_eq!(b.get(root).unwrap().children, vec![anon]);
        assert!(
            b.dom_to_layout.values().all(|v| !v.contains(&anon)),
            "anon boxes are never registered in dom_to_layout"
        );
    }

    #[test]
    fn create_anonymous_node_appends_in_call_order() {
        let sd = mixed_dom();
        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs = None;
        let root = b.create_node_from_dom(&sd, NodeId::ZERO, None, &mut msgs);
        let a = b.create_anonymous_node(
            root,
            AnonymousBoxType::TableRow,
            FormattingContext::TableRow,
        );
        let c = b.create_anonymous_node(
            root,
            AnonymousBoxType::TableCell,
            FormattingContext::TableCell,
        );
        assert_eq!((a, c), (1, 2));
        assert_eq!(b.get(root).unwrap().children, vec![a, c]);
    }

    #[test]
    fn create_node_from_dom_registers_the_dom_mapping_and_the_parent_link() {
        let sd = mixed_dom();
        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs = None;
        let root = b.create_node_from_dom(&sd, NodeId::ZERO, None, &mut msgs);
        let child = b.create_node_from_dom(&sd, NodeId::new(1), Some(root), &mut msgs);

        assert_eq!(b.get(child).unwrap().dom_node_id, Some(NodeId::new(1)));
        assert_eq!(b.get(child).unwrap().parent, Some(root));
        assert_eq!(b.get(root).unwrap().children, vec![child]);
        assert_eq!(b.dom_to_layout[&NodeId::new(1)], vec![child]);
        assert_eq!(b.get(child).unwrap().dirty_flag, DirtyFlag::Layout);
    }

    #[test]
    fn create_node_from_dom_turns_the_roots_visible_overflow_into_auto() {
        // CSS Overflow 3 §3.3 — only for the root (parent == None).
        let sd = styled(
            Dom::create_body().with_child(div_class("d")),
            ".d { display: block; }",
        );
        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs = None;
        let root = b.create_node_from_dom(&sd, NodeId::ZERO, None, &mut msgs);
        let child = b.create_node_from_dom(&sd, NodeId::new(1), Some(root), &mut msgs);

        let root_style = &b.get(root).unwrap().computed_style;
        assert_ne!(root_style.overflow_x, LayoutOverflow::Visible);
        assert_ne!(root_style.overflow_y, LayoutOverflow::Visible);

        let child_style = &b.get(child).unwrap().computed_style;
        assert_eq!(
            child_style.overflow_x,
            LayoutOverflow::Visible,
            "the rule applies to the viewport only, not to every node"
        );
    }

    #[test]
    fn clone_node_from_old_resets_children_and_dirty_state() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        let old_root = tree.get_full_node(0).unwrap();
        let old_child = tree.get_full_node(1).unwrap();
        assert!(
            !old_root.children.is_empty(),
            "the source root has children"
        );

        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let root = b.clone_node_from_old(&old_root, None, old_root.dom_node_id);
        let child = b.clone_node_from_old(&old_child, Some(root), old_child.dom_node_id);
        assert_eq!((root, child), (0, 1));

        assert!(
            b.get(root).unwrap().children == vec![child],
            "the clone's children come only from later clone calls"
        );
        assert!(b.get(child).unwrap().children.is_empty());
        assert_eq!(b.get(child).unwrap().parent, Some(root));
        assert_eq!(b.get(child).unwrap().dirty_flag, DirtyFlag::None);
        let root_fc = b.get(root).unwrap().formatting_context;
        assert_eq!(
            b.get(child).unwrap().parent_formatting_context,
            Some(root_fc)
        );
    }

    #[test]
    fn clone_node_from_old_skips_dom_registration_for_anonymous_nodes() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        let anon = (0..tree.nodes.len())
            .find(|&i| {
                tree.cold(LayoutNodeId::new(i))
                    .unwrap()
                    .anonymous_type
                    .is_some()
            })
            .expect("mixed_dom generates one anonymous wrapper");
        let old = tree.get_full_node(anon).unwrap();
        assert_eq!(old.dom_node_id, None);

        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let idx = b.clone_node_from_old(&old, None, old.dom_node_id);
        assert_eq!(idx, 0);
        assert!(
            b.dom_to_layout.is_empty(),
            "a node with no dom_node_id must not create a mapping entry"
        );
    }

    /// A clone takes the identity it is given, not the identity it was
    /// copied from.
    ///
    /// Reconciliation falls back to POSITIONAL matching when a new child's
    /// DOM id is absent from the old child list (`cache.rs`, the
    /// `.or_else(|| old_children_indices.get(i))` arm). That pairs an old
    /// node with a new node of a DIFFERENT id, and the clone is taken to
    /// reuse the old node's measurements. Carrying the old id through made
    /// the new tree describe a DOM that no longer exists: after a ribbon
    /// tab switch shrank the tree, `compute_counters` panicked indexing
    /// `node_data[52]` on a 37-node StyledDom, and everything quieter —
    /// style lookups, node rects, damage attribution — read the wrong node.
    ///
    /// NEGATIVE CONTROL: restoring `new_node.dom_node_id` to the old value
    /// (i.e. dropping the `new_node.dom_node_id = new_dom_id` assignment)
    /// makes both assertions below fail — verified.
    #[test]
    fn a_clone_takes_the_new_dom_identity_not_the_one_it_was_copied_from() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        // Any node that HAS a DOM id; its id is the "old" one.
        let old_idx = (0..tree.nodes.len())
            .find(|&i| tree.get_full_node(i).and_then(|n| n.dom_node_id).is_some())
            .expect("mixed_dom has DOM-backed nodes");
        let old = tree.get_full_node(old_idx).unwrap();
        let old_id = old.dom_node_id.unwrap();

        // Reconcile it against a DIFFERENT DOM node, as the positional
        // fallback does.
        let new_id = NodeId::new(old_id.index() + 7);

        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let idx = b.clone_node_from_old(&old, None, Some(new_id));

        assert_eq!(
            b.get(idx).unwrap().dom_node_id,
            Some(new_id),
            "the clone must describe the node it was reconciled AGAINST"
        );
        assert_eq!(
            b.dom_to_layout.get(&new_id).map(Vec::as_slice),
            Some(&[idx][..]),
            "dom_to_layout must index the clone under its NEW id"
        );
        assert!(
            !b.dom_to_layout.contains_key(&old_id),
            "the identity it was copied from must leave no mapping behind - a \
             stale entry here is what hands a later lookup a different node's \
             rect"
        );
    }

    #[test]
    fn build_flattens_children_into_the_arena_losslessly() {
        let sd = mixed_dom();
        let mut builder = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs = None;
        let root_id = sd.root.into_crate_internal().unwrap_or(NodeId::ZERO);
        let root = builder.process_node(&sd, root_id, None, &mut msgs).unwrap();
        let expected: Vec<Vec<usize>> = builder.nodes.iter().map(|n| n.children.clone()).collect();

        let tree = builder.build(root);
        assert_eq!(tree.nodes.len(), expected.len());
        assert_eq!(tree.warm.len(), expected.len());
        assert_eq!(tree.cold.len(), expected.len());
        for (i, want) in expected.iter().enumerate() {
            assert_eq!(tree.children(i), want.as_slice(), "node {i}");
        }
    }

    #[test]
    fn build_on_an_empty_builder_yields_an_empty_tree() {
        let tree = LayoutTreeBuilder::new(VIEWPORT).build(0);
        assert!(tree.nodes.is_empty());
        assert!(tree.children_arena.is_empty());
        assert!(tree.children_offsets.is_empty());
        assert!(tree.subtree_needs_intrinsic.is_empty());
        assert!(tree.get(LayoutNodeId::new(0)).is_none());
        assert!(tree.children(0).is_empty());
        assert_eq!(
            tree.get_content_size(LayoutNodeId::new(0)),
            LogicalSize::default()
        );
        assert_eq!(tree.memory_report().node_count, 0);
    }

    #[test]
    fn build_with_an_out_of_range_root_index_does_not_panic() {
        let sd = mixed_dom();
        let mut builder = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs = None;
        builder
            .process_node(&sd, NodeId::ZERO, None, &mut msgs)
            .unwrap();

        let tree = builder.build(usize::MAX);
        assert_eq!(tree.root, usize::MAX, "build() stores the index verbatim");
        assert!(tree.get(LayoutNodeId::new(tree.root)).is_none());
        assert!(tree.children(tree.root).is_empty());
        assert_eq!(tree.get_ifc_root_layout_index(tree.root), usize::MAX);
    }

    #[test]
    fn blockify_node_display_blockifies_an_inline_flex_item() {
        let sd = styled(
            Dom::create_body().with_child(div_class("f").with_child(div_class("i"))),
            ".f { display: flex; } .i { display: inline; }",
        );
        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs = None;
        let root = b.create_node_from_dom(&sd, NodeId::ZERO, None, &mut msgs);
        let flex = b.create_node_from_dom(&sd, NodeId::new(1), Some(root), &mut msgs);
        let item = b.create_node_from_dom(&sd, NodeId::new(2), Some(flex), &mut msgs);

        assert_eq!(
            b.get(flex).unwrap().formatting_context,
            FormattingContext::Flex
        );
        assert_eq!(
            b.get(item).unwrap().computed_style.display,
            LayoutDisplay::Inline
        );

        b.blockify_node_display(&sd, NodeId::new(2), item, Some(flex));

        assert_eq!(
            b.get(item).unwrap().computed_style.display,
            LayoutDisplay::Block,
            "CSS Display 3 §2.7: a flex item's inline display blockifies"
        );
        assert!(matches!(
            b.get(item).unwrap().formatting_context,
            FormattingContext::Block { .. }
        ));
    }

    #[test]
    fn blockify_node_display_leaves_a_plain_block_child_alone() {
        let sd = styled(
            Dom::create_body().with_child(div_class("p").with_child(div_class("b"))),
            ".p { display: block; } .b { display: block; }",
        );
        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs = None;
        let root = b.create_node_from_dom(&sd, NodeId::ZERO, None, &mut msgs);
        let p = b.create_node_from_dom(&sd, NodeId::new(1), Some(root), &mut msgs);
        let child = b.create_node_from_dom(&sd, NodeId::new(2), Some(p), &mut msgs);
        let before = b.get(child).unwrap().formatting_context;

        b.blockify_node_display(&sd, NodeId::new(2), child, Some(p));
        assert_eq!(
            b.get(child).unwrap().computed_style.display,
            LayoutDisplay::Block
        );
        assert_eq!(b.get(child).unwrap().formatting_context, before);
    }

    #[test]
    fn blockify_node_display_with_a_bogus_node_index_is_a_no_op() {
        let sd = mixed_dom();
        let mut b = LayoutTreeBuilder::new(VIEWPORT);
        let mut msgs = None;
        let root = b.create_node_from_dom(&sd, NodeId::ZERO, None, &mut msgs);
        // A valid DOM id with a garbage layout index must not panic: both the
        // read and the write go through `get`/`get_mut`.
        b.blockify_node_display(&sd, NodeId::ZERO, usize::MAX, None);
        b.blockify_node_display(&sd, NodeId::ZERO, 999, Some(usize::MAX));
        assert_eq!(b.nodes.len(), 1);
        assert!(b.get(root).is_some());
    }

    // ==================================================================
    // blockify_flex_item_if_table_internal (numeric / slice bounds)
    // ==================================================================

    #[test]
    fn blockify_flex_item_rewrites_every_table_internal_context() {
        let tree = build_tree(&mixed_dom());
        let mut nodes = vec![tree.get_full_node(0).unwrap()];

        for fc in [
            FormattingContext::TableCell,
            FormattingContext::TableRow,
            FormattingContext::TableRowGroup,
            FormattingContext::TableColumnGroup,
            FormattingContext::TableCaption,
            FormattingContext::Table,
        ] {
            nodes[0].formatting_context = fc;
            blockify_flex_item_if_table_internal(&mut nodes, 0);
            assert_eq!(
                nodes[0].formatting_context,
                FormattingContext::Block {
                    establishes_new_context: true
                },
                "{fc:?} is table-internal and must blockify"
            );
        }
    }

    #[test]
    fn blockify_flex_item_leaves_non_table_contexts_untouched() {
        let tree = build_tree(&mixed_dom());
        let mut nodes = vec![tree.get_full_node(0).unwrap()];

        for fc in [
            FormattingContext::Inline,
            FormattingContext::InlineBlock,
            FormattingContext::Flex,
            FormattingContext::Grid,
            FormattingContext::None,
            FormattingContext::Contents,
            FormattingContext::Block {
                establishes_new_context: false,
            },
        ] {
            nodes[0].formatting_context = fc;
            blockify_flex_item_if_table_internal(&mut nodes, 0);
            assert_eq!(nodes[0].formatting_context, fc, "{fc:?} must be left alone");
        }
    }

    #[test]
    fn blockify_flex_item_out_of_range_or_empty_is_a_no_op() {
        let tree = build_tree(&mixed_dom());
        let mut nodes = vec![tree.get_full_node(0).unwrap()];
        nodes[0].formatting_context = FormattingContext::TableCell;

        blockify_flex_item_if_table_internal(&mut nodes, 1);
        blockify_flex_item_if_table_internal(&mut nodes, usize::MAX);
        blockify_flex_item_if_table_internal(&mut [], 0);
        blockify_flex_item_if_table_internal(&mut [], usize::MAX);
        assert_eq!(nodes[0].formatting_context, FormattingContext::TableCell);
    }

    #[test]
    fn table_cell_flex_items_do_not_produce_anonymous_table_boxes() {
        // CSS Flexbox §3: two `display:table-cell` flex items become two
        // independent block flex items, NOT one anonymous table row.
        let sd = styled(
            Dom::create_body().with_child(
                div_class("f")
                    .with_child(div_class("c"))
                    .with_child(div_class("c")),
            ),
            ".f { display: flex; } .c { display: table-cell; }",
        );
        let tree = build_tree(&sd);
        assert!(
            (0..tree.nodes.len()).all(|i| tree
                .cold(LayoutNodeId::new(i))
                .unwrap()
                .anonymous_type
                .is_none()),
            "no anonymous table boxes for blockified flex items"
        );
        let flex = tree.children(tree.root)[0];
        for &c in tree.children(flex) {
            assert!(matches!(
                tree.get(LayoutNodeId::new(c)).unwrap().formatting_context,
                FormattingContext::Block { .. }
            ));
        }
    }

    // ==================================================================
    // Shrink-to-fit bitmap
    // ==================================================================

    #[test]
    fn is_shrink_to_fit_context_is_true_for_the_intrinsic_reading_contexts() {
        let sd = mixed_dom();
        for fc in [
            FormattingContext::Flex,
            FormattingContext::Grid,
            FormattingContext::Table,
            FormattingContext::InlineBlock,
        ] {
            assert!(
                is_shrink_to_fit_context(&sd, None, fc),
                "{fc:?} sizes from children's intrinsics"
            );
        }
    }

    #[test]
    fn is_shrink_to_fit_context_is_false_for_a_plain_block_with_no_dom_node() {
        let sd = mixed_dom();
        for fc in [
            FormattingContext::Block {
                establishes_new_context: false,
            },
            FormattingContext::Block {
                establishes_new_context: true,
            },
            FormattingContext::Inline,
            FormattingContext::None,
            FormattingContext::TableRow,
        ] {
            assert!(
                !is_shrink_to_fit_context(&sd, None, fc),
                "{fc:?} with no DOM node cannot be float/abspos ⇒ not STF"
            );
        }
    }

    #[test]
    fn is_shrink_to_fit_context_catches_floats_and_abspos() {
        let sd = styled(
            Dom::create_body()
                .with_child(div_class("fl"))
                .with_child(div_class("ab"))
                .with_child(div_class("fx"))
                .with_child(div_class("plain")),
            ".fl { float: left; } .ab { position: absolute; } .fx { position: fixed; } .plain { \
             display: block; }",
        );
        let block = FormattingContext::Block {
            establishes_new_context: false,
        };
        assert!(
            is_shrink_to_fit_context(&sd, Some(NodeId::new(1)), block),
            "float:left"
        );
        assert!(
            is_shrink_to_fit_context(&sd, Some(NodeId::new(2)), block),
            "position:absolute"
        );
        assert!(
            is_shrink_to_fit_context(&sd, Some(NodeId::new(3)), block),
            "position:fixed"
        );
        assert!(
            !is_shrink_to_fit_context(&sd, Some(NodeId::new(4)), block),
            "an in-flow static block is sized top-down ⇒ not STF"
        );
    }

    #[test]
    fn compute_subtree_needs_intrinsic_is_one_bit_per_node() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        let bits = compute_subtree_needs_intrinsic(&sd, &tree);
        assert_eq!(bits.len(), tree.nodes.len());
        assert_eq!(tree.subtree_needs_intrinsic.len(), tree.nodes.len());
    }

    #[test]
    fn compute_subtree_needs_intrinsic_is_all_false_for_a_pure_block_tree() {
        let sd = mixed_dom();
        let tree = build_tree(&sd);
        assert!(
            compute_subtree_needs_intrinsic(&sd, &tree)
                .iter()
                .all(|b| !b),
            "nothing in mixed_dom() is flex/grid/table/float/abspos"
        );
    }

    #[test]
    fn compute_subtree_needs_intrinsic_propagates_a_deep_flex_up_to_the_root() {
        let sd = styled(
            Dom::create_body()
                .with_child(div_class("a").with_child(div_class("b").with_child(div_class("f")))),
            ".a { display: block; } .b { display: block; } .f { display: flex; }",
        );
        let tree = build_tree(&sd);
        let bits = compute_subtree_needs_intrinsic(&sd, &tree);
        assert!(
            bits[tree.root],
            "out[i] = self || any(children) — must reach the root"
        );
        assert!(
            bits.iter().all(|b| *b),
            "every node on the chain is on the flex path"
        );
    }

    #[test]
    fn compute_subtree_needs_intrinsic_leaves_a_flex_free_sibling_branch_false() {
        let sd = styled(
            Dom::create_body()
                .with_child(div_class("f"))
                .with_child(div_class("plain")),
            ".f { display: flex; } .plain { display: block; }",
        );
        let tree = build_tree(&sd);
        let bits = compute_subtree_needs_intrinsic(&sd, &tree);
        let kids = tree.children(tree.root);
        let flex = kids
            .iter()
            .copied()
            .find(|&i| {
                tree.get(LayoutNodeId::new(i)).unwrap().formatting_context
                    == FormattingContext::Flex
            })
            .expect("the flex child");
        let plain = kids
            .iter()
            .copied()
            .find(|&i| i != flex)
            .expect("the plain child");
        assert!(bits[flex]);
        assert!(
            !bits[plain],
            "a sibling that reads no intrinsics stays false"
        );
        assert!(bits[tree.root], "…but the root still sees the flex branch");
    }

    #[test]
    fn compute_subtree_needs_intrinsic_on_an_empty_tree_is_empty() {
        let sd = mixed_dom();
        let tree = raw_tree(Vec::new(), &[]);
        assert!(compute_subtree_needs_intrinsic(&sd, &tree).is_empty());
    }

    // ==================================================================
    // Level / display predicates
    // ==================================================================

    #[test]
    fn is_block_level_matches_the_block_level_display_values() {
        let sd = styled(
            Dom::create_body()
                .with_child(div_class("b"))
                .with_child(div_class("i")),
            ".b { display: block; } .i { display: inline; }",
        );
        assert!(is_block_level(&sd, NodeId::new(1)));
        assert!(!is_block_level(&sd, NodeId::new(2)));
    }

    #[test]
    fn is_block_level_covers_the_table_and_list_item_families() {
        for (css_display, want) in [
            ("block", true),
            ("flow-root", true),
            ("flex", true),
            ("grid", true),
            ("table", true),
            ("table-row", true),
            ("table-cell", true),
            ("table-caption", true),
            ("list-item", true),
            ("inline", false),
            ("inline-block", false),
            ("inline-flex", false),
            ("inline-grid", false),
            ("inline-table", false),
            ("none", false),
        ] {
            let sd = styled(
                Dom::create_body().with_child(div_class("x")),
                &format!(".x {{ display: {css_display}; }}"),
            );
            assert_eq!(
                is_block_level(&sd, NodeId::new(1)),
                want,
                "display:{css_display}"
            );
        }
    }

    #[test]
    fn is_inline_level_is_always_true_for_text_regardless_of_display() {
        let sd = styled(
            Dom::create_body().with_child(
                div_class("b")
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("t")),
            ),
            ".b { display: block; }",
        );
        let t = text_node(&sd, "t");
        assert!(
            is_inline_level(&sd, t),
            "text nodes are inline-level by definition"
        );
        assert!(
            !is_inline_level(&sd, NodeId::new(1)),
            "the block div is not"
        );
    }

    #[test]
    fn is_inline_level_matches_the_inline_display_family() {
        for (css_display, want) in [
            ("inline", true),
            ("inline-block", true),
            ("inline-table", true),
            ("inline-flex", true),
            ("inline-grid", true),
            ("block", false),
            ("flex", false),
            ("table", false),
            ("list-item", false),
        ] {
            let sd = styled(
                Dom::create_body().with_child(div_class("x")),
                &format!(".x {{ display: {css_display}; }}"),
            );
            assert_eq!(
                is_inline_level(&sd, NodeId::new(1)),
                want,
                "display:{css_display}"
            );
        }
    }

    #[test]
    fn block_and_inline_level_are_mutually_exclusive_for_element_nodes() {
        for css_display in [
            "block",
            "inline",
            "inline-block",
            "flex",
            "inline-flex",
            "grid",
            "table",
            "list-item",
        ] {
            let sd = styled(
                Dom::create_body().with_child(div_class("x")),
                &format!(".x {{ display: {css_display}; }}"),
            );
            let id = NodeId::new(1);
            assert!(
                !(is_block_level(&sd, id) && is_inline_level(&sd, id)),
                "display:{css_display} cannot be both block- and inline-level"
            );
        }
    }

    #[test]
    fn has_only_inline_children_is_false_for_a_childless_node() {
        let sd = styled(
            Dom::create_body().with_child(div_class("e")),
            ".e { display: block; }",
        );
        assert!(
            !has_only_inline_children(&sd, NodeId::new(1)),
            "no children ⇒ no IFC (it's empty, not inline)"
        );
    }

    #[test]
    fn has_only_inline_children_is_true_for_an_all_inline_run() {
        let sd = mixed_dom();
        // `.block` (DOM 1) holds a text node and an inline div.
        assert!(has_only_inline_children(&sd, NodeId::new(1)));
    }

    #[test]
    fn has_only_inline_children_is_false_as_soon_as_one_block_child_appears() {
        let sd = mixed_dom();
        // `.mixed` (DOM 5) holds text + a block div + text.
        assert!(!has_only_inline_children(&sd, NodeId::new(5)));
    }

    #[test]
    fn has_only_inline_children_is_false_for_an_out_of_range_node_id() {
        let sd = mixed_dom();
        let past_end = NodeId::new(sd.node_data.len() + 10);
        assert!(
            !has_only_inline_children(&sd, past_end),
            "the hierarchy lookup is a `.get`, so a bogus id must be false, not a panic"
        );
        assert!(!has_only_inline_children(&sd, NodeId::new(usize::MAX / 2)));
    }

    // ==================================================================
    // is_whitespace_only_text (predicate / unicode / boundary)
    // ==================================================================

    fn ws_dom(text: &str, css: &str) -> StyledDom {
        styled(
            Dom::create_body().with_child(div_class("p").with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper(text),
            )),
            css,
        )
    }

    #[test]
    fn is_whitespace_only_text_recognises_the_css_document_whitespace_set() {
        // CSS Text 3 §4.1: space, tab, CR, LF, FF.
        for text in [" ", "\t", "\n", "\r", "\u{000C}", " \t\r\n\u{000C} "] {
            let sd = ws_dom(text, "");
            let id = NodeId::new(2);
            assert!(
                is_whitespace_only_text(&sd, id),
                "{text:?} is collapsible document whitespace"
            );
        }
    }

    #[test]
    fn is_whitespace_only_text_rejects_unicode_spaces_that_css_does_not_collapse() {
        // NBSP, ideographic space, en/em space, zero-width space, line separator:
        // none of these are in the CSS document-whitespace set.
        for text in [
            "\u{00A0}",
            "\u{3000}",
            "\u{2002}",
            "\u{2003}",
            "\u{200B}",
            "\u{2028}",
            " \u{00A0} ",
        ] {
            let sd = ws_dom(text, "");
            assert!(
                !is_whitespace_only_text(&sd, NodeId::new(2)),
                "{text:?} must NOT be treated as collapsible whitespace"
            );
        }
    }

    #[test]
    fn is_whitespace_only_text_is_false_for_real_text() {
        for text in ["hi", " hi ", "\u{1F600}", "a\nb"] {
            let sd = ws_dom(text, "");
            assert!(!is_whitespace_only_text(&sd, NodeId::new(2)), "{text:?}");
        }
    }

    #[test]
    fn is_whitespace_only_text_treats_the_empty_string_as_whitespace() {
        // `"".chars().all(..)` is vacuously true — an empty text node is
        // collapsible and generates no anonymous inline box.
        let sd = ws_dom("", "");
        assert!(is_whitespace_only_text(&sd, NodeId::new(2)));
    }

    #[test]
    fn is_whitespace_only_text_respects_whitespace_preserving_modes() {
        for (ws, collapses) in [
            ("normal", true),
            ("nowrap", true),
            ("pre-line", true),
            ("pre", false),
            ("pre-wrap", false),
            ("break-spaces", false),
        ] {
            let sd = ws_dom(" \n ", &format!(".p {{ white-space: {ws}; }}"));
            assert_eq!(
                is_whitespace_only_text(&sd, NodeId::new(2)),
                collapses,
                "white-space:{ws} — preserved whitespace still generates a box"
            );
        }
    }

    #[test]
    fn is_whitespace_only_text_is_false_for_non_text_and_bogus_nodes() {
        let sd = mixed_dom();
        assert!(
            !is_whitespace_only_text(&sd, NodeId::new(1)),
            "a div is not text"
        );
        assert!(
            !is_whitespace_only_text(&sd, NodeId::ZERO),
            "the body is not text"
        );
        let past_end = NodeId::new(sd.node_data.len() + 1);
        assert!(
            !is_whitespace_only_text(&sd, past_end),
            "an out-of-range id must return false, not panic"
        );
        assert!(!is_whitespace_only_text(&sd, NodeId::new(usize::MAX / 2)));
    }

    // ==================================================================
    // Table-structure predicates
    // ==================================================================

    #[test]
    fn should_skip_for_table_structure_only_fires_inside_table_parents() {
        let sd = ws_dom(" ", "");
        let ws = NodeId::new(2);
        for parent in [
            LayoutDisplay::Table,
            LayoutDisplay::InlineTable,
            LayoutDisplay::TableRowGroup,
            LayoutDisplay::TableHeaderGroup,
            LayoutDisplay::TableFooterGroup,
            LayoutDisplay::TableRow,
            // css-flexbox-1 section 4 / css-grid-1 section 6: whitespace-only
            // anonymous items are not rendered in flex/grid containers.
            LayoutDisplay::Flex,
            LayoutDisplay::InlineFlex,
            LayoutDisplay::Grid,
            LayoutDisplay::InlineGrid,
        ] {
            assert!(
                should_skip_for_table_structure(&sd, ws, parent),
                "whitespace under {parent:?} is an irrelevant box"
            );
        }
        for parent in [
            LayoutDisplay::Block,
            LayoutDisplay::Inline,
            LayoutDisplay::TableCell,
            LayoutDisplay::TableCaption,
            LayoutDisplay::TableColumn,
        ] {
            assert!(
                !should_skip_for_table_structure(&sd, ws, parent),
                "whitespace under {parent:?} is NOT an irrelevant box"
            );
        }
    }

    #[test]
    fn should_skip_for_table_structure_never_skips_real_content() {
        let sd = ws_dom("cell text", "");
        for parent in ALL_DISPLAYS {
            assert!(
                !should_skip_for_table_structure(&sd, NodeId::new(2), parent),
                "non-whitespace text must never be dropped (parent {parent:?})"
            );
        }
    }

    #[test]
    fn is_proper_table_child_matches_exactly_the_seven_spec_values() {
        let proper = [
            LayoutDisplay::TableRowGroup,
            LayoutDisplay::TableHeaderGroup,
            LayoutDisplay::TableFooterGroup,
            LayoutDisplay::TableRow,
            LayoutDisplay::TableColumnGroup,
            LayoutDisplay::TableColumn,
            LayoutDisplay::TableCaption,
        ];
        for d in ALL_DISPLAYS {
            assert_eq!(
                is_proper_table_child(d),
                proper.contains(&d),
                "CSS 2.2 §17.2.1 proper-table-child set: {d:?}"
            );
        }
        assert!(
            !is_proper_table_child(LayoutDisplay::TableCell),
            "a cell is a proper child of a ROW, not of a table"
        );
    }

    // ==================================================================
    // is_replaced_element
    // ==================================================================

    #[test]
    fn is_replaced_element_covers_the_css_display_3_appendix_b_set() {
        for nt in [
            NodeType::Br,
            NodeType::Wbr,
            NodeType::Meter,
            NodeType::Progress,
            NodeType::Canvas,
            NodeType::Embed,
            NodeType::Object,
            NodeType::Audio,
            NodeType::Video,
            NodeType::Input,
            NodeType::TextArea,
            NodeType::Select,
            NodeType::VirtualView,
        ] {
            let nd = NodeData::create_node(nt.clone());
            assert!(is_replaced_element(&nd), "{nt:?} is a replaced element");
        }

        let img =
            NodeData::create_image(ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new()));
        assert!(
            is_replaced_element(&img),
            "an <img> is the canonical replaced element"
        );
    }

    #[test]
    fn is_replaced_element_is_false_for_ordinary_containers_and_text() {
        for nt in [
            NodeType::Div,
            NodeType::Body,
            NodeType::Html,
            NodeType::P,
            NodeType::Span,
            NodeType::Table,
            NodeType::Button,
            NodeType::Label,
            NodeType::Hr,
        ] {
            let nd = NodeData::create_node(nt.clone());
            assert!(!is_replaced_element(&nd), "{nt:?} is not replaced");
        }
        assert!(!is_replaced_element(
            &NodeData::create_text_do_not_use_without_block_level_wrapper("hello")
        ));
    }

    #[test]
    fn display_contents_on_a_replaced_element_degrades_to_display_none() {
        // CSS Display 3 §2.5: a replaced element cannot be un-boxed.
        let sd = styled(
            Dom::create_body().with_child(
                Dom::create_from_data(NodeData::create_node(NodeType::Br))
                    .with_ids_and_classes(vec![IdOrClass::Class("c".into())].into()),
            ),
            ".c { display: contents; }",
        );
        let tree = build_tree(&sd);
        assert!(
            tree.children(tree.root).is_empty(),
            "the <br> must be dropped from its parent's child list"
        );
        let br = (0..tree.nodes.len())
            .find(|&i| tree.get(LayoutNodeId::new(i)).unwrap().dom_node_id == Some(NodeId::new(1)))
            .expect("the node object still exists, just unparented");
        assert_eq!(
            tree.warm(LayoutNodeId::new(br))
                .unwrap()
                .computed_style
                .display,
            LayoutDisplay::None
        );
        assert_eq!(
            tree.get(LayoutNodeId::new(br)).unwrap().formatting_context,
            FormattingContext::None
        );
    }

    // ==================================================================
    // get_display_type
    // ==================================================================

    #[test]
    fn get_display_type_reads_the_computed_display() {
        for (css_display, want) in [
            ("none", LayoutDisplay::None),
            ("block", LayoutDisplay::Block),
            ("inline", LayoutDisplay::Inline),
            ("inline-block", LayoutDisplay::InlineBlock),
            ("flex", LayoutDisplay::Flex),
            ("grid", LayoutDisplay::Grid),
            ("table", LayoutDisplay::Table),
            ("table-row", LayoutDisplay::TableRow),
            ("table-cell", LayoutDisplay::TableCell),
            ("flow-root", LayoutDisplay::FlowRoot),
            ("list-item", LayoutDisplay::ListItem),
            ("contents", LayoutDisplay::Contents),
        ] {
            let sd = styled(
                Dom::create_body().with_child(div_class("x")),
                &format!(".x {{ display: {css_display}; }}"),
            );
            assert_eq!(
                get_display_type(&sd, NodeId::new(1)),
                want,
                "display:{css_display}"
            );
        }
    }

    #[test]
    fn get_display_type_is_stable_across_repeated_calls() {
        let sd = mixed_dom();
        for i in 0..sd.node_data.len() {
            let id = NodeId::new(i);
            let a = get_display_type(&sd, id);
            let b = get_display_type(&sd, id);
            assert_eq!(a, b, "node {i} must be deterministic");
        }
    }

    // ==================================================================
    // Formatting-context determination
    // ==================================================================

    #[test]
    fn determine_formatting_context_is_inline_for_every_text_node() {
        let sd = mixed_dom();
        for needle in ["hello", "world", "tail", " \n\t"] {
            let id = text_node(&sd, needle);
            assert_eq!(
                determine_formatting_context(&sd, id),
                FormattingContext::Inline,
                "text node {needle:?}"
            );
        }
    }

    #[test]
    fn determine_formatting_context_for_display_ignores_display_on_text_nodes() {
        // The text early-out fires before the display match — a text node is
        // Inline even if you hand it `display: grid`.
        let sd = mixed_dom();
        let t = text_node(&sd, "hello");
        for d in ALL_DISPLAYS {
            assert_eq!(
                determine_formatting_context_for_display(&sd, t, d),
                FormattingContext::Inline,
                "text + display:{d:?}"
            );
        }
    }

    #[test]
    fn determine_formatting_context_for_display_maps_each_display_value() {
        let sd = styled(
            Dom::create_body().with_child(div_class("x")),
            ".x { display: block; }",
        );
        let id = NodeId::new(1);
        for (d, want) in [
            (LayoutDisplay::Inline, FormattingContext::Inline),
            (
                LayoutDisplay::FlowRoot,
                FormattingContext::Block {
                    establishes_new_context: true,
                },
            ),
            (LayoutDisplay::InlineBlock, FormattingContext::InlineBlock),
            (LayoutDisplay::Table, FormattingContext::Table),
            (LayoutDisplay::InlineTable, FormattingContext::Table),
            (
                LayoutDisplay::TableRowGroup,
                FormattingContext::TableRowGroup,
            ),
            (
                LayoutDisplay::TableHeaderGroup,
                FormattingContext::TableRowGroup,
            ),
            (
                LayoutDisplay::TableFooterGroup,
                FormattingContext::TableRowGroup,
            ),
            (LayoutDisplay::TableRow, FormattingContext::TableRow),
            (LayoutDisplay::TableCell, FormattingContext::TableCell),
            (
                LayoutDisplay::TableColumnGroup,
                FormattingContext::TableColumnGroup,
            ),
            (LayoutDisplay::TableCaption, FormattingContext::TableCaption),
            (LayoutDisplay::TableColumn, FormattingContext::None),
            (LayoutDisplay::None, FormattingContext::None),
            (LayoutDisplay::Flex, FormattingContext::Flex),
            (LayoutDisplay::InlineFlex, FormattingContext::Flex),
            (LayoutDisplay::Grid, FormattingContext::Grid),
            (LayoutDisplay::InlineGrid, FormattingContext::Grid),
            (LayoutDisplay::Contents, FormattingContext::Contents),
            (
                LayoutDisplay::RunIn,
                FormattingContext::Block {
                    establishes_new_context: true,
                },
            ),
            (
                LayoutDisplay::Marker,
                FormattingContext::Block {
                    establishes_new_context: true,
                },
            ),
        ] {
            assert_eq!(
                determine_formatting_context_for_display(&sd, id, d),
                want,
                "display:{d:?}"
            );
        }
    }

    #[test]
    fn determine_formatting_context_for_display_never_panics_on_any_display_value() {
        let sd = styled(
            Dom::create_body().with_child(div_class("x")),
            ".x { display: block; }",
        );
        for d in ALL_DISPLAYS {
            let _ = determine_formatting_context_for_display(&sd, NodeId::new(1), d);
            let _ = determine_formatting_context_for_display(&sd, NodeId::ZERO, d);
        }
    }

    #[test]
    fn a_block_with_only_inline_children_establishes_an_ifc() {
        let sd = mixed_dom();
        assert_eq!(
            determine_formatting_context(&sd, NodeId::new(1)),
            FormattingContext::Inline,
            "CSS 2.2 §9.4.2: a block container with no block-level boxes establishes an IFC"
        );
    }

    #[test]
    fn a_block_with_a_block_child_stays_a_bfc() {
        let sd = mixed_dom();
        assert!(matches!(
            determine_formatting_context(&sd, NodeId::new(5)),
            FormattingContext::Block { .. }
        ));
    }

    #[test]
    fn establishes_new_bfc_for_the_unconditional_display_values() {
        for css_display in ["inline-block", "table-cell", "table-caption", "flow-root"] {
            let sd = styled(
                Dom::create_body().with_child(div_class("x")),
                &format!(".x {{ display: {css_display}; }}"),
            );
            assert!(
                establishes_new_block_formatting_context(&sd, NodeId::new(1)),
                "display:{css_display} always establishes a BFC"
            );
        }
    }

    #[test]
    fn establishes_new_bfc_for_non_visible_overflow_floats_and_abspos() {
        for css in [
            ".x { display: block; overflow-x: hidden; }",
            ".x { display: block; overflow-y: scroll; }",
            ".x { display: block; overflow: auto; }",
            ".x { display: block; float: left; }",
            ".x { display: block; float: right; }",
            ".x { display: block; position: absolute; }",
            ".x { display: block; position: fixed; }",
        ] {
            let sd = styled(Dom::create_body().with_child(div_class("x")), css);
            assert!(
                establishes_new_block_formatting_context(&sd, NodeId::new(1)),
                "{css} must establish a BFC"
            );
        }
    }

    #[test]
    fn establishes_new_bfc_is_false_for_a_plain_in_flow_block() {
        let sd = styled(
            Dom::create_body().with_child(div_class("x")),
            ".x { display: block; }",
        );
        assert!(
            !establishes_new_block_formatting_context(&sd, NodeId::new(1)),
            "a static, visible-overflow, unfloated block does not open a BFC"
        );
    }

    #[test]
    fn establishes_new_bfc_for_the_root_and_for_replaced_elements() {
        let sd = styled(
            Dom::create_body()
                .with_child(Dom::create_from_data(NodeData::create_node(NodeType::Br))),
            "",
        );
        assert!(
            establishes_new_block_formatting_context(&sd, NodeId::ZERO),
            "the root element always establishes a BFC"
        );
        assert!(
            establishes_new_block_formatting_context(&sd, NodeId::new(1)),
            "replaced elements always establish an independent formatting context"
        );
    }

    // ==================================================================
    // compute_layout_style
    // ==================================================================

    #[test]
    fn compute_layout_style_captures_every_property_it_advertises() {
        let sd = styled(
            Dom::create_body().with_child(div_class("x")),
            ".x { display: flex; position: absolute; overflow-x: hidden; overflow-y: scroll; \
             width: 50px; height: 60px; min-width: 10px; min-height: 11px; max-width: 99px; \
             max-height: 98px; text-align: center; }",
        );
        let s = compute_layout_style(&sd, NodeId::new(1));
        assert_eq!(s.display, LayoutDisplay::Flex);
        assert_eq!(s.position, LayoutPosition::Absolute);
        assert_eq!(s.overflow_x, LayoutOverflow::Hidden);
        assert_eq!(s.overflow_y, LayoutOverflow::Scroll);
        assert_eq!(s.text_align, StyleTextAlign::Center);
        assert!(s.width.is_some());
        assert!(s.height.is_some());
        assert!(s.min_width.is_some());
        assert!(s.min_height.is_some());
        assert!(s.max_width.is_some());
        assert!(s.max_height.is_some());
    }

    #[test]
    fn compute_layout_style_leaves_auto_sizes_as_none() {
        let sd = styled(
            Dom::create_body().with_child(div_class("x")),
            ".x { display: block; }",
        );
        let s = compute_layout_style(&sd, NodeId::new(1));
        assert!(s.width.is_none(), "auto width must be None, not 0px");
        assert!(s.height.is_none());
        assert!(s.max_width.is_none());
        assert!(s.max_height.is_none());
        assert_eq!(s.float, LayoutFloat::None);
        assert_eq!(s.position, LayoutPosition::Static);
    }

    #[test]
    fn compute_layout_style_reads_float_left_and_right() {
        for (css, want) in [("left", LayoutFloat::Left), ("right", LayoutFloat::Right)] {
            let sd = styled(
                Dom::create_body().with_child(div_class("x")),
                &format!(".x {{ float: {css}; }}"),
            );
            assert_eq!(compute_layout_style(&sd, NodeId::new(1)).float, want);
        }
    }

    #[test]
    fn compute_layout_style_never_panics_on_any_node_of_a_real_dom() {
        let sd = mixed_dom();
        for i in 0..sd.node_data.len() {
            let _ = compute_layout_style(&sd, NodeId::new(i));
        }
    }

    // ==================================================================
    // Font-size helpers
    // ==================================================================

    #[test]
    fn font_size_helpers_fall_back_to_the_default_when_nothing_is_specified() {
        let sd = styled(Dom::create_body().with_child(div_class("x")), "");
        assert_eq!(get_root_font_size(&sd), DEFAULT_FONT_SIZE);
        assert_eq!(
            get_parent_font_size(&sd, NodeId::ZERO),
            DEFAULT_FONT_SIZE,
            "the root has no parent ⇒ documented DEFAULT_FONT_SIZE fallback"
        );
        assert_eq!(
            get_element_font_size(&sd, NodeId::new(1)),
            DEFAULT_FONT_SIZE
        );
    }

    #[test]
    fn get_element_and_parent_font_size_track_the_cascade() {
        let sd = styled(
            Dom::create_body().with_child(div_class("big").with_child(div_class("small"))),
            ".big { font-size: 32px; } .small { font-size: 8px; }",
        );
        assert_eq!(get_element_font_size(&sd, NodeId::new(1)), 32.0);
        assert_eq!(get_element_font_size(&sd, NodeId::new(2)), 8.0);
        assert_eq!(
            get_parent_font_size(&sd, NodeId::new(2)),
            32.0,
            "the parent's size, not the element's own"
        );
    }

    #[test]
    fn get_root_font_size_reads_node_zero() {
        let root = Dom::create_body()
            .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
            .with_child(div_class("x"));
        let sd = styled(root, ".root { font-size: 20px; }");
        assert_eq!(
            get_root_font_size(&sd),
            20.0,
            "get_root_font_size hard-codes NodeId(0)"
        );
        assert_eq!(
            get_root_font_size(&sd),
            get_element_font_size(&sd, NodeId::ZERO)
        );
    }

    #[test]
    fn font_size_helpers_return_a_finite_positive_size_for_every_node() {
        let sd = mixed_dom();
        for i in 0..sd.node_data.len() {
            let id = NodeId::new(i);
            for size in [
                get_element_font_size(&sd, id),
                get_parent_font_size(&sd, id),
            ] {
                assert!(size.is_finite(), "node {i}: {size}");
                assert!(
                    size > 0.0,
                    "node {i}: a zero/negative font-size breaks em math"
                );
            }
        }
    }

    // ==================================================================
    // create_resolution_context (numeric / NaN-inf passthrough)
    // ==================================================================

    #[test]
    fn create_resolution_context_zeroes_an_unknown_containing_block() {
        // css-sizing-3 §5.2.1: % margins/padding resolve against 0 when the
        // containing block isn't known yet (cycle breaking).
        let sd = mixed_dom();
        let ctx = create_resolution_context(&sd, NodeId::new(1), None, VIEWPORT);
        assert_eq!(ctx.containing_block_size.width, 0.0);
        assert_eq!(ctx.containing_block_size.height, 0.0);
        assert!(ctx.element_size.is_none(), "not laid out yet");
        assert_eq!(ctx.viewport_size.width, VIEWPORT.width);
        assert_eq!(ctx.viewport_size.height, VIEWPORT.height);
    }

    #[test]
    fn create_resolution_context_passes_a_known_containing_block_through() {
        let sd = mixed_dom();
        let cb = PhysicalSize::new(321.0, 123.0);
        let ctx = create_resolution_context(&sd, NodeId::new(1), Some(cb), VIEWPORT);
        assert_eq!(ctx.containing_block_size.width, 321.0);
        assert_eq!(ctx.containing_block_size.height, 123.0);
    }

    #[test]
    fn create_resolution_context_survives_a_degenerate_viewport() {
        let sd = mixed_dom();
        for vp in [
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(-100.0, -100.0),
            LogicalSize::new(f32::MAX, f32::MAX),
            LogicalSize::new(f32::INFINITY, f32::NEG_INFINITY),
            LogicalSize::new(f32::NAN, f32::NAN),
        ] {
            let ctx = create_resolution_context(&sd, NodeId::new(1), None, vp);
            // Viewport is passed through verbatim; the font sizes must stay sane.
            assert!(ctx.element_font_size.is_finite());
            assert!(ctx.parent_font_size.is_finite());
            assert!(ctx.root_font_size.is_finite());
        }
    }

    #[test]
    fn create_resolution_context_survives_a_degenerate_containing_block() {
        let sd = mixed_dom();
        for cb in [
            PhysicalSize::new(0.0, 0.0),
            PhysicalSize::new(-1.0, -1.0),
            PhysicalSize::new(f32::NAN, f32::INFINITY),
            PhysicalSize::new(f32::MAX, f32::MIN),
        ] {
            let ctx = create_resolution_context(&sd, NodeId::new(1), Some(cb), VIEWPORT);
            assert!(ctx.root_font_size.is_finite());
        }
    }

    // ==================================================================
    // collect_box_props (numeric / saturation / spec zeroing)
    // ==================================================================

    fn collect_for(css: &str, node: usize, viewport: LogicalSize) -> CollectedBoxProps {
        let sd = styled(Dom::create_body().with_child(div_class("x")), css);
        let mut msgs = None;
        collect_box_props(&sd, NodeId::new(node), &mut msgs, viewport)
    }

    #[test]
    fn collect_box_props_resolves_plain_pixel_edges() {
        let c = collect_for(
            ".x { margin: 10px; padding: 5px; border: 2px solid black; }",
            1,
            VIEWPORT,
        );
        assert_eq!(c.resolved.margin.top, 10.0);
        assert_eq!(c.resolved.margin.left, 10.0);
        assert_eq!(c.resolved.padding.right, 5.0);
        assert_eq!(c.resolved.border.bottom, 2.0);
    }

    #[test]
    fn collect_box_props_zeroes_a_border_whose_style_is_none() {
        // CSS 2.2 §8.5.1: computed border-width is 0 when border-style is none/hidden.
        let c = collect_for(".x { border-width: 9px; border-style: none; }", 1, VIEWPORT);
        assert_eq!(c.resolved.border.top, 0.0);
        assert_eq!(c.resolved.border.left, 0.0);

        let c = collect_for(
            ".x { border-width: 9px; border-style: hidden; }",
            1,
            VIEWPORT,
        );
        assert_eq!(c.resolved.border.right, 0.0);
    }

    #[test]
    fn collect_box_props_strips_margins_and_padding_from_internal_table_boxes() {
        // CSS 2.2 §17.5: internal table elements have no margins; rows/groups/
        // columns additionally have no padding.
        for display in [
            "table-row",
            "table-row-group",
            "table-header-group",
            "table-footer-group",
            "table-column",
            "table-column-group",
        ] {
            let c = collect_for(
                &format!(".x {{ display: {display}; margin: 10px; padding: 7px; }}"),
                1,
                VIEWPORT,
            );
            assert_eq!(c.resolved.margin.top, 0.0, "display:{display} margin");
            assert_eq!(c.resolved.padding.top, 0.0, "display:{display} padding");
        }

        // A cell keeps its padding but loses its margin.
        let c = collect_for(
            ".x { display: table-cell; margin: 10px; padding: 7px; }",
            1,
            VIEWPORT,
        );
        assert_eq!(c.resolved.margin.left, 0.0, "cells have no margins");
        assert_eq!(c.resolved.padding.left, 7.0, "…but they do have padding");
    }

    #[test]
    fn collect_box_props_zeroes_vertical_margins_on_a_non_replaced_inline() {
        let c = collect_for(".x { display: inline; margin: 10px; }", 1, VIEWPORT);
        assert_eq!(c.resolved.margin.top, 0.0);
        assert_eq!(c.resolved.margin.bottom, 0.0);
        assert_eq!(
            c.resolved.margin.left, 10.0,
            "horizontal margins still apply to inline boxes"
        );
        assert_eq!(c.resolved.margin.right, 10.0);
    }

    #[test]
    fn collect_box_props_does_not_clamp_huge_lengths_before_packing() {
        // collect_box_props returns f32; the ±3276.8px saturation happens later,
        // in PackedBoxProps. Assert the split so a regression in either is visible.
        let c = collect_for(".x { margin: 99999px; }", 1, VIEWPORT);
        assert_eq!(c.resolved.margin.top, 99_999.0);

        let packed = PackedBoxProps::pack(&c.resolved);
        assert_eq!(
            packed.margin[0],
            i16::MAX,
            "the packing saturates, it does not wrap"
        );
    }

    #[test]
    fn collect_box_props_survives_a_degenerate_viewport() {
        for vp in [
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(-800.0, -600.0),
            LogicalSize::new(f32::MAX, f32::MAX),
            LogicalSize::new(f32::INFINITY, f32::INFINITY),
            LogicalSize::new(f32::NAN, f32::NAN),
        ] {
            // vh/vw units make the viewport actually load-bearing here.
            let c = collect_for(".x { margin: 10vh; padding: 5vw; }", 1, vp);
            let packed = PackedBoxProps::pack(&c.resolved);
            for v in packed.margin.iter().chain(packed.padding.iter()) {
                assert!(
                    (i16::MIN..=i16::MAX).contains(v),
                    "packing must stay in range for viewport {vp:?}"
                );
            }
        }
    }

    #[test]
    fn collect_box_props_fills_debug_messages_when_asked() {
        let sd = styled(
            Dom::create_body().with_child(div_class("x")),
            ".x { margin: 3px; }",
        );
        let mut msgs: Option<Vec<LayoutDebugMessage>> = Some(Vec::new());
        let _ = collect_box_props(&sd, NodeId::new(1), &mut msgs, VIEWPORT);
        assert!(
            !msgs.expect("still Some").is_empty(),
            "a Some(vec) sink must actually receive the [BOX] trace"
        );

        // …and a None sink must be left alone (no allocation, no panic).
        let mut none_sink: Option<Vec<LayoutDebugMessage>> = None;
        let _ = collect_box_props(&sd, NodeId::new(1), &mut none_sink, VIEWPORT);
        assert!(none_sink.is_none());
    }

    #[test]
    fn collect_box_props_unresolved_and_resolved_agree_after_a_re_resolve() {
        let c = collect_for(".x { margin: 4px; padding: 6px; }", 1, VIEWPORT);
        let params = crate::solver3::geometry::ResolutionParams {
            containing_block: VIEWPORT,
            viewport_size: VIEWPORT,
            element_font_size: DEFAULT_FONT_SIZE,
            root_font_size: DEFAULT_FONT_SIZE,
        };
        let again = c.unresolved.resolve(&params);
        assert_eq!(again.margin.top, c.resolved.margin.top);
        assert_eq!(again.padding.left, c.resolved.padding.left);
        assert_eq!(again.border.top, c.resolved.border.top);
    }

    #[test]
    fn edge_sizes_default_is_all_zero() {
        let e = EdgeSizes::default();
        assert_eq!((e.top, e.right, e.bottom, e.left), (0.0, 0.0, 0.0, 0.0));
    }

    // ==================================================================
    // Whole-pipeline invariants
    // ==================================================================

    #[test]
    fn a_freshly_built_tree_satisfies_every_structural_invariant() {
        for sd in [
            mixed_dom(),
            styled(Dom::create_body(), ""),
            styled(
                Dom::create_body().with_child(div_class("f").with_child(div_class("c"))),
                ".f { display: flex; } .c { display: table-cell; }",
            ),
            styled(
                Dom::create_body().with_child(div_class("t").with_child(div_class("c"))),
                ".t { display: table; } .c { display: table-cell; }",
            ),
            styled(
                Dom::create_body().with_child(
                    div_class("li")
                        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("x")),
                ),
                ".li { display: list-item; }",
            ),
        ] {
            let tree = build_tree(&sd);
            let n = tree.nodes.len();
            assert!(n >= 1);
            assert_eq!(tree.warm.len(), n);
            assert_eq!(tree.cold.len(), n);
            assert_eq!(tree.children_offsets.len(), n);
            assert_eq!(tree.subtree_needs_intrinsic.len(), n);
            assert!(tree.root < n);
            assert_eq!(tree.get(LayoutNodeId::new(tree.root)).unwrap().parent, None);

            for i in 0..n {
                if let Some(p) = tree.get(LayoutNodeId::new(i)).unwrap().parent {
                    assert!(p < n, "node {i}'s parent {p} is out of range");
                }
                for &c in tree.children(i) {
                    assert!(c < n, "node {i}'s child {c} is out of range");
                    assert_ne!(c, i, "no node may be its own child");
                }
            }
            for (dom_id, indices) in &tree.dom_to_layout {
                for &i in indices {
                    assert!(
                        i < LayoutNodeId::new(n),
                        "dom_to_layout[{dom_id:?}] points at {i}, out of range"
                    );
                    assert_eq!(tree.get(i).unwrap().dom_node_id, Some(*dom_id));
                }
            }
        }
    }

    #[test]
    fn building_a_body_only_dom_yields_exactly_one_node() {
        let sd = styled(Dom::create_body(), "");
        let tree = build_tree(&sd);
        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.root, 0);
        assert!(tree.children(0).is_empty());
        assert!(tree.children_arena.is_empty());
        assert_eq!(tree.children_offsets, vec![(0, 0)]);
        assert_eq!(tree.memory_report().node_count, 1);
    }

    #[test]
    fn the_root_box_always_establishes_a_new_block_formatting_context() {
        let tree = build_tree(&mixed_dom());
        match tree
            .get(LayoutNodeId::new(tree.root))
            .unwrap()
            .formatting_context
        {
            FormattingContext::Block {
                establishes_new_context,
            } => assert!(
                establishes_new_context,
                "process_node forces this for the root"
            ),
            other => panic!("the root should be a Block FC, got {other:?}"),
        }
    }

    #[test]
    fn a_deeply_nested_dom_builds_without_blowing_the_stack() {
        // process_node recurses once per level; 200 is well inside a test thread's
        // stack but deep enough to catch an accidental per-level allocation blowup.
        let mut dom = div_class("d");
        for _ in 0..200 {
            dom = div_class("d").with_child(dom);
        }
        let sd = styled(Dom::create_body().with_child(dom), ".d { display: block; }");
        let tree = build_tree(&sd);
        assert_eq!(tree.nodes.len(), 202, "body + 201 divs");
        // The chain must be a straight line: every node but the last has 1 child.
        let mut i = tree.root;
        let mut depth = 0;
        while let Some(&next) = tree.children(i).first() {
            i = next;
            depth += 1;
            assert!(depth <= 202, "the parent/child links formed a cycle");
        }
        assert_eq!(depth, 201);
    }

    // ==================================================================
    // ancestor_chain — THE ancestor walk, with explicit inclusivity
    // ==================================================================

    /// body > a > b, a straight chain, so the ancestor order is unambiguous.
    fn chain_tree() -> LayoutTree {
        let sd = styled(
            Dom::create_body().with_child(div_class("a").with_child(div_class("b"))),
            ".a, .b { display: block; }",
        );
        build_tree(&sd)
    }

    fn chain_from(tree: &LayoutTree, start: usize, incl: Inclusivity) -> Vec<usize> {
        tree.ancestor_chain(LayoutNodeId::new(start), incl)
            .map(LayoutNodeId::index)
            .collect()
    }

    #[test]
    fn ancestor_chain_inclusivity_decides_only_the_first_element() {
        let tree = chain_tree();
        // The deepest node is the last one the straight chain reaches.
        let mut deepest = tree.root;
        while let Some(&next) = tree.children(deepest).first() {
            deepest = next;
        }
        let with_self = chain_from(&tree, deepest, Inclusivity::SelfAndAncestors);
        let without = chain_from(&tree, deepest, Inclusivity::AncestorsOnly);
        assert_eq!(with_self.first(), Some(&deepest));
        assert_eq!(&with_self[1..], &without[..]);
        assert_eq!(
            with_self.last(),
            Some(&tree.root),
            "the walk must reach the root"
        );
        assert_eq!(with_self.len(), without.len() + 1);
    }

    #[test]
    fn ancestor_chain_at_the_root_is_the_root_or_nothing() {
        let tree = chain_tree();
        assert_eq!(
            chain_from(&tree, tree.root, Inclusivity::SelfAndAncestors),
            vec![tree.root]
        );
        assert!(chain_from(&tree, tree.root, Inclusivity::AncestorsOnly).is_empty());
    }

    #[test]
    fn ancestor_chain_on_an_out_of_range_index_yields_at_most_that_index() {
        let tree = chain_tree();
        assert!(chain_from(&tree, 9_999, Inclusivity::AncestorsOnly).is_empty());
        // Self-inclusive still reports the requested index once (callers
        // filter on `tree.get()`), then stops: no panic, no spin.
        assert_eq!(
            chain_from(&tree, 9_999, Inclusivity::SelfAndAncestors),
            vec![9_999]
        );
    }

    #[test]
    fn ancestor_chain_terminates_on_a_cyclic_parent_link() {
        let mut tree = chain_tree();
        // Forge a cycle: root's parent points at its own child.
        let child = tree
            .children(tree.root)
            .first()
            .copied()
            .expect("body has a child");
        tree.nodes[tree.root].parent = Some(child);
        let walked = chain_from(&tree, child, Inclusivity::SelfAndAncestors);
        assert!(
            walked.len() <= tree.nodes.len(),
            "the budget must stop a cyclic chain, walked {} of {} nodes",
            walked.len(),
            tree.nodes.len()
        );
    }
}
