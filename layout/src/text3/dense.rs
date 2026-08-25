//! DENSE TEXT MODEL — compact-record text types, introduced ALONGSIDE the
//! current `PositionedItem`/`ShapedCluster` model rather than replacing it.
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

use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalSize},
    ui_solver::GlyphInstance,
};

use super::cache::{
    BidiDirection, ClusterFlags, LayoutFontMetrics, LoadedFonts, ParsedFontTrait, PositionedItem,
    ShapedItem, StyleProperties, UnifiedLayout,
};
use super::glyphs::{PdfGlyphRun, PdfPositionedGlyph, PositionedGlyph, SimpleGlyphRun};
use super::cache::Point;
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
    /// The cluster's BASE advance — equal to the sparse cluster's
    /// `advance` and to `ShapedItem::bounds().width` (d2 redefinition;
    /// was kerning-folded). Sound because a kerned cluster ALWAYS has a
    /// detail entry (`needs_detail` includes kerning != 0), and every
    /// walker derives detail-cluster pens from `DetailGlyph.advance`
    /// (kerning-folded there), never from this field.
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
    /// (d6h) Reconstructs a cluster's `source_content_index.item_index`:
    /// `item_base + start_byte` when [`Self::item_linear`], else
    /// `item_base` verbatim. (0/linear for plain runs; the segment offset
    /// for override-segmented runs, closing the §10 "item-index-blind"
    /// gap.)
    pub item_base: u32,
    /// (#25b) Which item-index model this run follows. `true` = the
    /// linear post-layout model (`item_base + start_byte`); `false` =
    /// CONSTANT `item_base` for every cluster — the shape produced by
    /// paths that never restamp `item_index` per cluster. Before this
    /// flag, a constant-index run DEGENERATED into one run per cluster
    /// (the linear delta changed every cluster), ~25 KB/IFC of `DenseRun`
    /// headers for zero information.
    pub item_linear: bool,
    pub script: Script,
    /// Bidi direction — uniform per run BY CONSTRUCTION (the builder
    /// splits on it): mixed-bidi text in one styled run must not share a
    /// dense run, both for the PDF walker's run predicate and for RTL
    /// border-fragment assignment.
    pub direction: BidiDirection,
    /// The item's own solved block-axis position (`PositionedItem.position.y`),
    /// amortised per run because the builder splits on it.
    ///
    /// This exists because "y comes from the line" is FALSE. `LineRecord`
    /// stores one y, frozen from whichever cluster opened the line, and every
    /// walker that reconstructed a baseline as `line.top_y + ascent` silently
    /// assumed every cluster on the line shared that y. A line mixing font
    /// sizes breaks it: the taller run sits on a different baseline, and its
    /// glyphs were emitted 12.8px off at 16px/32px (and 1.98px off on the
    /// real-world mixed-font line that exposed this). The sparse reference
    /// always used the ITEM's own `position.y`; this is that value, kept at
    /// run granularity so the amortisation still holds — runs are ~1 per 42
    /// clusters, and y changes exactly where the run already splits (style,
    /// font metrics), so in practice this costs no extra runs at all.
    pub y: f32,
}

/// One per line: replaces per-cluster `line_index` + `position.y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineRecord {
    pub clusters: (u32, u32),
    pub baseline_y: f32,
    pub top_y: f32,
    pub height: f32,
    /// The source `PositionedItem::line_index` — kept because record
    /// ORDINALS diverge from it when a line carries no clusters (only
    /// objects), and the PDF walker reports/breaks on the source index.
    pub source_index: u32,
}

/// Detail entry for clusters that need more than one glyph or non-zero
/// GPOS offsets (ligatures, combining marks, kashida).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterDetail {
    pub cluster: u32,
    pub glyphs: (u32, u32),
    /// (d4) The cluster's SOURCE byte length. Needed because a
    /// ligature-fused cluster spans multiple graphemes, so
    /// "next grapheme boundary" under-measures it; simple clusters
    /// reconstruct their length grapheme-exactly (T1) and stay 16 B.
    pub byte_len: u32,
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
    /// (d6h) The kerning HALF of `advance` — walkers keep consuming the
    /// folded `advance`; the sparse expander un-folds via this.
    pub kerning: f32,
    /// (d6h) `ShapedGlyph::kind` — non-Character kinds (hyphen …) force
    /// a detail entry so the expander can reproduce them.
    pub kind: super::cache::GlyphKind,
    /// (d6h) Vertical-text pair, exact-roundtrip completeness.
    pub vertical_advance: f32,
    pub vertical_offset_x: f32,
    pub vertical_offset_y: f32,
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
        Self { clusters: (0, 0), baseline_y: 0.0, top_y: 0.0, height: 0.0, source_index: 0 }
    }
}

/// (#25b) Finalize a closing run's item-index model from the surviving
/// viability bases. Linear preferred when both hold (single-cluster runs
/// then keep the pre-#25b encoding byte-for-byte). At least one base is
/// always `Some` — the split above opens a fresh run (both seeded) the
/// moment neither model fits.
fn close_item_model(r: &mut DenseRun, linear: Option<u32>, constant: Option<u32>) {
    match (linear, constant) {
        (Some(b), _) => {
            r.item_base = b;
            r.item_linear = true;
        }
        (None, Some(b)) => {
            r.item_base = b;
            r.item_linear = false;
        }
        (None, None) => {
            debug_assert!(false, "a run closed with no surviving item model");
            r.item_base = 0;
            r.item_linear = true;
        }
    }
}

impl DenseText {
    /// Build the dense view from the current model. Clusters keep their
    /// item order; runs split where (style Arc identity, `font_hash` of the
    /// first glyph, `source_run`, `source_node`) change; lines come from the
    /// items' `line_index`. Non-cluster items (objects, breaks, combined
    /// blocks, tabs) are SKIPPED here — they stay on the sparse side per
    /// the plan's `AtomicItem` design and migrate in a later step.
    #[must_use]
    pub fn from_unified(layout: &UnifiedLayout) -> Self {
        Self::from_unified_with_content(layout, &[])
    }

    /// Identical to [`Self::from_unified`] — the `content` parameter is
    /// IGNORED since 3c: every cluster carries its logical item's shared
    /// text Arc (`ShapedCluster::source_text`), which is the text its
    /// `start_byte` actually indexes (correct for override-segmented
    /// runs, where the old `content.get(source_run)` mapping was not).
    /// Kept for signature compatibility with the gate tests.
    #[must_use]
    pub fn from_unified_with_content(
        layout: &UnifiedLayout,
        content: &[super::cache::InlineContent],
    ) -> Self {
        let _ = content;
        let mut dense = Self::default();
        let mut current_run: Option<DenseRun> = None;
        let mut current_line: Option<(usize, LineRecord)> = None;
        // The current run's resolved line height — the d3 fill for
        // `LineRecord.height` (max over the line's clusters).
        let mut current_run_lh = 0.0f32;
        // (#25b) The open run's still-viable item-index models: LINEAR
        // base (item_index − start_byte of the seed cluster) and CONSTANT
        // base (the seed's item_index). `None` = that model already broke
        // mid-run. Finalized into the run at close by `close_item_model`.
        let mut item_run_linear_base: Option<u32> = None;
        let mut item_run_const_base: Option<u32> = None;

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

            // Run split on any amortised-field change. The source-text
            // Arc identity is part of the predicate since 3c: clusters
            // carry their LOGICAL ITEM's shared Arc (offset-correct for
            // override-segmented runs, where the old content.get(run)
            // mapping was wrong — §10 finding 1), and one item's clusters
            // all share one Arc by construction.
            // (d6h/#25b) Item-index model viability for the OPEN run.
            // Two models can reconstruct item_index: LINEAR
            // (item_base + start_byte — the post-layout restamped shape)
            // and CONSTANT (item_base verbatim — paths that never restamp
            // per cluster). The run stays open while EITHER still holds;
            // close picks linear when both do (single-cluster runs keep
            // today's encoding). Before this, a constant-index run split
            // on EVERY cluster (the linear delta changed each time) —
            // one DenseRun header per cluster for zero information.
            // wrapping arithmetic keeps a malformed (item_index <
            // start_byte) pair from panicking — the split still isolates
            // it when neither model fits.
            let item_index = c.source_content_index.item_index;
            let start_byte = c.source_cluster_id.start_byte_in_run;
            let fits_linear = item_run_linear_base
                .is_some_and(|b| item_index == b.wrapping_add(start_byte));
            let fits_const = item_run_const_base.is_some_and(|b| item_index == b);
            let split = match &current_run {
                None => true,
                Some(r) => {
                    !Arc::ptr_eq(&r.style, &c.style)
                        || !Arc::ptr_eq(&r.text, &c.source_text)
                        || r.font_hash != font_hash
                        || r.source_run != c.source_cluster_id.source_run
                        || r.source_node != source_node
                        || r.direction != c.direction
                        // The item's own y is amortised on the run, so a
                        // change in it MUST open a new run — otherwise the
                        // run's y would silently describe only its first
                        // cluster, which is the per-line freeze one level down.
                        || (r.y - position.y).abs() > 0.001
                        || (!fits_linear && !fits_const)
                }
            };
            if split {
                if let Some(mut r) = current_run.take() {
                    r.clusters.end = cluster_index;
                    close_item_model(
                        &mut r,
                        item_run_linear_base,
                        item_run_const_base,
                    );
                    dense.runs.push(r);
                }
                // Fresh run: both models start viable, seeded from this
                // cluster.
                item_run_linear_base = Some(item_index.wrapping_sub(start_byte));
                item_run_const_base = Some(item_index);
                current_run_lh = if font_metrics.units_per_em == 0 {
                    0.0
                } else {
                    c.style
                        .line_height
                        .resolve_with_metrics(c.style.font_size_px, &font_metrics)
                };
                current_run = Some(DenseRun {
                    style: c.style.clone(),
                    font_hash,
                    font_metrics,
                    source_run: c.source_cluster_id.source_run,
                    source_node,
                    // The cluster's own shared source Arc (3c) — the text
                    // `ClusterCompact.start_byte` actually indexes into,
                    // for EVERY case including override segments.
                    text: c.source_text.clone(),
                    clusters: cluster_index..cluster_index,
                    // Placeholders — `close_item_model` writes the real
                    // values from the surviving model at run close.
                    item_base: 0,
                    item_linear: true,
                    script,
                    direction: c.direction,
                    y: position.y,
                });
            } else {
                // Staying in the run: a cluster that fits only one model
                // permanently kills the other (viability is monotonic —
                // a broken model cannot come back later in the run).
                if !fits_linear {
                    item_run_linear_base = None;
                }
                if !fits_const {
                    item_run_const_base = None;
                }
            }

            // Line records from line_index transitions.
            match &mut current_line {
                Some((idx, rec)) if *idx == *line_index => {
                    rec.clusters.1 = cluster_index + 1;
                    rec.height = rec.height.max(current_run_lh);
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
                            // d3: filled with the max resolved line height of
                            // the line's clusters (was always 0.0).
                            height: current_run_lh,
                            source_index: u32::try_from(*line_index).unwrap_or(u32::MAX),
                        },
                    ));
                }
            }

            // Detail side table for multi-glyph / offset clusters —
            // (d6h) non-Character kinds and vertical metrics also force
            // an entry so the sparse expander loses nothing.
            //
            // A cluster without a detail entry is reconstructed ENTIRELY from
            // the compact record, and the compact record has no byte length —
            // `cluster_byte_len` falls back to "the next grapheme at
            // start_byte". So a detail is also required whenever the cluster's
            // true `source_byte_len` is NOT that grapheme length. The case
            // that makes this reachable is a LIGATURE: "fi" fused into
            // exactly one glyph with no offsets, no kerning, kind Character —
            // satisfying none of the other clauses — while spanning TWO
            // graphemes. Without this clause every such cluster silently
            // shrank to its first grapheme on the way through the dense
            // model, and pdftotext read "Confgure" out of documents that
            // said "Configure". `ShapedCluster::source_byte_len`'s doc states
            // the invariant: "Stored, not re-derived: ligature-fused clusters
            // span MULTIPLE graphemes, so 'next grapheme boundary' cannot
            // reconstruct the slice in general." This mirrors the READ-side
            // fallback exactly, so predicate and fallback cannot disagree.
            let compact_len_reconstructible = {
                use unicode_segmentation::UnicodeSegmentation;
                let start = c.source_cluster_id.start_byte_in_run as usize;
                let grapheme_len = c
                    .source_text
                    .get(start..)
                    .and_then(|s| s.graphemes(true).next())
                    .map_or(0, str::len);
                usize::from(c.source_byte_len) == grapheme_len
            };
            let needs_detail = c.glyphs.len() != 1
                || !compact_len_reconstructible
                || c.glyphs.first().is_some_and(|g| {
                    g.offset.x != 0.0
                        || g.offset.y != 0.0
                        || g.kerning != 0.0
                        || g.kind != super::cache::GlyphKind::Character
                        || g.vertical_advance != 0.0
                        || g.vertical_offset.x != 0.0
                        || g.vertical_offset.y != 0.0
                });
            if needs_detail {
                let start = u32::try_from(dense.detail_glyphs.len()).unwrap_or(u32::MAX);
                for g in &c.glyphs {
                    dense.detail_glyphs.push(DetailGlyph {
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
                let end = u32::try_from(dense.detail_glyphs.len()).unwrap_or(u32::MAX);
                dense.details.push(ClusterDetail {
                    cluster: cluster_index,
                    glyphs: (start, end),
                    byte_len: u32::from(c.source_byte_len),
                });
            }

            // (d6h) Bits 7-10 pack the ShapedCluster fields the compact
            // record has no room for — DENSE-SIDE ONLY (classify() never
            // sets them; equivalence pins mask them off).
            let mut packed = c.flags.0;
            if c.is_first_fragment {
                packed |= ClusterFlags::DENSE_IS_FIRST_FRAGMENT;
            }
            if c.is_last_fragment {
                packed |= ClusterFlags::DENSE_IS_LAST_FRAGMENT;
            }
            if let Some(outside) = c.marker_position_outside {
                packed |= ClusterFlags::DENSE_MARKER_SOME;
                if outside {
                    packed |= ClusterFlags::DENSE_MARKER_OUTSIDE;
                }
            }
            dense.clusters.push(ClusterCompact {
                glyph_id: first_glyph.map_or(0, |g| g.glyph_id),
                flags: ClusterFlags(packed),
                advance: c.advance,
                start_byte: c.source_cluster_id.start_byte_in_run,
                x: position.x,
            });
        }
        if let Some(mut r) = current_run.take() {
            r.clusters.end = u32::try_from(dense.clusters.len()).unwrap_or(u32::MAX);
            close_item_model(&mut r, item_run_linear_base, item_run_const_base);
            dense.runs.push(r);
        }
        if let Some((_, rec)) = current_line.take() {
            dense.lines.push(rec);
        }
        dense
    }

    /// (d4) The source byte length of cluster `ci`: the detail table's
    /// stored length when present (ligature clusters span multiple
    /// graphemes), else the grapheme at `start_byte` in the run text —
    /// exact for simple clusters by T1.
    #[must_use]
    pub fn cluster_byte_len(&self, ci: u32) -> u32 {
        use unicode_segmentation::UnicodeSegmentation;
        if let Ok(i) = self.details.binary_search_by_key(&ci, |d| d.cluster) {
            return self.details[i].byte_len;
        }
        let c = &self.clusters[ci as usize];
        let run = self
            .runs
            .iter()
            .find(|r| r.clusters.contains(&ci))
            .expect("cluster belongs to a run by construction");
        run.text
            .get(c.start_byte as usize..)
            .and_then(|s| s.graphemes(true).next())
            .map_or(0, |g| g.len() as u32)
    }

    /// (d6b) The caret-stop list — the PRIMITIVE the whole cursor-movement
    /// library reduces to (left/right/home/end are offset arithmetic over
    /// it; selection ranges bound by it). Same output as the sparse
    /// `UnifiedLayout::grapheme_stops`: cluster ids sorted by
    /// (run, byte), deduped, grapheme-continuation clusters excluded —
    /// via the precomputed flag instead of the text probe (the flags are
    /// pinned equal to the sparse classification by the base gate).
    #[must_use]
    pub fn grapheme_stops(&self) -> Vec<azul_core::selection::GraphemeClusterId> {
        use azul_core::selection::GraphemeClusterId;
        let mut stops: Vec<GraphemeClusterId> = self
            .runs
            .iter()
            .flat_map(|r| (r.clusters.start..r.clusters.end).map(move |ci| (ci, r)))
            .filter(|(ci, _)| {
                !self.clusters[*ci as usize]
                    .flags
                    .has(ClusterFlags::GRAPHEME_CONTINUATION)
            })
            .map(|(ci, r)| GraphemeClusterId {
                source_run: r.source_run,
                start_byte_in_run: self.clusters[ci as usize].start_byte,
            })
            .collect();
        stops.sort_by_key(|id| (id.source_run, id.start_byte_in_run));
        stops.dedup();
        stops
    }

    /// (d6e) The cluster's source text slice (run text at `start_byte` for
    /// `cluster_byte_len` bytes) — the word-boundary predicate's input.
    fn cluster_text_slice(&self, ci: u32) -> &str {
        let c = &self.clusters[ci as usize];
        let run = self
            .runs
            .iter()
            .find(|r| r.clusters.contains(&ci))
            .expect("cluster belongs to a run by construction");
        let start = c.start_byte as usize;
        let len = self.cluster_byte_len(ci) as usize;
        run.text.get(start..start + len).unwrap_or("")
    }

    /// (d6e) Word-boundary predicate — same as the sparse
    /// `cluster_is_word_boundary`: no word character in the cluster text
    /// (whitespace AND punctuation are boundaries).
    fn cluster_is_word_boundary(&self, ci: u32) -> bool {
        !self
            .cluster_text_slice(ci)
            .chars()
            .any(super::cache::is_word_char)
    }

    /// (d6e) Visual line start — cluster by id, min-x cluster on its
    /// line, Leading affinity (mirrors the sparse flow).
    #[must_use]
    pub fn move_cursor_to_line_start(
        &self,
        cursor: azul_core::selection::TextCursor,
    ) -> azul_core::selection::TextCursor {
        use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
        let Some((_, line_ord)) = self.find_cursor_cluster(&cursor) else {
            return cursor;
        };
        let l = &self.lines[line_ord];
        let best = (l.clusters.0..l.clusters.1)
            .min_by(|a, b| {
                self.clusters[*a as usize]
                    .x
                    .partial_cmp(&self.clusters[*b as usize].x)
                    .unwrap_or(core::cmp::Ordering::Equal)
            });
        let Some(ci) = best else { return cursor };
        let run = self.runs.iter().find(|r| r.clusters.contains(&ci));
        let Some(run) = run else { return cursor };
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run.source_run,
                start_byte_in_run: self.clusters[ci as usize].start_byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    /// (d6e) Visual line end — max-x cluster on the line, Trailing.
    #[must_use]
    pub fn move_cursor_to_line_end(
        &self,
        cursor: azul_core::selection::TextCursor,
    ) -> azul_core::selection::TextCursor {
        use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
        let Some((_, line_ord)) = self.find_cursor_cluster(&cursor) else {
            return cursor;
        };
        let l = &self.lines[line_ord];
        let best = (l.clusters.0..l.clusters.1)
            .max_by(|a, b| {
                self.clusters[*a as usize]
                    .x
                    .partial_cmp(&self.clusters[*b as usize].x)
                    .unwrap_or(core::cmp::Ordering::Equal)
            });
        let Some(ci) = best else { return cursor };
        let run = self.runs.iter().find(|r| r.clusters.contains(&ci));
        let Some(run) = run else { return cursor };
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run.source_run,
                start_byte_in_run: self.clusters[ci as usize].start_byte,
            },
            affinity: CursorAffinity::Trailing,
        }
    }

    /// (d6e) Cursor at index `ci`, given affinity.
    fn cursor_at_index(
        &self,
        ci: u32,
        affinity: azul_core::selection::CursorAffinity,
    ) -> Option<azul_core::selection::TextCursor> {
        use azul_core::selection::{GraphemeClusterId, TextCursor};
        let c = self.clusters.get(ci as usize)?;
        let run = self.runs.iter().find(|r| r.clusters.contains(&ci))?;
        Some(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run.source_run,
                start_byte_in_run: c.start_byte,
            },
            affinity,
        })
    }

    /// (d6e) One word left — mirrors the sparse two-phase flow (skip
    /// boundary clusters, then skip the word, land Leading on its first
    /// cluster). Identical for pure-cluster layouts (the dense domain).
    #[must_use]
    pub fn move_cursor_to_prev_word(
        &self,
        cursor: azul_core::selection::TextCursor,
    ) -> azul_core::selection::TextCursor {
        use azul_core::selection::CursorAffinity;
        let Some((current, _)) = self.find_cursor_cluster(&cursor) else {
            return cursor;
        };
        let mut pos = if cursor.affinity == CursorAffinity::Leading {
            current.checked_sub(1)
        } else {
            Some(current)
        };
        while let Some(p) = pos {
            if !self.cluster_is_word_boundary(p) {
                break;
            }
            pos = p.checked_sub(1);
        }
        while let Some(p) = pos {
            if self.cluster_is_word_boundary(p) {
                if p + 1 < self.clusters.len() as u32 {
                    if let Some(c) = self.cursor_at_index(p + 1, CursorAffinity::Leading) {
                        return c;
                    }
                }
                break;
            }
            if p == 0 {
                if let Some(c) = self.cursor_at_index(0, CursorAffinity::Leading) {
                    return c;
                }
                break;
            }
            pos = p.checked_sub(1);
        }
        if pos.is_none() {
            if let Some(c) = self.cursor_at_index(0, CursorAffinity::Leading) {
                return c;
            }
        }
        cursor
    }

    /// (d6h) The per-run resolved ascent — the baseline distance from an
    /// item's TOP (metrics + half-leading), the same math the walkers
    /// derive inline. What makes per-item `position.y` reconstructible
    /// on MIXED-SIZE lines: the d6h expansion gate caught that sparse
    /// `position.y` is per-item (baseline-aligned tops differ when font
    /// sizes mix), while the line record's y is only the FIRST item's.
    #[must_use]
    pub fn resolved_run_ascent(run: &DenseRun) -> f32 {
        let m = &run.font_metrics;
        if m.units_per_em == 0 {
            return 0.0;
        }
        let scale = run.style.font_size_px / f32::from(m.units_per_em);
        let font_ascent = m.ascent * scale;
        let font_descent = (-m.descent * scale).max(0.0);
        let ad = font_ascent + font_descent;
        let lh = run
            .style
            .line_height
            .resolve_with_metrics(run.style.font_size_px, m);
        font_ascent + (lh - ad) / 2.0
    }

    /// The run containing cluster `ci` (runs partition clusters in
    /// order, so this is a binary search).
    #[must_use]
    pub fn run_of(&self, ci: u32) -> Option<&DenseRun> {
        let idx = self.runs.partition_point(|r| r.clusters.end <= ci);
        self.runs.get(idx).filter(|r| r.clusters.contains(&ci))
    }

    /// (d6g, y-semantics fixed in d6h) The sparse `PositionedItem`
    /// fields for cluster `i`: `(x, y, line_index)`. `y` is the ITEM's
    /// top — the line record's recorded first-item top, baseline-aligned
    /// across mixed-size runs via the run ascents (same-run clusters
    /// reduce to the recorded value). `line_index` is the line's
    /// `source_index`. `None` when `i` is out of range.
    #[must_use]
    pub fn positioned_cluster(&self, i: u32) -> Option<(f32, f32, usize)> {
        let c = self.clusters.get(i as usize)?;
        let li = self
            .lines
            .partition_point(|l| l.clusters.1 <= i)
            .min(self.lines.len().checked_sub(1)?);
        let line = &self.lines[li];
        if i < line.clusters.0 || i >= line.clusters.1 {
            return None;
        }
        let first_run = self.run_of(line.clusters.0)?;
        let my_run = self.run_of(i)?;
        // Same run ⟹ bit-exact recorded value (no float round-trip).
        let y = if core::ptr::eq(first_run, my_run) {
            line.baseline_y
        } else {
            line.baseline_y + Self::resolved_run_ascent(first_run)
                - Self::resolved_run_ascent(my_run)
        };
        Some((c.x, y, line.source_index as usize))
    }

    /// (d6h) FULL sparse materialization: rebuild the `PositionedItem`
    /// vec these arrays were built from — exact (`PartialEq`) for
    /// pure-cluster layouts, pinned by the equivalence gate. Transient:
    /// the page clipper (print/PDF) expands on demand once the retained
    /// sparse form retires; nothing stores the result.
    #[must_use]
    pub fn to_unified_items(&self) -> Vec<PositionedItem> {
        use super::cache::{
            ClusterFlags, ContentIndex, GlyphKind, GraphemeClusterId, Point, ShapedCluster,
            ShapedGlyph, ShapedGlyphVec, ShapedItem,
        };
        let mut out = Vec::with_capacity(self.clusters.len());
        let mut line_cursor = 0usize;
        let mut detail_cursor = 0usize;
        for run in &self.runs {
            let source_node_id =
                (run.source_node != u32::MAX).then(|| NodeId::new(run.source_node as usize));
            for ci in run.clusters.clone() {
                let c = &self.clusters[ci as usize];
                while line_cursor < self.lines.len() && self.lines[line_cursor].clusters.1 <= ci {
                    line_cursor += 1;
                }
                let line = &self.lines[line_cursor.min(self.lines.len().saturating_sub(1))];
                while detail_cursor < self.details.len()
                    && self.details[detail_cursor].cluster < ci
                {
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
                                offset: Point { x: dg.offset_x, y: dg.offset_y },
                                vertical_advance: dg.vertical_advance,
                                vertical_offset: Point {
                                    x: dg.vertical_offset_x,
                                    y: dg.vertical_offset_y,
                                },
                                script: run.script,
                                font_hash: run.font_hash,
                                font_metrics: run.font_metrics,
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
                        script: run.script,
                        font_hash: run.font_hash,
                        font_metrics: run.font_metrics,
                    })
                    .collect(),
                };
                let byte_len = detail.map_or_else(|| self.cluster_byte_len(ci), |d| d.byte_len);
                let f = c.flags.0;
                out.push(PositionedItem {
                    item: ShapedItem::Cluster(ShapedCluster {
                        source_text: run.text.clone(),
                        source_byte_len: u16::try_from(byte_len).unwrap_or(u16::MAX),
                        source_cluster_id: GraphemeClusterId {
                            source_run: run.source_run,
                            start_byte_in_run: c.start_byte,
                        },
                        source_content_index: ContentIndex {
                            run_index: run.source_run,
                            // (#25b) Two reconstruction models — see
                            // `DenseRun::item_linear`.
                            item_index: if run.item_linear {
                                run.item_base.wrapping_add(c.start_byte)
                            } else {
                                run.item_base
                            },
                        },
                        source_node_id,
                        glyphs,
                        flags: ClusterFlags(f & ClusterFlags::CLASSIFY_MASK),
                        advance: c.advance,
                        direction: run.direction,
                        style: run.style.clone(),
                        marker_position_outside: (f & ClusterFlags::DENSE_MARKER_SOME != 0)
                            .then_some(f & ClusterFlags::DENSE_MARKER_OUTSIDE != 0),
                        is_first_fragment: f & ClusterFlags::DENSE_IS_FIRST_FRAGMENT != 0,
                        is_last_fragment: f & ClusterFlags::DENSE_IS_LAST_FRAGMENT != 0,
                    }),
                    position: Point {
                        x: c.x,
                        // (d6h) Per-item y on mixed-size lines: the
                        // record holds the line's FIRST item top;
                        // baseline-align via run ascents. Same run ⟹
                        // the recorded value bit-exactly.
                        y: match self.run_of(line.clusters.0) {
                            Some(fr) if !core::ptr::eq(fr, run) => {
                                line.baseline_y + Self::resolved_run_ascent(fr)
                                    - Self::resolved_run_ascent(run)
                            }
                            _ => line.baseline_y,
                        },
                    },
                    line_index: line.source_index as usize,
                });
            }
        }
        out
    }

    /// (d6f) The single (direction, step) dispatch over the dense
    /// movement library — twin of the window's `resolve_step_static`.
    #[must_use]
    pub fn resolve_step(
        &self,
        cursor: &azul_core::selection::TextCursor,
        direction: azul_core::events::SelectionDirection,
        step: azul_core::events::SelectionStep,
    ) -> azul_core::selection::TextCursor {
        use azul_core::events::{SelectionDirection as D, SelectionStep as S};
        match (direction, step) {
            (D::Backward, S::Character) => self.move_cursor_left(*cursor),
            (D::Forward, S::Character) => self.move_cursor_right(*cursor),
            (D::Backward, S::Word) => self.move_cursor_to_prev_word(*cursor),
            (D::Forward, S::Word) => self.move_cursor_to_next_word(*cursor),
            (D::Backward, S::VisualLine) => self.move_cursor_up(*cursor, &mut None),
            (D::Forward, S::VisualLine) => self.move_cursor_down(*cursor, &mut None),
            (D::Backward, S::Line) => self.move_cursor_to_line_start(*cursor),
            (D::Forward, S::Line) => self.move_cursor_to_line_end(*cursor),
            (D::Backward, S::Document) => self.first_cluster_cursor().unwrap_or(*cursor),
            (D::Forward, S::Document) => self.last_cluster_cursor().unwrap_or(*cursor),
        }
    }

    /// (d6e) One word right — mirrors the sparse flow (skip the current
    /// word, then boundary clusters, land Leading on the next word; end
    /// of text falls to the last cluster Trailing).
    #[must_use]
    pub fn move_cursor_to_next_word(
        &self,
        cursor: azul_core::selection::TextCursor,
    ) -> azul_core::selection::TextCursor {
        use azul_core::selection::CursorAffinity;
        let Some((current, _)) = self.find_cursor_cluster(&cursor) else {
            return cursor;
        };
        let len = self.clusters.len() as u32;
        let start = if cursor.affinity == CursorAffinity::Trailing {
            current + 1
        } else {
            current
        };
        if start >= len {
            return cursor;
        }
        let mut pos = start;
        while pos < len && !self.cluster_is_word_boundary(pos) {
            pos += 1;
        }
        while pos < len {
            if !self.cluster_is_word_boundary(pos) {
                if let Some(c) = self.cursor_at_index(pos, CursorAffinity::Leading) {
                    return c;
                }
            }
            pos += 1;
        }
        self.last_cluster_cursor().unwrap_or(cursor)
    }

    /// (d6d) Point -> cursor hit test — the SAME weighted-distance scan
    /// as the sparse `hittest_cursor` (vertical distance x2 + horizontal
    /// outside-distance; closest cluster wins; affinity by midpoint).
    /// Cluster geometry from the dense arrays: x/width from
    /// `ClusterCompact` (base advance == bounds().width), y/height from the
    /// line record + the run's resolved line height. Also the
    /// click-to-position primitive.
    #[must_use]
    pub fn hittest_cursor(
        &self,
        point: Point,
    ) -> Option<azul_core::selection::TextCursor> {
        use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
        if self.clusters.is_empty() {
            return None;
        }
        let mut best: Option<(f32, u32, &DenseRun, f32)> = None; // (dist, ci, run, x)
        let mut line_iter = self.lines.iter().peekable();
        for (ci, run) in self
            .runs
            .iter()
            .flat_map(|r| (r.clusters.start..r.clusters.end).map(move |i| (i, r)))
        {
            let c = &self.clusters[ci as usize];
            while let Some(l) = line_iter.peek() {
                if ci >= l.clusters.1 {
                    line_iter.next();
                } else {
                    break;
                }
            }
            let (top_y, line_h) = line_iter.peek().map_or((0.0, 0.0), |l| (l.top_y, l.height));
            let m = &run.font_metrics;
            let h = if m.units_per_em == 0 {
                line_h
            } else {
                run.style
                    .line_height
                    .resolve_with_metrics(run.style.font_size_px, m)
            };
            let center_y = top_y + h / 2.0;
            let vertical = (point.y - center_y).abs();
            let horizontal = if point.x < c.x {
                c.x - point.x
            } else if point.x > c.x + c.advance {
                point.x - (c.x + c.advance)
            } else {
                0.0
            };
            let dist = vertical.mul_add(2.0, horizontal);
            if best.is_none_or(|(d, ..)| dist < d) {
                best = Some((dist, ci, run, c.x));
            }
        }
        let (_, ci, run, x) = best?;
        let c = &self.clusters[ci as usize];
        let affinity = if point.x < x + c.advance / 2.0 {
            CursorAffinity::Leading
        } else {
            CursorAffinity::Trailing
        };
        Some(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run.source_run,
                start_byte_in_run: c.start_byte,
            },
            affinity,
        })
    }

    /// (d6d) Locate a cursor's cluster index + its line ordinal.
    fn find_cursor_cluster(
        &self,
        cursor: &azul_core::selection::TextCursor,
    ) -> Option<(u32, usize)> {
        for r in &self.runs {
            if r.source_run != cursor.cluster_id.source_run {
                continue;
            }
            for ci in r.clusters.start..r.clusters.end {
                if self.clusters[ci as usize].start_byte == cursor.cluster_id.start_byte_in_run {
                    let line = self
                        .lines
                        .iter()
                        .position(|l| ci >= l.clusters.0 && ci < l.clusters.1)?;
                    return Some((ci, line));
                }
            }
        }
        None
    }

    /// (d6d) One line up, preserving the horizontal goal column — mirrors
    /// the sparse `move_cursor_up` flow: current cluster by id, `goal_x`
    /// seeded from affinity (Trailing = x + advance), target line's
    /// mid-height, then the weighted hit test.
    #[must_use]
    pub fn move_cursor_up(
        &self,
        cursor: azul_core::selection::TextCursor,
        goal_x: &mut Option<f32>,
    ) -> azul_core::selection::TextCursor {
        use azul_core::selection::CursorAffinity;
        let Some((ci, line_ord)) = self.find_cursor_cluster(&cursor) else {
            return cursor;
        };
        if line_ord == 0 {
            return cursor;
        }
        let c = &self.clusters[ci as usize];
        let current_x = goal_x.unwrap_or_else(|| {
            let x = match cursor.affinity {
                CursorAffinity::Leading => c.x,
                CursorAffinity::Trailing => c.x + c.advance,
            };
            *goal_x = Some(x);
            x
        });
        let target = &self.lines[line_ord - 1];
        let target_y = target.top_y + target.height / 2.0;
        self.hittest_cursor(Point { x: current_x, y: target_y })
            .unwrap_or(cursor)
    }

    /// (d6d) One line down — see [`Self::move_cursor_up`].
    #[must_use]
    pub fn move_cursor_down(
        &self,
        cursor: azul_core::selection::TextCursor,
        goal_x: &mut Option<f32>,
    ) -> azul_core::selection::TextCursor {
        use azul_core::selection::CursorAffinity;
        let Some((ci, line_ord)) = self.find_cursor_cluster(&cursor) else {
            return cursor;
        };
        if line_ord + 1 >= self.lines.len() {
            return cursor;
        }
        let c = &self.clusters[ci as usize];
        let current_x = goal_x.unwrap_or_else(|| {
            let x = match cursor.affinity {
                CursorAffinity::Leading => c.x,
                CursorAffinity::Trailing => c.x + c.advance,
            };
            *goal_x = Some(x);
            x
        });
        let target = &self.lines[line_ord + 1];
        let target_y = target.top_y + target.height / 2.0;
        self.hittest_cursor(Point { x: current_x, y: target_y })
            .unwrap_or(cursor)
    }

    /// (d6c) One caret stop left — IDENTICAL to the sparse
    /// `move_cursor_left` by construction: the stops list is pinned equal
    /// (`grapheme_stops` gate) and the offset arithmetic is the SAME static
    /// helper the sparse implementation uses.
    #[must_use]
    pub fn move_cursor_left(
        &self,
        cursor: azul_core::selection::TextCursor,
    ) -> azul_core::selection::TextCursor {
        use super::cache::UnifiedLayout;
        let stops = self.grapheme_stops();
        if stops.is_empty() {
            return cursor;
        }
        let Some(offset) = UnifiedLayout::grapheme_caret_offset(&stops, &cursor) else {
            return cursor;
        };
        UnifiedLayout::cursor_from_grapheme_offset(&stops, offset.saturating_sub(1))
    }

    /// (d6c) One caret stop right — see [`Self::move_cursor_left`].
    #[must_use]
    pub fn move_cursor_right(
        &self,
        cursor: azul_core::selection::TextCursor,
    ) -> azul_core::selection::TextCursor {
        use super::cache::UnifiedLayout;
        let stops = self.grapheme_stops();
        if stops.is_empty() {
            return cursor;
        }
        let Some(offset) = UnifiedLayout::grapheme_caret_offset(&stops, &cursor) else {
            return cursor;
        };
        UnifiedLayout::cursor_from_grapheme_offset(&stops, (offset + 1).min(stops.len()))
    }

    /// (d6f) Leading cursor on the FIRST cluster — sparse
    /// `get_first_cluster_cursor` twin.
    #[must_use]
    pub fn first_cluster_cursor(&self) -> Option<azul_core::selection::TextCursor> {
        self.cursor_at_index(0, azul_core::selection::CursorAffinity::Leading)
    }

    /// (d4) The trailing cursor on the LAST cluster — the dense twin of
    /// the sparse `items.iter().rev().find_map(Cluster)` scans (which
    /// skip trailing non-clusters, exactly as taking the last dense
    /// cluster does). `None` when the layout has no clusters.
    #[must_use]
    pub fn last_cluster_cursor(&self) -> Option<azul_core::selection::TextCursor> {
        let last_ci = u32::try_from(self.clusters.len()).ok()?.checked_sub(1)?;
        let c = self.clusters.last()?;
        let run = self.runs.iter().rev().find(|r| r.clusters.contains(&last_ci))?;
        Some(azul_core::selection::TextCursor {
            cluster_id: azul_core::selection::GraphemeClusterId {
                source_run: run.source_run,
                start_byte_in_run: c.start_byte,
            },
            affinity: azul_core::selection::CursorAffinity::Trailing,
        })
    }

    /// (d4) Cursor for an IFC-wide byte offset — the dense twin of the
    /// sparse accumulation walk: clusters in item order, each
    /// contributing `cluster_byte_len`, first cluster whose span
    /// contains the offset wins; past-the-end falls to the last cluster.
    #[must_use]
    pub fn byte_offset_to_cursor(&self, byte_offset: u32) -> Option<azul_core::selection::TextCursor> {
        use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
        let cursor_at = |ci: u32| -> Option<TextCursor> {
            let c = self.clusters.get(ci as usize)?;
            let run = self.runs.iter().find(|r| r.clusters.contains(&ci))?;
            Some(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: run.source_run,
                    start_byte_in_run: c.start_byte,
                },
                affinity: CursorAffinity::Trailing,
            })
        };
        if self.clusters.is_empty() {
            return None;
        }
        if byte_offset == 0 {
            return cursor_at(0);
        }
        let mut acc = 0u32;
        for ci in 0..self.clusters.len() as u32 {
            let len = self.cluster_byte_len(ci);
            let end = acc + len;
            if byte_offset >= acc && byte_offset <= end {
                return cursor_at(ci);
            }
            acc = end;
        }
        cursor_at(self.clusters.len() as u32 - 1)
    }
}


/// §3.2 step 3: the dense twin of [`super::glyphs::get_glyph_positions`]
/// (the reference walker the other two consumers agree with). Walks the
/// dense arrays only. Positions agree EXACTLY with the reference for
/// uniform-font clusters (the run's metrics reproduce the per-item
/// ascent math); multi-font fallback clusters would need the per-glyph
/// metrics the detail table deliberately does not carry — the atomics /
/// combined blocks stay on the sparse side and are not walked here.
///
/// `PositionedGlyph.advance` reports the PAINTED advance (incl. kerning,
/// as the dense model folds it) — the reference reports the base advance
/// and advances its pen by base+kerning; positions are identical either
/// way, which is what the agreement gate compares.
#[must_use]
pub fn get_glyph_positions_dense(dense: &DenseText) -> Vec<PositionedGlyph> {
    let mut out = Vec::with_capacity(dense.clusters.len());
    let mut line_iter = dense.lines.iter().peekable();
    let mut detail_iter = dense.details.iter().peekable();

    for (ci, run) in dense
        .runs
        .iter()
        .flat_map(|r| (r.clusters.start..r.clusters.end).map(move |i| (i, r)))
    {
        let c = &dense.clusters[ci as usize];
        // Advance the line cursor to the record containing this cluster.
        while let Some(l) = line_iter.peek() {
            if ci >= l.clusters.1 {
                line_iter.next();
            } else {
                break;
            }
        }
        let top_y = line_iter.peek().map_or(0.0, |l| l.top_y);
        // Per-run ascent: the same math the reference derives per item
        // (metrics + half-leading), amortised — run metrics are uniform.
        let m = &run.font_metrics;
        let ascent = if m.units_per_em == 0 {
            0.0
        } else {
            let scale = run.style.font_size_px / f32::from(m.units_per_em);
            let font_ascent = m.ascent * scale;
            let font_descent = (-m.descent * scale).max(0.0);
            let ad = font_ascent + font_descent;
            let lh = run
                .style
                .line_height
                .resolve_with_metrics(run.style.font_size_px, m);
            font_ascent + (lh - ad) / 2.0
        };
        // The RUN's own solved y, not the line's: a line mixing sizes puts
        // its taller run on a different baseline (see DenseRun::y).
        let baseline_y = run.y + ascent;

        // Detail cluster? (details are in cluster order.)
        let detail = loop {
            match detail_iter.peek() {
                Some(d) if d.cluster < ci => {
                    detail_iter.next();
                }
                Some(d) if d.cluster == ci => break Some(**d),
                _ => break None,
            }
        };
        match detail {
            Some(d) => {
                let mut pen_x = c.x;
                for dg in &dense.detail_glyphs[d.glyphs.0 as usize..d.glyphs.1 as usize] {
                    out.push(PositionedGlyph {
                        glyph_id: dg.glyph_id,
                        position: Point {
                            x: pen_x + dg.offset_x,
                            y: baseline_y - dg.offset_y,
                        },
                        advance: dg.advance,
                    });
                    pen_x += dg.advance;
                }
            }
            None => {
                out.push(PositionedGlyph {
                    glyph_id: c.glyph_id,
                    position: Point { x: c.x, y: baseline_y },
                    advance: c.advance,
                });
            }
        }
    }
    out
}

/// §3.2 step 4: the dense twin of [`super::glyphs::get_glyph_runs_simple`]
/// (the paint-path consumer). Same walk as [`get_glyph_positions_dense`];
/// the run-merge predicate is the REFERENCE's — painted VALUES on the same
/// baseline, not Arc identity — so two dense runs that split only on style
/// Arc identity or `source_run` merge back into one paint run exactly as
/// the reference merges glyphs from different shaping runs. The border
/// fragment post-process is literally shared
/// ([`super::glyphs::suppress_split_border_fragments`]), so CSS 2.2 §9.4.2
/// split-point suppression cannot drift between the walkers.
///
/// Same documented limits as the position walker: combined blocks
/// (tate-chu-yoko) stay on the sparse side per the plan's `AtomicItem`
/// design, and a multi-font-fallback CLUSTER would split mid-cluster in
/// the reference but not here (the detail table carries no per-glyph
/// font hash) — both migrate in a later step; the agreement gate covers
/// the dense-expressible subset.
#[allow(clippy::float_cmp)] // intentional exact compare: same predicate as the reference walker
#[must_use]
pub fn get_glyph_runs_simple_dense(dense: &DenseText) -> Vec<SimpleGlyphRun> {
    let mut runs: Vec<SimpleGlyphRun> = Vec::new();
    let mut current_run: Option<SimpleGlyphRun> = None;
    let mut current_baseline: Option<f32> = None;
    let mut line_iter = dense.lines.iter().peekable();
    let mut detail_iter = dense.details.iter().peekable();

    for (ci, run) in dense
        .runs
        .iter()
        .flat_map(|r| (r.clusters.start..r.clusters.end).map(move |i| (i, r)))
    {
        let c = &dense.clusters[ci as usize];

        // Detail cluster? (details are in cluster order.) Resolved FIRST:
        // a zero-glyph detail cluster contributes nothing and must not
        // open a run (the reference's per-glyph loop never runs there).
        let detail = loop {
            match detail_iter.peek() {
                Some(d) if d.cluster < ci => {
                    detail_iter.next();
                }
                Some(d) if d.cluster == ci => break Some(**d),
                _ => break None,
            }
        };
        if detail.is_some_and(|d| d.glyphs.0 == d.glyphs.1) {
            continue;
        }

        // Line cursor + per-run ascent: identical to the position walker.
        while let Some(l) = line_iter.peek() {
            if ci >= l.clusters.1 {
                line_iter.next();
            } else {
                break;
            }
        }
        let top_y = line_iter.peek().map_or(0.0, |l| l.top_y);
        let m = &run.font_metrics;
        let ascent = if m.units_per_em == 0 {
            0.0
        } else {
            let scale = run.style.font_size_px / f32::from(m.units_per_em);
            let font_ascent = m.ascent * scale;
            let font_descent = (-m.descent * scale).max(0.0);
            let ad = font_ascent + font_descent;
            let lh = run
                .style
                .line_height
                .resolve_with_metrics(run.style.font_size_px, m);
            font_ascent + (lh - ad) / 2.0
        };
        // The RUN's own solved y, not the line's: a line mixing sizes puts
        // its taller run on a different baseline (see DenseRun::y).
        let baseline_y = run.y + ascent;

        let style = &run.style;
        let source_node_id =
            (run.source_node != u32::MAX).then(|| NodeId::new(run.source_node as usize));

        // The reference predicate, evaluated per CLUSTER — every compared
        // field is uniform within a cluster there, so boundaries are
        // identical.
        let merges = current_run.as_ref().is_some_and(|r| {
            current_baseline == Some(baseline_y)
                && r.font_hash == run.font_hash
                && r.color == style.color
                && r.background_color == style.background_color
                && r.background_content == style.background_content
                && r.border == style.border
                && r.font_size_px == style.font_size_px
                && r.text_decoration == style.text_decoration
                && r.source_node_id == source_node_id
        });
        if !merges {
            if let Some(prev) = current_run.take() {
                runs.push(prev);
            }
            current_baseline = Some(baseline_y);
            current_run = Some(SimpleGlyphRun {
                glyphs: Vec::new(),
                color: style.color,
                background_color: style.background_color,
                background_content: style.background_content.clone(),
                border: style.border,
                font_hash: run.font_hash,
                font_size_px: style.font_size_px,
                text_decoration: style.text_decoration,
                is_ime_preview: false,
                source_node_id,
            });
        }
        let out = &mut current_run
            .as_mut()
            .expect("opened above when absent")
            .glyphs;

        match detail {
            Some(d) => {
                let mut pen_x = c.x;
                for dg in &dense.detail_glyphs[d.glyphs.0 as usize..d.glyphs.1 as usize] {
                    out.push(GlyphInstance {
                        index: u32::from(dg.glyph_id),
                        point: LogicalPosition {
                            x: pen_x + dg.offset_x,
                            y: baseline_y - dg.offset_y,
                        },
                        size: LogicalSize::default(),
                    });
                    pen_x += dg.advance;
                }
            }
            None => {
                out.push(GlyphInstance {
                    index: u32::from(c.glyph_id),
                    point: LogicalPosition { x: c.x, y: baseline_y },
                    size: LogicalSize::default(),
                });
            }
        }
    }
    if let Some(r) = current_run {
        runs.push(r);
    }

    super::glyphs::suppress_split_border_fragments(&mut runs);
    runs
}

/// §3.2 step 5: the dense twin of [`super::glyphs::get_glyph_runs_pdf`]
/// (printpdf's frozen contract: `cluster.glyphs` iteration +
/// `glyph.font_hash`). Same walk as the other dense twins; the run
/// predicate is the reference's (font, colour, background, size,
/// decoration, LINE index, direction, writing mode — note: no border, no
/// background layers, no source node).
///
/// The per-cluster TEXT — which the reference reads from
/// `ShapedCluster::text` — is reconstructed here as the grapheme cluster
/// at `start_byte` in the run's shared source text: the same
/// segmentation shaping used to build the cluster, so the two agree
/// byte-for-byte (the agreement gate pins it, and 3c deletes the
/// per-cluster copy on the strength of exactly this equivalence).
/// Styles that rewrite text between source and shaping (text-transform)
/// keep the sparse walker until the transform story lands.
///
/// `PdfPositionedGlyph::advance` reports the PAINTED advance for detail
/// glyphs (kerning folded, as the dense model stores it) where the
/// reference reports the base advance — positions are identical either
/// way, same documented divergence as the position walker.
#[allow(clippy::float_cmp)] // intentional exact compare: same predicate as the reference walker
#[allow(clippy::too_many_lines)] // one cohesive walk, mirrors the reference's structure
#[must_use]
pub fn get_glyph_runs_pdf_dense<T: ParsedFontTrait>(
    dense: &DenseText,
    fonts: &LoadedFonts<T>,
) -> Vec<PdfGlyphRun<T>> {
    use unicode_segmentation::UnicodeSegmentation;

    let mut runs: Vec<PdfGlyphRun<T>> = Vec::new();
    let mut current_run: Option<PdfGlyphRun<T>> = None;
    let mut line_iter = dense.lines.iter().peekable();
    let mut detail_iter = dense.details.iter().peekable();

    for (ci, run) in dense
        .runs
        .iter()
        .flat_map(|r| (r.clusters.start..r.clusters.end).map(move |i| (i, r)))
    {
        let c = &dense.clusters[ci as usize];

        // Detail resolution first: a zero-glyph cluster contributes
        // nothing (the reference skips `cluster.glyphs.is_empty()`).
        let detail = loop {
            match detail_iter.peek() {
                Some(d) if d.cluster < ci => {
                    detail_iter.next();
                }
                Some(d) if d.cluster == ci => break Some(**d),
                _ => break None,
            }
        };
        if detail.is_some_and(|d| d.glyphs.0 == d.glyphs.1) {
            continue;
        }

        // A glyph whose font is not loaded is skipped WITHOUT breaking
        // the open run, exactly as the reference's per-glyph `continue`.
        let Some(font) = fonts.get_by_hash(run.font_hash) else {
            continue;
        };

        // Line cursor: source line index + per-run ascent → baseline.
        while let Some(l) = line_iter.peek() {
            if ci >= l.clusters.1 {
                line_iter.next();
            } else {
                break;
            }
        }
        let (top_y, line_index) = line_iter
            .peek()
            .map_or((0.0, 0usize), |l| (l.top_y, l.source_index as usize));
        let m = &run.font_metrics;
        let ascent = if m.units_per_em == 0 {
            0.0
        } else {
            let scale = run.style.font_size_px / f32::from(m.units_per_em);
            let font_ascent = m.ascent * scale;
            let font_descent = (-m.descent * scale).max(0.0);
            let ad = font_ascent + font_descent;
            let lh = run
                .style
                .line_height
                .resolve_with_metrics(run.style.font_size_px, m);
            font_ascent + (lh - ad) / 2.0
        };
        // The RUN's own solved y, not the line's: a line mixing sizes puts
        // its taller run on a different baseline (see DenseRun::y).
        let baseline_y = run.y + ascent;

        let style = &run.style;

        // The cluster's source text: `cluster_byte_len` bytes at start_byte in
        // the shared run text.
        //
        // NOT "the next grapheme". A LIGATURE-FUSED cluster spans several
        // graphemes — "fi" is two — so a grapheme walk returns "f" and the
        // second character is lost. That is what `ClusterDetail.byte_len`
        // exists to record, in its own words: "a ligature-fused cluster spans
        // multiple graphemes, so 'next grapheme boundary' under-measures it".
        // The sparse expander and the word-boundary predicate both ask
        // `cluster_byte_len`; this walker asked the graphemes, and the
        // difference reached users as PDF text: every ligated word came out of
        // pdftotext with its second letter missing — "Configure" -> "Confgure",
        // "filter" -> "flter", "offline" -> "offine" — because the ToUnicode
        // entry for the ligature glyph said "f".
        let start = c.start_byte as usize;
        let len = dense.cluster_byte_len(ci) as usize;
        let cluster_text: &str = run
            .text
            .get(start..start.saturating_add(len))
            // A byte length that is not a char boundary would slice-panic;
            // fall back to the old grapheme walk rather than take the process
            // down over a malformed record.
            .or_else(|| {
                run.text
                    .get(start..)
                    .and_then(|s| s.graphemes(true).next())
            })
            .unwrap_or("");

        // The reference predicate, evaluated per CLUSTER (all compared
        // fields are uniform within a cluster in dense scope).
        let merges = current_run.as_ref().is_some_and(|r| {
            r.font_hash == run.font_hash
                && r.color == style.color
                && r.background_color == style.background_color
                && r.font_size_px == style.font_size_px
                && r.text_decoration == style.text_decoration
                && r.line_index == line_index
                && r.direction == run.direction
                && r.writing_mode == style.writing_mode
        });
        if !merges {
            if let Some(prev) = current_run.take() {
                runs.push(prev);
            }
            current_run = Some(PdfGlyphRun {
                glyphs: Vec::new(),
                color: style.color,
                background_color: style.background_color,
                font: font.clone(),
                font_hash: run.font_hash,
                font_size_px: style.font_size_px,
                text_decoration: style.text_decoration,
                line_index,
                direction: run.direction,
                writing_mode: style.writing_mode,
                baseline_start: Point { x: c.x, y: baseline_y },
                cluster_texts: Vec::new(),
            });
        }
        let open = current_run.as_mut().expect("opened above when absent");

        if let Some(d) = detail {
            let dgs = &dense.detail_glyphs[d.glyphs.0 as usize..d.glyphs.1 as usize];
            let count = dgs.len();
            let mut pen_x = c.x;
            for (glyph_idx, dg) in dgs.iter().enumerate() {
                // The reference's per-glyph codepoint split, verbatim.
                let unicode_codepoint = if count == 1 {
                    cluster_text.to_string()
                } else {
                    let byte_offset = dg.cluster_offset as usize;
                    if byte_offset < cluster_text.len() {
                        cluster_text[byte_offset..].chars().next().map_or_else(
                            || cluster_text.to_string(),
                            |ch| ch.to_string(),
                        )
                    } else if glyph_idx == 0 {
                        cluster_text.to_string()
                    } else {
                        String::new()
                    }
                };
                open.glyphs.push(PdfPositionedGlyph {
                    glyph_id: dg.glyph_id,
                    position: Point {
                        x: pen_x + dg.offset_x,
                        y: baseline_y - dg.offset_y,
                    },
                    advance: dg.advance,
                    unicode_codepoint,
                });
                open.cluster_texts.push(cluster_text.to_string());
                pen_x += dg.advance;
            }
        } else {
            open.glyphs.push(PdfPositionedGlyph {
                glyph_id: c.glyph_id,
                position: Point { x: c.x, y: baseline_y },
                advance: c.advance,
                unicode_codepoint: cluster_text.to_string(),
            });
            open.cluster_texts.push(cluster_text.to_string());
        }
    }
    if let Some(r) = current_run {
        runs.push(r);
    }
    runs
}
