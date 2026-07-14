#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use super::*;

use std::collections::HashMap;
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_css::props::basic::{ColorU, FontRef};
use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
use azul_css::props::style::filter::StyleFilter;
use agg_rust::blur::stack_blur_rgba32;
use agg_rust::rendering_buffer::RowAccessor;
use agg_rust::trans_affine::TransAffine;
use crate::glyph_cache::GlyphCache;
use crate::solver3::display_list::{BorderRadius, DisplayList, DisplayListItem, LocalScrollId};
use crate::text3::cache::FontManager;

const IDENTITY_EPSILON: f32 = 0.0001;


// ============================================================================
// Retained-Mode Compositor — Layer Tree
// ============================================================================

/// Unique identifier for a compositing layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

/// Persistent compositor state across frames.
///
/// Holds a tree of `Layer`s, each with its own pixbuf. On incremental updates
/// only damaged layers are re-rendered, and scroll is handled by pixel-shift.
#[derive(Debug)]
pub struct CompositorState {
    /// All layers keyed by ID.
    pub layers: HashMap<LayerId, Layer>,
    /// Root layer of the tree.
    pub root_layer: LayerId,
    /// Monotonic counter for generating unique `LayerIds`.
    next_layer_id: u64,
    /// Previous frame's per-node positions, used for damage computation.
    pub previous_positions: Vec<LogicalPosition>,
}

/// A single compositing layer with its own pixel buffer.
#[derive(Debug)]
pub struct Layer {
    pub id: LayerId,
    /// Persistent RGBA buffer for this layer's content.
    pub pixbuf: AzulPixmap,
    /// Position and size in parent layer coordinates.
    pub bounds: LogicalRect,
    /// Dirty regions that need re-rendering this frame.
    pub damage: Vec<LogicalRect>,
    /// Child layers in z-order (bottom to top).
    pub children: Vec<LayerId>,
    /// Current scroll offset (for scroll-frame layers).
    pub scroll_offset: (f32, f32),
    /// Layer opacity (1.0 = fully opaque).
    pub opacity: f32,
    /// CSS filters applied at composite time. For a normal filter layer these
    /// are applied to the layer's OWN content; for a `backdrop-filter` layer
    /// (see `is_backdrop_filter`) they are instead applied to the already-
    /// composited backdrop pixels under the layer's bounds.
    pub filters: Vec<StyleFilter>,
    /// If true, `filters` apply to the backdrop (parent + earlier siblings
    /// already in `output`), not to this layer's own content.
    pub is_backdrop_filter: bool,
    /// CSS transform for this layer.
    pub transform: TransAffine,
    /// Range of display list items [start, end) that render into this layer.
    pub display_list_range: (usize, usize),
    /// If this layer is a scroll frame, the scroll ID.
    pub scroll_id: Option<LocalScrollId>,
    /// Whether this layer needs re-compositing onto its parent.
    pub composite_dirty: bool,
}

/// Reason a layer was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerReason {
    /// Root layer (always exists).
    Root,
    /// Created for a `PushScrollFrame`.
    ScrollFrame,
    /// Created for a `PushFilter` containing blur.
    BlurFilter,
    /// Created for a `PushOpacity` with opacity < 1.0.
    Opacity,
    /// Created for a `PushReferenceFrame` with non-identity transform.
    Transform,
}

impl CompositorState {
    /// Create a new compositor with a root layer sized to the viewport.
    #[allow(clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
    #[must_use] pub fn new(width: u32, height: u32) -> Self {
        let root_id = LayerId(0);
        let root_layer = Layer::new(
            root_id,
            LogicalRect {
                origin: LogicalPosition::zero(),
                size: LogicalSize {
                    width: width as f32,
                    height: height as f32,
                },
            },
            width,
            height,
        );
        let mut layers = HashMap::new();
        layers.insert(root_id, root_layer);
        Self {
            layers,
            root_layer: root_id,
            next_layer_id: 1,
            previous_positions: Vec::new(),
        }
    }

    /// Allocate a new unique layer ID.
    pub const fn alloc_layer_id(&mut self) -> LayerId {
        let id = LayerId(self.next_layer_id);
        self.next_layer_id += 1;
        id
    }

    /// Read-only peek at the next layer ID counter (for leak probes).
    #[must_use] pub const fn next_layer_id_peek(&self) -> u64 {
        self.next_layer_id
    }

    /// Walk the display list and create layers for scroll frames, filters, opacity, transforms.
    /// Returns a mapping from display-list item index to the `LayerId` it should render into.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    /// # Panics
    ///
    /// Panics if the internal layer stack underflows (a malformed display list).
    // single-pass walk over the full display-list opcode set; splitting it would
    // only scatter the shared layer-stack state across helpers
    #[allow(clippy::cognitive_complexity)]
    pub fn allocate_layers_from_display_list(
        &mut self,
        display_list: &DisplayList,
        dpi_factor: f32,
    ) {
        // Remove all non-root layers from previous frame
        let root_id = self.root_layer;
        self.layers.retain(|id, _| *id == root_id);
        if let Some(root) = self.layers.get_mut(&root_id) {
            root.children.clear();
            root.damage.clear();
            root.display_list_range = (0, display_list.items.len());
            root.composite_dirty = true;
        }

        let mut layer_stack: Vec<LayerId> = vec![root_id];
        let mut i = 0;

        while i < display_list.items.len() {
            match &display_list.items[i] {
                DisplayListItem::PushScrollFrame {
                    clip_bounds,
                    content_size,
                    scroll_id,
                    ..
                } => {
                    let bounds = *clip_bounds.inner();
                    let pw = (bounds.size.width * dpi_factor).ceil() as u32;
                    let ph = (bounds.size.height * dpi_factor).ceil() as u32;
                    if pw > 0 && ph > 0 {
                        let new_id = self.alloc_layer_id();
                        let mut layer = Layer::new(new_id, bounds, pw, ph);
                        layer.scroll_id = Some(*scroll_id);
                        // Find the matching PopScrollFrame to set range
                        let end = find_matching_pop(&display_list.items, i, MatchKind::ScrollFrame);
                        layer.display_list_range = (i + 1, end);
                        self.layers.insert(new_id, layer);
                        // Add as child of current parent
                        let parent_id = *layer_stack.last().unwrap();
                        if let Some(parent) = self.layers.get_mut(&parent_id) {
                            parent.children.push(new_id);
                        }
                        layer_stack.push(new_id);
                    }
                }
                DisplayListItem::PopScrollFrame => {
                    if layer_stack.len() > 1 {
                        layer_stack.pop();
                    }
                }
                DisplayListItem::PushOpacity { bounds, opacity } => {
                    if *opacity < 1.0 {
                        let b = *bounds.inner();
                        let pw = (b.size.width * dpi_factor).ceil() as u32;
                        let ph = (b.size.height * dpi_factor).ceil() as u32;
                        if pw > 0 && ph > 0 {
                            let new_id = self.alloc_layer_id();
                            let mut layer = Layer::new(new_id, b, pw, ph);
                            layer.opacity = *opacity;
                            let end = find_matching_pop(&display_list.items, i, MatchKind::Opacity);
                            layer.display_list_range = (i + 1, end);
                            self.layers.insert(new_id, layer);
                            let parent_id = *layer_stack.last().unwrap();
                            if let Some(parent) = self.layers.get_mut(&parent_id) {
                                parent.children.push(new_id);
                            }
                            layer_stack.push(new_id);
                        }
                    }
                }
                DisplayListItem::PopOpacity => {
                    // Only pop if the top layer was an opacity layer
                    if layer_stack.len() > 1 {
                        let top_id = *layer_stack.last().unwrap();
                        if let Some(layer) = self.layers.get(&top_id) {
                            if layer.opacity < 1.0 && layer.scroll_id.is_none() {
                                layer_stack.pop();
                            }
                        }
                    }
                }
                DisplayListItem::PushFilter { bounds, filters } => {
                    let has_blur = filters.iter().any(|f| matches!(f, StyleFilter::Blur(_)));
                    if has_blur {
                        let b = *bounds.inner();
                        let pw = (b.size.width * dpi_factor).ceil() as u32;
                        let ph = (b.size.height * dpi_factor).ceil() as u32;
                        if pw > 0 && ph > 0 {
                            let new_id = self.alloc_layer_id();
                            let mut layer = Layer::new(new_id, b, pw, ph);
                            layer.filters.clone_from(filters);
                            let end = find_matching_pop(&display_list.items, i, MatchKind::Filter);
                            layer.display_list_range = (i + 1, end);
                            self.layers.insert(new_id, layer);
                            let parent_id = *layer_stack.last().unwrap();
                            if let Some(parent) = self.layers.get_mut(&parent_id) {
                                parent.children.push(new_id);
                            }
                            layer_stack.push(new_id);
                        }
                    }
                }
                DisplayListItem::PopFilter => {
                    if layer_stack.len() > 1 {
                        let top_id = *layer_stack.last().unwrap();
                        if let Some(layer) = self.layers.get(&top_id) {
                            if !layer.filters.is_empty() {
                                layer_stack.pop();
                            }
                        }
                    }
                }
                DisplayListItem::PushReferenceFrame {
                    initial_transform,
                    bounds,
                    ..
                } => {
                    let m = &initial_transform.m;
                    let is_identity = (m[0][0] - 1.0).abs() < IDENTITY_EPSILON
                        && m[0][1].abs() < IDENTITY_EPSILON
                        && m[1][0].abs() < IDENTITY_EPSILON
                        && (m[1][1] - 1.0).abs() < IDENTITY_EPSILON
                        && m[3][0].abs() < IDENTITY_EPSILON
                        && m[3][1].abs() < IDENTITY_EPSILON;
                    if !is_identity {
                        let b = *bounds.inner();
                        let pw = (b.size.width * dpi_factor).ceil().max(1.0) as u32;
                        let ph = (b.size.height * dpi_factor).ceil().max(1.0) as u32;
                        let new_id = self.alloc_layer_id();
                        let mut layer = Layer::new(new_id, b, pw, ph);
                        layer.transform = TransAffine::new_custom(
                            f64::from(m[0][0]),
                            f64::from(m[0][1]),
                            f64::from(m[1][0]),
                            f64::from(m[1][1]),
                            f64::from(m[3][0]),
                            f64::from(m[3][1]),
                        );
                        let end =
                            find_matching_pop(&display_list.items, i, MatchKind::ReferenceFrame);
                        layer.display_list_range = (i + 1, end);
                        self.layers.insert(new_id, layer);
                        let parent_id = *layer_stack.last().unwrap();
                        if let Some(parent) = self.layers.get_mut(&parent_id) {
                            parent.children.push(new_id);
                        }
                        layer_stack.push(new_id);
                    }
                }
                DisplayListItem::PopReferenceFrame => {
                    if layer_stack.len() > 1 {
                        let top_id = *layer_stack.last().unwrap();
                        if let Some(layer) = self.layers.get(&top_id) {
                            if !layer.transform.is_identity(IDENTITY_EPSILON_F64) {
                                layer_stack.pop();
                            }
                        }
                    }
                }
                // `backdrop-filter` (superplan g4): allocate a layer mirroring
                // PushFilter, but tagged `is_backdrop_filter` so the compositor
                // applies the filter to the *backdrop* (parent + earlier siblings
                // already in `output`) rather than to the layer's own content.
                // The compositing side reads back the `output` region under the
                // layer bounds and runs `apply_layer_filters` on it before
                // blitting the content (see `composite_layer_recursive`).
                DisplayListItem::PushBackdropFilter { bounds, filters } => {
                    let b = *bounds.inner();
                    let pw = (b.size.width * dpi_factor).ceil() as u32;
                    let ph = (b.size.height * dpi_factor).ceil() as u32;
                    if pw > 0 && ph > 0 && !filters.is_empty() {
                        let new_id = self.alloc_layer_id();
                        let mut layer = Layer::new(new_id, b, pw, ph);
                        layer.filters.clone_from(filters);
                        layer.is_backdrop_filter = true;
                        // The layer's OWN content may be empty (e.g. an empty
                        // div with only `backdrop-filter`). render_layers skips
                        // empty display-list ranges, leaving the Layer::new
                        // opaque-white pixbuf, which would then be blitted over
                        // (and wipe) the filtered backdrop. Start transparent so
                        // an empty backdrop-filter element shows the backdrop.
                        layer.pixbuf.fill(0, 0, 0, 0);
                        let end =
                            find_matching_pop(&display_list.items, i, MatchKind::BackdropFilter);
                        layer.display_list_range = (i + 1, end);
                        self.layers.insert(new_id, layer);
                        let parent_id = *layer_stack.last().unwrap();
                        if let Some(parent) = self.layers.get_mut(&parent_id) {
                            parent.children.push(new_id);
                        }
                        layer_stack.push(new_id);
                    }
                }
                DisplayListItem::PopBackdropFilter => {
                    if layer_stack.len() > 1 {
                        let top_id = *layer_stack.last().unwrap();
                        if let Some(layer) = self.layers.get(&top_id) {
                            if layer.is_backdrop_filter {
                                layer_stack.pop();
                            }
                        }
                    }
                }
                // `text-shadow` (Push/PopTextShadow) is a text-rasterization
                // concern, not a layer boundary, so it is handled in
                // `render_single_item`, not here.
                _ => {}
            }
            i += 1;
        }
    }

    /// Compute damage rects from dirty node sets and old/new positions.
    pub fn compute_damage(
        &mut self,
        dirty_nodes: &std::collections::BTreeSet<usize>,
        old_positions: &[LogicalPosition],
        new_positions: &[LogicalPosition],
        calculated_rects: &[LogicalRect],
    ) {
        if dirty_nodes.is_empty() {
            return;
        }

        let mut damage_rects = Vec::new();
        for &node_idx in dirty_nodes {
            // Old bounds
            if node_idx < old_positions.len() && node_idx < calculated_rects.len() {
                let old_rect = LogicalRect {
                    origin: old_positions[node_idx],
                    size: calculated_rects[node_idx].size,
                };
                damage_rects.push(old_rect);
            }
            // New bounds
            if node_idx < new_positions.len() && node_idx < calculated_rects.len() {
                let new_rect = LogicalRect {
                    origin: new_positions[node_idx],
                    size: calculated_rects[node_idx].size,
                };
                damage_rects.push(new_rect);
            }
        }

        // Distribute damage rects to affected layers
        for layer in self.layers.values_mut() {
            for damage in &damage_rects {
                if let Some(intersection) = rect_intersection(&layer.bounds, damage) {
                    layer.damage.push(intersection);
                    layer.composite_dirty = true;
                }
            }
        }
    }

    /// Render display list items into their respective layer pixbufs.
    /// # Panics
    ///
    /// Panics if a referenced layer id is not present in the layer map.
    /// # Errors
    ///
    /// Returns an error string if the layers cannot be composited.
    pub fn render_layers(
        &mut self,
        display_list: &DisplayList,
        dpi_factor: f32,
        renderer_resources: &RendererResources,
        font_manager: Option<&FontManager<FontRef>>,
        glyph_cache: &mut GlyphCache,
        render_state: &CpuRenderState,
    ) -> Result<(), String> {
        let scroll_offsets = &render_state.scroll_offsets;
        // Collect layer IDs, ranges, bounds, scroll_id and child ranges.
        let layer_ranges: Vec<(
            LayerId,
            (usize, usize),
            LogicalRect,
            Option<LocalScrollId>,
            Vec<(usize, usize)>,
        )> = self
            .layers
            .iter()
            .map(|(id, layer)| {
                // Ranges of this layer's DIRECT children (nested scroll frames /
                // opacity / transform groups). They render into their own
                // pixbufs, so they must be skipped when rendering this layer's
                // range (which, for the root, spans the whole display list).
                let child_ranges: Vec<(usize, usize)> = layer
                    .children
                    .iter()
                    .filter_map(|cid| self.layers.get(cid).map(|c| c.display_list_range))
                    .collect();
                (*id, layer.display_list_range, layer.bounds, layer.scroll_id, child_ranges)
            })
            .collect();

        #[cfg(feature = "std")]
        if std::env::var("AZ_MAP_DEBUG").is_ok() {
            for (id, range, bounds, scroll_id, child_ranges) in &layer_ranges {
                std::eprintln!(
                    "[cpu-layer] render id={:?} range={:?} bounds={:?} scroll={:?} skip={:?} (dl_len={})",
                    id, range, bounds, scroll_id, child_ranges, display_list.items.len()
                );
            }
        }

        for (layer_id, range, layer_bounds, scroll_id, child_ranges) in &layer_ranges {
            let (start, end) = *range;
            if start >= end || start >= display_list.items.len() {
                continue;
            }

            // This layer's scroll offset (0 for non-scroll layers). Content inside
            // a scroll frame is at absolute coords; the renderer draws at
            // `pos - seed`, so folding the scroll offset into the seed shifts the
            // frame's content within its own pixbuf. composite_frame blits the
            // pixbuf back at `layer.bounds.origin` (NOT scroll_offset), so applying
            // it here is the single place — no double offset. (Without this, a full
            // repaint while scrolled drew content at offset 0.)
            let soff = scroll_id
                .and_then(|id| scroll_offsets.get(&id).copied())
                .unwrap_or((0.0, 0.0));

            let layer = self.layers.get_mut(layer_id).unwrap();
            layer.scroll_offset = soff;

            // Clear the layer pixbuf (transparent for non-root, white for root)
            if *layer_id == self.root_layer {
                layer.pixbuf.fill(255, 255, 255, 255);
            } else {
                layer.pixbuf.fill(0, 0, 0, 0);
            }

            // Seed = layer origin (for pixbuf-local placement) + scroll offset.
            let offset_x = layer_bounds.origin.x + soff.0;
            let offset_y = layer_bounds.origin.y + soff.1;
            render_display_list_range(
                display_list,
                &mut layer.pixbuf,
                start,
                end.min(display_list.items.len()),
                child_ranges,
                offset_x,
                offset_y,
                dpi_factor,
                renderer_resources,
                font_manager,
                glyph_cache,
                render_state,
            )?;
        }

        Ok(())
    }

    /// Composite all layers bottom-up into the final output pixmap.
    pub fn composite_frame(&self, output: &mut AzulPixmap, dpi_factor: f32) {
        // Start from root layer
        self.composite_layer_recursive(self.root_layer, output, 0.0, 0.0, dpi_factor);
    }

    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn composite_layer_recursive(
        &self,
        layer_id: LayerId,
        output: &mut AzulPixmap,
        parent_offset_x: f32,
        parent_offset_y: f32,
        dpi_factor: f32,
    ) {
        let Some(layer) = self.layers.get(&layer_id) else {
            return;
        };

        let abs_x = parent_offset_x + layer.bounds.origin.x;
        let abs_y = parent_offset_y + layer.bounds.origin.y;

        // For root layer, just blit directly
        if layer_id == self.root_layer {
            blit_pixmap(&layer.pixbuf, output, 0, 0, 1.0);
        } else {
            let px_x = (abs_x * dpi_factor) as i32;
            let px_y = (abs_y * dpi_factor) as i32;

            if layer.is_backdrop_filter && !layer.filters.is_empty() {
                // `backdrop-filter`: the backdrop (parent + earlier siblings) is
                // ALREADY composited into `output` at this point (bottom-up
                // order). Snapshot the region under the layer's bounds, run the
                // filter on that copy, write it back, THEN blit the layer's own
                // (unfiltered) content on top.
                let w = layer.pixbuf.width;
                let h = layer.pixbuf.height;
                let snap = snapshot_region(output, px_x, px_y, w, h);
                let mut backdrop = AzulPixmap {
                    data: snap,
                    width: w,
                    height: h,
                };
                apply_layer_filters(&mut backdrop, &layer.filters, dpi_factor);
                write_region(output, &backdrop.data, w, h, px_x, px_y);
                blit_pixmap(&layer.pixbuf, output, px_x, px_y, layer.opacity);
            } else {
                // Apply filters at composite time (to the layer's own content).
                let src = if layer.filters.is_empty() {
                    None
                } else {
                    let mut filtered = layer.pixbuf.clone_pixmap();
                    apply_layer_filters(&mut filtered, &layer.filters, dpi_factor);
                    Some(filtered)
                };

                let src_pixbuf = src.as_ref().unwrap_or(&layer.pixbuf);
                blit_pixmap(src_pixbuf, output, px_x, px_y, layer.opacity);
            }
        }

        // Composite children in z-order
        let children: Vec<LayerId> = layer.children.clone();
        for child_id in &children {
            self.composite_layer_recursive(
                *child_id,
                output,
                if layer_id == self.root_layer {
                    0.0
                } else {
                    abs_x
                },
                if layer_id == self.root_layer {
                    0.0
                } else {
                    abs_y
                },
                dpi_factor,
            );
        }
    }

    /// Handle scroll by shifting pixels and re-rendering the exposed strip.
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    #[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
    /// # Panics
    ///
    /// Panics if `layer_id` is not present in the layer map.
    /// # Errors
    ///
    /// Returns an error string if the layer cannot be scrolled.
    pub fn scroll_layer(
        &mut self,
        scroll_id: LocalScrollId,
        new_offset: (f32, f32),
        display_list: &DisplayList,
        dpi_factor: f32,
        renderer_resources: &RendererResources,
        font_manager: Option<&FontManager<FontRef>>,
        glyph_cache: &mut GlyphCache,
    ) -> Result<(), String> {
        // Find the layer with this scroll_id
        let layer_id = self
            .layers
            .iter()
            .find(|(_, l)| l.scroll_id == Some(scroll_id))
            .map(|(id, _)| *id);

        let Some(layer_id) = layer_id else {
            return Ok(()); // No layer for this scroll ID
        };

        let layer = self.layers.get_mut(&layer_id).unwrap();
        let old_offset = layer.scroll_offset;
        let dx = new_offset.0 - old_offset.0;
        let dy = new_offset.1 - old_offset.1;

        if dx.abs() < 0.5 && dy.abs() < 0.5 {
            return Ok(());
        }

        // Shift pixels
        let px_dx = (dx * dpi_factor).round() as i32;
        let px_dy = (dy * dpi_factor).round() as i32;
        shift_pixbuf(&mut layer.pixbuf, px_dx, px_dy);

        // Compute exposed strips and re-render them.
        // Diagonal scroll produces 2 rects (one vertical strip + one horizontal strip).
        let exposed = compute_exposed_rects(&layer.bounds, dx, dy);
        for exposed_rect in exposed {
            layer.damage.push(exposed_rect);
        }

        layer.scroll_offset = new_offset;
        layer.composite_dirty = true;

        // Re-render damaged regions
        let range = layer.display_list_range;
        let bounds = layer.bounds;
        let offset_x = bounds.origin.x;
        let offset_y = bounds.origin.y;
        // Child-layer ranges to skip (rendered separately) — same as render_layers.
        let child_ranges: Vec<(usize, usize)> = self
            .layers
            .get(&layer_id)
            .map(|l| {
                l.children
                    .iter()
                    .filter_map(|cid| self.layers.get(cid).map(|c| c.display_list_range))
                    .collect()
            })
            .unwrap_or_default();
        // Scroll fast-path: VirtualView content (separate child DOMs) isn't
        // re-composited here — an empty state suffices (VirtualViews inside a
        // scrolling region are an edge case; the next full repaint composites them).
        let empty_rs = CpuRenderState::new(ScrollOffsetMap::new());
        render_display_list_range(
            display_list,
            &mut self.layers.get_mut(&layer_id).unwrap().pixbuf,
            range.0,
            range.1.min(display_list.items.len()),
            &child_ranges,
            offset_x,
            offset_y,
            dpi_factor,
            renderer_resources,
            font_manager,
            glyph_cache,
            &empty_rs,
        )?;

        Ok(())
    }
}

impl Layer {
    fn new(id: LayerId, bounds: LogicalRect, pixel_width: u32, pixel_height: u32) -> Self {
        Self {
            id,
            pixbuf: AzulPixmap::new(pixel_width.max(1), pixel_height.max(1)).unwrap_or_else(|| {
                AzulPixmap {
                    data: vec![0; 4],
                    width: 1,
                    height: 1,
                }
            }),
            bounds,
            damage: Vec::new(),
            children: Vec::new(),
            scroll_offset: (0.0, 0.0),
            opacity: 1.0,
            filters: Vec::new(),
            is_backdrop_filter: false,
            transform: TransAffine::new(),
            display_list_range: (0, 0),
            scroll_id: None,
            composite_dirty: true,
        }
    }
}

// ============================================================================
// Layer helper types and functions
// ============================================================================

/// Which Push/Pop pair to match.
#[derive(Clone, Copy)]
enum MatchKind {
    ScrollFrame,
    Opacity,
    Filter,
    BackdropFilter,
    ReferenceFrame,
}

/// Find the matching Pop for a given Push at index `start`.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
fn find_matching_pop(items: &[DisplayListItem], start: usize, kind: MatchKind) -> usize {
    let mut depth = 1u32;
    for (i, item) in items.iter().enumerate().skip(start + 1) {
        match (item, kind) {
            (DisplayListItem::PushScrollFrame { .. }, MatchKind::ScrollFrame) => depth += 1,
            (DisplayListItem::PopScrollFrame, MatchKind::ScrollFrame) => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            (DisplayListItem::PushOpacity { .. }, MatchKind::Opacity) => depth += 1,
            (DisplayListItem::PopOpacity, MatchKind::Opacity) => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            (DisplayListItem::PushFilter { .. }, MatchKind::Filter) => depth += 1,
            (DisplayListItem::PopFilter, MatchKind::Filter) => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            (DisplayListItem::PushBackdropFilter { .. }, MatchKind::BackdropFilter) => depth += 1,
            (DisplayListItem::PopBackdropFilter, MatchKind::BackdropFilter) => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            (DisplayListItem::PushReferenceFrame { .. }, MatchKind::ReferenceFrame) => depth += 1,
            (DisplayListItem::PopReferenceFrame, MatchKind::ReferenceFrame) => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    items.len()
}

/// Compute exposed rectangles after a scroll of (dx, dy) in logical coords.
/// Returns 0, 1, or 2 rects: a vertical strip (top/bottom) and/or a horizontal
/// strip (left/right). Diagonal scrolling produces both strips.
fn compute_exposed_rects(bounds: &LogicalRect, dx: f32, dy: f32) -> Vec<LogicalRect> {
    let w = bounds.size.width;
    let h = bounds.size.height;
    let mut rects = Vec::new();

    // Vertical exposed strip (full width, covers top or bottom edge)
    if dy.abs() > 0.5 {
        let strip = if dy > 0.0 {
            // Scrolled down — top strip exposed
            LogicalRect {
                origin: LogicalPosition {
                    x: bounds.origin.x,
                    y: bounds.origin.y,
                },
                size: LogicalSize {
                    width: w,
                    height: dy.min(h),
                },
            }
        } else {
            // Scrolled up — bottom strip exposed
            LogicalRect {
                origin: LogicalPosition {
                    x: bounds.origin.x,
                    y: bounds.origin.y + h + dy,
                },
                size: LogicalSize {
                    width: w,
                    height: (-dy).min(h),
                },
            }
        };
        rects.push(strip);
    }

    // Horizontal exposed strip (full height, covers left or right edge)
    if dx.abs() > 0.5 {
        let strip = if dx > 0.0 {
            LogicalRect {
                origin: LogicalPosition {
                    x: bounds.origin.x,
                    y: bounds.origin.y,
                },
                size: LogicalSize {
                    width: dx.min(w),
                    height: h,
                },
            }
        } else {
            LogicalRect {
                origin: LogicalPosition {
                    x: bounds.origin.x + w + dx,
                    y: bounds.origin.y,
                },
                size: LogicalSize {
                    width: (-dx).min(w),
                    height: h,
                },
            }
        };
        rects.push(strip);
    }

    rects
}

/// Scroll a frame's clip region by *moving the pixels already on screen* and
/// return the newly-exposed strip(s) (logical coords) that still need painting.
///
/// This is the thin-strip optimisation for scrolling: instead of repainting the
/// whole `clip_bounds` viewport every frame, we `memmove` the pixels that are
/// still visible and only re-rasterise the strip that scrolled into view. For a
/// 30px scroll of a 200×100 viewport that turns ~20k painted px into ~6k.
///
/// Sign convention is the renderer's, NOT the legacy `compute_exposed_rects`:
/// `render_single_item`/`scroll_rect` draw a content item at `position - offset`,
/// so a *positive* `delta` (the user scrolled further down/right) moves on-screen
/// content UP/LEFT. We therefore move the existing pixels UP/LEFT and expose a
/// strip at the trailing (bottom/right) edge. `compute_exposed_rects` assumed the
/// inverse and never matched the renderer — it and `scroll_layer` are dead code.
///
/// Only pixels strictly inside the (clamped) clip rectangle are moved, so the
/// scrollbar, the parent background and sibling content outside the frame are
/// left untouched. Diagonal scroll (both axes in one frame — mobile pan) is
/// handled as TWO strips: the vertical move + horizontal move are separable 1-D
/// passes, so the net effect is a 2-D translation and the exposed region is an
/// L-shape (a full-width top/bottom strip + a full-height left/right strip).
/// The two strips overlap in one corner; that corner is simply repainted twice,
/// which is correct (the caller clears then renders each item once).
///
/// Returns an empty vec when nothing moved, or `[clip_bounds]` when the shift is
/// large enough that the whole viewport is exposed (caller repaints in full).
///
/// NOTE: the move copies *composited* pixels, so a scroll frame whose content is
/// not opaque over its clip can drag whatever showed through. Real scroll
/// containers paint an opaque background or fully cover their box, so this is a
/// known, documented limitation rather than a correctness bug for the common case.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
pub fn scroll_shift_region(
    pixmap: &mut AzulPixmap,
    clip_bounds: &LogicalRect,
    delta: (f32, f32),
    new_offset: (f32, f32),
    dpi_factor: f32,
) -> Vec<LogicalRect> {
    // Physical shift = difference of the ROUNDED offsets, not the rounded
    // difference. Rounding each frame's delta independently accumulates up to
    // 0.5px of error per step at fractional dpi (the moved block drifts away
    // from the freshly-rasterised strips, showing internal seams); anchoring
    // both ends to the absolute offset keeps the cumulative error ≤ 1px
    // forever: round(new·dpi) − round(prev·dpi) telescopes across frames.
    let prev_offset = (new_offset.0 - delta.0, new_offset.1 - delta.1);
    let px_dx =
        (new_offset.0 * dpi_factor).round() as i32 - (prev_offset.0 * dpi_factor).round() as i32;
    let px_dy =
        (new_offset.1 * dpi_factor).round() as i32 - (prev_offset.1 * dpi_factor).round() as i32;

    // Nothing actually moved (sub-pixel jitter rounds to zero).
    if px_dx == 0 && px_dy == 0 {
        return Vec::new();
    }

    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;

    // Clip rectangle in physical pixels, clamped to the pixmap.
    let cx0 = ((clip_bounds.origin.x * dpi_factor).floor() as i32).clamp(0, pw);
    let cy0 = ((clip_bounds.origin.y * dpi_factor).floor() as i32).clamp(0, ph);
    let cx1 = (((clip_bounds.origin.x + clip_bounds.size.width) * dpi_factor).ceil() as i32)
        .clamp(0, pw);
    let cy1 = (((clip_bounds.origin.y + clip_bounds.size.height) * dpi_factor).ceil() as i32)
        .clamp(0, ph);
    let region_w = cx1 - cx0;
    let region_h = cy1 - cy0;
    if region_w <= 0 || region_h <= 0 {
        return Vec::new();
    }

    // Shift exceeds the region — every pixel is exposed, so skip the memmove and
    // let the caller repaint the whole clip.
    if px_dx.abs() >= region_w || px_dy.abs() >= region_h {
        return vec![*clip_bounds];
    }

    // Dispatch to a specialised mover. The common single-axis cases get a tight
    // 1-D pass; diagonal pan gets a SINGLE-pass 2-D move (each row copied once
    // from its diagonally-offset source) instead of two sequential full passes —
    // half the memory traffic. (no-op is already handled by the early return.)
    let stride_px = pw;
    let data = pixmap.data_mut();
    match (px_dx != 0, px_dy != 0) {
        (false, true) => shift_vertical_1d(data, stride_px, cx0, cy0, cx1, cy1, px_dy),
        (true, false) => shift_horizontal_1d(data, stride_px, cx0, cy0, cx1, cy1, px_dx),
        (true, true) => shift_diagonal_2d(data, stride_px, cx0, cy0, cx1, cy1, px_dx, px_dy),
        (false, false) => {}
    }

    // Exposed strip(s) in LOGICAL coords. Over-cover the moving edge by one
    // physical pixel so dpi rounding never leaves a 1px white seam between the
    // moved block and the freshly-painted strip. One strip per moved axis, so
    // diagonal pan yields two (an L-shape, overlapping in one corner).
    //
    // Strips are derived from the CLAMPED region (`cx0..cx1`/`cy0..cy1`, the
    // pixels the memmove actually touched), not the raw `clip_bounds`. When
    // the clip extends past the pixmap (container taller than the window),
    // the raw clip's trailing edge is off-screen — a strip placed there
    // clamps to nothing, nothing repaints, and the rows at the WINDOW edge
    // keep their pre-shift content: a stale duplicated band that gets
    // re-dragged on every subsequent scroll.
    let cbx = cx0 as f32 / dpi_factor;
    let cby = cy0 as f32 / dpi_factor;
    let cbw = (cx1 - cx0) as f32 / dpi_factor;
    let cbh = (cy1 - cy0) as f32 / dpi_factor;
    let mut exposed = Vec::new();
    if px_dy != 0 {
        let h_logical = (px_dy.abs() as f32 + 1.0) / dpi_factor;
        let h = h_logical.min(cbh);
        let y = if px_dy > 0 {
            // bottom strip exposed
            cby + cbh - h
        } else {
            // top strip exposed
            cby
        };
        exposed.push(LogicalRect {
            origin: LogicalPosition { x: cbx, y },
            size: LogicalSize { width: cbw, height: h },
        });
    }
    if px_dx != 0 {
        let w_logical = (px_dx.abs() as f32 + 1.0) / dpi_factor;
        let w = w_logical.min(cbw);
        let x = if px_dx > 0 {
            // right strip exposed
            cbx + cbw - w
        } else {
            // left strip exposed
            cbx
        };
        exposed.push(LogicalRect {
            origin: LogicalPosition { x, y: cby },
            size: LogicalSize { width: w, height: cbh },
        });
    }
    exposed
}

// --- scroll_shift_region movers -------------------------------------------
// All three operate in PHYSICAL pixels on the raw RGBA buffer. `cx0..cx1` /
// `cy0..cy1` is the clamped clip region; `stride_px` is the buffer width in
// pixels. They only ever touch bytes inside the clip rectangle. Sign of the
// `px_*` deltas follows the renderer: positive = content moves up/left, so the
// exposed strip is the trailing (bottom/right) edge.

/// Single-axis VERTICAL move: shift whole rows up (`px_dy>0`) or down (`px_dy`<0).
/// Iteration order is chosen so a row read as a source is never already
/// overwritten (src and dst row SETS overlap, so order matters).
#[inline]
#[allow(clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
fn shift_vertical_1d(
    data: &mut [u8],
    stride_px: i32,
    cx0: i32,
    cy0: i32,
    cx1: i32,
    cy1: i32,
    px_dy: i32,
) {
    let col_bytes = ((cx1 - cx0) * 4) as usize;
    let row_off = |row: i32| ((row * stride_px + cx0) as usize) * 4;
    if px_dy > 0 {
        // Content up: dst = src - px_dy (dst < src) → iterate top→bottom.
        for dst in cy0..(cy1 - px_dy) {
            let s = row_off(dst + px_dy);
            data.copy_within(s..s + col_bytes, row_off(dst));
        }
    } else {
        let amt = -px_dy;
        // Content down: dst = src + amt (dst > src) → iterate bottom→top.
        for dst in ((cy0 + amt)..cy1).rev() {
            let s = row_off(dst - amt);
            data.copy_within(s..s + col_bytes, row_off(dst));
        }
    }
}

/// Single-axis HORIZONTAL move: shift each row's pixels left (`px_dx>0`) or right
/// (`px_dx`<0). Source and dest overlap WITHIN a row, so `copy_within`'s memmove
/// semantics handle it directly — no per-row ordering needed.
#[inline]
#[allow(clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
fn shift_horizontal_1d(
    data: &mut [u8],
    stride_px: i32,
    cx0: i32,
    cy0: i32,
    cx1: i32,
    cy1: i32,
    px_dx: i32,
) {
    let col_bytes = ((cx1 - cx0) * 4) as usize;
    let row_off = |row: i32| ((row * stride_px + cx0) as usize) * 4;
    if px_dx > 0 {
        let shift = (px_dx * 4) as usize;
        for row in cy0..cy1 {
            let left = row_off(row);
            data.copy_within(left + shift..left + col_bytes, left);
        }
    } else {
        let shift = ((-px_dx) * 4) as usize;
        for row in cy0..cy1 {
            let left = row_off(row);
            data.copy_within(left..left + col_bytes - shift, left + shift);
        }
    }
}

/// Diagonal (two-axis) pan in ONE pass: each destination row is copied directly
/// from its diagonally-offset source row, applying the column shift in the same
/// `copy_within`. Because |`px_dy`| ≥ 1, the source and dest rows are always
/// DIFFERENT rows ≥ one stride apart, so the per-copy byte ranges never overlap
/// regardless of the horizontal direction — only the row iteration order (by
/// `px_dy` sign) matters, exactly as in the vertical case. This does the work of
/// the two 1-D passes with half the memory traffic.
#[inline]
#[allow(clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
fn shift_diagonal_2d(
    data: &mut [u8],
    stride_px: i32,
    cx0: i32,
    cy0: i32,
    cx1: i32,
    cy1: i32,
    px_dx: i32,
    px_dy: i32,
) {
    let span_cols = (cx1 - cx0) - px_dx.abs();
    if span_cols <= 0 {
        return; // horizontal shift covers the whole region — nothing to keep
    }
    let len = (span_cols * 4) as usize;
    // Column starts for the kept span: content-left reads from the right, etc.
    let (src_col, dst_col) = if px_dx > 0 {
        (cx0 + px_dx, cx0)
    } else {
        (cx0, cx0 - px_dx)
    };
    let src_byte = |row: i32| ((row * stride_px + src_col) as usize) * 4;
    let dst_byte = |row: i32| ((row * stride_px + dst_col) as usize) * 4;
    if px_dy > 0 {
        // Content up: src row = dst + px_dy (below) → iterate top→bottom.
        for dst in cy0..(cy1 - px_dy) {
            let s = src_byte(dst + px_dy);
            data.copy_within(s..s + len, dst_byte(dst));
        }
    } else {
        let amt = -px_dy;
        // Content down: src row = dst - amt (above) → iterate bottom→top.
        for dst in ((cy0 + amt)..cy1).rev() {
            let s = src_byte(dst - amt);
            data.copy_within(s..s + len, dst_byte(dst));
        }
    }
}

/// Decide whether scroll frame `scroll_id` may use the [`scroll_shift_region`]
/// memmove fast path, or whether the caller must full-repaint the clip instead.
///
/// The memmove drags whatever is composited inside the clip. That is only WRONG
/// when transparent gaps in the SCROLLING content let static "backdrop" pixels
/// (painted *behind* the frame) show through and get dragged along. Per the
/// project's aggressive policy: take the fast path UNLESS that exact condition is
/// proven — i.e. fall back ONLY when (a) something is painted behind the frame
/// within the clip AND (b) the scrolling content does not opaquely cover the clip.
///
/// `scroll_offset` is the frame's current offset and `prev_offset` the offset
/// the pixels being moved were rendered at; both are used to project the
/// content's opaque fills (stored at content coords) into viewport space for
/// the coverage test — coverage must hold at BOTH offsets, since the memmove
/// drags pixels that were composited at the OLD offset. A scroll frame over
/// nothing-but-the-clear-color is always eligible (no backdrop to drag).
/// Returns `true` when there is no such frame (nothing to do).
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[must_use] pub fn scroll_fast_path_eligible(
    display_list: &DisplayList,
    scroll_id: LocalScrollId,
    clip_bounds: &LogicalRect,
    scroll_offset: (f32, f32),
    prev_offset: (f32, f32),
) -> bool {
    // Locate the frame's content range [start+1, end).
    let start = display_list.items.iter().position(|it| {
        matches!(it, DisplayListItem::PushScrollFrame { scroll_id: sid, .. } if *sid == scroll_id)
    });
    let Some(start) = start else {
        return true; // no frame for this id → nothing to shift
    };
    let end = find_matching_pop(&display_list.items, start, MatchKind::ScrollFrame)
        .min(display_list.items.len());

    // NESTED frame → ineligible. An inner frame's clip_bounds are the OUTER
    // frame's content coords: with the outer frame scrolled, the memmove
    // would shift a region displaced from the real on-screen clip by the
    // outer offset. Conservative full-clip repaint instead.
    let mut depth = 0i32;
    for it in &display_list.items[..start] {
        match it {
            DisplayListItem::PushScrollFrame { .. } => depth += 1,
            DisplayListItem::PopScrollFrame => depth -= 1,
            _ => {}
        }
    }
    if depth > 0 {
        return false;
    }

    // NOTE on overlays: anything painted AFTER the frame that overlaps the
    // clip (the frame's own scrollbar, an open dropdown, a tooltip) gets
    // dragged by the memmove. That does NOT make the frame ineligible — the
    // caller repaints those regions after the shift via
    // [`overlay_rects_after_frame`] (a scrollbar would otherwise disable the
    // fast path for every scroll container).

    // (a) Best case: the SCROLLING content opaquely covers the clip (projected
    // into viewport space by the scroll offset — at BOTH the old offset, where
    // the dragged pixels were rendered, and the new one). Then nothing behind
    // can ever show through, so the shift is always safe.
    let covered_at = |off: (f32, f32)| {
        let fills: Vec<LogicalRect> = display_list.items[start + 1..end]
            .iter()
            .filter_map(opaque_fill_rect)
            .map(|r| LogicalRect {
                origin: LogicalPosition {
                    x: r.origin.x - off.0,
                    y: r.origin.y - off.1,
                },
                size: r.size,
            })
            .collect();
        rect_covered_by(clip_bounds, &fills)
    };
    if covered_at(scroll_offset) && covered_at(prev_offset) {
        return true;
    }

    // (b) Content has gaps. The drag is only VISIBLE if a NON-UNIFORM backdrop
    // shows through. Scan items behind the frame for SIGNIFICANT backdrop fills
    // (≥10% of the clip) — borders, text, shadows, thin/small decorations smear
    // imperceptibly and are ignored (aggressive policy: fall back only on a
    // proven artifact). Classify each significant backdrop item:
    //   - flat opaque Rect  → track its colour
    //   - Image / gradient  → non-uniform → not safe
    // Then: no significant backdrop → safe (only the clear behind); a single flat
    // colour that COVERS the clip → drags invisibly → safe; mixed colours, a
    // partial cover, or any non-uniform fill → full-repaint.
    let clip_area = (clip_bounds.size.width * clip_bounds.size.height).max(1.0);
    let mut backdrop_fills: Vec<LogicalRect> = Vec::new();
    let mut backdrop_color: Option<ColorU> = None;
    for it in &display_list.items[..start] {
        if it.is_state_management() {
            continue;
        }
        let b = match it.bounds() {
            Some(b) if rects_overlap_or_adjacent(&b, clip_bounds, 0.0) => b,
            _ => continue,
        };
        // Area of this item within the clip; ignore negligible coverage.
        let ix = b.origin.x.max(clip_bounds.origin.x);
        let iy = b.origin.y.max(clip_bounds.origin.y);
        let ix1 = (b.origin.x + b.size.width).min(clip_bounds.origin.x + clip_bounds.size.width);
        let iy1 = (b.origin.y + b.size.height).min(clip_bounds.origin.y + clip_bounds.size.height);
        let isect_area = ((ix1 - ix).max(0.0)) * ((iy1 - iy).max(0.0));
        if isect_area < clip_area * 0.10 {
            continue; // negligible — thin border / small decoration
        }
        match it {
            DisplayListItem::Rect { color, border_radius, .. }
                if color.a == 255 && border_radius.is_zero() =>
            {
                match backdrop_color {
                    None => backdrop_color = Some(*color),
                    Some(prev) if prev == *color => {}
                    Some(_) => return false, // ≥2 distinct backdrop colours → visible
                }
                backdrop_fills.push(b);
            }
            DisplayListItem::Rect { .. } => {} // translucent / rounded — let it drag
            DisplayListItem::Image { .. }
            | DisplayListItem::LinearGradient { .. }
            | DisplayListItem::RadialGradient { .. }
            | DisplayListItem::ConicGradient { .. } => return false, // non-uniform fill
            _ => {} // border/text/shadow/scrollbar etc. — negligible
        }
    }
    if backdrop_fills.is_empty() {
        return true; // only the clear (or negligible decoration) behind
    }
    // Single flat colour: safe only if it fills the whole clip (else its edge
    // against the clear would drag visibly).
    rect_covered_by(clip_bounds, &backdrop_fills)
}

/// Result of diffing the GPU-animated values between two frames.
#[derive(Debug, Default)]
pub struct GpuValueDamage {
    /// Regions to repaint (scrollbar bounds whose thumb/opacity value changed).
    pub rects: Vec<LogicalRect>,
    /// A changed transform is bound to a `PushReferenceFrame` (drag / CSS
    /// transform animation): the moved CONTENT's extent isn't derivable from
    /// the item alone, so the caller must full-repaint.
    pub needs_full: bool,
}

/// Diff the GPU value maps of two frames and damage the items BOUND to the
/// changed keys.
///
/// Scrollbar thumb position, scrollbar fade opacity, and drag/CSS transforms
/// live in the GPU value cache; display-list items only carry the KEYS, so
/// they compare `is_visually_equal` while the pixels must change. Without
/// this channel, the `ScrollBarStyled` equality arm would freeze the thumb
/// (missed damage); with it, an idle window reaches `FrameDamage::None` even
/// with scrollbars present.
#[allow(clippy::implicit_hasher)] // internal call sites all use std hasher
#[must_use] pub fn gpu_value_damage(
    display_list: &DisplayList,
    old_transforms: &HashMap<usize, azul_core::transform::ComputedTransform3D>,
    old_opacities: &HashMap<usize, f32>,
    new_transforms: &HashMap<usize, azul_core::transform::ComputedTransform3D>,
    new_opacities: &HashMap<usize, f32>,
) -> GpuValueDamage {
    use std::collections::HashSet;

    let mut changed_t: HashSet<usize> = HashSet::new();
    for (k, v) in new_transforms {
        if old_transforms.get(k) != Some(v) {
            changed_t.insert(*k);
        }
    }
    for k in old_transforms.keys() {
        if !new_transforms.contains_key(k) {
            changed_t.insert(*k);
        }
    }
    let mut changed_o: HashSet<usize> = HashSet::new();
    for (k, v) in new_opacities {
        if old_opacities.get(k) != Some(v) {
            changed_o.insert(*k);
        }
    }
    for k in old_opacities.keys() {
        if !new_opacities.contains_key(k) {
            changed_o.insert(*k);
        }
    }

    let mut out = GpuValueDamage::default();
    if changed_t.is_empty() && changed_o.is_empty() {
        return out;
    }

    for item in &display_list.items {
        match item {
            DisplayListItem::ScrollBarStyled { info } => {
                let thumb_moved = info
                    .thumb_transform_key
                    .is_some_and(|k| changed_t.contains(&k.id));
                let faded = info.opacity_key.is_some_and(|k| changed_o.contains(&k.id));
                if thumb_moved || faded {
                    // The whole bar bounds cover the thumb's old AND new
                    // position — precise and cheap.
                    out.rects.push(info.bounds.0);
                }
            }
            DisplayListItem::PushReferenceFrame { transform_key, .. } => {
                if changed_t.contains(&transform_key.id) {
                    out.needs_full = true;
                }
            }
            _ => {}
        }
    }
    // A changed key bound to nothing in THIS display list (another DOM's
    // scrollbar, a stale key) is ignored — it cannot affect these pixels.
    out
}

/// Clip-intersected bounds of every item painted AFTER scroll frame
/// `scroll_id`'s `PopScrollFrame` that STRICTLY overlaps `clip_bounds`.
///
/// Anything composited over the frame inside its clip (the frame's own
/// scrollbar, an open dropdown/context menu/tooltip, a sibling's box-shadow)
/// gets DRAGGED by the `scroll_shift_region` memmove. Rather than making such
/// frames ineligible for the fast path (a scrollbar would disable it for
/// every scroll container), the caller adds these rects to the damage set so
/// the dragged pixels are simply repainted after the shift.
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[must_use] pub fn overlay_rects_after_frame(
    display_list: &DisplayList,
    scroll_id: LocalScrollId,
    clip_bounds: &LogicalRect,
) -> Vec<LogicalRect> {
    let mut out = Vec::new();
    let Some(start) = display_list.items.iter().position(|it| {
        matches!(it, DisplayListItem::PushScrollFrame { scroll_id: sid, .. } if *sid == scroll_id)
    }) else {
        return out;
    };
    let end = find_matching_pop(&display_list.items, start, MatchKind::ScrollFrame)
        .min(display_list.items.len());
    let cx1 = clip_bounds.origin.x + clip_bounds.size.width;
    let cy1 = clip_bounds.origin.y + clip_bounds.size.height;
    for it in &display_list.items[end..] {
        if it.is_state_management() {
            continue;
        }
        let Some(b) = it.bounds() else { continue };
        // STRICT overlap: merely touching shares no pixels with the clip and
        // cannot be dragged.
        let ix = b.origin.x.max(clip_bounds.origin.x);
        let iy = b.origin.y.max(clip_bounds.origin.y);
        let ix1 = (b.origin.x + b.size.width).min(cx1);
        let iy1 = (b.origin.y + b.size.height).min(cy1);
        if ix1 > ix && iy1 > iy {
            out.push(LogicalRect {
                origin: LogicalPosition { x: ix, y: iy },
                size: LogicalSize {
                    width: ix1 - ix,
                    height: iy1 - iy,
                },
            });
        }
    }
    out
}

/// If `it` is a fully-opaque, square-cornered rectangle fill, its bounds.
fn opaque_fill_rect(it: &DisplayListItem) -> Option<LogicalRect> {
    match it {
        DisplayListItem::Rect {
            bounds,
            color,
            border_radius,
        } if color.a == 255 && border_radius.is_zero() => Some(*bounds.inner()),
        _ => None,
    }
}

/// True if every ~4px sample of `target` lies inside some rect in `covers`.
/// Point-sampled so sub-4px gaps (imperceptible if dragged) don't force a full
/// repaint; empty `covers` → not covered.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
fn rect_covered_by(target: &LogicalRect, covers: &[LogicalRect]) -> bool {
    if covers.is_empty() {
        return false;
    }
    let step = 4.0_f32;
    let x0 = target.origin.x;
    let y0 = target.origin.y;
    let x1 = x0 + target.size.width;
    let y1 = y0 + target.size.height;
    let mut y = y0 + step * 0.5;
    #[allow(clippy::while_float)] // intentional bounded float loop (angle-wrap / pixel-step); an integer counter would be artificial
    while y < y1 {
        let mut x = x0 + step * 0.5;
        #[allow(clippy::while_float)] // intentional bounded float loop (angle-wrap / pixel-step); an integer counter would be artificial
        while x < x1 {
            let inside = covers.iter().any(|r| {
                x >= r.origin.x
                    && x < r.origin.x + r.size.width
                    && y >= r.origin.y
                    && y < r.origin.y + r.size.height
            });
            if !inside {
                return false;
            }
            x += step;
        }
        y += step;
    }
    true
}

/// Apply CSS filters to a pixbuf at composite time.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn apply_layer_filters(pixmap: &mut AzulPixmap, filters: &[StyleFilter], dpi_factor: f32) {
    for filter in filters {
        match filter {
            StyleFilter::Blur(blur) => {
                let rx = blur
                    .width
                    .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
                    * dpi_factor;
                let ry = blur
                    .height
                    .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
                    * dpi_factor;
                let radius = f32::midpoint(rx, ry).ceil() as u32;
                if radius > 0 {
                    let w = pixmap.width;
                    let h = pixmap.height;
                    let stride = (w * 4) as i32;
                    let mut ra = unsafe {
                        RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride)
                    };
                    stack_blur_rgba32(&mut ra, radius, radius);
                }
            }
            StyleFilter::Opacity(pct) => {
                let op = (pct.normalized() * 255.0).clamp(0.0, 255.0) as u32;
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    chunk[3] = ((u32::from(chunk[3]) * op) / 255) as u8;
                }
            }
            StyleFilter::Grayscale(pct) => {
                let amount = pct.normalized().clamp(0.0, 1.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    let r = f32::from(chunk[0]);
                    let g = f32::from(chunk[1]);
                    let b = f32::from(chunk[2]);
                    let gray = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    chunk[0] = (r + (gray - r) * amount).clamp(0.0, 255.0) as u8;
                    chunk[1] = (g + (gray - g) * amount).clamp(0.0, 255.0) as u8;
                    chunk[2] = (b + (gray - b) * amount).clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::Brightness(pct) => {
                let factor = pct.normalized().max(0.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    chunk[0] = (f32::from(chunk[0]) * factor).clamp(0.0, 255.0) as u8;
                    chunk[1] = (f32::from(chunk[1]) * factor).clamp(0.0, 255.0) as u8;
                    chunk[2] = (f32::from(chunk[2]) * factor).clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::Contrast(pct) => {
                let factor = pct.normalized().max(0.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    chunk[0] = ((((f32::from(chunk[0]) / 255.0) - 0.5) * factor + 0.5) * 255.0)
                        .clamp(0.0, 255.0) as u8;
                    chunk[1] = ((((f32::from(chunk[1]) / 255.0) - 0.5) * factor + 0.5) * 255.0)
                        .clamp(0.0, 255.0) as u8;
                    chunk[2] = ((((f32::from(chunk[2]) / 255.0) - 0.5) * factor + 0.5) * 255.0)
                        .clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::Invert(pct) => {
                let amount = pct.normalized().clamp(0.0, 1.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    chunk[0] = (f32::from(chunk[0]) + (255.0 - 2.0 * f32::from(chunk[0])) * amount)
                        .clamp(0.0, 255.0) as u8;
                    chunk[1] = (f32::from(chunk[1]) + (255.0 - 2.0 * f32::from(chunk[1])) * amount)
                        .clamp(0.0, 255.0) as u8;
                    chunk[2] = (f32::from(chunk[2]) + (255.0 - 2.0 * f32::from(chunk[2])) * amount)
                        .clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::Sepia(pct) => {
                let amount = pct.normalized().clamp(0.0, 1.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    let r = f32::from(chunk[0]);
                    let g = f32::from(chunk[1]);
                    let b = f32::from(chunk[2]);
                    let sr = (0.393 * r + 0.769 * g + 0.189 * b).min(255.0);
                    let sg = (0.349 * r + 0.686 * g + 0.168 * b).min(255.0);
                    let sb = (0.272 * r + 0.534 * g + 0.131 * b).min(255.0);
                    chunk[0] = (r + (sr - r) * amount).clamp(0.0, 255.0) as u8;
                    chunk[1] = (g + (sg - g) * amount).clamp(0.0, 255.0) as u8;
                    chunk[2] = (b + (sb - b) * amount).clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::Saturate(pct) => {
                let s = pct.normalized().max(0.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    let r = f32::from(chunk[0]);
                    let g = f32::from(chunk[1]);
                    let b = f32::from(chunk[2]);
                    let gray = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    chunk[0] = (gray + (r - gray) * s).clamp(0.0, 255.0) as u8;
                    chunk[1] = (gray + (g - gray) * s).clamp(0.0, 255.0) as u8;
                    chunk[2] = (gray + (b - gray) * s).clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::HueRotate(angle) => {
                let rad = angle.to_degrees().to_radians();
                let cos_a = rad.cos();
                let sin_a = rad.sin();
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    let r = f32::from(chunk[0]);
                    let g = f32::from(chunk[1]);
                    let b = f32::from(chunk[2]);
                    let nr = (0.213 + 0.787 * cos_a - 0.213 * sin_a) * r
                        + (0.715 - 0.715 * cos_a - 0.715 * sin_a) * g
                        + (0.072 - 0.072 * cos_a + 0.928 * sin_a) * b;
                    let ng = (0.213 - 0.213 * cos_a + 0.143 * sin_a) * r
                        + (0.715 + 0.285 * cos_a + 0.140 * sin_a) * g
                        + (0.072 - 0.072 * cos_a - 0.283 * sin_a) * b;
                    let nb = (0.213 - 0.213 * cos_a - 0.787 * sin_a) * r
                        + (0.715 - 0.715 * cos_a + 0.715 * sin_a) * g
                        + (0.072 + 0.928 * cos_a + 0.072 * sin_a) * b;
                    chunk[0] = nr.clamp(0.0, 255.0) as u8;
                    chunk[1] = ng.clamp(0.0, 255.0) as u8;
                    chunk[2] = nb.clamp(0.0, 255.0) as u8;
                }
            }
            _ => {} // Blend, Flood, ColorMatrix, DropShadow, ComponentTransfer, Offset, Composite not yet implemented
        }
    }
}

/// Render a range of display list items into a layer pixbuf,
/// offsetting coordinates by the layer's origin.
fn render_display_list_range(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    start: usize,
    end: usize,
    // Index ranges (start..end) that belong to CHILD layers (nested scroll
    // frames / opacity / transform groups). Those items render into the child's
    // OWN pixbuf, so they must be skipped here — otherwise they're drawn twice
    // (once in this layer at absolute coords AND once in the child layer),
    // which produced overlapping / ghosted text in overflow:scroll content.
    skip_ranges: &[(usize, usize)],
    offset_x: f32,
    offset_y: f32,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
) -> Result<(), String> {
    let mut transform_stack = vec![TransAffine::new()];
    let mut clip_stack: Vec<Option<AzRect>> = vec![None];
    let mut mask_stack: Vec<MaskEntry> = Vec::new();
    // Apply the layer origin offset: content is translated by -(offset_x,offset_y)
    // so it's rendered RELATIVE to this layer's pixbuf origin (which is then
    // composited back at +layer_origin). The renderer translates positions by
    // `pos - scroll_offset`, so seeding the scroll-offset stack with the layer
    // origin achieves the relative placement. Previously offset_x/offset_y were
    // ignored, so child layers were double-offset (content drawn at absolute
    // coords then composited at +origin) — text fell to the bottom of the box.
    let mut scroll_offset_stack: Vec<(f32, f32)> = vec![(offset_x, offset_y)];
    let mut text_shadow_stack: Vec<azul_css::props::style::box_shadow::StyleBoxShadow> = Vec::new();

    for i in start..end {
        // Skip items rendered by a child layer (see skip_ranges doc above).
        if skip_ranges.iter().any(|(s, e)| i >= *s && i < *e) {
            continue;
        }
        let item = &display_list.items[i];
        render_single_item(
            item,
            pixmap,
            dpi_factor,
            renderer_resources,
            font_manager,
            glyph_cache,
            &mut transform_stack,
            &mut clip_stack,
            &mut mask_stack,
            &mut scroll_offset_stack,
            &mut text_shadow_stack,
            render_state,
        )?;
    }

    Ok(())
}

// ============================================================================
// AzulPixmap — replacement for tiny_skia::Pixmap
// ============================================================================

/// Compute damage rects by comparing two display lists item by item.
///
/// Returns a list of bounding rects that need repainting, or `None` if a
/// full repaint is required (structural change, different item count, etc.).
///
/// The comparison is conservative: any item whose bounds or content changed
/// produces a damage rect covering both the old and new bounds.
///
/// The returned rects are in VIEWPORT space. Items inside a scroll frame are
/// stored at CONTENT coords but render at `pos - scroll_offset`, so a changed
/// item's bounds are projected through the accumulated scroll offset of its
/// enclosing frame(s) — the OLD item through `old_offsets` (where its pixels
/// were on screen), the NEW item through `new_offsets` (where they will be).
/// Without this projection, damage for items inside a scrolled frame lands at
/// the content-space position (off by exactly the scroll offset), so the
/// consumer repaints the wrong band and the changed item stays visually stale.
#[must_use] pub fn compute_display_list_damage(
    old: &DisplayList,
    new: &DisplayList,
    old_offsets: &ScrollOffsetMap,
    new_offsets: &ScrollOffsetMap,
) -> Option<Vec<LogicalRect>> {
    // Different item counts → structural change → full repaint
    if old.items.len() != new.items.len() {
        return None;
    }

    let mut damage = Vec::new();

    // Accumulated (old, new) scroll offsets of the enclosing frames. The two
    // lists are structurally identical (discriminants checked below), so one
    // stack driven by the new list tracks both.
    let mut offset_stack: Vec<((f32, f32), (f32, f32))> = vec![((0.0, 0.0), (0.0, 0.0))];

    for (old_item, new_item) in old.items.iter().zip(new.items.iter()) {
        // Compare discriminant first (cheap)
        if std::mem::discriminant(old_item) != std::mem::discriminant(new_item) {
            return None; // structural change
        }

        match new_item {
            DisplayListItem::PushScrollFrame { scroll_id, .. } => {
                let (acc_old, acc_new) = *offset_stack.last().unwrap_or(&((0.0, 0.0), (0.0, 0.0)));
                let o = old_offsets.get(scroll_id).copied().unwrap_or((0.0, 0.0));
                let n = new_offsets.get(scroll_id).copied().unwrap_or((0.0, 0.0));
                offset_stack.push((
                    (acc_old.0 + o.0, acc_old.1 + o.1),
                    (acc_new.0 + n.0, acc_new.1 + n.1),
                ));
            }
            DisplayListItem::PopScrollFrame => {
                if offset_stack.len() > 1 {
                    offset_stack.pop();
                }
            }
            _ => {}
        }

        // Compare full visual content, not just bounds — a color or text
        // change within the same bounds must still produce a damage rect.
        // Use visual_bounds() to include effects like box-shadow extent.
        if !old_item.is_visually_equal(new_item) {
            let (acc_old, acc_new) = *offset_stack.last().unwrap_or(&((0.0, 0.0), (0.0, 0.0)));
            if let Some(ob) = old_item.visual_bounds() {
                damage.push(LogicalRect {
                    origin: LogicalPosition {
                        x: ob.origin.x - acc_old.0,
                        y: ob.origin.y - acc_old.1,
                    },
                    size: ob.size,
                });
            }
            if let Some(nb) = new_item.visual_bounds() {
                damage.push(LogicalRect {
                    origin: LogicalPosition {
                        x: nb.origin.x - acc_new.0,
                        y: nb.origin.y - acc_new.1,
                    },
                    size: nb.size,
                });
            }
        }
    }

    // Coalesce overlapping rects
    coalesce_damage_rects(&mut damage);
    Some(damage)
}

/// Are two display lists visually identical?
///
/// (same length, same item
/// discriminants, every item `is_visually_equal`). Cheaper proxy than a
/// structural hash, reusing the same per-item comparison the damage diff uses.
#[must_use] pub fn display_lists_visually_equal(a: &DisplayList, b: &DisplayList) -> bool {
    if a.items.len() != b.items.len() {
        return false;
    }
    a.items.iter().zip(b.items.iter()).all(|(x, y)| {
        std::mem::discriminant(x) == std::mem::discriminant(y) && x.is_visually_equal(y)
    })
}

/// Damage rects for `VirtualView` child DOMs whose content changed since the
/// previous frame.
///
/// The parent display list only carries a `VirtualView { child_dom_id, bounds }`
/// item that stays byte-identical when the *child* DOM re-renders (e.g. a
/// `MapWidget` tile arriving on a worker thread and re-invoking the `VirtualView`
/// in place). So `compute_display_list_damage` — which only diffs the parent —
/// reports "nothing changed", and `render_frame` would skip the frame, freezing
/// the child content. This compares each `VirtualView`'s child DL against the
/// previous frame's and returns the on-screen bounds of every one that differs,
/// so the caller can damage exactly those regions.
///
/// `current` / `previous` are keyed by the child `DomId` (the non-root entries
/// of `layout_results`). A child that is newly present or newly absent counts
/// as changed.
#[must_use] pub fn compute_virtual_view_damage(
    parent: &DisplayList,
    current: &std::collections::BTreeMap<azul_core::dom::DomId, std::sync::Arc<DisplayList>>,
    previous: &std::collections::BTreeMap<azul_core::dom::DomId, std::sync::Arc<DisplayList>>,
) -> Vec<LogicalRect> {
    let mut damage = Vec::new();
    for item in &parent.items {
        if let DisplayListItem::VirtualView { child_dom_id, bounds, .. } = item {
            let changed = match (current.get(child_dom_id), previous.get(child_dom_id)) {
                (Some(c), Some(p)) => {
                    // Same Arc → definitely unchanged (cheap fast-path).
                    !std::sync::Arc::ptr_eq(c, p) && !display_lists_visually_equal(c, p)
                }
                (Some(_), None) | (None, Some(_)) => true,
                (None, None) => false,
            };
            if changed {
                damage.push(*bounds.inner());
            }
        }
    }
    damage
}

/// Merge overlapping or adjacent damage rects to reduce overdraw.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
pub fn coalesce_damage_rects(rects: &mut Vec<LogicalRect>) {
    if rects.len() <= 1 {
        return;
    }

    // Simple O(n^2) merge — fine for typical damage counts (<20 rects)
    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < rects.len() {
            let mut j = i + 1;
            while j < rects.len() {
                // 8 logical pixels: merge rects that are close enough to avoid
                // many tiny damage regions that would cause redundant repaints —
                // BUT only when the merged box doesn't balloon the repaint. Two
                // PERPENDICULAR thin strips (e.g. a vertical + a horizontal
                // scrollbar meeting at a corner) are "adjacent" yet their bounding
                // box is the whole viewport: merging them turns ~3k px of overdraw
                // into ~20k. Reject a merge whose union is much larger than the two
                // rects combined; keep them separate instead.
                if rects_overlap_or_adjacent(&rects[i], &rects[j], 8.0) {
                    let u = union_rect(&rects[i], &rects[j]);
                    let area_u = (u.size.width * u.size.height).max(0.0);
                    let area_i = (rects[i].size.width * rects[i].size.height).max(0.0);
                    let area_j = (rects[j].size.width * rects[j].size.height).max(0.0);
                    // 1.5× slack covers genuine overlap (union < sum) and small-gap
                    // tiling (union ≈ sum) while rejecting perpendicular-strip bboxes.
                    if area_u <= (area_i + area_j) * 1.5 + 64.0 {
                        rects[i] = u;
                        rects.swap_remove(j);
                        changed = true;
                    } else {
                        j += 1;
                    }
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
}

#[must_use] pub fn rects_overlap_or_adjacent(a: &LogicalRect, b: &LogicalRect, gap: f32) -> bool {
    a.origin.x - gap <= b.origin.x + b.size.width
        && b.origin.x - gap <= a.origin.x + a.size.width
        && a.origin.y - gap <= b.origin.y + b.size.height
        && b.origin.y - gap <= a.origin.y + a.size.height
}

/// Compute damage rects for a grow-only window resize.
/// Returns the right strip and bottom strip that need rendering.
#[must_use] pub fn compute_resize_damage(
    old_width: f32,
    old_height: f32,
    new_width: f32,
    new_height: f32,
) -> Vec<LogicalRect> {
    let mut rects = Vec::new();
    if new_width > old_width {
        rects.push(LogicalRect {
            origin: LogicalPosition {
                x: old_width,
                y: 0.0,
            },
            size: LogicalSize {
                width: new_width - old_width,
                height: new_height,
            },
        });
    }
    if new_height > old_height {
        rects.push(LogicalRect {
            origin: LogicalPosition {
                x: 0.0,
                y: old_height,
            },
            size: LogicalSize {
                width: old_width.min(new_width),
                height: new_height - old_height,
            },
        });
    }
    rects
}

/// Compare a rectangular sub-region of two pixmaps pixel-by-pixel.
/// Returns the number of pixels that differ by more than `threshold` per channel.
#[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::many_single_char_names)] // domain-standard coordinate/geometry/short-lived names
#[must_use] pub fn compare_region(
    a: &AzulPixmap,
    b: &AzulPixmap,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    threshold: u8,
) -> usize {
    let mut diff_count = 0;
    for row in y..(y + h).min(a.height).min(b.height) {
        for col in x..(x + w).min(a.width).min(b.width) {
            let ai = (row * a.width + col) as usize * 4;
            let bi = (row * b.width + col) as usize * 4;
            if ai + 3 >= a.data.len() || bi + 3 >= b.data.len() {
                continue;
            }
            let dr = (i16::from(a.data[ai]) - i16::from(b.data[bi])).unsigned_abs() as u8;
            let dg = (i16::from(a.data[ai + 1]) - i16::from(b.data[bi + 1])).unsigned_abs() as u8;
            let db = (i16::from(a.data[ai + 2]) - i16::from(b.data[bi + 2])).unsigned_abs() as u8;
            if dr > threshold || dg > threshold || db > threshold {
                diff_count += 1;
            }
        }
    }
    diff_count
}

// ============================================================================
// scroll_shift_region — unit tests (#14 single-axis, #16 diagonal pan)
// ============================================================================
#[cfg(test)]
mod scroll_shift_tests {
    use super::*;
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

    /// Pixmap where every pixel encodes its own coords: R = x&0xFF, G = y&0xFF.
    /// After a shift, a pixel's (R,G) tells you which source pixel landed there,
    /// so we can assert the move is an exact translation.
    #[allow(clippy::many_single_char_names)] // domain-standard coordinate/geometry/short-lived names
    fn xy_pixmap(w: u32, h: u32) -> AzulPixmap {
        let mut p = AzulPixmap::new(w, h).unwrap();
        let d = p.data_mut();
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                d[i] = (x & 0xFF) as u8;
                d[i + 1] = (y & 0xFF) as u8;
                d[i + 2] = 0;
                d[i + 3] = 255;
            }
        }
        p
    }
    #[allow(clippy::many_single_char_names)] // domain-standard coordinate/geometry/short-lived names
    fn at(p: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
        let w = p.width();
        let d = p.data();
        let i = ((y * w + x) * 4) as usize;
        [d[i], d[i + 1], d[i + 2], d[i + 3]]
    }
    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition::new(x, y),
            size: LogicalSize::new(w, h),
        }
    }

    #[test]
    fn noop_when_delta_zero() {
        let mut p = xy_pixmap(64, 64);
        let strips = scroll_shift_region(&mut p, &rect(0.0, 0.0, 64.0, 64.0), (0.0, 0.0), (0.0, 0.0), 1.0);
        assert!(strips.is_empty(), "zero delta must not shift or expose anything");
        // Buffer untouched.
        assert_eq!(at(&p, 10, 20), [10, 20, 0, 255]);
    }

    #[test]
    #[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
    fn vertical_scroll_one_strip_and_translates() {
        let mut p = xy_pixmap(200, 100);
        // Scroll DOWN by 30 → content moves UP → bottom strip exposed.
        let strips = scroll_shift_region(&mut p, &rect(0.0, 0.0, 200.0, 100.0), (0.0, 30.0), (0.0, 30.0), 1.0);
        assert_eq!(strips.len(), 1, "single-axis scroll = one strip, got {strips:?}");
        let s = &strips[0];
        assert!(
            (s.origin.y - (100.0 - s.size.height)).abs() < 0.01 && s.size.width == 200.0,
            "vertical scroll-down must expose a full-width BOTTOM strip, got {s:?}"
        );
        // Kept region (top): (x, y) now holds original (x, y+30).
        assert_eq!(at(&p, 50, 10), [50, 40, 0, 255], "content not translated up by 30");
    }

    #[test]
    #[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
    #[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
    fn diagonal_pan_two_strips_and_translates() {
        let mut p = xy_pixmap(200, 100);
        // Diagonal scroll down-right by (20, 30): content moves up-left.
        let strips =
            scroll_shift_region(&mut p, &rect(0.0, 0.0, 200.0, 100.0), (20.0, 30.0), (20.0, 30.0), 1.0);
        assert_eq!(
            strips.len(),
            2,
            "diagonal pan must expose TWO strips (L-shape), got {strips:?}"
        );
        // One full-width strip (the vertical move) + one full-height strip (horizontal).
        let has_h_strip = strips.iter().any(|s| s.size.width == 200.0);
        let has_v_strip = strips.iter().any(|s| s.size.height == 100.0);
        assert!(
            has_h_strip && has_v_strip,
            "expected a full-width AND a full-height strip, got {strips:?}"
        );
        // Kept top-left region: (sx,sy) now holds original (sx+20, sy+30).
        // (50,40) is inside the kept block (bottom strip y>=69, right strip x>=179).
        let got = at(&p, 50, 40);
        assert_eq!(got[0], 70, "x not translated left by 20 (R channel)");
        assert_eq!(got[1], 70, "y not translated up by 30 (G channel)");
    }

    #[test]
    fn shift_only_touches_inside_clip() {
        let mut p = xy_pixmap(200, 100);
        // Clip is a sub-region; everything OUTSIDE must be byte-identical after.
        let clip = rect(8.0, 16.0, 180.0, 60.0); // phys [8,188) x [16,76)
        drop(scroll_shift_region(&mut p, &clip, (0.0, 10.0), (0.0, 10.0), 1.0));
        for &(x, y) in &[(0u32, 0u32), (199, 99), (100, 5), (100, 90), (2, 50), (190, 50)] {
            assert_eq!(
                at(&p, x, y),
                [(x & 0xFF) as u8, (y & 0xFF) as u8, 0, 255],
                "pixel ({x},{y}) OUTSIDE the clip was modified — scroll leaked past its frame"
            );
        }
        // Inside the kept region it DID move: (50,40) holds original (50,50).
        assert_eq!(at(&p, 50, 40), [50, 50, 0, 255], "inside-clip content not shifted");
    }

    #[test]
    #[allow(clippy::float_cmp)] // test asserts exact float equality on deterministic values
    fn shift_larger_than_region_returns_full_clip() {
        let mut p = xy_pixmap(64, 64);
        let clip = rect(0.0, 0.0, 64.0, 64.0);
        // Shift exceeds the region height → whole clip exposed (no partial strip).
        let strips = scroll_shift_region(&mut p, &clip, (0.0, 100.0), (0.0, 100.0), 1.0);
        assert_eq!(strips.len(), 1);
        assert_eq!(strips[0].size.width, 64.0);
        assert_eq!(strips[0].size.height, 64.0);
    }

    // --- #20 fast-path eligibility ---
    use crate::solver3::display_list::{
        BorderRadius, DisplayList, DisplayListItem, WindowLogicalRect,
    };
    use azul_css::props::basic::color::ColorU;

    fn dl(items: Vec<DisplayListItem>) -> DisplayList {
        DisplayList {
            items,
            node_mapping: Vec::new(),
            forced_page_breaks: Vec::new(),
            fixed_position_item_ranges: Vec::new(),
        }
    }
    fn wr(x: f32, y: f32, w: f32, h: f32) -> WindowLogicalRect {
        rect(x, y, w, h).into()
    }
    #[allow(clippy::many_single_char_names)] // domain-standard coordinate/geometry/short-lived names
    fn fill(x: f32, y: f32, w: f32, h: f32, a: u8) -> DisplayListItem {
        DisplayListItem::Rect {
            bounds: wr(x, y, w, h),
            color: ColorU { r: 10, g: 20, b: 30, a },
            border_radius: BorderRadius::default(),
        }
    }
    fn scroll_frame(id: u64) -> DisplayListItem {
        DisplayListItem::PushScrollFrame {
            clip_bounds: wr(0.0, 0.0, 100.0, 100.0),
            content_size: LogicalSize::new(100.0, 1000.0),
            scroll_id: id,
        }
    }

    #[test]
    fn eligible_when_no_backdrop_even_if_transparent() {
        // Transparent content, but nothing painted behind the frame → safe.
        let list = dl(vec![
            scroll_frame(7),
            fill(0.0, 0.0, 100.0, 30.0, 0), // transparent row
            DisplayListItem::PopScrollFrame,
        ]);
        assert!(scroll_fast_path_eligible(&list, 7, &rect(0.0, 0.0, 100.0, 100.0), (0.0, 0.0), (0.0, 0.0)));
    }

    #[test]
    fn eligible_when_backdrop_is_single_uniform_colour() {
        // A SINGLE flat colour covering the whole clip behind transparent content
        // drags invisibly (same colour everywhere) → aggressive policy keeps the
        // fast path. (This is the common body/container background case.)
        let list = dl(vec![
            fill(0.0, 0.0, 100.0, 100.0, 255), // one flat colour covering the clip
            scroll_frame(7),
            fill(0.0, 0.0, 100.0, 30.0, 0), // transparent content
            DisplayListItem::PopScrollFrame,
        ]);
        assert!(scroll_fast_path_eligible(&list, 7, &rect(0.0, 0.0, 100.0, 100.0), (0.0, 0.0), (0.0, 0.0)));
    }

    #[test]
    fn ineligible_when_backdrop_is_non_uniform() {
        // Two DIFFERENT colours behind transparent content → dragging them is
        // visible → must full-repaint.
        let mut left = fill(0.0, 0.0, 50.0, 100.0, 255);
        if let DisplayListItem::Rect { color, .. } = &mut left {
            *color = ColorU { r: 200, g: 0, b: 0, a: 255 };
        }
        let mut right = fill(50.0, 0.0, 50.0, 100.0, 255);
        if let DisplayListItem::Rect { color, .. } = &mut right {
            *color = ColorU { r: 0, g: 0, b: 200, a: 255 };
        }
        let list = dl(vec![
            left,
            right,
            scroll_frame(7),
            fill(0.0, 0.0, 100.0, 30.0, 0), // transparent content
            DisplayListItem::PopScrollFrame,
        ]);
        assert!(!scroll_fast_path_eligible(&list, 7, &rect(0.0, 0.0, 100.0, 100.0), (0.0, 0.0), (0.0, 0.0)));
    }

    #[test]
    fn ineligible_when_single_colour_only_partly_covers() {
        // One flat colour that covers only PART of the clip (rest is clear): its
        // edge against the clear would drag visibly → full-repaint.
        let list = dl(vec![
            fill(0.0, 0.0, 100.0, 40.0, 255), // covers only the top 40px
            scroll_frame(7),
            fill(0.0, 0.0, 100.0, 30.0, 0), // transparent content
            DisplayListItem::PopScrollFrame,
        ]);
        assert!(!scroll_fast_path_eligible(&list, 7, &rect(0.0, 0.0, 100.0, 100.0), (0.0, 0.0), (0.0, 0.0)));
    }

    #[test]
    fn eligible_when_backdrop_but_opaque_content_covers() {
        // Backdrop behind, but the scrolling content opaquely covers the clip →
        // nothing behind ever shows through → fast path is safe.
        let list = dl(vec![
            fill(0.0, 0.0, 100.0, 100.0, 255), // backdrop
            scroll_frame(7),
            fill(0.0, 0.0, 100.0, 1000.0, 255), // opaque full-content cover
            DisplayListItem::PopScrollFrame,
        ]);
        assert!(scroll_fast_path_eligible(&list, 7, &rect(0.0, 0.0, 100.0, 100.0), (0.0, 0.0), (0.0, 0.0)));
    }

    #[test]
    fn rect_covered_by_detects_gap() {
        let target = rect(0.0, 0.0, 100.0, 100.0);
        // Single full cover.
        assert!(rect_covered_by(&target, &[rect(0.0, 0.0, 100.0, 100.0)]));
        // Two halves tile it.
        assert!(rect_covered_by(
            &target,
            &[rect(0.0, 0.0, 100.0, 50.0), rect(0.0, 50.0, 100.0, 50.0)]
        ));
        // A gap in the middle is NOT covered.
        assert!(!rect_covered_by(
            &target,
            &[rect(0.0, 0.0, 100.0, 40.0), rect(0.0, 60.0, 100.0, 40.0)]
        ));
        // Empty → not covered.
        assert!(!rect_covered_by(&target, &[]));
    }
}

#[cfg(test)]
mod backdrop_filter_tests {
    use super::*;
    use azul_core::resources::RendererResources;
    use azul_css::props::basic::ColorU;
    use azul_css::props::style::filter::StyleFilter;
    use azul_css::props::basic::length::PercentageValue;
    use crate::solver3::display_list::DisplayList;
    use crate::cpurender::{CpuRenderState, ScrollOffsetMap};

    fn lrect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition::new(x, y),
            size: LogicalSize::new(w, h),
        }
    }
    // p/x/y/w/d/i are the conventional pixel-access short names
    #[allow(clippy::many_single_char_names)]
    fn px(p: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
        let w = p.width();
        let d = p.data();
        let i = ((y * w + x) * 4) as usize;
        [d[i], d[i + 1], d[i + 2], d[i + 3]]
    }

    /// A `backdrop-filter: invert(100%)` must invert the already-composited
    /// backdrop under the element, while leaving pixels outside the element box
    /// untouched.
    #[test]
    fn backdrop_filter_inverts_backdrop_region() {
        let w = 100u32;
        let h = 100u32;

        // Background: a solid blue rect over the whole canvas (root layer).
        // Then a backdrop-filter:invert region over the right half (no own
        // content), so its backdrop (blue) becomes inverted (yellow).
        let blue = ColorU { r: 0, g: 0, b: 255, a: 255 };
        let dl = DisplayList {
            items: vec![
                DisplayListItem::Rect {
                    bounds: lrect(0.0, 0.0, 100.0, 100.0).into(),
                    color: blue,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PushBackdropFilter {
                    bounds: lrect(50.0, 0.0, 50.0, 100.0).into(),
                    filters: vec![StyleFilter::Invert(PercentageValue::new(100.0))],
                },
                DisplayListItem::PopBackdropFilter,
            ],
            ..Default::default()
        };

        let mut comp = CompositorState::new(w, h);
        comp.allocate_layers_from_display_list(&dl, 1.0);

        // A backdrop-filter layer must have been allocated.
        assert!(
            comp.layers.values().any(|l| l.is_backdrop_filter),
            "no backdrop-filter layer allocated"
        );

        let rr = RendererResources::default();
        let mut gc = GlyphCache::new();
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        comp.render_layers(&dl, 1.0, &rr, None, &mut gc, &state).unwrap();

        let mut out = AzulPixmap::new(w, h).unwrap();
        out.fill(0, 0, 0, 255);
        comp.composite_frame(&mut out, 1.0);

        // Left half: untouched blue backdrop.
        let left = px(&out, 10, 50);
        assert_eq!(left, [0, 0, 255, 255], "left half should stay blue");

        // Right half: blue inverted -> (255,255,0).
        let right = px(&out, 75, 50);
        assert!(
            right[0] > 200 && right[1] > 200 && right[2] < 60,
            "right half backdrop should be inverted to yellow, got {right:?}"
        );
    }
}

// ============================================================================
// Adversarial unit tests (autotest fleet)
//
// Focus: the compositor's numeric edges — NaN/inf/negative/zero `dpi_factor`,
// saturating float→int casts, clip/region clamping, damage-rect arithmetic and
// the malformed-display-list paths that the doc comments claim panic.
// ============================================================================
#[cfg(test)]
#[allow(clippy::float_cmp)] // deterministic float values: exact compare is the assertion
#[allow(clippy::many_single_char_names)] // domain-standard coordinate/pixel short names
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry short names
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, BTreeSet, HashMap},
        sync::Arc,
    };

    use azul_core::{
        dom::DomId,
        resources::{RendererResources, TransformKey},
        transform::ComputedTransform3D,
    };
    use azul_css::{
        props::basic::{angle::AngleValue, color::ColorU, length::PercentageValue,
            pixel::PixelValue},
        props::style::filter::{StyleBlur, StyleFilter},
    };

    use super::*;
    use crate::{
        cpurender::{CpuRenderState, ScrollOffsetMap},
        solver3::display_list::{
            BorderRadius, DisplayList, DisplayListItem, WindowLogicalRect,
        },
    };

    // ---------------------------------------------------------------- helpers

    fn lr(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition::new(x, y),
            size: LogicalSize::new(w, h),
        }
    }
    fn wlr(x: f32, y: f32, w: f32, h: f32) -> WindowLogicalRect {
        lr(x, y, w, h).into()
    }
    fn dlist(items: Vec<DisplayListItem>) -> DisplayList {
        DisplayList {
            items,
            ..Default::default()
        }
    }
    fn rect_item(x: f32, y: f32, w: f32, h: f32, color: ColorU) -> DisplayListItem {
        DisplayListItem::Rect {
            bounds: wlr(x, y, w, h),
            color,
            border_radius: BorderRadius::default(),
        }
    }
    fn opaque_rect(x: f32, y: f32, w: f32, h: f32) -> DisplayListItem {
        rect_item(x, y, w, h, ColorU { r: 10, g: 20, b: 30, a: 255 })
    }
    fn push_scroll(id: u64, x: f32, y: f32, w: f32, h: f32) -> DisplayListItem {
        DisplayListItem::PushScrollFrame {
            clip_bounds: wlr(x, y, w, h),
            content_size: LogicalSize::new(w, h * 10.0),
            scroll_id: id,
        }
    }
    /// Pixmap where each pixel encodes its own coordinates (R = x, G = y), so a
    /// shift can be checked as an exact translation.
    fn xy_map(w: u32, h: u32) -> AzulPixmap {
        let mut p = AzulPixmap::new(w, h).unwrap();
        let d = p.data_mut();
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                d[i] = (x & 0xFF) as u8;
                d[i + 1] = (y & 0xFF) as u8;
                d[i + 2] = 0;
                d[i + 3] = 255;
            }
        }
        p
    }
    fn solid(w: u32, h: u32, c: [u8; 4]) -> AzulPixmap {
        let mut p = AzulPixmap::new(w, h).unwrap();
        p.fill(c[0], c[1], c[2], c[3]);
        p
    }
    fn at(p: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
        let w = p.width();
        let d = p.data();
        let i = ((y * w + x) * 4) as usize;
        [d[i], d[i + 1], d[i + 2], d[i + 3]]
    }
    fn render_deps() -> (RendererResources, GlyphCache, CpuRenderState) {
        (
            RendererResources::default(),
            GlyphCache::new(),
            CpuRenderState::new(ScrollOffsetMap::new()),
        )
    }

    // ============================== CompositorState::new (constructor) =======

    #[test]
    fn compositor_new_zero_viewport_does_not_panic() {
        // AzulPixmap::new(0, 0) returns None — Layer::new must clamp to 1×1
        // instead of unwrapping a None.
        let c = CompositorState::new(0, 0);
        assert_eq!(c.layers.len(), 1, "only the root layer exists after new()");
        assert_eq!(c.root_layer, LayerId(0));
        let root = c.layers.get(&c.root_layer).unwrap();
        assert_eq!(
            (root.pixbuf.width(), root.pixbuf.height()),
            (1, 1),
            "a 0×0 viewport must degrade to a 1×1 pixbuf, not an empty/absent one"
        );
        assert_eq!(root.bounds.size.width, 0.0);
        assert_eq!(root.bounds.size.height, 0.0);
    }

    #[test]
    fn compositor_new_invariants_hold() {
        let c = CompositorState::new(800, 600);
        assert_eq!(c.layers.len(), 1);
        assert_eq!(c.next_layer_id_peek(), 1, "root consumes id 0");
        assert!(c.previous_positions.is_empty());
        let root = c.layers.get(&c.root_layer).unwrap();
        assert_eq!(root.id, LayerId(0));
        assert_eq!(root.bounds.size.width, 800.0);
        assert_eq!(root.bounds.size.height, 600.0);
        assert_eq!((root.pixbuf.width(), root.pixbuf.height()), (800, 600));
        assert_eq!(root.pixbuf.data().len(), 800 * 600 * 4);
        assert_eq!(at(&root.pixbuf, 0, 0), [255, 255, 255, 255], "root starts opaque white");
        assert_eq!(root.opacity, 1.0);
        assert!(root.children.is_empty());
        assert!(root.damage.is_empty());
    }

    // ============================== alloc_layer_id / next_layer_id_peek ======

    #[test]
    fn alloc_layer_id_is_unique_and_monotonic() {
        let mut c = CompositorState::new(4, 4);
        let ids: Vec<LayerId> = (0..1000).map(|_| c.alloc_layer_id()).collect();
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(*id, LayerId(i as u64 + 1), "ids must be dense + monotonic");
        }
        assert_eq!(c.next_layer_id_peek(), 1001);
    }

    #[test]
    fn next_layer_id_peek_is_side_effect_free() {
        let mut c = CompositorState::new(4, 4);
        let before = c.next_layer_id_peek();
        assert_eq!(before, c.next_layer_id_peek(), "peek must not mutate the counter");
        let _ = c.alloc_layer_id();
        assert_eq!(c.next_layer_id_peek(), before + 1);
    }

    #[test]
    fn alloc_layer_id_at_u64_max_boundary() {
        // The last id that can be handed out without overflowing the counter.
        let mut c = CompositorState::new(4, 4);
        c.next_layer_id = u64::MAX - 1;
        assert_eq!(c.alloc_layer_id(), LayerId(u64::MAX - 1));
        assert_eq!(c.next_layer_id_peek(), u64::MAX);
    }

    // ============================== Layer::new (constructor) =================

    #[test]
    fn layer_new_zero_pixels_clamps_to_1x1_and_sets_defaults() {
        let l = Layer::new(LayerId(9), lr(1.0, 2.0, 3.0, 4.0), 0, 0);
        assert_eq!(l.id, LayerId(9));
        assert_eq!((l.pixbuf.width(), l.pixbuf.height()), (1, 1));
        assert_eq!(l.bounds.origin.x, 1.0);
        assert_eq!(l.bounds.size.height, 4.0);
        assert_eq!(l.opacity, 1.0);
        assert_eq!(l.scroll_offset, (0.0, 0.0));
        assert_eq!(l.display_list_range, (0, 0));
        assert!(l.damage.is_empty());
        assert!(l.children.is_empty());
        assert!(l.filters.is_empty());
        assert!(!l.is_backdrop_filter);
        assert!(l.scroll_id.is_none());
        assert!(l.composite_dirty);
        assert!(l.transform.is_identity(IDENTITY_EPSILON_F64));
    }

    #[test]
    fn layer_new_with_nan_bounds_does_not_panic() {
        let nan_bounds = lr(f32::NAN, f32::NAN, f32::NAN, f32::NAN);
        let l = Layer::new(LayerId(1), nan_bounds, 2, 3);
        assert_eq!((l.pixbuf.width(), l.pixbuf.height()), (2, 3));
        assert!(l.bounds.size.width.is_nan(), "bounds are stored verbatim");
    }

    // ============================== find_matching_pop =======================

    #[test]
    fn find_matching_pop_on_empty_items_returns_len() {
        let items: Vec<DisplayListItem> = Vec::new();
        assert_eq!(find_matching_pop(&items, 0, MatchKind::ScrollFrame), 0);
    }

    #[test]
    fn find_matching_pop_start_past_end_returns_len() {
        // `skip(start + 1)` past the end must yield an empty iterator, not panic.
        let items = vec![push_scroll(1, 0.0, 0.0, 10.0, 10.0), DisplayListItem::PopScrollFrame];
        assert_eq!(find_matching_pop(&items, 10, MatchKind::ScrollFrame), 2);
        assert_eq!(find_matching_pop(&items, 1_000_000, MatchKind::Opacity), 2);
    }

    #[test]
    fn find_matching_pop_unmatched_push_returns_len() {
        let items = vec![push_scroll(1, 0.0, 0.0, 10.0, 10.0), opaque_rect(0.0, 0.0, 5.0, 5.0)];
        assert_eq!(
            find_matching_pop(&items, 0, MatchKind::ScrollFrame),
            2,
            "a Push with no Pop must clamp to items.len()"
        );
    }

    #[test]
    fn find_matching_pop_respects_nesting() {
        let items = vec![
            push_scroll(1, 0.0, 0.0, 10.0, 10.0),
            push_scroll(2, 0.0, 0.0, 5.0, 5.0),
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PopScrollFrame,
        ];
        assert_eq!(find_matching_pop(&items, 0, MatchKind::ScrollFrame), 3, "outer pop");
        assert_eq!(find_matching_pop(&items, 1, MatchKind::ScrollFrame), 2, "inner pop");
    }

    #[test]
    fn find_matching_pop_ignores_other_kinds() {
        let items = vec![
            push_scroll(1, 0.0, 0.0, 10.0, 10.0),
            DisplayListItem::PopOpacity,
            DisplayListItem::PopFilter,
            DisplayListItem::PopScrollFrame,
        ];
        assert_eq!(find_matching_pop(&items, 0, MatchKind::ScrollFrame), 3);
    }

    #[test]
    fn find_matching_pop_extra_pops_do_not_underflow_depth() {
        // depth is a u32: a stream of stray Pops must not wrap it below zero.
        let items = vec![
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PopScrollFrame,
        ];
        assert_eq!(find_matching_pop(&items, 0, MatchKind::ScrollFrame), 1);
    }

    // ============================== compute_exposed_rects ===================

    #[test]
    fn compute_exposed_rects_zero_and_subpixel_delta_expose_nothing() {
        let b = lr(0.0, 0.0, 200.0, 100.0);
        assert!(compute_exposed_rects(&b, 0.0, 0.0).is_empty());
        assert!(compute_exposed_rects(&b, 0.49, -0.49).is_empty(), "|d| <= 0.5 is a no-op");
    }

    #[test]
    fn compute_exposed_rects_nan_delta_exposes_nothing() {
        let b = lr(0.0, 0.0, 200.0, 100.0);
        // NaN.abs() > 0.5 is false — no strip, no panic.
        assert!(compute_exposed_rects(&b, f32::NAN, f32::NAN).is_empty());
    }

    #[test]
    fn compute_exposed_rects_clamps_strip_to_bounds() {
        let b = lr(0.0, 0.0, 200.0, 100.0);
        let r = compute_exposed_rects(&b, 0.0, 1000.0);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].size.height, 100.0, "strip cannot exceed the frame height");
        assert_eq!(r[0].size.width, 200.0);
    }

    #[test]
    fn compute_exposed_rects_infinite_delta_saturates_to_bounds() {
        let b = lr(0.0, 0.0, 200.0, 100.0);
        let pos = compute_exposed_rects(&b, 0.0, f32::INFINITY);
        assert_eq!(pos.len(), 1);
        assert_eq!(pos[0].size.height, 100.0, "+inf must clamp via min(h), not stay inf");
        assert!(pos[0].origin.y.is_finite());

        let neg = compute_exposed_rects(&b, 0.0, f32::NEG_INFINITY);
        assert_eq!(neg.len(), 1);
        assert_eq!(neg[0].size.height, 100.0);
        assert!(!neg[0].size.height.is_nan());
    }

    #[test]
    fn compute_exposed_rects_diagonal_yields_two_strips() {
        let b = lr(0.0, 0.0, 200.0, 100.0);
        let r = compute_exposed_rects(&b, -10.0, 10.0);
        assert_eq!(r.len(), 2, "diagonal scroll = vertical strip + horizontal strip");
        assert!(r.iter().any(|s| s.size.width == 200.0), "full-width strip");
        assert!(r.iter().any(|s| s.size.height == 100.0), "full-height strip");
    }

    // ============================== scroll_shift_region =====================

    #[test]
    fn scroll_shift_zero_dpi_is_a_noop() {
        let mut p = xy_map(64, 64);
        let strips = scroll_shift_region(&mut p, &lr(0.0, 0.0, 64.0, 64.0), (0.0, 30.0), (0.0, 30.0), 0.0);
        assert!(strips.is_empty(), "dpi 0 → 0 physical px moved → nothing exposed");
        assert_eq!(at(&p, 10, 20), [10, 20, 0, 255], "buffer must be untouched");
    }

    #[test]
    fn scroll_shift_nan_dpi_is_a_noop() {
        let mut p = xy_map(64, 64);
        let strips =
            scroll_shift_region(&mut p, &lr(0.0, 0.0, 64.0, 64.0), (0.0, 30.0), (0.0, 30.0), f32::NAN);
        assert!(strips.is_empty(), "NaN casts to 0 px — no move, no exposure");
        assert_eq!(at(&p, 10, 20), [10, 20, 0, 255]);
    }

    #[test]
    fn scroll_shift_nan_offsets_are_a_noop() {
        let mut p = xy_map(64, 64);
        let strips = scroll_shift_region(
            &mut p,
            &lr(0.0, 0.0, 64.0, 64.0),
            (f32::NAN, f32::NAN),
            (f32::NAN, f32::NAN),
            1.0,
        );
        assert!(strips.is_empty());
        assert_eq!(at(&p, 10, 20), [10, 20, 0, 255]);
    }

    #[test]
    fn scroll_shift_negative_dpi_is_a_noop() {
        // A negative scale collapses the clamped clip region to zero width.
        let mut p = xy_map(64, 64);
        let strips =
            scroll_shift_region(&mut p, &lr(0.0, 0.0, 64.0, 64.0), (0.0, 10.0), (0.0, 10.0), -1.0);
        assert!(strips.is_empty(), "negative dpi → empty region → no move");
        assert_eq!(at(&p, 10, 20), [10, 20, 0, 255]);
    }

    #[test]
    fn scroll_shift_clip_entirely_outside_pixmap_is_a_noop() {
        let mut p = xy_map(64, 64);
        let strips =
            scroll_shift_region(&mut p, &lr(500.0, 500.0, 10.0, 10.0), (0.0, 10.0), (0.0, 10.0), 1.0);
        assert!(strips.is_empty(), "off-screen clip clamps to an empty region");
        assert_eq!(at(&p, 63, 63), [63, 63, 0, 255]);
    }

    #[test]
    fn scroll_shift_huge_offset_returns_whole_clip() {
        let mut p = xy_map(64, 64);
        let clip = lr(0.0, 0.0, 64.0, 64.0);
        let strips = scroll_shift_region(&mut p, &clip, (0.0, 1.0e9), (0.0, 1.0e9), 1.0);
        assert_eq!(strips.len(), 1, "shift ≥ region → caller repaints the whole clip");
        assert_eq!(strips[0].size.width, 64.0);
        assert_eq!(strips[0].size.height, 64.0);
        assert_eq!(at(&p, 10, 20), [10, 20, 0, 255], "memmove is skipped entirely");
    }

    #[test]
    fn scroll_shift_infinite_dpi_returns_whole_clip() {
        let mut p = xy_map(64, 64);
        let clip = lr(0.0, 0.0, 64.0, 64.0);
        // inf saturates the px delta to i32::MAX → "exceeds region" branch.
        let strips = scroll_shift_region(&mut p, &clip, (0.0, 10.0), (0.0, 10.0), f32::INFINITY);
        assert_eq!(strips.len(), 1);
        assert_eq!(strips[0].size.height, 64.0);
    }

    #[test]
    fn scroll_shift_clip_larger_than_pixmap_keeps_strip_on_screen() {
        // Regression guard for the documented "stale duplicated band": the strip
        // must come from the CLAMPED region, so it always lands inside the pixmap.
        let mut p = xy_map(64, 64);
        let clip = lr(-100.0, -100.0, 400.0, 400.0);
        let strips = scroll_shift_region(&mut p, &clip, (0.0, 10.0), (0.0, 10.0), 1.0);
        assert_eq!(strips.len(), 1);
        let s = &strips[0];
        assert_eq!(s.origin.x, 0.0);
        assert_eq!(s.size.width, 64.0);
        assert!(
            s.origin.y >= 0.0 && s.origin.y + s.size.height <= 64.0,
            "exposed strip must stay inside the pixmap, got {s:?}"
        );
        assert_eq!(at(&p, 50, 10), [50, 20, 0, 255], "content translated up by 10");
    }

    #[test]
    fn scroll_shift_rounds_absolute_offsets_not_the_delta() {
        // The doc promises round(new·dpi) − round(prev·dpi): at dpi 2 a +0.25
        // logical step is 0.5 physical px, which must NOT round to 1 px each frame.
        let mut p = xy_map(64, 64);
        let clip = lr(0.0, 0.0, 32.0, 32.0);
        let strips = scroll_shift_region(&mut p, &clip, (0.0, 0.25), (0.0, 10.25), 2.0);
        // round(10.25*2)=21 (round-half-away-from-zero on .5), round(10.0*2)=20 → 1px.
        assert_eq!(strips.len(), 1, "a 1px move exposes exactly one strip");
        assert_eq!(at(&p, 5, 0), [5, 1, 0, 255], "moved up by exactly 1 physical px");
    }

    // ============================== shift_*_1d / shift_diagonal_2d ==========

    #[test]
    fn shift_vertical_1d_zero_delta_is_a_noop() {
        let mut p = xy_map(8, 8);
        let before = p.data().to_vec();
        shift_vertical_1d(p.data_mut(), 8, 0, 0, 8, 8, 0);
        assert_eq!(p.data(), &before[..], "px_dy = 0 must not touch a single byte");
    }

    #[test]
    fn shift_vertical_1d_delta_larger_than_region_is_a_noop() {
        let mut p = xy_map(8, 8);
        let before = p.data().to_vec();
        shift_vertical_1d(p.data_mut(), 8, 0, 0, 8, 8, 100);
        assert_eq!(p.data(), &before[..], "|px_dy| ≥ height → empty row range, no panic");
        shift_vertical_1d(p.data_mut(), 8, 0, 0, 8, 8, -100);
        assert_eq!(p.data(), &before[..]);
    }

    #[test]
    fn shift_vertical_1d_moves_content_up_and_down() {
        let mut p = xy_map(8, 8);
        shift_vertical_1d(p.data_mut(), 8, 0, 0, 8, 8, 2); // content up by 2
        assert_eq!(at(&p, 3, 0), [3, 2, 0, 255], "row 0 now holds original row 2");
        assert_eq!(at(&p, 3, 5), [3, 7, 0, 255], "row 5 now holds original row 7");

        let mut q = xy_map(8, 8);
        shift_vertical_1d(q.data_mut(), 8, 0, 0, 8, 8, -3); // content down by 3
        assert_eq!(at(&q, 3, 7), [3, 4, 0, 255], "row 7 now holds original row 4");
        assert_eq!(at(&q, 3, 3), [3, 0, 0, 255], "row 3 now holds original row 0");
    }

    #[test]
    fn shift_horizontal_1d_zero_delta_is_a_noop() {
        let mut p = xy_map(8, 8);
        let before = p.data().to_vec();
        shift_horizontal_1d(p.data_mut(), 8, 0, 0, 8, 8, 0);
        assert_eq!(p.data(), &before[..]);
    }

    #[test]
    fn shift_horizontal_1d_max_valid_shift_keeps_one_column() {
        // px_dx = region_w - 1 is the largest shift scroll_shift_region can pass
        // through (it early-returns at >= region_w): the copy must not underflow.
        let mut p = xy_map(8, 8);
        shift_horizontal_1d(p.data_mut(), 8, 0, 0, 8, 8, 7);
        assert_eq!(at(&p, 0, 4), [7, 4, 0, 255], "col 0 now holds original col 7");

        let mut q = xy_map(8, 8);
        shift_horizontal_1d(q.data_mut(), 8, 0, 0, 8, 8, -7);
        assert_eq!(at(&q, 7, 4), [0, 4, 0, 255], "col 7 now holds original col 0");
    }

    #[test]
    fn shift_horizontal_1d_only_touches_the_clip_columns() {
        let mut p = xy_map(8, 8);
        shift_horizontal_1d(p.data_mut(), 8, 2, 1, 6, 3, 1);
        // Outside the clip rows/cols: untouched.
        assert_eq!(at(&p, 0, 0), [0, 0, 0, 255]);
        assert_eq!(at(&p, 7, 2), [7, 2, 0, 255], "col 7 is outside cx0..cx1");
        assert_eq!(at(&p, 3, 7), [3, 7, 0, 255], "row 7 is outside cy0..cy1");
        // Inside: shifted left by 1.
        assert_eq!(at(&p, 2, 1), [3, 1, 0, 255]);
    }

    #[test]
    fn shift_diagonal_2d_full_width_shift_is_a_noop() {
        let mut p = xy_map(8, 8);
        let before = p.data().to_vec();
        shift_diagonal_2d(p.data_mut(), 8, 0, 0, 8, 8, 8, 1); // span_cols == 0
        assert_eq!(p.data(), &before[..], "nothing left to keep → early return");
    }

    #[test]
    fn shift_diagonal_2d_translates_both_axes_in_one_pass() {
        let mut p = xy_map(8, 8);
        shift_diagonal_2d(p.data_mut(), 8, 0, 0, 8, 8, 2, 3); // content up-left
        assert_eq!(at(&p, 0, 0), [2, 3, 0, 255], "(0,0) holds original (2,3)");
        assert_eq!(at(&p, 5, 4), [7, 7, 0, 255], "(5,4) holds original (7,7)");

        let mut q = xy_map(8, 8);
        shift_diagonal_2d(q.data_mut(), 8, 0, 0, 8, 8, -2, -3); // content down-right
        assert_eq!(at(&q, 7, 7), [5, 4, 0, 255], "(7,7) holds original (5,4)");
    }

    // ============================== rect_covered_by =========================

    #[test]
    fn rect_covered_by_empty_covers_is_false() {
        assert!(!rect_covered_by(&lr(0.0, 0.0, 10.0, 10.0), &[]));
    }

    #[test]
    fn rect_covered_by_degenerate_target_is_vacuously_covered() {
        let cover = [lr(0.0, 0.0, 10.0, 10.0)];
        // No sample points → the "every sample is inside" predicate holds.
        assert!(rect_covered_by(&lr(0.0, 0.0, 0.0, 0.0), &cover), "zero-size target");
        assert!(rect_covered_by(&lr(0.0, 0.0, -50.0, -50.0), &cover), "negative-size target");
        assert!(
            rect_covered_by(&lr(0.0, 0.0, f32::NAN, f32::NAN), &cover),
            "NaN target must terminate (no infinite while-float loop) and be defined"
        );
    }

    #[test]
    fn rect_covered_by_tolerates_sub_4px_gaps_but_not_wide_ones() {
        let target = lr(0.0, 0.0, 100.0, 100.0);
        // 1px gap at y ∈ [49, 50) — no 4px sample lands in it → still "covered".
        assert!(rect_covered_by(
            &target,
            &[lr(0.0, 0.0, 100.0, 49.0), lr(0.0, 50.0, 100.0, 50.0)]
        ));
        // 20px gap → sampled → not covered.
        assert!(!rect_covered_by(
            &target,
            &[lr(0.0, 0.0, 100.0, 40.0), lr(0.0, 60.0, 100.0, 40.0)]
        ));
    }

    // ============================== rects_overlap_or_adjacent ===============

    #[test]
    fn rects_touching_with_zero_gap_are_adjacent() {
        let a = lr(0.0, 0.0, 10.0, 10.0);
        let b = lr(10.0, 0.0, 10.0, 10.0);
        assert!(rects_overlap_or_adjacent(&a, &b, 0.0), "shared edge counts as adjacent");
        assert!(!rects_overlap_or_adjacent(&a, &lr(11.0, 0.0, 10.0, 10.0), 0.0));
    }

    #[test]
    fn rects_overlap_negative_gap_requires_real_overlap() {
        let a = lr(0.0, 0.0, 10.0, 10.0);
        let b = lr(10.0, 0.0, 10.0, 10.0);
        assert!(
            !rects_overlap_or_adjacent(&a, &b, -1.0),
            "a negative gap must SHRINK the test, not widen it"
        );
        assert!(rects_overlap_or_adjacent(&a, &lr(5.0, 0.0, 10.0, 10.0), -1.0));
    }

    #[test]
    fn rects_overlap_nan_inputs_are_false_not_panics() {
        let a = lr(0.0, 0.0, 10.0, 10.0);
        let nan = lr(f32::NAN, f32::NAN, f32::NAN, f32::NAN);
        assert!(!rects_overlap_or_adjacent(&a, &nan, 0.0), "NaN compares false everywhere");
        assert!(!rects_overlap_or_adjacent(&a, &a, f32::NAN), "NaN gap → false");
    }

    #[test]
    fn rects_overlap_infinite_gap_swallows_everything() {
        let a = lr(0.0, 0.0, 1.0, 1.0);
        let b = lr(1.0e9, 1.0e9, 1.0, 1.0);
        assert!(rects_overlap_or_adjacent(&a, &b, f32::INFINITY));
    }

    // ============================== coalesce_damage_rects ===================

    #[test]
    fn coalesce_empty_and_single_are_untouched() {
        let mut v: Vec<LogicalRect> = Vec::new();
        coalesce_damage_rects(&mut v);
        assert!(v.is_empty());
        let mut one = vec![lr(1.0, 2.0, 3.0, 4.0)];
        coalesce_damage_rects(&mut one);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].origin.x, 1.0);
    }

    #[test]
    fn coalesce_merges_identical_and_chained_rects() {
        let mut v = vec![lr(0.0, 0.0, 10.0, 10.0), lr(0.0, 0.0, 10.0, 10.0)];
        coalesce_damage_rects(&mut v);
        assert_eq!(v.len(), 1, "duplicates must collapse");

        let mut chain = vec![
            lr(0.0, 0.0, 10.0, 10.0),
            lr(5.0, 0.0, 10.0, 10.0),
            lr(10.0, 0.0, 10.0, 10.0),
        ];
        coalesce_damage_rects(&mut chain);
        assert_eq!(chain.len(), 1, "an overlapping chain collapses transitively");
        assert_eq!(chain[0].size.width, 20.0);
    }

    #[test]
    fn coalesce_keeps_distant_rects_separate() {
        let mut v = vec![lr(0.0, 0.0, 10.0, 10.0), lr(500.0, 500.0, 10.0, 10.0)];
        coalesce_damage_rects(&mut v);
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn coalesce_rejects_perpendicular_strip_union() {
        // The documented anti-ballooning guard: a vertical + a horizontal
        // scrollbar strip touch at a corner but their union is the viewport.
        let mut v = vec![lr(990.0, 0.0, 10.0, 1000.0), lr(0.0, 990.0, 1000.0, 10.0)];
        coalesce_damage_rects(&mut v);
        assert_eq!(v.len(), 2, "union would be 100× the painted area — must stay split");
    }

    #[test]
    fn coalesce_with_nan_rects_terminates() {
        // NaN comparisons are always false → no merge, and the fixpoint loop
        // must still terminate rather than spin.
        let mut v = vec![
            lr(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            lr(0.0, 0.0, 10.0, 10.0),
            lr(f32::NAN, 0.0, 10.0, 10.0),
        ];
        coalesce_damage_rects(&mut v);
        assert_eq!(v.len(), 3);
    }

    // ============================== compute_resize_damage ===================

    #[test]
    fn resize_damage_shrink_or_equal_is_empty() {
        assert!(compute_resize_damage(100.0, 100.0, 100.0, 100.0).is_empty());
        assert!(compute_resize_damage(100.0, 100.0, 50.0, 50.0).is_empty(), "grow-only");
    }

    #[test]
    fn resize_damage_grow_both_axes_gives_right_and_bottom_strips() {
        let r = compute_resize_damage(100.0, 100.0, 200.0, 150.0);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].origin.x, 100.0, "right strip starts at the old width");
        assert_eq!(r[0].size.width, 100.0);
        assert_eq!(r[0].size.height, 150.0, "right strip spans the NEW height");
        assert_eq!(r[1].origin.y, 100.0, "bottom strip starts at the old height");
        assert_eq!(r[1].size.width, 100.0, "bottom strip is min(old, new) wide — no overdraw");
        assert_eq!(r[1].size.height, 50.0);
    }

    #[test]
    fn resize_damage_zero_and_nan_are_defined() {
        assert!(compute_resize_damage(0.0, 0.0, 0.0, 0.0).is_empty());
        assert_eq!(compute_resize_damage(0.0, 0.0, 10.0, 10.0).len(), 2);
        assert!(
            compute_resize_damage(f32::NAN, f32::NAN, f32::NAN, f32::NAN).is_empty(),
            "NaN > NaN is false → no damage, no panic"
        );
        assert!(compute_resize_damage(f32::NAN, f32::NAN, 10.0, 10.0).is_empty());
    }

    #[test]
    fn resize_damage_infinite_new_size_does_not_panic() {
        let r = compute_resize_damage(0.0, 0.0, f32::INFINITY, f32::INFINITY);
        assert_eq!(r.len(), 2);
        assert!(r[0].size.width.is_infinite(), "inf propagates, but nothing panics");
    }

    // ============================== compare_region ==========================

    #[test]
    fn compare_region_identical_pixmaps_report_zero() {
        let a = solid(4, 4, [1, 2, 3, 255]);
        let b = solid(4, 4, [1, 2, 3, 255]);
        assert_eq!(compare_region(&a, &b, 0, 0, 4, 4, 0), 0);
    }

    #[test]
    fn compare_region_threshold_255_never_counts_a_pixel() {
        // Max per-channel distance is 255, and the test is strictly `>`.
        let a = solid(4, 4, [0, 0, 0, 255]);
        let b = solid(4, 4, [255, 255, 255, 255]);
        assert_eq!(compare_region(&a, &b, 0, 0, 4, 4, 255), 0, "saturated threshold = blind");
        assert_eq!(compare_region(&a, &b, 0, 0, 4, 4, 254), 16, "one below → every pixel");
        assert_eq!(compare_region(&a, &b, 0, 0, 4, 4, 0), 16);
    }

    #[test]
    fn compare_region_ignores_the_alpha_channel() {
        let a = solid(4, 4, [9, 9, 9, 255]);
        let b = solid(4, 4, [9, 9, 9, 0]);
        assert_eq!(compare_region(&a, &b, 0, 0, 4, 4, 0), 0, "only RGB is compared");
    }

    #[test]
    fn compare_region_clamps_oversized_and_empty_regions() {
        let a = solid(4, 4, [0, 0, 0, 255]);
        let b = solid(4, 4, [255, 255, 255, 255]);
        assert_eq!(
            compare_region(&a, &b, 0, 0, u32::MAX, u32::MAX, 0),
            16,
            "w/h are clamped to the pixmaps — no OOB, no overflow"
        );
        assert_eq!(compare_region(&a, &b, 0, 0, 0, 0, 0), 0, "empty region");
        assert_eq!(
            compare_region(&a, &b, u32::MAX, u32::MAX, 0, 0, 0),
            0,
            "origin at u32::MAX with a zero extent must not overflow x + w"
        );
    }

    #[test]
    fn compare_region_with_mismatched_pixmap_sizes_uses_the_overlap() {
        let a = solid(4, 4, [0, 0, 0, 255]);
        let b = solid(2, 2, [255, 255, 255, 255]);
        assert_eq!(
            compare_region(&a, &b, 0, 0, 4, 4, 0),
            4,
            "iteration clamps to min(a, b) — the 2×2 overlap"
        );
    }

    // ============================== opaque_fill_rect ========================

    #[test]
    fn opaque_fill_rect_accepts_only_opaque_square_rects() {
        assert!(opaque_fill_rect(&opaque_rect(1.0, 2.0, 3.0, 4.0)).is_some());
        assert!(
            opaque_fill_rect(&rect_item(0.0, 0.0, 1.0, 1.0, ColorU { r: 0, g: 0, b: 0, a: 254 }))
                .is_none(),
            "a = 254 is not fully opaque"
        );
        let rounded = DisplayListItem::Rect {
            bounds: wlr(0.0, 0.0, 10.0, 10.0),
            color: ColorU { r: 0, g: 0, b: 0, a: 255 },
            border_radius: BorderRadius {
                top_left: 0.1,
                top_right: 0.0,
                bottom_left: 0.0,
                bottom_right: 0.0,
            },
        };
        assert!(opaque_fill_rect(&rounded).is_none(), "any corner radius disqualifies");
        assert!(opaque_fill_rect(&DisplayListItem::PopScrollFrame).is_none());
        let b = opaque_fill_rect(&opaque_rect(1.0, 2.0, 3.0, 4.0)).unwrap();
        assert_eq!((b.origin.x, b.size.height), (1.0, 4.0));
    }

    // ============================== scroll_fast_path_eligible ===============

    #[test]
    fn fast_path_eligible_when_no_such_frame() {
        assert!(scroll_fast_path_eligible(&dlist(vec![]), 3, &lr(0.0, 0.0, 10.0, 10.0), (0.0, 0.0), (0.0, 0.0)));
    }

    #[test]
    fn fast_path_ineligible_for_a_nested_frame() {
        // Inner clip_bounds are in the OUTER frame's content space → memmove
        // would shift the wrong region.
        let list = dlist(vec![
            push_scroll(1, 0.0, 0.0, 100.0, 100.0),
            push_scroll(2, 0.0, 0.0, 50.0, 50.0),
            opaque_rect(0.0, 0.0, 50.0, 500.0),
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PopScrollFrame,
        ]);
        assert!(
            !scroll_fast_path_eligible(&list, 2, &lr(0.0, 0.0, 50.0, 50.0), (0.0, 0.0), (0.0, 0.0)),
            "a nested frame must fall back to a full repaint"
        );
        assert!(
            scroll_fast_path_eligible(&list, 1, &lr(0.0, 0.0, 100.0, 100.0), (0.0, 0.0), (0.0, 0.0)),
            "the outer frame is still eligible (nothing painted behind it)"
        );
    }

    #[test]
    fn fast_path_zero_area_clip_does_not_divide_by_zero() {
        let list = dlist(vec![
            push_scroll(1, 0.0, 0.0, 0.0, 0.0),
            opaque_rect(0.0, 0.0, 10.0, 10.0),
            DisplayListItem::PopScrollFrame,
        ]);
        // clip_area is max(1.0)-guarded; must return a bool, not panic.
        assert!(scroll_fast_path_eligible(&list, 1, &lr(0.0, 0.0, 0.0, 0.0), (0.0, 0.0), (0.0, 0.0)));
    }

    #[test]
    fn fast_path_checks_coverage_at_both_offsets() {
        // Content covers the clip only while the frame is at offset 0; scrolled
        // by 60 it uncovers the bottom half, exposing a partial backdrop.
        let list = dlist(vec![
            opaque_rect(0.0, 0.0, 100.0, 40.0), // partial backdrop (≥10% of clip)
            push_scroll(7, 0.0, 0.0, 100.0, 100.0),
            opaque_rect(0.0, 0.0, 100.0, 100.0), // covers the clip at offset 0 only
            DisplayListItem::PopScrollFrame,
        ]);
        let clip = lr(0.0, 0.0, 100.0, 100.0);
        assert!(scroll_fast_path_eligible(&list, 7, &clip, (0.0, 0.0), (0.0, 0.0)));
        assert!(
            !scroll_fast_path_eligible(&list, 7, &clip, (0.0, 60.0), (0.0, 0.0)),
            "coverage must hold at the NEW offset too, not just the old one"
        );
        assert!(
            !scroll_fast_path_eligible(&list, 7, &clip, (0.0, 0.0), (0.0, 60.0)),
            "…and at the OLD offset, where the dragged pixels were rendered"
        );
    }

    // ============================== overlay_rects_after_frame ================

    #[test]
    fn overlay_rects_empty_without_a_matching_frame() {
        let list = dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]);
        assert!(overlay_rects_after_frame(&list, 99, &lr(0.0, 0.0, 10.0, 10.0)).is_empty());
    }

    #[test]
    fn overlay_rects_require_strict_overlap_and_are_clipped() {
        let clip = lr(0.0, 0.0, 100.0, 100.0);
        let list = dlist(vec![
            push_scroll(1, 0.0, 0.0, 100.0, 100.0),
            opaque_rect(0.0, 0.0, 100.0, 500.0), // inside the frame — not an overlay
            DisplayListItem::PopScrollFrame,
            opaque_rect(100.0, 0.0, 10.0, 10.0), // merely TOUCHES the clip edge
            opaque_rect(90.0, 90.0, 40.0, 40.0), // genuinely overlaps
            DisplayListItem::PushClip {          // state-management → skipped
                bounds: wlr(0.0, 0.0, 100.0, 100.0),
                border_radius: BorderRadius::default(),
            },
        ]);
        let r = overlay_rects_after_frame(&list, 1, &clip);
        assert_eq!(r.len(), 1, "only the strictly-overlapping item, got {r:?}");
        assert_eq!(r[0].origin.x, 90.0);
        assert_eq!(r[0].size.width, 10.0, "intersection is clipped to the frame");
        assert_eq!(r[0].size.height, 10.0);
    }

    // ============================== gpu_value_damage =========================

    fn translate(tx: f32, ty: f32) -> ComputedTransform3D {
        ComputedTransform3D {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [tx, ty, 0.0, 1.0],
            ],
        }
    }
    fn ref_frame(key: usize) -> DisplayListItem {
        DisplayListItem::PushReferenceFrame {
            transform_key: TransformKey { id: key },
            initial_transform: ComputedTransform3D::IDENTITY,
            bounds: wlr(0.0, 0.0, 50.0, 50.0),
        }
    }

    #[test]
    fn gpu_value_damage_unchanged_maps_report_nothing() {
        let list = dlist(vec![ref_frame(3), DisplayListItem::PopReferenceFrame]);
        let mut t: HashMap<usize, ComputedTransform3D> = HashMap::new();
        t.insert(3, translate(1.0, 1.0));
        let mut o: HashMap<usize, f32> = HashMap::new();
        o.insert(4, 0.5);
        let d = gpu_value_damage(&list, &t, &o, &t.clone(), &o.clone());
        assert!(d.rects.is_empty());
        assert!(!d.needs_full);

        let empty_t: HashMap<usize, ComputedTransform3D> = HashMap::new();
        let empty_o: HashMap<usize, f32> = HashMap::new();
        let d2 = gpu_value_damage(&list, &empty_t, &empty_o, &empty_t, &empty_o);
        assert!(d2.rects.is_empty() && !d2.needs_full, "empty maps → no damage");
    }

    #[test]
    fn gpu_value_damage_changed_transform_on_a_reference_frame_needs_full_repaint() {
        let list = dlist(vec![ref_frame(3), DisplayListItem::PopReferenceFrame]);
        let mut old_t: HashMap<usize, ComputedTransform3D> = HashMap::new();
        old_t.insert(3, ComputedTransform3D::IDENTITY);
        let mut new_t: HashMap<usize, ComputedTransform3D> = HashMap::new();
        new_t.insert(3, translate(20.0, 0.0));
        let o: HashMap<usize, f32> = HashMap::new();
        let d = gpu_value_damage(&list, &old_t, &o, &new_t, &o);
        assert!(d.needs_full, "a moved reference frame's content extent is unknowable");
    }

    #[test]
    fn gpu_value_damage_removed_key_counts_as_changed() {
        let list = dlist(vec![ref_frame(3), DisplayListItem::PopReferenceFrame]);
        let mut old_t: HashMap<usize, ComputedTransform3D> = HashMap::new();
        old_t.insert(3, translate(5.0, 5.0));
        let new_t: HashMap<usize, ComputedTransform3D> = HashMap::new();
        let o: HashMap<usize, f32> = HashMap::new();
        assert!(
            gpu_value_damage(&list, &old_t, &o, &new_t, &o).needs_full,
            "a key present in old but absent in new is a change"
        );
    }

    #[test]
    fn gpu_value_damage_ignores_keys_bound_to_nothing() {
        let list = dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]);
        let t: HashMap<usize, ComputedTransform3D> = HashMap::new();
        let mut new_t: HashMap<usize, ComputedTransform3D> = HashMap::new();
        new_t.insert(77, translate(1.0, 0.0));
        let old_o: HashMap<usize, f32> = HashMap::new();
        let mut new_o: HashMap<usize, f32> = HashMap::new();
        new_o.insert(88, 0.25);
        let d = gpu_value_damage(&list, &t, &old_o, &new_t, &new_o);
        assert!(d.rects.is_empty() && !d.needs_full, "a key bound to no item cannot damage");
    }

    #[test]
    fn gpu_value_damage_nan_opacity_is_change_but_still_unbound() {
        let list = dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]);
        let t: HashMap<usize, ComputedTransform3D> = HashMap::new();
        let mut o: HashMap<usize, f32> = HashMap::new();
        o.insert(1, f32::NAN);
        // NaN != NaN → the key reads as "changed"; nothing binds it, so no damage.
        let d = gpu_value_damage(&list, &t, &o.clone(), &t, &o);
        assert!(d.rects.is_empty() && !d.needs_full);
    }

    // ============================== display list diffing =====================

    #[test]
    fn display_lists_visually_equal_basics() {
        let a = dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]);
        let b = dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]);
        assert!(display_lists_visually_equal(&dlist(vec![]), &dlist(vec![])), "empty == empty");
        assert!(display_lists_visually_equal(&a, &b));
        assert!(!display_lists_visually_equal(&a, &dlist(vec![])), "length differs");
        let c = dlist(vec![rect_item(0.0, 0.0, 10.0, 10.0, ColorU { r: 9, g: 9, b: 9, a: 255 })]);
        assert!(!display_lists_visually_equal(&a, &c), "colour differs");
        let d = dlist(vec![DisplayListItem::PopClip]);
        assert!(!display_lists_visually_equal(&a, &d), "discriminant differs");
    }

    #[test]
    fn damage_diff_returns_none_on_structural_change() {
        let off = ScrollOffsetMap::new();
        let a = dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]);
        assert!(
            compute_display_list_damage(&a, &dlist(vec![]), &off, &off).is_none(),
            "different item counts → full repaint"
        );
        let b = dlist(vec![DisplayListItem::PopClip]);
        assert!(
            compute_display_list_damage(&a, &b, &off, &off).is_none(),
            "same length, different discriminant → full repaint"
        );
    }

    #[test]
    fn damage_diff_of_identical_lists_is_empty() {
        let off = ScrollOffsetMap::new();
        let a = dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]);
        let b = dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]);
        let d = compute_display_list_damage(&a, &b, &off, &off).expect("no structural change");
        assert!(d.is_empty(), "identical frames damage nothing");
        let e = compute_display_list_damage(&dlist(vec![]), &dlist(vec![]), &off, &off);
        assert_eq!(e.map(|v| v.len()), Some(0), "two empty lists are comparable");
    }

    #[test]
    fn damage_diff_covers_a_colour_change() {
        let off = ScrollOffsetMap::new();
        let a = dlist(vec![opaque_rect(10.0, 10.0, 20.0, 20.0)]);
        let b = dlist(vec![rect_item(10.0, 10.0, 20.0, 20.0, ColorU { r: 1, g: 1, b: 1, a: 255 })]);
        let d = compute_display_list_damage(&a, &b, &off, &off).unwrap();
        assert_eq!(d.len(), 1, "old + new bounds coincide → one coalesced rect");
        assert_eq!(d[0].origin.x, 10.0);
        assert_eq!(d[0].size.width, 20.0);
    }

    #[test]
    fn damage_diff_projects_scroll_offsets_into_viewport_space() {
        // The item lives at content y = 100 inside frame 1. Old offset 0, new
        // offset 50 → its pixels were at y=100 and will be at y=50.
        let mut old_off = ScrollOffsetMap::new();
        old_off.insert(1, (0.0, 0.0));
        let mut new_off = ScrollOffsetMap::new();
        new_off.insert(1, (0.0, 50.0));
        let old = dlist(vec![
            push_scroll(1, 0.0, 0.0, 100.0, 100.0),
            opaque_rect(0.0, 100.0, 10.0, 10.0),
            DisplayListItem::PopScrollFrame,
        ]);
        let new = dlist(vec![
            push_scroll(1, 0.0, 0.0, 100.0, 100.0),
            rect_item(0.0, 100.0, 10.0, 10.0, ColorU { r: 7, g: 7, b: 7, a: 255 }),
            DisplayListItem::PopScrollFrame,
        ]);
        let d = compute_display_list_damage(&old, &new, &old_off, &new_off).unwrap();
        assert_eq!(d.len(), 2, "old and new positions are 40px apart → no merge, got {d:?}");
        let ys: Vec<f32> = d.iter().map(|r| r.origin.y).collect();
        assert!(ys.contains(&100.0), "old pixels at y=100 (offset 0), got {ys:?}");
        assert!(ys.contains(&50.0), "new pixels at y=50 (offset 50), got {ys:?}");
    }

    // ============================== compute_virtual_view_damage ==============

    #[test]
    fn virtual_view_damage_without_virtual_views_is_empty() {
        let parent = dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]);
        let cur: BTreeMap<DomId, Arc<DisplayList>> = BTreeMap::new();
        let prev: BTreeMap<DomId, Arc<DisplayList>> = BTreeMap::new();
        assert!(compute_virtual_view_damage(&parent, &cur, &prev).is_empty());
    }

    #[test]
    fn virtual_view_damage_tracks_child_dom_changes() {
        let dom = DomId { inner: 1 };
        let parent = dlist(vec![DisplayListItem::VirtualView {
            child_dom_id: dom,
            bounds: wlr(5.0, 6.0, 40.0, 30.0),
            clip_rect: wlr(5.0, 6.0, 40.0, 30.0),
        }]);
        let shared = Arc::new(dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]));
        let equal_but_distinct = Arc::new(dlist(vec![opaque_rect(0.0, 0.0, 10.0, 10.0)]));
        let different = Arc::new(dlist(vec![opaque_rect(0.0, 0.0, 99.0, 99.0)]));

        let mut cur: BTreeMap<DomId, Arc<DisplayList>> = BTreeMap::new();
        let mut prev: BTreeMap<DomId, Arc<DisplayList>> = BTreeMap::new();

        // Same Arc → cheap pointer fast-path → no damage.
        cur.insert(dom, Arc::clone(&shared));
        prev.insert(dom, Arc::clone(&shared));
        assert!(compute_virtual_view_damage(&parent, &cur, &prev).is_empty());

        // Distinct Arcs, identical content → still no damage.
        cur.insert(dom, Arc::clone(&equal_but_distinct));
        assert!(compute_virtual_view_damage(&parent, &cur, &prev).is_empty());

        // Content actually changed → damage the VirtualView's on-screen bounds.
        cur.insert(dom, Arc::clone(&different));
        let d = compute_virtual_view_damage(&parent, &cur, &prev);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].origin.x, 5.0);
        assert_eq!(d[0].size.width, 40.0);

        // Newly present child (absent last frame) counts as changed.
        prev.remove(&dom);
        assert_eq!(compute_virtual_view_damage(&parent, &cur, &prev).len(), 1);

        // Absent in both → nothing to draw, nothing to damage.
        cur.remove(&dom);
        assert!(compute_virtual_view_damage(&parent, &cur, &prev).is_empty());
    }

    // ============================== apply_layer_filters ======================

    #[test]
    fn filters_empty_list_is_a_noop() {
        let mut p = solid(2, 2, [10, 20, 30, 40]);
        apply_layer_filters(&mut p, &[], 1.0);
        assert_eq!(at(&p, 0, 0), [10, 20, 30, 40]);
    }

    #[test]
    fn filter_opacity_saturates_at_both_ends() {
        let mut zero = solid(2, 2, [10, 20, 30, 255]);
        apply_layer_filters(&mut zero, &[StyleFilter::Opacity(PercentageValue::new(0.0))], 1.0);
        assert_eq!(at(&zero, 0, 0)[3], 0, "0% → fully transparent");

        let mut half = solid(2, 2, [10, 20, 30, 255]);
        apply_layer_filters(&mut half, &[StyleFilter::Opacity(PercentageValue::new(50.0))], 1.0);
        assert_eq!(at(&half, 0, 0)[3], 127, "50% → 127 (127.5 truncated)");

        let mut over = solid(2, 2, [10, 20, 30, 255]);
        apply_layer_filters(&mut over, &[StyleFilter::Opacity(PercentageValue::new(500.0))], 1.0);
        assert_eq!(at(&over, 0, 0)[3], 255, "500% must clamp, not wrap");

        let mut neg = solid(2, 2, [10, 20, 30, 255]);
        apply_layer_filters(&mut neg, &[StyleFilter::Opacity(PercentageValue::new(-100.0))], 1.0);
        assert_eq!(at(&neg, 0, 0)[3], 0, "a negative percentage clamps to 0, not to 255");
        assert_eq!(at(&neg, 0, 0)[0], 10, "RGB is untouched by opacity");
    }

    #[test]
    fn filter_nan_percentage_quantizes_to_zero_and_does_not_panic() {
        // PercentageValue is fixed-point (isize ×1000): `NaN as isize` saturates
        // to 0, so a NaN filter amount degrades to 0% rather than poisoning the
        // pixels with NaN.
        let mut p = solid(2, 2, [200, 100, 50, 255]);
        apply_layer_filters(&mut p, &[StyleFilter::Grayscale(PercentageValue::new(f32::NAN))], 1.0);
        assert_eq!(at(&p, 0, 0), [200, 100, 50, 255], "NaN amount ⇒ 0% ⇒ identity");
    }

    #[test]
    fn filter_brightness_clamps_below_zero_and_above_white() {
        let mut dark = solid(2, 2, [200, 100, 50, 255]);
        apply_layer_filters(&mut dark, &[StyleFilter::Brightness(PercentageValue::new(-500.0))], 1.0);
        assert_eq!(at(&dark, 0, 0), [0, 0, 0, 255], "negative brightness floors at black");

        let mut bright = solid(2, 2, [100, 100, 100, 255]);
        apply_layer_filters(
            &mut bright,
            &[StyleFilter::Brightness(PercentageValue::new(100_000.0))],
            1.0,
        );
        assert_eq!(at(&bright, 0, 0), [255, 255, 255, 255], "huge brightness saturates to white");
    }

    #[test]
    fn filter_grayscale_and_saturate_agree_at_their_extremes() {
        // luma(255, 0, 0) = 0.2126 * 255 ≈ 54
        let mut gray = solid(2, 2, [255, 0, 0, 255]);
        apply_layer_filters(&mut gray, &[StyleFilter::Grayscale(PercentageValue::new(100.0))], 1.0);
        let g = at(&gray, 0, 0);
        assert_eq!((g[0], g[1], g[2]), (54, 54, 54), "full grayscale → luma on every channel");
        assert_eq!(g[3], 255, "alpha untouched");

        let mut desat = solid(2, 2, [255, 0, 0, 255]);
        apply_layer_filters(&mut desat, &[StyleFilter::Saturate(PercentageValue::new(0.0))], 1.0);
        assert_eq!(at(&desat, 0, 0), g, "saturate(0) == grayscale(1)");
    }

    #[test]
    fn filter_grayscale_amount_over_100_percent_is_clamped() {
        let mut a = solid(2, 2, [255, 0, 0, 255]);
        apply_layer_filters(&mut a, &[StyleFilter::Grayscale(PercentageValue::new(100.0))], 1.0);
        let mut b = solid(2, 2, [255, 0, 0, 255]);
        apply_layer_filters(&mut b, &[StyleFilter::Grayscale(PercentageValue::new(9999.0))], 1.0);
        assert_eq!(at(&a, 0, 0), at(&b, 0, 0), "amount is clamped to 1.0 — no overshoot");
    }

    #[test]
    fn filter_invert_full_inverts_rgb_only() {
        let mut p = solid(2, 2, [0, 0, 255, 200]);
        apply_layer_filters(&mut p, &[StyleFilter::Invert(PercentageValue::new(100.0))], 1.0);
        assert_eq!(at(&p, 0, 0), [255, 255, 0, 200]);
    }

    #[test]
    fn filter_hue_rotate_by_zero_preserves_the_colour() {
        let mut p = solid(2, 2, [200, 100, 50, 255]);
        apply_layer_filters(&mut p, &[StyleFilter::HueRotate(AngleValue::deg(0.0))], 1.0);
        let g = at(&p, 0, 0);
        for (got, want) in g.iter().zip([200u8, 100, 50, 255].iter()) {
            assert!(
                (i32::from(*got) - i32::from(*want)).abs() <= 2,
                "identity hue rotation must round-trip (±2 for f32 matrix error), got {g:?}"
            );
        }
    }

    #[test]
    fn filter_blur_with_no_effective_radius_is_a_noop() {
        let base = solid(8, 8, [10, 20, 30, 255]);
        let blur = |px: f32| StyleFilter::Blur(StyleBlur {
            width: PixelValue::px(px),
            height: PixelValue::px(px),
        });

        let mut zero = solid(8, 8, [10, 20, 30, 255]);
        apply_layer_filters(&mut zero, &[blur(0.0)], 1.0);
        assert_eq!(zero.data(), base.data(), "0px radius → skipped");

        let mut neg = solid(8, 8, [10, 20, 30, 255]);
        apply_layer_filters(&mut neg, &[blur(-8.0)], 1.0);
        assert_eq!(neg.data(), base.data(), "a negative radius casts to 0, it must not wrap");

        let mut nan_dpi = solid(8, 8, [10, 20, 30, 255]);
        apply_layer_filters(&mut nan_dpi, &[blur(4.0)], f32::NAN);
        assert_eq!(nan_dpi.data(), base.data(), "NaN dpi → radius 0 → skipped");

        let mut zero_dpi = solid(8, 8, [10, 20, 30, 255]);
        apply_layer_filters(&mut zero_dpi, &[blur(4.0)], 0.0);
        assert_eq!(zero_dpi.data(), base.data(), "dpi 0 → radius 0 → skipped");
    }

    #[test]
    fn filter_blur_softens_a_hard_edge() {
        let mut p = AzulPixmap::new(16, 16).unwrap();
        p.fill(0, 0, 0, 255);
        p.fill_rect(8, 0, 8, 16, 255, 255, 255, 255); // right half white
        let before = p.data().to_vec();
        apply_layer_filters(
            &mut p,
            &[StyleFilter::Blur(StyleBlur {
                width: PixelValue::px(2.0),
                height: PixelValue::px(2.0),
            })],
            1.0,
        );
        assert_ne!(p.data(), &before[..], "a 2px blur must actually change pixels");
        assert_eq!(p.data().len(), before.len(), "the buffer must not be reallocated");
    }

    #[test]
    fn filter_unimplemented_variants_are_noops() {
        let mut p = solid(2, 2, [10, 20, 30, 255]);
        apply_layer_filters(&mut p, &[StyleFilter::ComponentTransfer], 1.0);
        assert_eq!(at(&p, 0, 0), [10, 20, 30, 255]);
    }

    #[test]
    fn filter_chain_applies_in_order() {
        let mut p = solid(2, 2, [255, 0, 0, 255]);
        apply_layer_filters(
            &mut p,
            &[
                StyleFilter::Brightness(PercentageValue::new(0.0)), // → black
                StyleFilter::Invert(PercentageValue::new(100.0)),   // → white
            ],
            1.0,
        );
        assert_eq!(at(&p, 0, 0), [255, 255, 255, 255], "filters must compose left-to-right");
    }

    // ============================== allocate_layers_from_display_list ========

    #[test]
    fn allocate_layers_on_empty_display_list_keeps_only_the_root() {
        let mut c = CompositorState::new(64, 64);
        c.allocate_layers_from_display_list(&dlist(vec![]), 1.0);
        assert_eq!(c.layers.len(), 1);
        let root = c.layers.get(&c.root_layer).unwrap();
        assert_eq!(root.display_list_range, (0, 0));
        assert!(root.children.is_empty());
    }

    /// A display list that wants one layer of every kind.
    fn layer_soup() -> DisplayList {
        dlist(vec![
            push_scroll(1, 0.0, 0.0, 20.0, 20.0),
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PushOpacity {
                bounds: wlr(0.0, 0.0, 20.0, 20.0),
                opacity: 0.5,
            },
            DisplayListItem::PopOpacity,
            DisplayListItem::PushFilter {
                bounds: wlr(0.0, 0.0, 20.0, 20.0),
                filters: vec![StyleFilter::Blur(StyleBlur {
                    width: PixelValue::px(2.0),
                    height: PixelValue::px(2.0),
                })],
            },
            DisplayListItem::PopFilter,
        ])
    }

    #[test]
    fn allocate_layers_at_dpi_one_creates_one_layer_per_group() {
        let mut c = CompositorState::new(64, 64);
        c.allocate_layers_from_display_list(&layer_soup(), 1.0);
        assert_eq!(c.layers.len(), 4, "root + scroll + opacity + blur");
        let root = c.layers.get(&c.root_layer).unwrap();
        assert_eq!(root.children.len(), 3, "all three are direct children of the root");
    }

    #[test]
    fn allocate_layers_at_degenerate_dpi_creates_no_layers() {
        for dpi in [0.0_f32, -2.0, f32::NAN] {
            let mut c = CompositorState::new(64, 64);
            c.allocate_layers_from_display_list(&layer_soup(), dpi);
            assert_eq!(
                c.layers.len(),
                1,
                "dpi {dpi} yields a 0-pixel pixbuf — the layer must be skipped, not allocated"
            );
        }
    }

    #[test]
    fn allocate_layers_zero_sized_scroll_frame_is_skipped() {
        let mut c = CompositorState::new(64, 64);
        let list = dlist(vec![push_scroll(1, 0.0, 0.0, 0.0, 0.0), DisplayListItem::PopScrollFrame]);
        c.allocate_layers_from_display_list(&list, 1.0);
        assert_eq!(c.layers.len(), 1, "a 0×0 clip cannot get a pixbuf");
    }

    #[test]
    fn allocate_layers_scroll_frame_records_id_and_range() {
        let mut c = CompositorState::new(64, 64);
        let list = dlist(vec![
            push_scroll(7, 5.0, 6.0, 20.0, 20.0),
            opaque_rect(0.0, 0.0, 10.0, 10.0),
            DisplayListItem::PopScrollFrame,
        ]);
        c.allocate_layers_from_display_list(&list, 1.0);
        assert_eq!(c.layers.len(), 2);
        let l = c.layers.values().find(|l| l.scroll_id == Some(7)).expect("scroll layer");
        assert_eq!(l.display_list_range, (1, 2), "range is (push+1, matching pop)");
        assert_eq!(l.bounds.origin.x, 5.0);
        assert_eq!((l.pixbuf.width(), l.pixbuf.height()), (20, 20));
    }

    #[test]
    fn allocate_layers_unbalanced_pops_do_not_underflow_the_stack() {
        // The doc claims this panics on stack underflow — it must not.
        let mut c = CompositorState::new(64, 64);
        let list = dlist(vec![
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PopOpacity,
            DisplayListItem::PopFilter,
            DisplayListItem::PopReferenceFrame,
            DisplayListItem::PopBackdropFilter,
            DisplayListItem::PopScrollFrame,
        ]);
        c.allocate_layers_from_display_list(&list, 1.0);
        assert_eq!(c.layers.len(), 1, "stray pops are ignored, the root survives");
    }

    #[test]
    fn allocate_layers_unmatched_push_runs_to_the_end_of_the_list() {
        let mut c = CompositorState::new(64, 64);
        let list = dlist(vec![
            push_scroll(1, 0.0, 0.0, 20.0, 20.0),
            opaque_rect(0.0, 0.0, 5.0, 5.0),
        ]);
        c.allocate_layers_from_display_list(&list, 1.0);
        let l = c.layers.values().find(|l| l.scroll_id == Some(1)).unwrap();
        assert_eq!(l.display_list_range, (1, 2), "an unmatched push clamps to items.len()");
    }

    #[test]
    fn allocate_layers_opacity_edge_values() {
        // opacity >= 1.0 and NaN must NOT allocate a layer (`*opacity < 1.0`).
        for op in [1.0_f32, 2.0, f32::NAN] {
            let mut c = CompositorState::new(64, 64);
            let list = dlist(vec![
                DisplayListItem::PushOpacity {
                    bounds: wlr(0.0, 0.0, 20.0, 20.0),
                    opacity: op,
                },
                DisplayListItem::PopOpacity,
            ]);
            c.allocate_layers_from_display_list(&list, 1.0);
            assert_eq!(c.layers.len(), 1, "opacity {op} needs no layer");
        }
        let mut c = CompositorState::new(64, 64);
        let list = dlist(vec![
            DisplayListItem::PushOpacity {
                bounds: wlr(0.0, 0.0, 20.0, 20.0),
                opacity: -3.0,
            },
            DisplayListItem::PopOpacity,
        ]);
        c.allocate_layers_from_display_list(&list, 1.0);
        assert_eq!(c.layers.len(), 2, "a negative opacity still needs its own layer");
    }

    #[test]
    fn allocate_layers_identity_reference_frame_is_not_promoted() {
        let mut c = CompositorState::new(64, 64);
        let ident = dlist(vec![ref_frame(1), DisplayListItem::PopReferenceFrame]);
        c.allocate_layers_from_display_list(&ident, 1.0);
        assert_eq!(c.layers.len(), 1, "an identity transform needs no layer");

        let mut c2 = CompositorState::new(64, 64);
        let moved = dlist(vec![
            DisplayListItem::PushReferenceFrame {
                transform_key: TransformKey { id: 1 },
                initial_transform: translate(20.0, 10.0),
                bounds: wlr(0.0, 0.0, 20.0, 20.0),
            },
            DisplayListItem::PopReferenceFrame,
        ]);
        c2.allocate_layers_from_display_list(&moved, 1.0);
        assert_eq!(c2.layers.len(), 2, "a non-identity transform gets its own layer");
        let l = c2.layers.values().find(|l| l.id != c2.root_layer).unwrap();
        assert!(!l.transform.is_identity(IDENTITY_EPSILON_F64));
    }

    #[test]
    fn allocate_layers_nests_children_under_their_parent() {
        let mut c = CompositorState::new(64, 64);
        let list = dlist(vec![
            push_scroll(1, 0.0, 0.0, 40.0, 40.0),
            push_scroll(2, 0.0, 0.0, 20.0, 20.0),
            opaque_rect(0.0, 0.0, 5.0, 5.0),
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PopScrollFrame,
        ]);
        c.allocate_layers_from_display_list(&list, 1.0);
        assert_eq!(c.layers.len(), 3);
        let root = c.layers.get(&c.root_layer).unwrap();
        assert_eq!(root.children.len(), 1, "only the outer frame hangs off the root");
        let outer_id = root.children[0];
        let outer = c.layers.get(&outer_id).unwrap();
        assert_eq!(outer.scroll_id, Some(1));
        assert_eq!(outer.children.len(), 1, "the inner frame is a child of the outer one");
        let inner = c.layers.get(&outer.children[0]).unwrap();
        assert_eq!(inner.scroll_id, Some(2));
        assert_eq!(inner.display_list_range, (2, 3));
    }

    #[test]
    fn allocate_layers_is_idempotent_across_frames() {
        let mut c = CompositorState::new(64, 64);
        c.allocate_layers_from_display_list(&layer_soup(), 1.0);
        let first = c.layers.len();
        c.allocate_layers_from_display_list(&layer_soup(), 1.0);
        assert_eq!(c.layers.len(), first, "re-allocating must not leak last frame's layers");
        let root = c.layers.get(&c.root_layer).unwrap();
        assert_eq!(root.children.len(), 3, "root children are rebuilt, not appended to");
        assert!(c.next_layer_id_peek() > first as u64, "ids stay monotonic across frames");
    }

    #[test]
    fn allocate_layers_backdrop_filter_starts_transparent() {
        let mut c = CompositorState::new(64, 64);
        let list = dlist(vec![
            DisplayListItem::PushBackdropFilter {
                bounds: wlr(0.0, 0.0, 20.0, 20.0),
                filters: vec![StyleFilter::Invert(PercentageValue::new(100.0))],
            },
            DisplayListItem::PopBackdropFilter,
        ]);
        c.allocate_layers_from_display_list(&list, 1.0);
        let l = c.layers.values().find(|l| l.is_backdrop_filter).expect("backdrop layer");
        assert_eq!(
            at(&l.pixbuf, 0, 0),
            [0, 0, 0, 0],
            "an empty backdrop-filter box must not blit opaque white over the backdrop"
        );

        // With no filters at all it is NOT a backdrop layer.
        let mut c2 = CompositorState::new(64, 64);
        let empty = dlist(vec![
            DisplayListItem::PushBackdropFilter {
                bounds: wlr(0.0, 0.0, 20.0, 20.0),
                filters: Vec::new(),
            },
            DisplayListItem::PopBackdropFilter,
        ]);
        c2.allocate_layers_from_display_list(&empty, 1.0);
        assert_eq!(c2.layers.len(), 1, "no filters → no layer");
    }

    // ============================== compute_damage ===========================

    #[test]
    fn compute_damage_with_no_dirty_nodes_is_a_noop() {
        let mut c = CompositorState::new(64, 64);
        c.compute_damage(&BTreeSet::new(), &[], &[], &[]);
        assert!(c.layers.get(&c.root_layer).unwrap().damage.is_empty());
    }

    #[test]
    fn compute_damage_ignores_out_of_range_node_indices() {
        let mut c = CompositorState::new(64, 64);
        let dirty: BTreeSet<usize> = [0usize, 5, usize::MAX].into_iter().collect();
        // Every slice is empty → every index is out of range → guarded, no panic.
        c.compute_damage(&dirty, &[], &[], &[]);
        assert!(c.layers.get(&c.root_layer).unwrap().damage.is_empty());
    }

    #[test]
    fn compute_damage_covers_the_old_and_the_new_position() {
        let mut c = CompositorState::new(64, 64);
        let dirty: BTreeSet<usize> = [0usize].into_iter().collect();
        let old = [LogicalPosition::new(0.0, 0.0)];
        let new = [LogicalPosition::new(20.0, 20.0)];
        let rects = [lr(0.0, 0.0, 10.0, 10.0)];
        c.compute_damage(&dirty, &old, &new, &rects);
        let root = c.layers.get(&c.root_layer).unwrap();
        assert_eq!(root.damage.len(), 2, "a moved node damages where it was AND where it is");
        assert!(root.composite_dirty);
        assert!(root.damage.iter().any(|d| d.origin.x == 0.0));
        assert!(root.damage.iter().any(|d| d.origin.x == 20.0));
    }

    #[test]
    fn compute_damage_with_nan_positions_does_not_leak_nan() {
        // `rect_intersection` uses f32::max/min, which IGNORE NaN — so a NaN
        // node degrades to whole-layer damage (conservative) rather than to
        // `None`. Either way, no NaN may reach a damage rect: a NaN rect would
        // silently rasterise to nothing and the node would never repaint.
        let mut c = CompositorState::new(64, 64);
        let dirty: BTreeSet<usize> = [0usize].into_iter().collect();
        let nan = [LogicalPosition::new(f32::NAN, f32::NAN)];
        let rects = [lr(0.0, 0.0, f32::NAN, f32::NAN)];
        c.compute_damage(&dirty, &nan, &nan, &rects);
        let root = c.layers.get(&c.root_layer).unwrap();
        for d in &root.damage {
            assert!(
                d.origin.x.is_finite()
                    && d.origin.y.is_finite()
                    && d.size.width.is_finite()
                    && d.size.height.is_finite(),
                "NaN must not leak into a damage rect, got {d:?}"
            );
        }
    }

    #[test]
    fn compute_damage_clips_to_the_layer_bounds() {
        let mut c = CompositorState::new(64, 64);
        let dirty: BTreeSet<usize> = [0usize].into_iter().collect();
        let pos = [LogicalPosition::new(60.0, 60.0)];
        let rects = [lr(0.0, 0.0, 100.0, 100.0)];
        c.compute_damage(&dirty, &pos, &pos, &rects);
        let root = c.layers.get(&c.root_layer).unwrap();
        for d in &root.damage {
            assert!(
                d.origin.x + d.size.width <= 64.0 && d.origin.y + d.size.height <= 64.0,
                "damage must be clipped to the layer, got {d:?}"
            );
        }
    }

    // ============================== render_layers / composite_frame ==========

    #[test]
    fn render_layers_on_an_empty_display_list_is_ok() {
        let mut c = CompositorState::new(8, 8);
        let list = dlist(vec![]);
        c.allocate_layers_from_display_list(&list, 1.0);
        let (rr, mut gc, st) = render_deps();
        assert!(c.render_layers(&list, 1.0, &rr, None, &mut gc, &st).is_ok());
    }

    #[test]
    fn render_layers_paints_a_rect_into_the_root_pixbuf() {
        let mut c = CompositorState::new(16, 16);
        let list = dlist(vec![rect_item(
            0.0,
            0.0,
            16.0,
            16.0,
            ColorU { r: 0, g: 0, b: 255, a: 255 },
        )]);
        c.allocate_layers_from_display_list(&list, 1.0);
        let (rr, mut gc, st) = render_deps();
        c.render_layers(&list, 1.0, &rr, None, &mut gc, &st).unwrap();
        let root = c.layers.get(&c.root_layer).unwrap();
        assert_eq!(at(&root.pixbuf, 8, 8), [0, 0, 255, 255]);

        let mut out = AzulPixmap::new(16, 16).unwrap();
        out.fill(0, 0, 0, 255);
        c.composite_frame(&mut out, 1.0);
        assert_eq!(at(&out, 8, 8), [0, 0, 255, 255], "the root layer is blitted 1:1");
    }

    #[test]
    fn render_layers_survives_degenerate_dpi_factors() {
        // A non-finite / non-positive scale makes every rect un-rasterisable;
        // the renderer must skip it and still return Ok with a cleared root.
        for dpi in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
            let mut c = CompositorState::new(8, 8);
            let list = dlist(vec![rect_item(
                0.0,
                0.0,
                8.0,
                8.0,
                ColorU { r: 0, g: 0, b: 255, a: 255 },
            )]);
            c.allocate_layers_from_display_list(&list, dpi);
            let (rr, mut gc, st) = render_deps();
            assert!(
                c.render_layers(&list, dpi, &rr, None, &mut gc, &st).is_ok(),
                "dpi {dpi} must not error or panic"
            );
            let root = c.layers.get(&c.root_layer).unwrap();
            assert_eq!(
                at(&root.pixbuf, 4, 4),
                [255, 255, 255, 255],
                "dpi {dpi} rasterises nothing — the root stays cleared to white"
            );
        }
    }

    #[test]
    fn render_layers_skips_a_layer_range_past_the_end_of_the_list() {
        let mut c = CompositorState::new(8, 8);
        let list = dlist(vec![opaque_rect(0.0, 0.0, 8.0, 8.0)]);
        c.allocate_layers_from_display_list(&list, 1.0);
        // Simulate a stale range left over from a longer display list.
        let root_id = c.root_layer;
        c.layers.get_mut(&root_id).unwrap().display_list_range = (999, 1000);
        let (rr, mut gc, st) = render_deps();
        assert!(
            c.render_layers(&list, 1.0, &rr, None, &mut gc, &st).is_ok(),
            "an out-of-range range must be skipped, not indexed"
        );
    }

    #[test]
    fn render_layers_clamps_a_range_that_overruns_the_list() {
        let mut c = CompositorState::new(8, 8);
        let list = dlist(vec![rect_item(
            0.0,
            0.0,
            8.0,
            8.0,
            ColorU { r: 0, g: 255, b: 0, a: 255 },
        )]);
        c.allocate_layers_from_display_list(&list, 1.0);
        let root_id = c.root_layer;
        c.layers.get_mut(&root_id).unwrap().display_list_range = (0, 9999);
        let (rr, mut gc, st) = render_deps();
        c.render_layers(&list, 1.0, &rr, None, &mut gc, &st).unwrap();
        let root = c.layers.get(&c.root_layer).unwrap();
        assert_eq!(at(&root.pixbuf, 4, 4), [0, 255, 0, 255], "end is clamped to items.len()");
    }

    #[test]
    fn composite_frame_handles_degenerate_dpi_and_undersized_output() {
        let mut c = CompositorState::new(16, 16);
        let list = dlist(vec![]);
        c.allocate_layers_from_display_list(&list, 1.0);
        let (rr, mut gc, st) = render_deps();
        c.render_layers(&list, 1.0, &rr, None, &mut gc, &st).unwrap();

        for dpi in [0.0_f32, -1.0, f32::NAN] {
            let mut out = AzulPixmap::new(16, 16).unwrap();
            out.fill(0, 0, 0, 255);
            c.composite_frame(&mut out, dpi);
            assert_eq!(at(&out, 0, 0), [255, 255, 255, 255], "root blit ignores dpi {dpi}");
        }

        // Output smaller than the root layer: the blit must clip, not panic.
        let mut small = AzulPixmap::new(4, 4).unwrap();
        small.fill(0, 0, 0, 255);
        c.composite_frame(&mut small, 1.0);
        assert_eq!(at(&small, 3, 3), [255, 255, 255, 255]);
    }

    #[test]
    fn composite_frame_applies_layer_opacity() {
        let mut c = CompositorState::new(16, 16);
        let list = dlist(vec![
            DisplayListItem::PushOpacity {
                bounds: wlr(0.0, 0.0, 16.0, 16.0),
                opacity: 0.0, // fully transparent group
            },
            rect_item(0.0, 0.0, 16.0, 16.0, ColorU { r: 255, g: 0, b: 0, a: 255 }),
            DisplayListItem::PopOpacity,
        ]);
        c.allocate_layers_from_display_list(&list, 1.0);
        let (rr, mut gc, st) = render_deps();
        c.render_layers(&list, 1.0, &rr, None, &mut gc, &st).unwrap();
        let mut out = AzulPixmap::new(16, 16).unwrap();
        out.fill(0, 0, 0, 255);
        c.composite_frame(&mut out, 1.0);
        assert_eq!(
            at(&out, 8, 8),
            [255, 255, 255, 255],
            "an opacity-0 layer must contribute nothing over the white root"
        );
    }

    // ============================== scroll_layer =============================

    #[test]
    fn scroll_layer_with_an_unknown_id_is_ok() {
        let mut c = CompositorState::new(32, 32);
        let list = dlist(vec![]);
        c.allocate_layers_from_display_list(&list, 1.0);
        let (rr, mut gc, _st) = render_deps();
        assert!(
            c.scroll_layer(4242, (0.0, 10.0), &list, 1.0, &rr, None, &mut gc).is_ok(),
            "scrolling a frame that has no layer is a no-op, not a panic"
        );
    }

    #[test]
    fn scroll_layer_ignores_subpixel_deltas() {
        let mut c = CompositorState::new(32, 32);
        let list = dlist(vec![
            push_scroll(1, 0.0, 0.0, 32.0, 32.0),
            opaque_rect(0.0, 0.0, 32.0, 200.0),
            DisplayListItem::PopScrollFrame,
        ]);
        c.allocate_layers_from_display_list(&list, 1.0);
        let (rr, mut gc, _st) = render_deps();
        c.scroll_layer(1, (0.0, 0.4), &list, 1.0, &rr, None, &mut gc).unwrap();
        let l = c.layers.values().find(|l| l.scroll_id == Some(1)).unwrap();
        assert_eq!(l.scroll_offset, (0.0, 0.0), "|dy| < 0.5 must not move anything");
        assert!(l.damage.is_empty());
    }

    #[test]
    fn scroll_layer_updates_offset_and_records_the_exposed_strip() {
        let mut c = CompositorState::new(32, 32);
        let list = dlist(vec![
            push_scroll(1, 0.0, 0.0, 32.0, 32.0),
            opaque_rect(0.0, 0.0, 32.0, 200.0),
            DisplayListItem::PopScrollFrame,
        ]);
        c.allocate_layers_from_display_list(&list, 1.0);
        let (rr, mut gc, _st) = render_deps();
        c.scroll_layer(1, (0.0, 10.0), &list, 1.0, &rr, None, &mut gc).unwrap();
        let l = c.layers.values().find(|l| l.scroll_id == Some(1)).unwrap();
        assert_eq!(l.scroll_offset, (0.0, 10.0));
        assert_eq!(l.damage.len(), 1, "a single-axis scroll exposes one strip");
        assert!(l.composite_dirty);
    }

    // ============================== render_display_list_range ================

    #[test]
    fn render_range_with_start_after_end_is_ok() {
        let list = dlist(vec![opaque_rect(0.0, 0.0, 4.0, 4.0)]);
        let mut p = solid(4, 4, [255, 255, 255, 255]);
        let (rr, mut gc, st) = render_deps();
        let r = render_display_list_range(
            &list, &mut p, 5, 2, &[], 0.0, 0.0, 1.0, &rr, None, &mut gc, &st,
        );
        assert!(r.is_ok(), "an inverted range is an empty range, not a panic");
        assert_eq!(at(&p, 0, 0), [255, 255, 255, 255], "nothing was drawn");
    }

    #[test]
    fn render_range_honours_skip_ranges() {
        let list = dlist(vec![rect_item(
            0.0,
            0.0,
            4.0,
            4.0,
            ColorU { r: 255, g: 0, b: 0, a: 255 },
        )]);
        let mut p = solid(4, 4, [255, 255, 255, 255]);
        let (rr, mut gc, st) = render_deps();
        render_display_list_range(
            &list, &mut p, 0, 1, &[(0, 1)], 0.0, 0.0, 1.0, &rr, None, &mut gc, &st,
        )
        .unwrap();
        assert_eq!(
            at(&p, 2, 2),
            [255, 255, 255, 255],
            "an item claimed by a child layer must not be drawn twice"
        );
    }
}
