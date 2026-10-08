//! The text shaping cache: its entries and keys, its memory report and the layout entry points it serves.

use std::{
    cmp::Ordering,
    collections::{
        hash_map::{DefaultHasher, HashMap},
        BTreeSet, HashSet,
    },
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::ImageRef,
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::{
    corety::LayoutDebugMessage,
    props::{basic::ColorU, style::StyleBackgroundContent},
};
#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Language as HyphenationLanguage, Load, Standard};
use rust_fontconfig::{
    FcFontCache, FcPattern, FcStretch, FcWeight, FontId, PatternMatch, UnicodeRange,
};
use smallvec::{smallvec, SmallVec};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;
use crate::text3::script::{script_to_language, Language, Script};
#[allow(clippy::wildcard_imports)]
// the text layout cache's items, re-exported from the sibling modules by mod.rs
use super::*;

// --- Caching Infrastructure ---

pub type CacheId = u64;

/// (d7, segmented rework) The compact stored form of a per-item shaped
/// entry. A coalesce GROUP spans multiple logical items, so amortized
/// fields change mid-entry — the first single-header design atomized
/// every cluster after the first text-Arc change and retained ~300
/// B/cluster on the real corpus (measured 9.1 MiB; the whole point
/// missed). Headers are now per-SEGMENT (the `DenseRun` pattern): a new
/// segment starts whenever any amortized field changes; clusters
/// compact to 16 B within their segment; glyph irregularities go to
/// the shared detail tables; only non-cluster items and genuinely
/// irregular clusters (multi-font glyphs, markers, cleared fragment
/// flags) stay verbatim in `atoms`. `expand()` reproduces the input
/// EXACTLY (the d7 roundtrip gate pins it, including a multi-segment
/// case); the per-hit re-stamp then runs unchanged.
#[derive(Debug)]
pub(crate) struct CompactSegment {
    style: Arc<StyleProperties>,
    source_text: Arc<str>,
    font_hash: u64,
    font_metrics: LayoutFontMetrics,
    script: Script,
    direction: BidiDirection,
    source_run: u32,
    source_node: Option<NodeId>,
    /// The segment's `source_content_index.item_index` VERBATIM: at the
    /// shaping stage it is constant per item-fragment (the split-trace
    /// showed the linear `item_base` model drifting on EVERY cluster —
    /// 31k segments; post-layout dense uses the linear model, this
    /// stage does not).
    item_index: u32,
    /// Range into the entry-wide `clusters` array.
    clusters: core::ops::Range<u32>,
}

#[derive(Debug)]
pub(crate) struct CompactShapedEntry {
    segments: Vec<CompactSegment>,
    pub(super) clusters: Vec<super::super::dense::ClusterCompact>,
    details: Vec<super::super::dense::ClusterDetail>,
    detail_glyphs: Vec<super::super::dense::DetailGlyph>,
    /// (`expanded_index`, verbatim item) — non-clusters and irregulars.
    atoms: Vec<(u32, ShapedItem)>,
    /// Expanded sequence length.
    total: u32,
}

impl CompactShapedEntry {
    /// Compact `items`. Total: every item lands in a segment's compact
    /// arrays or verbatim in `atoms`; `expand()` is exact either way.
    pub(crate) fn build(items: &[ShapedItem]) -> Self {
        use super::super::dense::{ClusterCompact, ClusterDetail, DetailGlyph};
        let mut out = Self {
            segments: Vec::new(),
            clusters: Vec::new(),
            details: Vec::new(),
            detail_glyphs: Vec::new(),
            atoms: Vec::new(),
            total: u32::try_from(items.len()).unwrap_or(u32::MAX),
        };
        for (i, item) in items.iter().enumerate() {
            let idx = u32::try_from(i).unwrap_or(u32::MAX);
            let ShapedItem::Cluster(c) = item else {
                out.atoms.push((idx, item.clone()));
                continue;
            };
            let first_glyph = c.glyphs.first();
            let font_hash = first_glyph.map_or(0, |g| g.font_hash);
            let font_metrics = first_glyph.map_or(
                LayoutFontMetrics {
                    ascent: 0.0,
                    descent: 0.0,
                    cap_height: None,
                    browser_ascent_boost: false,
                    x_height: None,
                    line_gap: 0.0,
                    units_per_em: 0,
                },
                |g| g.font_metrics,
            );
            let script = first_glyph.map_or(Script::Latin, |g| g.script);
            let item_index = c.source_content_index.item_index;
            // Irregular clusters stay verbatim: mixed fonts WITHIN one
            // cluster, markers, or fragment flags off their shaping-
            // stage default (true, true) — the line breaker owns those.
            let irregular = c.marker_position_outside.is_some()
                || !c.is_first_fragment
                || !c.is_last_fragment
                // font_hash IS the font identity (metrics are derived
                // from it); comparing LayoutFontMetrics by PartialEq
                // split a segment on EVERY cluster when a metric was
                // NaN (NaN != NaN) — 31,174 segments for 31k clusters.
                || c.glyphs.iter().any(|g| {
                    g.font_hash != font_hash || g.script != script
                })
                || (!Self::needs_detail(c)
                    && Self::grapheme_len_at(
                        &c.source_text,
                        c.source_cluster_id.start_byte_in_run,
                    ) != Some(usize::from(c.source_byte_len)));
            if irregular {
                out.atoms.push((idx, item.clone()));
                continue;
            }
            // Segment split on any amortized-field change.
            let ci = u32::try_from(out.clusters.len()).unwrap_or(u32::MAX);
            let fits = out.segments.last().is_some_and(|seg| {
                Arc::ptr_eq(&seg.style, &c.style)
                    && Arc::ptr_eq(&seg.source_text, &c.source_text)
                    && seg.font_hash == font_hash
                    && seg.script == script
                    && seg.direction == c.direction
                    && seg.source_run == c.source_cluster_id.source_run
                    && seg.source_node == c.source_node_id
                    && seg.item_index == item_index
                    // Segments must stay contiguous in the cluster
                    // array; an intervening atom ends the segment.
                    && seg.clusters.end == ci
            });
            if fits {
                if let Some(seg) = out.segments.last_mut() {
                    seg.clusters.end = ci + 1;
                }
            } else {
                out.segments.push(CompactSegment {
                    style: c.style.clone(),
                    source_text: c.source_text.clone(),
                    font_hash,
                    font_metrics,
                    script,
                    direction: c.direction,
                    source_run: c.source_cluster_id.source_run,
                    source_node: c.source_node_id,
                    item_index,
                    // `ci..ci + 1`, NOT `ci..=ci`: the field is a half-open
                    // `Range<u32>`, so clippy::range_plus_one's rewrite does
                    // not typecheck. The allow keeps `--fix` from re-breaking it.
                    #[allow(clippy::range_plus_one)]
                    clusters: ci..ci + 1,
                });
            }
            if Self::needs_detail(c) {
                let start = u32::try_from(out.detail_glyphs.len()).unwrap_or(u32::MAX);
                for g in &c.glyphs {
                    out.detail_glyphs.push(DetailGlyph {
                        glyph_id: g.glyph_id,
                        cluster_offset: u16::try_from(g.cluster_offset).unwrap_or(u16::MAX),
                        advance: g.advance + g.kerning,
                        offset_x: g.offset.x,
                        offset_y: g.offset.y,
                        kerning: g.kerning,
                        kind: g.kind,
                        vertical_advance: g.vertical_advance,
                        vertical_offset_x: g.vertical_offset.x,
                        vertical_offset_y: g.vertical_offset.y,
                    });
                }
                let end = u32::try_from(out.detail_glyphs.len()).unwrap_or(u32::MAX);
                out.details.push(ClusterDetail {
                    cluster: ci,
                    glyphs: (start, end),
                    byte_len: u32::from(c.source_byte_len),
                });
            }
            out.clusters.push(ClusterCompact {
                glyph_id: first_glyph.map_or(0, |g| g.glyph_id),
                flags: c.flags,
                advance: c.advance,
                start_byte: c.source_cluster_id.start_byte_in_run,
                x: 0.0,
            });
        }
        out
    }

    fn needs_detail(c: &ShapedCluster) -> bool {
        // Vertical metrics come from the font's vmtx table — per-glyph
        // and nonzero for CJK-class fonts even in horizontal text — so
        // they route to the DETAIL table rather than atomizing.
        c.glyphs.len() != 1
            || c.glyphs.first().is_some_and(|g| {
                g.offset.x != 0.0
                    || g.offset.y != 0.0
                    || g.kerning != 0.0
                    || g.kind != GlyphKind::Character
                    || g.vertical_advance != 0.0
                    || g.vertical_offset.x != 0.0
                    || g.vertical_offset.y != 0.0
            })
    }

    fn grapheme_len_at(text: &str, start: u32) -> Option<usize> {
        use unicode_segmentation::UnicodeSegmentation;
        text.get(start as usize..)
            .and_then(|s| s.graphemes(true).next())
            .map(str::len)
    }

    /// Exact reconstruction of the original item vec.
    pub(crate) fn expand(&self) -> Vec<ShapedItem> {
        let mut out = Vec::with_capacity(self.total as usize);
        let mut atom_cursor = 0usize;
        let mut detail_cursor = 0usize;
        let mut seg_cursor = 0usize;
        let mut ci = 0u32;
        for idx in 0..self.total {
            if let Some((ai, item)) = self.atoms.get(atom_cursor) {
                if *ai == idx {
                    out.push(item.clone());
                    atom_cursor += 1;
                    continue;
                }
            }
            while seg_cursor < self.segments.len() && self.segments[seg_cursor].clusters.end <= ci {
                seg_cursor += 1;
            }
            let seg = &self.segments[seg_cursor];
            let c = &self.clusters[ci as usize];
            while detail_cursor < self.details.len() && self.details[detail_cursor].cluster < ci {
                detail_cursor += 1;
            }
            let detail = self.details.get(detail_cursor).filter(|d| d.cluster == ci);
            let glyphs: ShapedGlyphVec = match detail {
                Some(d) => (d.glyphs.0..d.glyphs.1)
                    .map(|gi| {
                        let dg = &self.detail_glyphs[gi as usize];
                        ShapedGlyph {
                            kind: dg.kind,
                            glyph_id: dg.glyph_id,
                            cluster_offset: u32::from(dg.cluster_offset),
                            advance: dg.advance - dg.kerning,
                            kerning: dg.kerning,
                            offset: Point {
                                x: dg.offset_x,
                                y: dg.offset_y,
                            },
                            vertical_advance: dg.vertical_advance,
                            vertical_offset: Point {
                                x: dg.vertical_offset_x,
                                y: dg.vertical_offset_y,
                            },
                            script: seg.script,
                            font_hash: seg.font_hash,
                            font_metrics: seg.font_metrics,
                        }
                    })
                    .collect(),
                None => core::iter::once(ShapedGlyph {
                    kind: GlyphKind::Character,
                    glyph_id: c.glyph_id,
                    cluster_offset: 0,
                    advance: c.advance,
                    kerning: 0.0,
                    offset: Point { x: 0.0, y: 0.0 },
                    vertical_advance: 0.0,
                    vertical_offset: Point { x: 0.0, y: 0.0 },
                    script: seg.script,
                    font_hash: seg.font_hash,
                    font_metrics: seg.font_metrics,
                })
                .collect(),
            };
            let byte_len = detail.map_or_else(
                || Self::grapheme_len_at(&seg.source_text, c.start_byte).unwrap_or(0) as u32,
                |d| d.byte_len,
            );
            out.push(ShapedItem::Cluster(ShapedCluster {
                source_text: seg.source_text.clone(),
                source_byte_len: u16::try_from(byte_len).unwrap_or(u16::MAX),
                source_cluster_id: GraphemeClusterId {
                    source_run: seg.source_run,
                    start_byte_in_run: c.start_byte,
                },
                source_content_index: ContentIndex {
                    run_index: seg.source_run,
                    item_index: seg.item_index,
                },
                source_node_id: seg.source_node,
                glyphs,
                flags: c.flags,
                advance: c.advance,
                direction: seg.direction,
                style: seg.style.clone(),
                marker_position_outside: None,
                is_first_fragment: true,
                is_last_fragment: true,
            }));
            ci += 1;
        }
        out
    }

    /// Approximate retained bytes, for the memory report.
    pub(crate) const fn retained_bytes(&self) -> usize {
        use core::mem::size_of;
        self.segments.capacity() * size_of::<CompactSegment>()
            + self.clusters.capacity() * size_of::<super::super::dense::ClusterCompact>()
            + self.details.capacity() * size_of::<super::super::dense::ClusterDetail>()
            + self.detail_glyphs.capacity() * size_of::<super::super::dense::DetailGlyph>()
            + self.atoms.capacity() * (size_of::<(u32, ShapedItem)>())
    }

    #[cfg(test)]
    pub(crate) fn atom_count(&self) -> usize {
        self.atoms.len()
    }

    #[cfg(test)]
    pub(crate) fn segment_count(&self) -> usize {
        self.segments.len()
    }
}

/// Cached shaped result for a single visual item (or coalesced group).
/// Enables per-item cache hits when only one word changes in a paragraph.
/// (d7) Stores the COMPACT form; `expand()` materializes on hit, where
/// the old form cloned every item anyway.
#[derive(Debug)]
pub(crate) struct PerItemShapedEntry {
    /// The compacted shaped clusters for this single item/group.
    pub(crate) compact: CompactShapedEntry,
    /// Sum of advance widths — for fast same-width detection during incremental relayout.
    pub(crate) total_advance: f32,
    /// The group's text items as they were when it was shaped, in group
    /// order: each one's `ContentIndex` and its byte offset in its logical
    /// run (`VisualItem::run_byte_offset`). A hit re-stamps a cluster from the
    /// hitting group's item at the SAME position - the key hashes the texts in
    /// order, so both groups hold the same items, but not at the same run
    /// indices.
    pub(crate) items: Vec<(ContentIndex, usize)>,
}

/// One text item of a shaping group, as a cache hit re-stamps from it.
pub(super) struct GroupItem {
    pub(super) source: ContentIndex,
    pub(super) run_byte_offset: usize,
    pub(super) style: Arc<StyleProperties>,
    pub(super) source_node_id: Option<NodeId>,
}

/// The text items of a shaping group, in group order.
pub(super) fn group_items(group: &[VisualItem]) -> Vec<GroupItem> {
    group
        .iter()
        .filter_map(|it| match &it.logical_source {
            LogicalItem::Text {
                source,
                style,
                source_node_id,
                ..
            } => Some(GroupItem {
                source: *source,
                run_byte_offset: it.run_byte_offset,
                style: style.clone(),
                source_node_id: *source_node_id,
            }),
            LogicalItem::CombinedText { source, style, .. } => Some(GroupItem {
                source: *source,
                run_byte_offset: it.run_byte_offset,
                style: style.clone(),
                source_node_id: None,
            }),
            _ => None,
        })
        .collect()
}

#[derive(Debug)]
pub struct TextShapingCache {
    // Stage 1 Cache: InlineContent -> LogicalItems
    pub(super) logical_items: HashMap<CacheId, Arc<Vec<LogicalItem>>>,
    // Stage 2 Cache: LogicalItems -> VisualItems
    pub(super) visual_items: HashMap<CacheId, Arc<Vec<VisualItem>>>,
    // (d7) The monolithic Stage-3 cache (VisualItems -> ShapedItems) is
    // DELETED: it duplicated every cluster held by `per_item_shaped`
    // (6.1 MB on the 960-line corpus at fat-struct sizes), text edits
    // always missed it (the key hashes the text), and post-R1/R2 resize
    // rarely re-enters layout at all. Assembly from the per-item cache
    // runs each pass; the per-item entries are the single cache copy.
    // Stage 3b Cache: Per-item/coalesce-group shaped results
    // Key: hash(text, bidi_level, script, style.layout_hash())
    pub(super) per_item_shaped: HashMap<u64, Arc<PerItemShapedEntry>>,
    /// Tracks which `per_item_shaped` keys were accessed in the current generation.
    pub(super) per_item_accessed: HashSet<u64>,
    /// Same, for the three STAGE caches above.
    ///
    /// Those three had no cap and no sweep of any kind: every distinct piece
    /// of text ever laid out stayed in them for the life of the process, so
    /// they grew monotonically with editing. Measured on one 960-line
    /// markdown: 792 entries each, 7.0 MB (logical 396 KiB, visual 519 KiB,
    /// shaped 6140 KiB) — on top of the 13.5 MB of the SAME shaped text
    /// already retained per layout node as `warm.inline`.
    pub(super) stage_accessed: HashSet<CacheId>,
    /// Current generation counter, incremented each layout pass.
    pub(super) generation: u64,
}

/// Bytes an `Arc` allocation adds in front of its payload: strong + weak
/// refcount.
pub(super) const ARC_HEADER: usize = 2 * size_of::<usize>();

/// Approximate heap bytes of a `HashMap`'s own table.
///
/// hashbrown sizes for `entries / 0.875` buckets rounded up to a power of two,
/// each holding a `(K, V)` pair plus one control byte. An ESTIMATE, labelled as
/// such where it is printed — but far closer than the zero charged before.
pub(super) const fn hashmap_bytes<K, V>(entries: usize) -> usize {
    if entries == 0 {
        return 0;
    }
    let buckets = ((entries * 8) / 7).next_power_of_two();
    buckets * (size_of::<K>() + size_of::<V>() + 1)
}

/// Approximate heap bytes retained by a [`TextShapingCache`].
#[derive(Copy, Debug, Clone, Default)]
pub struct TextCacheMemoryReport {
    pub logical_items_entries: usize,
    pub logical_items_bytes: usize,
    pub visual_items_entries: usize,
    pub visual_items_bytes: usize,
    pub shaped_items_entries: usize,
    pub shaped_items_bytes: usize,
    pub shaped_glyph_bytes: usize,
    pub shaped_cluster_text_bytes: usize,
    pub per_item_shaped_entries: usize,
    pub per_item_shaped_bytes: usize,
    pub per_item_atoms: usize,
    pub per_item_segments: usize,
    pub per_item_detail_glyphs: usize,
    /// `HashMap` table allocations plus one `Arc` header per entry.
    ///
    /// Previously uncounted, which is why the itemised lines did not add up
    /// to the total anyone read off this report: on a 960-line document the
    /// four maps hold ~3 000 entries and the gap was ~3.4 MB.
    pub map_overhead_bytes: usize,
    /// Distinct `Arc<StyleProperties>` reachable from cached clusters, and
    /// the bytes they hold. Counted once per allocation, not per referent —
    /// the whole point of the `Arc` is that glyphs share it.
    pub distinct_style_arcs: usize,
    pub style_arc_bytes: usize,
    /// Glyphs in `ShapedItem::CombinedBlock` (tate-chu-yoko). Zero on Latin;
    /// non-zero on vertical CJK, where the old walk silently skipped them
    /// because it only matched the `Cluster` arm.
    pub combined_block_glyph_bytes: usize,
    /// Bytes the NAIVE per-key walk would have charged twice, because several
    /// cache keys point at one shared `Arc`. Not part of any total — this is
    /// the size of the error that per-key counting used to make, kept in the
    /// output so the correction is auditable rather than invisible.
    pub shared_bytes_avoided: usize,
    /// Cluster count, so a reader can derive bytes-per-cluster without
    /// having to find it in another section of the report.
    pub cluster_count: usize,
}

impl TextCacheMemoryReport {
    #[must_use]
    pub const fn total_bytes(&self) -> usize {
        self.logical_items_bytes
            + self.visual_items_bytes
            + self.shaped_items_bytes
            + self.shaped_glyph_bytes
            + self.shaped_cluster_text_bytes
            + self.per_item_shaped_bytes
            + self.map_overhead_bytes
            + self.style_arc_bytes
            + self.combined_block_glyph_bytes
    }

    /// Bytes per shaped cluster, the figure worth comparing against other
    /// engines. `None` when nothing is cached.
    #[must_use]
    pub const fn bytes_per_cluster(&self) -> Option<usize> {
        if self.cluster_count == 0 {
            None
        } else {
            Some(self.total_bytes() / self.cluster_count)
        }
    }
}

impl TextShapingCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            logical_items: HashMap::new(),
            visual_items: HashMap::new(),
            per_item_shaped: HashMap::new(),
            per_item_accessed: HashSet::new(),
            stage_accessed: HashSet::new(),
            generation: 0,
        }
    }

    /// #28: fork this cache for a SPECULATIVE layout query (`LayoutWindow::
    /// query_pagination`). The three stage maps hold `Arc`'d entries keyed by
    /// CONTENT (the per-item key hashes text/bidi/script/style — never a
    /// width), so cloning the maps is refcount bumps only: the fork re-shapes
    /// nothing that the live cache already shaped. Entries the query adds
    /// (its own constraint's line breaks, new memoizations) land in the fork
    /// and die with it — the window's cache is never polluted with
    /// query-constraint entries, which is why this takes `&self` and works
    /// from read-only callback contexts.
    #[must_use]
    pub fn fork_shared(&self) -> Self {
        Self {
            logical_items: self.logical_items.clone(),
            visual_items: self.visual_items.clone(),
            per_item_shaped: self.per_item_shaped.clone(),
            per_item_accessed: HashSet::new(),
            stage_accessed: HashSet::new(),
            generation: self.generation,
        }
    }

    /// Test/pin hook (#28): whether `key` maps to the SAME allocation as in
    /// `other` — proves a fork shares (not copies) a shaped entry.
    #[must_use]
    pub fn per_item_entry_ptr_eq(&self, other: &Self, key: u64) -> bool {
        match (
            self.per_item_shaped.get(&key),
            other.per_item_shaped.get(&key),
        ) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Test/pin hook (#28): the per-item keys currently cached.
    #[must_use]
    pub fn per_item_keys(&self) -> Vec<u64> {
        self.per_item_shaped.keys().copied().collect()
    }

    /// Approximate per-stage heap-byte breakdown.
    #[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
    #[must_use]
    pub fn memory_report(&self) -> TextCacheMemoryReport {
        let mut r = TextCacheMemoryReport::default();

        // COUNT EACH ALLOCATION ONCE, NOT EACH KEY.
        //
        // All four stage maps are `HashMap<_, Arc<..>>`, and sharing one `Arc`
        // between several keys is the entire point of the cache. Charging
        // `capacity()` per VALUE therefore charged shared allocations once per
        // key that referenced them, and the report claimed bytes that do not
        // exist in the process. `shared_bytes_avoided` records what the naive
        // walk would have double-charged, so the correction is visible in the
        // output instead of appearing as an unexplained drop.
        let mut counted: BTreeSet<usize> = BTreeSet::new();

        r.logical_items_entries = self.logical_items.len();
        for arc in self.logical_items.values() {
            let bytes = arc.capacity() * size_of::<LogicalItem>();
            if counted.insert(Arc::as_ptr(arc).cast::<u8>() as usize) {
                r.logical_items_bytes += bytes;
            } else {
                r.shared_bytes_avoided += bytes;
            }
        }
        r.visual_items_entries = self.visual_items.len();
        for arc in self.visual_items.values() {
            let bytes = arc.capacity() * size_of::<VisualItem>();
            if counted.insert(Arc::as_ptr(arc).cast::<u8>() as usize) {
                r.visual_items_bytes += bytes;
            } else {
                r.shared_bytes_avoided += bytes;
            }
        }
        let mut text_arcs: BTreeSet<usize> = BTreeSet::new();
        let mut style_arcs: BTreeSet<usize> = BTreeSet::new();

        // ONE glyph lives INLINE in the cluster's `SmallVec<[ShapedGlyph; 1]>`
        // and is already inside the `size_of::<ShapedItem>()` charged above.
        // Counting full `capacity()` charged it twice — on Latin text, where
        // every cluster has exactly one glyph, that DOUBLED the reported glyph
        // bytes. Fixed in `solver3/layout_tree.rs:952` (d41a15dbe); never
        // applied here until now.
        fn glyph_spill_bytes(c: &ShapedCluster) -> usize {
            c.glyphs.capacity().saturating_sub(1) * size_of::<ShapedGlyph>()
        }

        // (d7) The monolithic stage-3 map is DELETED; these fields stay
        // in the report as tombstones (0) so historical dumps compare.
        r.shaped_items_entries = 0;
        // (the walk below is the single shaped-store walk now)
        r.per_item_shaped_entries = self.per_item_shaped.len();
        for arc in self.per_item_shaped.values() {
            if !counted.insert(Arc::as_ptr(arc).cast::<u8>() as usize) {
                r.shared_bytes_avoided += arc.compact.retained_bytes();
                continue;
            }
            // (d7) The compact arrays, plus each segment's shared source
            // text once.
            r.per_item_shaped_bytes += arc.compact.retained_bytes();
            r.per_item_atoms += arc.compact.atoms.len();
            r.per_item_segments += arc.compact.segments.len();
            r.per_item_detail_glyphs += arc.compact.detail_glyphs.len();
            r.cluster_count += arc.compact.clusters.len();
            for seg in &arc.compact.segments {
                if text_arcs.insert(Arc::as_ptr(&seg.source_text).cast::<u8>() as usize) {
                    r.per_item_shaped_bytes += seg.source_text.len();
                }
                style_arcs.insert(Arc::as_ptr(&seg.style) as usize);
            }
            for (_, item) in &arc.compact.atoms {
                match item {
                    ShapedItem::Cluster(c) => {
                        r.per_item_shaped_bytes += glyph_spill_bytes(c);
                        // 3c: shared Arc slice — count each source buffer once.
                        if text_arcs.insert(Arc::as_ptr(&c.source_text).cast::<u8>() as usize) {
                            r.per_item_shaped_bytes += c.source_text.len();
                        }
                        r.cluster_count += 1;
                        style_arcs.insert(Arc::as_ptr(&c.style) as usize);
                    }
                    ShapedItem::CombinedBlock { glyphs, style, .. } => {
                        r.combined_block_glyph_bytes +=
                            glyphs.capacity() * size_of::<ShapedGlyph>();
                        style_arcs.insert(Arc::as_ptr(style) as usize);
                    }
                    // No heap beyond the `size_of::<ShapedItem>()` already
                    // charged for the slot. Listed explicitly rather than
                    // caught by a wildcard so that adding a heap-owning arm
                    // later fails to compile instead of silently going
                    // uncounted — which is exactly how `CombinedBlock` was
                    // missed by the old `if let Cluster`.
                    ShapedItem::Object { .. }
                    | ShapedItem::Tab { .. }
                    | ShapedItem::Break { .. } => {}
                }
            }
        }

        r.distinct_style_arcs = style_arcs.len();
        r.style_arc_bytes = style_arcs.len() * (size_of::<StyleProperties>() + ARC_HEADER);

        // The maps themselves. Every line above measures what an entry POINTS
        // AT; none measured the table holding the pointers or the `Arc` header
        // in front of each payload. That is why the itemised lines never summed
        // to the printed total — a ~3.4 MB gap on a 960-line document.
        r.map_overhead_bytes =
            hashmap_bytes::<CacheId, Arc<Vec<LogicalItem>>>(self.logical_items.len())
                + hashmap_bytes::<CacheId, Arc<Vec<VisualItem>>>(self.visual_items.len())
                + hashmap_bytes::<u64, Arc<PerItemShapedEntry>>(self.per_item_shaped.len())
                + ARC_HEADER
                    * (self.logical_items.len()
                        + self.visual_items.len()
                        + self.per_item_shaped.len());
        r
    }

    /// Call at the start of each layout pass. Evicts per-item shaped entries
    /// not accessed in the previous generation to prevent unbounded growth.
    pub fn begin_generation(&mut self) {
        if self.generation > 0 && !self.per_item_accessed.is_empty() {
            // Evict entries not accessed in this generation
            let accessed = &self.per_item_accessed;
            self.per_item_shaped.retain(|k, _| accessed.contains(k));
        }
        // The three STAGE caches get the same policy. They had none at all,
        // so text that is edited away was never released — the entry for the
        // old wording stayed shaped and resident forever. Same rule as
        // per-item: anything touched this generation survives.
        if self.generation > 0 && !self.stage_accessed.is_empty() {
            let accessed = &self.stage_accessed;
            self.logical_items.retain(|k, _| accessed.contains(k));
            self.visual_items.retain(|k, _| accessed.contains(k));
        }
        self.per_item_accessed.clear();
        self.stage_accessed.clear();
        self.generation += 1;
    }

    /// Entry counts of the stage caches, for tests and the memory
    /// report. The third slot is the PER-ITEM shaped store since d7
    /// (the monolithic stage-3 map is deleted).
    #[must_use]
    pub fn stage_entry_counts(&self) -> (usize, usize, usize) {
        (
            self.logical_items.len(),
            self.visual_items.len(),
            self.per_item_shaped.len(),
        )
    }

    /// Mark a stage-cache id as used this generation (see `stage_accessed`).
    pub(super) fn touch_stage(&mut self, id: CacheId) {
        self.stage_accessed.insert(id);
    }

    /// Check if we can reuse an old layout based on layout-affecting parameters.
    ///
    /// This function compares only the parameters that affect glyph positions,
    /// not rendering-only parameters like color or text-decoration.
    ///
    /// # Parameters
    /// - `old_constraints`: The constraints used for the cached layout
    /// - `new_constraints`: The constraints for the new layout request
    /// - `old_content`: The content used for the cached layout
    /// - `new_content`: The new content to layout
    ///
    /// # Returns
    /// - `true` if the old layout can be reused (only rendering changed)
    /// - `false` if a new layout is needed (layout-affecting params changed)
    #[must_use]
    pub fn use_old_layout(
        old_constraints: &UnifiedConstraints,
        new_constraints: &UnifiedConstraints,
        old_content: &[InlineContent],
        new_content: &[InlineContent],
    ) -> bool {
        // First check: constraints must match exactly for layout purposes
        if old_constraints != new_constraints {
            return false;
        }

        // Second check: content length must match
        if old_content.len() != new_content.len() {
            return false;
        }

        // Third check: each content item must have same layout properties
        for (old, new) in old_content.iter().zip(new_content.iter()) {
            if !Self::inline_content_layout_eq(old, new) {
                return false;
            }
        }

        true
    }

    /// Compare two `InlineContent` items for layout equality.
    ///
    /// Returns true if the layouts would be identical (only rendering differs).
    pub(super) fn inline_content_layout_eq(old: &InlineContent, new: &InlineContent) -> bool {
        use InlineContent::{Image, LineBreak, Marker, Ruby, Shape, Space, Tab, Text};
        match (old, new) {
            (Text(old_run), Text(new_run)) => {
                // Text must match exactly, but style only needs layout_eq
                old_run.text == new_run.text && old_run.style.layout_eq(&new_run.style)
            }
            (Image(old_img), Image(new_img)) => {
                // Images: size affects layout, but not visual properties
                old_img.intrinsic_size == new_img.intrinsic_size
                    && old_img.display_size == new_img.display_size
                    && old_img.baseline_offset == new_img.baseline_offset
                    && old_img.alignment == new_img.alignment
            }
            (Space(old_sp), Space(new_sp)) => old_sp == new_sp,
            (LineBreak(old_br), LineBreak(new_br)) => old_br == new_br,
            (Tab { style: old_style }, Tab { style: new_style }) => old_style.layout_eq(new_style),
            (
                Marker {
                    run: old_run,
                    position_outside: old_pos,
                },
                Marker {
                    run: new_run,
                    position_outside: new_pos,
                },
            ) => {
                old_pos == new_pos
                    && old_run.text == new_run.text
                    && old_run.style.layout_eq(&new_run.style)
            }
            (Shape(old_shape), Shape(new_shape)) => {
                // Shapes: shape_def affects layout, not fill/stroke
                old_shape.shape_def == new_shape.shape_def
                    && old_shape.baseline_offset == new_shape.baseline_offset
            }
            (
                Ruby {
                    base: old_base,
                    text: old_text,
                    style: old_style,
                },
                Ruby {
                    base: new_base,
                    text: new_text,
                    style: new_style,
                },
            ) => {
                old_style.layout_eq(new_style)
                    && old_base.len() == new_base.len()
                    && old_text.len() == new_text.len()
                    && old_base
                        .iter()
                        .zip(new_base.iter())
                        .all(|(o, n)| Self::inline_content_layout_eq(o, n))
                    && old_text
                        .iter()
                        .zip(new_text.iter())
                        .all(|(o, n)| Self::inline_content_layout_eq(o, n))
            }
            // Different variants cannot have same layout
            _ => false,
        }
    }
}

impl Default for TextShapingCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Key for caching the conversion from `InlineContent` to `LogicalItem`s.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct LogicalItemsKey<'a> {
    pub(crate) inline_content_hash: u64,
    pub(crate) default_font_size: u32,
    pub(crate) _marker: std::marker::PhantomData<&'a ()>,
}

/// Key for caching the Bidi reordering stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct VisualItemsKey {
    pub(crate) logical_items_id: CacheId,
    pub(crate) base_direction: BidiDirection,
}

/// Key for caching the shaping stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct ShapedItemsKey {
    pub(crate) visual_items_id: CacheId,
    pub(crate) style_hash: u64,
}

impl ShapedItemsKey {
    pub(crate) fn new(visual_items_id: CacheId, visual_items: &[VisualItem]) -> Self {
        let style_hash = {
            let mut hasher = DefaultHasher::new();
            for item in visual_items {
                // Hash the style from the logical source, as this is what determines the font.
                match &item.logical_source {
                    LogicalItem::Text { style, .. } | LogicalItem::CombinedText { style, .. } => {
                        style.as_ref().hash(&mut hasher);
                    }
                    _ => {}
                }
            }
            hasher.finish()
        };

        Self {
            visual_items_id,
            style_hash,
        }
    }
}

/// Key for the final layout stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct LayoutKey {
    pub(crate) shaped_items_id: CacheId,
    pub(crate) constraints: UnifiedConstraints,
}

/// Helper to create a `CacheId` from any `Hash`able type.
pub(super) fn calculate_id<T: Hash>(item: &T) -> CacheId {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    hasher.finish()
}

// --- Main Layout Pipeline Implementation ---

impl TextShapingCache {
    /// New top-level entry point for flowing layout across multiple regions.
    ///
    /// This function orchestrates the entire layout pipeline, but instead of fitting
    /// content into a single set of constraints, it flows the content through an
    /// ordered sequence of `LayoutFragment`s.
    ///
    /// # CSS Inline Layout Module Level 3: Pipeline Implementation
    ///
    /// This implements the inline formatting context with 5 stages:
    ///
    /// ## Stage 1: Logical Analysis (`InlineContent` -> `LogicalItem`)
    /// \u2705 IMPLEMENTED: Parses raw content into logical units
    /// - Handles text runs, inline-blocks, replaced elements
    /// - Applies style overrides at character level
    /// - Implements \u00a7 2.2: Content size contribution calculation
    ///
    /// ## Stage 2: `BiDi` Reordering (`LogicalItem` -> `VisualItem`)
    /// \u2705 IMPLEMENTED: Uses CSS 'direction' property per CSS Writing Modes
    /// - Reorders items for right-to-left text (Arabic, Hebrew)
    /// - Respects containing block direction (not auto-detection)
    /// - Conforms to Unicode `BiDi` Algorithm (UAX #9)
    ///
    /// ## Stage 3: Shaping (`VisualItem` -> `ShapedItem`)
    /// \u2705 IMPLEMENTED: Converts text to glyphs
    /// - Uses `HarfBuzz` for OpenType shaping
    /// - Handles ligatures, kerning, contextual forms
    /// - Caches shaped results for performance
    ///
    /// ## Stage 4: Text Orientation Transformations
    /// \u26a0\ufe0f PARTIAL: Applies text-orientation for vertical text
    /// - Uses constraints from *first* fragment only
    /// - \u274c TODO: Should re-orient if fragments have different writing modes
    ///
    /// ## Stage 5: Flow Loop (`ShapedItem` -> `PositionedItem`)
    /// \u2705 IMPLEMENTED: Breaks lines and positions content
    /// - Calls `perform_fragment_layout` for each fragment
    /// - Uses `BreakCursor` to flow content across fragments
    /// - Implements \u00a7 5: Line breaking and hyphenation
    ///
    /// # Missing Features from CSS Inline-3:
    /// - \u00a7 3.3: initial-letter (drop caps)
    /// - \u00a7 4: vertical-align (only baseline supported)
    /// - \u00a7 6: text-box-trim (leading trim)
    /// - \u00a7 7: inline-sizing (aspect-ratio for inline-blocks)
    ///
    /// # Arguments
    /// * `content` - The raw `InlineContent` to be laid out.
    /// * `style_overrides` - Character-level style changes.
    /// * `flow_chain` - An ordered slice of `LayoutFragment` defining the regions (e.g., columns,
    ///   pages) that the content should flow through.
    /// * `font_chain_cache` - Pre-resolved font chains (from `FontManager.font_chain_cache`)
    /// * `fc_cache` - The fontconfig cache for font lookups
    /// * `loaded_fonts` - Pre-loaded fonts, keyed by `FontId`
    ///
    /// # Returns
    /// A `FlowLayout` struct containing the positioned items for each fragment that
    /// was filled, and any content that did not fit in the final fragment.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    /// # Panics
    ///
    /// Panics if bidi reordering of the logical items fails (an internal invariant).
    /// # Errors
    ///
    /// Returns a `LayoutError` if text flow layout fails.
    pub fn layout_flow<T: ParsedFontTrait>(
        &mut self,
        content: &[InlineContent],
        style_overrides: &[StyleOverride],
        flow_chain: &[LayoutFragment],
        font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
        fc_cache: &FcFontCache,
        loaded_fonts: &LoadedFonts<T>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<FlowLayout, LayoutError> {
        // [g150 az-web-lift DIAG] content data ptr (0x60BD0) + len (0x60BD4) at layout_flow ENTRY.
        #[cfg(feature = "web_lift")]
        unsafe {
            crate::az_mark((0x60BD0) as u32, (content.as_ptr() as usize as u32) as u32);
            crate::az_mark((0x60BD4) as u32, (content.len() as u32 | 0xC0DE0000) as u32);
        }
        // [g218 2026-06-09] The g158 `content.len()` force-materialize (a volatile read of
        // content+16) is DELETED: the within-fn SROA-to-0 of content.len() it worked around
        // is now fixed (NEON-decoder + volatile-guest-load transpiler work). VERIFIED:
        // hello-world lays out without it — counter "5" (label_wrapper 8,16,784,40) +
        // button shape correctly, same rects as before. (The cross-FN Vec-*return*-
        // len mis-lift is a separate, still-present issue handled by the g127/g129/g130 out-param
        // hacks — see g134 marker: callee content.len=1 but the caller's return-read sees
        // 0.) --- Stages 1-3: Preparation ---
        // These stages are independent of the final geometry. We perform them once
        // on the entire content block before flowing. Caching is used at each stage.

        let _probe_flow = crate::probe::Probe::span("text_layout_flow");
        // Cap per-item shaped cache to prevent unbounded growth.
        // When threshold is exceeded, evict entries not accessed this generation.
        const PER_ITEM_CACHE_MAX: usize = 4096;
        // The stage caches need a trigger too — they were the unbounded
        // ones. A document's worth of distinct runs is ~800 entries, so
        // 4096 leaves several documents' worth resident before any sweep.
        const STAGE_CACHE_MAX: usize = 4096;
        let (l, v, sh) = self.stage_entry_counts();
        if self.per_item_shaped.len() > PER_ITEM_CACHE_MAX || l.max(v).max(sh) > STAGE_CACHE_MAX {
            self.begin_generation();
        }

        // Stage 1: Logical Analysis (InlineContent -> LogicalItem)
        // [g213 2026-06-09] The web lift uses the real `self.logical_items` HashMap cache (NO
        // bypass). This entry() find-probe USED to spin forever on the lift (g178-g210
        // mis-diagnosed it many ways). TRUE root cause: hashbrown's portable WIDTH=8
        // `Group::static_empty()` — `[0xFF; 8]` in libazul's `__TEXT.__const` — was not
        // mirrored into the wasm, so the empty-map ctrl-scan read 0x00, looked
        // ALL-FULL (EMPTY=0xFF), and the probe never terminated. FIXED entirely transpiler-side in
        // `dll/src/web/symbol_table.rs::compute_hashbrown_empty_group_ranges` (signature-scans
        // `__const` for >=8-byte 8-aligned 0xFF runs and mirrors them). Verified:
        // web-nested-text lays out ("Hello" at 8,16,800,20), __remill_error=0. No
        // azul-source workaround needed here.
        let logical_items_id = calculate_id(&content);
        self.touch_stage(logical_items_id);
        let logical_items = self
            .logical_items
            .entry(logical_items_id)
            .or_insert_with(|| {
                Arc::new(create_logical_items(
                    content,
                    style_overrides,
                    debug_messages,
                ))
            })
            .clone();

        // Get the first fragment's constraints to extract the CSS direction property.
        // This is used for BiDi reordering in Stage 2.
        let default_constraints = UnifiedConstraints::default();
        let first_constraints = flow_chain
            .first()
            .map_or(&default_constraints, |f| &f.constraints);

        // +spec:containing-block:e7a271 - paragraph embedding level set from containing block's
        // 'direction' property +spec:display-property:7665cb - inline boxes split into
        // multiple visual runs due to bidi text processing +spec:display-property:929d6b -
        // applies Unicode bidi algorithm to inline-level box sequences
        // +spec:display-property:e8584a - Apply Unicode bidi algorithm to inline-level box
        // sequences per CSS Writing Modes §2.4 Stage 2: Bidi Reordering (LogicalItem ->
        // VisualItem) +spec:containing-block:961e3c - bidi paragraph level from containing
        // block direction, not UAX9 heuristic +spec:writing-modes:0a5368 - unicode-bidi:
        // plaintext auto-detects direction from text content Per CSS Writing Modes §8.3:
        // when unicode-bidi is plaintext, the paragraph's base direction is determined from
        // text content (first strong character), ignoring the containing block's direction
        // property. Empty paragraphs fall back to the containing block's direction.
        let unicode_bidi_val = first_constraints.unicode_bidi;
        let base_direction = if unicode_bidi_val == UnicodeBidi::Plaintext {
            // Auto-detect from text content; fall back to containing block direction
            let has_strong = logical_items.iter().any(|item| {
                if let LogicalItem::Text { text, .. } = item {
                    matches!(
                        unicode_bidi::get_base_direction(&**text),
                        unicode_bidi::Direction::Ltr | unicode_bidi::Direction::Rtl
                    )
                } else {
                    false
                }
            });
            if has_strong {
                get_base_direction_from_logical(&logical_items)
            } else {
                // Empty paragraph: use containing block's direction
                first_constraints.direction.unwrap_or(BidiDirection::Ltr)
            }
        } else {
            // Normal case: use CSS direction property
            first_constraints.direction.unwrap_or(BidiDirection::Ltr)
        };
        let visual_key = VisualItemsKey {
            logical_items_id,
            base_direction,
        };
        let visual_items_id = calculate_id(&visual_key);
        self.touch_stage(visual_items_id);
        // [g213] web lift uses the real visual_items HashMap cache (g180 bypass deleted; WIDTH=8
        // EMPTY_GROUP now mirrored — see Stage-1 note + symbol_table.rs).
        let visual_items = self
            .visual_items
            .entry(visual_items_id)
            .or_insert_with(|| {
                Arc::new(
                    reorder_logical_items(
                        &logical_items,
                        base_direction,
                        unicode_bidi_val,
                        debug_messages,
                    )
                    .unwrap(),
                )
            })
            .clone();

        // Stage 3: Shaping (VisualItem -> ShapedItem)
        // Two-level cache: monolithic (fast path) + per-item (incremental path).
        let _probe_shape = crate::probe::Probe::span("text_shape_stage");
        // (d7) Per-item assembly every pass — the monolithic map is gone
        // (see the field comment). Hits come from `per_item_shaped`.
        let shaped_items = Arc::new(shape_visual_items_with_per_item_cache(
            &visual_items,
            &mut self.per_item_shaped,
            &mut self.per_item_accessed,
            font_chain_cache,
            fc_cache,
            loaded_fonts,
            debug_messages,
        )?);

        // --- Stage 4: Apply Vertical Text Transformations ---

        // Note: first_constraints was already extracted above for BiDi reordering (Stage 2).
        // This orients all text based on the constraints of the *first* fragment.
        // A more advanced system could defer orientation until inside the loop if
        // fragments can have different writing modes.
        let oriented_items = apply_text_orientation(shaped_items, first_constraints);

        // --- Stage 5: The Flow Loop ---
        let mut fragment_layouts = HashMap::new();
        // The cursor now manages the stream of items for the entire flow.
        // §5.2 word-break: pass word_break from constraints to cursor
        let mut cursor =
            BreakCursor::with_word_break(&oriented_items, first_constraints.word_break);
        cursor.hyphens = first_constraints.hyphenation;
        cursor.line_break = first_constraints.line_break;

        // [g147 az-web-lift] Hard safety bound on the Stage-5 flow loop. On the remill lift this
        // `for fragment in flow_chain` (or the `cursor.is_done()` break) mis-lifts for the NESTED
        // IFC and iterates without terminating → solveLayoutReal HANGS (fuel trap in
        // layout_flow). The text is fully laid out on the first iteration(s); cap the
        // iterations so the loop always converges. (native is unaffected — the cap is far
        // above any real fragment count.)
        #[allow(clippy::no_effect_underscore_binding)] // web_lift-gated debug iteration counter
        let mut _az_flow_iters: usize = 0;
        let _probe_break = crate::probe::Probe::span("text_line_break");
        for fragment in flow_chain {
            #[cfg(feature = "web_lift")]
            {
                _az_flow_iters += 1;
                unsafe {
                    crate::az_mark(
                        (0x60BC0) as u32,
                        (_az_flow_iters as u32 | 0xC0DE0000) as u32,
                    );
                }
                if _az_flow_iters > 256 {
                    break;
                }
            }
            // Perform layout for this single fragment, consuming items from the cursor.
            let fragment_layout = perform_fragment_layout(
                &mut cursor,
                &logical_items,
                &fragment.constraints,
                debug_messages,
                loaded_fonts,
            )?;

            fragment_layouts.insert(fragment.id.clone(), Arc::new(fragment_layout));
            if cursor.is_done() {
                break; // All content has been laid out.
            }
        }

        if env_flag!("TEXTDBG") {
            let total_items: usize = fragment_layouts.values().map(|f| f.items.len()).sum();
            let text_preview: String = content
                .iter()
                .filter_map(|c| match c {
                    InlineContent::Text(r) => Some(r.text.chars().take(24).collect::<String>()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("|");
            eprintln!(
                "[TEXTDBG] layout_flow: content={} logical={} shaped={} frags={} \
                 placed_items={total_items} avail_h={:?} text='{text_preview}'",
                content.len(),
                logical_items.len(),
                oriented_items.len(),
                fragment_layouts.len(),
                first_constraints.available_height,
            );
        }
        Ok(FlowLayout {
            fragment_layouts,
            remaining_items: cursor.drain_remaining(),
        })
    }

    /// Runs stages 1–4 of the layout pipeline (logical analysis, `BiDi`, shaping,
    /// text orientation) and derives min/max-content widths by scanning the
    /// resulting `ShapedItem`s directly — without running stage 5's line-breaking
    /// `BreakCursor` loop.
    ///
    /// Used by `calculate_ifc_root_intrinsic_sizes` to avoid the 24% CPU spent
    /// cloning `ShapedCluster`s inside `BreakCursor::peek_next_unit` on every
    /// sizing pass. Since stages 1–3 hit the same `per_item_shaped` cache as
    /// `layout_flow`, a subsequent `layout_flow` call for the same content at
    /// a real container width is a pure cache hit for the shaping work.
    ///
    /// The item walk uses the same break-opportunity predicate that the
    /// `BreakCursor` would — min-content accumulates advances between break
    /// opportunities and tracks the maximum; max-content is the sum of all
    /// advances (as if the flow were laid out on a single infinitely-wide line).
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    /// # Panics
    ///
    /// Panics if bidi reordering of the logical items fails (an internal invariant).
    /// # Errors
    ///
    /// Returns a `LayoutError` if measuring intrinsic widths fails.
    pub fn measure_intrinsic_widths<T: ParsedFontTrait>(
        &mut self,
        content: &[InlineContent],
        style_overrides: &[StyleOverride],
        constraints: &UnifiedConstraints,
        font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
        fc_cache: &FcFontCache,
        loaded_fonts: &LoadedFonts<T>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<IntrinsicTextSizes, LayoutError> {
        const PER_ITEM_CACHE_MAX: usize = 4096;
        // The stage caches need a trigger too — they were the unbounded
        // ones. A document's worth of distinct runs is ~800 entries, so
        // 4096 leaves several documents' worth resident before any sweep.
        const STAGE_CACHE_MAX: usize = 4096;
        let (l, v, sh) = self.stage_entry_counts();
        if self.per_item_shaped.len() > PER_ITEM_CACHE_MAX || l.max(v).max(sh) > STAGE_CACHE_MAX {
            self.begin_generation();
        }

        // Stage 1: Logical Analysis (cached, same as layout_flow — the historic web-lift
        // bypass here was rooted in the un-mirrored hashbrown EMPTY_GROUP, fixed transpiler-side
        // in symbol_table.rs::compute_hashbrown_empty_group_ranges).
        let logical_items_id = calculate_id(&content);
        self.touch_stage(logical_items_id);
        let logical_items = self
            .logical_items
            .entry(logical_items_id)
            .or_insert_with(|| {
                Arc::new(create_logical_items(
                    content,
                    style_overrides,
                    debug_messages,
                ))
            })
            .clone();

        // Stage 2: BiDi (same derivation as layout_flow)
        let unicode_bidi_val = constraints.unicode_bidi;
        let base_direction = if unicode_bidi_val == UnicodeBidi::Plaintext {
            let has_strong = logical_items.iter().any(|item| {
                if let LogicalItem::Text { text, .. } = item {
                    matches!(
                        unicode_bidi::get_base_direction(&**text),
                        unicode_bidi::Direction::Ltr | unicode_bidi::Direction::Rtl
                    )
                } else {
                    false
                }
            });
            if has_strong {
                get_base_direction_from_logical(&logical_items)
            } else {
                constraints.direction.unwrap_or(BidiDirection::Ltr)
            }
        } else {
            constraints.direction.unwrap_or(BidiDirection::Ltr)
        };
        let visual_key = VisualItemsKey {
            logical_items_id,
            base_direction,
        };
        let visual_items_id = calculate_id(&visual_key);
        self.touch_stage(visual_items_id);
        let visual_items = self
            .visual_items
            .entry(visual_items_id)
            .or_insert_with(|| {
                Arc::new(
                    reorder_logical_items(
                        &logical_items,
                        base_direction,
                        unicode_bidi_val,
                        debug_messages,
                    )
                    .unwrap(),
                )
            })
            .clone();

        // Stage 3: Shaping (two-level cache, same as layout_flow)
        // (d7) Per-item assembly every pass (monolithic map deleted).
        let shaped_items = Arc::new(shape_visual_items_with_per_item_cache(
            &visual_items,
            &mut self.per_item_shaped,
            &mut self.per_item_accessed,
            font_chain_cache,
            fc_cache,
            loaded_fonts,
            debug_messages,
        )?);

        // Stage 4: Text orientation
        let oriented_items = apply_text_orientation(shaped_items, constraints);

        // Stage 5 bypass: scan items for min/max contributions.
        let word_break = constraints.word_break;
        let hyphens = constraints.hyphenation;
        let scan_is_vertical = constraints.is_vertical();

        let mut total = 0.0f32; // running width of the current line
        let mut max_line = 0.0f32; // widest line between forced breaks = max-content
        let mut max_word = 0.0f32;
        let mut cur_word = 0.0f32;
        let mut max_line_height = 0.0f32;

        // CSS Text 3 4.1.2 (white-space Phase II): in the collapsing modes the
        // spaces at a line's start and end are removed - the line breaker
        // strips them (`break_one_line`'s strip_leading / strip_trailing), so
        // the max-content does not hold them either: a leading one is never
        // folded, and a line measures to its last item that is not one
        // (`line_content`). `<td> text </td>` was two spaces wider than its
        // text, and centred content sat off-centre in it (MAILREF8 group F).
        let collapsing = matches!(
            constraints.white_space_mode,
            WhiteSpaceMode::Normal | WhiteSpaceMode::Nowrap | WhiteSpaceMode::PreLine
        );
        let mut line_has_content = false;
        let mut line_content = 0.0f32;
        let line_width =
            |total: f32, line_content: f32| if collapsing { line_content } else { total };

        // `text-indent` counts in the intrinsic sizes (CSS Text 3 8.1; the
        // caller passes a percentage as 0): a line box is narrower by its indent
        // (`text_indent_of_line`), so a box sized from these widths must hold
        // indent + content. A line between forced breaks takes its own indent
        // (max-content); the FIRST word of such a line takes it too, every later
        // word may start a soft-wrapped line and takes that line's (min-content).
        let soft_wrap_indent = text_indent_of_line(constraints, false, false);
        let mut forced_lines = 0usize;
        let mut line_indent = text_indent_of_line(constraints, true, false);
        let mut word_indent = line_indent;

        let scan_items: &[ShapedItem] = &oriented_items;
        for (item_idx, item) in scan_items.iter().enumerate() {
            // A forced break (preserved LF, <br>) ends the current line. max-content
            // is the widest line BETWEEN forced breaks, not the running sum across
            // them — otherwise a white-space:pre block with newlines (or any <br>
            // content) over-measures its max-content as the concatenation of all
            // lines. Reset the line accumulators here.
            if let ShapedItem::Break { .. } = item {
                let width = line_width(total, line_content);
                if width + line_indent > max_line {
                    max_line = width + line_indent;
                }
                if cur_word > 0.0 && cur_word + word_indent > max_word {
                    max_word = cur_word + word_indent;
                }
                total = 0.0;
                line_content = 0.0;
                line_has_content = false;
                cur_word = 0.0;
                forced_lines += 1;
                line_indent = text_indent_of_line(constraints, false, forced_lines > 0);
                word_indent = line_indent;
                continue;
            }
            // The scan MUST fold the same per-item measure, in the same order,
            // onto the same running total as the line breaker - shared via
            // fold_line_width / get_item_measure_with_spacing (kerning,
            // letter-spacing, word-spacing included; see that function's doc
            // for why any other grouping re-introduces the one-word-wrap bug).
            // An inline box's start / end margin + border + padding are on the
            // line too, where `position_one_line` puts them (the shared
            // `inline_box_edge_advances`): a line never strips them, and they
            // stick to the box's first / last word. Zero for unboxed text, so
            // the shared fold is untouched there.
            let (edge_start, edge_end) = inline_box_edge_advances(scan_items, item_idx);
            let edges = edge_start + edge_end;
            // An outside marker hangs in the gutter: no width and no part
            // of a word (`is_hanging_marker`, the line breaker's rule; its
            // line height still counts below).
            let hanging = is_hanging_marker(item);
            let adv = if hanging {
                0.0
            } else {
                (get_item_measure_with_spacing(item, scan_is_vertical).max(0.0) + edges).max(0.0)
            };
            let removable_space = collapsing && is_collapsible_whitespace(item);
            if line_has_content || !removable_space {
                total = fold_line_width(total, item, scan_is_vertical);
            }
            let boxed = edges.abs() > 0.0;
            if boxed {
                total += edges;
            }
            if (!removable_space || boxed) && !hanging {
                line_has_content = true;
                line_content = total;
            }

            let (asc, desc) = get_item_vertical_metrics_approx(item);
            let h = (asc + desc).max(item.bounds().height);
            if h > max_line_height {
                max_line_height = h;
            }

            if is_break_opportunity_with_word_break(item, word_break, hyphens) {
                if cur_word > 0.0 {
                    if cur_word + word_indent > max_word {
                        max_word = cur_word + word_indent;
                    }
                    word_indent = soft_wrap_indent;
                }
                // A break opportunity that is itself a rendered unit (a CJK
                // ideograph in normal mode, or any cluster under break-all /
                // overflow-wrap:anywhere) still forms a minimal unbreakable unit
                // of its own advance; only true separators (spaces) contribute 0.
                // Without this, pure-CJK / break-all text measures min-content = 0
                // and the box collapses to zero inline width.
                if !is_word_separator(item) {
                    if adv + word_indent > max_word {
                        max_word = adv + word_indent;
                    }
                    word_indent = soft_wrap_indent;
                }
                cur_word = 0.0;
            } else {
                cur_word += adv;
            }
        }
        if cur_word > 0.0 && cur_word + word_indent > max_word {
            max_word = cur_word + word_indent;
        }
        // The last line: an indent only counts on a line that holds something
        // (an empty paragraph has no line box to indent).
        let width = line_width(total, line_content);
        if (width > 0.0 || forced_lines > 0) && width + line_indent > max_line {
            max_line = width + line_indent;
        }

        // white-space:nowrap forbids soft-wrap opportunities entirely, so the
        // min-content width equals the max-content width (one unbreakable line).
        // Without this the scan resets cur_word at each space and reports a
        // too-small min-content, letting flex/shrink-to-fit clip the text.
        let min_content_width = if matches!(constraints.white_space_mode, WhiteSpaceMode::Nowrap) {
            max_line
        } else {
            max_word
        };

        // CEIL to a fixed 1/64px sub-pixel grid before reporting - the same
        // reason browsers ceil preferred widths (Chromium's LayoutUnit):
        // an intrinsic width is a PROMISE that content of exactly this width
        // fits, but the scan and the breaker fold separately-shaped item
        // lists whose sums can differ by a few ULP (observed live: the
        // breaker's fold came out one bit ABOVE the reported max-content and
        // a shrink-to-fit "Decrease Indent" wrapped under Noto Sans, even
        // with both passes sharing fold_line_width). 1/64px is invisible on
        // screen and orders of magnitude above float noise; values already
        // on the grid (mock/test fonts with integral advances) round-trip
        // bit-identically through ceil.
        const SUBPIXEL_GRID: f32 = 64.0;
        let ceil_grid = |w: f32| -> f32 {
            if w.is_finite() && w > 0.0 {
                (w * SUBPIXEL_GRID).ceil() / SUBPIXEL_GRID
            } else {
                w
            }
        };

        Ok(IntrinsicTextSizes {
            min_content_width: ceil_grid(min_content_width),
            max_content_width: ceil_grid(max_line),
            max_content_height: max_line_height,
        })
    }
}
