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
//! Activated with `AZ_BACKEND=headless` (optionally `AZ_DEBUG=<port>` for the debug server).

use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::{FullHitTest, ScrollbarHitId},
    spaces::{BorderBoxLocal, ContentBoxLocal, Inclusivity, StaticLayoutPoint},
    styled_dom::StyledDom,
};

use crate::{
    solver3::{
        getters::{get_overflow_x, get_overflow_y},
        layout_tree::{LayoutNodeHot, LayoutNodeId},
        scroll_chain::{ScrollChain, ScrollChains},
        PositionVec,
    },
    window::DomLayoutResult,
};

/// Large finite half-extent used in place of `f32::INFINITY` for clip axes that
/// are not constrained by any ancestor. Keeping it finite avoids `NaN` in
/// `point_in_rect` (`origin + size` would be `inf - inf = NaN`) while staying
/// far outside any realistic logical-pixel coordinate.
const CLIP_UNBOUNDED: f32 = 1.0e7;

/// The layout-side hit tester of every backend (`WebRender`'s own is not
/// asked: `CommonWindowState::perform_hit_test`).
///
/// It answers from the layout results - the boxes, in paint order - and from
/// the frames the display list paints each box in (its *chain*): a window
/// point is taken through the inverse of every frame around a box, outermost
/// first, into the static space the box is kept in, with the values the
/// frames have NOW (scroll offsets, the matrices of the last frame, how far
/// a view's content has moved), and tested against the box and the clips as
/// they are drawn.
///
/// # What one query costs
///
/// Not a walk over every box: a dom the point is not shown through (outside
/// its `VirtualView`'s box) is skipped whole; the boxes of a dom are grouped
/// by chain, a big group (the labels of a map under one zoomed root) is
/// found through a grid over its static rects (the grid does not depend on
/// any live value, so nothing invalidates it but a rebuild), and a chain's
/// point is worked out once per query and only when a group or a clip asks
/// for it. What remains linear is the number of distinct chains in the doms
/// the point is shown through - one inverse per turned label - not the
/// number of boxes ([`HitTestWork`]).
#[derive(Debug)]
pub struct CpuHitTester {
    /// Cached hit test results from the last layout.
    /// Maps `DomId` -> list of (`NodeId`, positioned rect) sorted by paint order.
    node_rects: BTreeMap<DomId, Vec<HitTestEntry>>,
    /// Interned ancestor chains (scroll frames, reference frames, the steps
    /// into `VirtualView` child doms), as a trie: entry 0 is always the
    /// empty chain, every other entry is its parent's chain with one more
    /// link inside it. Entry / clip `chain` values index into this. A box is
    /// painted where its chain's links take its static rect, innermost link
    /// first ([`link_to_screen`]) - each `PushReferenceFrame` about its own
    /// box's corner, each scroll frame inside the frames around it: the
    /// nesting `cpurender::raster` and `compositor2` paint with, so pixels
    /// and pointer targets cannot disagree.
    chains: Vec<ChainNode>,
    /// Every node that got a `PushScrollFrame` (from
    /// `DomLayoutResult::scroll_ids`, the same set the display list uses),
    /// translated into window space, with the chain of its STRICT scroll
    /// ancestors (a container's own viewport box does not move when it
    /// scrolls — only its content does).
    scroll_containers: Vec<ScrollContainerEntry>,
    /// `VirtualView` child-DOM placements in window space (static coords).
    dom_placements: BTreeMap<DomId, LogicalRect>,
    /// The child doms each dom shows through `VirtualView`s, with the index
    /// of the view's item in its display list, the LAST painted first.
    children_of: BTreeMap<DomId, Vec<(usize, DomId)>>,
    /// Per child dom: the `VirtualView` node that shows it, in its host dom.
    /// A view scrolls its child dom's content without a scroll frame (its
    /// offset is baked into the view's item), so it is in no chain of the
    /// child's boxes - and is still what a wheel over them scrolls
    /// (`wheel_frames`).
    view_hosts: BTreeMap<DomId, (DomId, NodeId)>,
    /// Per child dom: its `VirtualView` item, the `content_offset` its boxes
    /// were placed with, and how far the item has moved the content since
    /// ([`Self::sync_view_placements`]) - the value of the dom's
    /// [`HitChainLink::View`].
    view_items: BTreeMap<DomId, ViewItem>,
    /// Per dom: its boxes grouped by chain, and the viewports it shows
    /// through ([`DomIndex`]).
    index: BTreeMap<DomId, DomIndex>,
    /// The matrix each node's reference frame was BUILT with
    /// (`PushReferenceFrame::initial_transform` of the list the tester was
    /// built from). What the renderers paint a frame with while its key has
    /// no live value - the frames between the end of an animation, which
    /// drops the key, and the list that is built without the frame - and so
    /// what the frame is hit with then ([`painted_or_baked`]). Not in
    /// [`HitChainLink`]: a link is a key of the chain table.
    baked_frames: std::collections::HashMap<(DomId, NodeId), azul_core::transform::ComputedTransform3D>,
}

/// Where a child dom's `VirtualView` item is, and how far it has moved the
/// child's content since the tester was built.
#[derive(Debug, Clone, Copy)]
struct ViewItem {
    /// The dom whose display list holds the item, and the item's index there
    /// when the tester was built.
    host: DomId,
    item: usize,
    /// The item's `content_offset` then: the child's boxes are placed with it.
    built_offset: LogicalPosition,
    /// The item's `content_offset` now, minus `built_offset`.
    shift: LogicalPosition,
}

/// What one query did ([`CpuHitTester::hit_test_work`]): counts, for the
/// guards that a pointer move is not a walk over every box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HitTestWork {
    /// Boxes (and scrollbars) whose rect was tested against the point.
    pub entries_tested: usize,
    /// Chain groups looked at: a group is the boxes of one dom under one
    /// chain.
    pub groups_visited: usize,
    /// Links inverted with their live value: the scroll offset, matrix or
    /// view move looked up and applied.
    pub links_resolved: usize,
}

/// The boxes of one dom, for a query: the viewports the dom shows through
/// and its entries by chain.
#[derive(Debug, Default)]
struct DomIndex {
    /// The enclosing `VirtualView` boxes, each with the chain it is painted
    /// in. Every entry of the dom carries them among its clips
    /// (`compute_node_clips`), so a point outside one hits nothing of the
    /// dom and none of its entries is looked at.
    dom_clips: Vec<(LogicalRect, u32)>,
    /// One group per chain the dom's entries are painted in.
    groups: Vec<ChainGroup>,
    /// Per DOM node index: its topmost box entry (`u32::MAX`: none). Empty
    /// for a dom whose node ids are too sparse to list.
    top_entry_of_node: Vec<u32>,
    /// The groups a query that uses `frame_bounds` looks at whatever the
    /// point (indices into `groups`): every group that is not in `framed`.
    plain: Vec<u32>,
    /// The groups whose innermost link is a reference frame of one of this
    /// dom's own nodes (a turned label: one frame, one chain, one group), by
    /// the chain around the frame - for the chains that hold
    /// [`FRAME_BOUNDS_MIN_GROUPS`] of them or more ([`Self::split_framed`]).
    framed: Vec<FramedGroups>,
    /// Where the framed groups are painted, in the space of the chain around
    /// them, for the matrices the stamps stand for; `None` while not worked
    /// out ([`CpuHitTester::sync_frame_bounds`]).
    frame_bounds: Option<FrameBounds>,
    /// The dom's transform stamps as the last sync found them, and in how
    /// many syncs in a row.
    seen_stamps: TransformStamps,
    seen_times: u8,
}

/// The stamps of the six maps a dom's reference frames are read from
/// (`azul_core::gpu::GpuValueCache::transform_stamps`): while they are what
/// they were, every frame of the dom has the matrix it had. `[0; 6]`: the
/// dom has no frame.
pub type TransformStamps = [u64; 6];

/// The framed groups (indices into [`DomIndex::groups`]) under one chain.
#[derive(Debug)]
struct FramedGroups {
    parent: u32,
    groups: Vec<u32>,
}

/// Where the framed groups of a dom are painted ([`DomIndex::frame_bounds`]).
#[derive(Debug)]
struct FrameBounds {
    /// The stamps of the matrices the bounds were made with.
    stamps: TransformStamps,
    parents: Vec<ParentBounds>,
}

/// The framed groups under one chain, by where their boxes are painted in
/// that chain's space: a group can hold the query point only if the point,
/// in the parent's space, is in the group's painted bounds.
#[derive(Debug)]
struct ParentBounds {
    parent: u32,
    /// Groups looked at whatever the point: no bounds (a corner behind the
    /// eye of a perspective, a box that is not finite), or bounds over many
    /// cells.
    always: Vec<u32>,
    /// The other groups, by their painted bounds.
    grid: Option<EntryGrid>,
}

/// A chain holds this many framed groups before their painted bounds are
/// kept: below it, inverting each frame per query is as cheap.
const FRAME_BOUNDS_MIN_GROUPS: usize = 64;
/// The painted bounds are worked out at the sync that finds the stamps
/// unchanged for the this-many-th time in a row: while a matrix changes
/// between the queries (an animation, a zoom of the frame around the
/// labels), keeping bounds would cost a pass over every frame per query for
/// nothing.
const FRAME_BOUNDS_AFTER_SYNCS: u8 = 3;

/// The entries of one dom that are painted in one chain: they share the
/// static space the point is mapped into, so one mapped point tests them all.
#[derive(Debug)]
struct ChainGroup {
    chain: u32,
    /// Entries tested on every query of the group (indices into the dom's
    /// entries): all of a small group, and of a big one those the grid does
    /// not hold (a box over many cells, a rect that is not finite).
    always: Vec<u32>,
    /// The other entries of a big group, by where their rect is.
    grid: Option<EntryGrid>,
    /// The box around every rect of the group, in the group's static space
    /// (`min x, min y, max x, max y`, the far edges as `point_in_rect` forms
    /// them); `None` when a rect is not finite.
    bounds: Option<[f32; 4]>,
}

/// A group is put into a grid from this many entries on.
const GRID_MIN_ENTRIES: usize = 32;
/// An entry over more cells than this is kept out of the grid (the ground
/// under a map, a tile): it would be listed in every cell it covers.
const GRID_MAX_CELLS_PER_ENTRY: usize = 16;
/// The most cells a grid has along one axis.
const GRID_MAX_SIDE: usize = 128;

/// A uniform grid over the static rects of one chain group: a cell lists the
/// entries whose rect reaches into it, in paint order. The rects are static
/// (the chain's live values move the POINT, not the boxes), so the grid is
/// valid from one rebuild of the tester to the next.
#[derive(Debug)]
struct EntryGrid {
    /// The bounds of the rects in the grid.
    min: LogicalPosition,
    max: LogicalPosition,
    cols: usize,
    rows: usize,
    /// Cells per logical px.
    cells_per_x: f32,
    cells_per_y: f32,
    /// Cell `c` (row-major) lists `items[starts[c]..starts[c + 1]]`.
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl EntryGrid {
    /// The column (or row) of coordinate `v`: the same expression for a
    /// rect's edges and for a query point, and it never decreases with `v`,
    /// so a point inside a rect lands in a cell between the cells of the
    /// rect's edges.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to the grid
    fn cell_along(v: f32, min: f32, cells_per_px: f32, count: usize) -> usize {
        // `as` saturates and maps NaN to 0: any `v` lands in a cell.
        let cell = ((v - min) * cells_per_px) as i64;
        (cell.max(0) as usize).min(count.saturating_sub(1))
    }

    /// The grid over `rects` (entry index, rect), or `None` when they have
    /// no extent to divide. Entries the grid does not take are pushed to
    /// `always`.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)] // cell counts
    fn build(rects: &[(u32, LogicalRect)], always: &mut Vec<u32>) -> Option<Self> {
        let finite = |r: &LogicalRect| {
            r.origin.x.is_finite()
                && r.origin.y.is_finite()
                && r.size.width.is_finite()
                && r.size.height.is_finite()
                && r.size.width >= 0.0
                && r.size.height >= 0.0
        };
        let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut count = 0usize;
        for (_, r) in rects.iter().filter(|(_, r)| finite(r)) {
            min_x = min_x.min(r.origin.x);
            min_y = min_y.min(r.origin.y);
            max_x = max_x.max(r.origin.x + r.size.width);
            max_y = max_y.max(r.origin.y + r.size.height);
            count += 1;
        }
        let (width, height) = (max_x - min_x, max_y - min_y);
        if count == 0 || !(width.is_finite() && height.is_finite()) || width <= 0.0 || height <= 0.0
        {
            return None;
        }
        // About one cell per entry, in cells of the bounds' own shape.
        let cols = (((count as f32) * width / height).sqrt().ceil() as usize).clamp(1, GRID_MAX_SIDE);
        let rows = count.div_ceil(cols).clamp(1, GRID_MAX_SIDE);
        let (cells_per_x, cells_per_y) = (cols as f32 / width, rows as f32 / height);
        // (entry, first col, last col, first row, last row) of what the grid takes.
        let mut placed: Vec<(u32, usize, usize, usize, usize)> = Vec::with_capacity(count);
        let mut per_cell: Vec<u32> = vec![0; cols * rows + 1];
        for (entry, r) in rects {
            if !finite(r) {
                always.push(*entry);
                continue;
            }
            let c0 = Self::cell_along(r.origin.x, min_x, cells_per_x, cols);
            let c1 = Self::cell_along(r.origin.x + r.size.width, min_x, cells_per_x, cols);
            let r0 = Self::cell_along(r.origin.y, min_y, cells_per_y, rows);
            let r1 = Self::cell_along(r.origin.y + r.size.height, min_y, cells_per_y, rows);
            if (c1 - c0 + 1) * (r1 - r0 + 1) > GRID_MAX_CELLS_PER_ENTRY {
                always.push(*entry);
                continue;
            }
            for row in r0..=r1 {
                for col in c0..=c1 {
                    per_cell[row * cols + col + 1] += 1;
                }
            }
            placed.push((*entry, c0, c1, r0, r1));
        }
        // Counts to starts, then fill: `rects` is in paint order, and so is
        // every cell's list.
        for cell in 0..cols * rows {
            per_cell[cell + 1] += per_cell[cell];
        }
        let starts = per_cell;
        let mut next: Vec<u32> = starts[..cols * rows].to_vec();
        let mut items: Vec<u32> = vec![0; starts[cols * rows] as usize];
        for (entry, c0, c1, r0, r1) in placed {
            for row in r0..=r1 {
                for col in c0..=c1 {
                    let slot = &mut next[row * cols + col];
                    items[*slot as usize] = entry;
                    *slot += 1;
                }
            }
        }
        Some(Self {
            min: LogicalPosition::new(min_x, min_y),
            max: LogicalPosition::new(max_x, max_y),
            cols,
            rows,
            cells_per_x,
            cells_per_y,
            starts,
            items,
        })
    }

    /// The entries whose rect can hold `p`: those of its cell. None for a
    /// point outside the grid's bounds (every rect in it lies inside them).
    fn at(&self, p: LogicalPosition) -> &[u32] {
        let inside =
            p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y;
        if !inside {
            return &[];
        }
        let col = Self::cell_along(p.x, self.min.x, self.cells_per_x, self.cols);
        let row = Self::cell_along(p.y, self.min.y, self.cells_per_y, self.rows);
        let cell = row * self.cols + col;
        match (self.starts.get(cell), self.starts.get(cell + 1)) {
            (Some(&from), Some(&to)) => self.items.get(from as usize..to as usize).unwrap_or(&[]),
            _ => &[],
        }
    }
}

impl DomIndex {
    /// `entries` (one dom's, in paint order) by chain, with the dom's
    /// viewports.
    fn build(entries: &[HitTestEntry], dom_clips: Vec<(LogicalRect, u32)>) -> Self {
        let mut by_chain: BTreeMap<u32, Vec<(u32, LogicalRect)>> = BTreeMap::new();
        for (i, entry) in entries.iter().enumerate() {
            // An index past `u32` cannot be listed; no dom has that many boxes.
            let Ok(i) = u32::try_from(i) else {
                break;
            };
            by_chain.entry(entry.chain).or_default().push((i, entry.rect));
        }
        let groups = by_chain
            .into_iter()
            .map(|(chain, rects)| {
                let mut always = Vec::new();
                let grid = if rects.len() >= GRID_MIN_ENTRIES {
                    EntryGrid::build(&rects, &mut always)
                } else {
                    None
                };
                if grid.is_none() {
                    // No grid (a small group, or rects with no extent):
                    // every entry is tested, in paint order.
                    always = rects.iter().map(|(i, _)| *i).collect();
                }
                let mut bounds = Some([
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ]);
                for (_, r) in &rects {
                    let (x1, y1) = (r.origin.x + r.size.width, r.origin.y + r.size.height);
                    let finite = r.origin.x.is_finite()
                        && r.origin.y.is_finite()
                        && x1.is_finite()
                        && y1.is_finite();
                    bounds = match bounds {
                        Some([x, y, far_x, far_y]) if finite => Some([
                            x.min(r.origin.x),
                            y.min(r.origin.y),
                            far_x.max(x1),
                            far_y.max(y1),
                        ]),
                        _ => None,
                    };
                }
                ChainGroup {
                    chain,
                    always,
                    grid,
                    bounds,
                }
            })
            .collect();
        // The topmost box of every node (the entries are in paint order: a
        // later one overwrites). Scrollbars are no boxes.
        let boxes = || entries.iter().enumerate().filter(|(_, e)| e.scrollbar.is_none());
        let mut top_entry_of_node: Vec<u32> = Vec::new();
        let max_node = boxes().map(|(_, e)| e.node_id.index()).max();
        if let Some(max_node) = max_node {
            if max_node < entries.len().saturating_mul(4).saturating_add(1024) {
                top_entry_of_node = vec![u32::MAX; max_node + 1];
                for (i, entry) in boxes() {
                    if let Ok(i) = u32::try_from(i) {
                        top_entry_of_node[entry.node_id.index()] = i;
                    }
                }
            }
        }
        Self {
            dom_clips,
            groups,
            top_entry_of_node,
            plain: Vec::new(),
            framed: Vec::new(),
            frame_bounds: None,
            seen_stamps: [0; 6],
            seen_times: 0,
        }
    }

    /// Sorts the groups into `framed` (the innermost link of the group's
    /// chain is a reference frame of a node of `dom_id`, its rects are
    /// finite, and the chain around the frame holds
    /// [`FRAME_BOUNDS_MIN_GROUPS`] such groups) and `plain` (the rest).
    /// `chains` is the tester's chain table.
    fn split_framed(&mut self, dom_id: DomId, chains: &[ChainNode]) {
        self.plain.clear();
        self.framed.clear();
        self.frame_bounds = None;
        self.seen_times = 0;
        // A group index past `u32` cannot be listed: no bounds are kept then
        // (`framed` stays empty), and a query walks `groups` as before.
        let Ok(count) = u32::try_from(self.groups.len()) else {
            return;
        };
        let mut by_parent: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for g in 0..count {
            let group = &self.groups[g as usize];
            let framed_under = chains.get(group.chain as usize).and_then(|node| {
                let own_frame = matches!(
                    node.link,
                    Some(HitChainLink::Transform(d, _, _)) if d == dom_id
                );
                // A chain's parent is interned before it (`ChainPoints`
                // walks the table by the same rule).
                (own_frame && group.bounds.is_some() && node.parent < group.chain)
                    .then_some(node.parent)
            });
            match framed_under {
                Some(parent) => by_parent.entry(parent).or_default().push(g),
                None => self.plain.push(g),
            }
        }
        for (parent, groups) in by_parent {
            if groups.len() >= FRAME_BOUNDS_MIN_GROUPS {
                self.framed.push(FramedGroups { parent, groups });
            } else {
                self.plain.extend(groups);
            }
        }
    }
}

/// What the rounding of a point to the `f32` of the boxes can move it by,
/// for a coordinate of size `reach`: the painted bounds of a framed group
/// are widened by it on both sides of the frame's map, so a point the exact
/// test accepts is never outside them.
fn bounds_slack(reach: f64) -> f64 {
    1.0e-3 + reach.abs() * 1.0e-6
}

/// Where the box `bounds` (`min x, min y, max x, max y` in the space inside
/// `link`) is painted in the space around the link: the box around its four
/// painted corners, widened by [`bounds_slack`]. `None` when a corner is not
/// painted (behind the eye of a perspective) or the result is not finite:
/// then nothing is known about where the box's content can be hit.
///
/// A point `q` of the outer space whose inverse through the link lies in the
/// box is inside the answer: a frame's plane map is a projective map with
/// `w > 0` at the four corners, so over the whole box, and such a map takes
/// the box into the hull of its corners' images.
fn painted_bounds(
    link: &HitChainLink,
    bounds: [f32; 4],
    live: &LiveValues<'_>,
) -> Option<LogicalRect> {
    let [x0, y0, x1, y1] = bounds.map(f64::from);
    let inner = bounds_slack(x0.abs().max(y0.abs()).max(x1.abs()).max(y1.abs()));
    let (x0, y0, x1, y1) = (x0 - inner, y0 - inner, x1 + inner, y1 + inner);
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for corner in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
        let (x, y) = link_to_screen(link, corner, live)?;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let outer = bounds_slack(
        min_x.abs().max(min_y.abs()).max(max_x.abs()).max(max_y.abs()),
    );
    #[allow(clippy::cast_possible_truncation)] // back in the f32 of the boxes, widened first
    let rect = LogicalRect {
        origin: LogicalPosition {
            x: (min_x - outer) as f32,
            y: (min_y - outer) as f32,
        },
        size: LogicalSize {
            width: (max_x - min_x + 2.0 * outer) as f32,
            height: (max_y - min_y + 2.0 * outer) as f32,
        },
    };
    let finite = rect.origin.x.is_finite()
        && rect.origin.y.is_finite()
        && rect.size.width.is_finite()
        && rect.size.height.is_finite();
    finite.then_some(rect)
}

/// No scroll frame has an offset: for a map through reference frames only.
const fn no_scroll(_: DomId, _: NodeId) -> Option<LogicalPosition> {
    None
}

impl FrameBounds {
    /// The painted bounds of the `framed` groups for the matrices
    /// `resolve_transform` answers now, which `stamps` stand for.
    fn build(
        groups: &[ChainGroup],
        framed: &[FramedGroups],
        chains: &[ChainNode],
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
        stamps: TransformStamps,
    ) -> Self {
        let live = LiveValues {
            scroll: &no_scroll,
            transform: resolve_transform,
            view_shift: &no_view_shift,
        };
        let parents = framed
            .iter()
            .map(|under| {
                let mut always = Vec::new();
                let mut rects: Vec<(u32, LogicalRect)> = Vec::with_capacity(under.groups.len());
                for &g in &under.groups {
                    let painted = groups.get(g as usize).and_then(|group| {
                        let link = chains.get(group.chain as usize)?.link?;
                        painted_bounds(&link, group.bounds?, &live)
                    });
                    match painted {
                        Some(rect) => rects.push((g, rect)),
                        None => always.push(g),
                    }
                }
                let grid = EntryGrid::build(&rects, &mut always);
                if grid.is_none() {
                    // No grid (bounds with no extent to divide): every
                    // group is looked at.
                    always.extend(rects.iter().map(|(g, _)| *g));
                }
                ParentBounds {
                    parent: under.parent,
                    always,
                    grid,
                }
            })
            .collect();
        Self { stamps, parents }
    }
}

/// A node's clip geometry together with the `viewBox`
/// (`min_x`, `min_y`, `width`, `height`) it was authored in, if any.
///
/// Deliberately NOT called `ClipPath`: `azul_css` already exports a
/// `ClipPath` (the CSS property), and the C-API safety scanner rejects two
/// types of the same name in different files as a collision.
type HitClipGeometry = (
    azul_core::svg::SvgMultiPolygon,
    Option<(f32, f32, f32, f32)>,
);

/// A single entry in the CPU hit test acceleration structure.
#[derive(Debug, Clone)]
struct HitTestEntry {
    /// The DOM node that this entry corresponds to.
    node_id: NodeId,
    /// Static (unscrolled) position and size of this node in logical pixels,
    /// window space (`VirtualView` placement already applied).
    rect: LogicalRect,
    /// Ancestor scroll frames whose offsets shift this node on screen
    /// (index into [`CpuHitTester::chains`]).
    chain: u32,
    /// Clip boxes from `overflow`-clipping ancestors and the `VirtualView`
    /// composite bounds. Each clip carries the chain of ITS owner's strict
    /// scroll ancestors — a clip box inside a scrolled frame moves with that
    /// frame, while the clipping container's own scroll does not move its
    /// viewport. Axis-only clips (`overflow-x`/`overflow-y` independent) are
    /// stored with the unclipped axis widened to [`CLIP_UNBOUNDED`].
    clips: Vec<(LogicalRect, u32)>,
    /// Whether this node is pointer-events: none
    pointer_events_none: bool,
    /// This node's CLIP PATH - its own SVG geometry, plus the `viewBox` of the
    /// `<svg>` that geometry is drawn in.
    ///
    /// A clip path is not decoration: what it clips AWAY is not part of the
    /// element any more, and that has to include its pointer target. Without
    /// this, the transparent corners of a circular button stayed clickable and
    /// - worse - kept SHADOWING whatever was behind them, which is precisely
    /// the thing a clip-path is asked for.
    clip_path: Option<HitClipGeometry>,
    /// Its paint rank in its dom's display list (`paint_ranks`; `usize::MAX`
    /// for a node that paints nothing): where a child dom's `VirtualView`
    /// paints between this dom's boxes.
    rank: usize,
    /// Whether the box paints an item of its OWN (`rank` is then that
    /// item's index); `false` for a box placed in paint order only by its
    /// relatives (`paint_ranks`). Only a box that paints after a scrollbar
    /// covers it ([`CpuHitTester::scrollbar_at`]).
    paints_own: bool,
    /// `Some` for a SCROLLBAR entry: the bar `node_id` (a scroll container)
    /// paints on this axis, at its own item (`rank`), inside its ancestors'
    /// clips and scroll frames. Only [`CpuHitTester::scrollbar_at`] reads
    /// them; every node query skips them.
    scrollbar: Option<ScrollbarOrientation>,
}

/// One entry a point lands on, in front-to-back order
/// ([`CpuHitTester::entries_at`]).
#[derive(Debug, Clone, Copy)]
struct EntryHit {
    dom_id: DomId,
    node_id: NodeId,
    /// The point in the entry's static layout space.
    local: LogicalPosition,
    /// The entry's chain (index into [`CpuHitTester::chains`]).
    chain: u32,
    paints_own: bool,
    scrollbar: Option<ScrollbarOrientation>,
}

/// A scroll container (`PushScrollFrame` owner) for wheel-target resolution.
#[derive(Debug, Clone)]
struct ScrollContainerEntry {
    dom_id: DomId,
    node_id: NodeId,
    /// Index of this node in its DOM's layout tree (for content-size lookup).
    layout_idx: LayoutNodeId,
    scroll_id: u64,
    /// Static viewport box, window space (placement-translated).
    rect: LogicalRect,
    /// Chain of STRICT scroll ancestors (index into [`CpuHitTester::chains`]).
    chain: u32,
}

/// One link in a node's ancestor chain: something between the node and the
/// window root that moves the node's on-screen position at runtime.
///
/// A chain lists its links OUTERMOST FIRST, in the order the display list
/// nests the frames, and each link is one map from the space inside it to
/// the space around it ([`link_to_screen`]; [`link_to_local`] is its
/// inverse). The links do not commute - a scroll frame around a turned box
/// is not a turned box around a scroll frame - so they are applied one by
/// one, never summed by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HitChainLink {
    /// An ancestor scroll frame — shifts content by the CURRENT scroll
    /// offset (`ScrollManager::get_current_offset`), painted as
    /// `pos - offset`.
    Scroll(DomId, NodeId),
    /// An ancestor wrapped in a `PushReferenceFrame` (CSS transform / drag /
    /// animation) — the CURRENT matrix lives in the GPU value cache
    /// (`GpuStateManager::painted_transform_of`: the CSS or the animation
    /// channel), the same source the CPU raster reads at paint time. The
    /// matrix acts ABOUT THE FRAME'S ORIGIN, the box's own top-left corner
    /// (`PushReferenceFrame::bounds.origin`): `transform-origin` is folded
    /// into the matrix relative to that corner, and both renderers paint
    /// `origin + M (pos - origin)`.
    Transform(DomId, NodeId, FrameOrigin),
    /// The step into a `VirtualView`'s child dom: the child's boxes were
    /// placed with the `content_offset` its view's item had when the tester
    /// was built, and the item is re-pointed in place when the view scrolls
    /// or pans (`LayoutWindow::patch_virtual_view_content_offset`, no layout,
    /// no rebuild of this tester). The link is how far the content has moved
    /// since ([`CpuHitTester::sync_view_placements`]).
    View(DomId),
}

/// The origin of a reference frame - the top-left corner of its box, in the
/// static space the boxes of its dom are kept in. Stored as bit patterns so
/// a [`HitChainLink`] stays `Eq + Hash` (chains are interned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameOrigin {
    x: u32,
    y: u32,
}

impl FrameOrigin {
    #[must_use]
    pub const fn new(origin: LogicalPosition) -> Self {
        Self {
            x: origin.x.to_bits(),
            y: origin.y.to_bits(),
        }
    }

    #[must_use]
    pub const fn get(self) -> LogicalPosition {
        LogicalPosition {
            x: f32::from_bits(self.x),
            y: f32::from_bits(self.y),
        }
    }
}

/// Minimal 2D affine mirroring `agg_rust::trans_affine::TransAffine`.
///
/// Row-vector convention: `x' = x·sx + y·shx + tx; y' = x·shy + y·sy + ty`.
/// Local copy because `agg-rust` is optional (svg/cpurender features) and
/// hit-testing must exist in every configuration. The multiply/invert bodies
/// are transcribed from agg so composition matches the raster EXACTLY.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenMapAffine {
    pub sx: f32,
    pub shy: f32,
    pub shx: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

impl ScreenMapAffine {
    pub const IDENTITY: Self = Self {
        sx: 1.0,
        shy: 0.0,
        shx: 0.0,
        sy: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// The 2D-affine slice of a `ComputedTransform3D`, exactly the elements
    /// the CPU raster feeds `TransAffine::new_custom`
    /// (`m[0][0], m[0][1], m[1][0], m[1][1], m[3][0], m[3][1]`).
    #[must_use]
    pub const fn from_transform_3d(t: &azul_core::transform::ComputedTransform3D) -> Self {
        Self {
            sx: t.m[0][0],
            shy: t.m[0][1],
            shx: t.m[1][0],
            sy: t.m[1][1],
            tx: t.m[3][0],
            ty: t.m[3][1],
        }
    }

    /// `self = self · m` (agg `multiply`).
    pub fn multiply(&mut self, m: &Self) {
        let t0 = self.sx.mul_add(m.sx, self.shy * m.shx);
        let t2 = self.shx.mul_add(m.sx, self.sy * m.shx);
        let t4 = self.tx.mul_add(m.sx, self.ty * m.shx) + m.tx;
        self.shy = self.sx.mul_add(m.shy, self.shy * m.sy);
        self.sy = self.shx.mul_add(m.shy, self.sy * m.sy);
        self.ty = self.tx.mul_add(m.shy, self.ty * m.sy) + m.ty;
        self.sx = t0;
        self.shx = t2;
        self.tx = t4;
    }

    /// `self = m · self` (agg `premultiply`) — the raster's per-push step is
    /// `composed = tf; composed.premultiply(&current)`.
    pub fn premultiply(&mut self, m: &Self) {
        let mut t = *m;
        t.multiply(self);
        *self = t;
    }

    /// In-place inverse (agg `invert`). Degenerate matrices (determinant 0)
    /// leave a non-finite result; callers treat non-finite mapped points as
    /// misses, which matches "a zero-scale transform is unclickable".
    pub fn invert(&mut self) {
        let d = 1.0 / self.sx.mul_add(self.sy, -(self.shy * self.shx));
        let t0 = self.sy * d;
        self.sy = self.sx * d;
        self.shy = -self.shy * d;
        self.shx = -self.shx * d;
        let t4 = (-self.tx).mul_add(t0, -(self.ty * self.shx));
        self.ty = (-self.tx).mul_add(self.shy, -(self.ty * self.sy));
        self.sx = t0;
        self.tx = t4;
    }

    #[must_use]
    pub fn apply(&self, p: LogicalPosition) -> LogicalPosition {
        LogicalPosition {
            x: p.x.mul_add(self.sx, p.y * self.shx) + self.tx,
            y: p.x.mul_add(self.shy, p.y * self.sy) + self.ty,
        }
    }

    /// The axis-aligned bounds of `r` mapped by this transform (its four
    /// corners' AABB): exact for a translation and a scale, the enclosing
    /// box for a rotation or a skew.
    #[must_use]
    pub fn map_rect(&self, r: LogicalRect) -> LogicalRect {
        let corners = [
            r.origin,
            LogicalPosition::new(r.origin.x + r.size.width, r.origin.y),
            LogicalPosition::new(r.origin.x, r.origin.y + r.size.height),
            LogicalPosition::new(r.origin.x + r.size.width, r.origin.y + r.size.height),
        ]
        .map(|c| self.apply(c));
        let min_x = corners.iter().map(|c| c.x).fold(f32::INFINITY, f32::min);
        let min_y = corners.iter().map(|c| c.y).fold(f32::INFINITY, f32::min);
        let max_x = corners.iter().map(|c| c.x).fold(f32::NEG_INFINITY, f32::max);
        let max_y = corners.iter().map(|c| c.y).fold(f32::NEG_INFINITY, f32::max);
        LogicalRect::new(
            LogicalPosition::new(min_x, min_y),
            LogicalSize::new((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
        )
    }

    #[allow(clippy::float_cmp)]
    // exact equality is correct here: identity is a fast-path gate; a near-identity matrix must
    // still be applied
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.sx == 1.0
            && self.shy == 0.0
            && self.shx == 0.0
            && self.sy == 1.0
            && self.tx == 0.0
            && self.ty == 0.0
    }
}

/// The values the links of a chain have NOW: the scroll offsets
/// (`ScrollManager::get_current_offset`), the matrices the last frame was
/// drawn with (`GpuStateManager::painted_transform_of`: the GPU value, not
/// the one layout baked), and how far each `VirtualView`'s content has moved
/// since its boxes were placed.
#[derive(Clone, Copy)]
struct LiveValues<'a> {
    scroll: &'a dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
    transform: &'a dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
    view_shift: &'a dyn Fn(DomId) -> LogicalPosition,
}

/// A view's content where its boxes were placed (no move since).
const fn no_view_shift(_: DomId) -> LogicalPosition {
    LogicalPosition::zero()
}

/// A point in `f64`: the maps below divide and multiply by a matrix about
/// an origin that can be thousands of pixels away, and the result is
/// compared with box edges.
type PlanePoint = (f64, f64);

/// Below this determinant a frame's matrix is not inverted: the frame is
/// drawn as a line or a point, and nothing in it can be hit.
const MIN_FRAME_DETERMINANT: f64 = 1.0e-12;

/// A point whose homogeneous `w` under a frame's matrix is not above this is
/// behind the eye of a `perspective()`: it is not drawn, and not hit.
const MIN_FRAME_W: f64 = 1.0e-9;

/// The plane map of a reference frame's matrix: the row-vector 4x4 on the
/// `z = 0` plane, `[x', y', w] = [a b c; d e f; g h i] [x, y, 1]` - the nine
/// numbers the CPU raster builds its `layer_h` from and `WebRender` gets as
/// the frame's transform (translation in logical px).
#[allow(clippy::many_single_char_names)] // the nine entries of a 3x3 matrix
fn frame_plane(t: &azul_core::transform::ComputedTransform3D) -> [f64; 9] {
    let m = |row: usize, col: usize| f64::from(t.m[row][col]);
    [
        m(0, 0),
        m(1, 0),
        m(3, 0),
        m(0, 1),
        m(1, 1),
        m(3, 1),
        m(0, 3),
        m(1, 3),
        m(3, 3),
    ]
}

/// Is the plane map a translation (the way most frames move: a drag, a
/// slide, a sticky header)? Exact, not within a tolerance: it only picks
/// the path without a division, and a near-translation takes the general one.
#[allow(clippy::float_cmp)] // an exact gate, see above
fn plane_translation(h: &[f64; 9]) -> Option<PlanePoint> {
    (h[0] == 1.0
        && h[1] == 0.0
        && h[3] == 0.0
        && h[4] == 1.0
        && h[6] == 0.0
        && h[7] == 0.0
        && h[8] == 1.0)
        .then_some((h[2], h[5]))
}

/// Where a reference frame with matrix `t` about `origin` PAINTS the point
/// `p` of its content: `origin + M (p - origin)`, the rule of
/// `cpurender::raster::ReferenceFrameGroup::begin` and of the two frames
/// `compositor2` pushes for it. `None` where the point is not drawn (behind
/// the eye of a perspective, or not finite).
#[allow(clippy::many_single_char_names)] // the nine entries of a 3x3 matrix
#[allow(clippy::neg_cmp_op_on_partial_ord)] // a NaN must fail the test
fn frame_to_screen(
    t: &azul_core::transform::ComputedTransform3D,
    origin: LogicalPosition,
    p: PlanePoint,
) -> Option<PlanePoint> {
    let plane = frame_plane(t);
    if let Some((tx, ty)) = plane_translation(&plane) {
        return Some((p.0 + tx, p.1 + ty));
    }
    let [a, b, c, d, e, f, g, h, i] = plane;
    let (ox, oy) = (f64::from(origin.x), f64::from(origin.y));
    let (qx, qy) = (p.0 - ox, p.1 - oy);
    let w = g * qx + h * qy + i;
    // `!(w > ..)` so that a NaN is "not drawn" too.
    if !(w > MIN_FRAME_W) {
        return None;
    }
    let (x, y) = ((a * qx + b * qy + c) / w, (d * qx + e * qy + f) / w);
    (x.is_finite() && y.is_finite()).then_some((ox + x, oy + y))
}

/// The inverse of [`frame_to_screen`]: the point of the frame's content that
/// is painted at `p`, `origin + M^-1 (p - origin)`. `None` when the matrix
/// cannot be inverted (a zero scale: nothing of the frame can be hit) or the
/// point lies behind the eye of a perspective.
#[allow(clippy::many_single_char_names)] // the nine entries of a 3x3 matrix
#[allow(clippy::neg_cmp_op_on_partial_ord)] // a NaN must fail the test
fn frame_to_local(
    t: &azul_core::transform::ComputedTransform3D,
    origin: LogicalPosition,
    p: PlanePoint,
) -> Option<PlanePoint> {
    let plane = frame_plane(t);
    if let Some((tx, ty)) = plane_translation(&plane) {
        return Some((p.0 - tx, p.1 - ty));
    }
    let [a, b, c, d, e, f, g, h, i] = plane;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !det.is_finite() || det.abs() < MIN_FRAME_DETERMINANT {
        return None;
    }
    let (ox, oy) = (f64::from(origin.x), f64::from(origin.y));
    let (qx, qy) = (p.0 - ox, p.1 - oy);
    // The adjugate times (qx, qy, 1): `det` times the local point's
    // homogeneous coordinates, so the quotients need no division by `det`.
    let x = (e * i - f * h) * qx + (c * h - b * i) * qy + (b * f - c * e);
    let y = (f * g - d * i) * qx + (a * i - c * g) * qy + (c * d - a * f);
    let w = (d * h - e * g) * qx + (b * g - a * h) * qy + (a * e - b * d);
    let (lx, ly) = (x / w, y / w);
    if !(lx.is_finite() && ly.is_finite()) {
        return None;
    }
    // Drawn at all: in front of the eye where the frame paints it.
    if !(g * lx + h * ly + i > MIN_FRAME_W) {
        return None;
    }
    Some((ox + lx, oy + ly))
}

/// The matrix a reference frame is PAINTED with: its live value (`live`),
/// or - where its key has none - the matrix its list was built with
/// (`baked`, [`CpuHitTester::baked_frames`]). The rule of the rasteriser
/// (`render_state.transforms.get(key).map_or(&initial_transform.m, ..)`) and
/// of `WebRender`'s property binding (the value, else the binding's own).
/// With the live value alone such a frame was hit as the identity while it
/// was painted moved.
fn painted_or_baked<'a>(
    live: &'a dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
    baked: &'a std::collections::HashMap<(DomId, NodeId), azul_core::transform::ComputedTransform3D>,
) -> impl Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D> + 'a {
    move |dom_id, node_id| {
        live(dom_id, node_id).or_else(|| baked.get(&(dom_id, node_id)).copied())
    }
}

/// One link, from the space INSIDE it to the space around it - where the
/// renderers paint a point of the link's content.
fn link_to_screen(link: &HitChainLink, p: PlanePoint, live: &LiveValues<'_>) -> Option<PlanePoint> {
    match link {
        HitChainLink::Scroll(dom_id, node_id) => Some(match (live.scroll)(*dom_id, *node_id) {
            Some(offset) => (p.0 - f64::from(offset.x), p.1 - f64::from(offset.y)),
            None => p,
        }),
        HitChainLink::View(dom_id) => {
            let shift = (live.view_shift)(*dom_id);
            Some((p.0 + f64::from(shift.x), p.1 + f64::from(shift.y)))
        }
        HitChainLink::Transform(dom_id, node_id, origin) => {
            match (live.transform)(*dom_id, *node_id) {
                Some(t) => frame_to_screen(&t, origin.get(), p),
                None => Some(p),
            }
        }
    }
}

/// One link, from the space around it to the space INSIDE it: the inverse of
/// [`link_to_screen`]. `None` when nothing inside the link is at `p`.
fn link_to_local(link: &HitChainLink, p: PlanePoint, live: &LiveValues<'_>) -> Option<PlanePoint> {
    match link {
        HitChainLink::Scroll(dom_id, node_id) => Some(match (live.scroll)(*dom_id, *node_id) {
            Some(offset) => (p.0 + f64::from(offset.x), p.1 + f64::from(offset.y)),
            None => p,
        }),
        HitChainLink::View(dom_id) => {
            let shift = (live.view_shift)(*dom_id);
            Some((p.0 - f64::from(shift.x), p.1 - f64::from(shift.y)))
        }
        HitChainLink::Transform(dom_id, node_id, origin) => {
            match (live.transform)(*dom_id, *node_id) {
                Some(t) => frame_to_local(&t, origin.get(), p),
                None => Some(p),
            }
        }
    }
}

/// A point of the static space inside `chain` (outermost link first), where
/// it is painted: the innermost link first, then each one around it.
fn chain_to_screen(
    chain: &[HitChainLink],
    p: LogicalPosition,
    live: &LiveValues<'_>,
) -> Option<PlanePoint> {
    let mut point = (f64::from(p.x), f64::from(p.y));
    for link in chain.iter().rev() {
        point = link_to_screen(link, point, live)?;
    }
    Some(point)
}

/// The inverse of [`chain_to_screen`]: the outermost link first.
fn chain_to_local(
    chain: &[HitChainLink],
    p: LogicalPosition,
    live: &LiveValues<'_>,
) -> Option<PlanePoint> {
    let mut point = (f64::from(p.x), f64::from(p.y));
    for link in chain {
        point = link_to_local(link, point, live)?;
    }
    Some(point)
}

#[allow(clippy::cast_possible_truncation)] // a point back in the f32 the boxes are kept in
const fn plane_point(p: PlanePoint) -> LogicalPosition {
    LogicalPosition {
        x: p.0 as f32,
        y: p.1 as f32,
    }
}

/// The bounds of `rect` painted through `chain`: its four corners' bounding
/// box. `None` when a corner is not drawn.
fn chain_rect_to_screen(
    chain: &[HitChainLink],
    rect: LogicalRect,
    live: &LiveValues<'_>,
) -> Option<LogicalRect> {
    let corners = [
        rect.origin,
        LogicalPosition::new(rect.origin.x + rect.size.width, rect.origin.y),
        LogicalPosition::new(rect.origin.x, rect.origin.y + rect.size.height),
        LogicalPosition::new(
            rect.origin.x + rect.size.width,
            rect.origin.y + rect.size.height,
        ),
    ];
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for corner in corners {
        let (x, y) = chain_to_screen(chain, corner, live)?;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if !(min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite()) {
        return None;
    }
    let origin = plane_point((min_x, min_y));
    let far = plane_point((max_x, max_y));
    Some(LogicalRect::new(
        origin,
        LogicalSize::new((far.x - origin.x).max(0.0), (far.y - origin.y).max(0.0)),
    ))
}

/// The affine map `chain` paints its content with, when one of its reference
/// frames has a matrix now - a translation included (`None`: the chain has no
/// frame, it only scrolls its content). Taken from where the chain paints
/// three points, so it is the chain's own map whatever its links; a
/// perspective is flattened to its affine part at the origin.
#[allow(clippy::cast_possible_truncation)] // an affine for f32 consumers
fn chain_affine(chain: &[HitChainLink], live: &LiveValues<'_>) -> Option<ScreenMapAffine> {
    let has_matrix = chain.iter().any(|link| match link {
        HitChainLink::Transform(dom_id, node_id, _) => {
            (live.transform)(*dom_id, *node_id).is_some()
        }
        HitChainLink::Scroll(..) | HitChainLink::View(..) => false,
    });
    if !has_matrix {
        return None;
    }
    let at = |x: f32, y: f32| chain_to_screen(chain, LogicalPosition::new(x, y), live);
    let (o, ex, ey) = (at(0.0, 0.0)?, at(1.0, 0.0)?, at(0.0, 1.0)?);
    Some(ScreenMapAffine {
        sx: (ex.0 - o.0) as f32,
        shy: (ex.1 - o.1) as f32,
        shx: (ey.0 - o.0) as f32,
        sy: (ey.1 - o.1) as f32,
        tx: o.0 as f32,
        ty: o.1 as f32,
    })
}

/// Map a node's STATIC rect to its ON-SCREEN axis-aligned bounds.
///
/// Takes the four corners through the frames around the node, innermost
/// first, as the renderers nest them - a scroll frame moves what is inside it
/// by its offset, a reference frame by its matrix about its own box's corner
/// (`origin + M (corner - origin)`) - and answers their bounding box. `rect`
/// is in the layout space of the node's dom; so is the answer (a nested dom
/// is lifted to the window by [`nested_dom_window_origin`]).
///
/// This is THE shared answer to "where is this node on screen right now" —
/// menu positioning (`LayoutWindow::get_node_hit_test_bounds`) and the a11y
/// snapshot both go through it, so what a screen reader is told and where a
/// context menu opens can never disagree with painted pixels.
///
/// Transform membership is decided by `resolve_transform` returning `Some`
/// — pass the same GPU-cache lookup the raster paints from.
pub fn node_rect_to_screen(
    layout_result: &DomLayoutResult,
    dom_id: DomId,
    layout_idx: usize,
    rect: LogicalRect,
    resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> LogicalRect {
    rect_to_screen(
        layout_result,
        dom_id,
        layout_idx,
        rect,
        Inclusivity::AncestorsOnly,
        resolve_scroll,
        resolve_transform,
    )
}

/// [`node_rect_to_screen`] for a rect in the node's CONTENT - a caret, a
/// selection rect, a glyph run: the node's own scroll frame moves it, and
/// the node's own reference frame (its `transform`) wraps it, on top of
/// every ancestor's.
///
/// THE answer for the IME's caret rects (`LayoutWindow::
/// cursor_rect_viewport_for`, `TextTarget::rect_to_window`): they applied
/// the INVERSE of a per-ancestor map that held scrollbar thumb offsets, not
/// the forward transforms the raster paints with.
pub fn content_rect_to_screen(
    layout_result: &DomLayoutResult,
    dom_id: DomId,
    layout_idx: usize,
    rect: LogicalRect,
    resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> LogicalRect {
    rect_to_screen(
        layout_result,
        dom_id,
        layout_idx,
        rect,
        Inclusivity::SelfAndAncestors,
        resolve_scroll,
        resolve_transform,
    )
}

/// Where the renderers paint `rect` of `layout_idx`'s box
/// ([`Inclusivity::AncestorsOnly`]: a scroll container's own offset moves its
/// content, never its own border box) or of its content
/// ([`Inclusivity::SelfAndAncestors`]): through the reference frames - its
/// own included, in both cases - and the scroll frames around it, each
/// applied where it nests
/// ([`chain_rect_to_screen`]) - a frame's matrix about its box's corner, a
/// scroll frame's offset inside the frames around it - as the bounding box of
/// the four corners. `rect` itself when no frame is around it, or when a
/// corner is not drawn.
fn rect_to_screen(
    layout_result: &DomLayoutResult,
    dom_id: DomId,
    layout_idx: usize,
    rect: LogicalRect,
    inclusivity: Inclusivity,
    resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> LogicalRect {
    let tree = &layout_result.layout_tree;
    let nodes = &tree.nodes;

    // The boxes from the node up to the root, nearest first: a frame's place
    // on this path is how far out it is, which is what orders the scroll
    // frames among the reference frames.
    let path: Vec<usize> = tree
        .ancestor_chain(LayoutNodeId::new(layout_idx), Inclusivity::SelfAndAncestors)
        .map(LayoutNodeId::index)
        .collect();
    // (how far up the path, the frame of a box before its scroll frame, link)
    let mut links: Vec<(usize, u8, HitChainLink)> = Vec::new();
    // The transformed layout ancestors AND the node itself: the boxes the
    // display list opens a reference frame for. A node's own frame is
    // pushed before its own items (`generate_for_stacking_context`,
    // `paint_in_flow_child`), so it moves the node's BOX as well as its
    // content - unlike its own scroll frame (`inclusivity`, below), which
    // moves its content only. The hit tester's chains say the same
    // (`compute_node_chains`).
    for (up, idx) in path.iter().enumerate() {
        let Some(node) = nodes.get(*idx) else {
            break;
        };
        let Some(node_id) = node.dom_node_id else {
            continue;
        };
        if resolve_transform(dom_id, node_id).is_some() {
            // The frame's origin is its box's corner, where layout put it.
            let origin = layout_result
                .calculated_positions
                .get(*idx)
                .copied()
                .unwrap_or_default();
            links.push((
                up,
                0,
                HitChainLink::Transform(dom_id, node_id, FrameOrigin::new(origin)),
            ));
        }
    }
    // The scroll frames the node's box is painted in - the display list's
    // own answer (`solver3::scroll_chain`), not every ancestor with an id.
    for link in ScrollChain::of(
        tree,
        &layout_result.styled_dom,
        &layout_result.scroll_ids,
        LayoutNodeId::new(layout_idx),
        inclusivity,
    )
    .scrolling()
    {
        // A scroll frame's box is on the node's path (a containing block is
        // an ancestor); one that is not is taken as the outermost.
        let up = path
            .iter()
            .position(|idx| *idx == link.layout_index.index())
            .unwrap_or(usize::MAX);
        links.push((up, 1, HitChainLink::Scroll(dom_id, link.node)));
    }
    if links.is_empty() {
        return rect;
    }
    // Outermost first; of one box, its reference frame is around its scroll
    // frame (the frame wraps the box, the scroll frame its content).
    links.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let chain: Vec<HitChainLink> = links.into_iter().map(|(_, _, link)| link).collect();
    let live = LiveValues {
        scroll: resolve_scroll,
        transform: resolve_transform,
        view_shift: &no_view_shift,
    };
    chain_rect_to_screen(&chain, rect, &live).unwrap_or(rect)
}

/// One interned chain: the chain it extends, and one more link inside it.
/// A chain's parent is interned before it, so it has a smaller index: a walk
/// up the parents ends at entry 0, the empty chain.
#[derive(Debug, Clone, Copy)]
struct ChainNode {
    /// The chain this one extends by `link` (entry 0's is itself).
    parent: u32,
    /// The innermost link; `None` only for entry 0.
    link: Option<HitChainLink>,
}

/// `(parent chain, link)` to the chain that is the parent with the link
/// inside it.
type ChainLookup = std::collections::HashMap<(u32, HitChainLink), u32>;

/// The chain `parent` with `link` inside it, interned.
fn intern_link(
    chains: &mut Vec<ChainNode>,
    lookup: &mut ChainLookup,
    parent: u32,
    link: HitChainLink,
) -> u32 {
    if let Some(&i) = lookup.get(&(parent, link)) {
        return i;
    }
    // More chains than a `u32` counts cannot be built; a table that full
    // degrades to the parent's chain instead of indexing out of it.
    let Ok(i) = u32::try_from(chains.len()) else {
        return parent;
    };
    chains.push(ChainNode {
        parent,
        link: Some(link),
    });
    lookup.insert((parent, link), i);
    i
}

/// The chain `base` with `links` inside it (outermost first), interned.
fn intern_chain(
    chains: &mut Vec<ChainNode>,
    lookup: &mut ChainLookup,
    base: u32,
    links: impl IntoIterator<Item = HitChainLink>,
) -> u32 {
    links
        .into_iter()
        .fold(base, |chain, link| intern_link(chains, lookup, chain, link))
}

impl Default for CpuHitTester {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve each layout node's ancestor chain index into `chains`.
///
/// A node's chain is everything between it and the window that moves its
/// box: the reference frames of its transformed layout ancestors AND its
/// own (the exact set the display-list builder wrapped in
/// `PushReferenceFrame`, read off the GPU value cache's
/// `reference_frame_of` - CSS or animation channel - via `has_transform`),
/// and the scroll frames of its [`ScrollChain`] - the same frames the
/// builder opened around it (`scroll_chains`) - in the order they NEST:
/// outermost first, by how deep in the tree the box of each frame is, and
/// of one box its reference frame around its scroll frame (the frame wraps
/// the box, the scroll frame its content).
///
/// The two kinds differ in what they move. A scroll container shifts its
/// CONTENT, not itself, so a node's own scroll frame is not in its chain. A
/// transform moves the element's OWN box too: the builder pushes the
/// reference frame before the node's own items, so its background and its
/// hit-test area are painted transformed. Leaving the node's own transform
/// out kept a translated box hittable at its static place, where nothing of
/// it is painted, shadowing whatever is (e2e/bug-transform-offsets-hit-test).
///
/// The links are not sorted by kind: a scroll frame AROUND a turned or scaled
/// box moves it by the plain offset, one INSIDE it by the offset turned or
/// scaled, and the two orders used to be one (every transform, then every
/// scroll frame), which is right for translations only.
///
/// `frame_origin(layout index, node)` is the origin the node's reference
/// frame is painted about, in the static space of the dom's entries.
///
/// [`ScrollChain`]: crate::solver3::scroll_chain::ScrollChain
#[allow(clippy::too_many_arguments)] // one pass over one dom's tree, with its tables
fn compute_node_chains(
    layout_result: &DomLayoutResult,
    scroll_chains: &ScrollChains,
    dom_id: DomId,
    base_chain: u32,
    has_transform: &dyn Fn(NodeId) -> bool,
    frame_origin: &dyn Fn(usize, NodeId) -> LogicalPosition,
    chains: &mut Vec<ChainNode>,
    chain_lookup: &mut ChainLookup,
) -> Vec<u32> {
    const UNSET: u32 = u32::MAX;
    let nodes = &layout_result.layout_tree.nodes;
    let n = nodes.len();

    // The transformed boxes above a node, as a trie of this dom's own: entry
    // `k` is entry `.0` plus the box at layout index `.1`; entry 0 is "none".
    // `t(n) = t(parent) (+ parent)`, so boxes under the same frames share one.
    let mut frames: Vec<(u32, usize)> = vec![(0, usize::MAX)];
    // Per node: the frames of its strict ancestors, the same plus its own,
    // and how deep in the tree it is.
    let mut inherited: Vec<u32> = vec![UNSET; n];
    let mut with_own: Vec<u32> = vec![0; n];
    let mut depth: Vec<u32> = vec![0; n];
    let mut path: Vec<usize> = Vec::new();
    for start in 0..n {
        if inherited[start] != UNSET {
            continue;
        }
        path.clear();
        path.push(start);
        let mut cur = nodes[start].parent;
        while let Some(p) = cur {
            if p >= n || inherited[p] != UNSET || path.len() > n {
                break;
            }
            path.push(p);
            cur = nodes[p].parent;
        }
        for &idx in path.iter().rev() {
            // No parent: the root. A parent that is not resolved: the cycle
            // guard tripped; degrade to "no frame above".
            let (above, deep) = nodes[idx]
                .parent
                .filter(|p| *p < n && inherited[*p] != UNSET)
                .map_or((0, 0), |p| (with_own[p], depth[p].saturating_add(1)));
            inherited[idx] = above;
            depth[idx] = deep;
            let is_frame = nodes[idx].dom_node_id.is_some_and(has_transform);
            with_own[idx] = if is_frame {
                match u32::try_from(frames.len()) {
                    Ok(id) => {
                        frames.push((above, idx));
                        id
                    }
                    Err(_) => above,
                }
            } else {
                above
            };
        }
    }

    // The scroll frames, along the node's scroll chain, merged in. Nodes
    // painted in the same frames under the same transforms share one
    // interned chain.
    let mut combined: std::collections::HashMap<(u32, u32), u32> =
        std::collections::HashMap::new();
    let mut chain_of: Vec<u32> = Vec::with_capacity(n);
    // (how deep the frame's box is, reference frame before scroll frame, link)
    let mut links: Vec<(u32, u8, HitChainLink)> = Vec::new();
    for (idx, &t) in with_own.iter().enumerate() {
        let s = scroll_chains.box_chain_id(LayoutNodeId::new(idx));
        let c = if t == 0 && s == ScrollChains::EMPTY {
            base_chain
        } else if let Some(&c) = combined.get(&(t, s)) {
            c
        } else {
            links.clear();
            // The reference frames: the node's own (see the doc comment)
            // and its ancestors', walked up the trie.
            let mut cur = t;
            let mut budget = frames.len();
            while cur != 0 && budget > 0 {
                budget -= 1;
                let Some(&(above, at)) = frames.get(cur as usize) else {
                    break;
                };
                if let Some(nid) = nodes.get(at).and_then(|node| node.dom_node_id) {
                    links.push((
                        depth[at],
                        0,
                        HitChainLink::Transform(
                            dom_id,
                            nid,
                            FrameOrigin::new(frame_origin(at, nid)),
                        ),
                    ));
                }
                cur = above;
            }
            if s != ScrollChains::EMPTY {
                for link in scroll_chains.chain(s).scrolling() {
                    let deep = depth
                        .get(link.layout_index.index())
                        .copied()
                        .unwrap_or(0);
                    links.push((deep, 1, HitChainLink::Scroll(dom_id, link.node)));
                }
            }
            links.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            let c = intern_chain(
                chains,
                chain_lookup,
                base_chain,
                links.iter().map(|(_, _, link)| *link),
            );
            combined.insert((t, s), c);
            c
        };
        chain_of.push(c);
    }
    chain_of
}

/// A resolved `VirtualView` child-DOM placement.
///
/// TWO rectangles, and they are not the same one:
///
/// * `rect` — where the child dom's 0-relative boxes LAND, i.e. `bounds.origin + content_offset`.
///   Scrolling moves this.
/// * `clips` — the VIEWPORTS the child is visible through: every enclosing `VirtualView`'s own
///   `bounds`, each paired with the host-side chain active where that viewport lives. Scrolling
///   does NOT move these.
///
/// Plus the chain the child's CONTENT is painted in: the host-side chain
/// (scroll frames AND reference frames) active at the innermost
/// `VirtualView` item, and the step into the child itself
/// ([`HitChainLink::View`]) last. The viewports carry the host-side chain
/// without that step: a view's content moves through its box.
struct Placement {
    rect: LogicalRect,
    clips: Vec<(LogicalRect, Vec<HitChainLink>)>,
    chain: Vec<HitChainLink>,
    /// The `content_offset` of the view's item that `rect` was placed with.
    content_offset: LogicalPosition,
    /// The dom whose display list shows this child dom, and the index of
    /// the `VirtualView` item there: the child paints at that item.
    host: DomId,
    item: usize,
    /// The `VirtualView` node of `host` that item belongs to (the item's
    /// node attribution: the placeholder it replaced was pushed for it).
    host_node: Option<NodeId>,
}

/// Resolve where each `VirtualView` / iframe child DOM lives on screen.
///
/// Child DOMs lay out in CHILD-LOCAL coordinates (origin 0,0) but live on
/// screen at the host `VirtualView` item's bounds. Hit entries must be
/// TRANSLATED there and CLIPPED to the composite bounds — otherwise the
/// child's nodes claim pointer events across the whole window (live bug:
/// azul-maps' tile grid ate every click on the header toolbar, so the
/// buttons never fired; the same escape the renderer had before
/// `intersect_clips()`).
///
/// Placements resolve iteratively so nested `VirtualView`s accumulate their
/// host offsets (a child's own `VirtualView` item is in that child's local
/// space). They also carry the host-side chain active at the `VirtualView`
/// item: if the host scrolls or transforms, the child viewport (and all of
/// the child's content) moves on screen with it. The chain is read off the
/// host display list by tracking `PushScrollFrame`/`PopScrollFrame` and
/// `PushReferenceFrame`/`PopReferenceFrame` nesting around the
/// `VirtualView` item — the same nesting the renderer applies when it
/// composites the child. Reference-frame owners come from
/// `DisplayList::node_mapping` (item index → source node).
/// WINDOW-space origin of a nested (VirtualView-mounted) dom's 0-relative
/// coordinate space, by the raster's own composition rule: the dom's origin
/// through the host's frames at its view's item, innermost first, each
/// reference frame about its own box's corner. (One point: a host frame that
/// turns or scales also turns or scales the dom's content about this origin,
/// which a caller that adds this to a child rect leaves out.)
///
/// THE one cross-dom lift: it reads the host display lists' `VirtualView`
/// items via [`resolve_virtual_view_placements`] — the same records the CPU
/// raster composites from and the hit-tester translates by — so an IME caret
/// rect, a transient-window anchor and the painted pixels cannot disagree.
/// (`LayoutWindow::window_space_offset_of_dom` used to re-derive this from
/// `calculated_positions`, which knows nothing of ancestor scroll frames or
/// reference-frame transforms in the host.)
///
/// `None` for the root dom or a dom no display list currently mounts.
pub fn nested_dom_window_origin(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    dom_id: DomId,
    resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> Option<LogicalPosition> {
    let placements = resolve_virtual_view_placements(layout_results);
    let p = placements.get(&dom_id)?;
    // The placement is read off the items as they are now, so the view's
    // content is where `rect` says: no move since.
    let live = LiveValues {
        scroll: resolve_scroll,
        transform: resolve_transform,
        view_shift: &no_view_shift,
    };
    chain_to_screen(&p.chain, p.rect.origin, &live).map(plane_point)
}

/// `rect`, in `dom_id`'s own 0-relative painted space (what
/// [`node_rect_to_screen`] / [`content_rect_to_screen`] answer for a node of
/// a nested dom), where the WINDOW shows it: through the frames the dom's
/// `VirtualView` is painted in - the bounds of its four corners.
///
/// [`nested_dom_window_origin`] plus the rect is the same while those frames
/// only move the view; a host that scales or turns it (a view in a zoomed
/// panel) scales and turns the rect about the view's place, which a sum
/// leaves out: a caret 400 px into a view under `scale(2)` was reported 400
/// px from the view's corner where it is drawn 800 px from it.
/// `None` for the window's own dom, a dom no host mounts, or a corner that
/// is not drawn.
#[must_use]
pub fn nested_dom_rect_to_window(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    dom_id: DomId,
    rect: LogicalRect,
    resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> Option<LogicalRect> {
    let placements = resolve_virtual_view_placements(layout_results);
    let p = placements.get(&dom_id)?;
    let live = LiveValues {
        scroll: resolve_scroll,
        transform: resolve_transform,
        view_shift: &no_view_shift,
    };
    let placed = LogicalRect::new(
        LogicalPosition::new(rect.origin.x + p.rect.origin.x, rect.origin.y + p.rect.origin.y),
        rect.size,
    );
    chain_rect_to_screen(&p.chain, placed, &live)
}

/// The inverse of [`nested_dom_rect_to_window`] for a point: the window
/// point `point` in `dom_id`'s own 0-relative painted space. `None` for the
/// window's own dom, a dom no host mounts, or where nothing of the dom is
/// drawn at the point (a host frame that cannot be inverted).
#[must_use]
pub fn window_point_to_nested_dom(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    dom_id: DomId,
    point: LogicalPosition,
    resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> Option<LogicalPosition> {
    let placements = resolve_virtual_view_placements(layout_results);
    let p = placements.get(&dom_id)?;
    let live = LiveValues {
        scroll: resolve_scroll,
        transform: resolve_transform,
        view_shift: &no_view_shift,
    };
    let (x, y) = chain_to_local(&p.chain, point, &live)?;
    Some(plane_point((
        x - f64::from(p.rect.origin.x),
        y - f64::from(p.rect.origin.y),
    )))
}

/// One viewport a nested dom shows through ([`nested_dom_viewports`]): an
/// enclosing `VirtualView`'s box with its frames at rest, the host SCROLL
/// frames that move it (kept symbolic, for live offsets), and the forward
/// transform of the host REFERENCE frames around it, resolved now (`None`:
/// untransformed: each frame's matrix about its own box's corner, composed
/// in nesting order). On screen it is `transform(rect - scroll(frames))`,
/// which is where the raster paints it while the scroll frames lie inside
/// the frames that turn or scale (or the frames only translate).
pub type NestedDomViewport = (
    LogicalRect,
    Vec<(DomId, NodeId)>,
    Option<ScreenMapAffine>,
);

/// Every nested dom's viewports ([`NestedDomViewport`]), innermost last -
/// the last one is the dom's own `VirtualView`, and its frames and transform
/// are the ones the dom's content is composited under - from the same
/// records [`nested_dom_window_origin`] resolves. The scroll frames stay
/// symbolic for a consumer that resolves them against LIVE offsets
/// (`ScrollManager::set_nested_dom_placements`, via
/// `register_scroll_nodes`); the transforms are resolved with
/// `resolve_transform` when this is called, so a transform animating
/// between two registrations is followed at the next one.
///
/// Empty when no display list mounts a nested dom (the common window: one
/// dom, and no walk over its list at all).
#[must_use]
pub fn nested_dom_viewports(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> BTreeMap<DomId, Vec<NestedDomViewport>> {
    if layout_results.len() < 2 {
        return BTreeMap::new();
    }
    let scroll_frames = |chain: &[HitChainLink]| -> Vec<(DomId, NodeId)> {
        chain
            .iter()
            .filter_map(|link| match link {
                HitChainLink::Scroll(dom_id, node_id) => Some((*dom_id, *node_id)),
                HitChainLink::Transform(..) | HitChainLink::View(..) => None,
            })
            .collect()
    };
    let no_scroll = |_: DomId, _: NodeId| -> Option<LogicalPosition> { None };
    let at_rest = LiveValues {
        scroll: &no_scroll,
        transform: resolve_transform,
        view_shift: &no_view_shift,
    };
    // Each frame's matrix about its own box's corner, composed in nesting
    // order: the map of the reference frames alone, the scroll frames at
    // rest. The consumer applies it to `rect - scroll(frames)`, which is
    // exact when the scroll frames lie inside the turned or scaled frames
    // (or the frames only translate).
    let transform_of =
        |chain: &[HitChainLink]| -> Option<ScreenMapAffine> { chain_affine(chain, &at_rest) };
    resolve_virtual_view_placements(layout_results)
        .into_iter()
        .map(|(dom_id, placement)| {
            let viewports: Vec<NestedDomViewport> = placement
                .clips
                .iter()
                .map(|(viewport, chain)| {
                    (
                        *viewport,
                        scroll_frames(chain.as_slice()),
                        transform_of(chain.as_slice()),
                    )
                })
                .collect();
            (dom_id, viewports)
        })
        .collect()
}

fn resolve_virtual_view_placements(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
) -> BTreeMap<DomId, Placement> {
    let mut placements: BTreeMap<DomId, Placement> = BTreeMap::new();
    for _ in 0..4 {
        // bounded depth; each pass resolves one nesting level
        let mut changed = false;
        for (host_dom, lr) in layout_results {
            let (host_offset, host_clips, host_chain) = if host_dom.inner == 0 {
                (LogicalPosition::zero(), Vec::new(), Vec::new())
            } else if let Some(p) = placements.get(host_dom) {
                (p.rect.origin, p.clips.clone(), p.chain.clone())
            } else {
                continue;
            };
            let base_depth = host_chain.len();
            let mut stack = host_chain;
            for (item_idx, item) in lr.display_list.items.iter().enumerate() {
                use crate::solver3::display_list::DisplayListItem as I;
                match item {
                    I::PushScrollFrame { scroll_id, .. } => {
                        // `scroll_id` is the owning node's layout index by
                        // construction (`get_scroll_id`); the reverse map
                        // is authoritative, the index a safe fallback.
                        let nid = lr
                            .scroll_id_to_node_id
                            .get(scroll_id)
                            .copied()
                            .unwrap_or_else(|| {
                                NodeId::new(usize::try_from(*scroll_id).unwrap_or(usize::MAX))
                            });
                        stack.push(HitChainLink::Scroll(*host_dom, nid));
                    }
                    I::PopScrollFrame => {
                        // Never pop below the host's own chain.
                        if matches!(stack.last(), Some(HitChainLink::Scroll(..)))
                            && stack.len() > base_depth
                        {
                            stack.pop();
                        }
                    }
                    I::PushReferenceFrame { bounds, .. } => {
                        // The owner node comes from node_mapping; a frame
                        // with no source node (scrollbar thumbs) still
                        // needs a stack entry for pop symmetry — use a
                        // link that resolves to no transform.
                        let nid = lr
                            .display_list
                            .node_mapping
                            .get(item_idx)
                            .copied()
                            .flatten()
                            .unwrap_or(NodeId::ZERO);
                        // The frame's matrix is painted about its box's
                        // corner, in the space this dom's boxes are kept in.
                        let corner = bounds.inner().origin;
                        stack.push(HitChainLink::Transform(
                            *host_dom,
                            nid,
                            FrameOrigin::new(LogicalPosition::new(
                                corner.x + host_offset.x,
                                corner.y + host_offset.y,
                            )),
                        ));
                    }
                    I::PopReferenceFrame => {
                        if matches!(stack.last(), Some(HitChainLink::Transform(..)))
                            && stack.len() > base_depth
                        {
                            stack.pop();
                        }
                    }
                    I::VirtualView {
                        child_dom_id,
                        bounds,
                        content_offset,
                        ..
                    } => {
                        let b = *bounds.inner();
                        // THREE rects decide where a VirtualView's child lives:
                        // the outer `bounds`, the MATERIALIZED window inside it,
                        // and the virtual document. The renderer places the
                        // child at `bounds.origin + content_offset`, where
                        // content_offset is `materialized_origin -
                        // scroll_offset` (raster.rs subtracts exactly those two
                        // terms). This hit-test placement used `bounds.origin`
                        // alone and dropped content_offset on the floor, so
                        // clicks were mapped into the child as if the
                        // materialized window began at row 0 and nothing had
                        // scrolled.
                        //
                        // The caret still landed — the right text node was hit —
                        // just at the wrong CHARACTER, off by exactly
                        // `materialized_origin - scroll_offset`. That is zero on
                        // the first screenful and grows as you scroll, which is
                        // why clicking looked "heavily broken" further down a
                        // document and why dragging selected the wrong range.
                        let absolute = LogicalRect {
                            origin: LogicalPosition {
                                x: b.origin.x + host_offset.x + content_offset.x,
                                y: b.origin.y + host_offset.y + content_offset.y,
                            },
                            size: b.size,
                        };
                        // …and the VIEWPORT it shows through, which is the
                        // host box itself. `content_offset` moves the CONTENT
                        // through the viewport; it does not move the viewport.
                        // Clipping to `absolute` instead made the visible
                        // window travel with the scroll: on a document scrolled
                        // one page down, `content_offset.y` is a whole page
                        // NEGATIVE, so the clip sat a page ABOVE the canvas and
                        // rejected every node of the child dom. Clicks in the
                        // page then hit nothing at all — no caret, no
                        // selection, no typing — while the renderer (which
                        // clips to `bounds`, see cpurender/raster.rs) went on
                        // drawing the text the user was aiming at.
                        let viewport = LogicalRect {
                            origin: LogicalPosition {
                                x: b.origin.x + host_offset.x,
                                y: b.origin.y + host_offset.y,
                            },
                            size: b.size,
                        };
                        let mut clips = host_clips.clone();
                        clips.push((viewport, stack.clone()));
                        // The child's content is inside everything open at
                        // the item, and one step further in: the view.
                        let mut content_chain = stack.clone();
                        content_chain.push(HitChainLink::View(*child_dom_id));
                        let differs = placements.get(child_dom_id).is_none_or(|p| {
                            p.rect != absolute
                                || p.clips != clips
                                || p.chain != content_chain
                                || p.host != *host_dom
                                || p.item != item_idx
                        });
                        if differs {
                            placements.insert(
                                *child_dom_id,
                                Placement {
                                    rect: absolute,
                                    clips,
                                    chain: content_chain,
                                    content_offset: *content_offset,
                                    host: *host_dom,
                                    item: item_idx,
                                    host_node: lr
                                        .display_list
                                        .node_mapping
                                        .get(item_idx)
                                        .copied()
                                        .flatten(),
                                },
                            );
                            changed = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        if !changed {
            break;
        }
    }
    placements
}

/// One `<webview>` where the display lists put it right now, in window
/// logical coordinates ([`painted_webviews`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaintedWebView {
    /// The dom the web view is in.
    pub dom_id: DomId,
    /// The `<webview>` node.
    pub node_id: NodeId,
    /// Its content box on screen - where its native view goes, past the
    /// window's edges if it reaches past them.
    pub rect: LogicalRect,
    /// The part of `rect` the window shows, after every enclosing clip and
    /// scroll frame and the window's own edges; `None` when nothing of it
    /// shows (scrolled out, clipped away).
    pub clip: Option<LogicalRect>,
    /// How the page maps into `rect`: its own size and the linear part of
    /// the transforms above it (untransformed without any).
    pub transform: crate::managers::webview::WebViewTransform,
}

/// Every `<webview>` the display lists reserve a rect for
/// (`DisplayListItem::WebView`), where it is on screen NOW.
///
/// By the raster's own rule (every frame applied where it nests, a
/// reference frame about its box's corner), read off
/// the same records it paints from: each list is walked with the scroll and
/// reference frames open at each item - so the live scroll offsets
/// (`resolve_scroll`) and the transforms (`resolve_transform`) move the view
/// exactly as they move the pixels around it - and with the clips open at
/// it: every `PushClip` and every scroll frame's viewport, each mapped by
/// the frames it was pushed under (a scroll frame's viewport does not move
/// with its own scroll), intersected with the window. A web view in a
/// nested dom paints at its host's `VirtualView` item
/// ([`resolve_virtual_view_placements`]) and is seen through that view's
/// viewport too. A transformed view is placed at its transformed bounding
/// box, and the linear part of its map comes with it (`transform`: a native
/// view zooms by it, or turns where it can).
///
/// Empty - and no list walked - when no display list holds a web view.
#[must_use]
pub fn painted_webviews(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    viewport: LogicalSize,
    resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> Vec<PaintedWebView> {
    use crate::solver3::display_list::DisplayListItem as I;

    let has_webview = |lr: &DomLayoutResult| {
        lr.display_list
            .items
            .iter()
            .any(|item| matches!(item, I::WebView { .. }))
    };
    if !layout_results.values().any(has_webview) {
        return Vec::new();
    }
    let placements = if layout_results.len() > 1 {
        resolve_virtual_view_placements(layout_results)
    } else {
        BTreeMap::new()
    };
    // The placements are read off the items as they are now: no view has
    // moved its content since.
    let live = LiveValues {
        scroll: resolve_scroll,
        transform: resolve_transform,
        view_shift: &no_view_shift,
    };
    let to_screen = |r: LogicalRect, chain: &[HitChainLink]| -> LogicalRect {
        if chain.is_empty() {
            return r;
        }
        chain_rect_to_screen(chain, r, &live).unwrap_or(r)
    };
    let window = LogicalRect::new(LogicalPosition::zero(), viewport);
    let mut out = Vec::new();
    for (dom_id, lr) in layout_results {
        if !has_webview(lr) {
            continue;
        }
        // A nested dom is shifted by its placement, seen through every
        // enclosing view's viewport and moved by the host's frames; one no
        // display list mounts is nowhere.
        let (offset, host_clips, host_chain) = if *dom_id == DomId::ROOT_ID {
            (LogicalPosition::zero(), Vec::new(), Vec::new())
        } else if let Some(p) = placements.get(dom_id) {
            (p.rect.origin, p.clips.clone(), p.chain.clone())
        } else {
            continue;
        };
        let shift = |r: LogicalRect| {
            LogicalRect::new(
                LogicalPosition::new(r.origin.x + offset.x, r.origin.y + offset.y),
                r.size,
            )
        };
        let base_depth = host_chain.len();
        let mut chain = host_chain;
        // The open clips and scroll-frame viewports, each with the chain it
        // was pushed under.
        let mut clips: Vec<(LogicalRect, Vec<HitChainLink>)> = Vec::new();
        let mut frames: Vec<(LogicalRect, Vec<HitChainLink>)> = Vec::new();
        for (item_idx, item) in lr.display_list.items.iter().enumerate() {
            match item {
                I::PushClip { bounds, .. } => clips.push((shift(*bounds.inner()), chain.clone())),
                I::PopClip => {
                    clips.pop();
                }
                I::PushScrollFrame {
                    clip_bounds,
                    scroll_id,
                    ..
                } => {
                    frames.push((shift(*clip_bounds.inner()), chain.clone()));
                    // `scroll_id` names its node (see `resolve_virtual_view_placements`).
                    let nid = lr
                        .scroll_id_to_node_id
                        .get(scroll_id)
                        .copied()
                        .unwrap_or_else(|| {
                            NodeId::new(usize::try_from(*scroll_id).unwrap_or(usize::MAX))
                        });
                    chain.push(HitChainLink::Scroll(*dom_id, nid));
                }
                I::PopScrollFrame => {
                    frames.pop();
                    if matches!(chain.last(), Some(HitChainLink::Scroll(..)))
                        && chain.len() > base_depth
                    {
                        chain.pop();
                    }
                }
                I::PushReferenceFrame {
                    bounds: frame_box, ..
                } => {
                    let nid = lr
                        .display_list
                        .node_mapping
                        .get(item_idx)
                        .copied()
                        .flatten()
                        .unwrap_or(NodeId::ZERO);
                    // Painted about its box's corner (shifted like every
                    // rect of this dom).
                    chain.push(HitChainLink::Transform(
                        *dom_id,
                        nid,
                        FrameOrigin::new(shift(*frame_box.inner()).origin),
                    ));
                }
                I::PopReferenceFrame => {
                    if matches!(chain.last(), Some(HitChainLink::Transform(..)))
                        && chain.len() > base_depth
                    {
                        chain.pop();
                    }
                }
                I::WebView { node_id, bounds } => {
                    let local = shift(*bounds.inner());
                    let rect = to_screen(local, &chain);
                    let transform = match chain_affine(&chain, &live) {
                        Some(forward) => crate::managers::webview::WebViewTransform {
                            size: local.size,
                            sx: forward.sx,
                            shy: forward.shy,
                            shx: forward.shx,
                            sy: forward.sy,
                        },
                        None => crate::managers::webview::WebViewTransform::untransformed(local.size),
                    };
                    let clip = host_clips
                        .iter()
                        .chain(&clips)
                        .chain(&frames)
                        .try_fold(window, |visible, (c, c_chain)| {
                            intersect_rects(visible, to_screen(*c, c_chain))
                        })
                        .and_then(|visible| intersect_rects(visible, rect));
                    out.push(PaintedWebView {
                        dom_id: *dom_id,
                        node_id: *node_id,
                        rect,
                        clip,
                        transform,
                    });
                }
                _ => {}
            }
        }
    }
    out
}

/// The overlap of two rects; `None` when they do not overlap.
fn intersect_rects(a: LogicalRect, b: LogicalRect) -> Option<LogicalRect> {
    let (x0, y0) = (a.origin.x.max(b.origin.x), a.origin.y.max(b.origin.y));
    let (x1, y1) = (a.max_x().min(b.max_x()), a.max_y().min(b.max_y()));
    if x1 > x0 && y1 > y0 {
        Some(LogicalRect::new(
            LogicalPosition::new(x0, y0),
            LogicalSize::new(x1 - x0, y1 - y0),
        ))
    } else {
        None
    }
}

impl CpuHitTester {
    /// Create a new empty hit tester.
    #[must_use]
    pub fn new() -> Self {
        Self {
            node_rects: BTreeMap::new(),
            chains: vec![ChainNode {
                parent: 0,
                link: None,
            }],
            scroll_containers: Vec::new(),
            dom_placements: BTreeMap::new(),
            children_of: BTreeMap::new(),
            view_hosts: BTreeMap::new(),
            view_items: BTreeMap::new(),
            index: BTreeMap::new(),
            baked_frames: std::collections::HashMap::new(),
        }
    }

    /// The links of an interned chain, outermost first.
    fn chain_links(&self, chain: u32) -> Vec<HitChainLink> {
        let mut links = Vec::new();
        let mut cur = chain;
        // A chain's parent has a smaller index; the budget only guards a
        // corrupt table.
        let mut budget = self.chains.len();
        while let Some(node) = self.chains.get(cur as usize) {
            let Some(link) = node.link else {
                break; // entry 0, the empty chain
            };
            if budget == 0 {
                break;
            }
            budget -= 1;
            links.push(link);
            cur = node.parent;
        }
        links.reverse();
        links
    }

    /// How far the `VirtualView` of child dom `dom_id` has moved its content
    /// since the tester was built ([`Self::sync_view_placements`]).
    fn view_shift_of(&self, dom_id: DomId) -> LogicalPosition {
        self.view_items
            .get(&dom_id)
            .map_or_else(LogicalPosition::zero, |view| view.shift)
    }

    /// Map a window point into the static space of an interned chain, with
    /// the values its links have now. Not finite when nothing inside the
    /// chain is at the point (a frame that cannot be inverted): such a point
    /// is in no box.
    fn map_point_through_chain(
        &self,
        chain: u32,
        p: LogicalPosition,
        resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
    ) -> LogicalPosition {
        if chain == 0 || chain as usize >= self.chains.len() {
            return p;
        }
        let view_shift = |dom_id: DomId| self.view_shift_of(dom_id);
        let painted = painted_or_baked(resolve_transform, &self.baked_frames);
        let live = LiveValues {
            scroll: resolve_scroll,
            transform: &painted,
            view_shift: &view_shift,
        };
        chain_to_local(&self.chain_links(chain), p, &live)
            .map_or(LogicalPosition::new(f32::NAN, f32::NAN), plane_point)
    }

    /// Follow the `VirtualView` items: a view that scrolled or panned since
    /// the tester was built has had its item's `content_offset` re-pointed
    /// in place (`LayoutWindow::patch_virtual_view_content_offset`: no
    /// layout, so no rebuild of this tester), and its child's boxes are
    /// painted that much away from where they were placed. Reads each
    /// view's item - one per child dom, found by the index it had (the list
    /// is searched only when the item is no longer there) - and keeps the
    /// difference, which every query applies at the step into the child dom
    /// ([`HitChainLink::View`]).
    ///
    /// Call it before a query whenever the lists may have been patched since
    /// the last one; `CommonWindowState::perform_hit_test` does. A tester that is
    /// rebuilt before every query needs no call: a rebuild places the boxes
    /// with the offsets of that moment.
    pub fn sync_view_placements(&mut self, layout_results: &BTreeMap<DomId, DomLayoutResult>) {
        use crate::solver3::display_list::DisplayListItem;

        for (child, view) in &mut self.view_items {
            let Some(host) = layout_results.get(&view.host) else {
                continue;
            };
            let items = &host.display_list.items;
            let offset_at = |i: usize| match items.get(i) {
                Some(DisplayListItem::VirtualView {
                    child_dom_id,
                    content_offset,
                    ..
                }) if child_dom_id == child => Some(*content_offset),
                _ => None,
            };
            let now = offset_at(view.item).or_else(|| (0..items.len()).find_map(offset_at));
            if let Some(now) = now {
                view.shift = LogicalPosition::new(
                    now.x - view.built_offset.x,
                    now.y - view.built_offset.y,
                );
            }
        }
    }

    /// Keeps, per dom with many framed groups (a map's turned labels: one
    /// reference frame, one chain, one group each), where each group is
    /// painted in the space of the chain around its frame - so that a query
    /// with the same stamps ([`Self::hit_test_scrolled_stamped`]) looks at
    /// the groups whose painted bounds hold the point instead of inverting
    /// every frame's matrix to find out.
    ///
    /// `stamps_of` answers the transform stamps of a dom
    /// (`GpuValueCache::transform_stamps`, `[0; 6]` for a dom without a
    /// cache) and `resolve_transform` the matrices those stamps stand for:
    /// both from the same `GpuStateManager`. Bounds made with other stamps
    /// are dropped here, and no query uses bounds whose stamps are not the
    /// ones it is handed, so a matrix that changed is never missed: at worst
    /// every frame is inverted, as without this.
    ///
    /// The bounds are worked out once the stamps have been the same for
    /// [`FRAME_BOUNDS_AFTER_SYNCS`] syncs in a row: one matrix look-up and
    /// four mapped corners per framed group.
    pub fn sync_frame_bounds(
        &mut self,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
        stamps_of: &dyn Fn(DomId) -> TransformStamps,
    ) {
        // The bounds are of where the frames are painted: with the list's own
        // matrix where a key has no live value (the stamps move when a key
        // goes, and the baked matrices change only with a rebuild, which
        // drops the bounds).
        let painted = painted_or_baked(resolve_transform, &self.baked_frames);
        let resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D> = &painted;
        for (dom_id, index) in &mut self.index {
            if index.framed.is_empty() {
                continue;
            }
            let stamps = stamps_of(*dom_id);
            if index
                .frame_bounds
                .as_ref()
                .is_some_and(|bounds| bounds.stamps == stamps)
            {
                continue;
            }
            index.frame_bounds = None;
            if index.seen_times > 0 && index.seen_stamps == stamps {
                index.seen_times = index.seen_times.saturating_add(1);
            } else {
                index.seen_stamps = stamps;
                index.seen_times = 1;
            }
            if index.seen_times >= FRAME_BOUNDS_AFTER_SYNCS {
                index.frame_bounds = Some(FrameBounds::build(
                    &index.groups,
                    &index.framed,
                    &self.chains,
                    resolve_transform,
                    stamps,
                ));
            }
        }
    }

    /// Sum of `HitTestEntry` counts across all `DomIds` (for leak probes).
    #[must_use]
    pub fn node_rects_total(&self) -> usize {
        self.node_rects
            .values()
            .map(|entries| entries.iter().filter(|e| e.scrollbar.is_none()).count())
            .sum()
    }

    /// Rebuild the hit test structure from layout results.
    ///
    /// Called after each layout pass. Extracts positioned rectangles from
    /// `LayoutWindow::layout_results` and builds a flat list for fast
    /// point-in-rect testing.
    pub fn rebuild_from_layout(&mut self, layout_results: &BTreeMap<DomId, DomLayoutResult>) {
        self.rebuild_from_layout_with_gpu(layout_results, None);
    }

    /// Like [`Self::rebuild_from_layout`], but transform-aware: `gpu` is the
    /// window's [`GpuStateManager`](crate::managers::gpu_state::GpuStateManager),
    /// whose per-DOM `GpuValueCache::reference_frame_of` names the EXACT set
    /// of nodes the display list wrapped in `PushReferenceFrame` (the
    /// display-list builder asks the same question) — so hit-test chains and painted frames cannot
    /// disagree about which nodes transform. Pass `None` only when no
    /// transforms can exist (unit tests, static popups).
    /// The DOM node ids currently registered as USER-wheel scroll targets.
    /// Test/introspection helper: programmatically-scrollable-only containers
    /// (`overflow: hidden`) must never appear here.
    #[must_use]
    pub fn debug_scroll_container_nodes(&self) -> Vec<NodeId> {
        self.scroll_containers.iter().map(|e| e.node_id).collect()
    }

    /// Test/introspection helper: every registered entry for `node` in `dom`
    /// as `(rect, chain-index, clip-rects)`. A DOM node can own several
    /// layout nodes, hence several entries.
    #[must_use]
    pub fn debug_entries_for(
        &self,
        dom: DomId,
        node: NodeId,
    ) -> Vec<(LogicalRect, u32, Vec<LogicalRect>)> {
        self.node_rects
            .get(&dom)
            .into_iter()
            .flatten()
            .filter(|e| e.node_id == node && e.scrollbar.is_none())
            .map(|e| (e.rect, e.chain, e.clips.iter().map(|(r, _)| *r).collect()))
            .collect()
    }

    pub fn rebuild_from_layout_with_gpu(
        &mut self,
        layout_results: &BTreeMap<DomId, DomLayoutResult>,
        gpu: Option<&crate::managers::gpu_state::GpuStateManager>,
    ) {
        // Once per layout on the CPU backend (the finalize tail of every
        // relayout, animation frames included).
        let _p = crate::probe::Probe::span("cpu_hit_tester_rebuild");
        self.node_rects.clear();
        self.chains.clear();
        // chain 0 = empty
        self.chains.push(ChainNode {
            parent: 0,
            link: None,
        });
        self.scroll_containers.clear();
        self.dom_placements.clear();
        self.index.clear();
        self.baked_frames.clear();

        let placements = resolve_virtual_view_placements(layout_results);

        let mut chain_lookup: ChainLookup = std::collections::HashMap::new();
        self.children_of.clear();
        self.view_hosts.clear();
        self.view_items.clear();
        for (dom_id, p) in &placements {
            self.dom_placements.insert(*dom_id, p.rect);
            // The boxes below are placed with the item's offset of now.
            self.view_items.insert(
                *dom_id,
                ViewItem {
                    host: p.host,
                    item: p.item,
                    built_offset: p.content_offset,
                    shift: LogicalPosition::zero(),
                },
            );
            self.children_of
                .entry(p.host)
                .or_default()
                .push((p.item, *dom_id));
            if let Some(view) = p.host_node {
                self.view_hosts.insert(*dom_id, (p.host, view));
            }
        }
        for children in self.children_of.values_mut() {
            children.sort_by(|a, b| b.cmp(a));
        }

        for (dom_id, layout_result) in layout_results {
            let mut entries = Vec::new();
            // The layout node of every entry, for the paint-order sort below.
            let mut entry_layout_idx: Vec<usize> = Vec::new();

            let positions = &layout_result.calculated_positions;
            let nodes = &layout_result.layout_tree.nodes;
            let styled_dom = &layout_result.styled_dom;

            // Child DOM: shift into window space by the placement origin, and
            // clip to the host VIEWPORTS — a different rect, see `Placement`.
            let (offset, dom_clip_rects, base_chain_vec) = placements.get(dom_id).map_or_else(
                || (LogicalPosition::zero(), Vec::new(), Vec::new()),
                |p| (p.rect.origin, p.clips.clone(), p.chain.clone()),
            );
            let base_chain =
                intern_chain(&mut self.chains, &mut chain_lookup, 0, base_chain_vec);
            let dom_clips: Vec<(LogicalRect, u32)> = dom_clip_rects
                .into_iter()
                .map(|(rect, chain)| {
                    let interned = intern_chain(&mut self.chains, &mut chain_lookup, 0, chain);
                    (rect, interned)
                })
                .collect();

            let scroll_ids = &layout_result.scroll_ids;
            // The nodes the display list opens a reference frame for - CSS
            // `transform` or the animation channel (a node mid-slide).
            let gpu_cache = gpu.and_then(|g| g.caches.get(dom_id));
            // The frames every node of this dom is painted in - one answer
            // with the display list (see `solver3::scroll_chain`).
            let scroll_chains = ScrollChains::compute(
                &layout_result.layout_tree,
                styled_dom,
                &layout_result.scroll_ids,
            );
            // Where the display list opens each node's reference frame: the
            // corner its matrix is painted about (`PushReferenceFrame::
            // bounds`). A node the list opens no frame for (a list built
            // without the values, a hand-made one) takes its box's corner,
            // which is what the builder passes.
            let mut frame_corners: std::collections::HashMap<NodeId, LogicalPosition> =
                std::collections::HashMap::new();
            for (item_idx, item) in layout_result.display_list.items.iter().enumerate() {
                let crate::solver3::display_list::DisplayListItem::PushReferenceFrame {
                    bounds: frame_box,
                    transform_key,
                    initial_transform,
                } = item
                else {
                    continue;
                };
                let owner = layout_result
                    .display_list
                    .node_mapping
                    .get(item_idx)
                    .copied()
                    .flatten();
                if let Some(owner) = owner {
                    frame_corners
                        .entry(owner)
                        .or_insert_with(|| frame_box.inner().origin);
                    // The matrix this frame was built with, for the time its
                    // key has no live value (A6-24). The node's OWN frame: the
                    // first it opens, and not the sheen layer a material's
                    // node opens as well (another key, the highlight's place).
                    let sheen_layer = gpu_cache
                        .is_some_and(|c| c.sheen_transform_keys.get(&owner) == Some(transform_key));
                    if !sheen_layer {
                        self.baked_frames
                            .entry((*dom_id, owner))
                            .or_insert(*initial_transform);
                    }
                }
            }
            let chain_of = compute_node_chains(
                layout_result,
                &scroll_chains,
                *dom_id,
                base_chain,
                &|n| gpu_cache.is_some_and(|c| c.reference_frame_of(n).is_some()),
                &|layout_idx, node_id| {
                    let corner = frame_corners
                        .get(&node_id)
                        .or_else(|| positions.get(layout_idx))
                        .copied()
                        .unwrap_or_default();
                    // In the space the dom's entries are kept in.
                    LogicalPosition::new(corner.x + offset.x, corner.y + offset.y)
                },
                &mut self.chains,
                &mut chain_lookup,
            );

            // Scroll containers of this DOM, for wheel-target containment.
            // Only USER-scrollable containers become wheel targets:
            // overflow:hidden boxes carry scroll ids (programmatic
            // scrolling - scroll-into-view, callback offsets - reaches
            // them), but css-overflow-3 disables their user-triggered
            // scrolling, so the hit-tester must not route the wheel there.
            for (&layout_idx, &scroll_id) in scroll_ids {
                let Some(n) = nodes.get(layout_idx.index()) else {
                    continue;
                };
                let Some(node_id) = n.dom_node_id else {
                    continue;
                };
                let (Some(pos), Some(size)) = (positions.get(layout_idx.index()), n.used_size)
                else {
                    continue;
                };
                // THE VIEWPORT (CSS Overflow 3 §3.3): the root element's own
                // `visible` is the viewport's `auto`, and its scrollport is
                // the WINDOW - the rect `register_scroll_nodes` publishes -
                // not the root's box inside the page margins. Read through
                // its own overflow, the page's scroller was never a wheel
                // target and the wheel over the page was dropped.
                let is_viewport =
                    crate::solver3::scrollbar::is_viewport_scroller(*dom_id, node_id);
                let scrollport_overflow = |v| {
                    if is_viewport {
                        crate::solver3::getters::apply_viewport_overflow_rule(node_id, v)
                    } else {
                        v
                    }
                };
                let user_scrollable = styled_dom
                    .styled_nodes
                    .as_container()
                    .get(node_id)
                    .is_some_and(|sn| {
                        let st = &sn.styled_node_state;
                        scrollport_overflow(get_overflow_x(styled_dom, node_id, st))
                            .allows_user_scrolling()
                            || scrollport_overflow(get_overflow_y(styled_dom, node_id, st))
                                .allows_user_scrolling()
                    });
                if !user_scrollable {
                    continue;
                }
                let rect = if is_viewport {
                    LogicalRect::new(LogicalPosition::zero(), layout_result.viewport.size)
                } else {
                    LogicalRect {
                        origin: LogicalPosition {
                            x: pos.x + offset.x,
                            y: pos.y + offset.y,
                        },
                        size,
                    }
                };
                self.scroll_containers.push(ScrollContainerEntry {
                    dom_id: *dom_id,
                    node_id,
                    layout_idx,
                    scroll_id,
                    rect,
                    chain: chain_of[layout_idx.index()],
                });
            }

            let clip_axes = overflow_clip_axes(styled_dom, nodes);
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

                // Clip this node to the VirtualView composite bounds
                // (`dom_clip`) and every `overflow: hidden | clip | scroll |
                // auto` ancestor's box it is painted inside — otherwise a
                // node that is scrolled or clipped out of its ancestor would
                // still claim pointer events. "Inside" is its scroll chain: a
                // fixed box is not clipped by a wrapper that is not its
                // containing block, and the display list does not clip it.
                let scroll_chain = scroll_chains.box_chain(LayoutNodeId::new(idx));
                let clips = compute_node_clips(
                    &clip_axes,
                    nodes,
                    positions,
                    idx,
                    offset,
                    &dom_clips,
                    &chain_of,
                    &|anc| scroll_chain.contains(LayoutNodeId::new(anc)),
                );

                entries.push(HitTestEntry {
                    node_id,
                    rect,
                    chain: chain_of[idx],
                    clips,
                    // azul has no `pointer-events` CSS property yet, so every laid-out
                    // node is hit-testable. Populate this from the styled DOM once such
                    // a property is added to `azul_css`.
                    pointer_events_none: false,
                    clip_path: node_clip_path(styled_dom, node_id),
                    rank: usize::MAX,
                    paints_own: false,
                    scrollbar: None,
                });
                entry_layout_idx.push(idx);
            }

            // The fragments of the INLINE elements (a link inside a
            // paragraph), which the walk above cannot see: see
            // `push_inline_fragment_entries`.
            push_inline_fragment_entries(
                layout_result,
                *dom_id,
                offset,
                &mut entries,
                &mut entry_layout_idx,
            );

            // The SCROLLBARS, each at the item that paints it: see
            // `scrollbar_entries`.
            let bars = scrollbar_entries(
                layout_result,
                &clip_axes,
                offset,
                &dom_clips,
                &chain_of,
                &scroll_chains,
            );

            // PAINT ORDER, taken from the display list itself: the topmost hit
            // has to be the box painted on top. Layout order is not paint
            // order - a positioned box paints after the in-flow blocks that
            // follow it in the tree (CSS 2.2 Appendix E), so a fixed header
            // lost the pointer to the page content scrolled under it. The
            // sort is stable: ties keep tree order, and a bar - appended
            // last - stays above a box that ranks at the same item.
            let sort_span = crate::probe::Probe::span("hit_test_paint_order_sort");
            let (ranks, own) = paint_ranks(&layout_result.display_list, nodes);
            let mut ranked: Vec<(usize, HitTestEntry)> = entry_layout_idx
                .into_iter()
                .zip(entries)
                .map(|(idx, mut e)| {
                    e.paints_own = own.get(idx).is_some_and(|item| *item != usize::MAX);
                    (ranks.get(idx).copied().unwrap_or(usize::MAX), e)
                })
                .collect();
            ranked.extend(bars);
            ranked.sort_by_key(|(rank, _)| *rank);
            let entries: Vec<HitTestEntry> = ranked
                .into_iter()
                .map(|(rank, mut e)| {
                    e.rank = rank;
                    e
                })
                .collect();
            drop(sort_span);

            // The viewports as every entry carries them (`compute_node_clips`).
            let index_clips = dom_clips
                .iter()
                .map(|(rect, chain)| (sanitize_clip_rect(*rect), *chain))
                .collect();
            let mut index = DomIndex::build(&entries, index_clips);
            index.split_framed(*dom_id, &self.chains);
            self.index.insert(*dom_id, index);
            self.node_rects.insert(*dom_id, entries);
        }
    }

    /// Perform a hit test at the given position, ignoring scroll offsets.
    ///
    /// Only correct for content that cannot scroll (e.g. menu popups) and for
    /// unit tests. Interactive windows must use [`Self::hit_test_scrolled`] —
    /// this wrapper tests the STATIC layout geometry, which is exactly the
    /// "clicks land on pre-scroll targets" bug for anything inside a scroll
    /// frame.
    #[must_use]
    pub fn hit_test(&self, position: LogicalPosition) -> Vec<(DomId, NodeId)> {
        self.hit_test_scrolled(position, &|_, _| None, &|_, _| None)
            .into_iter()
            .map(|(d, n, _)| (d, n))
            .collect()
    }

    /// Perform a hit test at the given position with live scroll offsets and
    /// transform values.
    ///
    /// `resolve_scroll` returns the CURRENT scroll offset of a scroll
    /// container (`ScrollManager::get_current_offset`); `resolve_transform`
    /// the CURRENT matrix of a reference-frame owner
    /// (`GpuStateManager::painted_transform_of` — the same value the CPU
    /// raster paints with and `WebRender` is sent, CSS or animation channel).
    /// Content is hit where it is painted: a point `p` hits a node iff `p`,
    /// taken back through the node's chain - outermost link first, a scroll
    /// frame by adding its offset, a reference frame by the inverse of its
    /// matrix about its box's corner, a `VirtualView` by how far its content
    /// has moved ([`Self::sync_view_placements`]) - lands in the node's
    /// static rect. A frame whose matrix cannot be inverted is hit nowhere.
    /// Clip boxes are tested in the clip OWNER's chain — a scroller's
    /// viewport clips where the viewport IS, not where its content went.
    ///
    /// Returns `(dom, node, local_point)` triples in reverse paint order
    /// (topmost first) ACROSS DOMs as well as within one - a child DOM's hits
    /// precede its host's, as they do in `WebRender`'s result - where
    /// `local_point` is the query point mapped into
    /// that node's STATIC layout space — callers use it directly for
    /// node-relative points (caret placement, `point_relative_to_item`).
    ///
    /// Boxes only: the scrollbars painted among them are the press
    /// arbiter's question ([`Self::scrollbar_at`]).
    #[must_use]
    pub fn hit_test_scrolled(
        &self,
        position: LogicalPosition,
        resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
    ) -> Vec<(DomId, NodeId, LogicalPosition)> {
        self.boxes_at(position, resolve_scroll, resolve_transform, None)
    }

    /// [`Self::hit_test_scrolled`] for a caller that knows the transform
    /// stamps of the doms (`stamps_of`, as for [`Self::sync_frame_bounds`]):
    /// the same answer, and where the tester holds painted bounds made with
    /// the same stamps, only the framed groups whose bounds hold the point
    /// are looked at.
    #[must_use]
    pub fn hit_test_scrolled_stamped(
        &self,
        position: LogicalPosition,
        resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
        stamps_of: &dyn Fn(DomId) -> TransformStamps,
    ) -> Vec<(DomId, NodeId, LogicalPosition)> {
        self.boxes_at(position, resolve_scroll, resolve_transform, Some(stamps_of))
    }

    /// The boxes at `position`, front to back ([`Self::hit_test_scrolled`]).
    fn boxes_at(
        &self,
        position: LogicalPosition,
        resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
        stamps_of: Option<&dyn Fn(DomId) -> TransformStamps>,
    ) -> Vec<(DomId, NodeId, LogicalPosition)> {
        let results: Vec<(DomId, NodeId, LogicalPosition)> = self
            .entries_at_counted(position, resolve_scroll, resolve_transform, stamps_of)
            .0
            .into_iter()
            .filter(|hit| hit.scrollbar.is_none())
            .map(|hit| (hit.dom_id, hit.node_id, hit.local))
            .collect();
        // `AZ_TRACE_HIT=1`: every query's hits, front to back - which node a
        // click lands on, without a debugger.
        #[cfg(feature = "std")]
        {
            static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            if *TRACE.get_or_init(|| std::env::var_os("AZ_TRACE_HIT").is_some()) {
                let front: Vec<(usize, usize)> = results
                    .iter()
                    .take(12)
                    .map(|(d, n, _)| (d.inner, n.index()))
                    .collect();
                eprintln!(
                    "[hit] ({:.1}, {:.1}) -> (dom, node) front to back: {front:?}",
                    position.x, position.y
                );
            }
        }
        results
    }

    /// THE SCROLLBAR PAINTED ON TOP at `position`, for the press arbiter
    /// (`LayoutWindow::route_press`): `(dom, scroll container, axis)`, or
    /// `None` where no bar is painted on top.
    ///
    /// Chrome's rule - a scrollbar is hit only where it is painted - with
    /// the bars ranked among the boxes at the items that paint them
    /// (`scrollbar_entries`): a bar an ancestor's clip cuts off is not
    /// there, and the front-most bar at the point is pressed only if no box
    /// painted after it covers the point. A box covers it when it paints an
    /// item of its OWN after the bar - a positioned dialog over a list, its
    /// button, a child dom composited above. Two kinds of box in front of the
    /// bar do not:
    /// - one placed there only by its relatives' paint (`paint_ranks`: the
    ///   bar's own box, its ancestors or its content, painting nothing
    ///   themselves) - what is painted at the point is the bar;
    /// - the bar's own CONTENT, any box painted in the container's scroll
    ///   frame: a scroller's bars stay above everything it scrolls, its
    ///   positioned descendants included (Chrome reorders the overflow
    ///   controls above them for exactly that).
    #[must_use]
    pub fn scrollbar_at(
        &self,
        position: LogicalPosition,
        resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
    ) -> Option<(DomId, NodeId, ScrollbarOrientation)> {
        let front_to_back = self.entries_at(position, resolve_scroll, resolve_transform);
        let bar_at = front_to_back
            .iter()
            .position(|hit| hit.scrollbar.is_some())?;
        let bar = front_to_back[bar_at];
        let own_frame = HitChainLink::Scroll(bar.dom_id, bar.node_id);
        let scrolled_by_the_bar =
            |hit: &EntryHit| self.chain_links(hit.chain).contains(&own_frame);
        if front_to_back[..bar_at]
            .iter()
            .any(|hit| hit.paints_own && !scrolled_by_the_bar(hit))
        {
            return None;
        }
        Some((bar.dom_id, bar.node_id, bar.scrollbar?))
    }

    /// The scroll frames a wheel at `position` may drive, given the
    /// front-most box there (`front`, the first of
    /// [`Self::hit_test_scrolled`]'s hits): the frames that box is painted
    /// in - its chain, host frames of a child dom included - and the box
    /// itself (its own frame moves its content, not the box).
    ///
    /// The wheel's answer to the same question the press arbiter asks: a
    /// scroll container clipped away at the point, or under a box painted
    /// over it, is not what the pointer is over.
    fn wheel_frames(
        &self,
        front: Option<&(DomId, NodeId, LogicalPosition)>,
    ) -> std::collections::BTreeSet<(DomId, NodeId)> {
        let mut frames = std::collections::BTreeSet::new();
        let Some(&(dom_id, node_id, local)) = front else {
            return frames;
        };
        frames.insert((dom_id, node_id));
        // The entry the hit came from: `local` is in its chain's space, so
        // it lies in that entry's rect (the topmost such, as the walk went).
        let chain = self.node_rects.get(&dom_id).and_then(|entries| {
            let holds = |e: &&HitTestEntry| {
                e.scrollbar.is_none() && e.node_id == node_id && point_in_rect(local, &e.rect)
            };
            // The node's topmost box, which the index keeps: the entry the
            // hit came from unless the node has several boxes - then the
            // walk from the top, as before.
            self.index
                .get(&dom_id)
                .and_then(|index| index.top_entry_of_node.get(node_id.index()))
                .and_then(|i| entries.get(*i as usize))
                .filter(holds)
                .or_else(|| entries.iter().rev().find(holds))
                .map(|e| e.chain)
        });
        if let Some(chain) = chain {
            for link in self.chain_links(chain) {
                if let HitChainLink::Scroll(d, n) = link {
                    frames.insert((d, n));
                }
            }
        }
        // The `VirtualView`s that show the box's dom, out to the window: they
        // scroll it without a frame in its chain. Bounded like the
        // placements' nesting, in case a view graph is cyclic.
        let mut dom = dom_id;
        for _ in 0..self.view_hosts.len() {
            let Some(&(host, view)) = self.view_hosts.get(&dom) else {
                break;
            };
            frames.insert((host, view));
            dom = host;
        }
        frames
    }

    /// Every entry - boxes and scrollbars - `position` lands on, front to
    /// back ACROSS doms (see [`Self::hit_test_scrolled`] for the spaces).
    fn entries_at(
        &self,
        position: LogicalPosition,
        resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
    ) -> Vec<EntryHit> {
        self.entries_at_counted(position, resolve_scroll, resolve_transform, None)
            .0
    }

    /// What a query at `position` does ([`HitTestWork`]): how many boxes it
    /// tests, how many chain groups it looks at, how many links it inverts.
    /// For the guards of the query's cost, and for a diagnosis.
    #[must_use]
    pub fn hit_test_work(
        &self,
        position: LogicalPosition,
        resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
    ) -> HitTestWork {
        self.entries_at_counted(position, resolve_scroll, resolve_transform, None)
            .1
    }

    /// What [`Self::hit_test_scrolled_stamped`] does at `position`
    /// ([`HitTestWork`]).
    #[must_use]
    pub fn hit_test_work_stamped(
        &self,
        position: LogicalPosition,
        resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
        stamps_of: &dyn Fn(DomId) -> TransformStamps,
    ) -> HitTestWork {
        self.entries_at_counted(position, resolve_scroll, resolve_transform, Some(stamps_of))
            .1
    }

    /// [`Self::entries_at`] with what it cost. `stamps_of`: the transform
    /// stamps of the doms, from a caller that keeps the painted bounds of
    /// the framed groups in step ([`Self::sync_frame_bounds`]); `None`: no
    /// kept bounds are used.
    fn entries_at_counted(
        &self,
        position: LogicalPosition,
        resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
        stamps_of: Option<&dyn Fn(DomId) -> TransformStamps>,
    ) -> (Vec<EntryHit>, HitTestWork) {
        let mut results = Vec::new();
        let mut work = HitTestWork::default();

        // A chain's point is worked out when a group or a clip first asks
        // for it, from its parent chain's point and its own innermost link,
        // and kept for the rest of the query.
        let view_shift = |dom_id: DomId| self.view_shift_of(dom_id);
        // A frame whose key has no live value is hit where the list's own
        // matrix paints it (A6-24).
        let painted = painted_or_baked(resolve_transform, &self.baked_frames);
        let mut points = ChainPoints::new(
            &self.chains,
            LiveValues {
                scroll: resolve_scroll,
                transform: &painted,
                view_shift: &view_shift,
            },
            position,
        );
        points.stamps_of = stamps_of;

        // FRONT-MOST FIRST across doms as well (9g-ii-e-i): a child dom (a
        // `VirtualView` page, an iframe) paints AT ITS VIEW'S ITEM in its
        // host's display list - over the boxes painted before the item, under
        // the ones painted after it (a map's zoom buttons, a sidebar laid
        // over the map). WebRender's one scene for every dom answers in that
        // order; visiting the doms by id put every child dom over its whole
        // host, so a click on a button over a map reached the map. The root
        // doms (no host) go highest id first; a dom no walk reaches (its
        // host is gone) last, so it is still hit.
        let mut visited = std::collections::BTreeSet::new();
        let hosted: std::collections::BTreeSet<DomId> =
            self.children_of.values().flatten().map(|(_, d)| *d).collect();
        let roots: Vec<DomId> = self
            .node_rects
            .keys()
            .rev()
            .filter(|d| !hosted.contains(d))
            .copied()
            .collect();
        for dom_id in roots {
            self.hit_dom(dom_id, &mut points, &mut visited, &mut results, &mut work);
        }
        let unreached: Vec<DomId> = self
            .node_rects
            .keys()
            .rev()
            .filter(|d| !visited.contains(*d))
            .copied()
            .collect();
        for dom_id in unreached {
            self.hit_dom(dom_id, &mut points, &mut visited, &mut results, &mut work);
        }
        work.links_resolved = points.links_resolved;
        (results, work)
    }

    /// Does `entry` take a press at `p`, the query point in the entry's
    /// static space? Its rect (and clip path) holds the point, and every
    /// clip box does, each in its owner's space.
    fn entry_holds(entry: &HitTestEntry, p: LogicalPosition, points: &mut ChainPoints<'_>) -> bool {
        if entry.pointer_events_none {
            return false;
        }
        if !point_in_rect(p, &entry.rect)
            || !point_in_clip_path(p, &entry.rect, entry.clip_path.as_ref())
        {
            return false;
        }
        entry.clips.iter().all(|(clip, chain)| {
            points
                .local(*chain)
                .is_some_and(|q| point_in_rect(plane_point(q), clip))
        })
    }

    /// The entries of `dom_id` the query point is in, topmost (painted last)
    /// first, each with the point in its static space.
    fn dom_hits(
        &self,
        dom_id: DomId,
        entries: &[HitTestEntry],
        points: &mut ChainPoints<'_>,
        work: &mut HitTestWork,
    ) -> Vec<(usize, LogicalPosition)> {
        let mut hits: Vec<(usize, LogicalPosition)> = Vec::new();
        let Some(index) = self.index.get(&dom_id) else {
            // No index (never by construction): every entry is looked at.
            for (i, entry) in entries.iter().enumerate().rev() {
                work.entries_tested += 1;
                if let Some(p) = points.local(entry.chain).map(plane_point) {
                    if Self::entry_holds(entry, p, points) {
                        hits.push((i, p));
                    }
                }
            }
            return hits;
        };
        // The viewports the dom shows through: outside one, none of its
        // entries can hold the point (each carries these clips).
        for (viewport, chain) in &index.dom_clips {
            let shown = points
                .local(*chain)
                .is_some_and(|q| point_in_rect(plane_point(q), viewport));
            if !shown {
                return hits;
            }
        }
        // The painted bounds of the framed groups, when they were made with
        // the matrices the frames have now (the caller's stamps say so).
        let stamps_of = points.stamps_of;
        let bounds = index.frame_bounds.as_ref().filter(|bounds| {
            stamps_of.is_some_and(|stamps_of| stamps_of(dom_id) == bounds.stamps)
        });
        if let Some(bounds) = bounds {
            for &g in &index.plain {
                if let Some(group) = index.groups.get(g as usize) {
                    Self::group_hits(group, entries, points, work, &mut hits);
                }
            }
            for under in &bounds.parents {
                // Nothing inside the chain around the frames is at the
                // point: nothing inside one of the frames is.
                let Some(around) = points.local(under.parent).map(plane_point) else {
                    continue;
                };
                let cell: &[u32] = under.grid.as_ref().map_or(&[], |grid| grid.at(around));
                for &g in under.always.iter().chain(cell) {
                    if let Some(group) = index.groups.get(g as usize) {
                        Self::group_hits(group, entries, points, work, &mut hits);
                    }
                }
            }
        } else {
            for group in &index.groups {
                Self::group_hits(group, entries, points, work, &mut hits);
            }
        }
        // Reverse paint order: the box painted last is on top.
        hits.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        hits
    }

    /// The entries of one chain group the query point is in, pushed to
    /// `hits` with the point in the group's static space.
    fn group_hits(
        group: &ChainGroup,
        entries: &[HitTestEntry],
        points: &mut ChainPoints<'_>,
        work: &mut HitTestWork,
        hits: &mut Vec<(usize, LogicalPosition)>,
    ) {
        work.groups_visited += 1;
        let Some(p) = points.local(group.chain).map(plane_point) else {
            return; // nothing inside this chain is at the point
        };
        let cell: &[u32] = group.grid.as_ref().map_or(&[], |grid| grid.at(p));
        for &i in group.always.iter().chain(cell) {
            let Some(entry) = entries.get(i as usize) else {
                continue;
            };
            work.entries_tested += 1;
            if Self::entry_holds(entry, p, points) {
                hits.push((i as usize, p));
            }
        }
    }

    /// The hits of `dom_id`, topmost first, with each child dom it shows
    /// spliced in where its `VirtualView` paints: before every entry painted
    /// at or before the view's item (a node that paints nothing ranks below
    /// the view). `visited` ends a cyclic view graph.
    ///
    /// The entries are in paint order (ascending rank, the unranked last),
    /// and only the ones the point is in are looked at ([`Self::dom_hits`]).
    /// A child dom comes before an entry when its item is at or after the
    /// entry's rank - and before every entry when the dom has an unranked
    /// one: walked from the last entry, that one is met first and is under
    /// every view. Which is what a walk over all the entries answers.
    fn hit_dom(
        &self,
        dom_id: DomId,
        points: &mut ChainPoints<'_>,
        visited: &mut std::collections::BTreeSet<DomId>,
        results: &mut Vec<EntryHit>,
        work: &mut HitTestWork,
    ) {
        if !visited.insert(dom_id) {
            return;
        }
        let children: &[(usize, DomId)] =
            self.children_of.get(&dom_id).map_or(&[], Vec::as_slice);
        let mut next = 0;
        if let Some(entries) = self.node_rects.get(&dom_id) {
            let views_over_everything = entries.last().is_some_and(|e| e.rank == usize::MAX);
            for (i, local) in self.dom_hits(dom_id, entries, points, work) {
                let entry = &entries[i];
                while let Some(&(item, child)) = children.get(next) {
                    // At the item: the view's host (whose first painted
                    // item IS the view when it paints nothing else) and the
                    // wrappers that rank at it are under the page.
                    let child_is_above = views_over_everything || item >= entry.rank;
                    if !child_is_above {
                        break;
                    }
                    self.hit_dom(child, points, visited, results, work);
                    next += 1;
                }
                results.push(EntryHit {
                    dom_id,
                    node_id: entry.node_id,
                    local,
                    chain: entry.chain,
                    paints_own: entry.paints_own,
                    scrollbar: entry.scrollbar,
                });
            }
        }
        for &(_, child) in children.get(next..).unwrap_or(&[]) {
            self.hit_dom(child, points, visited, results, work);
        }
    }
}

/// The query point in every chain's static space, worked out on demand
/// (one query of [`CpuHitTester::entries_at`]).
struct ChainPoints<'a> {
    chains: &'a [ChainNode],
    live: LiveValues<'a>,
    /// Per chain: its point, once asked for.
    points: Vec<ChainPoint>,
    /// The chains still to resolve on the way up from the one asked for.
    pending: Vec<u32>,
    /// Links inverted so far ([`HitTestWork::links_resolved`]).
    links_resolved: usize,
    /// The transform stamps of the doms, from a caller that keeps the
    /// painted bounds of the framed groups in step with them; `None`: no
    /// kept bounds are used ([`CpuHitTester::sync_frame_bounds`]).
    stamps_of: Option<&'a dyn Fn(DomId) -> TransformStamps>,
}

/// The query point in one chain's static space.
#[derive(Clone, Copy)]
enum ChainPoint {
    /// Not asked for yet.
    Unknown,
    /// Nothing inside the chain is painted at the query point (a frame of it
    /// cannot be inverted).
    Nowhere,
    At(PlanePoint),
}

impl<'a> ChainPoints<'a> {
    fn new(chains: &'a [ChainNode], live: LiveValues<'a>, screen: LogicalPosition) -> Self {
        let mut points = vec![ChainPoint::Unknown; chains.len().max(1)];
        // The empty chain: the window's own space.
        points[0] = ChainPoint::At((f64::from(screen.x), f64::from(screen.y)));
        Self {
            chains,
            live,
            points,
            pending: Vec::new(),
            links_resolved: 0,
            stamps_of: None,
        }
    }

    /// The query point in `chain`'s static space; `None` when nothing inside
    /// the chain is at the point. A chain the table does not know is taken
    /// as the empty one.
    fn local(&mut self, chain: u32) -> Option<PlanePoint> {
        // Up: to the nearest chain whose point is known.
        self.pending.clear();
        let mut cur = chain;
        let mut point = loop {
            let known = match self.points.get(cur as usize).copied() {
                Some(known) => known,
                None => self.points[0],
            };
            match known {
                ChainPoint::At(p) => break Some(p),
                ChainPoint::Nowhere => break None,
                ChainPoint::Unknown => {}
            }
            self.pending.push(cur);
            // A chain's parent is interned before it; a table where that is
            // not so ends the walk at the window's space.
            let parent = self.chains.get(cur as usize).map_or(0, |node| node.parent);
            cur = if parent < cur { parent } else { 0 };
        };
        // Down: one link at a time.
        while let Some(c) = self.pending.pop() {
            let link = self.chains.get(c as usize).and_then(|node| node.link);
            point = match (point, link) {
                (Some(p), Some(link)) => {
                    self.links_resolved += 1;
                    link_to_local(&link, p, &self.live)
                }
                (kept, _) => kept,
            };
            if let Some(slot) = self.points.get_mut(c as usize) {
                *slot = point.map_or(ChainPoint::Nowhere, ChainPoint::At);
            }
        }
        point
    }
}

/// The paint rank of every layout node of one dom: the index of its first
/// item in `display_list`, which the generator emits in CSS painting order.
/// A node that paints nothing of its own ranks at its first painted
/// descendant, and one with neither at its parent, so it stays where it
/// would paint.
/// The hit-test entries of the INLINE elements of one dom (E10).
///
/// A non-replaced inline element (`<a>`, `<span>`, `<b>`) has no box of its
/// own: its text is laid out in the enclosing block's inline formatting
/// context, so the layout-node walk gives it no rect and a click on a link
/// inside a paragraph reached the paragraph, never the link - no click
/// callback, no pointer cursor, no `href` for the app to open.
///
/// Its FRAGMENTS are the text runs the display list already hit-tests for
/// the cursor (`TAG_TYPE_CURSOR` areas: one per line and style run, keyed
/// by the TEXT node, extending over the run's last letter). For every such
/// run, every inline element between its text node and the first ancestor
/// that is not `display: inline` (the block, or an inline-block) gets an
/// entry with the run's rect - a link wrapped over three lines gets three,
/// and the space between the lines stays the paragraph's. The entries take
/// the chain and the clips of the nearest boxed ancestor they are painted
/// in, and its paint rank (pushed after it, so the stable sort keeps them on
/// top of it).
fn push_inline_fragment_entries(
    layout_result: &DomLayoutResult,
    dom_id: DomId,
    offset: LogicalPosition,
    entries: &mut Vec<HitTestEntry>,
    entry_layout_idx: &mut Vec<usize>,
) {
    use azul_core::hit_test::TAG_TYPE_CURSOR;
    use azul_css::props::layout::LayoutDisplay;

    use crate::solver3::{display_list::DisplayListItem, getters::get_display_property};

    /// Deeper than any real inline nesting; bounds a malformed hierarchy.
    const MAX_INLINE_DEPTH: usize = 256;

    let styled_dom = &layout_result.styled_dom;
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let node_count = styled_dom.node_data.as_ref().len();
    let parent_of = |n: NodeId| {
        hierarchy
            .get(n)
            .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
    };
    // The first entry of every boxed node (a node can own several).
    let mut first_entry: BTreeMap<NodeId, usize> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        first_entry.entry(e.node_id).or_insert(i);
    }

    let mut fragments: Vec<(HitTestEntry, usize)> = Vec::new();
    for item in &layout_result.display_list.items {
        let DisplayListItem::HitTestArea { bounds, tag } = item else {
            continue;
        };
        if tag.1 & 0xFF00 != TAG_TYPE_CURSOR || (tag.0 >> 32) as usize != dom_id.inner {
            continue;
        }
        let text_node = NodeId::new((tag.0 & 0xFFFF_FFFF) as usize);
        let run = bounds.0;
        if text_node.index() >= node_count || !(run.size.width > 0.0 && run.size.height > 0.0)
        {
            continue;
        }
        // 1. The inline elements around this run, innermost first: every
        //    ancestor up to the first one that is not `display: inline`.
        let mut inline_elements: Vec<NodeId> = Vec::new();
        let mut cursor = parent_of(text_node);
        let mut depth = 0;
        while let Some(n) = cursor {
            depth += 1;
            if depth > MAX_INLINE_DEPTH {
                break;
            }
            let is_inline = get_display_property(styled_dom, Some(n)).unwrap_or_default()
                == LayoutDisplay::Inline;
            if !is_inline {
                break;
            }
            inline_elements.push(n);
            cursor = parent_of(n);
        }
        if inline_elements.is_empty() {
            continue; // text straight in its block: the block's own entry hits it
        }
        // 2. From there, the nearest ancestor with an entry: the box the run
        //    is painted in (its chain, clips and rank).
        let mut owner: Option<usize> = None;
        while let Some(n) = cursor {
            depth += 1;
            if depth > MAX_INLINE_DEPTH {
                break;
            }
            if let Some(&i) = first_entry.get(&n) {
                owner = Some(i);
                break;
            }
            cursor = parent_of(n);
        }
        let Some(owner) = owner else {
            continue;
        };
        let rect = LogicalRect {
            origin: LogicalPosition {
                x: run.origin.x + offset.x,
                y: run.origin.y + offset.y,
            },
            size: run.size,
        };
        for node_id in inline_elements {
            fragments.push((
                HitTestEntry {
                    node_id,
                    rect,
                    chain: entries[owner].chain,
                    clips: entries[owner].clips.clone(),
                    pointer_events_none: false,
                    clip_path: None,
                    rank: usize::MAX,
                    paints_own: false,
                    scrollbar: None,
                },
                entry_layout_idx[owner],
            ));
        }
    }
    for (entry, layout_idx) in fragments {
        entries.push(entry);
        entry_layout_idx.push(layout_idx);
    }
}

/// The SCROLLBAR entries of one dom: one per `ScrollBarStyled` / `ScrollBar`
/// item with a hit id, `(item index, entry)`, ranked at that item.
///
/// `paint_scrollbars` draws a scroll container's bars after its content and
/// after closing the container's own clips, so a bar is painted inside its
/// ANCESTORS' clips and scroll frames - the clips and chain of its box's own
/// entry - and under everything painted after it: a positioned dialog over a
/// list, an `overflow: hidden` ancestor cutting it off. Ranked at its own
/// item, it takes its place among the boxes in paint order, which is what
/// lets the press arbiter ask for the bar painted on top
/// ([`CpuHitTester::scrollbar_at`]).
///
/// The bar takes its box's chain, transform links included. A transformed
/// scroll container paints as a stacking context, whose bars are drawn
/// outside its own reference frame; the scroll manager places such a bar
/// untransformed too, so it is an approximation on both sides alike.
fn scrollbar_entries(
    layout_result: &DomLayoutResult,
    clip_axes: &[(bool, bool)],
    offset: LogicalPosition,
    dom_clips: &[(LogicalRect, u32)],
    chain_of: &[u32],
    scroll_chains: &ScrollChains,
) -> Vec<(usize, HitTestEntry)> {
    use crate::solver3::display_list::DisplayListItem;

    let mut bars = Vec::new();
    for (item_index, item) in layout_result.display_list.items.iter().enumerate() {
        let (bounds, orientation, hit_id) = match item {
            DisplayListItem::ScrollBarStyled { info } => {
                (*info.bounds.inner(), info.orientation, info.hit_id)
            }
            DisplayListItem::ScrollBar {
                bounds,
                orientation,
                hit_id,
                ..
            } => (*bounds.inner(), *orientation, *hit_id),
            _ => continue,
        };
        let Some(hit_id) = hit_id else {
            continue;
        };
        let node_id = match hit_id {
            ScrollbarHitId::VerticalTrack(_, n)
            | ScrollbarHitId::VerticalThumb(_, n)
            | ScrollbarHitId::HorizontalTrack(_, n)
            | ScrollbarHitId::HorizontalThumb(_, n) => n,
        };
        let Some(&layout_idx) = layout_result
            .layout_tree
            .dom_to_layout
            .get(&node_id)
            .and_then(|indices| indices.first())
        else {
            continue;
        };
        let idx = layout_idx.index();
        let scroll_chain = scroll_chains.box_chain(layout_idx);
        let clips = compute_node_clips(
            clip_axes,
            &layout_result.layout_tree.nodes,
            &layout_result.calculated_positions,
            idx,
            offset,
            dom_clips,
            chain_of,
            &|anc| scroll_chain.contains(LayoutNodeId::new(anc)),
        );
        bars.push((
            item_index,
            HitTestEntry {
                node_id,
                rect: LogicalRect::new(
                    LogicalPosition::new(bounds.origin.x + offset.x, bounds.origin.y + offset.y),
                    bounds.size,
                ),
                chain: chain_of.get(idx).copied().unwrap_or(0),
                clips,
                pointer_events_none: false,
                clip_path: None,
                rank: item_index,
                paints_own: true,
                scrollbar: Some(orientation),
            },
        ));
    }
    bars
}

/// The paint rank of every layout node of one dom (the rule is spelled out
/// above `push_inline_fragment_entries`, where its comment ended up).
///
/// Returns `(rank, own)`: `own[idx]` is the node's first item of its OWN
/// (`usize::MAX` when it paints nothing itself), `rank[idx]` its rank.
fn paint_ranks(
    display_list: &crate::solver3::display_list::DisplayList,
    nodes: &[LayoutNodeHot],
) -> (Vec<usize>, Vec<usize>) {
    let mut own = vec![usize::MAX; nodes.len()];
    for (item, mapping) in display_list.layout_node_mapping.iter().enumerate() {
        let Some((idx, _)) = mapping else {
            continue;
        };
        if let Some(rank) = own.get_mut(*idx) {
            if *rank == usize::MAX {
                *rank = item;
            }
        }
    }
    // The tree is built pre-order (a parent's index is below its
    // children's): one sweep from the end carries each subtree's first
    // painted item up to its root.
    let mut subtree = own.clone();
    for idx in (0..nodes.len()).rev() {
        if let Some(parent) = nodes[idx].parent {
            if subtree[idx] < subtree[parent] {
                subtree[parent] = subtree[idx];
            }
        }
    }
    let mut rank = vec![usize::MAX; nodes.len()];
    for idx in 0..nodes.len() {
        rank[idx] = if own[idx] != usize::MAX {
            own[idx]
        } else if subtree[idx] != usize::MAX {
            subtree[idx]
        } else {
            nodes[idx].parent.map_or(usize::MAX, |p| rank[p])
        };
    }
    (rank, own)
}

/// A node's clip path: its own SVG geometry, with the `viewBox` it is drawn
/// in.
///
/// `None` for the overwhelming majority of nodes, which is why this is looked
/// up once at build time rather than per hit test.
fn node_clip_path(styled_dom: &StyledDom, node_id: NodeId) -> Option<HitClipGeometry> {
    let node_data = styled_dom.node_data.as_container();
    let azul_core::dom::SvgNodeData::Path(path) = node_data.get(node_id)?.get_svg_data()? else {
        return None;
    };
    // The coordinate system the geometry was authored in - the same walk the
    // display list does to place the clip mask, so pixels and pointer targets
    // cannot disagree.
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut cursor = Some(node_id);
    let mut view_box = None;
    while let Some(id) = cursor {
        if let Some(azul_core::dom::SvgNodeData::ViewBox {
            min_x,
            min_y,
            width,
            height,
        }) = node_data
            .get(id)
            .and_then(azul_core::dom::NodeData::get_svg_data)
        {
            view_box = Some((*min_x, *min_y, *width, *height));
            break;
        }
        cursor = hierarchy
            .get(id)
            .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
    }
    Some((path.clone(), view_box))
}

/// Is the point inside the node's clip path, if it has one?
///
/// `true` when there is no clip path: a node without one is bounded by its box
/// and the caller has already checked that.
fn point_in_clip_path(
    point: LogicalPosition,
    rect: &LogicalRect,
    clip_path: Option<&HitClipGeometry>,
) -> bool {
    let Some((path, view_box)) = clip_path else {
        return true;
    };
    // Window space -> the path's own user space. With a viewBox the geometry
    // was scaled into the box; without one it is already in logical px
    // relative to the box's origin.
    let (local_x, local_y) = match view_box {
        Some((min_x, min_y, vb_w, vb_h))
            if *vb_w > 0.0 && *vb_h > 0.0 && rect.size.width > 0.0 && rect.size.height > 0.0 =>
        {
            (
                (point.x - rect.origin.x) * vb_w / rect.size.width + min_x,
                (point.y - rect.origin.y) * vb_h / rect.size.height + min_y,
            )
        }
        _ => (point.x, point.y),
    };
    path.contains_point(local_x, local_y)
}

/// Simple point-in-rect test.
fn point_in_rect(point: LogicalPosition, rect: &LogicalRect) -> bool {
    point.x >= rect.origin.x
        && point.x < rect.origin.x + rect.size.width
        && point.y >= rect.origin.y
        && point.y < rect.origin.y + rect.size.height
}

/// Convert CPU hit test results to `FullHitTest` format.
///
/// Maps `(DomId, NodeId)` pairs from [`CpuHitTester::hit_test`] into the same
/// `FullHitTest` structure that `WebRender`'s `fullhittest_new_webrender`
/// produces, so the event dispatch code works identically for both backends.
///
/// This lives HERE (next to the tester that produces its input) rather than in
/// the DLL, because two hosts consume it: the desktop shells
/// (`wr_translate2::convert_cpu_hit_test_to_full`, which now delegates) and the
/// headless E2E runner (`crate::e2e::runner`). Two copies of "which node did
/// the pointer land on" is exactly the divergence that makes a scenario pass in
/// one host and fail in the other.
#[allow(clippy::cast_possible_truncation)] // bounded: DomId/NodeId indices, hit depth
#[allow(clippy::too_many_lines)] // moved verbatim from the DLL; one pass per hit-test kind
#[must_use]
pub fn convert_cpu_hit_test_to_full(
    tester: &CpuHitTester,
    hits: &[(DomId, NodeId, LogicalPosition)],
    old_focus_node: Option<DomNodeId>,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    cursor_position: LogicalPosition,
    resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> FullHitTest {
    use azul_core::{
        dom::OptionDomNodeId,
        hit_test::{HitTest, HitTestItem, OverflowingScrollNode, ScrollHitTestItem},
    };

    let focused_node = old_focus_node.map_or(OptionDomNodeId::None, OptionDomNodeId::Some);

    let mut hovered_nodes: BTreeMap<DomId, HitTest> = BTreeMap::new();

    for (depth, (dom_id, node_id, local_point)) in hits.iter().enumerate() {
        // `local_point` is the cursor already mapped into this node's STATIC
        // layout space (ancestor scroll offsets added back, ancestor
        // transforms inverted) — the same space the entry rects live in,
        // which is the node's static position translated by its VirtualView
        // placement. Two named steps take it to the ONE documented space
        // `HitTestItem::point_relative_to_item` carries: subtract the node's
        // static border-box origin (plus placement), then step in by the
        // content inset. `fullhittest_new_webrender` performs the identical
        // second step on WebRender's border-box-relative point, so the two
        // hosts now answer the same question the same way.
        let placement = tester
            .dom_placements
            .get(dom_id)
            .map_or_else(LogicalPosition::zero, |r| r.origin);
        let point_relative = layout_results
            .get(dom_id)
            .and_then(|lr| {
                lr.layout_tree
                    .dom_to_layout
                    .get(node_id)
                    .and_then(|indices| indices.first())
                    .and_then(|&idx| {
                        let node_pos = lr.calculated_positions.get(idx.index())?;
                        let border_box_origin = LogicalPosition::new(
                            node_pos.x + placement.x,
                            node_pos.y + placement.y,
                        );
                        Some(
                            StaticLayoutPoint::new(*local_point)
                                .to_border_box_local(border_box_origin)
                                .to_content_box_local(lr.layout_tree.content_inset(idx)),
                        )
                    })
            })
            .unwrap_or_else(ContentBoxLocal::zero);

        let hit_test = hovered_nodes.entry(*dom_id).or_insert_with(|| HitTest {
            regular_hit_test_nodes: BTreeMap::new(),
            scroll_hit_test_nodes: BTreeMap::new(),
            scrollbar_hit_test_nodes: BTreeMap::new(),
            cursor_hit_test_nodes: BTreeMap::new(),
        });

        // THE FRONT-MOST ENTRY OF A NODE IS THE NODE'S HIT. `hits` runs front to
        // back, and a node can be in it more than once (two boxes of one DOM
        // node: an absolutely placed span with text has two; an inline
        // element's fragments). `insert` let the LAST entry - the deepest -
        // overwrite the first: a node painted on top of another box and once
        // more under it got the depth of its lower box, and the box between
        // the two became the front-most hit. A map label's span over the
        // editor's hover highlight was `[span, highlight, span, ground]`:
        // depths span 2, highlight 1, so a press on a hovered label went to
        // the highlight and selected nothing. The point kept is the front-most
        // box's too.
        if let std::collections::btree_map::Entry::Vacant(slot) =
            hit_test.regular_hit_test_nodes.entry(*node_id)
        {
            slot.insert(HitTestItem {
                point_in_viewport: cursor_position,
                point_relative_to_item: point_relative,
                is_focusable: false,
                is_virtual_view_hit: None,
                hit_depth: depth as u32,
            });
        }
    }

    // The frames the box under the pointer is painted in (its chain) and the
    // box itself: the only containers the wheel may drive here, see below.
    let wheel_frames = tester.wheel_frames(hits.first());

    // Scroll containers: the CPU hit tester reports only regular DOM nodes,
    // so mirror the WR converter's TAG_TYPE_SCROLL_CONTAINER pass by rect
    // containment. Without this, scroll_hit_test_nodes stays empty on the
    // CPU-render path and wheel/trackpad scrolling never finds a target
    // (a11y scrolling still worked - it targets nodes directly - which is
    // how this stayed unnoticed).
    //
    // Containment tests the container's ON-SCREEN viewport box: the static
    // box from the hit tester (already placement-translated for VirtualView
    // child DOMs), shifted by the container's OWN ancestors' current scroll
    // offsets — a scroller nested in a scrolled frame moves with that frame,
    // while its own scrolling never moves its viewport. `parent_rect` /
    // `child_rect` stay in static layout coordinates: downstream only uses
    // their relative geometry (scroll ranges), which translation cannot
    // change.
    //
    // Containment alone is not enough: a container whose box holds the point
    // may be clipped away there by an ancestor, or lie under a box painted
    // over it (a `z-index` dialog over a list), and the wheel would scroll
    // what the pointer is not over. Chrome wheels the scroll chain of the
    // box under the pointer, so a container counts only if that box is
    // painted inside it (`wheel_frames`). The viewport is the end of every
    // chain - a fixed header and the page's margins scroll the page - so it
    // always counts.
    for sc in &tester.scroll_containers {
        let dom_id = &sc.dom_id;
        let node_id = sc.node_id;
        let scroll_id = sc.scroll_id;
        if !wheel_frames.contains(&(*dom_id, node_id))
            && !crate::solver3::scrollbar::is_viewport_scroller(*dom_id, node_id)
        {
            continue;
        }
        let Some(lr) = layout_results.get(dom_id) else {
            continue;
        };
        let layout_idx = sc.layout_idx;
        let p_local = tester.map_point_through_chain(
            sc.chain,
            cursor_position,
            resolve_scroll,
            resolve_transform,
        );
        let adj_x = p_local.x;
        let adj_y = p_local.y;
        {
            let node_pos = sc.rect.origin;
            let node_size = sc.rect.size;
            let inside = adj_x >= node_pos.x
                && adj_x <= node_pos.x + node_size.width
                && adj_y >= node_pos.y
                && adj_y <= node_pos.y + node_size.height;
            if !inside {
                continue;
            }
            let parent_rect = LogicalRect::new(node_pos, node_size);
            let child_rect = compute_scroll_child_rect(lr, layout_idx.index(), parent_rect);

            let scroll_node = OverflowingScrollNode {
                parent_rect,
                child_rect,
                virtual_child_rect: child_rect,
                // CPU path has no WebRender document; the pipeline half of the
                // external id is only used for WR scroll-layer sync.
                parent_external_scroll_id: azul_core::hit_test::ExternalScrollId(
                    scroll_id,
                    azul_core::hit_test::PipelineId(dom_id.inner as u32, 0),
                ),
                parent_dom_hash: azul_core::dom::DomNodeHash {
                    inner: node_id.index() as u64,
                },
                scroll_tag_id: azul_core::dom::ScrollTagId {
                    inner: azul_core::dom::TagId {
                        inner: node_id.index() as u64,
                    },
                },
            };
            hovered_nodes
                .entry(*dom_id)
                .or_insert_with(HitTest::empty)
                .scroll_hit_test_nodes
                .insert(
                    node_id,
                    ScrollHitTestItem {
                        point_in_viewport: cursor_position,
                        // Relative to the container's ON-SCREEN viewport box
                        // (static box shifted by the container's ancestors) —
                        // BORDER-box-local, unlike the regular hit item above:
                        // scroll geometry is measured against the border box.
                        point_relative_to_item: StaticLayoutPoint::new(LogicalPosition::new(
                            adj_x, adj_y,
                        ))
                        .to_border_box_local(node_pos),
                        scroll_node,
                    },
                );
        }
    }

    FullHitTest {
        hovered_nodes,
        focused_node,
    }
}

/// Compute the `child_rect` (scrollable content bounds) of an overflowing scroll
/// node from the layout tree.
///
/// The content rect is anchored at the node's own border-box origin and sized to
/// the node's overflow content size (`LayoutTree::get_content_size`, which honors
/// `overflow_content_size` / inline text overflow). It is clamped to be at least
/// as large as the viewport (`parent_rect`), so a node whose content does *not*
/// overflow yields `child_rect == parent_rect` and the `ScrollState` clamping
/// produces a zero scroll range (the prior, always-no-scroll behavior).
#[must_use]
pub fn compute_scroll_child_rect(
    layout_result: &DomLayoutResult,
    layout_idx: usize,
    parent_rect: LogicalRect,
) -> LogicalRect {
    let content_size = layout_result
        .layout_tree
        .get_content_size(LayoutNodeId::new(layout_idx));
    LogicalRect::new(
        parent_rect.origin,
        LogicalSize::new(
            content_size.width.max(parent_rect.size.width),
            content_size.height.max(parent_rect.size.height),
        ),
    )
}

/// Per layout node, whether its `overflow` clips what it contains along x and
/// along y. Read once per rebuild: the clip walk of every node up its
/// ancestors then looks no style up, where it read each ancestor's style once
/// per descendant.
fn overflow_clip_axes(styled_dom: &StyledDom, nodes: &[LayoutNodeHot]) -> Vec<(bool, bool)> {
    let styled_nodes = styled_dom.styled_nodes.as_container();
    nodes
        .iter()
        .map(|n| {
            n.dom_node_id
                .and_then(|d| styled_nodes.get(d).map(|sn| (d, &sn.styled_node_state)))
                .map_or((false, false), |(d, st)| {
                    (
                        get_overflow_x(styled_dom, d, st).is_clipped(),
                        get_overflow_y(styled_dom, d, st).is_clipped(),
                    )
                })
        })
        .collect()
}

/// A clip rect as it is stored: a non-finite edge degrades to "unclipped on
/// that side". `point_in_rect` against a NaN rect is always false, which
/// would make every node under a corrupt clip silently unhittable.
fn sanitize_clip_rect(r: LogicalRect) -> LogicalRect {
    let min_x = if r.min_x().is_finite() {
        r.min_x()
    } else {
        -CLIP_UNBOUNDED
    };
    let min_y = if r.min_y().is_finite() {
        r.min_y()
    } else {
        -CLIP_UNBOUNDED
    };
    let max_x = if r.max_x().is_finite() {
        r.max_x()
    } else {
        CLIP_UNBOUNDED
    };
    let max_y = if r.max_y().is_finite() {
        r.max_y()
    } else {
        CLIP_UNBOUNDED
    };
    LogicalRect {
        origin: LogicalPosition { x: min_x, y: min_y },
        size: LogicalSize {
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
        },
    }
}

/// Compute the hit-test clip boxes for a layout node: the enclosing
/// `VirtualView` VIEWPORTS (`dom_clips`, outermost host first) plus every
/// clipping ancestor's border box (any `overflow` other than `visible`), each
/// tagged with the chain of the clip OWNER's strict scroll ancestors so the
/// query can shift each box into its own scrolled space.
///
/// Clipping is tracked per-axis because `overflow-x` / `overflow-y` are
/// independent — an axis the ancestor does not clip is widened to
/// [`CLIP_UNBOUNDED`] (kept finite so `origin + size` arithmetic never
/// produces `inf - inf = NaN`). The ancestor box used is the border box
/// (`used_size`); CSS clips at the padding edge, but the slightly larger
/// border box is a safe over-inclusion for point hit-testing and avoids
/// resolving padding/border here.
///
/// `clips_it` says which ancestors' clips the node is painted inside - the
/// node's `ScrollChain` in production; an ancestor between an out-of-flow
/// node and its containing block clips neither its pixels nor its pointer
/// target.
#[allow(clippy::similar_names, clippy::too_many_arguments)] // domain-standard coordinate/geometry/short-lived names
fn compute_node_clips(
    clip_axes: &[(bool, bool)],
    nodes: &[LayoutNodeHot],
    positions: &PositionVec,
    node_index: usize,
    offset: LogicalPosition,
    dom_clips: &[(LogicalRect, u32)],
    chain_of: &[u32],
    clips_it: &dyn Fn(usize) -> bool,
) -> Vec<(LogicalRect, u32)> {
    let mut clips = Vec::new();
    for (dc, chain) in dom_clips {
        clips.push((sanitize_clip_rect(*dc), *chain));
    }

    // Walk ancestors. A node's own overflow clips its descendants, not itself, so
    // we start at the parent. `guard` bounds the loop in case `parent` links ever
    // form a cycle (they shouldn't, but a hit-test rebuild must never hang).
    let mut cur = nodes.get(node_index).and_then(|n| n.parent);
    let mut guard = 0usize;
    while let Some(anc) = cur {
        guard += 1;
        if guard > nodes.len() {
            break;
        }
        let Some(anc_node) = nodes.get(anc) else {
            break;
        };
        cur = anc_node.parent;

        if anc_node.dom_node_id.is_none() {
            continue;
        }
        // An ancestor whose frames the node is not painted in - one between
        // an out-of-flow node and its containing block - does not clip it.
        if !clips_it(anc) {
            continue;
        }
        let (clips_x, clips_y) = clip_axes.get(anc).copied().unwrap_or((false, false));
        if !clips_x && !clips_y {
            continue;
        }
        let (Some(pos), Some(size)) = (positions.get(anc), anc_node.used_size) else {
            continue;
        };
        let (ax0, ay0) = (pos.x + offset.x, pos.y + offset.y);
        let (min_x, max_x) = if clips_x {
            (ax0, ax0 + size.width)
        } else {
            (-CLIP_UNBOUNDED, CLIP_UNBOUNDED)
        };
        let (min_y, max_y) = if clips_y {
            (ay0, ay0 + size.height)
        } else {
            (-CLIP_UNBOUNDED, CLIP_UNBOUNDED)
        };
        clips.push((
            sanitize_clip_rect(LogicalRect {
                origin: LogicalPosition { x: min_x, y: min_y },
                size: LogicalSize {
                    width: (max_x - min_x).max(0.0),
                    height: (max_y - min_y).max(0.0),
                },
            }),
            chain_of.get(anc).copied().unwrap_or(0),
        ));
    }

    clips
}

/// Test-compat shim for the pre-scroll-aware single-rect clip API: the static
/// intersection of every clip box from [`compute_node_clips`] — exactly what
/// the query evaluates when nothing is scrolled. Kept so the generated clip
/// tests keep asserting the per-axis clip semantics they were written for.
#[cfg(test)]
fn compute_node_clip(
    styled_dom: &StyledDom,
    nodes: &[LayoutNodeHot],
    positions: &PositionVec,
    node_index: usize,
    offset: LogicalPosition,
    dom_clip: Option<LogicalRect>,
) -> Option<LogicalRect> {
    let chain_of = vec![0u32; nodes.len()];
    let clips = compute_node_clips(
        &overflow_clip_axes(styled_dom, nodes),
        nodes,
        positions,
        node_index,
        offset,
        &dom_clip.map(|r| (r, 0)).into_iter().collect::<Vec<_>>(),
        &chain_of,
        &|_| true,
    );
    if clips.is_empty() {
        return None;
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        -CLIP_UNBOUNDED,
        -CLIP_UNBOUNDED,
        CLIP_UNBOUNDED,
        CLIP_UNBOUNDED,
    );
    for (r, _) in &clips {
        min_x = min_x.max(r.min_x());
        min_y = min_y.max(r.min_y());
        max_x = max_x.min(r.max_x());
        max_y = max_y.min(r.max_y());
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
            display_list::{DisplayList, DisplayListItem, EmitPhase, WindowLogicalRect},
            layout_tree::LayoutTree,
        },
        window::DomLayoutResult,
    };
    use azul_core::geom::LogicalSize;
    use azul_css::props::basic::color::ColorU;

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
                dom_to_layout: Default::default(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            }.into(),
            calculated_positions,
            viewport: LogicalRect::zero(),
            display_list: std::sync::Arc::new(DisplayList {
                items,
                ..Default::default()
            }),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    fn virtual_view(child: usize, bounds: LogicalRect) -> DisplayListItem {
        DisplayListItem::VirtualView {
            child_dom_id: dom(child),
            bounds: WindowLogicalRect::new(bounds.origin, bounds.size),
            clip_rect: WindowLogicalRect::new(bounds.origin, bounds.size),
            content_offset: Default::default(),
        }
    }

    /// A `VirtualView` whose materialized window is offset from the viewport —
    /// what a scrolled virtual document produces
    /// (`materialized_origin - scroll_offset`).
    fn virtual_view_scrolled(
        child: usize,
        bounds: LogicalRect,
        content_offset: LogicalPosition,
    ) -> DisplayListItem {
        DisplayListItem::VirtualView {
            child_dom_id: dom(child),
            bounds: WindowLogicalRect::new(bounds.origin, bounds.size),
            clip_rect: WindowLogicalRect::new(bounds.origin, bounds.size),
            content_offset,
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
    fn a_child_dom_on_top_of_its_host_comes_first_and_gets_the_lower_depth() {
        // Host dom 0: one node covering (0,0)-(300,300), showing child dom 1
        // through a VirtualView at (100,100)-(150,150). Child dom 1: one node
        // covering its whole 200x200. A point inside the page is over BOTH.
        //
        // WebRender answers page first (front to back, one scene for every
        // DOM). The CPU tester visited DOMs in ascending id order, so the
        // HOST came first and `FullHitTest::topmost_node` - the smallest
        // `hit_depth` - named the node the page was covering.
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout_result(
                styled(""),
                vec![hot(Some(1), Some((300.0, 300.0)), None)],
                vec![p(0.0, 0.0)],
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

        let hits = tester.hit_test_scrolled(p(120.0, 120.0), &|_, _| None, &|_, _| None);
        let order: Vec<(DomId, NodeId)> = hits.iter().map(|(d, n, _)| (*d, *n)).collect();
        assert_eq!(
            order,
            vec![(dom(1), NodeId::new(1)), (dom(0), NodeId::new(1))],
            "the page over the host is nearer the user"
        );

        let full = convert_cpu_hit_test_to_full(
            &tester,
            &hits,
            None,
            &results,
            p(120.0, 120.0),
            &|_, _| None,
            &|_, _| None,
        );
        let depth = |d: usize| {
            full.hovered_nodes[&dom(d)].regular_hit_test_nodes[&NodeId::new(1)].hit_depth
        };
        assert_eq!(depth(1), 0, "front-most");
        assert_eq!(depth(0), 1);
        let top = full.topmost_node().expect("something is hit");
        assert_eq!(
            top.dom,
            dom(1),
            "topmost_node agrees with WebRender's rule now"
        );
        // Outside the page, only the host - and it is then the front-most.
        let outside = tester.hit_test(p(20.0, 20.0));
        assert_eq!(outside, vec![(dom(0), NodeId::new(1))]);
    }

    /// A node that is hit twice (two boxes of one DOM node) keeps the depth of
    /// its FRONT-MOST box. The list is the one the map editor's view answered
    /// at a press on a hovered label, `[(1, 26), (1, 13), (1, 26), (1, 1)]`:
    /// the label's span (26), the hover highlight under it (13), the span's
    /// second box, the ground (1). Mutation: `insert` instead of the vacant
    /// entry gives the span depth 2 and the highlight is the front-most hit.
    #[test]
    fn a_node_hit_twice_keeps_the_depth_of_its_front_most_box() {
        let tester = CpuHitTester::new();
        let results = BTreeMap::new();
        let at = p(700.0, 450.0);
        let (span, highlight, ground) = (NodeId::new(26), NodeId::new(13), NodeId::new(1));
        let hits = vec![
            (dom(1), span, p(3.0, 4.0)),
            (dom(1), highlight, p(0.5, 0.5)),
            (dom(1), span, p(3.0, 4.0)),
            (dom(1), ground, p(956.0, 706.0)),
        ];
        let full = convert_cpu_hit_test_to_full(
            &tester,
            &hits,
            None,
            &results,
            at,
            &|_, _| None,
            &|_, _| None,
        );
        let nodes = &full.hovered_nodes[&dom(1)].regular_hit_test_nodes;
        assert_eq!(nodes.len(), 3, "each node once");
        assert_eq!(nodes[&span].hit_depth, 0, "the span's front-most box");
        assert_eq!(nodes[&highlight].hit_depth, 1);
        assert_eq!(nodes[&ground].hit_depth, 3, "depths are places in the list");
        let front = |node: Option<azul_core::dom::DomNodeId>| {
            node.map(|n| (n.dom, n.node.into_crate_internal()))
        };
        assert_eq!(
            front(crate::managers::hover::deepest_node_across_doms(&full)),
            Some((dom(1), Some(span))),
            "the press goes to the span, not to the box under it"
        );
        assert_eq!(front(full.topmost_node()), Some((dom(1), Some(span))));
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
    fn a_box_painted_over_a_virtual_view_is_hit_before_the_page_under_it() {
        // Host dom 0: the root (0,0)-(300,300) painted first (item 0), a
        // VirtualView showing child dom 1 over (0,0)-(300,300) (item 1), and
        // an overlay - a zoom button over a map - at (200,200)-(240,240)
        // painted AFTER it (item 2). A child dom is painted at its
        // VirtualView item, not over the whole host dom: the overlay is on
        // top, so it is hit first, then the page, then the root. Every
        // child dom used to come first (AzMaps: a click on "+" dropped a
        // pin on the map under it).
        let rect = |x: f32, y: f32, w: f32, h: f32| DisplayListItem::Rect {
            bounds: WindowLogicalRect::new(p(x, y), LogicalSize { width: w, height: h }),
            color: ColorU::BLACK,
            border_radius: Default::default(),
        };
        let mut host = layout_result(
            styled(""),
            vec![
                hot(Some(0), Some((300.0, 300.0)), None),
                hot(Some(1), Some((40.0, 40.0)), Some(0)),
            ],
            vec![p(0.0, 0.0), p(200.0, 200.0)],
            Vec::new(),
        );
        host.display_list = std::sync::Arc::new(DisplayList {
            items: vec![
                rect(0.0, 0.0, 300.0, 300.0),
                virtual_view(1, r(0.0, 0.0, 300.0, 300.0)),
                rect(200.0, 200.0, 40.0, 40.0),
            ],
            layout_node_mapping: vec![
                Some((0, EmitPhase::BgBorder)),
                Some((0, EmitPhase::Content)),
                Some((1, EmitPhase::BgBorder)),
            ],
            ..Default::default()
        });
        let mut results = BTreeMap::new();
        results.insert(dom(0), host);
        results.insert(
            dom(1),
            layout_result(
                styled(""),
                vec![hot(Some(1), Some((300.0, 300.0)), None)],
                vec![p(0.0, 0.0)],
                Vec::new(),
            ),
        );
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);

        assert_eq!(
            tester.hit_test(p(220.0, 220.0)),
            vec![(dom(0), NodeId::new(1)), (dom(1), NodeId::new(1)), (dom(0), NodeId::new(0))],
            "the overlay, then the page it covers, then the host's root",
        );
        assert_eq!(
            tester.hit_test(p(20.0, 20.0)),
            vec![(dom(1), NodeId::new(1)), (dom(0), NodeId::new(0))],
            "away from the overlay the page is on top of its host",
        );
    }

    #[test]
    fn a_view_host_that_paints_after_its_page_still_stays_under_it() {
        // The host of a VirtualView whose own first item comes AFTER the
        // view's (a border or scrollbar drawn over the content) ranked over
        // its page, and so did every wrapper ranked at it: AzMonitor's
        // process table (a page in the host's scroll box) took no click -
        // the header's sort never ran. A host CONTAINS its page, as a parent
        // contains its children: it and its ancestors are under the page;
        // rank decides only against the host's other boxes.
        let rect = |x: f32, y: f32, w: f32, h: f32| DisplayListItem::Rect {
            bounds: WindowLogicalRect::new(p(x, y), LogicalSize { width: w, height: h }),
            color: ColorU::BLACK,
            border_radius: Default::default(),
        };
        let mut host = layout_result(
            styled(""),
            vec![
                hot(Some(0), Some((300.0, 300.0)), None),
                hot(Some(1), Some((300.0, 300.0)), Some(0)),
            ],
            vec![p(0.0, 0.0), p(0.0, 0.0)],
            Vec::new(),
        );
        host.display_list = std::sync::Arc::new(DisplayList {
            items: vec![
                virtual_view(1, r(0.0, 0.0, 300.0, 300.0)),
                rect(0.0, 0.0, 300.0, 300.0),
            ],
            // The view's item is its host's (display_list.rs
            // `push_virtual_view_placeholder`); the host's other item comes
            // after it.
            layout_node_mapping: vec![Some((1, EmitPhase::Content)), Some((1, EmitPhase::ScWalk))],
            node_mapping: vec![Some(NodeId::new(1)), Some(NodeId::new(1))],
            ..Default::default()
        });
        let mut results = BTreeMap::new();
        results.insert(dom(0), host);
        results.insert(
            dom(1),
            layout_result(
                styled(""),
                vec![hot(Some(1), Some((300.0, 300.0)), None)],
                vec![p(0.0, 0.0)],
                Vec::new(),
            ),
        );
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);
        assert_eq!(
            tester.hit_test(p(20.0, 20.0)).first(),
            Some(&(dom(1), NodeId::new(1))),
            "the page, not its host or the host's wrappers",
        );
    }

    #[test]
    fn a_view_host_that_paints_nothing_of_its_own_stays_under_its_page() {
        // The host of a VirtualView that paints nothing but the view - and
        // the wrapper around it - rank AT the view's item (a node's rank is
        // its first painted item, or its first painted descendant's): they
        // are under the page they show. Ranked over it, the host took every
        // click meant for the page (AzMonitor's table headers stopped
        // sorting).
        let mut host = layout_result(
            styled(""),
            vec![
                hot(Some(0), Some((300.0, 300.0)), None),
                hot(Some(1), Some((300.0, 300.0)), Some(0)),
            ],
            vec![p(0.0, 0.0), p(0.0, 0.0)],
            Vec::new(),
        );
        host.display_list = std::sync::Arc::new(DisplayList {
            items: vec![virtual_view(1, r(0.0, 0.0, 300.0, 300.0))],
            layout_node_mapping: vec![Some((1, EmitPhase::Content))],
            ..Default::default()
        });
        let mut results = BTreeMap::new();
        results.insert(dom(0), host);
        results.insert(
            dom(1),
            layout_result(
                styled(""),
                vec![hot(Some(1), Some((300.0, 300.0)), None)],
                vec![p(0.0, 0.0)],
                Vec::new(),
            ),
        );
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);
        assert_eq!(
            tester.hit_test(p(20.0, 20.0)).first(),
            Some(&(dom(1), NodeId::new(1))),
            "the page, not its host",
        );
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
    fn a_scrolled_virtual_views_clip_stays_on_the_viewport_it_does_not_travel_with_the_content() {
        // Host dom 0 shows child dom 1 through a 50x50 VIEWPORT at (100,100).
        // The child is scrolled: `content_offset` is (0,-80), so the renderer
        // draws the child's local (0,0) at (100,20) — 80px of it above the
        // viewport, the rest visible from y=100 down (cpurender/raster.rs
        // pushes `-vv_origin - content_offset` and clips to `bounds`).
        //
        // The hit test used the SHIFTED rect as the clip too, which moves the
        // window the user is looking through. Both directions are wrong and
        // both are asserted here.
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout_result(
                styled(""),
                Vec::new(),
                Vec::new(),
                vec![virtual_view_scrolled(
                    1,
                    r(100.0, 100.0, 50.0, 50.0),
                    p(0.0, -80.0),
                )],
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

        assert_eq!(
            tester.hit_test(p(120.0, 120.0)),
            vec![(dom(1), NodeId::new(1))],
            "a point inside BOTH the viewport and the drawn child must hit it — clipping to \
             `bounds.origin + content_offset` moved the viewport 80px up and rejected it, which \
             is a page canvas that goes dead as soon as you scroll"
        );
        assert!(
            tester.hit_test(p(120.0, 60.0)).is_empty(),
            "the part of the child scrolled ABOVE the viewport is not visible, so it must not \
             take the click either"
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
        // overflow-x: clip / overflow-y: visible — the y axis must stay
        // unbounded (finite stand-in), not collapse onto the ancestor's box.
        // `clip` is CSS's one-axis clip: a lone `hidden` would make the other
        // axis compute to `auto` (CSS Overflow 3 §3.1) and clip it too - see
        // `compute_node_clip_hidden_on_one_axis_clips_both`.
        let styled_dom = styled("div.clip { overflow-x: clip; }");
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
    fn compute_node_clip_hidden_on_one_axis_clips_both() {
        // CSS Overflow 3 §3.1: `overflow-x: hidden` makes the unset
        // `overflow-y` compute to `auto`, so the box is a scroll container on
        // BOTH axes and clips vertically as well.
        let styled_dom = styled("div.clip { overflow-x: hidden; }");
        let nodes = vec![
            hot(Some(0), Some((500.0, 500.0)), None),
            hot(Some(1), Some((100.0, 50.0)), Some(0)),
            hot(Some(2), Some((400.0, 400.0)), Some(1)),
        ];
        let positions: PositionVec = vec![p(0.0, 0.0), p(10.0, 10.0), p(10.0, 10.0)];

        let clip = compute_node_clip(&styled_dom, &nodes, &positions, 2, p(0.0, 0.0), None)
            .expect("overflow-x: hidden clips");

        assert_eq!(clip.origin.x, 10.0);
        assert_eq!(clip.size.width, 100.0);
        assert_eq!(clip.origin.y, 10.0);
        assert_eq!(clip.size.height, 50.0);
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

    // -----------------------------------------------------------------------
    // convert_cpu_hit_test_to_full — the CPU half of the ONE documented space
    // -----------------------------------------------------------------------

    /// `HitTestItem::point_relative_to_item` is CONTENT-box-relative on BOTH
    /// hosts. This pins the CPU half; the WebRender half applies the same
    /// `LayoutTree::content_inset` in `wr_translate2::fullhittest_new_webrender`.
    ///
    /// Before this, WebRender reported the point relative to the hit RECT
    /// (border box) while the CPU tester subtracted padding+border, so the
    /// SAME click resolved to different characters in headless E2E and in
    /// production — latent only because the default TextInput's value <p> has
    /// neither padding nor border.
    #[test]
    fn convert_cpu_hit_test_emits_content_box_relative_points() {
        use crate::solver3::geometry::{EdgeSizes, PackedBoxProps, ResolvedBoxProps};

        let inset = |left: f32, top: f32| {
            PackedBoxProps::pack(&ResolvedBoxProps {
                padding: EdgeSizes {
                    top,
                    right: 0.0,
                    bottom: 0.0,
                    left,
                },
                border: EdgeSizes {
                    top: 1.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 1.0,
                },
                ..Default::default()
            })
        };

        let styled_dom = styled("");
        let mut node = hot(Some(0), Some((100.0, 40.0)), None);
        node.box_props = inset(5.0, 2.0); // + 1px border on each of left/top
        let mut lr = layout_result(styled_dom, vec![node], vec![p(10.0, 20.0)], Vec::new());
        lr.layout_tree_mut()
            .dom_to_layout
            .insert(NodeId::new(0), vec![LayoutNodeId::new(0)]);

        let mut layout_results = BTreeMap::new();
        layout_results.insert(dom(0), lr);

        // The hit tester reports the point in STATIC layout space; the node's
        // border box starts at (10, 20) and its content box 6/3 further in.
        let hits = vec![(dom(0), NodeId::new(0), p(40.0, 50.0))];
        let full = convert_cpu_hit_test_to_full(
            &CpuHitTester::new(),
            &hits,
            None,
            &layout_results,
            p(40.0, 50.0),
            &|_, _| None,
            &|_, _| None,
        );

        let item = full.hovered_nodes[&dom(0)].regular_hit_test_nodes[&NodeId::new(0)];
        assert_eq!(
            item.point_relative_to_item.get(),
            p(40.0 - 10.0 - 6.0, 50.0 - 20.0 - 3.0),
            "content-box-relative: static point minus border-box origin minus content inset"
        );
        // ...and the round trip back out to the border box, which is what
        // `get_cursor_relative_to_node` hands widgets, is exact.
        assert_eq!(
            item.point_relative_to_item
                .to_border_box_local(azul_core::spaces::ContentInset::new(6.0, 3.0))
                .get(),
            p(30.0, 30.0),
        );
    }

    /// A node with no padding and no border: the two boxes coincide, which is
    /// why the divergence above stayed invisible in the default widget set.
    #[test]
    fn convert_cpu_hit_test_on_an_unpadded_node_is_plain_border_box_local() {
        let styled_dom = styled("");
        let mut lr = layout_result(
            styled_dom,
            vec![hot(Some(0), Some((100.0, 40.0)), None)],
            vec![p(10.0, 20.0)],
            Vec::new(),
        );
        lr.layout_tree_mut()
            .dom_to_layout
            .insert(NodeId::new(0), vec![LayoutNodeId::new(0)]);
        let mut layout_results = BTreeMap::new();
        layout_results.insert(dom(0), lr);

        let hits = vec![(dom(0), NodeId::new(0), p(40.0, 50.0))];
        let full = convert_cpu_hit_test_to_full(
            &CpuHitTester::new(),
            &hits,
            None,
            &layout_results,
            p(40.0, 50.0),
            &|_, _| None,
            &|_, _| None,
        );
        assert_eq!(
            full.hovered_nodes[&dom(0)].regular_hit_test_nodes[&NodeId::new(0)]
                .point_relative_to_item
                .get(),
            p(30.0, 30.0),
        );
    }
}

/// Guards of audit area A6 (`audit/A6.md`): a box is hit where it is drawn
/// under any reference frame, in a view that moved, and a query does not walk
/// every box.
#[cfg(test)]
#[allow(clippy::float_cmp)] // the fixtures use numbers f32 holds exactly
#[allow(clippy::cast_precision_loss)] // small fixture counts
mod audit_a6_tests {
    use std::collections::HashMap;

    use azul_core::{
        dom::{Dom, FormattingContext},
        resources::TransformKey,
        transform::ComputedTransform3D,
    };

    use super::*;
    use crate::{
        managers::gpu_state::GpuStateManager,
        solver3::{
            display_list::{DisplayList, DisplayListItem, WindowLogicalRect},
            layout_tree::LayoutTree,
        },
    };

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

    /// A box of DOM node `node`, `width` x `height`, under layout node `parent`.
    fn hot(node: usize, size: (f32, f32), parent: Option<usize>) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: Default::default(),
            dom_node_id: Some(NodeId::new(node)),
            used_size: Some(LogicalSize {
                width: size.0,
                height: size.1,
            }),
            formatting_context: FormattingContext::default(),
            parent,
        }
    }

    /// `body` (node 0) with `children` plain `div`s (nodes 1..=children).
    fn styled(children: usize) -> StyledDom {
        let css = azul_css::parser2::new_from_str("").0;
        let divs: Vec<Dom> = (0..children).map(|_| Dom::create_div()).collect();
        let mut d = Dom::create_body().with_children(divs.into());
        StyledDom::create(&mut d, css)
    }

    fn layout(
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
                dom_to_layout: Default::default(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            }
            .into(),
            calculated_positions,
            viewport: LogicalRect::zero(),
            display_list: std::sync::Arc::new(DisplayList {
                items,
                ..Default::default()
            }),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    fn view(child: usize, bounds: LogicalRect, content_offset: LogicalPosition) -> DisplayListItem {
        DisplayListItem::VirtualView {
            child_dom_id: dom(child),
            bounds: WindowLogicalRect::new(bounds.origin, bounds.size),
            clip_rect: WindowLogicalRect::new(bounds.origin, bounds.size),
            content_offset,
        }
    }

    /// `scale(k)` then `translate(tx, ty)`, about the box's corner: the
    /// editor's zoom of its picture root (`transform-origin: 0 0`).
    fn scaled(k: f32, tx: f32, ty: f32) -> ComputedTransform3D {
        ComputedTransform3D {
            m: [
                [k, 0.0, 0.0, 0.0],
                [0.0, k, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [tx, ty, 0.0, 1.0],
            ],
        }
    }

    /// A turn by `degrees` about the point `(cx, cy)` of the box (its
    /// `transform-origin`), row-vector: `x' = cx + (x - cx) cos - (y - cy) sin`,
    /// `y' = cy + (x - cx) sin + (y - cy) cos`.
    fn turned(degrees: f32, cx: f32, cy: f32) -> ComputedTransform3D {
        let (s, c) = degrees.to_radians().sin_cos();
        ComputedTransform3D {
            m: [
                [c, s, 0.0, 0.0],
                [-s, c, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [cx - (cx * c - cy * s), cy - (cx * s + cy * c), 0.0, 1.0],
            ],
        }
    }

    /// Gives `node` of `dom_id` a CSS reference frame with matrix `m`.
    fn set_frame(gpu: &mut GpuStateManager, dom_id: DomId, node: usize, m: ComputedTransform3D) {
        let cache = gpu.get_or_create_cache(dom_id);
        cache
            .css_transform_keys
            .entry(NodeId::new(node))
            .or_insert_with(TransformKey::unique);
        cache
            .css_current_transform_values
            .insert(NodeId::new(node), m);
    }

    fn nodes_at(
        tester: &CpuHitTester,
        gpu: &GpuStateManager,
        scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        x: f32,
        y: f32,
    ) -> Vec<(DomId, NodeId, LogicalPosition)> {
        tester.hit_test_scrolled(p(x, y), scroll, &|d, n| gpu.painted_transform_of(d, n))
    }

    fn hits_node(hits: &[(DomId, NodeId, LogicalPosition)], dom_id: DomId, node: usize) -> bool {
        hits.iter()
            .any(|(d, n, _)| *d == dom_id && *n == NodeId::new(node))
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1.0e-3 && (a.1 - b.1).abs() < 1.0e-3
    }

    // A6-01: a frame's matrix acts about its own box's corner.

    /// The example of A2-cpu-03: a label box at (300, 200), 100 x 12,
    /// `transform-origin: 0 12px; transform: rotate(90deg)`. Its point
    /// (350, 206) - 50 along, 6 down - is painted at
    /// `(300, 200) + ((0, 12) + R90 (50, -6)) = (300, 200) + (6, 62) = (306, 262)`.
    /// Mutation: `- ox` / `+ ox` dropped in `frame_to_local` (the matrix
    /// about the window's corner, as it was) answers (250, -294) and fails.
    #[test]
    fn a_frame_is_painted_and_hit_about_its_own_corner() {
        let m = turned(90.0, 0.0, 12.0);
        let corner = p(300.0, 200.0);
        let painted = frame_to_screen(&m, corner, (350.0, 206.0)).expect("drawn");
        assert!(close(painted, (306.0, 262.0)), "painted at {painted:?}");
        let back = frame_to_local(&m, corner, (306.0, 262.0)).expect("invertible");
        assert!(close(back, (350.0, 206.0)), "hit at {back:?}");
        // A translation needs no corner, and stays exact.
        let slide = scaled(1.0, 120.0, 80.0);
        assert_eq!(frame_to_local(&slide, corner, (160.0, 105.0)), Some((40.0, 25.0)));
        assert_eq!(frame_to_screen(&slide, corner, (40.0, 25.0)), Some((160.0, 105.0)));
    }

    /// A zero scale draws the frame as a point: nothing in it is hit.
    /// Mutation: the determinant test removed answers a point at infinity or
    /// the origin.
    #[test]
    fn a_frame_that_cannot_be_inverted_is_hit_nowhere() {
        let flat = scaled(0.0, 5.0, 5.0);
        assert_eq!(frame_to_local(&flat, p(10.0, 10.0), (15.0, 15.0)), None);

        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout(
                styled(1),
                vec![hot(0, (400.0, 300.0), None), hot(1, (50.0, 50.0), Some(0))],
                vec![p(0.0, 0.0), p(10.0, 10.0)],
                Vec::new(),
            ),
        );
        let mut gpu = GpuStateManager::default();
        set_frame(&mut gpu, dom(0), 1, flat);
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));
        for (x, y) in [(15.0, 15.0), (10.0, 10.0), (30.0, 30.0), (0.0, 0.0)] {
            let hits = nodes_at(&tester, &gpu, &|_, _| None, x, y);
            assert!(!hits_node(&hits, dom(0), 1), "({x}, {y}) hit the collapsed box");
            assert!(hits_node(&hits, dom(0), 0), "({x}, {y}) still hits the body");
        }
    }

    /// A6-24 (A2-cpu-R09). A reference frame whose key has no live value is painted with the
    /// matrix its list was built with (`PushReferenceFrame::initial_transform`: the rasteriser's
    /// `transforms.get(key).map_or(&initial_transform.m, ..)`), and was hit as the identity:
    /// the frames between the end of an animation, which drops key and value, and the list
    /// that is built without the frame.
    ///
    /// By hand. The frame's corner is (100, 100); its list bakes "scale by 2, then move by
    /// (40, 0)": a point `p` of the content is painted at `(100, 100) + 2 (p - (100, 100)) +
    /// (40, 0)`. The box, 50 x 50 at (100, 100), is painted at (140, 100), 100 x 100.
    /// - the window point (230, 190) is the box's (100 + (230 - 140) / 2, 100 + (190 - 100) / 2)
    ///   = (145, 145): inside, a hit. As the identity it was (230, 190): a miss.
    /// - the window point (120, 120) is the box's (100 + (120 - 140) / 2, 110) = (90, 110): left
    ///   of the box, no hit. As the identity it was (120, 120): a hit on a box drawn elsewhere.
    ///
    /// Mutation: `painted_or_baked` answering `live` alone fails both points.
    #[test]
    fn a_frame_whose_key_lost_its_value_is_hit_where_its_list_paints_it() {
        use azul_core::gpu::TransformChannel;

        let baked = scaled(2.0, 40.0, 0.0);
        let key = TransformKey::unique();
        let node = NodeId::new(1);
        let mut result = layout(
            styled(1),
            vec![hot(0, (400.0, 300.0), None), hot(1, (50.0, 50.0), Some(0))],
            vec![p(0.0, 0.0), p(100.0, 100.0)],
            vec![
                DisplayListItem::PushReferenceFrame {
                    transform_key: key,
                    initial_transform: baked,
                    bounds: WindowLogicalRect::new(p(100.0, 100.0), LogicalSize::new(50.0, 50.0)),
                },
                DisplayListItem::PopReferenceFrame,
            ],
        );
        // The frame is the box's own.
        std::sync::Arc::get_mut(&mut result.display_list)
            .expect("the list has one holder")
            .node_mapping = vec![Some(node), None];
        let mut results = BTreeMap::new();
        results.insert(dom(0), result);

        // The list was built while a slide moved the box: its key has a value.
        let mut gpu = GpuStateManager::default();
        let _ = gpu
            .get_or_create_cache(dom(0))
            .set_transform(TransformChannel::Animation, node, key, baked);
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));
        let hits = nodes_at(&tester, &gpu, &|_, _| None, 230.0, 190.0);
        assert!(hits_node(&hits, dom(0), 1), "harness: with its live value the box is hit there");

        // The slide ends: key and value go. The list is still the one that was built.
        let _ = gpu
            .get_or_create_cache(dom(0))
            .remove_transform(TransformChannel::Animation, node);
        assert_eq!(gpu.painted_transform_of(dom(0), node), None, "harness: no live value");

        let hits = nodes_at(&tester, &gpu, &|_, _| None, 230.0, 190.0);
        assert!(hits_node(&hits, dom(0), 1), "(230, 190) is (145, 145) of the box: a hit");
        let hits = nodes_at(&tester, &gpu, &|_, _| None, 120.0, 120.0);
        assert!(!hits_node(&hits, dom(0), 1), "(120, 120) is (90, 110): left of the box");
        assert!(hits_node(&hits, dom(0), 0), "and the body is under it");

        // A tester built again in that state has no frame for the node any more (the cache
        // has none) and no stale matrix either: the table is the rebuild's.
        tester.rebuild_from_layout_with_gpu(&BTreeMap::new(), Some(&gpu));
        assert!(tester.baked_frames.is_empty());
    }

    /// The editor's zoom, by hand. A view at the window's corner shows its
    /// child with `content_offset` (-256, -256). The picture root (child
    /// (0, 0)) is scaled 1.5 about the view point (615, 381): `scale(1.5)`
    /// then `translate(-307.5, -190.5)`. A label box at child (700, 300),
    /// 80 x 16, is turned 30 degrees about its centre.
    ///
    /// Its centre, child (740, 308), is painted at
    /// `1.5 (740, 308) + (-307.5, -190.5) = (802.5, 271.5)` in the child, so
    /// at window `(802.5 - 256, 271.5 - 256) = (546.5, 15.5)`. Its far corner
    /// is `1.5 R30 (40, 8) = (45.96, 40.39)` from there: (592.46, 55.89).
    ///
    /// Before: the point was taken through `M_root^-1` and `M_label^-1` about
    /// the WINDOW's corner and compared with the box at (444, 44): a press on
    /// the painted label missed it, and one at (484, 52) - where nothing of
    /// it is painted - could not be told from it.
    /// Mutation: the corner dropped from either frame, or the two links
    /// swapped, fails every assertion on node 1.
    #[test]
    fn a_turned_label_in_a_zoomed_and_panned_view_is_hit_where_it_is_drawn() {
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout(
                styled(0),
                Vec::new(),
                Vec::new(),
                vec![view(1, r(0.0, 0.0, 1280.0, 800.0), p(-256.0, -256.0))],
            ),
        );
        results.insert(
            dom(1),
            layout(
                styled(1),
                vec![hot(0, (2000.0, 1500.0), None), hot(1, (80.0, 16.0), Some(0))],
                vec![p(0.0, 0.0), p(700.0, 300.0)],
                Vec::new(),
            ),
        );
        let mut gpu = GpuStateManager::default();
        set_frame(&mut gpu, dom(1), 0, scaled(1.5, -307.5, -190.5));
        set_frame(&mut gpu, dom(1), 1, turned(30.0, 40.0, 8.0));
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));

        // The centre, and the point in the label's static space it maps to:
        // the box's centre where the tester keeps it, (700 - 256 + 40, 300 - 256 + 8).
        let hits = nodes_at(&tester, &gpu, &|_, _| None, 546.5, 15.5);
        let centre = hits
            .iter()
            .find(|(d, n, _)| *d == dom(1) && *n == NodeId::new(1))
            .expect("a press on the painted centre hits the label");
        assert!(
            (centre.2.x - 484.0).abs() < 1.0e-2 && (centre.2.y - 52.0).abs() < 1.0e-2,
            "the centre maps to the box's centre, got {:?}",
            centre.2
        );
        // The label is the front-most hit: it is painted over the root.
        assert_eq!((hits[0].0, hits[0].1), (dom(1), NodeId::new(1)));

        // 3 px inside the far corner along the diagonal, and 3 px outside.
        let inside = nodes_at(&tester, &gpu, &|_, _| None, 590.21, 53.91);
        assert!(hits_node(&inside, dom(1), 1), "just inside the turned corner");
        let outside = nodes_at(&tester, &gpu, &|_, _| None, 594.71, 57.87);
        assert!(!hits_node(&outside, dom(1), 1), "just outside the turned corner");
        assert!(hits_node(&outside, dom(1), 0), "the picture root is still there");

        // Where layout put the box (plus the view's offset): nothing of the
        // label is painted there now.
        let stale = nodes_at(&tester, &gpu, &|_, _| None, 484.0, 52.0);
        assert!(!hits_node(&stale, dom(1), 1), "the label is not at its layout place");
    }

    // A6-02: the links of a chain are applied where they nest.

    /// A scroll frame AROUND a scaled box moves it by the plain offset.
    /// body(0) > scroller(1, scrolled by (0, 30)) > box(2) at (100, 100),
    /// 50 x 50, `scale(2)` about its corner: painted at
    /// `(100, 100) - (0, 30) = (100, 70)`, 100 x 100.
    /// Before (`T^-1(p) + scroll`): (190, 160) mapped to (95, 110), a miss,
    /// and (250, 150) - 50 px right of the painted box - to (125, 105), a hit.
    /// Mutation: the links sorted by kind (every frame, then every scroll
    /// frame) restores both.
    #[test]
    fn a_scroll_frame_around_a_scaled_box_moves_it_unscaled() {
        let mut lr = layout(
            styled(2),
            vec![
                hot(0, (600.0, 600.0), None),
                hot(1, (400.0, 400.0), Some(0)),
                hot(2, (50.0, 50.0), Some(1)),
            ],
            vec![p(0.0, 0.0), p(0.0, 0.0), p(100.0, 100.0)],
            Vec::new(),
        );
        lr.scroll_ids.insert(LayoutNodeId::new(1), 7);
        let mut results = BTreeMap::new();
        results.insert(dom(0), lr);
        let mut gpu = GpuStateManager::default();
        set_frame(&mut gpu, dom(0), 2, scaled(2.0, 0.0, 0.0));
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));
        let scroll =
            |_: DomId, n: NodeId| (n == NodeId::new(1)).then_some(p(0.0, 30.0));

        let hits = nodes_at(&tester, &gpu, &scroll, 190.0, 160.0);
        let hit = hits
            .iter()
            .find(|(_, n, _)| *n == NodeId::new(2))
            .expect("(190, 160) is inside the painted box (100, 70) 100 x 100");
        assert_eq!(hit.2, p(145.0, 145.0), "(190, 190) unscrolled, halved about (100, 100)");
        assert!(!hits_node(&nodes_at(&tester, &gpu, &scroll, 250.0, 150.0), dom(0), 2));
        assert!(!hits_node(&nodes_at(&tester, &gpu, &scroll, 150.0, 60.0), dom(0), 2));
        assert!(hits_node(&nodes_at(&tester, &gpu, &scroll, 199.0, 169.0), dom(0), 2));
    }

    /// A scroll frame INSIDE a scaled box moves its content by the scaled
    /// offset. body(0) > box(1) `scale(2)` about (0, 0) > scroller(2,
    /// scrolled by (0, 30)) > leaf(3) at (10, 60), 20 x 20: painted at
    /// `2 ((10, 60) - (0, 30)) = (20, 60)`, 40 x 40.
    /// Mutation: the scroll frame applied outside the frame (the order
    /// reversed) maps (30, 70) to (15, 50): a miss.
    #[test]
    fn a_scroll_frame_inside_a_scaled_box_is_scaled_with_it() {
        let mut lr = layout(
            styled(3),
            vec![
                hot(0, (600.0, 600.0), None),
                hot(1, (300.0, 300.0), Some(0)),
                hot(2, (200.0, 200.0), Some(1)),
                hot(3, (20.0, 20.0), Some(2)),
            ],
            vec![p(0.0, 0.0), p(0.0, 0.0), p(0.0, 0.0), p(10.0, 60.0)],
            Vec::new(),
        );
        lr.scroll_ids.insert(LayoutNodeId::new(2), 9);
        let mut results = BTreeMap::new();
        results.insert(dom(0), lr);
        let mut gpu = GpuStateManager::default();
        set_frame(&mut gpu, dom(0), 1, scaled(2.0, 0.0, 0.0));
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));
        let scroll =
            |_: DomId, n: NodeId| (n == NodeId::new(2)).then_some(p(0.0, 30.0));

        let hits = nodes_at(&tester, &gpu, &scroll, 30.0, 70.0);
        let hit = hits
            .iter()
            .find(|(_, n, _)| *n == NodeId::new(3))
            .expect("(30, 70) is inside the painted leaf (20, 60) 40 x 40");
        assert_eq!(hit.2, p(15.0, 65.0));
        assert!(!hits_node(&nodes_at(&tester, &gpu, &scroll, 30.0, 105.0), dom(0), 3));
        assert!(!hits_node(&nodes_at(&tester, &gpu, &scroll, 15.0, 70.0), dom(0), 3));
    }

    /// `node_rect_to_screen` paints by the same rule: the box of the first
    /// test above, asked for its child's rect.
    #[test]
    fn the_forward_map_nests_the_frames_the_same_way() {
        let mut lr = layout(
            styled(3),
            vec![
                hot(0, (600.0, 600.0), None),
                hot(1, (400.0, 400.0), Some(0)),
                hot(2, (50.0, 50.0), Some(1)),
                hot(3, (10.0, 10.0), Some(2)),
            ],
            vec![p(0.0, 0.0), p(0.0, 0.0), p(100.0, 100.0), p(110.0, 120.0)],
            Vec::new(),
        );
        lr.scroll_ids.insert(LayoutNodeId::new(1), 7);
        let frame = scaled(2.0, 0.0, 0.0);
        let on_screen = node_rect_to_screen(
            &lr,
            dom(0),
            3,
            r(110.0, 120.0, 10.0, 10.0),
            &|_, n| (n == NodeId::new(1)).then_some(p(0.0, 30.0)),
            &|_, n| (n == NodeId::new(2)).then_some(frame),
        );
        // (100, 100) + 2 ((110, 120) - (100, 100)) - (0, 30) = (120, 110), 20 x 20.
        assert_eq!(on_screen, r(120.0, 110.0, 20.0, 20.0));
    }

    /// A6-06: a node's own reference frame moves its own box. A box at
    /// (100, 100), 50 x 50, `scale(2)` about its corner is painted at
    /// (100, 100), 100 x 100 - which is where the hit tester has it - and
    /// `node_rect_to_screen` answered its layout box (100, 100) 50 x 50
    /// (it took the frames of the node's ancestors only): the accessibility
    /// bounds, a menu's anchor and a scripted click aimed at the node's
    /// bounds disagreed with the pixels.
    /// Mutation: the frames loop skipping the node itself for a box
    /// (`.skip(1)`) answers 50 x 50.
    #[test]
    fn a_nodes_own_frame_moves_its_own_box() {
        let lr = layout(
            styled(1),
            vec![hot(0, (600.0, 600.0), None), hot(1, (50.0, 50.0), Some(0))],
            vec![p(0.0, 0.0), p(100.0, 100.0)],
            Vec::new(),
        );
        let frame = scaled(2.0, 10.0, 0.0);
        let own = |_: DomId, n: NodeId| (n == NodeId::new(1)).then_some(frame);
        let on_screen =
            node_rect_to_screen(&lr, dom(0), 1, r(100.0, 100.0, 50.0, 50.0), &|_, _| None, &own);
        // (100, 100) + 2 x ((100, 100) - (100, 100)) + (10, 0) = (110, 100), 100 x 100.
        assert_eq!(on_screen, r(110.0, 100.0, 100.0, 100.0));
        // The content's rect goes through the same frame.
        let content = content_rect_to_screen(
            &lr,
            dom(0),
            1,
            r(110.0, 120.0, 10.0, 10.0),
            &|_, _| None,
            &own,
        );
        assert_eq!(content, r(130.0, 140.0, 20.0, 20.0));

        // And the tester hits the box there: the two agree.
        let mut results = BTreeMap::new();
        results.insert(dom(0), lr);
        let mut gpu = GpuStateManager::default();
        set_frame(&mut gpu, dom(0), 1, frame);
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));
        assert!(hits_node(&nodes_at(&tester, &gpu, &|_, _| None, 205.0, 195.0), dom(0), 1));
        assert!(!hits_node(&nodes_at(&tester, &gpu, &|_, _| None, 105.0, 150.0), dom(0), 1));
    }

    // A6-03: a view that moved its content without a layout.

    /// The numbers of A2-cpu's "Spaces": a view box at (50, 38), the picture
    /// root at child (0, 0) with `scale(1.5)` then `translate(20, 10)`, a
    /// label at child (100, 40), 20 x 10. With `content_offset` (0, 0) it is
    /// painted at `(50, 38) + 1.5 (100, 40) + (20, 10) = (220, 108)`,
    /// 30 x 15. A pan re-points the item to (-12, 0) in place - no layout,
    /// no rebuild of the tester - and it is painted at (208, 108).
    /// Mutation: `sync_view_placements` not called (or the move applied
    /// outside the root's frame, scaled by 1.5: 18 px) keeps the stale place.
    #[test]
    fn a_view_that_panned_in_place_is_hit_where_its_content_is_now() {
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout(
                styled(0),
                Vec::new(),
                Vec::new(),
                vec![view(1, r(50.0, 38.0, 600.0, 400.0), p(0.0, 0.0))],
            ),
        );
        results.insert(
            dom(1),
            layout(
                styled(1),
                vec![hot(0, (1000.0, 800.0), None), hot(1, (20.0, 10.0), Some(0))],
                vec![p(0.0, 0.0), p(100.0, 40.0)],
                Vec::new(),
            ),
        );
        let mut gpu = GpuStateManager::default();
        set_frame(&mut gpu, dom(1), 0, scaled(1.5, 20.0, 10.0));
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));
        let label = |tester: &CpuHitTester, x: f32, y: f32| {
            hits_node(&nodes_at(tester, &gpu, &|_, _| None, x, y), dom(1), 1)
        };
        assert!(label(&tester, 221.0, 109.0) && label(&tester, 249.0, 122.0));
        assert!(!label(&tester, 219.0, 109.0) && !label(&tester, 251.0, 109.0));

        // The pan: the item's offset alone.
        let host = results.get_mut(&dom(0)).expect("the host");
        std::sync::Arc::make_mut(&mut host.display_list).items[0] =
            view(1, r(50.0, 38.0, 600.0, 400.0), p(-12.0, 0.0));
        // A query that does not look at the item still answers the old place.
        assert!(label(&tester, 249.0, 109.0), "harness: stale until synced");
        tester.sync_view_placements(&results);
        assert!(label(&tester, 209.0, 109.0), "12 px left of where it was");
        assert!(label(&tester, 237.0, 122.0));
        assert!(!label(&tester, 239.0, 109.0), "its old right part is empty now");
        assert!(!label(&tester, 207.0, 109.0));
        // Synced again with nothing moved: the same answer.
        tester.sync_view_placements(&results);
        assert!(label(&tester, 209.0, 109.0) && !label(&tester, 239.0, 109.0));
    }

    // A6-04: what a query costs.

    /// A map's shape: a view over a child whose root (node 0) is zoomed and
    /// holds `n` label boxes, 80 x 16, laid out 50 to a row on a 90 x 20
    /// pitch. `turned_every`: every that-many-th label has a frame of its own.
    fn map_of(n: usize, turned_every: Option<usize>) -> (BTreeMap<DomId, DomLayoutResult>, GpuStateManager) {
        let rows = n.div_ceil(50);
        let mut nodes = vec![hot(0, (4600.0, 20.0 * rows as f32 + 20.0), None)];
        let mut positions = vec![p(0.0, 0.0)];
        let mut gpu = GpuStateManager::default();
        set_frame(&mut gpu, dom(1), 0, scaled(1.25, 10.0, 6.0));
        for i in 0..n {
            nodes.push(hot(i + 1, (80.0, 16.0), Some(0)));
            positions.push(p(90.0 * (i % 50) as f32, 20.0 * (i / 50) as f32));
            if turned_every.is_some_and(|k| i % k == 0) {
                set_frame(&mut gpu, dom(1), i + 1, turned(10.0, 0.0, 12.0));
            }
        }
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout(
                styled(0),
                Vec::new(),
                Vec::new(),
                vec![view(1, r(40.0, 30.0, 900.0, 600.0), p(0.0, 0.0))],
            ),
        );
        results.insert(dom(1), layout(styled(n), nodes, positions, Vec::new()));
        (results, gpu)
    }

    /// One query over `n` labels under one zoomed root tests the boxes of one
    /// grid cell (and the root, which lies over every cell), not `n`: the
    /// same budget of 12 holds at `n` and at `4 n`, and the answer is the
    /// label under the point.
    /// Mutation: `EntryGrid::build` returning `None` (every box tested)
    /// makes `entries_tested` `n + 1`.
    #[test]
    fn a_query_over_a_zoomed_map_tests_a_cell_of_boxes_not_all_of_them() {
        for n in [1000_usize, 4000] {
            let (results, gpu) = map_of(n, None);
            let mut tester = CpuHitTester::new();
            tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));
            // Label 123: row 2, column 23, at child (2070, 40); its centre
            // (2110, 48) is painted at (40, 30) + 1.25 (2110, 48) + (10, 6):
            // far right of the view box. Label 53: (270, 20), centre
            // (310, 28) -> (40 + 387.5 + 10, 30 + 35 + 6) = (437.5, 71).
            let at = p(437.5, 71.0);
            let hits = nodes_at(&tester, &gpu, &|_, _| None, at.x, at.y);
            assert_eq!(
                hits.iter().map(|(d, node, _)| (*d, *node)).collect::<Vec<_>>(),
                vec![(dom(1), NodeId::new(54)), (dom(1), NodeId::new(0))],
                "n = {n}: label 53 (node 54) over the root"
            );
            let work =
                tester.hit_test_work(at, &|_, _| None, &|d, node| gpu.painted_transform_of(d, node));
            assert!(
                work.entries_tested <= 12,
                "n = {n}: {} boxes tested for one point",
                work.entries_tested
            );
            assert_eq!(work.groups_visited, 1, "n = {n}: one chain holds every box");
            assert_eq!(work.links_resolved, 2, "n = {n}: the view and the root's frame, once");
        }
    }

    /// With a frame per label (a turned label is one chain) a query inverts
    /// one link per chain - the root's frame once, not once per label - and
    /// tests one box per chain: linear in the turned labels with one cheap
    /// step each, where it was every box against every clip after every
    /// chain had been resolved from its first link.
    /// Mutation: the parent's point not kept (`ChainPoints` resolving a chain
    /// from the window each time) makes `links_resolved` `3 n`.
    #[test]
    fn a_frame_per_label_costs_one_link_each_and_the_shared_frames_once() {
        let mut links = Vec::new();
        for n in [500_usize, 2000] {
            let (results, gpu) = map_of(n, Some(1));
            let mut tester = CpuHitTester::new();
            tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));
            let at = p(437.5, 71.0);
            let work =
                tester.hit_test_work(at, &|_, _| None, &|d, node| gpu.painted_transform_of(d, node));
            assert!(
                work.links_resolved <= n + 2,
                "n = {n}: {} links inverted",
                work.links_resolved
            );
            assert!(work.entries_tested <= n + 1, "n = {n}: {}", work.entries_tested);
            links.push(work.links_resolved);
        }
        assert!(
            (links[1] as f32) < 6.0 * links[0] as f32,
            "4 n labels, under 6 times the links: {links:?}"
        );
    }

    // A6-07: a rect of a nested dom, in the window.

    /// A view box at (100, 50) inside a host box (node 1, at (20, 10)) that is
    /// scaled by 2 about its corner; no content offset. The child's rect
    /// (400, 30) 10 x 16 (a caret) sits at host point (500, 80), which the
    /// frame paints at (20, 10) + 2 x (480, 70) = (980, 150), 20 x 32. The sum
    /// "origin of the dom + rect" answers (180 + 400, 90 + 30) = (580, 120)
    /// 10 x 16. And back: the window point (980, 150) is the child's (400, 30).
    /// Mutation: the rect added to the mapped origin instead of mapped
    /// (`nested_dom_window_origin` + rect) answers (580, 120) and fails.
    #[test]
    fn a_rect_of_a_nested_dom_is_scaled_with_the_view_it_is_shown_in() {
        let mut results = BTreeMap::new();
        results.insert(
            dom(0),
            layout(
                styled(1),
                vec![hot(0, (2000.0, 1000.0), None), hot(1, (1500.0, 800.0), Some(0))],
                vec![p(0.0, 0.0), p(20.0, 10.0)],
                vec![
                    DisplayListItem::PushReferenceFrame {
                        transform_key: TransformKey::unique(),
                        initial_transform: scaled(2.0, 0.0, 0.0),
                        bounds: WindowLogicalRect::new(p(20.0, 10.0), LogicalSize::new(1500.0, 800.0)),
                    },
                    view(1, r(100.0, 50.0, 900.0, 600.0), p(0.0, 0.0)),
                    DisplayListItem::PopReferenceFrame,
                ],
            ),
        );
        results.insert(
            dom(1),
            layout(
                styled(0),
                vec![hot(0, (900.0, 600.0), None)],
                vec![p(0.0, 0.0)],
                Vec::new(),
            ),
        );
        // The frame's item has no node mapping in this list, so the
        // placement names node 0 as its owner: the matrix is filed there.
        let mut gpu = GpuStateManager::default();
        set_frame(&mut gpu, dom(0), 0, scaled(2.0, 0.0, 0.0));
        let tf = |d: DomId, n: NodeId| gpu.painted_transform_of(d, n);

        let origin = nested_dom_window_origin(&results, dom(1), &|_, _| None, &tf).expect("placed");
        assert_eq!(origin, p(180.0, 90.0), "(20, 10) + 2 x (80, 40)");
        let caret = r(400.0, 30.0, 10.0, 16.0);
        let shown = nested_dom_rect_to_window(&results, dom(1), caret, &|_, _| None, &tf)
            .expect("drawn");
        assert_eq!(shown, r(980.0, 150.0, 20.0, 32.0));
        let back = window_point_to_nested_dom(&results, dom(1), p(980.0, 150.0), &|_, _| None, &tf)
            .expect("invertible");
        assert_eq!(back, p(400.0, 30.0));
        // The window's own dom is in the window already.
        assert_eq!(
            nested_dom_rect_to_window(&results, dom(0), caret, &|_, _| None, &tf),
            None
        );
    }

    // A6-05: the frames near the point, not every frame.

    /// The transform stamps of `gpu`'s doms, as `perform_hit_test` reads them.
    fn stamps_of(gpu: &GpuStateManager) -> impl Fn(DomId) -> TransformStamps + '_ {
        move |d| {
            gpu.get_cache(d)
                .map_or([0; 6], azul_core::gpu::GpuValueCache::transform_stamps)
        }
    }

    /// `times` syncs of the painted bounds with `gpu`'s values.
    fn sync_bounds(tester: &mut CpuHitTester, gpu: &GpuStateManager, times: usize) {
        for _ in 0..times {
            tester.sync_frame_bounds(&|d, n| gpu.painted_transform_of(d, n), &stamps_of(gpu));
        }
    }

    fn stamped_at(
        tester: &CpuHitTester,
        gpu: &GpuStateManager,
        x: f32,
        y: f32,
    ) -> Vec<(DomId, NodeId, LogicalPosition)> {
        tester.hit_test_scrolled_stamped(
            p(x, y),
            &|_, _| None,
            &|d, n| gpu.painted_transform_of(d, n),
            &stamps_of(gpu),
        )
    }

    fn stamped_work(tester: &CpuHitTester, gpu: &GpuStateManager, x: f32, y: f32) -> HitTestWork {
        tester.hit_test_work_stamped(
            p(x, y),
            &|_, _| None,
            &|d, n| gpu.painted_transform_of(d, n),
            &stamps_of(gpu),
        )
    }

    /// The label of A6-01's example: box (300, 200) 100 x 12, turned by 90
    /// degrees about (300, 212). Its corners are painted at (312, 212),
    /// (312, 312), (300, 212), (300, 312): the bounds are (300, 212) 12 x 100,
    /// widened by under 0.01 px, and never narrower than that box.
    /// Mutation: the corners mapped about the window's corner (no `origin`)
    /// answers a box near (-212, 300).
    #[test]
    fn the_painted_bounds_of_a_turned_box_are_the_box_around_its_painted_corners() {
        let m = turned(90.0, 0.0, 12.0);
        let tf = move |_: DomId, _: NodeId| Some(m);
        let live = LiveValues {
            scroll: &no_scroll,
            transform: &tf,
            view_shift: &no_view_shift,
        };
        let link =
            HitChainLink::Transform(dom(0), NodeId::new(1), FrameOrigin::new(p(300.0, 200.0)));
        let b = painted_bounds(&link, [300.0, 200.0, 400.0, 212.0], &live).expect("painted");
        let (far_x, far_y) = (b.origin.x + b.size.width, b.origin.y + b.size.height);
        assert!(b.origin.x <= 300.0 && b.origin.y <= 212.0, "{b:?}");
        assert!(far_x >= 312.0 && far_y >= 312.0, "{b:?}");
        assert!(b.origin.x > 299.99 && b.origin.y > 211.99, "{b:?}");
        assert!(far_x < 312.01 && far_y < 312.01, "{b:?}");
    }

    /// Over `n` turned labels (a frame each) a query whose stamps the kept
    /// bounds were made with inverts the frames whose painted bounds hold the
    /// point: 16 links and 12 groups at most, at `n` and at `4 n`, where it
    /// was `n + 2` links. And it answers what the walk over every frame
    /// answers, at every point of a lattice over the labels.
    /// Mutations: `sync_frame_bounds` never building (or `dom_hits` ignoring
    /// the bounds) makes `links_resolved` `n + 2`; a bound that is too small
    /// (the slack negative, a corner left out) fails the comparison of the
    /// answers.
    #[test]
    fn a_query_over_turned_labels_inverts_the_frames_near_the_point() {
        for n in [500_usize, 2000] {
            let (results, gpu) = map_of(n, Some(1));
            let mut tester = CpuHitTester::new();
            tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));

            // The stamps have not rested yet: every frame, as before.
            sync_bounds(&mut tester, &gpu, 1);
            let cold = stamped_work(&tester, &gpu, 437.5, 71.0);
            assert_eq!(cold.links_resolved, n + 2, "n = {n}: no bounds yet");

            sync_bounds(&mut tester, &gpu, 2);
            let work = stamped_work(&tester, &gpu, 437.5, 71.0);
            assert!(
                work.links_resolved <= 16,
                "n = {n}: {} links inverted for one point",
                work.links_resolved
            );
            assert!(work.groups_visited <= 12, "n = {n}: {}", work.groups_visited);

            let (mut on_a_label, mut points) = (0, 0);
            for row in 0..18_u8 {
                for col in 0..24_u8 {
                    let (x, y) = (45.0 + 37.0 * f32::from(col), 35.0 + 7.0 * f32::from(row));
                    let walked = nodes_at(&tester, &gpu, &|_, _| None, x, y);
                    assert_eq!(stamped_at(&tester, &gpu, x, y), walked, "n = {n}, at ({x}, {y})");
                    points += 1;
                    if walked.iter().any(|(_, node, _)| node.index() > 0) {
                        on_a_label += 1;
                    }
                }
            }
            assert!(
                on_a_label * 3 > points,
                "n = {n}: the lattice is over the labels ({on_a_label} of {points})"
            );
        }
    }

    /// A matrix that changed is never hit through the bounds of the old one:
    /// the label moved under the point is hit by the very next query, with
    /// the tester not synced in between; and once the stamps rest again the
    /// bounds are those of the new matrix.
    /// 200 labels in four rows (child y 0 .. 96); label 0 (node 1) is moved
    /// to child (400, 300): painted at (40, 30) + 1.25 (400, 300) + (10, 6) =
    /// (550, 411), 100 x 20, so (560, 415) is on it and was on nothing.
    /// Mutation: the comparison of the stamps taken out of `dom_hits` (kept
    /// bounds used whatever the matrices are now) misses the moved label.
    #[test]
    fn a_matrix_that_changed_is_never_hit_through_old_bounds() {
        let (results, mut gpu) = map_of(200, Some(1));
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));
        sync_bounds(&mut tester, &gpu, 3);
        assert!(stamped_work(&tester, &gpu, 437.5, 71.0).links_resolved <= 16, "bounds are kept");
        assert!(stamped_at(&tester, &gpu, 560.0, 415.0).is_empty(), "nothing is painted there");

        set_frame(&mut gpu, dom(1), 1, scaled(1.0, 400.0, 300.0));
        // Not synced: the kept bounds are those of the old matrices.
        let hits = stamped_at(&tester, &gpu, 560.0, 415.0);
        assert!(hits_node(&hits, dom(1), 1), "the moved label is hit at once: {hits:?}");
        assert_eq!(hits, nodes_at(&tester, &gpu, &|_, _| None, 560.0, 415.0));
        assert_eq!(
            stamped_work(&tester, &gpu, 560.0, 415.0).links_resolved,
            202,
            "old bounds are not used: every frame is inverted"
        );

        sync_bounds(&mut tester, &gpu, 3);
        let hits = stamped_at(&tester, &gpu, 560.0, 415.0);
        assert!(hits_node(&hits, dom(1), 1), "{hits:?}");
        assert!(stamped_work(&tester, &gpu, 560.0, 415.0).links_resolved <= 16);
        // Where the label was, it is not hit any more: (40 + 10 + 1.25 x 40,
        // 30 + 6 + 1.25 x 8) = (100, 46) was inside it.
        assert!(!hits_node(&stamped_at(&tester, &gpu, 100.0, 46.0), dom(1), 1));
    }

    /// A point the view does not show (beside its box) looks at none of the
    /// child's boxes and inverts none of its frames, however many there are.
    /// Mutation: the viewport test of `dom_hits` removed tests every chain.
    #[test]
    fn a_point_beside_the_view_looks_at_none_of_its_boxes() {
        for n in [500_usize, 2000] {
            let (results, gpu) = map_of(n, Some(2));
            let mut tester = CpuHitTester::new();
            tester.rebuild_from_layout_with_gpu(&results, Some(&gpu));
            let work = tester.hit_test_work(
                p(20.0, 300.0),
                &|_, _| None,
                &|d, node| gpu.painted_transform_of(d, node),
            );
            assert_eq!(
                work,
                HitTestWork::default(),
                "n = {n}: nothing of the view is at (20, 300)"
            );
        }
    }

    /// The grid answers what a walk over every box answers, in the same
    /// order: 600 boxes of mixed sizes (points, slivers, boxes over many
    /// cells, one over everything), 400 points, against the boxes themselves.
    /// Mutation: a cell computed differently for a rect's edge and for the
    /// point (`cell_along`), or the hits left unsorted, fails.
    #[test]
    fn the_grid_answers_what_a_walk_over_every_box_answers() {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move |modulo: u32| -> f32 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % u64::from(modulo)) as f32
        };
        let n = 600_usize;
        let mut nodes = vec![hot(0, (1000.0, 800.0), None)];
        let mut positions = vec![p(0.0, 0.0)];
        let mut rects = vec![r(0.0, 0.0, 1000.0, 800.0)];
        for i in 0..n {
            let size = match i % 7 {
                0 => (0.0, 0.0),
                1 => (next(300) + 100.0, next(200) + 100.0),
                2 => (next(40) + 1.0, 0.5),
                _ => (next(60) + 4.0, next(24) + 4.0),
            };
            let at = p(next(960) * 1.0 + 0.25 * next(4), next(760) + 0.5 * next(2));
            nodes.push(hot(i + 1, size, Some(0)));
            positions.push(at);
            rects.push(r(at.x, at.y, size.0, size.1));
        }
        let mut results = BTreeMap::new();
        results.insert(dom(0), layout(styled(n), nodes, positions, Vec::new()));
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&results);

        for _ in 0..400 {
            let at = p(next(1010) - 5.0 + 0.25 * next(4), next(810) - 5.0 + 0.5 * next(2));
            let walked: Vec<(DomId, NodeId)> = rects
                .iter()
                .enumerate()
                .rev()
                .filter(|(_, rect)| point_in_rect(at, rect))
                .map(|(i, _)| (dom(0), NodeId::new(i)))
                .collect();
            assert_eq!(tester.hit_test(at), walked, "at {at:?}");
        }
    }
}
