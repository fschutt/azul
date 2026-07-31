//! Page-break analysis as a standalone, pure computation.
//!
//! Extracted from `display_list.rs::calculate_page_break_positions` so that
//! embedders (document editors, printpdf) can compute pagination *without*
//! generating any per-page display list, and so the slicer becomes a consumer
//! of the same analysis it used to inline.
//!
//! Three latent defects were fixed in the extraction (each pinned by a test):
//!
//! 1. the sort now uses `f32::total_cmp` — the old `partial_cmp().unwrap()`
//!    was a panic path if a NaN ever survived the input filter;
//! 2. when a forced break and an interval break land within the 1px merge
//!    window, the FORCED break survives (CSS Fragmentation: forced breaks
//!    win). The old positional dedup kept whichever sorted first, so an
//!    author's `break-before: always` could be silently replaced by the
//!    interval break up to 1px above it;
//! 3. `normal_page_content_height <= 0` (header + footer at least as tall as
//!    the page, with `skip_first_page` making the first page valid) made the
//!    old interval loop `y += normal` never terminate. Interval generation now
//!    stops after the first-page break when the normal height is not positive.

use azul_core::dom::NodeId;

use crate::solver3::display_list::{
    calculate_display_list_height, DisplayList, SlicerConfig,
};

/// Why a page ends where it does.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C, u8)]
pub enum BreakKind {
    /// A CSS `break-before/after: always` (or legacy `page-break-*`) forced
    /// this break.
    Forced,
    /// The page was simply full (regular interval break).
    Interval,
    /// The page was full at `pushed_from`, but the break moved UP to honor an
    /// avoid-rule (`break-inside: avoid`, line atomicity, widows/orphans).
    Avoided { pushed_from: f32 },
}

/// One page boundary in document space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageBreak {
    /// Document-space Y where the page ENDS (content at or below `y` belongs
    /// to the next page).
    pub y: f32,
    pub kind: BreakKind,
    /// For [`BreakKind::Forced`]: the node whose break property caused it,
    /// when known. `None` in the display-list-only path — the display list
    /// records only the Y positions of forced breaks.
    pub causing_node: Option<NodeId>,
}

/// Page geometry the break computation needs.
///
/// [`PageConstraints::from_slicer_config`] performs the header/footer
/// subtraction that used to be inlined in
/// `paginate_display_list_with_slicer_and_breaks`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageConstraints {
    /// Content height available on the first page (differs from
    /// `normal_page_content_height` when headers/footers skip the first page).
    pub first_page_content_height: f32,
    /// Content height available on every page after the first.
    pub normal_page_content_height: f32,
}

impl PageConstraints {
    /// Derive the per-page content heights from a slicer config: subtract
    /// header/footer space, and give the first page the full height when
    /// `skip_first_page` is set.
    #[must_use]
    pub fn from_slicer_config(cfg: &SlicerConfig) -> Self {
        let base_header_space = if cfg.header_footer.show_header {
            cfg.header_footer.header_height
        } else {
            0.0
        };
        let base_footer_space = if cfg.header_footer.show_footer {
            cfg.header_footer.footer_height
        } else {
            0.0
        };
        let normal_page_content_height =
            cfg.page_content_height - base_header_space - base_footer_space;
        let first_page_content_height = if cfg.header_footer.skip_first_page {
            // First page has full height when skipping headers/footers
            cfg.page_content_height
        } else {
            normal_page_content_height
        };
        Self {
            first_page_content_height,
            normal_page_content_height,
        }
    }
}

/// Breaks within this distance are the same boundary and are merged — a
/// duplicate would produce a zero-height page.
const MERGE_WINDOW_PX: f32 = 1.0;

/// Break-awareness policy. ALL flags off (the default) reproduces the plain
/// forced ∪ interval algorithm exactly — that is how this type can ship ahead
/// of the behaviors it gates (each lands in its own stage and flips on in
/// printpdf with a changelog entry, never silently).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreakPolicy {
    /// Honor `break-inside: avoid` (push boxes below the break intact).
    pub honor_break_inside: bool,
    /// Honor `widows` / `orphans` line constraints.
    pub widows_orphans: bool,
    /// Never tear a line box across pages (snap to line boundaries).
    pub atomic_lines: bool,
    /// Never tear a table row across pages.
    pub atomic_table_rows: bool,
    /// Repeat `<thead>` on continuation pages.
    pub repeat_table_headers: bool,
    /// Upper bound on how far a break may be pushed UP to satisfy
    /// avoid-rules, as a fraction of the page height (guards pathological
    /// cascades; beyond it the plain candidate snap applies).
    pub max_push_distance: f32,
}

impl Default for BreakPolicy {
    fn default() -> Self {
        Self {
            honor_break_inside: false,
            widows_orphans: false,
            atomic_lines: false,
            atomic_table_rows: false,
            repeat_table_headers: false,
            max_push_distance: 0.33,
        }
    }
}

/// The richer inputs break-awareness needs (geometry beyond the display
/// list: box rects and break properties). The display-list-only path stays
/// available via [`compute_page_breaks_from_display_list`].
pub struct PageBreakInput<'a> {
    /// Item geometry + forced break positions.
    pub display_list: &'a DisplayList,
    /// Box rects + line boxes for future break-candidate stages. v1
    /// break-awareness derives all geometry from the display list, so this
    /// may be `None`.
    pub layout_tree: Option<&'a crate::solver3::layout_tree::LayoutTree>,
    /// Break properties (`break-inside`, `widows`, `orphans`, …).
    pub styled_dom: &'a azul_core::styled_dom::StyledDom,
}

/// A vertical range a break may not enter, with the Y to snap up to
/// (`top`) when one lands inside.
#[derive(Debug, Clone, Copy)]
struct AvoidRange {
    top: f32,
    bottom: f32,
    /// The node that owns the range (diagnostics).
    node: Option<NodeId>,
}

/// The line boxes of one paragraph (for widows/orphans), sorted by top.
#[derive(Debug, Clone)]
struct ParagraphLines {
    node: NodeId,
    /// `(top, bottom)` of each line, sorted by top, deduplicated.
    lines: Vec<(f32, f32)>,
    widows: u32,
    orphans: u32,
}

/// Compute page breaks with break-awareness `policy`. With the default
/// (all-off) policy this is exactly [`compute_page_breaks_from_display_list`].
///
/// With flags on, interval breaks run as a single FORWARD pass (matching the
/// slicer's "breaks only move up, content never moves" model): the naive
/// break `prev + page_height` snaps UP out of avoid-ranges
/// (`break-inside: avoid` boxes, atomic line boxes) and back for
/// widows/orphans, bounded by `policy.max_push_distance` (beyond it the
/// naive break stands — a bounded fallback, never a loop). Forced breaks
/// never move. Boxes taller than the page are torn regardless (the monolith
/// rule — an unbreakable box that cannot fit must still paginate).
#[must_use]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
pub fn compute_page_breaks(
    input: &PageBreakInput,
    constraints: &PageConstraints,
    policy: &BreakPolicy,
) -> Vec<PageBreak> {
    let any_awareness =
        policy.honor_break_inside || policy.atomic_lines || policy.widows_orphans;
    if !any_awareness {
        return compute_page_breaks_from_display_list(input.display_list, constraints);
    }

    let total_height = calculate_display_list_height(input.display_list);
    let first = constraints.first_page_content_height;
    let normal = constraints.normal_page_content_height;
    if total_height <= 0.0 || first <= 0.0 {
        return Vec::new();
    }

    let avoid_ranges = collect_avoid_ranges(input, constraints, policy);
    let paragraphs = if policy.widows_orphans {
        collect_paragraph_lines(input)
    } else {
        Vec::new()
    };

    // Forced breaks, ascending (they are hard walls the forward pass emits
    // verbatim — CSS Fragmentation: forced always wins over avoid).
    let mut forced: Vec<f32> = input
        .display_list
        .forced_page_breaks
        .iter()
        .copied()
        .filter(|y| *y > 0.0 && *y < total_height)
        .collect();
    forced.sort_by(f32::total_cmp);

    let mut breaks: Vec<PageBreak> = Vec::new();
    let mut prev_end = 0.0_f32;
    let mut page_height = first;
    let mut forced_iter = forced.into_iter().peekable();

    loop {
        let naive = prev_end + page_height;

        // A forced break before (or at) the naive position ends the page there.
        if let Some(&fy) = forced_iter.peek() {
            if fy <= naive + MERGE_WINDOW_PX {
                forced_iter.next();
                if fy > prev_end + MERGE_WINDOW_PX {
                    breaks.push(PageBreak {
                        y: fy,
                        kind: BreakKind::Forced,
                        causing_node: None,
                    });
                    prev_end = fy;
                    page_height = normal;
                }
                continue;
            }
        }

        if naive >= total_height {
            break;
        }

        let max_push = (policy.max_push_distance.max(0.0)) * page_height;
        let floor = (naive - max_push).max(prev_end + MERGE_WINDOW_PX);
        let adjusted = snap_break_up(naive, floor, &avoid_ranges, &paragraphs, policy);

        let kind = if (adjusted - naive).abs() < f32::EPSILON {
            BreakKind::Interval
        } else {
            BreakKind::Avoided { pushed_from: naive }
        };
        breaks.push(PageBreak {
            y: adjusted,
            kind,
            causing_node: None,
        });
        prev_end = adjusted;
        page_height = normal;
        if !(normal > 0.0) {
            break;
        }
    }

    // Any forced breaks past the last interval position still apply.
    for fy in forced_iter {
        if fy > prev_end + MERGE_WINDOW_PX {
            breaks.push(PageBreak {
                y: fy,
                kind: BreakKind::Forced,
                causing_node: None,
            });
            prev_end = fy;
        }
    }

    breaks
}

/// Collect the vertical ranges a break may not enter.
fn collect_avoid_ranges(
    input: &PageBreakInput,
    constraints: &PageConstraints,
    policy: &BreakPolicy,
) -> Vec<AvoidRange> {
    use crate::solver3::getters::get_break_inside;
    use azul_css::props::layout::fragmentation::BreakInside;

    let mut ranges: Vec<AvoidRange> = Vec::new();
    let page_height = constraints.normal_page_content_height.max(1.0);

    if policy.honor_break_inside {
        // Union the bounds of every display item a break-inside:avoid node
        // produced (document-space Ys come from the DL — no positions map
        // needed). One range per node.
        let mut per_node: std::collections::BTreeMap<NodeId, (f32, f32)> =
            std::collections::BTreeMap::new();
        for (idx, item) in input.display_list.items.iter().enumerate() {
            let Some(node) = input.display_list.node_mapping.get(idx).copied().flatten() else {
                continue;
            };
            let Some(bounds) = item.bounds() else { continue };
            if get_break_inside(input.styled_dom, Some(node)) != BreakInside::Avoid {
                continue;
            }
            let top = bounds.origin.y;
            let bottom = bounds.origin.y + bounds.size.height;
            per_node
                .entry(node)
                .and_modify(|(t, b)| {
                    *t = t.min(top);
                    *b = b.max(bottom);
                })
                .or_insert((top, bottom));
        }
        for (node, (top, bottom)) in per_node {
            // Monolith rule: a box taller than the page may be torn — an
            // avoid-range that can never be satisfied would push forever.
            if bottom - top > page_height {
                continue;
            }
            ranges.push(AvoidRange {
                top,
                bottom,
                node: Some(node),
            });
        }
    }

    if policy.atomic_lines {
        // Every text item's rect is a line-box fragment; a break through one
        // slices a line (the "baseline-sliced line" artifact). Snap to its top.
        for item in &input.display_list.items {
            if let crate::solver3::display_list::DisplayListItem::Text {
                clip_rect, ..
            } = item
            {
                let r = clip_rect.inner();
                if r.size.height > 0.0 {
                    ranges.push(AvoidRange {
                        top: r.origin.y,
                        bottom: r.origin.y + r.size.height,
                        node: None,
                    });
                }
            }
        }
    }

    ranges.sort_by(|a, b| a.top.total_cmp(&b.top));
    ranges
}

/// Group text-item rects per source node into line boxes for widows/orphans.
fn collect_paragraph_lines(input: &PageBreakInput) -> Vec<ParagraphLines> {
    use crate::solver3::getters::{get_orphans, get_widows};

    let mut per_node: std::collections::BTreeMap<NodeId, Vec<(f32, f32)>> =
        std::collections::BTreeMap::new();
    for (idx, item) in input.display_list.items.iter().enumerate() {
        let crate::solver3::display_list::DisplayListItem::Text { clip_rect, .. } = item else {
            continue;
        };
        let Some(node) = input.display_list.node_mapping.get(idx).copied().flatten() else {
            continue;
        };
        let r = clip_rect.inner();
        if r.size.height <= 0.0 {
            continue;
        }
        per_node
            .entry(node)
            .or_default()
            .push((r.origin.y, r.origin.y + r.size.height));
    }

    per_node
        .into_iter()
        .filter_map(|(node, mut rects)| {
            rects.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Merge run-rects sharing a line (tops within 0.5px).
            let mut lines: Vec<(f32, f32)> = Vec::new();
            for (top, bottom) in rects {
                match lines.last_mut() {
                    Some((lt, lb)) if (top - *lt).abs() < 0.5 => *lb = lb.max(bottom),
                    _ => lines.push((top, bottom)),
                }
            }
            if lines.len() < 2 {
                return None; // single-line paragraphs have no widow/orphan case
            }
            Some(ParagraphLines {
                node,
                widows: get_widows(input.styled_dom, Some(node)).max(1),
                orphans: get_orphans(input.styled_dom, Some(node)).max(1),
                lines,
            })
        })
        .collect()
}

/// Snap a naive break Y up out of avoid-ranges and widow/orphan violations.
/// Iterates to a fixpoint (a snap can land inside ANOTHER range) but never
/// below `floor` — beyond the push budget the current candidate stands.
fn snap_break_up(
    naive: f32,
    floor: f32,
    avoid_ranges: &[AvoidRange],
    paragraphs: &[ParagraphLines],
    policy: &BreakPolicy,
) -> f32 {
    let mut y = naive;
    // Bounded iterations: each snap strictly decreases y and range/paragraph
    // counts are finite; 32 covers any sane nesting without risking a loop.
    for _ in 0..32 {
        let mut moved = false;

        for range in avoid_ranges {
            // STRICTLY inside (a break AT an edge is fine).
            if y > range.top + f32::EPSILON && y < range.bottom - f32::EPSILON {
                let _ = range.node;
                if range.top >= floor {
                    y = range.top;
                    moved = true;
                } // else: budget exceeded — the naive break stands mid-range
            }
        }

        if policy.widows_orphans {
            for para in paragraphs {
                let first_top = para.lines[0].0;
                let last_bottom = para.lines[para.lines.len() - 1].1;
                if y <= first_top || y >= last_bottom {
                    continue;
                }
                // Lines fully above the break stay; the rest move to the next page.
                let before = para.lines.iter().filter(|(_, b)| *b <= y + 0.5).count() as u32;
                let after = para.lines.len() as u32 - before;
                if before > 0 && before < para.orphans {
                    // Too few lines kept: the whole paragraph moves.
                    if first_top >= floor {
                        y = first_top;
                        moved = true;
                    }
                } else if after > 0 && after < para.widows {
                    // Too few lines moved: push more lines over the break.
                    let needed = para.lines.len() as u32 - para.widows;
                    let target = para
                        .lines
                        .get(needed as usize)
                        .map_or(first_top, |(t, _)| *t);
                    if target < y && target >= floor {
                        y = target;
                        moved = true;
                    }
                }
            }
        }

        if !moved {
            break;
        }
    }
    y
}

/// Which page a document-space Y coordinate lands on, given the break list
/// (page 0 = before the first break). The document-editor query: "what page
/// is this node on?" WITHOUT materializing any per-page display list.
#[must_use]
pub fn page_of_y(breaks: &[PageBreak], y: f32) -> usize {
    breaks.iter().take_while(|b| b.y <= y).count()
}

/// Precomputed pagination facts for a document — everything a viewer needs
/// to draw page chrome and schedule lazy page materialization, with NO
/// per-page display list generated.
#[derive(Debug, Clone, PartialEq)]
pub struct PaginationInfo {
    pub breaks: Vec<PageBreak>,
    pub page_count: usize,
    pub total_content_height: f32,
}

/// Compute page breaks for a display list: CSS-forced breaks
/// (`DisplayList::forced_page_breaks`) plus regular interval breaks wherever
/// the page runs full.
///
/// Returns an empty vector when the document has no content or the first-page
/// height is not positive — [`page_spans`] then yields the single-page result.
#[must_use]
pub fn compute_page_breaks_from_display_list(
    display_list: &DisplayList,
    constraints: &PageConstraints,
) -> Vec<PageBreak> {
    compute_page_breaks_from_positions(
        &display_list.forced_page_breaks,
        constraints,
        calculate_display_list_height(display_list),
    )
}

/// The display-list-independent core: forced break Y positions + page heights
/// + total content height. Also the seat for later break-awareness stages,
/// which add richer inputs without touching this contract.
#[must_use]
pub fn compute_page_breaks_from_positions(
    forced_breaks: &[f32],
    constraints: &PageConstraints,
    total_height: f32,
) -> Vec<PageBreak> {
    let first = constraints.first_page_content_height;
    let normal = constraints.normal_page_content_height;

    if total_height <= 0.0 || first <= 0.0 {
        return Vec::new();
    }

    let mut breaks: Vec<PageBreak> = Vec::new();

    // Forced breaks from CSS break-before/after: always.
    // The range check also filters NaN (both comparisons are false for NaN).
    for &forced_break_y in forced_breaks {
        if forced_break_y > 0.0 && forced_break_y < total_height {
            breaks.push(PageBreak {
                y: forced_break_y,
                kind: BreakKind::Forced,
                causing_node: None,
            });
        }
    }

    // Regular interval breaks. A non-positive normal height cannot advance the
    // cursor — emit the first-page break once and stop (defect 3: the old loop
    // never terminated here).
    let mut y = first;
    #[allow(clippy::while_float)] // intentional bounded float loop; an integer counter would be artificial
    while y < total_height {
        breaks.push(PageBreak {
            y,
            kind: BreakKind::Interval,
            causing_node: None,
        });
        if !(normal > 0.0) {
            break;
        }
        y += normal;
    }

    breaks.sort_by(|a, b| a.y.total_cmp(&b.y));

    // Merge breaks within the 1px window. A forced break replaces an interval
    // break in the same window (defect 2: forced breaks win); otherwise the
    // first break of a run is kept, matching the old positional dedup.
    let mut merged: Vec<PageBreak> = Vec::with_capacity(breaks.len());
    for b in breaks {
        match merged.last_mut() {
            Some(last) if (b.y - last.y).abs() < MERGE_WINDOW_PX => {
                if last.kind == BreakKind::Interval && b.kind == BreakKind::Forced {
                    *last = b;
                }
            }
            _ => merged.push(b),
        }
    }
    merged
}

/// Convert break positions into per-page `(start_y, end_y)` spans — what the
/// slicer consumes. Breaks at or below a previous break (and at Y=0) are
/// skipped rather than producing empty pages.
///
/// May return an empty vector when `total_height <= 0` and there are no
/// breaks; pagination entry points map that to their single-page fallback.
#[must_use]
pub fn page_spans(breaks: &[PageBreak], total_height: f32) -> Vec<(f32, f32)> {
    let mut spans: Vec<(f32, f32)> = Vec::with_capacity(breaks.len() + 1);
    let mut page_start = 0.0f32;

    for b in breaks {
        if b.y > page_start {
            spans.push((page_start, b.y));
            page_start = b.y;
        }
    }

    if page_start < total_height {
        spans.push((page_start, total_height));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constraints(first: f32, normal: f32) -> PageConstraints {
        PageConstraints {
            first_page_content_height: first,
            normal_page_content_height: normal,
        }
    }

    fn ys(breaks: &[PageBreak]) -> Vec<f32> {
        breaks.iter().map(|b| b.y).collect()
    }

    /// The pre-extraction algorithm, verbatim (minus the NaN-panic sort), as
    /// the golden reference. Differences are allowed ONLY where the named
    /// defects fire.
    fn reference_spans(
        forced: &[f32],
        first_page_height: f32,
        normal_page_height: f32,
        total_height: f32,
    ) -> Vec<(f32, f32)> {
        if total_height <= 0.0 || first_page_height <= 0.0 {
            return vec![(0.0, total_height.max(first_page_height))];
        }
        let mut break_points: Vec<f32> = Vec::new();
        for &forced_break_y in forced {
            if forced_break_y > 0.0 && forced_break_y < total_height {
                break_points.push(forced_break_y);
            }
        }
        let mut y = first_page_height;
        while y < total_height {
            break_points.push(y);
            // Guard added ONLY to keep the reference terminating; the shipped
            // code hung here (defect 3), which the corpus below avoids by
            // never combining normal<=0 with the reference.
            if !(normal_page_height > 0.0) {
                break;
            }
            y += normal_page_height;
        }
        break_points.sort_by(f32::total_cmp);
        break_points.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        let mut page_breaks: Vec<(f32, f32)> = Vec::new();
        let mut page_start = 0.0f32;
        for break_y in break_points {
            if break_y > page_start {
                page_breaks.push((page_start, break_y));
                page_start = break_y;
            }
        }
        if page_start < total_height {
            page_breaks.push((page_start, total_height));
        }
        if page_breaks.is_empty() {
            page_breaks.push((0.0, total_height.max(first_page_height)));
        }
        page_breaks
    }

    /// New-path spans including the entry-point fallback, so the comparison is
    /// against what callers actually observe.
    fn new_spans(forced: &[f32], first: f32, normal: f32, total: f32) -> Vec<(f32, f32)> {
        if total <= 0.0 || first <= 0.0 {
            return vec![(0.0, total.max(first))];
        }
        let breaks = compute_page_breaks_from_positions(forced, &constraints(first, normal), total);
        let mut spans = page_spans(&breaks, total);
        if spans.is_empty() {
            spans.push((0.0, total.max(first)));
        }
        spans
    }

    #[test]
    fn golden_corpus_matches_the_old_algorithm_where_no_defect_fires() {
        // (forced, first, normal, total) — no forced-vs-interval collisions
        // within 1px, normal > 0: behavior must be IDENTICAL.
        let corpus: &[(&[f32], f32, f32, f32)] = &[
            (&[], 100.0, 100.0, 250.0),
            (&[], 100.0, 100.0, 100.0),
            (&[], 100.0, 100.0, 99.0),
            (&[], 100.0, 50.0, 1000.0),
            (&[], 50.0, 100.0, 1000.0),
            (&[50.0], 100.0, 100.0, 250.0),
            (&[50.0, 150.0], 100.0, 100.0, 250.0),
            (&[-10.0, 0.0, 250.0, 9999.0, f32::NAN, f32::INFINITY], 100.0, 100.0, 250.0),
            (&[50.0, 50.4], 100.0, 100.0, 250.0), // forced-forced merge: first wins in both
            (&[249.5], 100.0, 100.0, 250.0),
            (&[], 0.0, 100.0, 250.0),   // degenerate: single page
            (&[], -50.0, 100.0, 250.0), // degenerate: single page
            (&[], 100.0, 100.0, 0.0),   // empty document
            (&[], 100.0, 100.0, -5.0),  // negative height
            (&[], f32::NAN, 100.0, 250.0), // NaN first: one unsplit page in both
            (&[], 100.0, 100.0, 50.0),  // total < first
        ];
        for &(forced, first, normal, total) in corpus {
            assert_eq!(
                new_spans(forced, first, normal, total),
                reference_spans(forced, first, normal, total),
                "case: forced={forced:?} first={first} normal={normal} total={total}"
            );
        }
    }

    #[test]
    fn defect_2_forced_break_within_1px_of_interval_break_survives() {
        // Old behavior: sorted [100.0 (interval), 100.5 (forced)] → dedup kept
        // 100.0 and the author's forced break vanished. New behavior: the
        // forced break replaces the interval break in the merge window.
        let breaks =
            compute_page_breaks_from_positions(&[100.5], &constraints(100.0, 100.0), 250.0);
        assert_eq!(ys(&breaks), vec![100.5, 200.0]);
        assert_eq!(breaks[0].kind, BreakKind::Forced);
        assert_eq!(breaks[1].kind, BreakKind::Interval);
        assert_eq!(
            page_spans(&breaks, 250.0),
            vec![(0.0, 100.5), (100.5, 200.0), (200.0, 250.0)]
        );

        // …and the old behavior really was the swallow (regression witness):
        let old = reference_spans(&[100.5], 100.0, 100.0, 250.0);
        assert_eq!(old, vec![(0.0, 100.0), (100.0, 200.0), (200.0, 250.0)]);

        // Forced break BELOW the interval break in the window: same outcome.
        let breaks =
            compute_page_breaks_from_positions(&[99.6], &constraints(100.0, 100.0), 250.0);
        assert_eq!(ys(&breaks), vec![99.6, 200.0]);
        assert_eq!(breaks[0].kind, BreakKind::Forced);
    }

    #[test]
    fn defect_3_non_positive_normal_height_terminates() {
        // Old behavior: `y += normal` with normal <= 0 never terminated when
        // skip_first_page made the first page valid. New behavior: the
        // first-page break is emitted once, everything else lands on page 2.
        for normal in [0.0, -20.0, f32::NAN] {
            let breaks =
                compute_page_breaks_from_positions(&[], &constraints(100.0, normal), 250.0);
            assert_eq!(ys(&breaks), vec![100.0], "normal={normal}");
            assert_eq!(
                page_spans(&breaks, 250.0),
                vec![(0.0, 100.0), (100.0, 250.0)],
                "normal={normal}"
            );
        }
    }

    #[test]
    fn from_slicer_config_reproduces_the_inlined_subtraction() {
        use crate::solver3::pagination::HeaderFooterConfig;

        let mut cfg = SlicerConfig::simple(800.0);
        let c = PageConstraints::from_slicer_config(&cfg);
        assert_eq!(c.first_page_content_height, 800.0);
        assert_eq!(c.normal_page_content_height, 800.0);

        cfg.header_footer = HeaderFooterConfig {
            show_header: true,
            header_height: 50.0,
            show_footer: true,
            footer_height: 30.0,
            ..Default::default()
        };
        let c = PageConstraints::from_slicer_config(&cfg);
        assert_eq!(c.first_page_content_height, 720.0);
        assert_eq!(c.normal_page_content_height, 720.0);

        cfg.header_footer.skip_first_page = true;
        let c = PageConstraints::from_slicer_config(&cfg);
        assert_eq!(c.first_page_content_height, 800.0);
        assert_eq!(c.normal_page_content_height, 720.0);
    }

    // ==================================================================
    // B3: break-awareness (policy-gated)
    // ==================================================================

    use crate::solver3::display_list::{DisplayList, DisplayListItem};
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
    use azul_core::styled_dom::StyledDom;
    use crate::solver3::display_list::BorderRadius as DlBorderRadius;
    use azul_css::props::basic::ColorU;

    fn rect(y: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y },
            size: LogicalSize {
                width: 100.0,
                height: h,
            },
        }
    }

    fn rect_item(y: f32, h: f32) -> DisplayListItem {
        DisplayListItem::Rect {
            bounds: rect(y, h).into(),
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            border_radius: DlBorderRadius::default(),
        }
    }

    fn text_item(y: f32, h: f32) -> DisplayListItem {
        DisplayListItem::Text {
            glyphs: Vec::new(),
            font_hash: crate::font_traits::FontHash::from_hash(1),
            font_size_px: 12.0,
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            clip_rect: rect(y, h).into(),
            source_node_index: None,
        }
    }

    /// A DOM whose node 1 is `.avoid { break-inside: avoid; }`, plus a
    /// display list of `(item, source node)` pairs.
    fn avoid_fixture(items: Vec<(DisplayListItem, Option<usize>)>) -> (StyledDom, DisplayList) {
        let mut dom = azul_core::dom::Dom::create_div();
        dom.add_child(
            azul_core::dom::Dom::create_div()
                .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("avoid".into())].into()),
        );
        let (css, _warnings) = azul_css::parser2::new_from_str(
            ".avoid { break-inside: avoid; } p { widows: 2; orphans: 2; }",
        );
        let styled = StyledDom::create(&mut dom, css);

        let mut dl = DisplayList::default();
        for (item, node) in items {
            dl.items.push(item);
            dl.node_mapping.push(node.map(NodeId::new));
        }
        (styled, dl)
    }

    fn breaks_with(
        styled: &StyledDom,
        dl: &DisplayList,
        policy: &BreakPolicy,
        first: f32,
        normal: f32,
    ) -> Vec<PageBreak> {
        compute_page_breaks(
            &PageBreakInput {
                display_list: dl,
                layout_tree: None,
                styled_dom: styled,
            },
            &constraints(first, normal),
            policy,
        )
    }

    #[test]
    fn policy_off_is_byte_identical_to_the_plain_algorithm() {
        let (styled, dl) = avoid_fixture(vec![
            (rect_item(0.0, 250.0), None),
            (rect_item(80.0, 60.0), Some(1)), // avoid-box straddling y=100
        ]);
        let plain = compute_page_breaks_from_display_list(&dl, &constraints(100.0, 100.0));
        let off = breaks_with(&styled, &dl, &BreakPolicy::default(), 100.0, 100.0);
        assert_eq!(plain, off);
    }

    #[test]
    fn break_inside_avoid_box_is_pushed_intact() {
        // Page 100; the avoid-box spans 80..140 — the naive break at 100 cuts
        // it, so the break snaps UP to the box top (80).
        let (styled, dl) = avoid_fixture(vec![
            (rect_item(0.0, 250.0), None),
            (rect_item(80.0, 60.0), Some(1)),
        ]);
        let policy = BreakPolicy {
            honor_break_inside: true,
            ..Default::default()
        };
        let breaks = breaks_with(&styled, &dl, &policy, 100.0, 100.0);
        assert_eq!(
            breaks[0].y, 80.0,
            "the break must land at the avoid-box top, got {breaks:?}"
        );
        assert!(matches!(breaks[0].kind, BreakKind::Avoided { pushed_from } if (pushed_from - 100.0).abs() < 0.01));
        // Following pages re-flow from the moved break.
        assert_eq!(breaks[1].y, 180.0);
    }

    #[test]
    fn taller_than_page_avoid_box_is_torn() {
        // The avoid-box spans 0..180 with a 100 page: satisfying it is
        // impossible (monolith rule) — the naive interval stands.
        let (styled, dl) = avoid_fixture(vec![
            (rect_item(0.0, 250.0), None),
            (rect_item(0.0, 180.0), Some(1)),
        ]);
        let policy = BreakPolicy {
            honor_break_inside: true,
            ..Default::default()
        };
        let breaks = breaks_with(&styled, &dl, &policy, 100.0, 100.0);
        assert_eq!(breaks[0].y, 100.0);
        assert!(matches!(breaks[0].kind, BreakKind::Interval));
    }

    #[test]
    fn forced_break_wins_over_avoid() {
        // Forced break at 90 INSIDE the avoid-box: forced always applies
        // (CSS Fragmentation §resolution), the avoid-range cannot move it.
        let (styled, mut dl) = avoid_fixture(vec![
            (rect_item(0.0, 250.0), None),
            (rect_item(80.0, 60.0), Some(1)),
        ]);
        dl.forced_page_breaks = vec![90.0];
        let policy = BreakPolicy {
            honor_break_inside: true,
            ..Default::default()
        };
        let breaks = breaks_with(&styled, &dl, &policy, 100.0, 100.0);
        assert_eq!(breaks[0].y, 90.0);
        assert!(matches!(breaks[0].kind, BreakKind::Forced));
    }

    #[test]
    fn atomic_lines_never_slice_a_text_rect() {
        // Lines at 90..106 and 106..122; page 100 cuts the first line — the
        // break snaps to its top (90).
        let (styled, dl) = avoid_fixture(vec![
            (rect_item(0.0, 250.0), None),
            (text_item(90.0, 16.0), None),
            (text_item(106.0, 16.0), None),
        ]);
        let policy = BreakPolicy {
            atomic_lines: true,
            ..Default::default()
        };
        let breaks = breaks_with(&styled, &dl, &policy, 100.0, 100.0);
        assert_eq!(breaks[0].y, 90.0, "{breaks:?}");
        assert!(matches!(breaks[0].kind, BreakKind::Avoided { .. }));
    }

    #[test]
    fn orphans_move_the_whole_paragraph() {
        // Paragraph (node 1) with 3 lines at 84..100, 100..116, 116..132;
        // orphans: 2 (from the p rule — attach the class to make it node 1).
        // The naive break at 100 keeps ONE line — fewer than orphans — so the
        // whole paragraph moves (break at its top, 84).
        let mut dom = azul_core::dom::Dom::create_div();
        dom.add_child(azul_core::dom::Dom::create_p());
        let (css, _warnings) = azul_css::parser2::new_from_str("p { widows: 2; orphans: 2; }");
        let styled = StyledDom::create(&mut dom, css);
        let mut dl = DisplayList::default();
        for (item, node) in [
            (rect_item(0.0, 250.0), None),
            (text_item(84.0, 16.0), Some(1)),
            (text_item(100.0, 16.0), Some(1)),
            (text_item(116.0, 16.0), Some(1)),
        ] {
            dl.items.push(item);
            dl.node_mapping.push(node.map(NodeId::new));
        }
        let policy = BreakPolicy {
            widows_orphans: true,
            ..Default::default()
        };
        let breaks = compute_page_breaks(
            &PageBreakInput {
                display_list: &dl,
                layout_tree: None,
                styled_dom: &styled,
            },
            &constraints(100.0, 100.0),
            &policy,
        );
        assert_eq!(breaks[0].y, 84.0, "{breaks:?}");
        assert!(matches!(breaks[0].kind, BreakKind::Avoided { .. }));
    }

    #[test]
    fn page_of_y_counts_breaks_at_or_below_y() {
        let b = |y: f32, kind: BreakKind| PageBreak {
            y,
            kind,
            causing_node: None,
        };
        let breaks = [
            b(100.0, BreakKind::Interval),
            b(180.0, BreakKind::Forced),
            b(280.0, BreakKind::Interval),
        ];
        assert_eq!(page_of_y(&breaks, 0.0), 0);
        assert_eq!(page_of_y(&breaks, 99.9), 0);
        // A break at EXACTLY y sends the content to the next page
        // ("content at or below y belongs to the next page").
        assert_eq!(page_of_y(&breaks, 100.0), 1);
        assert_eq!(page_of_y(&breaks, 179.0), 1);
        assert_eq!(page_of_y(&breaks, 200.0), 2);
        assert_eq!(page_of_y(&breaks, 9999.0), 3);
        assert_eq!(page_of_y(&[], 50.0), 0, "no breaks: everything is page 0");
    }

    #[test]
    fn default_break_policy_is_all_off() {
        let p = BreakPolicy::default();
        assert!(
            !p.honor_break_inside
                && !p.widows_orphans
                && !p.atomic_lines
                && !p.atomic_table_rows
                && !p.repeat_table_headers,
            "defaults-off is the bug-compat contract B2 ships under"
        );
    }

    #[test]
    fn page_spans_skips_zero_height_pages_and_may_be_empty() {
        let b = |y: f32| PageBreak {
            y,
            kind: BreakKind::Interval,
            causing_node: None,
        };
        // Break at 0 and duplicate breaks produce no empty page.
        assert_eq!(
            page_spans(&[b(0.0), b(100.0), b(100.0)], 250.0),
            vec![(0.0, 100.0), (100.0, 250.0)]
        );
        // Break beyond the end: final span still lands on total_height only
        // if content remains.
        assert_eq!(page_spans(&[b(250.0)], 250.0), vec![(0.0, 250.0)]);
        // No content, no breaks: empty (callers add the single-page fallback).
        assert_eq!(page_spans(&[], 0.0), Vec::<(f32, f32)>::new());
    }
}
