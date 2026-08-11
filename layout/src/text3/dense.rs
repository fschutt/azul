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
    pub script: Script,
    /// Bidi direction — uniform per run BY CONSTRUCTION (the builder
    /// splits on it): mixed-bidi text in one styled run must not share a
    /// dense run, both for the PDF walker's run predicate and for RTL
    /// border-fragment assignment.
    pub direction: BidiDirection,
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
#[derive(Debug, Clone, Copy, PartialEq)]
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
            let split = match &current_run {
                None => true,
                Some(r) => {
                    !Arc::ptr_eq(&r.style, &c.style)
                        || !Arc::ptr_eq(&r.text, &c.source_text)
                        || r.font_hash != font_hash
                        || r.source_run != c.source_cluster_id.source_run
                        || r.source_node != source_node
                        || r.direction != c.direction
                }
            };
            if split {
                if let Some(mut r) = current_run.take() {
                    r.clusters.end = cluster_index;
                    dense.runs.push(r);
                }
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
                    script,
                    direction: c.direction,
                });
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
                dense.details.push(ClusterDetail {
                    cluster: cluster_index,
                    glyphs: (start, end),
                    byte_len: u32::from(c.source_byte_len),
                });
            }

            dense.clusters.push(ClusterCompact {
                glyph_id: first_glyph.map_or(0, |g| g.glyph_id),
                flags: c.flags,
                advance: c.advance,
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
        let baseline_y = top_y + ascent;

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
        let baseline_y = top_y + ascent;

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
        let baseline_y = top_y + ascent;

        let style = &run.style;

        // The cluster's source text: the grapheme cluster at start_byte
        // in the shared run text (see the doc comment).
        let cluster_text: &str = run
            .text
            .get(c.start_byte as usize..)
            .and_then(|s| s.graphemes(true).next())
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

        match detail {
            Some(d) => {
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
            }
            None => {
                open.glyphs.push(PdfPositionedGlyph {
                    glyph_id: c.glyph_id,
                    position: Point { x: c.x, y: baseline_y },
                    advance: c.advance,
                    unicode_codepoint: cluster_text.to_string(),
                });
                open.cluster_texts.push(cluster_text.to_string());
            }
        }
    }
    if let Some(r) = current_run {
        runs.push(r);
    }
    runs
}
