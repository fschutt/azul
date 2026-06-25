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
    /// CSS filters applied at composite time.
    pub filters: Vec<StyleFilter>,
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
                // TODO(superplan g4): allocate a layer for `backdrop-filter` here
                // (mirror the PushFilter arm above), but tag it so the compositor
                // knows to filter the *backdrop* rather than the layer's own
                // content. The compositing side then reads back the parent
                // `output` region under the bounds in
                // `composite_layer_recursive` and runs `apply_layer_filters` on
                // it before blitting the content. Left unallocated for now — see
                // the matching known-limitation TODO in `render_single_item`.
                // `text-shadow` (Push/PopTextShadow) is a text-rasterization
                // concern, not a layer boundary, so it is handled (currently as a
                // documented no-op) in `render_single_item`, not here.
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
            // Apply filters at composite time
            let src = if layer.filters.is_empty() {
                None
            } else {
                let mut filtered = layer.pixbuf.clone_pixmap();
                apply_layer_filters(&mut filtered, &layer.filters, dpi_factor);
                Some(filtered)
            };

            let src_pixbuf = src.as_ref().unwrap_or(&layer.pixbuf);
            let px_x = (abs_x * dpi_factor) as i32;
            let px_y = (abs_y * dpi_factor) as i32;
            blit_pixmap(src_pixbuf, output, px_x, px_y, layer.opacity);
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
    dpi_factor: f32,
) -> Vec<LogicalRect> {
    let px_dx = (delta.0 * dpi_factor).round() as i32;
    let px_dy = (delta.1 * dpi_factor).round() as i32;

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
    let cb = clip_bounds;
    let mut exposed = Vec::new();
    if px_dy != 0 {
        let h_logical = (px_dy.abs() as f32 + 1.0) / dpi_factor;
        let h = h_logical.min(cb.size.height);
        let y = if px_dy > 0 {
            // bottom strip exposed
            cb.origin.y + cb.size.height - h
        } else {
            // top strip exposed
            cb.origin.y
        };
        exposed.push(LogicalRect {
            origin: LogicalPosition { x: cb.origin.x, y },
            size: LogicalSize { width: cb.size.width, height: h },
        });
    }
    if px_dx != 0 {
        let w_logical = (px_dx.abs() as f32 + 1.0) / dpi_factor;
        let w = w_logical.min(cb.size.width);
        let x = if px_dx > 0 {
            // right strip exposed
            cb.origin.x + cb.size.width - w
        } else {
            // left strip exposed
            cb.origin.x
        };
        exposed.push(LogicalRect {
            origin: LogicalPosition { x, y: cb.origin.y },
            size: LogicalSize { width: w, height: cb.size.height },
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
/// `scroll_offset` is the frame's current offset, used to project the content's
/// opaque fills (stored at content coords) into viewport space for the coverage
/// test. A scroll frame over nothing-but-the-clear-color is always eligible (no
/// backdrop to drag). Returns `true` when there is no such frame (nothing to do).
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
pub fn scroll_fast_path_eligible(
    display_list: &DisplayList,
    scroll_id: LocalScrollId,
    clip_bounds: &LogicalRect,
    scroll_offset: (f32, f32),
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

    // (a) Best case: the SCROLLING content opaquely covers the clip (projected
    // into viewport space by the scroll offset). Then nothing behind can ever
    // show through, so the shift is always safe.
    let content_opaque: Vec<LogicalRect> = display_list.items[start + 1..end]
        .iter()
        .filter_map(opaque_fill_rect)
        .map(|r| LogicalRect {
            origin: LogicalPosition {
                x: r.origin.x - scroll_offset.0,
                y: r.origin.y - scroll_offset.1,
            },
            size: r.size,
        })
        .collect();
    if rect_covered_by(clip_bounds, &content_opaque) {
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
#[must_use] pub fn compute_display_list_damage(
    old: &DisplayList,
    new: &DisplayList,
) -> Option<Vec<LogicalRect>> {
    // Different item counts → structural change → full repaint
    if old.items.len() != new.items.len() {
        return None;
    }

    let mut damage = Vec::new();

    for (old_item, new_item) in old.items.iter().zip(new.items.iter()) {
        // Compare discriminant first (cheap)
        if std::mem::discriminant(old_item) != std::mem::discriminant(new_item) {
            return None; // structural change
        }

        // Compare full visual content, not just bounds — a color or text
        // change within the same bounds must still produce a damage rect.
        // Use visual_bounds() to include effects like box-shadow extent.
        if !old_item.is_visually_equal(new_item) {
            let old_bounds = old_item.visual_bounds();
            let new_bounds = new_item.visual_bounds();
            if let Some(ob) = old_bounds {
                damage.push(ob);
            }
            if let Some(nb) = new_bounds {
                damage.push(nb);
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
fn coalesce_damage_rects(rects: &mut Vec<LogicalRect>) {
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
        let strips = scroll_shift_region(&mut p, &rect(0.0, 0.0, 64.0, 64.0), (0.0, 0.0), 1.0);
        assert!(strips.is_empty(), "zero delta must not shift or expose anything");
        // Buffer untouched.
        assert_eq!(at(&p, 10, 20), [10, 20, 0, 255]);
    }

    #[test]
    #[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
    fn vertical_scroll_one_strip_and_translates() {
        let mut p = xy_pixmap(200, 100);
        // Scroll DOWN by 30 → content moves UP → bottom strip exposed.
        let strips = scroll_shift_region(&mut p, &rect(0.0, 0.0, 200.0, 100.0), (0.0, 30.0), 1.0);
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
            scroll_shift_region(&mut p, &rect(0.0, 0.0, 200.0, 100.0), (20.0, 30.0), 1.0);
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
        drop(scroll_shift_region(&mut p, &clip, (0.0, 10.0), 1.0));
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
        let strips = scroll_shift_region(&mut p, &clip, (0.0, 100.0), 1.0);
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
        assert!(scroll_fast_path_eligible(&list, 7, &rect(0.0, 0.0, 100.0, 100.0), (0.0, 0.0)));
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
        assert!(scroll_fast_path_eligible(&list, 7, &rect(0.0, 0.0, 100.0, 100.0), (0.0, 0.0)));
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
        assert!(!scroll_fast_path_eligible(&list, 7, &rect(0.0, 0.0, 100.0, 100.0), (0.0, 0.0)));
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
        assert!(!scroll_fast_path_eligible(&list, 7, &rect(0.0, 0.0, 100.0, 100.0), (0.0, 0.0)));
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
        assert!(scroll_fast_path_eligible(&list, 7, &rect(0.0, 0.0, 100.0, 100.0), (0.0, 0.0)));
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
