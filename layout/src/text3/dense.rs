//! §3.2 DENSE TEXT MODEL — the compact-record types of
//! `scripts/SHAPED_TEXT_REFACTOR_PLAN.md`, introduced ALONGSIDE the
//! current `PositionedItem`/`ShapedCluster` model (campaign step 1).
//!
//! Nothing consumes these yet: this module stakes the types, their size
//! pins live in `tests/struct_sizes.rs`, and [`DenseText::from_unified`]
//! is the bridge that lets consumers migrate one at a time while the
//! equivalence test (`text3_dense_equivalence.rs`) proves the conversion
//! loses nothing the current model knows. The plan's own gates (T1
//! source-reproduction + id-integrity, T2 cache identity, the shaping
//! goldens) all pin the semantics this model must preserve.
//!
//! Per-record budget vs the current model, from the plan's §3.5/§3.6
//! arithmetic: `ClusterCompact` is 16 B against today's ~200 B
//! `PositionedItem` chain, with run-level data amortised over ~42
//! clusters/run and per-glyph detail only where ligatures / marks /
//! GPOS offsets actually occur.

use alloc::sync::Arc;
use alloc::vec::Vec;

use super::cache::{
    ClusterFlags, LayoutFontMetrics, PositionedItem, ShapedItem, StyleProperties, UnifiedLayout,
};
use crate::text3::script::Script;

/// One per shaped cluster. Dense, POD, no Drop glue, no owned heap.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ClusterCompact {
    /// Glyph to draw when the cluster has no detail entry.
    pub glyph_id: u16,
    /// Precomputed classification — the same word the retained cluster
    /// carries since 48c9bbcdf.
    pub flags: ClusterFlags,
    /// Total advance INCLUDING kerning, as painted.
    pub advance: f32,
    /// == `GraphemeClusterId::start_byte_in_run`; the run supplies
    /// `source_run`, so the id reconstructs exactly.
    pub start_byte: u32,
    /// Inline-axis position within the IFC; `y` comes from the line.
    pub x: f32,
}

/// One per shaped RUN (~one per 42 clusters on the measured corpus):
/// everything that is uniform across a run, amortised.
#[derive(Debug, Clone)]
pub struct DenseRun {
    pub style: Arc<StyleProperties>,
    pub font_hash: u64,
    /// ONE copy per run (today: one 32-B copy per GLYPH).
    pub font_metrics: LayoutFontMetrics,
    pub source_run: u32,
    /// Dense index into the DOM node table; `u32::MAX` = none.
    pub source_node: u32,
    /// The run's source text, shared — the single copy that replaces
    /// every per-cluster `String`.
    pub text: Arc<str>,
    /// Range into [`DenseText::clusters`].
    pub clusters: core::ops::Range<u32>,
    pub script: Script,
}

/// One per line: replaces per-cluster `line_index` + `position.y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineRecord {
    pub clusters: (u32, u32),
    pub baseline_y: f32,
    pub top_y: f32,
    pub height: f32,
}

/// Detail entry for clusters that need more than one glyph or non-zero
/// GPOS offsets (ligatures, combining marks, kashida).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClusterDetail {
    pub cluster: u32,
    pub glyphs: (u32, u32),
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct DetailGlyph {
    pub glyph_id: u16,
    pub cluster_offset: u16,
    /// Advance incl. kerning.
    pub advance: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

/// The dense view of a laid-out IFC. Step 1: derived FROM the current
/// `UnifiedLayout`; later steps make this the source of truth.
#[derive(Debug, Clone, Default)]
pub struct DenseText {
    pub clusters: Vec<ClusterCompact>,
    pub runs: Vec<DenseRun>,
    pub lines: Vec<LineRecord>,
    pub details: Vec<ClusterDetail>,
    pub detail_glyphs: Vec<DetailGlyph>,
}

impl Default for LineRecord {
    fn default() -> Self {
        Self { clusters: (0, 0), baseline_y: 0.0, top_y: 0.0, height: 0.0 }
    }
}

impl DenseText {
    /// Build the dense view from the current model. Clusters keep their
    /// item order; runs split where (style Arc identity, font_hash of the
    /// first glyph, source_run, source_node) change; lines come from the
    /// items' `line_index`. Non-cluster items (objects, breaks, combined
    /// blocks, tabs) are SKIPPED here — they stay on the sparse side per
    /// the plan's `AtomicItem` design and migrate in a later step.
    #[must_use]
    pub fn from_unified(layout: &UnifiedLayout) -> Self {
        Self::from_unified_with_content(layout, &[])
    }

    /// Like [`Self::from_unified`], but with the source content available:
    /// each `DenseRun.text` becomes an `Arc` CLONE of its `StyledRun`'s
    /// text (§3.2 step 2) — the true shared source — instead of the
    /// step-1 concatenation of surviving clusters.
    #[must_use]
    pub fn from_unified_with_content(
        layout: &UnifiedLayout,
        content: &[super::cache::InlineContent],
    ) -> Self {
        let mut dense = Self::default();
        let mut current_run: Option<DenseRun> = None;
        let mut current_line: Option<(usize, LineRecord)> = None;

        for item in &layout.items {
            let PositionedItem { item: shaped, position, line_index } = item;
            let ShapedItem::Cluster(c) = shaped else {
                continue;
            };
            let first_glyph = c.glyphs.first();
            let font_hash = first_glyph.map_or(0, |g| g.font_hash);
            let font_metrics = first_glyph.map_or(
                LayoutFontMetrics {
                    ascent: 0.0,
                    descent: 0.0,
                    cap_height: None,
                    x_height: None,
                    line_gap: 0.0,
                    units_per_em: 0,
                },
                |g| g.font_metrics,
            );
            let source_node = c
                .source_node_id
                .map_or(u32::MAX, |n| u32::try_from(n.index()).unwrap_or(u32::MAX));
            let script = first_glyph.map_or(Script::Latin, |g| g.script);
            let cluster_index = u32::try_from(dense.clusters.len()).unwrap_or(u32::MAX);

            // Run split on any amortised-field change.
            let split = match &current_run {
                None => true,
                Some(r) => {
                    !Arc::ptr_eq(&r.style, &c.style)
                        || r.font_hash != font_hash
                        || r.source_run != c.source_cluster_id.source_run
                        || r.source_node != source_node
                }
            };
            if split {
                if let Some(mut r) = current_run.take() {
                    r.clusters.end = cluster_index;
                    dense.runs.push(r);
                }
                current_run = Some(DenseRun {
                    style: c.style.clone(),
                    font_hash,
                    font_metrics,
                    source_run: c.source_cluster_id.source_run,
                    source_node,
                    // Step 1 carries the run text as the concatenation of
                    // its SURVIVING clusters' texts — the line breaker
                    // consumes separator clusters at wrap points, so this
                    // is NOT the full source text (the equivalence gate
                    // proved that on its first wrapped case). Later steps
                    // replace this with the true shared source Arc, which
                    // is precisely why the plan's run.text design exists.
                    text: Arc::from(""),
                    clusters: cluster_index..cluster_index,
                    script,
                });
            }
            if let Some(r) = &mut current_run {
                if r.text.is_empty() {
                    // The true shared source: the StyledRun's Arc, aliased.
                    if let Some(super::cache::InlineContent::Text(sr)) =
                        content.get(r.source_run as usize)
                    {
                        r.text = sr.text.clone();
                    }
                }
                if r.text.is_empty() {
                    // No content available (plain from_unified): fall back
                    // to concatenating surviving clusters, as in step 1.
                    let mut t = String::from(&*r.text);
                    t.push_str(&c.text);
                    r.text = Arc::from(t.as_str());
                }
            }

            // Line records from line_index transitions.
            match &mut current_line {
                Some((idx, rec)) if *idx == *line_index => {
                    rec.clusters.1 = cluster_index + 1;
                }
                _ => {
                    if let Some((_, rec)) = current_line.take() {
                        dense.lines.push(rec);
                    }
                    current_line = Some((
                        *line_index,
                        LineRecord {
                            clusters: (cluster_index, cluster_index + 1),
                            baseline_y: position.y,
                            top_y: position.y,
                            height: 0.0,
                        },
                    ));
                }
            }

            // Detail side table for multi-glyph / offset clusters.
            let needs_detail = c.glyphs.len() != 1
                || c.glyphs
                    .first()
                    .is_some_and(|g| g.offset.x != 0.0 || g.offset.y != 0.0 || g.kerning != 0.0);
            if needs_detail {
                let start = u32::try_from(dense.detail_glyphs.len()).unwrap_or(u32::MAX);
                for g in c.glyphs.iter() {
                    dense.detail_glyphs.push(DetailGlyph {
                        glyph_id: g.glyph_id,
                        cluster_offset: u16::try_from(g.cluster_offset).unwrap_or(u16::MAX),
                        advance: g.advance + g.kerning,
                        offset_x: g.offset.x,
                        offset_y: g.offset.y,
                    });
                }
                let end = u32::try_from(dense.detail_glyphs.len()).unwrap_or(u32::MAX);
                dense.details.push(ClusterDetail { cluster: cluster_index, glyphs: (start, end) });
            }

            dense.clusters.push(ClusterCompact {
                glyph_id: first_glyph.map_or(0, |g| g.glyph_id),
                flags: c.flags,
                advance: c.advance
                    + c.glyphs.iter().map(|g| g.kerning).sum::<f32>(),
                start_byte: c.source_cluster_id.start_byte_in_run,
                x: position.x,
            });
        }
        if let Some(mut r) = current_run.take() {
            r.clusters.end = u32::try_from(dense.clusters.len()).unwrap_or(u32::MAX);
            dense.runs.push(r);
        }
        if let Some((_, rec)) = current_line.take() {
            dense.lines.push(rec);
        }
        dense
    }
}
