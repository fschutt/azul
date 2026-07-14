//! Headless backend for CPU-only rendering without a display server.
//!
//! This module provides the resource management and rendering pipeline for
//! running Azul applications without any platform windowing APIs. It works
//! in combination with `HeadlessWindow` (in `dll/src/desktop/shell2/headless/`) which
//! provides the `PlatformWindow` trait implementation.
//!
//! # Architecture
//!
//! The headless path replaces the WebRender GPU pipeline with `cpurender`:
//! `LayoutWindow → solver3 DisplayList → cpurender → PNG/Pixmap`. Compared to the
//! GPU path there is no GL context, `webrender::Renderer`, or `RenderApi`; fonts
//! and images are managed by `FontManager`/`ImageCache` and read directly by
//! cpurender (no GPU texture atlas or upload), hit testing uses the layout-side
//! `CpuHitTester` instead of WebRender's `AsyncHitTester`, and present/swap is a
//! no-op.
//!
//! Activated with `AZUL_HEADLESS=1` (optionally `AZ_DEBUG=1` for the debug server).

use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    styled_dom::StyledDom,
};

use crate::solver3::{getters::{get_overflow_x, get_overflow_y}, layout_tree::LayoutNodeHot, PositionVec};

/// Large finite half-extent used in place of `f32::INFINITY` for clip axes that
/// are not constrained by any ancestor. Keeping it finite avoids `NaN` in
/// `point_in_rect` (`origin + size` would be `inf - inf = NaN`) while staying
/// far outside any realistic logical-pixel coordinate.
const CLIP_UNBOUNDED: f32 = 1.0e7;

/// CPU-based hit tester that works without `WebRender`.
///
/// In the GPU path, hit testing is done by `AsyncHitTester` which queries
/// `WebRender`'s spatial tree. In headless mode, we do hit testing directly
/// against the layout results (positioned rectangles).
///
/// This is actually simpler and faster than the `WebRender` path, since we
/// don't need to go through the compositor's spatial tree — we just walk
/// the layout result nodes and check point-in-rect.
#[derive(Debug)]
pub struct CpuHitTester {
    /// Cached hit test results from the last layout.
    /// Maps `DomId` -> list of (`NodeId`, positioned rect) sorted by paint order.
    node_rects: BTreeMap<DomId, Vec<HitTestEntry>>,
}

/// A single entry in the CPU hit test acceleration structure.
#[derive(Debug, Clone)]
struct HitTestEntry {
    /// The DOM node that this entry corresponds to.
    node_id: NodeId,
    /// Absolute position and size of this node in logical pixels.
    rect: LogicalRect,
    /// Clip rect (intersection of all ancestor overflow clips).
    clip: Option<LogicalRect>,
    /// Whether this node is pointer-events: none
    pointer_events_none: bool,
}

impl Default for CpuHitTester {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuHitTester {
    /// Create a new empty hit tester.
    #[must_use] pub const fn new() -> Self {
        Self {
            node_rects: BTreeMap::new(),
        }
    }

    /// Sum of `HitTestEntry` counts across all `DomIds` (for leak probes).
    #[must_use] pub fn node_rects_total(&self) -> usize {
        self.node_rects.values().map(Vec::len).sum()
    }

    /// Rebuild the hit test structure from layout results.
    ///
    /// Called after each layout pass. Extracts positioned rectangles from
    /// `LayoutWindow::layout_results` and builds a flat list for fast
    /// point-in-rect testing.
    pub fn rebuild_from_layout(
        &mut self,
        layout_results: &BTreeMap<DomId, crate::window::DomLayoutResult>,
    ) {
        self.node_rects.clear();

        // VirtualView / iframe child DOMs lay out in CHILD-LOCAL coordinates
        // (origin 0,0) but live on screen at the host VirtualView item's
        // bounds. Hit entries must be TRANSLATED there and CLIPPED to the
        // composite bounds — otherwise the child's nodes claim pointer events
        // across the whole window (live bug: azul-maps' tile grid ate every
        // click on the header toolbar, so the buttons never fired; the same
        // escape the renderer had before intersect_clips()).
        //
        // Resolve placements iteratively so nested VirtualViews accumulate
        // their host offsets (a child's own VirtualView item is in that
        // child's local space).
        let mut placements: BTreeMap<DomId, LogicalRect> = BTreeMap::new();
        for _ in 0..4 {
            // bounded depth; each pass resolves one nesting level
            let mut changed = false;
            for (host_dom, lr) in layout_results {
                let host_offset = if host_dom.inner == 0 {
                    Some(LogicalPosition::zero())
                } else {
                    placements.get(host_dom).map(|r| r.origin)
                };
                let Some(host_offset) = host_offset else { continue };
                for item in &lr.display_list.items {
                    if let crate::solver3::display_list::DisplayListItem::VirtualView {
                        child_dom_id,
                        bounds,
                        ..
                    } = item
                    {
                        let b = *bounds.inner();
                        let absolute = LogicalRect {
                            origin: LogicalPosition {
                                x: b.origin.x + host_offset.x,
                                y: b.origin.y + host_offset.y,
                            },
                            size: b.size,
                        };
                        if placements.get(child_dom_id) != Some(&absolute) {
                            placements.insert(*child_dom_id, absolute);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        for (dom_id, layout_result) in layout_results {
            let mut entries = Vec::new();

            let positions = &layout_result.calculated_positions;
            let nodes = &layout_result.layout_tree.nodes;
            let styled_dom = &layout_result.styled_dom;

            // Child DOM: shift into window space + clip to the composite rect.
            let (offset, dom_clip) = placements.get(dom_id).map_or_else(|| (LogicalPosition::zero(), None), |b| (b.origin, Some(*b)));

            // Walk the layout nodes and their computed positions
            for (idx, node) in nodes.iter().enumerate() {
                // Only include nodes that map to a real DOM node
                let Some(node_id) = node.dom_node_id else {
                    continue; // skip anonymous boxes
                };

                // Get the position for this layout node
                let pos = match positions.get(idx) {
                    Some(p) => *p,
                    None => continue,
                };

                // Get the computed size
                let Some(size) = node.used_size else {
                    continue;
                };

                let rect = LogicalRect {
                    origin: LogicalPosition {
                        x: pos.x + offset.x,
                        y: pos.y + offset.y,
                    },
                    size,
                };

                // Clip this node to the intersection of the VirtualView composite
                // bounds (`dom_clip`) and every `overflow: hidden | clip | scroll |
                // auto` ancestor's box — otherwise a node that is scrolled/clipped
                // out of its ancestor would still claim pointer events.
                let clip = compute_node_clip(styled_dom, nodes, positions, idx, offset, dom_clip);

                entries.push(HitTestEntry {
                    node_id,
                    rect,
                    clip,
                    // azul has no `pointer-events` CSS property yet, so every laid-out
                    // node is hit-testable. Populate this from the styled DOM once such
                    // a property is added to `azul_css`.
                    pointer_events_none: false,
                });
            }

            self.node_rects.insert(*dom_id, entries);
        }
    }

    /// Perform a hit test at the given position.
    ///
    /// Returns nodes hit at (x, y) in reverse paint order (topmost first).
    #[must_use] pub fn hit_test(
        &self,
        position: LogicalPosition,
    ) -> Vec<(DomId, NodeId)> {
        let mut results = Vec::new();

        for (dom_id, entries) in &self.node_rects {
            // Walk in reverse (last painted = topmost)
            for entry in entries.iter().rev() {
                if entry.pointer_events_none {
                    continue;
                }

                // Check clip rect first (if any)
                if let Some(ref clip) = entry.clip {
                    if !point_in_rect(position, clip) {
                        continue;
                    }
                }

                // Check node rect
                if point_in_rect(position, &entry.rect) {
                    results.push((*dom_id, entry.node_id));
                }
            }
        }

        results
    }
}

/// Simple point-in-rect test.
fn point_in_rect(point: LogicalPosition, rect: &LogicalRect) -> bool {
    point.x >= rect.origin.x
        && point.x < rect.origin.x + rect.size.width
        && point.y >= rect.origin.y
        && point.y < rect.origin.y + rect.size.height
}

/// Compute the hit-test clip rect for a layout node: the intersection of the
/// host `VirtualView` composite bounds (`dom_clip`) and every clipping ancestor's
/// border box (any `overflow` other than `visible`).
///
/// Clipping is tracked per-axis because `overflow-x` / `overflow-y` are
/// independent — an axis whose ancestors are all `overflow: visible` stays
/// unbounded (stored as a large finite extent, see [`CLIP_UNBOUNDED`]). The
/// ancestor box used is the border box (`used_size`); CSS clips at the padding
/// edge, but the slightly larger border box is a safe over-inclusion for point
/// hit-testing and avoids resolving padding/border here.
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
fn compute_node_clip(
    styled_dom: &StyledDom,
    nodes: &[LayoutNodeHot],
    positions: &PositionVec,
    node_index: usize,
    offset: LogicalPosition,
    dom_clip: Option<LogicalRect>,
) -> Option<LogicalRect> {
    // Accumulate clip bounds per axis, seeded from the DOM-level composite clip.
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::INFINITY,
    );
    let mut has_clip = false;
    if let Some(dc) = dom_clip {
        min_x = dc.min_x();
        min_y = dc.min_y();
        max_x = dc.max_x();
        max_y = dc.max_y();
        has_clip = true;
    }

    // Walk ancestors. A node's own overflow clips its descendants, not itself, so
    // we start at the parent. `guard` bounds the loop in case `parent` links ever
    // form a cycle (they shouldn't, but a hit-test rebuild must never hang).
    let styled_nodes = styled_dom.styled_nodes.as_container();
    let mut cur = nodes.get(node_index).and_then(|n| n.parent);
    let mut guard = 0usize;
    while let Some(anc) = cur {
        guard += 1;
        if guard > nodes.len() {
            break;
        }
        let Some(anc_node) = nodes.get(anc) else { break };
        cur = anc_node.parent;

        let Some(anc_dom_id) = anc_node.dom_node_id else {
            continue;
        };
        let node_state = &styled_nodes[anc_dom_id].styled_node_state;
        let clips_x = get_overflow_x(styled_dom, anc_dom_id, node_state).is_clipped();
        let clips_y = get_overflow_y(styled_dom, anc_dom_id, node_state).is_clipped();
        if !clips_x && !clips_y {
            continue;
        }
        let (Some(pos), Some(size)) = (positions.get(anc), anc_node.used_size) else {
            continue;
        };
        let (ax0, ay0) = (pos.x + offset.x, pos.y + offset.y);
        if clips_x {
            min_x = min_x.max(ax0);
            max_x = max_x.min(ax0 + size.width);
            has_clip = true;
        }
        if clips_y {
            min_y = min_y.max(ay0);
            max_y = max_y.min(ay0 + size.height);
            has_clip = true;
        }
    }

    if !has_clip {
        return None;
    }

    // Replace any still-unbounded axis with a large finite extent so the stored
    // rect's `origin + size` arithmetic stays finite (no `inf - inf = NaN`).
    if !min_x.is_finite() {
        min_x = -CLIP_UNBOUNDED;
    }
    if !min_y.is_finite() {
        min_y = -CLIP_UNBOUNDED;
    }
    if !max_x.is_finite() {
        max_x = CLIP_UNBOUNDED;
    }
    if !max_y.is_finite() {
        max_y = CLIP_UNBOUNDED;
    }

    Some(LogicalRect {
        origin: LogicalPosition { x: min_x, y: min_y },
        size: LogicalSize {
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_hit_tester_empty() {
        let tester = CpuHitTester::new();
        let results = tester.hit_test(LogicalPosition { x: 100.0, y: 100.0 });
        assert!(results.is_empty());
    }

    #[test]
    fn test_point_in_rect() {
        let rect = LogicalRect {
            origin: LogicalPosition { x: 10.0, y: 10.0 },
            size: LogicalSize {
                width: 100.0,
                height: 50.0,
            },
        };

        // Inside
        assert!(point_in_rect(LogicalPosition { x: 50.0, y: 30.0 }, &rect));
        // On edge
        assert!(point_in_rect(LogicalPosition { x: 10.0, y: 10.0 }, &rect));
        // Outside
        assert!(!point_in_rect(LogicalPosition { x: 5.0, y: 5.0 }, &rect));
        assert!(!point_in_rect(LogicalPosition { x: 200.0, y: 30.0 }, &rect));
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // clip/hit geometry must round-trip bit-exactly, not "approximately"
mod autotest_generated {
    use std::collections::HashMap;

    use azul_core::dom::{Dom, FormattingContext};

    use super::*;
    use crate::{
        solver3::{
            display_list::{DisplayList, DisplayListItem, WindowLogicalRect},
            layout_tree::LayoutTree,
        },
        window::DomLayoutResult,
    };

    // -----------------------------------------------------------------------
    // fixtures
    // -----------------------------------------------------------------------

    fn p(x: f32, y: f32) -> LogicalPosition {
        LogicalPosition { x, y }
    }

    fn r(x: f32, y: f32, width: f32, height: f32) -> LogicalRect {
        LogicalRect {
            origin: p(x, y),
            size: LogicalSize { width, height },
        }
    }

    fn dom(inner: usize) -> DomId {
        DomId { inner }
    }

    /// A layout node: `dom_node_id` as a raw index (`None` = anonymous box),
    /// `size` as (w, h) (`None` = never laid out), `parent` as a node index.
    fn hot(
        dom_node_id: Option<usize>,
        size: Option<(f32, f32)>,
        parent: Option<usize>,
    ) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: Default::default(),
            dom_node_id: dom_node_id.map(NodeId::new),
            used_size: size.map(|(width, height)| LogicalSize { width, height }),
            formatting_context: FormattingContext::default(),
            parent,
        }
    }

    /// `body > div.clip > div` (`NodeId` 0, 1, 2), styled by `css_src`.
    fn styled(css_src: &str) -> StyledDom {
        let css = azul_css::parser2::new_from_str(css_src).0;
        let mut d = Dom::create_body().with_children(
            vec![Dom::create_div()
                .with_class("clip".to_string().into())
                .with_children(vec![Dom::create_div()].into())]
            .into(),
        );
        StyledDom::create(&mut d, css)
    }

    fn layout_result(
        styled_dom: StyledDom,
        nodes: Vec<LayoutNodeHot>,
        calculated_positions: PositionVec,
        items: Vec<DisplayListItem>,
    ) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes,
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions,
            viewport: LogicalRect::zero(),
            display_list: DisplayList {
                items,
                ..Default::default()
            },
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    fn virtual_view(child: usize, bounds: LogicalRect) -> DisplayListItem {
        DisplayListItem::VirtualView {
            child_dom_id: dom(child),
            bounds: WindowLogicalRect::new(bounds.origin, bounds.size),
            clip_rect: WindowLogicalRect::new(bounds.origin, bounds.size),
        }
    }

    /// Every f32 that can plausibly reach a hit test from a broken input event.
    const HOSTILE_F32: [f32; 8] = [
        0.0,
        -0.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::MIN,
        f32::MIN_POSITIVE,
    ];

    // -----------------------------------------------------------------------
    // point_in_rect  (numeric)
    // -----------------------------------------------------------------------

    #[test]
    fn point_in_rect_is_half_open_top_left_inclusive_bottom_right_exclusive() {
        let rect = r(10.0, 10.0, 100.0, 50.0);

        assert!(point_in_rect(p(10.0, 10.0), &rect), "top-left is inclusive");
        assert!(point_in_rect(p(109.999, 59.999), &rect));
        assert!(
            !point_in_rect(p(110.0, 30.0), &rect),
            "right edge is exclusive"
        );
        assert!(
            !point_in_rect(p(50.0, 60.0), &rect),
            "bottom edge is exclusive"
        );
        assert!(!point_in_rect(p(110.0, 60.0), &rect));
    }

    #[test]
    fn point_in_rect_zero_sized_rect_contains_nothing_not_even_its_origin() {
        let rect = r(0.0, 0.0, 0.0, 0.0);
        assert!(!point_in_rect(p(0.0, 0.0), &rect));
        assert!(!point_in_rect(p(-0.0, -0.0), &rect));

        let elsewhere = r(7.0, 9.0, 0.0, 0.0);
        assert!(!point_in_rect(p(7.0, 9.0), &elsewhere));
    }

    #[test]
    fn point_in_rect_negative_size_rect_is_empty() {
        // A rect whose size is negative has max < min on both axes: nothing is
        // "inside" it, and in particular the test must not silently swap the
        // edges and report a hit.
        let rect = r(100.0, 100.0, -50.0, -50.0);
        for x in [50.0_f32, 75.0, 99.0, 100.0, 125.0] {
            for y in [50.0_f32, 75.0, 99.0, 100.0, 125.0] {
                assert!(!point_in_rect(p(x, y), &rect), "({x}, {y}) must not hit");
            }
        }
    }

    #[test]
    fn point_in_rect_negative_zero_origin_still_contains_zero() {
        // -0.0 >= 0.0 and 0.0 >= -0.0 both hold: signed zero must not flip a hit.
        let rect = r(-0.0, -0.0, 10.0, 10.0);
        assert!(point_in_rect(p(0.0, 0.0), &rect));
        assert!(point_in_rect(p(-0.0, -0.0), &rect));

        let zero_origin = r(0.0, 0.0, 10.0, 10.0);
        assert!(point_in_rect(p(-0.0, -0.0), &zero_origin));
    }

    #[test]
    fn point_in_rect_nan_point_never_hits() {
        let rect = r(-1000.0, -1000.0, 5000.0, 5000.0);
        assert!(!point_in_rect(p(f32::NAN, 0.0), &rect));
        assert!(!point_in_rect(p(0.0, f32::NAN), &rect));
        assert!(!point_in_rect(p(f32::NAN, f32::NAN), &rect));
    }

    #[test]
    fn point_in_rect_nan_rect_never_hits() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let nan_origin = r(bad, 0.0, 10.0, 10.0);
            let nan_size = r(0.0, 0.0, bad, 10.0);
            // NaN origin/size makes every comparison false except the trivially
            // true ones; the only thing that matters is that it doesn't panic and
            // that a NaN box can't claim an arbitrary point.
            let _ = point_in_rect(p(5.0, 5.0), &nan_origin);
            let _ = point_in_rect(p(5.0, 5.0), &nan_size);
        }
        assert!(!point_in_rect(p(5.0, 5.0), &r(f32::NAN, 0.0, 10.0, 10.0)));
        assert!(!point_in_rect(p(5.0, 5.0), &r(0.0, 0.0, f32::NAN, 10.0)));
    }

    #[test]
    fn point_in_rect_infinite_extent_is_empty_which_is_why_clip_unbounded_exists() {
        // origin = -inf, size = +inf  =>  origin + size = NaN  =>  `x < NaN` is
        // false  =>  nothing is inside. This is exactly the trap CLIP_UNBOUNDED
        // documents; the assertion pins the failure mode so nobody "optimizes"
        // CLIP_UNBOUNDED back into f32::INFINITY.
        let infinite = LogicalRect {
            origin: p(f32::NEG_INFINITY, f32::NEG_INFINITY),
            size: LogicalSize {
                width: f32::INFINITY,
                height: f32::INFINITY,
            },
        };
        assert!(!point_in_rect(p(0.0, 0.0), &infinite));
        assert!(!point_in_rect(p(-1.0e6, 1.0e6), &infinite));
    }

    #[test]
    fn point_in_rect_clip_unbounded_extent_contains_every_realistic_coordinate() {
        // The finite stand-in that compute_node_clip uses must behave like
        // "unbounded" for any coordinate a real window can produce.
        let unbounded = r(
            -CLIP_UNBOUNDED,
            -CLIP_UNBOUNDED,
            2.0 * CLIP_UNBOUNDED,
            2.0 * CLIP_UNBOUNDED,
        );
        for c in [0.0_f32, -0.0, 1.0, -1.0, 99_999.0, -99_999.0, 1.0e6, -1.0e6] {
            assert!(point_in_rect(p(c, c), &unbounded), "{c} must be inside");
        }
        // ...but it is finite, so it does NOT swallow f32::MAX.
        assert!(!point_in_rect(p(f32::MAX, 0.0), &unbounded));
    }

    #[test]
    fn point_in_rect_saturates_at_f32_max_without_panicking() {
        // origin + size overflows to +inf here; `x < inf` is true, so the point
        // is reported inside. No debug-panic, no wraparound.
        let huge = r(f32::MAX, f32::MAX, f32::MAX, f32::MAX);
        assert!(point_in_rect(p(f32::MAX, f32::MAX), &huge));
        assert!(!point_in_rect(p(0.0, 0.0), &huge));

        let from_zero = r(0.0, 0.0, f32::MAX, f32::MAX);
        assert!(point_in_rect(p(0.0, 0.0), &from_zero));
        assert!(
            !point_in_rect(p(f32::MAX, f32::MAX), &from_zero),
            "the far edge stays exclusive even at f32::MAX"
        );
    }

    #[test]
    fn point_in_rect_never_panics_for_any_hostile_f32_combination() {
        for &x in &HOSTILE_F32 {
            for &y in &HOSTILE_F32 {
                for &w in &HOSTILE_F32 {
                    let rect = r(x, y, w, w);
                    let _ = point_in_rect(p(y, x), &rect);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // CpuHitTester::new / node_rects_total  (constructor + getter)
    // -----------------------------------------------------------------------

    #[test]
    fn new_hit_tester_is_empty_and_matches_default() {
        let tester = CpuHitTester::new();
        assert_eq!(tester.node_rects_total(), 0);
        assert!(tester.hit_test(p(0.0, 0.0)).is_empty());

        let defaulted = CpuHitTester::default();
        assert_eq!(defaulted.node_rects_total(), tester.node_rects_total());
    }

    #[test]
    fn node_rects_total_sums_entries_across_doms_and_skips_unlaid_nodes() {
        let mut results = BTreeMap::new();
        // dom 0: 2 hit-testable nodes + 1 anonymous + 1 without a used_size
        results.insert(
            dom(0),
            layout_result(
                styled(""),
                vec![
                    hot(Some(0), Some((10.0, 10.0)), None),
                    hot(Some(1), Some((10.0, 10.0)), None),
                    hot(None, Some((10.0, 10.0)), None), // anonymous box
                    hot(Some(2), None, None),            // never laid out
                ],
                vec![p(0.0, 0.0), p(0.0, 0.0), p(0.0, 0.0), p(0.0, 0.0)],
                Vec::new(),
            ),
        );
        // dom 1: 1 hit-testable node
        results.insert(
            dom(1),
            layout_result(
                styled(""),
                vec![hot(Some(0), Some((10.0, 10.0)), None)],
                vec![p(0.0, 0.0)],
                Vec::new(),
            ),
        );

        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);
        assert_eq!(tester.node_rects_total(), 3);
    }

    #[test]
    fn node_rects_total_does_not_grow_when_the_same_layout_is_rebuilt() {
        // Leak probe: rebuild_from_layout must clear, not append.
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout_result(
                styled(""),
                vec![hot(Some(0), Some((10.0, 10.0)), None)],
                vec![p(0.0, 0.0)],
                Vec::new(),
            ),
        );

        let mut tester = CpuHitTester::new();
        for _ in 0..16 {
            tester.rebuild_from_layout(&results);
            assert_eq!(tester.node_rects_total(), 1);
        }

        tester.rebuild_from_layout(&BTreeMap::new());
        assert_eq!(tester.node_rects_total(), 0);
        assert!(tester.hit_test(p(1.0, 1.0)).is_empty());
    }

    // -----------------------------------------------------------------------
    // CpuHitTester::hit_test  (numeric)
    // -----------------------------------------------------------------------

    #[test]
    fn hit_test_on_empty_tester_never_panics_for_hostile_positions() {
        let tester = CpuHitTester::new();
        for &x in &HOSTILE_F32 {
            for &y in &HOSTILE_F32 {
                assert!(tester.hit_test(p(x, y)).is_empty());
            }
        }
    }

    #[test]
    fn hit_test_with_hostile_positions_against_a_real_node_returns_no_spurious_hits() {
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout_result(
                styled(""),
                vec![hot(Some(0), Some((100.0, 100.0)), None)],
                vec![p(0.0, 0.0)],
                Vec::new(),
            ),
        );
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);

        // Sanity: the node IS hittable at a normal coordinate.
        assert_eq!(tester.hit_test(p(50.0, 50.0)).len(), 1);

        for pos in [
            p(f32::NAN, f32::NAN),
            p(f32::NAN, 50.0),
            p(50.0, f32::NAN),
            p(f32::INFINITY, f32::INFINITY),
            p(f32::NEG_INFINITY, f32::NEG_INFINITY),
            p(f32::MAX, f32::MAX),
            p(f32::MIN, f32::MIN),
        ] {
            assert!(
                tester.hit_test(pos).is_empty(),
                "({}, {}) must not hit a 0,0,100x100 node",
                pos.x,
                pos.y
            );
        }

        // Zero and negative zero are inside (origin is inclusive).
        assert_eq!(tester.hit_test(p(0.0, 0.0)).len(), 1);
        assert_eq!(tester.hit_test(p(-0.0, -0.0)).len(), 1);
        // The exclusive far edge.
        assert!(tester.hit_test(p(100.0, 100.0)).is_empty());
        assert_eq!(tester.hit_test(p(99.999, 99.999)).len(), 1);
    }

    #[test]
    fn hit_test_returns_topmost_first() {
        // Two fully overlapping siblings: the one that paints last (higher index)
        // must come back first.
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout_result(
                styled(""),
                vec![
                    hot(Some(1), Some((100.0, 100.0)), None),
                    hot(Some(2), Some((100.0, 100.0)), None),
                ],
                vec![p(0.0, 0.0), p(0.0, 0.0)],
                Vec::new(),
            ),
        );
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);

        assert_eq!(
            tester.hit_test(p(50.0, 50.0)),
            vec![(dom(0), NodeId::new(2)), (dom(0), NodeId::new(1))]
        );
    }

    #[test]
    fn hit_test_skips_nodes_with_no_calculated_position() {
        // `calculated_positions` shorter than `nodes` is a torn/partial layout:
        // the extra nodes must be dropped, not indexed out of bounds.
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout_result(
                styled(""),
                vec![
                    hot(Some(0), Some((100.0, 100.0)), None),
                    hot(Some(1), Some((100.0, 100.0)), None),
                    hot(Some(2), Some((100.0, 100.0)), None),
                ],
                vec![p(0.0, 0.0)], // only node 0 has a position
                Vec::new(),
            ),
        );
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);

        assert_eq!(tester.node_rects_total(), 1);
        assert_eq!(tester.hit_test(p(50.0, 50.0)), vec![(dom(0), NodeId::ZERO)]);
    }

    #[test]
    fn hit_test_respects_an_overflow_hidden_ancestor() {
        // body(0) 500x500 > div.clip(1) 100x100 overflow:hidden > div(2) 400x400.
        // A point at (200,200) is inside node 2's rect but scrolled/clipped out of
        // its ancestor, so only the body may claim it.
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout_result(
                styled("div.clip { overflow: hidden; }"),
                vec![
                    hot(Some(0), Some((500.0, 500.0)), None),
                    hot(Some(1), Some((100.0, 100.0)), Some(0)),
                    hot(Some(2), Some((400.0, 400.0)), Some(1)),
                ],
                vec![p(0.0, 0.0), p(0.0, 0.0), p(0.0, 0.0)],
                Vec::new(),
            ),
        );
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);

        assert_eq!(
            tester.hit_test(p(50.0, 50.0)),
            vec![
                (dom(0), NodeId::new(2)),
                (dom(0), NodeId::new(1)),
                (dom(0), NodeId::new(0)),
            ],
            "inside the clip: all three nodes are hit, topmost first"
        );
        assert_eq!(
            tester.hit_test(p(200.0, 200.0)),
            vec![(dom(0), NodeId::new(0))],
            "outside the clip: the clipped-out child must not eat the event"
        );
    }

    // -----------------------------------------------------------------------
    // CpuHitTester::rebuild_from_layout  (VirtualView placement)
    // -----------------------------------------------------------------------

    #[test]
    fn rebuild_from_layout_with_no_doms_is_a_no_op() {
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&BTreeMap::new());
        assert_eq!(tester.node_rects_total(), 0);
        assert!(tester.hit_test(p(0.0, 0.0)).is_empty());
    }

    #[test]
    fn rebuild_translates_and_clips_virtual_view_child_doms() {
        // Host dom 0 hosts child dom 1 at (100,100) 50x50. The child lays out in
        // local coordinates with a 200x200 node at (0,0): it must be translated to
        // (100,100) AND clipped to the 50x50 composite box, otherwise it claims
        // pointer events across the whole window (the azul-maps tile-grid bug).
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout_result(
                styled(""),
                Vec::new(),
                Vec::new(),
                vec![virtual_view(1, r(100.0, 100.0, 50.0, 50.0))],
            ),
        );
        results.insert(
            dom(1),
            layout_result(
                styled(""),
                vec![hot(Some(1), Some((200.0, 200.0)), None)],
                vec![p(0.0, 0.0)],
                Vec::new(),
            ),
        );

        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);

        assert!(
            tester.hit_test(p(10.0, 10.0)).is_empty(),
            "the child's local (10,10) is not its window position"
        );
        assert_eq!(
            tester.hit_test(p(120.0, 120.0)),
            vec![(dom(1), NodeId::new(1))],
            "translated into the host's VirtualView bounds"
        );
        assert!(
            tester.hit_test(p(180.0, 180.0)).is_empty(),
            "inside the child's 200x200 rect but outside the 50x50 composite clip"
        );
    }

    #[test]
    fn rebuild_accumulates_offsets_through_nested_virtual_views() {
        // dom0 --VV(10,10)--> dom1 --VV(5,5 local)--> dom2, whose node sits at
        // local (0,0): absolute origin must be (15,15).
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout_result(
                styled(""),
                Vec::new(),
                Vec::new(),
                vec![virtual_view(1, r(10.0, 10.0, 200.0, 200.0))],
            ),
        );
        results.insert(
            dom(1),
            layout_result(
                styled(""),
                Vec::new(),
                Vec::new(),
                vec![virtual_view(2, r(5.0, 5.0, 100.0, 100.0))],
            ),
        );
        results.insert(
            dom(2),
            layout_result(
                styled(""),
                vec![hot(Some(1), Some((20.0, 20.0)), None)],
                vec![p(0.0, 0.0)],
                Vec::new(),
            ),
        );

        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);

        assert_eq!(
            tester.hit_test(p(16.0, 16.0)),
            vec![(dom(2), NodeId::new(1))]
        );
        assert!(
            tester.hit_test(p(14.0, 14.0)).is_empty(),
            "(14,14) is before the doubly-offset origin (15,15)"
        );
        assert!(tester.hit_test(p(36.0, 36.0)).is_empty());
    }

    #[test]
    fn rebuild_ignores_virtual_views_pointing_at_a_missing_child_dom() {
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout_result(
                styled(""),
                vec![hot(Some(0), Some((10.0, 10.0)), None)],
                vec![p(0.0, 0.0)],
                vec![virtual_view(42, r(0.0, 0.0, 10.0, 10.0))],
            ),
        );

        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);

        assert_eq!(tester.node_rects_total(), 1);
        assert_eq!(tester.hit_test(p(5.0, 5.0)), vec![(dom(0), NodeId::ZERO)]);
    }

    #[test]
    fn rebuild_terminates_on_a_cyclic_virtual_view_graph() {
        // dom1 hosts dom2 and dom2 hosts dom1: neither is reachable from the root
        // dom, so neither gets placed. The placement loop is bounded, so this must
        // terminate (a hang here would freeze every layout pass).
        let mut results = BTreeMap::new();
        results.insert(
            dom(1),
            layout_result(
                styled(""),
                vec![hot(Some(1), Some((10.0, 10.0)), None)],
                vec![p(0.0, 0.0)],
                vec![virtual_view(2, r(1.0, 1.0, 10.0, 10.0))],
            ),
        );
        results.insert(
            dom(2),
            layout_result(
                styled(""),
                vec![hot(Some(1), Some((10.0, 10.0)), None)],
                vec![p(0.0, 0.0)],
                vec![virtual_view(1, r(2.0, 2.0, 10.0, 10.0))],
            ),
        );

        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);
        assert_eq!(tester.node_rects_total(), 2);
    }

    #[test]
    fn rebuild_handles_a_virtual_view_with_hostile_bounds() {
        // A NaN/infinite composite box must not produce a NaN clip that panics or
        // makes the child hit-testable everywhere.
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
            let mut results = BTreeMap::new();
            results.insert(
                dom(0),
                layout_result(
                    styled(""),
                    Vec::new(),
                    Vec::new(),
                    vec![virtual_view(1, r(bad, bad, bad, bad))],
                ),
            );
            results.insert(
                dom(1),
                layout_result(
                    styled(""),
                    vec![hot(Some(1), Some((20.0, 20.0)), None)],
                    vec![p(0.0, 0.0)],
                    Vec::new(),
                ),
            );

            let mut tester = CpuHitTester::new();
            tester.rebuild_from_layout(&results);
            assert_eq!(tester.node_rects_total(), 1);
            // Whatever the clip degenerates to, hit testing must not panic.
            let _ = tester.hit_test(p(10.0, 10.0));
            let _ = tester.hit_test(p(f32::NAN, 0.0));
        }
    }

    // -----------------------------------------------------------------------
    // compute_node_clip  (numeric)
    // -----------------------------------------------------------------------

    #[test]
    fn compute_node_clip_without_ancestors_or_dom_clip_is_unclipped() {
        let styled_dom = styled("");
        let nodes = vec![hot(Some(0), Some((10.0, 10.0)), None)];
        let positions: PositionVec = vec![p(0.0, 0.0)];

        assert_eq!(
            compute_node_clip(&styled_dom, &nodes, &positions, 0, p(0.0, 0.0), None),
            None
        );
    }

    #[test]
    fn compute_node_clip_out_of_bounds_node_index_does_not_panic() {
        let styled_dom = styled("");
        let nodes: Vec<LayoutNodeHot> = Vec::new();
        let positions: PositionVec = Vec::new();

        for idx in [0_usize, 1, 999, usize::MAX] {
            assert_eq!(
                compute_node_clip(&styled_dom, &nodes, &positions, idx, p(0.0, 0.0), None),
                None
            );
            // ...and with a DOM clip it still returns exactly that clip.
            let clip = compute_node_clip(
                &styled_dom,
                &nodes,
                &positions,
                idx,
                p(0.0, 0.0),
                Some(r(1.0, 2.0, 3.0, 4.0)),
            );
            assert_eq!(clip, Some(r(1.0, 2.0, 3.0, 4.0)));
        }
    }

    #[test]
    fn compute_node_clip_round_trips_a_dom_clip_when_no_ancestor_clips() {
        // encode == decode: with no clipping ancestor the composite box must come
        // back byte-identical, offset included (the offset is already baked into
        // the placement, so it must NOT be applied twice).
        let styled_dom = styled("");
        let nodes = vec![hot(Some(0), Some((10.0, 10.0)), None)];
        let positions: PositionVec = vec![p(0.0, 0.0)];
        let dom_clip = r(100.0, 200.0, 50.0, 25.0);

        let clip = compute_node_clip(
            &styled_dom,
            &nodes,
            &positions,
            0,
            p(100.0, 200.0),
            Some(dom_clip),
        )
        .expect("dom_clip must survive");

        assert_eq!(clip.origin.x, dom_clip.origin.x);
        assert_eq!(clip.origin.y, dom_clip.origin.y);
        assert_eq!(clip.size.width, dom_clip.size.width);
        assert_eq!(clip.size.height, dom_clip.size.height);
    }

    #[test]
    fn compute_node_clip_never_lets_nan_escape_into_the_clip_rect() {
        let styled_dom = styled("");
        let nodes = vec![hot(Some(0), Some((10.0, 10.0)), None)];
        let positions: PositionVec = vec![p(0.0, 0.0)];

        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for dom_clip in [
                r(bad, 0.0, 10.0, 10.0),
                r(0.0, bad, 10.0, 10.0),
                r(0.0, 0.0, bad, 10.0),
                r(0.0, 0.0, 10.0, bad),
                r(bad, bad, bad, bad),
            ] {
                let clip = compute_node_clip(
                    &styled_dom,
                    &nodes,
                    &positions,
                    0,
                    p(0.0, 0.0),
                    Some(dom_clip),
                )
                .expect("a dom_clip always yields a clip");

                assert!(
                    clip.origin.x.is_finite()
                        && clip.origin.y.is_finite()
                        && clip.size.width.is_finite()
                        && clip.size.height.is_finite(),
                    "clip {clip:?} from dom_clip {dom_clip:?} must stay finite"
                );
                assert!(clip.size.width >= 0.0 && clip.size.height >= 0.0);
                assert!(
                    clip.max_x().is_finite() && clip.max_y().is_finite(),
                    "origin + size must not overflow to inf/NaN"
                );
                // point_in_rect over the result must be a real answer, not a NaN
                // comparison that silently drops every event.
                let _ = point_in_rect(p(0.0, 0.0), &clip);
            }
        }
    }

    #[test]
    fn compute_node_clip_clamps_an_infinite_dom_clip_to_clip_unbounded() {
        let styled_dom = styled("");
        let nodes = vec![hot(Some(0), Some((10.0, 10.0)), None)];
        let positions: PositionVec = vec![p(0.0, 0.0)];

        let clip = compute_node_clip(
            &styled_dom,
            &nodes,
            &positions,
            0,
            p(0.0, 0.0),
            Some(LogicalRect {
                origin: p(0.0, 0.0),
                size: LogicalSize {
                    width: f32::INFINITY,
                    height: f32::INFINITY,
                },
            }),
        )
        .expect("a dom_clip always yields a clip");

        assert_eq!(clip.origin.x, 0.0);
        assert_eq!(clip.origin.y, 0.0);
        assert_eq!(clip.size.width, CLIP_UNBOUNDED);
        assert_eq!(clip.size.height, CLIP_UNBOUNDED);
        assert!(point_in_rect(p(1.0e6, 1.0e6), &clip));
    }

    #[test]
    fn compute_node_clip_saturates_a_negative_sized_dom_clip_to_zero_not_negative() {
        let styled_dom = styled("");
        let nodes = vec![hot(Some(0), Some((10.0, 10.0)), None)];
        let positions: PositionVec = vec![p(0.0, 0.0)];

        let clip = compute_node_clip(
            &styled_dom,
            &nodes,
            &positions,
            0,
            p(0.0, 0.0),
            Some(r(100.0, 100.0, -50.0, -50.0)),
        )
        .expect("a dom_clip always yields a clip");

        assert_eq!(clip.size.width, 0.0);
        assert_eq!(clip.size.height, 0.0);
        assert!(!point_in_rect(p(100.0, 100.0), &clip));
        assert!(!point_in_rect(p(75.0, 75.0), &clip));
    }

    #[test]
    fn compute_node_clip_intersects_a_clipping_ancestor_with_the_dom_clip() {
        // ancestor div.clip at (10,10) 100x50; dom_clip (0,0) 60x60
        // => intersection (10,10) 50x50
        let styled_dom = styled("div.clip { overflow: hidden; }");
        let nodes = vec![
            hot(Some(0), Some((500.0, 500.0)), None),
            hot(Some(1), Some((100.0, 50.0)), Some(0)),
            hot(Some(2), Some((400.0, 400.0)), Some(1)),
        ];
        let positions: PositionVec = vec![p(0.0, 0.0), p(10.0, 10.0), p(10.0, 10.0)];

        let clip = compute_node_clip(
            &styled_dom,
            &nodes,
            &positions,
            2,
            p(0.0, 0.0),
            Some(r(0.0, 0.0, 60.0, 60.0)),
        )
        .expect("an overflow:hidden ancestor must clip");

        assert_eq!(clip.origin.x, 10.0);
        assert_eq!(clip.origin.y, 10.0);
        assert_eq!(clip.size.width, 50.0);
        assert_eq!(clip.size.height, 50.0);
    }

    #[test]
    fn compute_node_clip_applies_the_offset_to_the_ancestor_box() {
        let styled_dom = styled("div.clip { overflow: hidden; }");
        let nodes = vec![
            hot(Some(0), Some((500.0, 500.0)), None),
            hot(Some(1), Some((100.0, 50.0)), Some(0)),
            hot(Some(2), Some((400.0, 400.0)), Some(1)),
        ];
        let positions: PositionVec = vec![p(0.0, 0.0), p(10.0, 10.0), p(10.0, 10.0)];

        let clip = compute_node_clip(&styled_dom, &nodes, &positions, 2, p(1000.0, 2000.0), None)
            .expect("an overflow:hidden ancestor must clip");

        assert_eq!(clip.origin.x, 1010.0);
        assert_eq!(clip.origin.y, 2010.0);
        assert_eq!(clip.size.width, 100.0);
        assert_eq!(clip.size.height, 50.0);
    }

    #[test]
    fn compute_node_clip_leaves_the_unclipped_axis_unbounded() {
        // overflow-x: hidden / overflow-y: visible — the y axis must stay
        // unbounded (finite stand-in), not collapse onto the ancestor's box.
        let styled_dom = styled("div.clip { overflow-x: hidden; }");
        let nodes = vec![
            hot(Some(0), Some((500.0, 500.0)), None),
            hot(Some(1), Some((100.0, 50.0)), Some(0)),
            hot(Some(2), Some((400.0, 400.0)), Some(1)),
        ];
        let positions: PositionVec = vec![p(0.0, 0.0), p(10.0, 10.0), p(10.0, 10.0)];

        let clip = compute_node_clip(&styled_dom, &nodes, &positions, 2, p(0.0, 0.0), None)
            .expect("overflow-x: hidden must clip the x axis");

        assert_eq!(clip.origin.x, 10.0);
        assert_eq!(clip.size.width, 100.0);
        assert_eq!(clip.origin.y, -CLIP_UNBOUNDED);
        assert_eq!(clip.size.height, 2.0 * CLIP_UNBOUNDED);
        assert!(clip.max_y().is_finite());

        // A point far below the ancestor is still inside the clip (y unbounded),
        // but a point to the right of it is not.
        assert!(point_in_rect(p(50.0, 900_000.0), &clip));
        assert!(!point_in_rect(p(500.0, 20.0), &clip));
    }

    #[test]
    fn compute_node_clip_skips_a_clipping_ancestor_that_was_never_laid_out() {
        // used_size: None on the clipping ancestor => nothing to intersect with;
        // it must be skipped rather than contributing a garbage/zero box.
        let styled_dom = styled("div.clip { overflow: hidden; }");
        let nodes = vec![
            hot(Some(0), Some((500.0, 500.0)), None),
            hot(Some(1), None, Some(0)), // clips, but has no used_size
            hot(Some(2), Some((400.0, 400.0)), Some(1)),
        ];
        let positions: PositionVec = vec![p(0.0, 0.0), p(10.0, 10.0), p(10.0, 10.0)];

        assert_eq!(
            compute_node_clip(&styled_dom, &nodes, &positions, 2, p(0.0, 0.0), None),
            None
        );
    }

    #[test]
    fn compute_node_clip_terminates_on_a_parent_cycle() {
        // Two anonymous boxes that are each other's parent. The `guard` counter is
        // the only thing standing between this and an infinite loop inside a
        // hit-test rebuild.
        let styled_dom = styled("");
        let nodes = vec![
            hot(None, Some((10.0, 10.0)), Some(1)),
            hot(None, Some((10.0, 10.0)), Some(0)),
        ];
        let positions: PositionVec = vec![p(0.0, 0.0), p(0.0, 0.0)];

        assert_eq!(
            compute_node_clip(&styled_dom, &nodes, &positions, 0, p(0.0, 0.0), None),
            None
        );
        // The DOM clip still survives the bounded walk.
        assert_eq!(
            compute_node_clip(
                &styled_dom,
                &nodes,
                &positions,
                1,
                p(0.0, 0.0),
                Some(r(0.0, 0.0, 5.0, 5.0))
            ),
            Some(r(0.0, 0.0, 5.0, 5.0))
        );
    }

    #[test]
    fn compute_node_clip_terminates_on_a_self_parent_cycle() {
        let styled_dom = styled("");
        let nodes = vec![hot(None, Some((10.0, 10.0)), Some(0))];
        let positions: PositionVec = vec![p(0.0, 0.0)];

        assert_eq!(
            compute_node_clip(&styled_dom, &nodes, &positions, 0, p(0.0, 0.0), None),
            None
        );
    }

    #[test]
    fn compute_node_clip_tolerates_a_parent_index_past_the_end_of_the_node_slice() {
        let styled_dom = styled("");
        let nodes = vec![hot(Some(0), Some((10.0, 10.0)), Some(usize::MAX))];
        let positions: PositionVec = vec![p(0.0, 0.0)];

        assert_eq!(
            compute_node_clip(&styled_dom, &nodes, &positions, 0, p(0.0, 0.0), None),
            None
        );
    }
}
