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
            let (offset, dom_clip) = match placements.get(dom_id) {
                Some(b) => (b.origin, Some(*b)),
                None => (LogicalPosition::zero(), None),
            };

            // Walk the layout nodes and their computed positions
            for (idx, node) in nodes.iter().enumerate() {
                // Only include nodes that map to a real DOM node
                let node_id = match node.dom_node_id {
                    Some(id) => id,
                    None => continue, // skip anonymous boxes
                };

                // Get the position for this layout node
                let pos = match positions.get(idx) {
                    Some(p) => *p,
                    None => continue,
                };

                // Get the computed size
                let size = match node.used_size {
                    Some(s) => s,
                    None => continue,
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
