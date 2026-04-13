//! Headless backend for CPU-only rendering without a display server.
//!
//! This module provides the resource management and rendering pipeline for
//! running Azul applications without any platform windowing APIs. It works
//! in combination with `HeadlessWindow` (in `dll/src/desktop/shell2/headless/`) which
//! provides the `PlatformWindow` trait implementation.
//!
//! # Architecture
//!
//! The headless backend replaces the WebRender GPU pipeline with a purely
//! CPU-based approach. Here's how each resource type is managed:
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │                    Normal (GPU) Path                     │
//! │                                                          │
//! │  LayoutWindow  ──→  DisplayList  ──→  WebRender  ──→  GL │
//! │       │                                    │              │
//! │       │              RenderApi   ←─── Renderer            │
//! │       │            (font/image              │              │
//! │       │             registration)     AsyncHitTester      │
//! │       │                                                   │
//! └──────────────────────────────────────────────────────────┘
//!
//! ┌──────────────────────────────────────────────────────────┐
//! │                  Headless (CPU) Path                      │
//! │                                                          │
//! │  LayoutWindow  ──→  DisplayList  ──→  cpurender  ──→  PNG│
//! │       │                                    │              │
//! │       │         HeadlessResources    (agg-rust           │
//! │       │         (font/image            Pixmap)            │
//! │       │          management)                              │
//! │       │                             CpuHitTester          │
//! │       │                                                   │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Key Differences from GPU Path
//!
//! | Concern             | GPU Path                | Headless Path          |
//! |---------------------|-------------------------|------------------------|
//! | Window              | NSWindow / HWND / X11   | HeadlessWindow (no-op) |
//! | OpenGL              | GlContextPtr            | None                   |
//! | Renderer            | webrender::Renderer     | None (skip)            |
//! | RenderApi           | WrRenderApi             | None (skip)            |
//! | Hit Testing         | AsyncHitTester (WR)     | CpuHitTester (layout)  |
//! | Font Registration   | RenderApi::add_font()   | FontManager only       |
//! | Image Registration  | RenderApi::add_image()  | ImageCache only        |
//! | Frame Generation    | generate_frame() + WR   | generate_frame() only  |
//! | Screenshot          | glReadPixels            | cpurender → Pixmap     |
//! | Display List        | WR DisplayList          | solver3 DisplayList    |
//! | Present/Swap        | swapBuffers             | no-op                  |
//!
//! ## Resource Lifecycle (Headless)
//!
//! Fonts and images are managed entirely through `LayoutWindow`:
//!
//! ```text
//! Font Loading:
//!   1. FcFontCache discovers system fonts (same as GPU path)
//!   2. FontManager loads + caches parsed fonts
//!   3. TextLayoutCache shapes text and caches glyph positions
//!   4. cpurender reads glyph outlines directly from ParsedFont
//!      (no GPU texture atlas needed)
//!
//! Image Loading:
//!   1. ImageCache stores decoded images (same as GPU path)
//!   2. cpurender blits pixels directly from DecodedImage
//!      (no GPU texture upload needed)
//! ```
//!
//! ## Usage
//!
//! The headless backend is activated by setting `AZUL_HEADLESS=1`:
//!
//! ```bash
//! AZUL_HEADLESS=1 ./my_azul_app
//! ```
//!
//! Or combined with the debug server for remote inspection:
//!
//! ```bash
//! AZUL_HEADLESS=1 AZUL_DEBUG=1 ./my_azul_app
//! ```

use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
};

/// CPU-based hit tester that works without WebRender.
///
/// In the GPU path, hit testing is done by `AsyncHitTester` which queries
/// WebRender's spatial tree. In headless mode, we do hit testing directly
/// against the layout results (positioned rectangles).
///
/// This is actually simpler and faster than the WebRender path, since we
/// don't need to go through the compositor's spatial tree — we just walk
/// the layout result nodes and check point-in-rect.
pub struct CpuHitTester {
    /// Cached hit test results from the last layout.
    /// Maps DomId -> list of (NodeId, positioned rect) sorted by paint order.
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

impl CpuHitTester {
    /// Create a new empty hit tester.
    pub fn new() -> Self {
        Self {
            node_rects: BTreeMap::new(),
        }
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

        for (dom_id, layout_result) in layout_results {
            let mut entries = Vec::new();

            let positions = &layout_result.calculated_positions;
            let nodes = &layout_result.layout_tree.nodes;

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
                    origin: pos,
                    size,
                };

                entries.push(HitTestEntry {
                    node_id,
                    rect,
                    clip: None, // TODO: compute clip chains
                    pointer_events_none: false, // TODO: check CSS property
                });
            }

            self.node_rects.insert(*dom_id, entries);
        }
    }

    /// Perform a hit test at the given position.
    ///
    /// Returns nodes hit at (x, y) in reverse paint order (topmost first).
    pub fn hit_test(
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
