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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)] // becomes #[repr(C, u8)] when a data-carrying variant lands (B3: Avoided)
pub enum BreakKind {
    /// A CSS `break-before/after: always` (or legacy `page-break-*`) forced
    /// this break.
    Forced,
    /// The page was simply full (regular interval break).
    Interval,
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
