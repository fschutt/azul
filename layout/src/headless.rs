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

use crate::solver3::layout_tree::LayoutNodeId;
use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::FullHitTest,
    spaces::{BorderBoxLocal, ContentBoxLocal, Inclusivity, StaticLayoutPoint},
    styled_dom::StyledDom,
};

use crate::solver3::{getters::{get_overflow_x, get_overflow_y}, layout_tree::LayoutNodeHot, PositionVec};
use crate::window::DomLayoutResult;

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
    /// Interned ancestor chains (scroll frames + reference frames). Index 0
    /// is always the empty chain. Entry / clip `chain` values index into
    /// this. A node's on-screen position is
    /// `T_total(static_pos − scroll_total)` — the same rule
    /// `cpurender::raster` paints with (accumulated scroll subtraction, then
    /// the composed reference-frame transform), so pixels and pointer
    /// targets cannot disagree.
    chains: Vec<Vec<HitChainLink>>,
    /// Every node that got a `PushScrollFrame` (from
    /// `DomLayoutResult::scroll_ids`, the same set the display list uses),
    /// translated into window space, with the chain of its STRICT scroll
    /// ancestors (a container's own viewport box does not move when it
    /// scrolls — only its content does).
    scroll_containers: Vec<ScrollContainerEntry>,
    /// `VirtualView` child-DOM placements in window space (static coords).
    dom_placements: BTreeMap<DomId, LogicalRect>,
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HitChainLink {
    /// An ancestor scroll frame — shifts content by the CURRENT scroll
    /// offset (`ScrollManager::get_current_offset`), painted as
    /// `pos - offset`.
    Scroll(DomId, NodeId),
    /// An ancestor wrapped in a `PushReferenceFrame` (CSS transform / drag /
    /// animation) — the CURRENT matrix lives in the GPU value cache
    /// (`css_current_transform_values`), the same source the CPU raster
    /// reads at paint time.
    Transform(DomId, NodeId),
}

/// Minimal 2D affine mirroring `agg_rust::trans_affine::TransAffine`.
///
/// Row-vector convention: `x' = x·sx + y·shx + tx; y' = x·shy + y·sy + ty`.
/// Local copy because `agg-rust` is optional (svg/cpurender features) and
/// hit-testing must exist in every configuration. The multiply/invert bodies
/// are transcribed from agg so composition matches the raster EXACTLY.
#[derive(Debug, Clone, Copy)]
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
    #[must_use] pub const fn from_transform_3d(t: &azul_core::transform::ComputedTransform3D) -> Self {
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

    #[must_use] pub fn apply(&self, p: LogicalPosition) -> LogicalPosition {
        LogicalPosition {
            x: p.x.mul_add(self.sx, p.y * self.shx) + self.tx,
            y: p.x.mul_add(self.shy, p.y * self.sy) + self.ty,
        }
    }

    #[allow(clippy::float_cmp)] // exact equality is correct here: identity is a fast-path gate; a near-identity matrix must still be applied
    #[must_use] pub fn is_identity(&self) -> bool {
        self.sx == 1.0
            && self.shy == 0.0
            && self.shx == 0.0
            && self.sy == 1.0
            && self.tx == 0.0
            && self.ty == 0.0
    }
}

/// A chain resolved against the CURRENT scroll offsets and transform values.
///
/// The CPU raster paints `screen = T_total(pos − scroll_total)` — one
/// accumulated scroll translation and one composed transform stack, in that
/// order. The inverse mapping used for hit-testing is therefore
/// `local = T_total⁻¹(screen) + scroll_total`.
#[derive(Debug, Clone, Copy)]
struct ResolvedChain {
    scroll: LogicalPosition,
    /// FORWARD composed transform (identity when the chain has none).
    forward: ScreenMapAffine,
    has_transform: bool,
}

impl ResolvedChain {
    fn map_screen_to_local(&self, p: LogicalPosition) -> LogicalPosition {
        let p = if self.has_transform {
            let mut inv = self.forward;
            inv.invert();
            inv.apply(p)
        } else {
            p
        };
        LogicalPosition {
            x: p.x + self.scroll.x,
            y: p.y + self.scroll.y,
        }
    }

    fn map_local_to_screen(&self, p: LogicalPosition) -> LogicalPosition {
        let shifted = LogicalPosition {
            x: p.x - self.scroll.x,
            y: p.y - self.scroll.y,
        };
        if self.has_transform {
            self.forward.apply(shifted)
        } else {
            shifted
        }
    }
}

fn resolve_chain(
    chain: &[HitChainLink],
    resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> ResolvedChain {
    let mut scroll = LogicalPosition::zero();
    let mut forward = ScreenMapAffine::IDENTITY;
    let mut has_transform = false;
    for link in chain {
        match link {
            HitChainLink::Scroll(d, n) => {
                if let Some(o) = resolve_scroll(*d, *n) {
                    scroll.x += o.x;
                    scroll.y += o.y;
                }
            }
            HitChainLink::Transform(d, n) => {
                if let Some(t) = resolve_transform(*d, *n) {
                    // Mirror the raster's per-push composition:
                    // composed = tf.premultiply(current)
                    let mut tf = ScreenMapAffine::from_transform_3d(&t);
                    tf.premultiply(&forward);
                    forward = tf;
                    has_transform = true;
                }
            }
        }
    }
    ResolvedChain {
        scroll,
        forward,
        has_transform,
    }
}

/// Map a node's STATIC rect to its ON-SCREEN axis-aligned bounds.
///
/// Walks the node's ancestors, accumulates scroll offsets and reference-frame
/// transforms, and applies the raster's forward rule
/// `screen = T_total(corner − scroll_total)` to all four corners (result is
/// their AABB).
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
    let nodes = &layout_result.layout_tree.nodes;

    // Collect links walking child→root; reversing yields outermost-first
    // with, per ancestor, Transform before Scroll (the builder nests the
    // reference frame OUTSIDE the scroll frame).
    //
    // ANCESTORS ONLY, and now said so out loud: a scroll container's own
    // offset moves its CONTENT, so it must not move the container's own box.
    // `LayoutWindow::accumulated_scroll` answers the same question with an
    // explicit `Inclusivity`; the two used to differ only in a loop's
    // starting value.
    let mut links_rev: Vec<HitChainLink> = Vec::new();
    for anc in layout_result
        .layout_tree
        .ancestor_chain(LayoutNodeId::new(layout_idx), Inclusivity::AncestorsOnly)
    {
        let Some(anc_node) = nodes.get(anc.index()) else { break };
        if let Some(anid) = anc_node.dom_node_id {
            if layout_result.scroll_ids.contains_key(&anc) {
                links_rev.push(HitChainLink::Scroll(dom_id, anid));
            }
            if resolve_transform(dom_id, anid).is_some() {
                links_rev.push(HitChainLink::Transform(dom_id, anid));
            }
        }
    }
    if links_rev.is_empty() {
        return rect;
    }
    let chain: Vec<HitChainLink> = links_rev.into_iter().rev().collect();
    let resolved = resolve_chain(&chain, resolve_scroll, resolve_transform);

    let corners = [
        rect.origin,
        LogicalPosition {
            x: rect.origin.x + rect.size.width,
            y: rect.origin.y,
        },
        LogicalPosition {
            x: rect.origin.x,
            y: rect.origin.y + rect.size.height,
        },
        LogicalPosition {
            x: rect.origin.x + rect.size.width,
            y: rect.origin.y + rect.size.height,
        },
    ];
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for c in corners {
        let s = resolved.map_local_to_screen(c);
        min_x = min_x.min(s.x);
        min_y = min_y.min(s.y);
        max_x = max_x.max(s.x);
        max_y = max_y.max(s.y);
    }
    if !(min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite()) {
        return rect;
    }
    LogicalRect {
        origin: LogicalPosition { x: min_x, y: min_y },
        size: LogicalSize {
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
        },
    }
}

/// Intern a chain into the table, returning its index.
fn intern_chain(
    chains: &mut Vec<Vec<HitChainLink>>,
    lookup: &mut std::collections::HashMap<Vec<HitChainLink>, u32>,
    v: Vec<HitChainLink>,
) -> u32 {
    if let Some(&i) = lookup.get(&v) {
        return i;
    }
    let i = u32::try_from(chains.len()).unwrap_or(u32::MAX);
    chains.push(v.clone());
    lookup.insert(v, i);
    i
}

impl Default for CpuHitTester {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve each layout node's ancestor chain index into `chains`.
///
/// `chain(n) = chain(parent) (+ parent's links)` — an ancestor shifts its
/// CONTENT, not itself. Scroll membership comes from `scroll_ids` (the exact
/// set the display-list builder emitted `PushScrollFrame` for); transform
/// membership from the GPU value cache's `css_transform_keys` via
/// `has_transform` (the exact set it wrapped in `PushReferenceFrame`). A node
/// with both nests the reference frame OUTSIDE the scroll frame, same as the
/// builder.
fn compute_node_chains(
    layout_result: &DomLayoutResult,
    dom_id: DomId,
    base_chain: u32,
    has_transform: &dyn Fn(NodeId) -> bool,
    chains: &mut Vec<Vec<HitChainLink>>,
    chain_lookup: &mut std::collections::HashMap<Vec<HitChainLink>, u32>,
) -> Vec<u32> {
    let nodes = &layout_result.layout_tree.nodes;
    let scroll_ids = &layout_result.scroll_ids;
    let mut chain_of: Vec<u32> = vec![u32::MAX; nodes.len()];
    let mut path: Vec<usize> = Vec::new();
    for start in 0..nodes.len() {
        if chain_of[start] != u32::MAX {
            continue;
        }
        path.clear();
        path.push(start);
        let mut cur = nodes[start].parent;
        while let Some(p) = cur {
            if chain_of[p] != u32::MAX || path.len() > nodes.len() {
                break;
            }
            path.push(p);
            cur = nodes[p].parent;
        }
        for &idx in path.iter().rev() {
            let c = nodes[idx].parent.map_or(base_chain, |p| {
                let pc = if chain_of[p] == u32::MAX {
                    base_chain // cycle guard tripped; degrade gracefully
                } else {
                    chain_of[p]
                };
                let is_scroll = scroll_ids.contains_key(&LayoutNodeId::new(p));
                let pnid = nodes[p].dom_node_id;
                let parent_transforms = pnid.is_some_and(has_transform);
                match (pnid, is_scroll || parent_transforms) {
                    (Some(pnid), true) => {
                        let mut v = chains[pc as usize].clone();
                        if parent_transforms {
                            v.push(HitChainLink::Transform(dom_id, pnid));
                        }
                        if is_scroll {
                            v.push(HitChainLink::Scroll(dom_id, pnid));
                        }
                        intern_chain(chains, chain_lookup, v)
                    }
                    _ => pc,
                }
            });
            chain_of[idx] = c;
        }
    }
    chain_of
}

/// A resolved `VirtualView` child-DOM placement.
///
/// The composite rect in window space plus the host-side chain (scroll
/// frames AND reference frames) active at the `VirtualView` item.
struct Placement {
    rect: LogicalRect,
    chain: Vec<HitChainLink>,
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
fn resolve_virtual_view_placements(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
) -> BTreeMap<DomId, Placement> {
    let mut placements: BTreeMap<DomId, Placement> = BTreeMap::new();
    for _ in 0..4 {
        // bounded depth; each pass resolves one nesting level
        let mut changed = false;
        for (host_dom, lr) in layout_results {
            let (host_offset, host_chain) = if host_dom.inner == 0 {
                (LogicalPosition::zero(), Vec::new())
            } else if let Some(p) = placements.get(host_dom) {
                (p.rect.origin, p.chain.clone())
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
                    I::PushReferenceFrame { .. } => {
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
                        stack.push(HitChainLink::Transform(*host_dom, nid));
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
                        let differs = placements.get(child_dom_id).is_none_or(|p| {
                            p.rect != absolute || p.chain != stack
                        });
                        if differs {
                            placements.insert(
                                *child_dom_id,
                                Placement {
                                    rect: absolute,
                                    chain: stack.clone(),
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

impl CpuHitTester {
    /// Create a new empty hit tester.
    #[must_use] pub fn new() -> Self {
        Self {
            node_rects: BTreeMap::new(),
            chains: vec![Vec::new()],
            scroll_containers: Vec::new(),
            dom_placements: BTreeMap::new(),
        }
    }

    /// Resolve an interned chain against the current scroll offsets and
    /// transform values, then map a screen point into the chain's local
    /// (static layout) space.
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
        self.chains.get(chain as usize).map_or(p, |elems| {
            resolve_chain(elems, resolve_scroll, resolve_transform).map_screen_to_local(p)
        })
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
        layout_results: &BTreeMap<DomId, DomLayoutResult>,
    ) {
        self.rebuild_from_layout_with_gpu(layout_results, None);
    }

    /// Like [`Self::rebuild_from_layout`], but transform-aware: `gpu` is the
    /// window's [`GpuStateManager`](crate::managers::gpu_state::GpuStateManager),
    /// whose per-DOM `css_transform_keys` is the EXACT set of nodes the
    /// display list wrapped in `PushReferenceFrame` (the display-list builder
    /// reads the same cache) — so hit-test chains and painted frames cannot
    /// disagree about which nodes transform. Pass `None` only when no
    /// transforms can exist (unit tests, static popups).
    /// The DOM node ids currently registered as USER-wheel scroll targets.
    /// Test/introspection helper: programmatically-scrollable-only containers
    /// (`overflow: hidden`) must never appear here.
    #[must_use]
    pub fn debug_scroll_container_nodes(&self) -> Vec<NodeId> {
        self.scroll_containers.iter().map(|e| e.node_id).collect()
    }

    pub fn rebuild_from_layout_with_gpu(
        &mut self,
        layout_results: &BTreeMap<DomId, DomLayoutResult>,
        gpu: Option<&crate::managers::gpu_state::GpuStateManager>,
    ) {
        self.node_rects.clear();
        self.chains.clear();
        self.chains.push(Vec::new()); // chain 0 = empty
        self.scroll_containers.clear();
        self.dom_placements.clear();

        let placements = resolve_virtual_view_placements(layout_results);

        let mut chain_lookup: std::collections::HashMap<Vec<HitChainLink>, u32> =
            std::collections::HashMap::new();
        chain_lookup.insert(Vec::new(), 0);
        for (dom_id, p) in &placements {
            self.dom_placements.insert(*dom_id, p.rect);
        }

        for (dom_id, layout_result) in layout_results {
            let mut entries = Vec::new();

            let positions = &layout_result.calculated_positions;
            let nodes = &layout_result.layout_tree.nodes;
            let styled_dom = &layout_result.styled_dom;

            // Child DOM: shift into window space + clip to the composite rect.
            let (offset, dom_clip, base_chain_vec) = placements.get(dom_id).map_or_else(
                || (LogicalPosition::zero(), None, Vec::new()),
                |p| (p.rect.origin, Some(p.rect), p.chain.clone()),
            );
            let base_chain = intern_chain(&mut self.chains, &mut chain_lookup, base_chain_vec);
            let dom_clip_entry = dom_clip.map(|r| (r, base_chain));

            let scroll_ids = &layout_result.scroll_ids;
            let transform_nodes = gpu
                .and_then(|g| g.caches.get(dom_id))
                .map(|c| &c.css_transform_keys);
            let chain_of = compute_node_chains(
                layout_result,
                *dom_id,
                base_chain,
                &|n| transform_nodes.is_some_and(|t| t.contains_key(&n)),
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
                let Some(n) = nodes.get(layout_idx.index()) else { continue };
                let Some(node_id) = n.dom_node_id else { continue };
                let (Some(pos), Some(size)) = (positions.get(layout_idx.index()), n.used_size) else {
                    continue;
                };
                let user_scrollable = styled_dom
                    .styled_nodes
                    .as_container()
                    .get(node_id)
                    .is_some_and(|sn| {
                        let st = &sn.styled_node_state;
                        get_overflow_x(styled_dom, node_id, st)
                            .allows_user_scrolling()
                            || get_overflow_y(styled_dom, node_id, st)
                                .allows_user_scrolling()
                    });
                if !user_scrollable {
                    continue;
                }
                self.scroll_containers.push(ScrollContainerEntry {
                    dom_id: *dom_id,
                    node_id,
                    layout_idx,
                    scroll_id,
                    rect: LogicalRect {
                        origin: LogicalPosition {
                            x: pos.x + offset.x,
                            y: pos.y + offset.y,
                        },
                        size,
                    },
                    chain: chain_of[layout_idx.index()],
                });
            }

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
                // auto` ancestor's box — otherwise a node that is scrolled or
                // clipped out of its ancestor would still claim pointer events.
                let clips = compute_node_clips(
                    styled_dom,
                    nodes,
                    positions,
                    idx,
                    offset,
                    dom_clip_entry,
                    &chain_of,
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
                });
            }

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
    #[must_use] pub fn hit_test(
        &self,
        position: LogicalPosition,
    ) -> Vec<(DomId, NodeId)> {
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
    /// (`GpuValueCache::css_current_transform_values` — the same map the CPU
    /// raster reads at paint time). Content painted at
    /// `T_total(static_pos − scroll_total)` is hit at the same place: a
    /// point `p` hits a node iff `T⁻¹(p) + scroll_total` lands in the node's
    /// static rect. Clip boxes are shifted by the clip OWNER's chain — a
    /// scroller's viewport clips where the viewport IS, not where its
    /// content went.
    ///
    /// Returns `(dom, node, local_point)` triples in reverse paint order
    /// (topmost first), where `local_point` is the query point mapped into
    /// that node's STATIC layout space — callers use it directly for
    /// node-relative points (caret placement, `point_relative_to_item`).
    #[must_use] pub fn hit_test_scrolled(
        &self,
        position: LogicalPosition,
        resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<LogicalPosition>,
        resolve_transform: &dyn Fn(
            DomId,
            NodeId,
        ) -> Option<azul_core::transform::ComputedTransform3D>,
    ) -> Vec<(DomId, NodeId, LogicalPosition)> {
        let mut results = Vec::new();

        // Resolve every chain once per query, then map the point through it.
        let mapped: Vec<LogicalPosition> = self
            .chains
            .iter()
            .map(|chain| {
                resolve_chain(chain, resolve_scroll, resolve_transform)
                    .map_screen_to_local(position)
            })
            .collect();
        let local = |chain: u32| -> LogicalPosition {
            mapped.get(chain as usize).copied().unwrap_or(position)
        };

        for (dom_id, entries) in &self.node_rects {
            // Walk in reverse (last painted = topmost)
            for entry in entries.iter().rev() {
                if entry.pointer_events_none {
                    continue;
                }

                // Every clip box must contain the point (each in its owner's
                // space).
                if !entry
                    .clips
                    .iter()
                    .all(|(clip, chain)| point_in_rect(local(*chain), clip))
                {
                    continue;
                }

                // Check node rect in the node's local (static) space.
                let p_local = local(entry.chain);
                if point_in_rect(p_local, &entry.rect) {
                    results.push((*dom_id, entry.node_id, p_local));
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

        hit_test.regular_hit_test_nodes.insert(
            *node_id,
            HitTestItem {
                point_in_viewport: cursor_position,
                point_relative_to_item: point_relative,
                is_focusable: false,
                is_virtual_view_hit: None,
                hit_depth: depth as u32,
            },
        );
    }

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
    for sc in &tester.scroll_containers {
        let dom_id = &sc.dom_id;
        let node_id = sc.node_id;
        let scroll_id = sc.scroll_id;
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
                        point_relative_to_item: StaticLayoutPoint::new(
                            LogicalPosition::new(adj_x, adj_y),
                        )
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
    let content_size = layout_result.layout_tree.get_content_size(LayoutNodeId::new(layout_idx));
    LogicalRect::new(
        parent_rect.origin,
        LogicalSize::new(
            content_size.width.max(parent_rect.size.width),
            content_size.height.max(parent_rect.size.height),
        ),
    )
}

/// Compute the hit-test clip boxes for a layout node: the host `VirtualView`
/// composite bounds (`dom_clip_entry`) plus every clipping ancestor's border
/// box (any `overflow` other than `visible`), each tagged with the chain of
/// the clip OWNER's strict scroll ancestors so the query can shift each box
/// into its own scrolled space.
///
/// Clipping is tracked per-axis because `overflow-x` / `overflow-y` are
/// independent — an axis the ancestor does not clip is widened to
/// [`CLIP_UNBOUNDED`] (kept finite so `origin + size` arithmetic never
/// produces `inf - inf = NaN`). The ancestor box used is the border box
/// (`used_size`); CSS clips at the padding edge, but the slightly larger
/// border box is a safe over-inclusion for point hit-testing and avoids
/// resolving padding/border here.
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
fn compute_node_clips(
    styled_dom: &StyledDom,
    nodes: &[LayoutNodeHot],
    positions: &PositionVec,
    node_index: usize,
    offset: LogicalPosition,
    dom_clip_entry: Option<(LogicalRect, u32)>,
    chain_of: &[u32],
) -> Vec<(LogicalRect, u32)> {
    // A non-finite edge must degrade to "unclipped on that side", never be
    // stored: `point_in_rect` against a NaN rect is always false, which would
    // make every node under a corrupt clip silently unhittable.
    fn sanitize_clip_rect(r: LogicalRect) -> LogicalRect {
        let min_x = if r.min_x().is_finite() { r.min_x() } else { -CLIP_UNBOUNDED };
        let min_y = if r.min_y().is_finite() { r.min_y() } else { -CLIP_UNBOUNDED };
        let max_x = if r.max_x().is_finite() { r.max_x() } else { CLIP_UNBOUNDED };
        let max_y = if r.max_y().is_finite() { r.max_y() } else { CLIP_UNBOUNDED };
        LogicalRect {
            origin: LogicalPosition { x: min_x, y: min_y },
            size: LogicalSize {
                width: (max_x - min_x).max(0.0),
                height: (max_y - min_y).max(0.0),
            },
        }
    }

    let mut clips = Vec::new();
    if let Some((dc, chain)) = dom_clip_entry {
        clips.push((sanitize_clip_rect(dc), chain));
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
        styled_dom,
        nodes,
        positions,
        node_index,
        offset,
        dom_clip.map(|r| (r, 0)),
        &chain_of,
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

        let inset = |left: f32, top: f32| PackedBoxProps::pack(&ResolvedBoxProps {
            padding: EdgeSizes { top, right: 0.0, bottom: 0.0, left },
            border: EdgeSizes { top: 1.0, right: 0.0, bottom: 0.0, left: 1.0 },
            ..Default::default()
        });

        let styled_dom = styled("");
        let mut node = hot(Some(0), Some((100.0, 40.0)), None);
        node.box_props = inset(5.0, 2.0); // + 1px border on each of left/top
        let mut lr = layout_result(styled_dom, vec![node], vec![p(10.0, 20.0)], Vec::new());
        lr.layout_tree
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
        lr.layout_tree
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
