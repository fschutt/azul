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

use azul_core::{dom::NodeId, id::OptionNodeId};
use azul_css::impl_option_inner;

use crate::solver3::display_list::{calculate_display_list_height, DisplayList, SlicerConfig};

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
    Avoided(f32),
}

/// One page boundary in document space.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)] // in the C API through `PaginationInfo` (9g-ii-f-i); the 8-aligned
// option first so it is not padded after two 4-byte fields
pub struct PageBreakPosition {
    /// For [`BreakKind::Forced`]: the node whose break property caused it,
    /// when known. `None` in the display-list-only path - the display list
    /// records only the Y positions of forced breaks.
    pub causing_node: OptionNodeId,
    /// Document-space Y where the page ENDS (content at or below `y` belongs
    /// to the next page).
    pub y: f32,
    pub kind: BreakKind,
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

/// Break-awareness policy.
///
/// ALL flags off (the default) reproduces the plain forced ∪ interval
/// algorithm exactly — that is how this type can ship ahead of the behaviors
/// it gates (each lands in its own stage and flips on in printpdf with a
/// changelog entry, never silently).
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // independent feature flags; mirrors the C-ABI struct layout
#[repr(C)] // in the C API through `FakePageConfig` (9g-ii-f-i); the f32 first
pub struct BreakPolicy {
    /// Upper bound on how far a break may be pushed UP to satisfy
    /// avoid-rules, as a fraction of the page height (guards pathological
    /// cascades; beyond it the plain candidate snap applies).
    pub max_push_distance: f32,
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
#[derive(Debug)]
pub struct PageBreakInput<'a> {
    /// Item geometry + forced break positions.
    pub display_list: &'a DisplayList,
    /// Box rects + line boxes for future break-candidate stages. v1
    /// break-awareness derives all geometry from the display list, so this
    /// may be `None`.
    pub layout_tree: Option<&'a crate::solver3::layout_tree::LayoutTree>,
    /// Break properties (`break-inside`, `widows`, `orphans`, …).
    pub styled_dom: &'a azul_core::styled_dom::StyledDom,
    /// Registered table headers (`collect_table_headers`) — continuation
    /// pages that start inside a table reserve the repeated thead's height.
    /// `None` when `repeat_table_headers` is off.
    pub table_headers: Option<&'a crate::solver3::pagination::TableHeaderTracker>,
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
pub fn compute_page_breaks(
    input: &PageBreakInput<'_>,
    constraints: &PageConstraints,
    policy: &BreakPolicy,
) -> Vec<PageBreakPosition> {
    compute_page_breaks_impl(input, *constraints, policy, None, None)
}

/// Why an element could not be kept intact across a page boundary (E21).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonolithReason {
    /// A `break-inside: avoid` box taller than the page content area —
    /// no break position can satisfy it, so it tears.
    AvoidBoxTallerThanPage,
    /// An atomic table row taller than the page content area.
    RowTallerThanPage,
}

/// E21: one element the break pass had to TEAR despite a keep-intact rule.
/// The exporter surfaces these as "element X could not be kept intact".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonolithWarning {
    /// The node whose rule was violated (None = anonymous, e.g. a row range
    /// derived without a node mapping).
    pub node: Option<NodeId>,
    /// Document-space extent of the torn box.
    pub top: f32,
    pub bottom: f32,
    pub reason: MonolithReason,
}

/// [`compute_page_breaks`] + the E21 monolith report: elements whose
/// keep-intact rules (`break-inside: avoid`, atomic rows) could not be
/// honored because they are taller than the page. The breaks are identical
/// to [`compute_page_breaks`] — the report only ADDS information.
#[must_use]
pub fn compute_page_breaks_with_report(
    input: &PageBreakInput<'_>,
    constraints: &PageConstraints,
    policy: &BreakPolicy,
) -> (Vec<PageBreakPosition>, Vec<MonolithWarning>) {
    let mut warnings = Vec::new();
    let breaks = compute_page_breaks_impl(input, *constraints, policy, None, Some(&mut warnings));
    (breaks, warnings)
}

/// [`compute_page_breaks`] with an office-suite-style [`PageSequence`].
///
/// Every page's content HEIGHT comes from `setup_for_page(index)` (default /
/// explicit override / different-first / odd-even parity), so "page 345 is
/// landscape-height with a 2cm footer" flows straight into the break
/// positions. Content WIDTH must be uniform for now (the fragmentainer
/// re-wrap stage) — a non-uniform sequence announces once and lays out at
/// the default width.
#[must_use]
pub fn compute_page_breaks_with_sequence(
    input: &PageBreakInput<'_>,
    sequence: &crate::solver3::pagination::PageSequence,
    policy: &BreakPolicy,
) -> Vec<PageBreakPosition> {
    let _ = sequence.has_uniform_width(); // announce the width degradation once
    let default_h = sequence.default.content_height();
    let constraints = PageConstraints {
        first_page_content_height: sequence.setup_for_page(0).content_height(),
        normal_page_content_height: default_h,
    };
    compute_page_breaks_impl(input, constraints, policy, Some(sequence), None)
}

#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn compute_page_breaks_impl(
    input: &PageBreakInput<'_>,
    constraints: PageConstraints,
    policy: &BreakPolicy,
    sequence: Option<&crate::solver3::pagination::PageSequence>,
    mut monolith_report: Option<&mut Vec<MonolithWarning>>,
) -> Vec<PageBreakPosition> {
    let any_awareness = policy.honor_break_inside
        || policy.atomic_lines
        || policy.widows_orphans
        || policy.atomic_table_rows
        || policy.repeat_table_headers
        // A page sequence varies heights per page — the forward pass is the
        // only correct evaluator even with all avoid-rules off.
        || sequence.is_some();
    if !any_awareness {
        return compute_page_breaks_from_display_list(input.display_list, &constraints);
    }

    let total_height = calculate_display_list_height(input.display_list);
    let first = constraints.first_page_content_height;
    let normal = constraints.normal_page_content_height;
    if total_height <= 0.0 || first <= 0.0 {
        return Vec::new();
    }

    let mut avoid_ranges =
        collect_avoid_ranges(input, constraints, policy, monolith_report.as_deref_mut());
    if policy.atomic_table_rows {
        // A break may not slice a table row (monolith rule still applies:
        // a row taller than the page tears).
        let page_h = constraints.normal_page_content_height.max(1.0);
        for (top, bottom) in crate::solver3::pagination::collect_table_row_ranges(
            input.display_list,
            input.styled_dom,
        ) {
            if bottom - top <= page_h {
                avoid_ranges.push(AvoidRange {
                    top,
                    bottom,
                    node: None,
                });
            } else if let Some(report) = monolith_report.as_deref_mut() {
                report.push(MonolithWarning {
                    node: None,
                    top,
                    bottom,
                    reason: MonolithReason::RowTallerThanPage,
                });
            }
        }
        avoid_ranges.sort_by(|a, b| a.top.total_cmp(&b.top));
    }

    // Tables whose continuation pages must reserve repeated-thead height.
    let reserved_tables: Vec<(f32, f32, f32)> = if policy.repeat_table_headers {
        input
            .table_headers
            .map(|t| {
                t.tables
                    .iter()
                    .map(|info| (info.table_start_y, info.table_end_y, info.thead_height))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let paragraphs = if policy.widows_orphans {
        collect_paragraph_lines(input)
    } else {
        Vec::new()
    };

    // Forced breaks, ascending (they are hard walls the forward pass emits
    // verbatim — CSS Fragmentation: forced always wins over avoid).
    let mut forced: Vec<crate::solver3::display_list::ForcedBreak> = input
        .display_list
        .forced_page_breaks
        .iter()
        .copied()
        .filter(|b| b.y > 0.0 && b.y < total_height)
        .collect();
    forced.sort_by(|a, b| a.y.total_cmp(&b.y));

    let mut breaks: Vec<PageBreakPosition> = Vec::new();
    let mut prev_end = 0.0_f32;
    let mut page_height = first;
    let mut page_index: usize = 0;
    let mut forced_iter = forced.into_iter().peekable();

    loop {
        // Per-page setup (classic office suites model): the sequence overrides the height
        // for THIS page index (explicit / first / parity / default).
        if let Some(seq) = sequence {
            page_height = seq.setup_for_page(page_index).content_height();
        }
        // A page STARTING inside a table shows that table's repeated thead —
        // its content area shrinks by the header stack's height.
        let reserve: f32 = reserved_tables
            .iter()
            .filter(|(start, end, _)| *start < prev_end && *end > prev_end)
            .map(|(_, _, h)| *h)
            .sum();
        let effective_height = (page_height - reserve).max(MERGE_WINDOW_PX);
        let naive = prev_end + effective_height;

        // A forced break before (or at) the naive position ends the page there.
        if let Some(&fb) = forced_iter.peek() {
            if fb.y <= naive + MERGE_WINDOW_PX {
                forced_iter.next();
                if fb.y > prev_end + MERGE_WINDOW_PX {
                    breaks.push(PageBreakPosition {
                        y: fb.y,
                        kind: BreakKind::Forced,
                        causing_node: fb.causing_node.into(),
                    });
                    prev_end = fb.y;
                    page_height = normal;
                    page_index += 1;
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
            BreakKind::Avoided(naive)
        };
        breaks.push(PageBreakPosition {
            y: adjusted,
            kind,
            causing_node: OptionNodeId::None,
        });
        prev_end = adjusted;
        page_height = normal;
        page_index += 1;
        // `!(x > 0.0)` semantics kept explicitly: a NaN height must also stop
        // the loop, so test `<= 0.0 || is_nan` instead of the negation.
        if sequence.is_none() && (normal <= 0.0 || normal.is_nan()) {
            break;
        }
        if sequence.is_some() {
            let next_height = page_height_floor(sequence, page_index, normal);
            if next_height <= 0.0 || next_height.is_nan() {
                break;
            }
        }
    }

    // Any forced breaks past the last interval position still apply.
    for fb in forced_iter {
        if fb.y > prev_end + MERGE_WINDOW_PX {
            breaks.push(PageBreakPosition {
                y: fb.y,
                kind: BreakKind::Forced,
                causing_node: fb.causing_node.into(),
            });
            prev_end = fb.y;
        }
    }

    breaks
}

/// Termination guard: the NEXT page's height (sequence-aware). A sequence
/// page with non-positive content height would loop forever, same as the
/// plain `normal <= 0` case.
fn page_height_floor(
    sequence: Option<&crate::solver3::pagination::PageSequence>,
    page_index: usize,
    normal: f32,
) -> f32 {
    sequence.map_or(normal, |s| s.setup_for_page(page_index).content_height())
}

/// Collect the vertical ranges a break may not enter.
fn collect_avoid_ranges(
    input: &PageBreakInput<'_>,
    constraints: PageConstraints,
    policy: &BreakPolicy,
    mut monolith_report: Option<&mut Vec<MonolithWarning>>,
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
            let Some(bounds) = item.bounds() else {
                continue;
            };
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
            // E21: the tear is no longer SILENT — it lands in the report.
            if bottom - top > page_height {
                if let Some(report) = monolith_report.as_deref_mut() {
                    report.push(MonolithWarning {
                        node: Some(node),
                        top,
                        bottom,
                        reason: MonolithReason::AvoidBoxTallerThanPage,
                    });
                }
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
            if let crate::solver3::display_list::DisplayListItem::Text { clip_rect, .. } = item {
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
fn collect_paragraph_lines(input: &PageBreakInput<'_>) -> Vec<ParagraphLines> {
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
                let total_lines = u32::try_from(para.lines.len()).unwrap_or(u32::MAX);
                let before_count = para.lines.iter().filter(|(_, b)| *b <= y + 0.5).count();
                let before = u32::try_from(before_count).unwrap_or(u32::MAX);
                let after = total_lines - before;
                if before > 0 && before < para.orphans {
                    // Too few lines kept: the whole paragraph moves.
                    if first_top >= floor {
                        y = first_top;
                        moved = true;
                    }
                } else if after > 0 && after < para.widows {
                    // Too few lines moved: push more lines over the break.
                    let needed = total_lines - para.widows;
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

/// Incremental re-break after an edit.
///
/// Recompute (always — correctness under
/// ANY input change, and the break pass is cheap next to the relayout that
/// preceded it), then NORMALIZE against the previous run — every fresh break
/// that matches a previous break within the merge window returns the
/// PREVIOUS value bit-for-bit. Unchanged pages therefore compare value-equal
/// against the old spans, which is the signal a paged viewer uses to keep
/// its cached page surfaces.
///
/// `dirty_y_start` (the topmost document-space Y whose content changed — the
/// editing IFC root's top, see `LayoutWindow::take_pagination_dirty_from`)
/// is a DEBUG contract: breaks above it must come out identical, and a
/// mismatch there means the caller under-reported the dirty region — the
/// fresh value wins (correctness over reuse).
#[must_use]
pub fn recompute_page_breaks_from(
    prev: &[PageBreakPosition],
    input: &PageBreakInput<'_>,
    constraints: &PageConstraints,
    policy: &BreakPolicy,
    dirty_y_start: f32,
) -> Vec<PageBreakPosition> {
    let fresh = compute_page_breaks(input, constraints, policy);

    let mut out: Vec<PageBreakPosition> = Vec::with_capacity(fresh.len());
    let mut prev_iter = prev.iter().peekable();
    for fb in fresh {
        while prev_iter
            .peek()
            .is_some_and(|pb| pb.y < fb.y - MERGE_WINDOW_PX)
        {
            prev_iter.next();
        }
        match prev_iter.peek() {
            Some(pb) if (pb.y - fb.y).abs() < MERGE_WINDOW_PX => {
                // Value-identical reuse — above the dirty bound this is the
                // guaranteed case; below it, it means the page converged.
                out.push(**pb);
                prev_iter.next();
            }
            _ => {
                debug_assert!(
                    fb.y >= dirty_y_start - MERGE_WINDOW_PX,
                    "a break above dirty_y_start changed ({} < {dirty_y_start}) — \
                     the caller under-reported the dirty region",
                    fb.y
                );
                out.push(fb);
            }
        }
    }
    out
}

/// Which page a document-space Y coordinate lands on.
///
/// Given the break list (page 0 = before the first break). The
/// document-editor query: "what page is this node on?" WITHOUT materializing
/// any per-page display list.
#[must_use]
pub fn page_of_y(breaks: &[PageBreakPosition], y: f32) -> usize {
    breaks.iter().take_while(|b| b.y <= y).count()
}

/// Precomputed pagination facts for a document — everything a viewer needs
/// to draw page chrome and schedule lazy page materialization, with NO
/// per-page display list generated.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)] // `CallbackInfo::query_pagination`'s answer (9g-ii-f-i)
pub struct PaginationInfo {
    pub breaks: PageBreakPositionVec,
    pub page_count: usize,
    pub total_content_height: f32,
}

azul_css::impl_option!(
    PageBreakPosition,
    OptionPageBreakPosition,
    [Debug, Copy, Clone, PartialEq]
);
azul_css::impl_vec!(
    PageBreakPosition,
    PageBreakPositionVec,
    PageBreakPositionVecDestructor,
    PageBreakPositionVecDestructorType,
    PageBreakPositionVecSlice,
    OptionPageBreakPosition
);
azul_css::impl_vec_clone!(PageBreakPosition, PageBreakPositionVec, PageBreakPositionVecDestructor);
azul_css::impl_vec_partialeq!(PageBreakPosition, PageBreakPositionVec);
azul_css::impl_vec_debug!(PageBreakPosition, PageBreakPositionVec);

azul_css::impl_option!(
    PaginationInfo,
    OptionPaginationInfo,
    copy = false,
    [Debug, Clone, PartialEq]
);

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
) -> Vec<PageBreakPosition> {
    compute_page_breaks_from_forced(
        &display_list.forced_page_breaks,
        constraints,
        calculate_display_list_height(display_list),
    )
}

/// The display-list-independent pagination core.
///
/// Consumes forced break Y positions plus page heights plus total content
/// height. Also the seat for later break-awareness stages, which add richer
/// inputs without touching this contract.
#[must_use]
pub fn compute_page_breaks_from_positions(
    forced_breaks: &[f32],
    constraints: &PageConstraints,
    total_height: f32,
) -> Vec<PageBreakPosition> {
    let typed: Vec<crate::solver3::display_list::ForcedBreak> = forced_breaks
        .iter()
        .map(|&y| crate::solver3::display_list::ForcedBreak {
            y,
            causing_node: None,
        })
        .collect();
    compute_page_breaks_from_forced(&typed, constraints, total_height)
}

/// [`compute_page_breaks_from_positions`] with the causing node carried
/// through to [`PageBreakPosition::causing_node`].
#[must_use]
pub fn compute_page_breaks_from_forced(
    forced_breaks: &[crate::solver3::display_list::ForcedBreak],
    constraints: &PageConstraints,
    total_height: f32,
) -> Vec<PageBreakPosition> {
    let first = constraints.first_page_content_height;
    let normal = constraints.normal_page_content_height;

    if total_height <= 0.0 || first <= 0.0 {
        return Vec::new();
    }

    let mut breaks: Vec<PageBreakPosition> = Vec::new();

    // Forced breaks from CSS break-before/after: always.
    // The range check also filters NaN (both comparisons are false for NaN).
    for fb in forced_breaks {
        if fb.y > 0.0 && fb.y < total_height {
            breaks.push(PageBreakPosition {
                y: fb.y,
                kind: BreakKind::Forced,
                causing_node: fb.causing_node.into(),
            });
        }
    }

    // Regular interval breaks. A non-positive normal height cannot advance the
    // cursor — emit the first-page break once and stop (defect 3: the old loop
    // never terminated here).
    let mut y = first;
    #[allow(clippy::while_float)]
    // intentional bounded float loop; an integer counter would be artificial
    while y < total_height {
        breaks.push(PageBreakPosition {
            y,
            kind: BreakKind::Interval,
            causing_node: OptionNodeId::None,
        });
        // `!(normal > 0.0)` semantics kept explicitly: NaN must also stop.
        if normal <= 0.0 || normal.is_nan() {
            break;
        }
        y += normal;
    }

    breaks.sort_by(|a, b| a.y.total_cmp(&b.y));

    // Merge breaks within the 1px window. A forced break replaces an interval
    // break in the same window (defect 2: forced breaks win); otherwise the
    // first break of a run is kept, matching the old positional dedup.
    let mut merged: Vec<PageBreakPosition> = Vec::with_capacity(breaks.len());
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
pub fn page_spans(breaks: &[PageBreakPosition], total_height: f32) -> Vec<(f32, f32)> {
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

    fn ys(breaks: &[PageBreakPosition]) -> Vec<f32> {
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
            // `!(x > 0.0)` semantics kept explicitly: NaN must also stop.
            if normal_page_height <= 0.0 || normal_page_height.is_nan() {
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
            (
                &[-10.0, 0.0, 250.0, 9999.0, f32::NAN, f32::INFINITY],
                100.0,
                100.0,
                250.0,
            ),
            (&[50.0, 50.4], 100.0, 100.0, 250.0), // forced-forced merge: first wins in both
            (&[249.5], 100.0, 100.0, 250.0),
            (&[], 0.0, 100.0, 250.0),      // degenerate: single page
            (&[], -50.0, 100.0, 250.0),    // degenerate: single page
            (&[], 100.0, 100.0, 0.0),      // empty document
            (&[], 100.0, 100.0, -5.0),     // negative height
            (&[], f32::NAN, 100.0, 250.0), // NaN first: one unsplit page in both
            (&[], 100.0, 100.0, 50.0),     // total < first
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
        let breaks = compute_page_breaks_from_positions(&[99.6], &constraints(100.0, 100.0), 250.0);
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

    use crate::solver3::display_list::BorderRadius as DlBorderRadius;
    use crate::solver3::display_list::{DisplayList, DisplayListItem};
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
    use azul_core::styled_dom::StyledDom;
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
            azul_core::dom::Dom::create_div().with_ids_and_classes(
                vec![azul_core::dom::IdOrClass::Class("avoid".into())].into(),
            ),
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
    ) -> Vec<PageBreakPosition> {
        compute_page_breaks(
            &PageBreakInput {
                display_list: dl,
                layout_tree: None,
                styled_dom: styled,
                table_headers: None,
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
        assert!(
            matches!(breaks[0].kind, BreakKind::Avoided(pushed_from) if (pushed_from - 100.0).abs() < 0.01)
        );
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
        dl.forced_page_breaks = vec![crate::solver3::display_list::ForcedBreak {
            y: 90.0,
            causing_node: None,
        }];
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
        assert!(matches!(breaks[0].kind, BreakKind::Avoided(..)));
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
                table_headers: None,
            },
            &constraints(100.0, 100.0),
            &policy,
        );
        assert_eq!(breaks[0].y, 84.0, "{breaks:?}");
        assert!(matches!(breaks[0].kind, BreakKind::Avoided(..)));
    }

    #[test]
    fn widows_pull_enough_lines_onto_the_next_page() {
        // E20 counterpart to the orphans test. Paragraph (node 1) with 3
        // lines at 68..84, 84..100, 100..116; widows: 2, orphans: 1.
        // The naive break at 100 carries ONE line to the next page — fewer
        // than widows — so the break moves UP one line (84): 1 line stays
        // (orphans: 1 satisfied), 2 lines carry over (widows: 2 satisfied).
        let mut dom = azul_core::dom::Dom::create_div();
        dom.add_child(azul_core::dom::Dom::create_p());
        let (css, _warnings) = azul_css::parser2::new_from_str("p { widows: 2; orphans: 1; }");
        let styled = StyledDom::create(&mut dom, css);
        let mut dl = DisplayList::default();
        for (item, node) in [
            (rect_item(0.0, 250.0), None),
            (text_item(68.0, 16.0), Some(1)),
            (text_item(84.0, 16.0), Some(1)),
            (text_item(100.0, 16.0), Some(1)),
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
                table_headers: None,
            },
            &constraints(100.0, 100.0),
            &policy,
        );
        assert_eq!(breaks[0].y, 84.0, "{breaks:?}");
        assert!(matches!(breaks[0].kind, BreakKind::Avoided(..)));
    }

    #[test]
    fn taller_than_page_avoid_box_lands_in_the_monolith_report() {
        // E21: the tear itself is unchanged (monolith rule), but it is no
        // longer silent. Avoid-box node 1 spans 250px on 100px pages.
        let (styled, dl) = avoid_fixture(vec![
            (rect_item(0.0, 400.0), None),
            (rect_item(20.0, 250.0), Some(1)),
        ]);
        let policy = BreakPolicy {
            honor_break_inside: true,
            ..Default::default()
        };
        let input = PageBreakInput {
            display_list: &dl,
            layout_tree: None,
            styled_dom: &styled,
            table_headers: None,
        };
        let (breaks, warnings) =
            compute_page_breaks_with_report(&input, &constraints(100.0, 100.0), &policy);
        assert_eq!(
            breaks,
            compute_page_breaks(&input, &constraints(100.0, 100.0), &policy),
            "the report only ADDS information, breaks are identical"
        );
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].node, Some(NodeId::new(1)));
        assert_eq!(warnings[0].reason, MonolithReason::AvoidBoxTallerThanPage);
        assert_eq!((warnings[0].top, warnings[0].bottom), (20.0, 270.0));

        // A box that FITS the page produces no warning.
        let (styled2, dl2) = avoid_fixture(vec![
            (rect_item(0.0, 400.0), None),
            (rect_item(20.0, 60.0), Some(1)),
        ]);
        let input2 = PageBreakInput {
            display_list: &dl2,
            layout_tree: None,
            styled_dom: &styled2,
            table_headers: None,
        };
        let (_, warnings2) =
            compute_page_breaks_with_report(&input2, &constraints(100.0, 100.0), &policy);
        assert!(warnings2.is_empty(), "{warnings2:?}");
    }

    /// A synthetic table DOM + display list: div > table > (thead > tr,
    /// tbody > tr×3), with one rect item per row (and one for the thead),
    /// mapped to the right nodes.
    fn table_fixture(thead_h: f32, row_h: f32, rows: usize) -> (StyledDom, DisplayList) {
        use azul_core::dom::Dom;
        let tag = azul_core::xml::tag_to_node_type;
        let mut table = Dom::create_node(tag("table"));
        let mut thead = Dom::create_node(tag("thead"));
        thead.add_child(Dom::create_node(tag("tr")));
        table.add_child(thead);
        let mut tbody = Dom::create_node(tag("tbody"));
        for _ in 0..rows {
            tbody.add_child(Dom::create_node(tag("tr")));
        }
        table.add_child(tbody);
        let mut root = Dom::create_div();
        root.add_child(table);
        let styled = StyledDom::create_from_dom(root);

        // Locate node ids structurally: thead's tr + tbody's trs.
        let container = styled.node_data.as_container();
        let trs: Vec<NodeId> = (0..container.len())
            .map(NodeId::new)
            .filter(|n| matches!(container[*n].get_node_type(), azul_core::dom::NodeType::Tr))
            .collect();
        assert_eq!(trs.len(), rows + 1, "thead tr + body trs");

        let mut dl = DisplayList::default();
        // thead row rect at the table top.
        dl.items.push(rect_item(0.0, thead_h));
        dl.node_mapping.push(Some(trs[0]));
        // body rows stacked below.
        for (i, tr) in trs[1..].iter().enumerate() {
            dl.items.push(rect_item(thead_h + i as f32 * row_h, row_h));
            dl.node_mapping.push(Some(*tr));
        }
        (styled, dl)
    }

    #[test]
    fn collect_table_headers_registers_the_thead_rebased() {
        let (styled, dl) = table_fixture(20.0, 30.0, 3);
        let tracker = crate::solver3::pagination::collect_table_headers(&dl, &styled);
        assert_eq!(tracker.tables.len(), 1, "one table registered");
        let info = &tracker.tables[0];
        assert_eq!(info.thead_height, 20.0);
        assert_eq!(info.table_start_y, 0.0);
        assert_eq!(info.table_end_y, 20.0 + 3.0 * 30.0);
        // Items stored REBASED to thead-local Y.
        let first = info.thead_items[0].bounds().expect("bounds");
        assert_eq!(first.origin.y, 0.0);
    }

    #[test]
    fn continuation_pages_inside_a_table_reserve_the_thead_height() {
        // Table spans 0..320 (thead 20 + 10 rows × 30); page height 100.
        let (styled, dl) = table_fixture(20.0, 30.0, 10);
        let tracker = crate::solver3::pagination::collect_table_headers(&dl, &styled);
        let policy = BreakPolicy {
            repeat_table_headers: true,
            ..Default::default()
        };
        let breaks = compute_page_breaks(
            &PageBreakInput {
                display_list: &dl,
                layout_tree: None,
                styled_dom: &styled,
                table_headers: Some(&tracker),
            },
            &constraints(100.0, 100.0),
            &policy,
        );
        // Page 1 ends at 100 (starts OUTSIDE any table continuation). Every
        // page STARTING inside the table reserves the 20px repeated thead:
        // 100 + 80 = 180, then 260 (the table ends at 320; the page starting
        // at 260 is still inside → 260 + 80 = 340 > 320 → last page).
        assert_eq!(ys(&breaks), vec![100.0, 180.0, 260.0], "{breaks:?}");

        // Control: flag off → plain 100/200/300.
        let plain = compute_page_breaks(
            &PageBreakInput {
                display_list: &dl,
                layout_tree: None,
                styled_dom: &styled,
                table_headers: Some(&tracker),
            },
            &constraints(100.0, 100.0),
            &BreakPolicy::default(),
        );
        assert_eq!(ys(&plain), vec![100.0, 200.0, 300.0]);
    }

    #[test]
    fn atomic_table_rows_snap_the_break_to_the_row_top() {
        // Rows at 20..50, 50..80, 80..110, 110..140…: the naive break at 100
        // slices the 80..110 row → snaps to 80.
        let (styled, dl) = table_fixture(20.0, 30.0, 6);
        let policy = BreakPolicy {
            atomic_table_rows: true,
            ..Default::default()
        };
        let breaks = compute_page_breaks(
            &PageBreakInput {
                display_list: &dl,
                layout_tree: None,
                styled_dom: &styled,
                table_headers: None,
            },
            &constraints(100.0, 100.0),
            &policy,
        );
        assert_eq!(breaks[0].y, 80.0, "{breaks:?}");
        assert!(matches!(breaks[0].kind, BreakKind::Avoided(..)));

        // Monolith rule: a row TALLER than the page still tears.
        let (styled, dl) = table_fixture(20.0, 300.0, 2);
        let breaks = compute_page_breaks(
            &PageBreakInput {
                display_list: &dl,
                layout_tree: None,
                styled_dom: &styled,
                table_headers: None,
            },
            &constraints(100.0, 100.0),
            &policy,
        );
        assert_eq!(breaks[0].y, 100.0);
        assert!(matches!(breaks[0].kind, BreakKind::Interval));
    }

    #[test]
    fn page_sequence_heights_flow_into_break_positions() {
        use crate::solver3::pagination::{PageMargins, PageSequence, PageSetup};
        use azul_core::geom::LogicalSize;

        let setup = |h: f32| PageSetup {
            page_size: LogicalSize::new(200.0, h),
            margins: PageMargins::default(),
            header_footer: Default::default(),
        };

        let (styled, dl) = avoid_fixture(vec![(rect_item(0.0, 500.0), None)]);
        let input = PageBreakInput {
            display_list: &dl,
            layout_tree: None,
            styled_dom: &styled,
            table_headers: None,
        };

        // Default 100-high pages, but PAGE 2 (0-based index 1) is 150 high:
        // breaks at 100, 250, 350, 450 (the classic office-suite "page 345 is different").
        let mut seq = PageSequence::uniform(setup(100.0));
        seq.set_override(1, setup(150.0));
        let breaks = compute_page_breaks_with_sequence(&input, &seq, &BreakPolicy::default());
        assert_eq!(ys(&breaks), vec![100.0, 250.0, 350.0, 450.0], "{breaks:?}");

        // Odd/even parity: 1-based odd pages 100 high, even pages 60 high:
        // breaks 100, 160, 260, 320, 420, 480 (alternating).
        let mut seq = PageSequence::uniform(setup(100.0));
        seq.even_pages = crate::solver3::pagination::OptionPageSetup::Some(setup(60.0));
        let breaks = compute_page_breaks_with_sequence(&input, &seq, &BreakPolicy::default());
        assert_eq!(
            ys(&breaks),
            vec![100.0, 160.0, 260.0, 320.0, 420.0, 480.0],
            "{breaks:?}"
        );

        // Different-first-page: first 40 high, rest 100: 40, 140, 240, …
        let mut seq = PageSequence::uniform(setup(100.0));
        seq.first_page = crate::solver3::pagination::OptionPageSetup::Some(setup(40.0));
        let breaks = compute_page_breaks_with_sequence(&input, &seq, &BreakPolicy::default());
        assert_eq!(
            ys(&breaks),
            vec![40.0, 140.0, 240.0, 340.0, 440.0],
            "{breaks:?}"
        );

        // Precedence: explicit override BEATS parity on the same index.
        let mut seq = PageSequence::uniform(setup(100.0));
        seq.even_pages = crate::solver3::pagination::OptionPageSetup::Some(setup(60.0));
        seq.set_override(1, setup(150.0)); // page 2 (even) overridden
        let breaks = compute_page_breaks_with_sequence(&input, &seq, &BreakPolicy::default());
        assert_eq!(breaks[1].y, 250.0, "override wins over parity: {breaks:?}");
    }

    #[test]
    fn page_setup_content_height_subtracts_margins_and_decoration() {
        use crate::solver3::pagination::{HeaderFooterConfig, PageMargins, PageSetup};
        use azul_core::geom::LogicalSize;
        let setup = PageSetup {
            page_size: LogicalSize::new(210.0, 297.0),
            margins: PageMargins {
                top: 20.0,
                right: 15.0,
                bottom: 20.0,
                left: 15.0,
            },
            header_footer: HeaderFooterConfig {
                show_footer: true,
                footer_height: 20.0,
                ..Default::default()
            },
        };
        assert_eq!(setup.content_height(), 297.0 - 40.0 - 20.0);
        assert_eq!(setup.content_width(), 210.0 - 30.0);
    }

    #[test]
    fn recompute_reuses_previous_values_where_pages_converge() {
        // A 1000-tall document, page 100: breaks at 100..900.
        let (styled, dl) = avoid_fixture(vec![(rect_item(0.0, 1000.0), None)]);
        let input = PageBreakInput {
            display_list: &dl,
            layout_tree: None,
            styled_dom: &styled,
            table_headers: None,
        };
        let c = constraints(100.0, 100.0);
        let policy = BreakPolicy::default();
        let prev = compute_page_breaks(&input, &c, &policy);
        assert_eq!(prev.len(), 9);

        // Edit on "page 3" (dirty from 250) that does NOT move any break:
        // the result must be VALUE-identical to prev — every span reusable.
        let again = recompute_page_breaks_from(&prev, &input, &c, &policy, 250.0);
        assert_eq!(again, prev, "no break moved → full value reuse");

        // A forced break appears at 450 (content change at 450): breaks
        // above stay value-identical, the region re-flows from the forced
        // break, and positions that happen to coincide again converge.
        let mut dl2 = dl.clone();
        dl2.forced_page_breaks = vec![crate::solver3::display_list::ForcedBreak {
            y: 450.0,
            causing_node: None,
        }];
        let input2 = PageBreakInput {
            display_list: &dl2,
            layout_tree: None,
            styled_dom: &styled,
            table_headers: None,
        };
        let re = recompute_page_breaks_from(&prev, &input2, &c, &policy, 450.0);
        assert_eq!(&re[..4], &prev[..4], "breaks above the change are reused");
        assert_eq!(re[4].y, 450.0, "{re:?}");
        assert!(matches!(re[4].kind, BreakKind::Forced));
        // The PLAIN algorithm keeps interval positions fixed (500, 600, …),
        // so every break below the insertion converges with prev and comes
        // back as prev's VALUES — the reuse signal a paged viewer caches on.
        assert_eq!(&re[5..], &prev[4..], "converged tail is value-reused");
    }

    #[test]
    fn page_of_y_counts_breaks_at_or_below_y() {
        let b = |y: f32, kind: BreakKind| PageBreakPosition {
            y,
            kind,
            causing_node: OptionNodeId::None,
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
        let b = |y: f32| PageBreakPosition {
            y,
            kind: BreakKind::Interval,
            causing_node: OptionNodeId::None,
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
