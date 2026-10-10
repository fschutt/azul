//! A laid-out text: the positioned items, its lines, overflow and intrinsic sizes, and incremental relayout.

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

#[derive(Debug, Clone)]
pub struct UnifiedLayout {
    pub items: Vec<PositionedItem>,
    /// Information about content that did not fit.
    pub overflow: OverflowInfo,
}

impl UnifiedLayout {
    /// The cursor AFTER the last text cluster (Trailing on the final
    /// grapheme) — the end-of-text position selections and Ctrl+End use.
    /// `None` for layouts with no text clusters.
    #[must_use]
    pub fn end_cursor(&self) -> Option<TextCursor> {
        use azul_core::selection::CursorAffinity;
        let mut best: Option<GraphemeClusterId> = None;
        for item in &self.items {
            if let ShapedItem::Cluster(c) = &item.item {
                let id = c.source_cluster_id;
                let better = best.is_none_or(|b| {
                    (id.source_run, id.start_byte_in_run) > (b.source_run, b.start_byte_in_run)
                });
                if better {
                    best = Some(id);
                }
            }
        }
        Some(TextCursor {
            cluster_id: best?,
            affinity: CursorAffinity::Trailing,
        })
    }

    /// Calculate the bounding box of all positioned items.
    /// This is computed on-demand rather than cached.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        if self.items.is_empty() {
            return Rect::default();
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for item in &self.items {
            let item_x = item.position.x;
            let item_y = item.position.y;

            // Get item dimensions
            let item_bounds = item.item.bounds();
            let item_width = item_bounds.width;
            let item_height = item_bounds.height;

            min_x = min_x.min(item_x);
            min_y = min_y.min(item_y);
            max_x = max_x.max(item_x + item_width);
            max_y = max_y.max(item_y + item_height);
        }

        Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    /// Where the FIRST line's baseline lies, from the top of the layout (the
    /// IFC's content box): see [`baseline_in_layout`].
    #[must_use]
    pub fn first_baseline(&self) -> Option<f32> {
        self.items.iter().find_map(baseline_in_layout)
    }

    /// Where the LAST line's baseline lies, from the top of the layout (the
    /// IFC's content box) - an inline-block's baseline (CSS 2.2 s10.8.1).
    /// It used to be the last item's ascent alone, wherever that item sat:
    /// the half-leading of a tall line and every line above went missing,
    /// and an inline-block of `line-height: 5` text rose 40px above its line.
    #[must_use]
    pub fn last_baseline(&self) -> Option<f32> {
        self.items.iter().rev().find_map(baseline_in_layout)
    }

    /// The baseline of this layout's LAST line box, in the layout's own space
    /// (from the top of the content box that holds the lines): what an
    /// inline-block holding these lines sits on (CSS 2.2 10.8.1, "the
    /// baseline of its last line box"). `None` without a line that has a
    /// baseline (only breaks, tabs or glyph-less clusters).
    ///
    /// It is the baseline of a baseline-aligned item of that line - an item
    /// shifted by `vertical-align` (sub, super, a length) only when the line
    /// has no other - placed where `position_one_line` put it: its top plus
    /// its ascent (`get_item_vertical_metrics_approx` is exact for a cluster
    /// with glyphs; an atomic inline's ascent is its height above its
    /// `baseline_offset`). Not [`Self::last_baseline`], which is the raw
    /// font ascent of the last item with no line position (the flex bridge
    /// reads that one as an item's first baseline).
    #[must_use]
    pub fn last_line_baseline(&self) -> Option<f32> {
        let ascent = |item: &ShapedItem| -> Option<f32> {
            match item {
                ShapedItem::Cluster(c) if !c.glyphs.is_empty() => {
                    Some(get_item_vertical_metrics_approx(item).0)
                }
                ShapedItem::Object {
                    bounds,
                    baseline_offset,
                    ..
                }
                | ShapedItem::CombinedBlock {
                    bounds,
                    baseline_offset,
                    ..
                } => Some((bounds.height - *baseline_offset).max(0.0)),
                _ => None,
            }
        };
        let last_line = self
            .items
            .iter()
            .filter(|p| ascent(&p.item).is_some())
            .map(|p| p.line_index)
            .max()?;
        let on_last_line = || self.items.iter().filter(move |p| p.line_index == last_line);
        let on_baseline = |p: &&PositionedItem| {
            matches!(
                get_item_vertical_align(&p.item),
                None | Some(VerticalAlign::Baseline)
            )
        };
        let item = on_last_line()
            .filter(|p| ascent(&p.item).is_some())
            .find(on_baseline)
            .or_else(|| on_last_line().find(|p| ascent(&p.item).is_some()))?;
        Some(item.position.y + ascent(&item.item)?)
    }

    /// The closest logical cursor position to a point in this layout's OWN
    /// coordinate space — [`ScrolledContentPoint`], i.e. content-box-local
    /// with the hosting box's own scroll offset added back.
    ///
    /// Prefer this over [`Self::hittest_cursor`] anywhere a real pointer is
    /// involved: the inline layout is built once, unscrolled, relative to the
    /// content box, and it is precisely the two terms in that sentence
    /// (`padding + border`, and the box's own scroll) that the pointer
    /// pipeline used to drop. `LayoutWindow::ifc_local_point` is the one
    /// function that produces this type from a hit-test result.
    #[inline]
    #[must_use]
    pub fn hittest_point(
        &self,
        point: azul_core::spaces::ScrolledContentPoint,
    ) -> Option<TextCursor> {
        self.hittest_cursor(point.get())
    }

    /// Takes a point relative to the layout's origin and returns the closest
    /// logical cursor position.
    ///
    /// SPACE-UNCHECKED: the point must already be in this layout's own space
    /// (see [`Self::hittest_point`], which states that in the type). Kept
    /// untyped for the text-layout unit tests, which construct layouts
    /// directly and have no boxes, padding or scrolling to speak of.
    ///
    /// This is the unified hit-testing implementation.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use]
    pub fn hittest_cursor(&self, point: LogicalPosition) -> Option<TextCursor> {
        if self.items.is_empty() {
            return None;
        }

        // Find the closest cluster vertically and horizontally
        let mut closest_item_idx = 0;
        let mut closest_distance = f32::MAX;

        for (idx, item) in self.items.iter().enumerate() {
            // Only consider cluster items for cursor placement
            if !matches!(item.item, ShapedItem::Cluster(_)) {
                continue;
            }

            let item_bounds = item.item.bounds();
            let item_center_y = item.position.y + item_bounds.height / 2.0;

            // Distance from click position to item center
            let vertical_distance = (point.y - item_center_y).abs();

            // For horizontal distance, check if we're within the cluster bounds
            let horizontal_distance = if point.x < item.position.x {
                item.position.x - point.x
            } else if point.x > item.position.x + item_bounds.width {
                point.x - (item.position.x + item_bounds.width)
            } else {
                0.0 // Inside the cluster horizontally
            };

            // Combined distance (prioritize vertical proximity)
            let distance = vertical_distance * 2.0 + horizontal_distance;

            if distance < closest_distance {
                closest_distance = distance;
                closest_item_idx = idx;
            }
        }

        // Get the closest cluster
        let closest_item = &self.items[closest_item_idx];
        let cluster = match &closest_item.item {
            ShapedItem::Cluster(c) => c,
            // Objects are treated as a single cluster for selection
            ShapedItem::Object { source, .. } | ShapedItem::CombinedBlock { source, .. } => {
                return Some(TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: source.run_index,
                        start_byte_in_run: source.item_index,
                    },
                    affinity: if point.x
                        < closest_item.position.x + (closest_item.item.bounds().width / 2.0)
                    {
                        CursorAffinity::Leading
                    } else {
                        CursorAffinity::Trailing
                    },
                });
            }
            _ => return None,
        };

        // Determine affinity based on which half of the cluster was clicked
        let cluster_mid_x = closest_item.position.x + cluster.advance / 2.0;
        let affinity = if point.x < cluster_mid_x {
            CursorAffinity::Leading
        } else {
            CursorAffinity::Trailing
        };

        Some(TextCursor {
            cluster_id: cluster.source_cluster_id,
            affinity,
        })
    }

    /// Given a logical selection range, returns a vector of visual rectangles
    /// that cover the selected text, in the layout's coordinate space.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    #[must_use]
    pub fn get_selection_rects(&self, range: &SelectionRange) -> Vec<LogicalRect> {
        // 1. Build a map from the logical cluster ID to the visual PositionedItem for fast lookups.
        let mut cluster_map: HashMap<GraphemeClusterId, &PositionedItem> = HashMap::new();
        for item in &self.items {
            if let Some(cluster) = item.item.as_cluster() {
                cluster_map.insert(cluster.source_cluster_id, item);
            }
        }

        // 2. Normalize the range to ensure start always logically precedes end.
        let (start_cursor, end_cursor) = if range.start.cluster_id > range.end.cluster_id
            || (range.start.cluster_id == range.end.cluster_id
                && range.start.affinity > range.end.affinity)
        {
            (range.end, range.start)
        } else {
            (range.start, range.end)
        };

        // 3. Find the positioned items corresponding to the start and end of the selection.
        let Some(start_item) = cluster_map.get(&start_cursor.cluster_id) else {
            return Vec::new();
        };
        let Some(end_item) = cluster_map.get(&end_cursor.cluster_id) else {
            return Vec::new();
        };

        let mut rects = Vec::new();

        // Helper to get the absolute visual X coordinate of a cursor. The logical
        // start (Leading) edge is the cluster's LEFT for LTR but its RIGHT for RTL;
        // Trailing is the mirror.
        let get_cursor_x = |item: &PositionedItem, affinity: CursorAffinity| -> f32 {
            let left = item.position.x;
            let right = item.position.x + get_item_measure(&item.item, false);
            let rtl = item.item.as_cluster().is_some_and(|c| c.direction.is_rtl());
            match (affinity, rtl) {
                (CursorAffinity::Leading, false) | (CursorAffinity::Trailing, true) => left,
                (CursorAffinity::Trailing, false) | (CursorAffinity::Leading, true) => right,
            }
        };

        // Helper to get the visual bounding box of all content on a specific line index.
        let get_line_bounds = |line_index: usize| -> Option<LogicalRect> {
            let items_on_line = self.items.iter().filter(|i| i.line_index == line_index);

            let mut min_x: Option<f32> = None;
            let mut max_x: Option<f32> = None;
            let mut min_y: Option<f32> = None;
            let mut max_y: Option<f32> = None;

            for item in items_on_line {
                // Skip items that don't take up space (like hard breaks)
                let item_bounds = item.item.bounds();
                if item_bounds.width <= 0.0 && item_bounds.height <= 0.0 {
                    continue;
                }

                let item_x_end = item.position.x + item_bounds.width;
                let item_y_end = item.position.y + item_bounds.height;

                min_x = Some(min_x.map_or(item.position.x, |mx| mx.min(item.position.x)));
                max_x = Some(max_x.map_or(item_x_end, |mx| mx.max(item_x_end)));
                min_y = Some(min_y.map_or(item.position.y, |my| my.min(item.position.y)));
                max_y = Some(max_y.map_or(item_y_end, |my| my.max(item_y_end)));
            }

            if let (Some(min_x), Some(max_x), Some(min_y), Some(max_y)) =
                (min_x, max_x, min_y, max_y)
            {
                Some(LogicalRect {
                    origin: LogicalPosition { x: min_x, y: min_y },
                    size: LogicalSize {
                        width: max_x - min_x,
                        height: max_y - min_y,
                    },
                })
            } else {
                None
            }
        };

        // 4. Handle single-line selection.
        if start_item.line_index == end_item.line_index {
            if let Some(line_bounds) = get_line_bounds(start_item.line_index) {
                // Walk the selected clusters in VISUAL order and group them into
                // segments by bidi direction + visual contiguity, emitting one rect
                // per segment. A single endpoint-to-endpoint span over-covers (and can
                // under-cover) bidi selections, whose logically-contiguous clusters are
                // NOT visually contiguous. Pure-LTR/RTL contiguous runs collapse to a
                // single rect, matching browser/CoreText behavior.
                let mut segments: Vec<(f32, f32, BidiDirection)> = Vec::new();
                for item in &self.items {
                    if item.line_index != start_item.line_index {
                        continue;
                    }
                    let Some(c) = item.item.as_cluster() else {
                        continue;
                    };
                    let id = c.source_cluster_id;
                    // A cluster is selected when it lies within the (affinity-aware)
                    // logical range: the start cluster is included only if the start
                    // cursor sits on its leading edge; the end cluster only if the end
                    // cursor sits on its trailing edge.
                    let after_start = id > start_cursor.cluster_id
                        || (id == start_cursor.cluster_id
                            && start_cursor.affinity == CursorAffinity::Leading);
                    let before_end = id < end_cursor.cluster_id
                        || (id == end_cursor.cluster_id
                            && end_cursor.affinity == CursorAffinity::Trailing);
                    if !(after_start && before_end) {
                        continue;
                    }
                    let x0 = item.position.x;
                    let x1 = item.position.x + get_item_measure(&item.item, false);
                    let (lo, hi) = (x0.min(x1), x0.max(x1));
                    if let Some(last) = segments.last_mut() {
                        let contiguous = lo <= last.1 + 0.5 && hi >= last.0 - 0.5;
                        if last.2 == c.direction && contiguous {
                            last.0 = last.0.min(lo);
                            last.1 = last.1.max(hi);
                            continue;
                        }
                    }
                    segments.push((lo, hi, c.direction));
                }

                if segments.is_empty() {
                    // No glyph-bearing clusters (e.g. zero-advance selection):
                    // fall back to the endpoint span so a caret-width rect still shows.
                    let start_x = get_cursor_x(start_item, start_cursor.affinity);
                    let end_x = get_cursor_x(end_item, end_cursor.affinity);
                    rects.push(LogicalRect {
                        origin: LogicalPosition {
                            x: start_x.min(end_x),
                            y: line_bounds.origin.y,
                        },
                        size: LogicalSize {
                            width: (end_x - start_x).abs(),
                            height: line_bounds.size.height,
                        },
                    });
                } else {
                    for (lo, hi, _dir) in segments {
                        rects.push(LogicalRect {
                            origin: LogicalPosition {
                                x: lo,
                                y: line_bounds.origin.y,
                            },
                            size: LogicalSize {
                                width: hi - lo,
                                height: line_bounds.size.height,
                            },
                        });
                    }
                }
            }
        }
        // 5. Handle multi-line selection.
        else {
            // Rectangle for the start line (from the start cursor to the line's end
            // in READING order). For an LTR line that is rightward (to the line's
            // right content edge); for an RTL line it is leftward (to the left edge).
            if let Some(start_line_bounds) = get_line_bounds(start_item.line_index) {
                let start_x = get_cursor_x(start_item, start_cursor.affinity);
                let line_left = start_line_bounds.origin.x;
                let line_right = start_line_bounds.origin.x + start_line_bounds.size.width;
                let rtl = start_item
                    .item
                    .as_cluster()
                    .is_some_and(|c| c.direction.is_rtl());
                let (lo, hi) = if rtl {
                    (line_left, start_x)
                } else {
                    (start_x, line_right)
                };
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: lo,
                        y: start_line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: hi - lo,
                        height: start_line_bounds.size.height,
                    },
                });
            }

            // Rectangles for all full lines in between.
            for line_idx in (start_item.line_index + 1)..end_item.line_index {
                if let Some(line_bounds) = get_line_bounds(line_idx) {
                    rects.push(line_bounds);
                }
            }

            // Rectangle for the end line (from the line's start in READING order to
            // the end cursor). For an LTR line that starts at the left content edge;
            // for an RTL line it starts at the right edge.
            if let Some(end_line_bounds) = get_line_bounds(end_item.line_index) {
                let end_x = get_cursor_x(end_item, end_cursor.affinity);
                let line_left = end_line_bounds.origin.x;
                let line_right = end_line_bounds.origin.x + end_line_bounds.size.width;
                let rtl = end_item
                    .item
                    .as_cluster()
                    .is_some_and(|c| c.direction.is_rtl());
                let (lo, hi) = if rtl {
                    (end_x, line_right)
                } else {
                    (line_left, end_x)
                };
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: lo,
                        y: end_line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: hi - lo,
                        height: end_line_bounds.size.height,
                    },
                });
            }
        }

        rects
    }

    /// Calculates the visual rectangle for a cursor at a given logical position.
    #[must_use]
    pub fn get_cursor_rect(&self, cursor: &TextCursor) -> Option<LogicalRect> {
        // Find the item and glyph corresponding to the cursor's cluster ID.
        let mut last_cluster: Option<(&PositionedItem, &ShapedCluster)> = None;
        for item in &self.items {
            if let ShapedItem::Cluster(cluster) = &item.item {
                if cluster.source_cluster_id == cursor.cluster_id {
                    // Exact match
                    let line_height = item.item.bounds().height;
                    // The logical-start (Leading) caret edge is the glyph's LEFT side for
                    // an LTR cluster but its RIGHT side for an RTL cluster; Trailing is the
                    // mirror. Resolve the edges from the cluster's own bidi direction.
                    let (lead_x, trail_x) = if cluster.direction.is_rtl() {
                        (item.position.x + cluster.advance, item.position.x)
                    } else {
                        (item.position.x, item.position.x + cluster.advance)
                    };
                    let cursor_x = match cursor.affinity {
                        CursorAffinity::Leading => lead_x,
                        CursorAffinity::Trailing => trail_x,
                    };
                    return Some(LogicalRect {
                        origin: LogicalPosition {
                            x: cursor_x,
                            y: item.position.y,
                        },
                        size: LogicalSize {
                            width: 1.0,
                            height: line_height,
                        },
                    });
                }
                last_cluster = Some((item, cluster));
            }
        }
        // Cursor past end of text: position after the last cluster
        if let Some((item, cluster)) = last_cluster {
            if cursor.cluster_id.source_run == cluster.source_cluster_id.source_run
                && cursor.cluster_id.start_byte_in_run
                    >= cluster.source_cluster_id.start_byte_in_run
            {
                let line_height = item.item.bounds().height;
                // Past the logical end of the run: the caret sits after the last cluster,
                // which is its RIGHT edge for LTR but its LEFT edge for RTL.
                let past_end_x = if cluster.direction.is_rtl() {
                    item.position.x
                } else {
                    item.position.x + cluster.advance
                };
                return Some(LogicalRect {
                    origin: LogicalPosition {
                        x: past_end_x,
                        y: item.position.y,
                    },
                    size: LogicalSize {
                        width: 1.0,
                        height: line_height,
                    },
                });
            }
        }
        // A cursor on a run that shaped to NO cluster: the EMPTY Text run an
        // Enter splices in after a hard break — `[.., LineBreak, Text("")]`
        // at the document end, or `[.., LineBreak, Text(""), LineBreak, ..]`
        // between two. The caret stands at the START of the line after that
        // break. No line box materializes for a content-less line (CSS
        // removes empty line boxes, and non-editable `pre` text must not
        // grow), so the rect is synthesized: the break's own `line_index`
        // names the line the caret follows, and the uniform line advance of
        // the layout places it. Empty runs are only ever produced by the
        // plain-text edit path, whose hosts carry one style, so the
        // first-top + n×line-height arithmetic is exact there.
        let cursor_run = cursor.cluster_id.source_run;
        let run_has_clusters = self.items.iter().any(|it| {
            matches!(&it.item, ShapedItem::Cluster(c)
                if c.source_cluster_id.source_run == cursor_run)
        });
        if run_has_clusters {
            return None;
        }
        let caret_line = self
            .items
            .iter()
            .filter_map(|it| match &it.item {
                ShapedItem::Break { source, .. } if source.run_index < cursor_run => {
                    Some(it.line_index + 1)
                }
                _ => None,
            })
            .max()?;
        let mut first_top: Option<f32> = None;
        let mut line_height: f32 = 0.0;
        for it in &self.items {
            if let ShapedItem::Cluster(_) = &it.item {
                let b = it.item.bounds();
                first_top = Some(first_top.map_or(it.position.y, |t: f32| t.min(it.position.y)));
                line_height = line_height.max(b.height);
            }
        }
        let first_top = first_top?;
        if line_height <= 0.0 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)] // line counts are small
        Some(LogicalRect {
            origin: LogicalPosition {
                x: 0.0,
                y: first_top + caret_line as f32 * line_height,
            },
            size: LogicalSize {
                width: 1.0,
                height: line_height,
            },
        })
    }

    /// Get a cursor at the first cluster (leading edge) in the layout.
    #[must_use]
    pub fn get_first_cluster_cursor(&self) -> Option<TextCursor> {
        for item in &self.items {
            if let ShapedItem::Cluster(cluster) = &item.item {
                return Some(TextCursor {
                    cluster_id: cluster.source_cluster_id,
                    affinity: CursorAffinity::Leading,
                });
            }
        }
        None
    }

    /// Get a cursor at the last cluster (trailing edge) in the layout.
    #[must_use]
    pub fn get_last_cluster_cursor(&self) -> Option<TextCursor> {
        for item in self.items.iter().rev() {
            if let ShapedItem::Cluster(cluster) = &item.item {
                return Some(TextCursor {
                    cluster_id: cluster.source_cluster_id,
                    affinity: CursorAffinity::Trailing,
                });
            }
        }
        None
    }

    /// Logical sequence of caret-stop grapheme clusters, sorted by
    /// `(source_run, start_byte_in_run)` and de-duplicated, with combining-mark
    /// continuations folded into their base (UAX#29). Left/right caret motion
    /// advances over THIS sequence so a base and its combining marks move as one
    /// unit, and so the document start/end are always reachable.
    #[doc(hidden)] // pub for the dense-equivalence gate only
    #[must_use]
    pub fn grapheme_stops(&self) -> Vec<GraphemeClusterId> {
        let mut stops: Vec<(GraphemeClusterId, &str)> = self
            .items
            .iter()
            .filter_map(|it| {
                it.item
                    .as_cluster()
                    .map(|c| (c.source_cluster_id, c.text()))
            })
            .collect();
        stops.sort_by(|a, b| {
            (a.0.source_run, a.0.start_byte_in_run).cmp(&(b.0.source_run, b.0.start_byte_in_run))
        });
        stops.dedup_by_key(|(id, _)| *id);
        stops
            .into_iter()
            .filter(|(_, text)| !Self::cluster_is_grapheme_continuation(text))
            .map(|(id, _)| id)
            .collect()
    }

    /// True if `text`'s leading char is a grapheme extender (combining mark,
    /// variation selector, …) — a cluster that merges into a preceding base and
    /// therefore must not be a standalone caret stop (UAX#29).
    pub(super) fn cluster_is_grapheme_continuation(text: &str) -> bool {
        let Some(first) = text.chars().next() else {
            return false;
        };
        // Probe with a dummy base letter: if `x` + first collapses to a single
        // grapheme, `first` extends the preceding grapheme.
        let mut probe = String::with_capacity(1 + first.len_utf8());
        probe.push('x');
        probe.push(first);
        probe.graphemes(true).count() == 1
    }

    /// Caret offset of `cursor` within `stops` (0..=len): the index of its
    /// grapheme, plus 1 for a Trailing affinity. A cursor addressing a folded
    /// combining mark (or otherwise between stops) maps to the nearest preceding
    /// stop.
    #[doc(hidden)] // pub for the dense movement twins (stops-only logic)
    #[must_use]
    pub fn grapheme_caret_offset(
        stops: &[GraphemeClusterId],
        cursor: &TextCursor,
    ) -> Option<usize> {
        let trailing = usize::from(cursor.affinity == CursorAffinity::Trailing);
        if let Some(idx) = stops.iter().position(|id| *id == cursor.cluster_id) {
            return Some(idx + trailing);
        }
        let key = (
            cursor.cluster_id.source_run,
            cursor.cluster_id.start_byte_in_run,
        );
        let idx = stops
            .iter()
            .rposition(|id| (id.source_run, id.start_byte_in_run) <= key)?;
        Some(idx + trailing)
    }

    /// [`Self::grapheme_caret_offset`] for a caret that may stand where NO
    /// cluster starts (`is_cluster` says which ids are clusters of the
    /// layout): past the last stop that is the END of the text - `(len,
    /// Leading)`, what a typed insertion leaves - so the offset after it. A
    /// caret on a folded mark (a cluster that is no stop) still snaps back to
    /// its grapheme. Read as the last stop, one Shift+Left from the end of a
    /// typed "krug" selected "ug" (E2E-A, 2026-10-06).
    #[doc(hidden)] // pub for the dense movement twins
    pub fn grapheme_caret_offset_in(
        stops: &[GraphemeClusterId],
        cursor: &TextCursor,
        is_cluster: &dyn Fn(&GraphemeClusterId) -> bool,
    ) -> Option<usize> {
        let offset = Self::grapheme_caret_offset(stops, cursor)?;
        let on_a_stop = stops.contains(&cursor.cluster_id);
        if !on_a_stop && !is_cluster(&cursor.cluster_id) {
            let key = (cursor.cluster_id.source_run, cursor.cluster_id.start_byte_in_run);
            let idx = stops
                .iter()
                .rposition(|id| (id.source_run, id.start_byte_in_run) <= key)?;
            return Some((idx + 1).min(stops.len()));
        }
        Some(offset)
    }

    /// Canonical cursor for a grapheme-stop `offset` (0..=len): interior/first
    /// offsets are the Leading edge of the stop that begins there; `len` is the
    /// Trailing edge of the last stop (the document end).
    #[doc(hidden)] // pub for the dense movement twins (stops-only logic)
    #[must_use]
    pub fn cursor_from_grapheme_offset(stops: &[GraphemeClusterId], offset: usize) -> TextCursor {
        let n = stops.len();
        if offset >= n {
            TextCursor {
                cluster_id: stops[n - 1],
                affinity: CursorAffinity::Trailing,
            }
        } else {
            TextCursor {
                cluster_id: stops[offset],
                affinity: CursorAffinity::Leading,
            }
        }
    }

    /// Moves a cursor one visible position to the left (the previous grapheme
    /// boundary). Affinity is consulted so each press moves exactly one stop and
    /// the document start (first grapheme, Leading) is reachable; combining marks
    /// move together with their base.
    pub fn move_cursor_left(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        let stops = self.grapheme_stops();
        if stops.is_empty() {
            return cursor;
        }
        let is_cluster = |id: &GraphemeClusterId| {
            self.items
                .iter()
                .any(|it| it.item.as_cluster().is_some_and(|c| c.source_cluster_id == *id))
        };
        let Some(offset) = Self::grapheme_caret_offset_in(&stops, &cursor, &is_cluster) else {
            return cursor;
        };
        let moved = Self::cursor_from_grapheme_offset(&stops, offset.saturating_sub(1));
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: byte {} -> byte {}",
                cursor.cluster_id.start_byte_in_run, moved.cluster_id.start_byte_in_run
            ));
        }
        moved
    }

    /// Moves a cursor one visible position to the right (the next grapheme
    /// boundary). Affinity is consulted so each press moves exactly one stop and
    /// the document end (last grapheme, Trailing) is reachable; combining marks
    /// move together with their base.
    pub fn move_cursor_right(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        let stops = self.grapheme_stops();
        if stops.is_empty() {
            return cursor;
        }
        let is_cluster = |id: &GraphemeClusterId| {
            self.items
                .iter()
                .any(|it| it.item.as_cluster().is_some_and(|c| c.source_cluster_id == *id))
        };
        let Some(offset) = Self::grapheme_caret_offset_in(&stops, &cursor, &is_cluster) else {
            return cursor;
        };
        let moved = Self::cursor_from_grapheme_offset(&stops, (offset + 1).min(stops.len()));
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: byte {} -> byte {}",
                cursor.cluster_id.start_byte_in_run, moved.cluster_id.start_byte_in_run
            ));
        }
        moved
    }

    /// Moves a cursor up one line, attempting to preserve the horizontal column.
    pub fn move_cursor_up(
        &self,
        cursor: TextCursor,
        goal_x: &mut Option<f32>,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: from byte {} (affinity {:?})",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_up: cursor not found in items, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let target_line_idx = current_item.line_index.saturating_sub(1);
        if current_item.line_index == target_line_idx {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_up: already at top line {}, staying put",
                    current_item.line_index
                ));
            }
            return cursor;
        }

        let current_x = goal_x.unwrap_or_else(|| {
            let x = match cursor.affinity {
                CursorAffinity::Leading => current_item.position.x,
                CursorAffinity::Trailing => {
                    current_item.position.x + get_item_measure(&current_item.item, false)
                }
            };
            *goal_x = Some(x);
            x
        });

        // Find the Y coordinate of the middle of the target line
        let target_y = self
            .items
            .iter()
            .find(|i| i.line_index == target_line_idx)
            .map_or(current_item.position.y, |i| {
                i.position.y + (i.item.bounds().height / 2.0)
            });

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: target line {target_line_idx}, hittesting at \
                 ({current_x}, {target_y})"
            ));
        }

        let result = self
            .hittest_cursor(LogicalPosition {
                x: current_x,
                y: target_y,
            })
            .unwrap_or(cursor);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: result byte {} (affinity {:?})",
                result.cluster_id.start_byte_in_run, result.affinity
            ));
        }

        result
    }

    /// Moves a cursor down one line, attempting to preserve the horizontal column.
    pub fn move_cursor_down(
        &self,
        cursor: TextCursor,
        goal_x: &mut Option<f32>,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: from byte {} (affinity {:?})",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_down: cursor not found in items, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let max_line = self.items.iter().map(|i| i.line_index).max().unwrap_or(0);
        let target_line_idx = (current_item.line_index + 1).min(max_line);
        if current_item.line_index == target_line_idx {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_down: already at bottom line {}, staying put",
                    current_item.line_index
                ));
            }
            return cursor;
        }

        let current_x = goal_x.unwrap_or_else(|| {
            let x = match cursor.affinity {
                CursorAffinity::Leading => current_item.position.x,
                CursorAffinity::Trailing => {
                    current_item.position.x + get_item_measure(&current_item.item, false)
                }
            };
            *goal_x = Some(x);
            x
        });

        let target_y = self
            .items
            .iter()
            .find(|i| i.line_index == target_line_idx)
            .map_or(current_item.position.y, |i| {
                i.position.y + (i.item.bounds().height / 2.0)
            });

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: hit testing at ({current_x}, {target_y})"
            ));
        }

        let result = self
            .hittest_cursor(LogicalPosition {
                x: current_x,
                y: target_y,
            })
            .unwrap_or(cursor);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: result byte {}, affinity {:?}",
                result.cluster_id.start_byte_in_run, result.affinity
            ));
        }

        result
    }

    /// Moves a cursor to the visual start of its current line.
    pub fn move_cursor_to_line_start(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_to_line_start: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let first_item_on_line = self
            .items
            .iter()
            .filter(|i| i.line_index == current_item.line_index)
            .min_by(|a, b| {
                a.position
                    .x
                    .partial_cmp(&b.position.x)
                    .unwrap_or(Ordering::Equal)
            });

        if let Some(item) = first_item_on_line {
            if let ShapedItem::Cluster(c) = &item.item {
                let result = TextCursor {
                    cluster_id: c.source_cluster_id,
                    affinity: CursorAffinity::Leading,
                };
                if let Some(d) = debug {
                    d.push(format!(
                        "[Cursor] move_cursor_to_line_start: result byte {}, affinity {:?}",
                        result.cluster_id.start_byte_in_run, result.affinity
                    ));
                }
                return result;
            }
        }

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: no first item found, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor to the visual end of its current line.
    pub fn move_cursor_to_line_end(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_to_line_end: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let last_item_on_line = self
            .items
            .iter()
            .filter(|i| i.line_index == current_item.line_index)
            .max_by(|a, b| {
                a.position
                    .x
                    .partial_cmp(&b.position.x)
                    .unwrap_or(Ordering::Equal)
            });

        if let Some(item) = last_item_on_line {
            if let ShapedItem::Cluster(c) = &item.item {
                let result = TextCursor {
                    cluster_id: c.source_cluster_id,
                    affinity: CursorAffinity::Trailing,
                };
                if let Some(d) = debug {
                    d.push(format!(
                        "[Cursor] move_cursor_to_line_end: result byte {}, affinity {:?}",
                        result.cluster_id.start_byte_in_run, result.affinity
                    ));
                }
                return result;
            }
        }

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: no last item found, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor one word to the left (Ctrl+Left / Option+Left).
    ///
    /// Word boundaries use the shared [`is_word_char`] predicate (alphanumeric or
    /// underscore are word characters; whitespace AND punctuation are boundaries),
    /// so this agrees with double-click word selection. The cursor moves past any
    /// boundary clusters to the left, then past word clusters until the next
    /// boundary or start of text.
    pub fn move_cursor_to_prev_word(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_prev_word: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_pos) = self.items.iter().position(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            return cursor;
        };

        // Phase 1: Skip whitespace going left
        let mut pos = if cursor.affinity == CursorAffinity::Leading {
            // Already at leading edge, start from previous item
            current_pos.checked_sub(1)
        } else {
            // At trailing edge, start from current item
            Some(current_pos)
        };

        // Skip boundary clusters (whitespace + punctuation)
        while let Some(p) = pos {
            if let Some(cluster) = self.items[p].item.as_cluster() {
                if !cluster_is_word_boundary(cluster) {
                    break;
                }
            }
            pos = p.checked_sub(1);
        }

        // Phase 2: Skip word clusters going left (the word itself)
        while let Some(p) = pos {
            if let Some(cluster) = self.items[p].item.as_cluster() {
                if cluster_is_word_boundary(cluster) {
                    // We've reached a boundary before the word — stop at next cluster
                    if p + 1 < self.items.len() {
                        if let Some(c) = self.items[p + 1].item.as_cluster() {
                            return TextCursor {
                                cluster_id: c.source_cluster_id,
                                affinity: CursorAffinity::Leading,
                            };
                        }
                    }
                    break;
                }
            }
            if p == 0 {
                // Reached start of text — return first cluster
                if let Some(c) = self.items[0].item.as_cluster() {
                    return TextCursor {
                        cluster_id: c.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    };
                }
                break;
            }
            pos = p.checked_sub(1);
        }

        // If we exhausted the search, go to first cluster
        if pos.is_none() {
            if let Some(first) = self.get_first_cluster_cursor() {
                return first;
            }
        }

        cursor
    }

    /// Moves a cursor one word to the right (Ctrl+Right / Option+Right).
    ///
    /// Word boundaries use the shared [`is_word_char`] predicate (alphanumeric or
    /// underscore are word characters; whitespace AND punctuation are boundaries),
    /// so this agrees with double-click word selection. The cursor moves past any
    /// word clusters, then past boundary clusters until the next word or end of text.
    pub fn move_cursor_to_next_word(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_next_word: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_pos) = self.items.iter().position(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            return cursor;
        };

        let len = self.items.len();

        // Start position: if at leading edge, start from current; if trailing, start from next
        let start = if cursor.affinity == CursorAffinity::Trailing {
            current_pos + 1
        } else {
            current_pos
        };

        if start >= len {
            return cursor;
        }

        let mut pos = start;

        // Phase 1: Skip word clusters (current word)
        while pos < len {
            if let Some(cluster) = self.items[pos].item.as_cluster() {
                if cluster_is_word_boundary(cluster) {
                    break;
                }
            }
            pos += 1;
        }

        // Phase 2: Skip boundary clusters (whitespace + punctuation) after word
        while pos < len {
            if let Some(cluster) = self.items[pos].item.as_cluster() {
                if !cluster_is_word_boundary(cluster) {
                    // Found start of next word
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    };
                }
            }
            pos += 1;
        }

        // Reached end of text
        if let Some(last) = self.get_last_cluster_cursor() {
            return last;
        }

        cursor
    }
}

/// Where `positioned`'s baseline lies in its layout (from the top of the
/// IFC's content box): its top - the line put it at `baseline - ascent` - plus
/// the ascent the line used (`get_item_vertical_metrics`: a glyph run's with
/// its half-leading; an atomic inline's is its height above its
/// `baseline_offset`, which counts from its bottom edge). `None` for what has
/// no baseline: a break, a tab, a cluster without glyphs.
pub(super) fn baseline_in_layout(positioned: &PositionedItem) -> Option<f32> {
    let ascent = match &positioned.item {
        ShapedItem::Cluster(c) if c.glyphs.is_empty() => return None,
        ShapedItem::Object {
            bounds,
            baseline_offset,
            ..
        } => bounds.height - *baseline_offset,
        ShapedItem::Cluster(_) | ShapedItem::CombinedBlock { .. } => {
            get_item_vertical_metrics_approx(&positioned.item).0
        }
        ShapedItem::Break { .. } | ShapedItem::Tab { .. } => return None,
    };
    Some(positioned.position.y + ascent)
}

/// Stores information about content that exceeded the available layout space.
#[derive(Debug, Clone, Default)]
pub struct OverflowInfo {
    /// The items that did not fit within the constraints.
    ///
    /// Currently always empty: the positioners place every item (visual overflow
    /// is clipped at paint time) rather than dropping content, so nothing is ever
    /// recorded here. The `window.rs` incremental-patch guard reads
    /// `overflow_items.is_empty()` to stay future-proof against a positioning path
    /// that *does* drop items. TODO(superplan): populate this if such a path lands.
    pub overflow_items: Vec<ShapedItem>,
    /// The total bounds of all positioned content, including any that overflows
    /// the constraints. Populated by both positioners (greedy + Knuth-Plass) from
    /// [`UnifiedLayout::bounds`]; useful for `OverflowBehavior::Visible`/`Scroll`.
    pub unclipped_bounds: Rect,
}

impl OverflowInfo {
    #[must_use]
    pub const fn has_overflow(&self) -> bool {
        !self.overflow_items.is_empty()
    }
}

/// Intermediate structure carrying information from the line breaker to the positioner.
#[derive(Debug, Clone)]
pub struct UnifiedLine {
    pub items: Vec<ShapedItem>,
    /// The y-position (for horizontal) or x-position (for vertical) of the line's baseline.
    pub cross_axis_position: f32,
    /// The geometric segments this line must fit into.
    pub constraints: LineConstraints,
    pub is_last: bool,
}

/// Defines a single area for layout, with its own shape and properties.
#[derive(Debug, Clone)]
pub struct LayoutFragment {
    /// A unique identifier for this fragment (e.g., "main-content", "sidebar").
    pub id: String,
    /// The geometric and style constraints for this specific fragment.
    pub constraints: UnifiedConstraints,
}

/// Represents the final layout distributed across multiple fragments.
#[derive(Debug, Clone)]
pub struct FlowLayout {
    /// A map from a fragment's unique ID to the layout it contains.
    pub fragment_layouts: HashMap<String, Arc<UnifiedLayout>>,
    /// Any items that did not fit into the last fragment in the flow chain.
    /// This is useful for pagination or determining if more layout space is needed.
    pub remaining_items: Vec<ShapedItem>,
}

/// Inline-axis intrinsic contributions derived from shaped text, without running
/// the line-breaking stage of the pipeline.
///
/// Callers that only need min/max-content widths for sizing (see
/// `calculate_ifc_root_intrinsic_sizes`) should prefer this over invoking
/// `layout_flow` twice with `AvailableSpace::MinContent`/`MaxContent`. The
/// latter runs the full flow loop — including `BreakCursor::peek_next_unit`,
/// which clones every `ShapedCluster` it inspects — even though no constraint
/// actually limits the line width.
#[derive(Copy, Debug, Clone, Default)]
pub struct IntrinsicTextSizes {
    /// CSS min-content = widest unbreakable unit (word) along the inline axis.
    pub min_content_width: f32,
    /// CSS max-content = sum of all advances along the inline axis (single line).
    pub max_content_width: f32,
    /// Height of a single line box: max(ascent + descent) across all items.
    pub max_content_height: f32,
}

/// Cached line break boundaries from a previous layout pass.
///
/// Enables incremental relayout: when a word changes width,
/// we can check if it still fits on the same line without
/// re-running the full line-breaking algorithm.
#[derive(Clone, Debug)]
pub struct CachedLineBreaks {
    /// Per-line: (`first_item_idx`, `last_item_idx_exclusive`) into positioned items.
    pub line_ranges: Vec<(usize, usize)>,
    /// Per-line total width (sum of item advances on that line).
    pub line_widths: Vec<f32>,
    /// The available width constraint used when these breaks were computed.
    pub available_width: f32,
}

/// Result of an incremental relayout attempt.
#[derive(Copy, Clone, Debug)]
pub enum IncrementalRelayoutResult {
    /// Glyphs changed but advance widths identical — swap in place, no repositioning.
    GlyphSwap,
    /// Width changed but still fits on same line — shift `x_offsets` of subsequent items.
    LineShift {
        /// Index of the first affected item.
        affected_item: usize,
        /// Width delta (`new_advance` - `old_advance`).
        delta: f32,
    },
    /// Line breaks changed — need to reflow from this line onward.
    PartialReflow {
        /// The line index from which to start reflowing.
        reflow_from_line: usize,
    },
    /// Cannot do incremental — fall back to full relayout.
    FullRelayout,
}

/// Extract line break boundaries from a positioned items list.
#[must_use]
pub fn extract_line_breaks(items: &[PositionedItem], available_width: f32) -> CachedLineBreaks {
    let mut line_ranges = Vec::new();
    let mut line_widths = Vec::new();

    if items.is_empty() {
        return CachedLineBreaks {
            line_ranges,
            line_widths,
            available_width,
        };
    }

    let mut line_start = 0usize;
    let mut current_line = items[0].line_index;
    let mut line_width = 0.0f32;

    for (i, item) in items.iter().enumerate() {
        if item.line_index != current_line {
            line_ranges.push((line_start, i));
            line_widths.push(line_width);
            line_start = i;
            current_line = item.line_index;
            line_width = 0.0;
        }
        line_width += get_item_measure(&item.item, false);
    }

    // Final line
    line_ranges.push((line_start, items.len()));
    line_widths.push(line_width);

    CachedLineBreaks {
        line_ranges,
        line_widths,
        available_width,
    }
}

/// Attempt incremental relayout given old metrics and new per-item advance widths.
///
/// `dirty_item_indices`: which items in the shaped list changed.
/// `old_advances`: per-item advance widths from the previous layout.
/// `new_advances`: per-item advance widths after reshaping.
/// `line_breaks`: cached line boundaries from previous layout.
#[must_use]
pub fn try_incremental_relayout(
    dirty_item_indices: &[usize],
    old_advances: &[f32],
    new_advances: &[f32],
    line_breaks: &CachedLineBreaks,
) -> IncrementalRelayoutResult {
    if dirty_item_indices.is_empty() {
        return IncrementalRelayoutResult::GlyphSwap;
    }

    // `LineShift` moves everything after ONE pivot by ONE delta, so it is
    // only sound when EXACTLY one dirty item changed width. Deciding on the
    // first changed item (the old shape of this loop) turned a whole-run
    // restyle — every advance different — into a single-delta shift that
    // pasted the new glyphs onto the old pen positions, compressing the
    // line into overlapping strokes.
    let mut changed: Option<(usize, f32)> = None;
    for &dirty_idx in dirty_item_indices {
        if dirty_idx >= old_advances.len() || dirty_idx >= new_advances.len() {
            return IncrementalRelayoutResult::FullRelayout;
        }

        let delta = new_advances[dirty_idx] - old_advances[dirty_idx];
        if delta.abs() < 0.001 {
            // Same width — just swap glyphs (GlyphSwap for this item)
            continue;
        }
        if changed.is_some() {
            // A second width change: one delta cannot describe the line.
            return IncrementalRelayoutResult::FullRelayout;
        }
        changed = Some((dirty_idx, delta));
    }

    let Some((dirty_idx, delta)) = changed else {
        // All dirty items had same width
        return IncrementalRelayoutResult::GlyphSwap;
    };

    // Width changed — find which line this item is on
    let line_idx = line_breaks
        .line_ranges
        .iter()
        .position(|&(start, end)| dirty_idx >= start && dirty_idx < end);

    let Some(line_idx) = line_idx else {
        return IncrementalRelayoutResult::FullRelayout;
    };

    let old_line_width = line_breaks.line_widths[line_idx];
    let new_line_width = old_line_width + delta;

    if new_line_width <= line_breaks.available_width {
        // Still fits on same line — shift subsequent items
        IncrementalRelayoutResult::LineShift {
            affected_item: dirty_idx,
            delta,
        }
    } else {
        // Overflows line — need to reflow from this line
        IncrementalRelayoutResult::PartialReflow {
            reflow_from_line: line_idx,
        }
    }
}
