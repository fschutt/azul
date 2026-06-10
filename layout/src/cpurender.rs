//! CPU rendering for solver3 DisplayList
//!
//! This module renders a flat DisplayList (from solver3) to an AzulPixmap using agg-rust.
//! Unlike the old hierarchical CachedDisplayList, the new DisplayList is a simple
//! flat vector of rendering commands that can be executed sequentially.

use std::collections::HashMap;

use azul_core::{
    dom::ScrollbarOrientation,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::{DecodedImage, FontInstanceKey, ImageRef, RendererResources},
    ui_solver::GlyphInstance,
};
use azul_css::props::basic::{pixel::DEFAULT_FONT_SIZE, ColorOrSystem, ColorU, FontRef};
use azul_css::props::style::filter::StyleFilter;

use agg_rust::{
    basics::{FillingRule, VertexSource, PATH_FLAGS_NONE},
    blur::stack_blur_rgba32,
    color::Rgba8,
    conv_stroke::ConvStroke,
    conv_transform::ConvTransform,
    gradient_lut::GradientLut,
    path_storage::PathStorage,
    pixfmt_rgba::{PixelFormat, PixfmtRgba32},
    rasterizer_scanline_aa::RasterizerScanlineAa,
    renderer_base::RendererBase,
    renderer_scanline::{render_scanlines_aa, render_scanlines_aa_solid},
    rendering_buffer::RowAccessor,
    rounded_rect::RoundedRect,
    scanline_u::ScanlineU8,
    span_allocator::SpanAllocator,
    span_gradient::{GradientConic, GradientFunction, GradientRadialD, GradientX, SpanGradient},
    span_interpolator_linear::SpanInterpolatorLinear,
    trans_affine::TransAffine,
};

use crate::{
    font::parsed::ParsedFont,
    glyph_cache::GlyphCache,
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem, LocalScrollId},
    text3::cache::{FontHash, FontManager},
};

const IDENTITY_EPSILON: f32 = 0.0001;
const IDENTITY_EPSILON_F64: f64 = 0.0001;
const MAX_SHADOW_PIXBUF_SIZE: u32 = 4096;

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
    /// Monotonic counter for generating unique LayerIds.
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
    pub fn new(width: u32, height: u32) -> Self {
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
        CompositorState {
            layers,
            root_layer: root_id,
            next_layer_id: 1,
            previous_positions: Vec::new(),
        }
    }

    /// Allocate a new unique layer ID.
    pub fn alloc_layer_id(&mut self) -> LayerId {
        let id = LayerId(self.next_layer_id);
        self.next_layer_id += 1;
        id
    }

    /// Read-only peek at the next layer ID counter (for leak probes).
    pub fn next_layer_id_peek(&self) -> u64 {
        self.next_layer_id
    }

    /// Walk the display list and create layers for scroll frames, filters, opacity, transforms.
    /// Returns a mapping from display-list item index to the LayerId it should render into.
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
                            layer.filters = filters.clone();
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
                            m[0][0] as f64,
                            m[0][1] as f64,
                            m[1][0] as f64,
                            m[1][1] as f64,
                            m[3][0] as f64,
                            m[3][1] as f64,
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
        for (_, layer) in self.layers.iter_mut() {
            for damage in &damage_rects {
                if let Some(intersection) = rect_intersection(&layer.bounds, damage) {
                    layer.damage.push(intersection);
                    layer.composite_dirty = true;
                }
            }
        }
    }

    /// Render display list items into their respective layer pixbufs.
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

    fn composite_layer_recursive(
        &self,
        layer_id: LayerId,
        output: &mut AzulPixmap,
        parent_offset_x: f32,
        parent_offset_y: f32,
        dpi_factor: f32,
    ) {
        let layer = match self.layers.get(&layer_id) {
            Some(l) => l,
            None => return,
        };

        let abs_x = parent_offset_x + layer.bounds.origin.x;
        let abs_y = parent_offset_y + layer.bounds.origin.y;

        // For root layer, just blit directly
        if layer_id == self.root_layer {
            blit_pixmap(&layer.pixbuf, output, 0, 0, 1.0);
        } else {
            // Apply filters at composite time
            let src = if !layer.filters.is_empty() {
                let mut filtered = layer.pixbuf.clone_pixmap();
                apply_layer_filters(&mut filtered, &layer.filters, dpi_factor);
                Some(filtered)
            } else {
                None
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

        let layer_id = match layer_id {
            Some(id) => id,
            None => return Ok(()), // No layer for this scroll ID
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
        Layer {
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
fn find_matching_pop(items: &[DisplayListItem], start: usize, kind: MatchKind) -> usize {
    let mut depth = 1u32;
    for i in (start + 1)..items.len() {
        match (&items[i], kind) {
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

/// Compute the intersection of two logical rects.
fn rect_intersection(a: &LogicalRect, b: &LogicalRect) -> Option<LogicalRect> {
    let x1 = a.origin.x.max(b.origin.x);
    let y1 = a.origin.y.max(b.origin.y);
    let x2 = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let y2 = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    if x2 > x1 && y2 > y1 {
        Some(LogicalRect {
            origin: LogicalPosition { x: x1, y: y1 },
            size: LogicalSize {
                width: x2 - x1,
                height: y2 - y1,
            },
        })
    } else {
        None
    }
}

/// Blit `src` onto `dst` at pixel position (px_x, px_y) with opacity.
fn blit_pixmap(src: &AzulPixmap, dst: &mut AzulPixmap, px_x: i32, px_y: i32, opacity: f32) {
    let sw = src.width as i32;
    let sh = src.height as i32;
    let dw = dst.width as i32;
    let dh = dst.height as i32;
    let op = (opacity * 255.0).clamp(0.0, 255.0) as u32;

    for sy in 0..sh {
        let dy = px_y + sy;
        if dy < 0 || dy >= dh {
            continue;
        }
        for sx in 0..sw {
            let dx = px_x + sx;
            if dx < 0 || dx >= dw {
                continue;
            }
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((dy * dw + dx) * 4) as usize;
            if si + 3 >= src.data.len() || di + 3 >= dst.data.len() {
                continue;
            }

            let sr = src.data[si] as u32;
            let sg = src.data[si + 1] as u32;
            let sb = src.data[si + 2] as u32;
            let sa = (src.data[si + 3] as u32 * op) / 255;

            if sa == 0 {
                continue;
            }
            if sa == 255 {
                dst.data[di] = sr as u8;
                dst.data[di + 1] = sg as u8;
                dst.data[di + 2] = sb as u8;
                dst.data[di + 3] = 255;
            } else {
                let inv_sa = 255 - sa;
                dst.data[di] = ((sr * sa + dst.data[di] as u32 * inv_sa) / 255) as u8;
                dst.data[di + 1] = ((sg * sa + dst.data[di + 1] as u32 * inv_sa) / 255) as u8;
                dst.data[di + 2] = ((sb * sa + dst.data[di + 2] as u32 * inv_sa) / 255) as u8;
                dst.data[di + 3] = ((sa + dst.data[di + 3] as u32 * inv_sa / 255).min(255)) as u8;
            }
        }
    }
}

/// Shift pixel data in a pixmap by (dx, dy) pixels, clearing exposed regions.
fn shift_pixbuf(pixmap: &mut AzulPixmap, dx: i32, dy: i32) {
    let w = pixmap.width as i32;
    let h = pixmap.height as i32;
    if dx.abs() >= w || dy.abs() >= h {
        // Entire buffer is exposed — just clear it
        pixmap.fill(0, 0, 0, 0);
        return;
    }

    let stride = (w * 4) as usize;
    let data = &mut pixmap.data;

    // Shift rows vertically
    if dy > 0 {
        // Shift down: copy from top to bottom
        for row in (0..h - dy).rev() {
            let src_start = (row * w * 4) as usize;
            let dst_start = ((row + dy) * w * 4) as usize;
            data.copy_within(src_start..src_start + stride, dst_start);
        }
        // Clear top rows
        for row in 0..dy {
            let start = (row * w * 4) as usize;
            data[start..start + stride].fill(0);
        }
    } else if dy < 0 {
        let ady = (-dy) as i32;
        // Shift up: copy from bottom to top
        for row in ady..h {
            let src_start = (row * w * 4) as usize;
            let dst_start = ((row - ady) * w * 4) as usize;
            data.copy_within(src_start..src_start + stride, dst_start);
        }
        // Clear bottom rows
        for row in (h - ady)..h {
            let start = (row * w * 4) as usize;
            data[start..start + stride].fill(0);
        }
    }

    // Shift columns horizontally
    if dx > 0 {
        for row in 0..h {
            let row_start = (row * w * 4) as usize;
            let shift = (dx * 4) as usize;
            // Shift right within the row
            data.copy_within(row_start..row_start + stride - shift, row_start + shift);
            // Clear left columns
            data[row_start..row_start + shift].fill(0);
        }
    } else if dx < 0 {
        let adx = (-dx * 4) as usize;
        for row in 0..h {
            let row_start = (row * w * 4) as usize;
            data.copy_within(row_start + adx..row_start + stride, row_start);
            // Clear right columns
            data[row_start + stride - adx..row_start + stride].fill(0);
        }
    }
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

/// Single-axis VERTICAL move: shift whole rows up (px_dy>0) or down (px_dy<0).
/// Iteration order is chosen so a row read as a source is never already
/// overwritten (src and dst row SETS overlap, so order matters).
#[inline]
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

/// Single-axis HORIZONTAL move: shift each row's pixels left (px_dx>0) or right
/// (px_dx<0). Source and dest overlap WITHIN a row, so `copy_within`'s memmove
/// semantics handle it directly — no per-row ordering needed.
#[inline]
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
/// `copy_within`. Because |px_dy| ≥ 1, the source and dest rows are always
/// DIFFERENT rows ≥ one stride apart, so the per-copy byte ranges never overlap
/// regardless of the horizontal direction — only the row iteration order (by
/// `px_dy` sign) matters, exactly as in the vertical case. This does the work of
/// the two 1-D passes with half the memory traffic.
#[inline]
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
    let start = match start {
        Some(s) => s,
        None => return true, // no frame for this id → nothing to shift
    };
    let end = find_matching_pop(&display_list.items, start, MatchKind::ScrollFrame)
        .min(display_list.items.len());

    // (a) Best case: the SCROLLING content opaquely covers the clip (projected
    // into viewport space by the scroll offset). Then nothing behind can ever
    // show through, so the shift is always safe.
    let content_opaque: Vec<LogicalRect> = display_list.items[start + 1..end]
        .iter()
        .filter_map(|it| opaque_fill_rect(it))
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
    for it in display_list.items[..start].iter() {
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
    while y < y1 {
        let mut x = x0 + step * 0.5;
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
                let radius = ((rx + ry) / 2.0).ceil() as u32;
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
                    chunk[3] = ((chunk[3] as u32 * op) / 255) as u8;
                }
            }
            StyleFilter::Grayscale(pct) => {
                let amount = pct.normalized().clamp(0.0, 1.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    let r = chunk[0] as f32;
                    let g = chunk[1] as f32;
                    let b = chunk[2] as f32;
                    let gray = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    chunk[0] = (r + (gray - r) * amount).clamp(0.0, 255.0) as u8;
                    chunk[1] = (g + (gray - g) * amount).clamp(0.0, 255.0) as u8;
                    chunk[2] = (b + (gray - b) * amount).clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::Brightness(pct) => {
                let factor = pct.normalized().max(0.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    chunk[0] = (chunk[0] as f32 * factor).clamp(0.0, 255.0) as u8;
                    chunk[1] = (chunk[1] as f32 * factor).clamp(0.0, 255.0) as u8;
                    chunk[2] = (chunk[2] as f32 * factor).clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::Contrast(pct) => {
                let factor = pct.normalized().max(0.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    chunk[0] = ((((chunk[0] as f32 / 255.0) - 0.5) * factor + 0.5) * 255.0)
                        .clamp(0.0, 255.0) as u8;
                    chunk[1] = ((((chunk[1] as f32 / 255.0) - 0.5) * factor + 0.5) * 255.0)
                        .clamp(0.0, 255.0) as u8;
                    chunk[2] = ((((chunk[2] as f32 / 255.0) - 0.5) * factor + 0.5) * 255.0)
                        .clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::Invert(pct) => {
                let amount = pct.normalized().clamp(0.0, 1.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    chunk[0] = (chunk[0] as f32 + (255.0 - 2.0 * chunk[0] as f32) * amount)
                        .clamp(0.0, 255.0) as u8;
                    chunk[1] = (chunk[1] as f32 + (255.0 - 2.0 * chunk[1] as f32) * amount)
                        .clamp(0.0, 255.0) as u8;
                    chunk[2] = (chunk[2] as f32 + (255.0 - 2.0 * chunk[2] as f32) * amount)
                        .clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::Sepia(pct) => {
                let amount = pct.normalized().clamp(0.0, 1.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    let r = chunk[0] as f32;
                    let g = chunk[1] as f32;
                    let b = chunk[2] as f32;
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
                    let r = chunk[0] as f32;
                    let g = chunk[1] as f32;
                    let b = chunk[2] as f32;
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
                    let r = chunk[0] as f32;
                    let g = chunk[1] as f32;
                    let b = chunk[2] as f32;
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

/// A simple RGBA pixel buffer. Replaces tiny_skia::Pixmap.
pub struct AzulPixmap {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

impl AzulPixmap {
    /// Create a new pixmap filled with opaque white.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let len = (width as usize) * (height as usize) * 4;
        let data = vec![255u8; len]; // opaque white
        Some(Self {
            data,
            width,
            height,
        })
    }

    /// Fill the entire pixmap with a single color.
    pub fn fill(&mut self, r: u8, g: u8, b: u8, a: u8) {
        for chunk in self.data.chunks_exact_mut(4) {
            chunk[0] = r;
            chunk[1] = g;
            chunk[2] = b;
            chunk[3] = a;
        }
    }

    /// Fill a rectangular region with a single color (pixel coordinates).
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8, a: u8) {
        let pw = self.width as i32;
        let ph = self.height as i32;
        let x0 = x.max(0).min(pw);
        let y0 = y.max(0).min(ph);
        let x1 = (x + w).max(0).min(pw);
        let y1 = (y + h).max(0).min(ph);
        for row in y0..y1 {
            let start = (row * pw + x0) as usize * 4;
            let end = (row * pw + x1) as usize * 4;
            if end <= self.data.len() {
                for chunk in self.data[start..end].chunks_exact_mut(4) {
                    chunk[0] = r;
                    chunk[1] = g;
                    chunk[2] = b;
                    chunk[3] = a;
                }
            }
        }
    }

    /// Raw RGBA pixel data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable raw RGBA pixel data.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Create a clone of this pixmap (for filter application).
    pub fn clone_pixmap(&self) -> Self {
        Self {
            data: self.data.clone(),
            width: self.width,
            height: self.height,
        }
    }

    /// Resize the pixmap preserving existing content in the top-left corner.
    /// New right/bottom strips are filled with the specified color.
    /// Only grows — returns None if new dimensions are smaller (caller should realloc).
    pub fn resize_grow_only(
        &mut self,
        new_width: u32,
        new_height: u32,
        fill_r: u8,
        fill_g: u8,
        fill_b: u8,
        fill_a: u8,
    ) -> Option<()> {
        if new_width < self.width || new_height < self.height {
            return None;
        }
        if new_width == self.width && new_height == self.height {
            return Some(());
        }

        let old_w = self.width as usize;
        let old_h = self.height as usize;
        let new_w = new_width as usize;
        let new_h = new_height as usize;
        let mut new_data = vec![fill_a; new_w * new_h * 4];

        // Fill entire buffer with fill color first (covers right + bottom strips)
        for chunk in new_data.chunks_exact_mut(4) {
            chunk[0] = fill_r;
            chunk[1] = fill_g;
            chunk[2] = fill_b;
            chunk[3] = fill_a;
        }

        // Copy old rows into top-left corner
        let old_stride = old_w * 4;
        let new_stride = new_w * 4;
        for row in 0..old_h {
            let src = row * old_stride;
            let dst = row * new_stride;
            new_data[dst..dst + old_stride].copy_from_slice(&self.data[src..src + old_stride]);
        }

        self.data = new_data;
        self.width = new_width;
        self.height = new_height;
        Some(())
    }

    /// Resize the pixmap, reusing existing content for the overlapping region.
    /// Works for both growing and shrinking. New areas are filled with the given color.
    pub fn resize_reuse(
        &mut self,
        new_width: u32,
        new_height: u32,
        fill_r: u8,
        fill_g: u8,
        fill_b: u8,
        fill_a: u8,
    ) {
        if new_width == self.width && new_height == self.height {
            return;
        }

        let old_w = self.width as usize;
        let old_h = self.height as usize;
        let new_w = new_width as usize;
        let new_h = new_height as usize;
        let new_stride = new_w * 4;
        let old_stride = old_w * 4;

        let mut new_data = vec![0u8; new_w * new_h * 4];

        // Fill entire buffer with fill color
        for chunk in new_data.chunks_exact_mut(4) {
            chunk[0] = fill_r;
            chunk[1] = fill_g;
            chunk[2] = fill_b;
            chunk[3] = fill_a;
        }

        // Copy overlapping region from old to new
        let copy_rows = old_h.min(new_h);
        let copy_cols_bytes = old_stride.min(new_stride);
        for row in 0..copy_rows {
            let src = row * old_stride;
            let dst = row * new_stride;
            new_data[dst..dst + copy_cols_bytes]
                .copy_from_slice(&self.data[src..src + copy_cols_bytes]);
        }

        self.data = new_data;
        self.width = new_width;
        self.height = new_height;
    }

    /// Encode to PNG using the `png` crate.
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| format!("PNG header error: {}", e))?;
            writer
                .write_image_data(&self.data)
                .map_err(|e| format!("PNG write error: {}", e))?;
        }
        Ok(buf)
    }

    /// Decode a PNG byte slice into an AzulPixmap.
    pub fn decode_png(png_bytes: &[u8]) -> Result<Self, String> {
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("PNG decode error: {}", e))?;
        let buf_size = reader
            .output_buffer_size()
            .ok_or_else(|| "PNG: unknown output buffer size".to_string())?;
        let mut buf = vec![0u8; buf_size];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| format!("PNG frame error: {}", e))?;
        let width = info.width;
        let height = info.height;

        // Convert to RGBA if needed
        let data = match info.color_type {
            png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
            png::ColorType::Rgb => {
                let mut rgba = Vec::with_capacity((width * height * 4) as usize);
                for chunk in buf[..info.buffer_size()].chunks_exact(3) {
                    rgba.push(chunk[0]);
                    rgba.push(chunk[1]);
                    rgba.push(chunk[2]);
                    rgba.push(255);
                }
                rgba
            }
            png::ColorType::Grayscale => {
                let mut rgba = Vec::with_capacity((width * height * 4) as usize);
                for &v in &buf[..info.buffer_size()] {
                    rgba.push(v);
                    rgba.push(v);
                    rgba.push(v);
                    rgba.push(255);
                }
                rgba
            }
            other => return Err(format!("Unsupported PNG color type: {:?}", other)),
        };

        Ok(Self {
            data,
            width,
            height,
        })
    }
}

// ============================================================================
// Pixel-diff comparison for regression testing
// ============================================================================

/// Result of comparing two pixmaps pixel-by-pixel.
#[derive(Debug, Clone)]
pub struct PixelDiffResult {
    /// Number of pixels that differ beyond the threshold.
    pub diff_count: u64,
    /// Total number of pixels compared.
    pub total_pixels: u64,
    /// Maximum per-channel delta found across all pixels.
    pub max_delta: u8,
    /// Whether dimensions matched.
    pub dimensions_match: bool,
    /// Width of the reference image.
    pub ref_width: u32,
    /// Height of the reference image.
    pub ref_height: u32,
    /// Width of the test image.
    pub test_width: u32,
    /// Height of the test image.
    pub test_height: u32,
}

impl PixelDiffResult {
    /// True if the images are identical within tolerance.
    pub fn is_match(&self) -> bool {
        self.dimensions_match && self.diff_count == 0
    }

    /// Fraction of pixels that differ (0.0 = identical, 1.0 = all different).
    pub fn diff_ratio(&self) -> f64 {
        if self.total_pixels == 0 {
            0.0
        } else {
            self.diff_count as f64 / self.total_pixels as f64
        }
    }
}

/// Compare two pixmaps pixel-by-pixel with a per-channel tolerance.
///
/// `threshold` is the maximum allowed per-channel difference (0 = exact match,
/// 2-3 = anti-aliasing tolerance, 10+ = loose match).
pub fn pixel_diff(reference: &AzulPixmap, test: &AzulPixmap, threshold: u8) -> PixelDiffResult {
    let dimensions_match = reference.width == test.width && reference.height == test.height;
    if !dimensions_match {
        return PixelDiffResult {
            diff_count: 0,
            total_pixels: 0,
            max_delta: 0,
            dimensions_match: false,
            ref_width: reference.width,
            ref_height: reference.height,
            test_width: test.width,
            test_height: test.height,
        };
    }

    let total_pixels = (reference.width as u64) * (reference.height as u64);
    let mut diff_count = 0u64;
    let mut max_delta = 0u8;

    for (ref_chunk, test_chunk) in reference
        .data
        .chunks_exact(4)
        .zip(test.data.chunks_exact(4))
    {
        let mut pixel_differs = false;
        for c in 0..4 {
            let delta = (ref_chunk[c] as i16 - test_chunk[c] as i16).unsigned_abs() as u8;
            if delta > threshold {
                pixel_differs = true;
            }
            if delta > max_delta {
                max_delta = delta;
            }
        }
        if pixel_differs {
            diff_count += 1;
        }
    }

    PixelDiffResult {
        diff_count,
        total_pixels,
        max_delta,
        dimensions_match: true,
        ref_width: reference.width,
        ref_height: reference.height,
        test_width: test.width,
        test_height: test.height,
    }
}

/// Compare a rendered pixmap against a reference PNG file.
///
/// Returns `Ok(result)` with the diff stats, or `Err` if the reference
/// file cannot be read/decoded.
pub fn compare_against_reference(
    rendered: &AzulPixmap,
    reference_png_path: &str,
    threshold: u8,
) -> Result<PixelDiffResult, String> {
    let ref_bytes = std::fs::read(reference_png_path)
        .map_err(|e| format!("Cannot read reference image {}: {}", reference_png_path, e))?;
    let reference = AzulPixmap::decode_png(&ref_bytes)?;
    Ok(pixel_diff(&reference, rendered, threshold))
}

// ============================================================================
// Simple rect type (replaces tiny_skia::Rect)
// ============================================================================

#[derive(Debug, Clone, Copy)]
struct AzRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl AzRect {
    fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        if w <= 0.0
            || h <= 0.0
            || !x.is_finite()
            || !y.is_finite()
            || !w.is_finite()
            || !h.is_finite()
        {
            return None;
        }
        Some(Self {
            x,
            y,
            width: w,
            height: h,
        })
    }

    /// Intersect this rect with a clip rect. Returns None if fully clipped.
    fn clip(&self, clip: &AzRect) -> Option<AzRect> {
        let x1 = self.x.max(clip.x);
        let y1 = self.y.max(clip.y);
        let x2 = (self.x + self.width).min(clip.x + clip.width);
        let y2 = (self.y + self.height).min(clip.y + clip.height);
        if x2 > x1 && y2 > y1 {
            Some(AzRect {
                x: x1,
                y: y1,
                width: x2 - x1,
                height: y2 - y1,
            })
        } else {
            None
        }
    }
}

// ============================================================================
// AGG helper: fill a PathStorage with a solid color into an AzulPixmap
// ============================================================================

fn agg_fill_path(
    pixmap: &mut AzulPixmap,
    path: &mut dyn VertexSource,
    color: &Rgba8,
    rule: FillingRule,
) {
    agg_fill_path_clipped(pixmap, path, color, rule, None);
}

/// Fill a path with an optional pixel-level clip box.
///
/// When `clip` is `Some`, `RendererBase::clip_box_i()` restricts all
/// scanline output to the clip region.  This handles scroll-frame clips,
/// border-radius is TODO (would need a mask), transforms are handled by
/// transforming the clip box through the inverse transform before setting it.
fn agg_fill_path_clipped(
    pixmap: &mut AzulPixmap,
    path: &mut dyn VertexSource,
    color: &Rgba8,
    rule: FillingRule,
    clip: Option<AzRect>,
) {
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;
    let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride) };
    let mut pf = PixfmtRgba32::new(&mut ra);
    let mut rb = RendererBase::new(pf);
    if let Some(c) = clip {
        rb.clip_box_i(
            c.x as i32,
            c.y as i32,
            (c.x + c.width) as i32 - 1,
            (c.y + c.height) as i32 - 1,
        );
    }
    let mut ras = RasterizerScanlineAa::new();
    ras.filling_rule(rule);
    ras.add_path(path, 0);
    let mut sl = ScanlineU8::new();
    render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, color);
}

fn agg_fill_transformed_path(
    pixmap: &mut AzulPixmap,
    path: &mut PathStorage,
    color: &Rgba8,
    rule: FillingRule,
    transform: &TransAffine,
) {
    agg_fill_transformed_path_clipped(pixmap, path, color, rule, transform, None);
}

fn agg_fill_transformed_path_clipped(
    pixmap: &mut AzulPixmap,
    path: &mut PathStorage,
    color: &Rgba8,
    rule: FillingRule,
    transform: &TransAffine,
    clip: Option<AzRect>,
) {
    if transform.is_identity(IDENTITY_EPSILON_F64) {
        agg_fill_path_clipped(pixmap, path, color, rule, clip);
    } else {
        let mut transformed = ConvTransform::new(path, transform.clone());
        agg_fill_path_clipped(pixmap, &mut transformed, color, rule, clip);
    }
}

// ============================================================================
// AGG helper: fill a path with a gradient into an AzulPixmap
// ============================================================================

fn agg_fill_gradient<G: GradientFunction>(
    pixmap: &mut AzulPixmap,
    path: &mut dyn VertexSource,
    lut: &GradientLut,
    gradient_fn: G,
    transform: TransAffine,
    d1: f64,
    d2: f64,
) {
    agg_fill_gradient_clipped(pixmap, path, lut, gradient_fn, transform, d1, d2, None);
}

fn agg_fill_gradient_clipped<G: GradientFunction>(
    pixmap: &mut AzulPixmap,
    path: &mut dyn VertexSource,
    lut: &GradientLut,
    gradient_fn: G,
    transform: TransAffine,
    d1: f64,
    d2: f64,
    clip: Option<AzRect>,
) {
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;
    let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride) };
    let mut pf = PixfmtRgba32::new(&mut ra);
    let mut rb = RendererBase::new(pf);
    if let Some(c) = clip {
        rb.clip_box_i(
            c.x as i32,
            c.y as i32,
            (c.x + c.width) as i32 - 1,
            (c.y + c.height) as i32 - 1,
        );
    }
    let mut ras = RasterizerScanlineAa::new();
    ras.filling_rule(FillingRule::NonZero);
    ras.add_path(path, 0);
    let mut sl = ScanlineU8::new();

    let interp = SpanInterpolatorLinear::new(transform);
    let mut sg = SpanGradient::new(interp, gradient_fn, lut, d1, d2);
    let mut alloc = SpanAllocator::<Rgba8>::new();
    render_scanlines_aa(&mut ras, &mut sl, &mut rb, &mut alloc, &mut sg);
}

// ============================================================================
// Gradient helpers
// ============================================================================

/// Fallback color used when a `system:*` keyword cannot be resolved
/// (for example because no `SystemStyle` is attached to the
/// [`CpuRenderState`], or because the requested key is unset on the
/// current platform). CSS Images Level 4 leaves the color undefined in
/// this case; transparent black means the stop simply contributes
/// nothing to the gradient instead of poisoning it with an arbitrary
/// visible color (the previous behaviour was hardcoded mid-gray, which
/// produced visibly wrong output).
const SYSTEM_COLOR_FALLBACK: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

/// Resolve a `ColorOrSystem` against the optional system palette.
///
/// Concrete colors are returned verbatim. `system:*` keywords are
/// resolved against `system_colors` when available and fall back to
/// `SYSTEM_COLOR_FALLBACK` otherwise.
fn resolve_color(
    color: &ColorOrSystem,
    system_colors: Option<&azul_css::system::SystemColors>,
) -> ColorU {
    match (color, system_colors) {
        (ColorOrSystem::Color(c), _) => *c,
        (ColorOrSystem::System(_), Some(sc)) => color.resolve(sc, SYSTEM_COLOR_FALLBACK),
        (ColorOrSystem::System(_), None) => SYSTEM_COLOR_FALLBACK,
    }
}

/// Build a GradientLut from normalized linear color stops.
fn build_gradient_lut_linear(
    stops: &azul_css::props::style::background::NormalizedLinearColorStopVec,
    system_colors: Option<&azul_css::system::SystemColors>,
) -> GradientLut {
    let mut lut = GradientLut::new_default();
    let stops_slice = stops.as_ref();
    if stops_slice.len() < 2 {
        // Need at least 2 stops; fill with transparent
        lut.add_color(0.0, Rgba8::new(0, 0, 0, 0));
        lut.add_color(1.0, Rgba8::new(0, 0, 0, 0));
        lut.build_lut();
        return lut;
    }
    for stop in stops_slice {
        let offset = stop.offset.normalized() as f64; // 0.0..1.0
        let c = resolve_color(&stop.color, system_colors);
        lut.add_color(
            offset,
            Rgba8::new(c.r as u32, c.g as u32, c.b as u32, c.a as u32),
        );
    }
    lut.build_lut();
    lut
}

/// Build a GradientLut from normalized radial (conic) color stops.
fn build_gradient_lut_radial(
    stops: &azul_css::props::style::background::NormalizedRadialColorStopVec,
    system_colors: Option<&azul_css::system::SystemColors>,
) -> GradientLut {
    let mut lut = GradientLut::new_default();
    let stops_slice = stops.as_ref();
    if stops_slice.len() < 2 {
        lut.add_color(0.0, Rgba8::new(0, 0, 0, 0));
        lut.add_color(1.0, Rgba8::new(0, 0, 0, 0));
        lut.build_lut();
        return lut;
    }
    for stop in stops_slice {
        // Conic stops use angle — normalize to 0..1 fraction of full circle
        let offset = (stop.angle.to_degrees() / 360.0).clamp(0.0, 1.0) as f64;
        let c = resolve_color(&stop.color, system_colors);
        lut.add_color(
            offset,
            Rgba8::new(c.r as u32, c.g as u32, c.b as u32, c.a as u32),
        );
    }
    lut.build_lut();
    lut
}

/// Resolve a background position to (x_fraction, y_fraction) in 0..1 range.
fn resolve_background_position(
    pos: &azul_css::props::style::background::StyleBackgroundPosition,
    width: f32,
    height: f32,
) -> (f32, f32) {
    use azul_css::props::style::background::{
        BackgroundPositionHorizontal, BackgroundPositionVertical,
    };

    let x = match pos.horizontal {
        BackgroundPositionHorizontal::Left => 0.0,
        BackgroundPositionHorizontal::Center => 0.5,
        BackgroundPositionHorizontal::Right => 1.0,
        BackgroundPositionHorizontal::Exact(px) => {
            let val = px.to_pixels_internal(width, 16.0, 16.0);
            if width > 0.0 {
                val / width
            } else {
                0.5
            }
        }
    };
    let y = match pos.vertical {
        BackgroundPositionVertical::Top => 0.0,
        BackgroundPositionVertical::Center => 0.5,
        BackgroundPositionVertical::Bottom => 1.0,
        BackgroundPositionVertical::Exact(px) => {
            let val = px.to_pixels_internal(height, 16.0, 16.0);
            if height > 0.0 {
                val / height
            } else {
                0.5
            }
        }
    };
    (x, y)
}

fn render_linear_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::LinearGradient,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
    system_colors: Option<&azul_css::system::SystemColors>,
) -> Result<(), String> {
    use azul_css::props::basic::geometry::{LayoutRect, LayoutSize};

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let stops = gradient.stops.as_ref();
    if stops.is_empty() {
        return Ok(());
    }

    let lut = build_gradient_lut_linear(&gradient.stops, system_colors);

    // Convert Direction to start/end points using the existing to_points method
    let layout_rect = LayoutRect {
        origin: azul_css::props::basic::geometry::LayoutPoint::new(0, 0),
        size: LayoutSize {
            width: (rect.width as isize),
            height: (rect.height as isize),
        },
    };
    let (from_pt, to_pt) = gradient.direction.to_points(&layout_rect);

    // Pixel-space start/end
    let x1 = rect.x as f64 + from_pt.x as f64;
    let y1 = rect.y as f64 + from_pt.y as f64;
    let x2 = rect.x as f64 + to_pt.x as f64;
    let y2 = rect.y as f64 + to_pt.y as f64;

    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return Ok(());
    }

    // gradient-space (0..100, 0) → pixel-space line (x1,y1)→(x2,y2). Use agg's
    // helper so the composition order is T * R * S — hand-rolling it via
    // new_translation().rotate().scale() pre-multiplies and ends up as
    // S * R * T, which rotates the translation and yields out-of-range gx.
    let mut transform = TransAffine::new_line_segment(x1, y1, x2, y2, 100.0);
    transform.invert();

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient_clipped(
        pixmap, &mut path, &lut, GradientX, transform, 0.0, 100.0, clip,
    );
    Ok(())
}

fn render_radial_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::RadialGradient,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
    system_colors: Option<&azul_css::system::SystemColors>,
) -> Result<(), String> {
    use azul_css::props::style::background::{RadialGradientSize, Shape};

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let stops = gradient.stops.as_ref();
    if stops.is_empty() {
        return Ok(());
    }

    let lut = build_gradient_lut_linear(&gradient.stops, system_colors);

    let w = rect.width as f64;
    let h = rect.height as f64;

    // Compute center from position
    let (cx_frac, cy_frac) =
        resolve_background_position(&gradient.position, rect.width, rect.height);
    let cx = rect.x as f64 + cx_frac as f64 * w;
    let cy = rect.y as f64 + cy_frac as f64 * h;

    // Compute radius based on shape and size
    let radius = match gradient.size {
        RadialGradientSize::ClosestSide => {
            let dx = (cx_frac as f64 * w).min((1.0 - cx_frac as f64) * w);
            let dy = (cy_frac as f64 * h).min((1.0 - cy_frac as f64) * h);
            match gradient.shape {
                Shape::Circle => dx.min(dy),
                Shape::Ellipse => dx.min(dy), // simplified
            }
        }
        RadialGradientSize::FarthestSide => {
            let dx = (cx_frac as f64 * w).max((1.0 - cx_frac as f64) * w);
            let dy = (cy_frac as f64 * h).max((1.0 - cy_frac as f64) * h);
            match gradient.shape {
                Shape::Circle => dx.max(dy),
                Shape::Ellipse => dx.max(dy),
            }
        }
        RadialGradientSize::ClosestCorner => {
            let dx = (cx_frac as f64 * w).min((1.0 - cx_frac as f64) * w);
            let dy = (cy_frac as f64 * h).min((1.0 - cy_frac as f64) * h);
            (dx * dx + dy * dy).sqrt()
        }
        RadialGradientSize::FarthestCorner => {
            let dx = (cx_frac as f64 * w).max((1.0 - cx_frac as f64) * w);
            let dy = (cy_frac as f64 * h).max((1.0 - cy_frac as f64) * h);
            (dx * dx + dy * dy).sqrt()
        }
    };

    if radius < 0.001 {
        return Ok(());
    }

    // Gradient-space (radius=100 at distance=100) → pixel-space around (cx, cy).
    // Build as T * S (scale first, then translate) so S only affects the radius.
    // scale() pre-multiplies so we must start from scaling matrix.
    let mut transform = TransAffine::new_scaling_uniform(radius / 100.0);
    transform.translate(cx, cy);
    transform.invert();

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient_clipped(
        pixmap,
        &mut path,
        &lut,
        GradientRadialD,
        transform,
        0.0,
        100.0,
        clip,
    );
    Ok(())
}

fn render_conic_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::ConicGradient,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
    system_colors: Option<&azul_css::system::SystemColors>,
) -> Result<(), String> {
    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let stops = gradient.stops.as_ref();
    if stops.is_empty() {
        return Ok(());
    }

    let lut = build_gradient_lut_radial(&gradient.stops, system_colors);

    let w = rect.width as f64;
    let h = rect.height as f64;

    // Compute center
    let (cx_frac, cy_frac) = resolve_background_position(&gradient.center, rect.width, rect.height);
    let cx = rect.x as f64 + cx_frac as f64 * w;
    let cy = rect.y as f64 + cy_frac as f64 * h;

    // Start angle (CSS conic gradients start at 12 o'clock = -90deg in math coords)
    let start_angle_deg = gradient.angle.to_degrees();
    let start_angle_rad = ((start_angle_deg - 90.0) as f64).to_radians();

    // Forward: gradient angle θ → pixel rotated by start_angle around (cx, cy).
    // Build as T * R so rotation is applied before translation (rotate() pre-multiplies,
    // so start from rotation matrix and translate last).
    let mut transform = TransAffine::new_rotation(start_angle_rad);
    transform.translate(cx, cy);
    transform.invert();

    // GradientConic maps atan2(y,x) * d / pi, covering [0, d] for the half-circle.
    // We use d2 = 100 as the range; the LUT maps 0..1 over that.
    let d2 = 100.0;

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient_clipped(
        pixmap,
        &mut path,
        &lut,
        GradientConic,
        transform,
        0.0,
        d2,
        clip,
    );
    Ok(())
}

// ============================================================================
// Box shadow rendering
// ============================================================================

fn render_box_shadow(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    shadow: &azul_css::props::style::box_shadow::StyleBoxShadow,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> Result<(), String> {
    use azul_css::props::style::box_shadow::BoxShadowClipMode;

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let offset_x =
        shadow
            .offset_x
            .inner
            .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
            * dpi_factor;
    let offset_y =
        shadow
            .offset_y
            .inner
            .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
            * dpi_factor;
    let blur_r =
        (shadow
            .blur_radius
            .inner
            .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
            * dpi_factor)
            .max(0.0);
    let spread =
        shadow
            .spread_radius
            .inner
            .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
            * dpi_factor;

    let color = shadow.color;
    if color.a == 0 {
        return Ok(());
    }

    // Compute shadow rect (expanded by spread, padded by blur)
    let padding = blur_r.ceil();
    let shadow_x = rect.x + offset_x - spread - padding;
    let shadow_y = rect.y + offset_y - spread - padding;
    let shadow_w = rect.width + 2.0 * spread + 2.0 * padding;
    let shadow_h = rect.height + 2.0 * spread + 2.0 * padding;

    if shadow_w <= 0.0 || shadow_h <= 0.0 {
        return Ok(());
    }

    let sw = shadow_w.ceil() as u32;
    let sh = shadow_h.ceil() as u32;

    if sw == 0 || sh == 0 || sw > MAX_SHADOW_PIXBUF_SIZE || sh > MAX_SHADOW_PIXBUF_SIZE {
        return Ok(());
    }

    // Create temp buffer and draw the shadow shape into it
    let mut tmp = AzulPixmap::new(sw, sh).ok_or("cannot create shadow pixmap")?;
    tmp.fill(0, 0, 0, 0); // transparent

    // The shape origin within the temp buffer
    let shape_x = padding + spread;
    let shape_y = padding + spread;
    let shape_rect = match AzRect::from_xywh(shape_x, shape_y, rect.width, rect.height) {
        Some(r) => r,
        None => return Ok(()),
    };

    let agg_color = Rgba8::new(
        color.r as u32,
        color.g as u32,
        color.b as u32,
        color.a as u32,
    );
    if border_radius.is_zero() {
        let mut path = build_rect_path(&shape_rect);
        agg_fill_path(&mut tmp, &mut path, &agg_color, FillingRule::NonZero);
    } else {
        let mut path = build_rounded_rect_path(&shape_rect, border_radius, dpi_factor);
        agg_fill_path(&mut tmp, &mut path, &agg_color, FillingRule::NonZero);
    }

    // Apply blur
    if blur_r > 0.5 {
        let blur_radius = (blur_r.ceil() as u32).min(254);
        let stride = (sw * 4) as i32;
        let mut ra = unsafe { RowAccessor::new_with_buf(tmp.data.as_mut_ptr(), sw, sh, stride) };
        stack_blur_rgba32(&mut ra, blur_radius, blur_radius);
    }

    // Blit the shadow buffer onto the main pixmap
    let dst_x = shadow_x as i32;
    let dst_y = shadow_y as i32;
    blit_buffer(pixmap, &tmp.data, sw, sh, dst_x, dst_y);

    Ok(())
}

/// Alpha-blend one premultiplied-alpha RGBA buffer onto another at (dx, dy).
fn blit_buffer(dst: &mut AzulPixmap, src: &[u8], src_w: u32, src_h: u32, dx: i32, dy: i32) {
    let dw = dst.width as i32;
    let dh = dst.height as i32;

    for py in 0..src_h as i32 {
        let ty = dy + py;
        if ty < 0 || ty >= dh {
            continue;
        }
        for px in 0..src_w as i32 {
            let tx = dx + px;
            if tx < 0 || tx >= dw {
                continue;
            }

            let si = ((py as u32 * src_w + px as u32) * 4) as usize;
            let di = ((ty as u32 * dst.width + tx as u32) * 4) as usize;

            if si + 3 >= src.len() || di + 3 >= dst.data.len() {
                continue;
            }

            let sa = src[si + 3] as u32;
            if sa == 0 {
                continue;
            }
            if sa == 255 {
                dst.data[di] = src[si];
                dst.data[di + 1] = src[si + 1];
                dst.data[di + 2] = src[si + 2];
                dst.data[di + 3] = 255;
            } else {
                // Premultiplied-alpha compositing: src RGB already premultiplied by AGG
                let inv_sa = 255 - sa;
                dst.data[di] =
                    ((src[si] as u32 + dst.data[di] as u32 * inv_sa / 255).min(255)) as u8;
                dst.data[di + 1] =
                    ((src[si + 1] as u32 + dst.data[di + 1] as u32 * inv_sa / 255).min(255)) as u8;
                dst.data[di + 2] =
                    ((src[si + 2] as u32 + dst.data[di + 2] as u32 * inv_sa / 255).min(255)) as u8;
                dst.data[di + 3] = ((sa + dst.data[di + 3] as u32 * inv_sa / 255).min(255)) as u8;
            }
        }
    }
}

// ============================================================================
// Image mask clipping
// ============================================================================

/// Entry on the mask/opacity stack.
enum MaskEntry {
    /// Image mask clip (R8 mask).
    ImageMask {
        snapshot: Vec<u8>,
        mask_data: Vec<u8>,
        origin_x: i32,
        origin_y: i32,
        width: u32,
        height: u32,
    },
    /// Opacity layer.
    Opacity {
        snapshot: Vec<u8>,
        rect: AzRect,
        opacity: f32,
    },
}

/// Take a snapshot of a rectangular region of the pixmap.
fn snapshot_region(pixmap: &AzulPixmap, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
    let pw = pixmap.width as i32;
    let ph = pixmap.height as i32;
    let mut snap = vec![0u8; (w as usize) * (h as usize) * 4];

    for py in 0..h as i32 {
        let sy = y + py;
        if sy < 0 || sy >= ph {
            continue;
        }
        for px in 0..w as i32 {
            let sx = x + px;
            if sx < 0 || sx >= pw {
                continue;
            }
            let si = ((sy as u32 * pixmap.width + sx as u32) * 4) as usize;
            let di = ((py as u32 * w + px as u32) * 4) as usize;
            if si + 3 < pixmap.data.len() && di + 3 < snap.len() {
                snap[di] = pixmap.data[si];
                snap[di + 1] = pixmap.data[si + 1];
                snap[di + 2] = pixmap.data[si + 2];
                snap[di + 3] = pixmap.data[si + 3];
            }
        }
    }
    snap
}

/// Extract and scale mask image data (R8) to target dimensions.
fn extract_mask_data(mask_image: &ImageRef, target_w: u32, target_h: u32) -> Option<Vec<u8>> {
    let image_data = mask_image.get_data();
    let (mask_bytes, src_w, src_h) = match &*image_data {
        DecodedImage::Raw((descriptor, data)) => {
            let w = descriptor.width as u32;
            let h = descriptor.height as u32;
            if w == 0 || h == 0 {
                return None;
            }
            let bytes = match data {
                azul_core::resources::ImageData::Raw(shared) => shared.as_ref(),
                _ => return None,
            };
            match descriptor.format {
                azul_core::resources::RawImageFormat::R8 => (bytes.to_vec(), w, h),
                azul_core::resources::RawImageFormat::BGRA8 => {
                    // Use alpha channel as mask
                    let mut r8 = Vec::with_capacity((w * h) as usize);
                    for chunk in bytes.chunks_exact(4) {
                        r8.push(chunk[3]); // alpha
                    }
                    (r8, w, h)
                }
                _ => {
                    // Use first channel as grayscale mask
                    let chan_count = bytes.len() / (w * h) as usize;
                    if chan_count == 0 {
                        return None;
                    }
                    let mut r8 = Vec::with_capacity((w * h) as usize);
                    for i in 0..(w * h) as usize {
                        r8.push(bytes[i * chan_count]);
                    }
                    (r8, w, h)
                }
            }
        }
        _ => return None,
    };

    if target_w == 0 || target_h == 0 {
        return None;
    }

    // Scale mask to target dimensions via nearest-neighbor
    let mut scaled = vec![0u8; (target_w * target_h) as usize];
    let sx = src_w as f32 / target_w as f32;
    let sy = src_h as f32 / target_h as f32;
    for py in 0..target_h {
        for px in 0..target_w {
            let mx = ((px as f32 * sx) as u32).min(src_w - 1);
            let my = ((py as f32 * sy) as u32).min(src_h - 1);
            scaled[(py * target_w + px) as usize] = mask_bytes[(my * src_w + mx) as usize];
        }
    }
    Some(scaled)
}

/// Apply a mask: for each pixel in the mask region, blend between the snapshot
/// (pre-mask state) and the current pixmap state using the mask value.
fn apply_mask(pixmap: &mut AzulPixmap, entry: &MaskEntry) {
    let (snapshot, mask_data, origin_x, origin_y, width, height) = match entry {
        MaskEntry::ImageMask {
            snapshot,
            mask_data,
            origin_x,
            origin_y,
            width,
            height,
        } => (
            snapshot,
            mask_data.as_slice(),
            *origin_x,
            *origin_y,
            *width,
            *height,
        ),
        _ => return,
    };

    let pw = pixmap.width as i32;
    let ph = pixmap.height as i32;

    for py in 0..height as i32 {
        let dy = origin_y + py;
        if dy < 0 || dy >= ph {
            continue;
        }
        for px in 0..width as i32 {
            let dx = origin_x + px;
            if dx < 0 || dx >= pw {
                continue;
            }

            let mi = (py as u32 * width + px as u32) as usize;
            let mask_val = mask_data.get(mi).copied().unwrap_or(0) as u32;

            let pi = ((dy as u32 * pixmap.width + dx as u32) * 4) as usize;
            let si = ((py as u32 * width + px as u32) * 4) as usize;

            if pi + 3 >= pixmap.data.len() || si + 3 >= snapshot.len() {
                continue;
            }

            // Blend: result = snapshot * (255 - mask) + current * mask
            // mask_val 255 = fully visible (keep current), 0 = fully clipped (restore snapshot)
            let inv_mask = 255 - mask_val;
            for c in 0..4 {
                let snap_c = snapshot[si + c] as u32;
                let cur_c = pixmap.data[pi + c] as u32;
                pixmap.data[pi + c] = ((cur_c * mask_val + snap_c * inv_mask) / 255) as u8;
            }
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

pub struct RenderOptions {
    pub width: f32,
    pub height: f32,
    pub dpi_factor: f32,
}

/// Reuse `retained` pixmap if it matches the target dimensions, otherwise allocate new.
fn acquire_pixmap(retained: Option<AzulPixmap>, w: u32, h: u32) -> Result<AzulPixmap, String> {
    if let Some(p) = retained {
        if p.width == w && p.height == h {
            return Ok(p);
        }
    }
    AzulPixmap::new(w, h).ok_or_else(|| "cannot create pixmap".to_string())
}

pub fn render(
    dl: &DisplayList,
    res: &RendererResources,
    opts: RenderOptions,
    glyph_cache: &mut GlyphCache,
) -> Result<AzulPixmap, String> {
    let RenderOptions {
        width,
        height,
        dpi_factor,
    } = opts;

    let mut pixmap = acquire_pixmap(
        None,
        (width * dpi_factor) as u32,
        (height * dpi_factor) as u32,
    )?;
    pixmap.fill(255, 255, 255, 255);

    render_display_list(dl, &mut pixmap, dpi_factor, res, None, glyph_cache)?;

    Ok(pixmap)
}

/// Render a display list using fonts from FontManager directly.
/// This is used in reftest scenarios where RendererResources doesn't have fonts registered.
pub fn render_with_font_manager(
    dl: &DisplayList,
    res: &RendererResources,
    font_manager: &FontManager<FontRef>,
    opts: RenderOptions,
    glyph_cache: &mut GlyphCache,
) -> Result<AzulPixmap, String> {
    let empty_state = CpuRenderState::new(ScrollOffsetMap::new());
    render_with_font_manager_and_scroll(dl, res, font_manager, opts, glyph_cache, &empty_state)
}

/// Render with FontManager and explicit render state (scroll offsets + GPU values).
/// Used by `take_screenshot` to render with the current scroll/transform/opacity state.
pub fn render_with_font_manager_and_scroll(
    dl: &DisplayList,
    res: &RendererResources,
    font_manager: &FontManager<FontRef>,
    opts: RenderOptions,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
) -> Result<AzulPixmap, String> {
    render_with_font_manager_and_scroll_retained(
        dl,
        res,
        font_manager,
        opts,
        glyph_cache,
        render_state,
        None,
    )
}

/// Render with optional retained pixmap. If `retained` is Some and matches
/// the target dimensions, it is reused (cleared to white) instead of
/// allocating a fresh buffer. The pixmap is returned regardless.
pub fn render_with_font_manager_and_scroll_retained(
    dl: &DisplayList,
    res: &RendererResources,
    font_manager: &FontManager<FontRef>,
    opts: RenderOptions,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
    retained: Option<AzulPixmap>,
) -> Result<AzulPixmap, String> {
    let RenderOptions {
        width,
        height,
        dpi_factor,
    } = opts;

    let pw = (width * dpi_factor) as u32;
    let ph = (height * dpi_factor) as u32;
    let mut pixmap = acquire_pixmap(retained, pw, ph)?;
    pixmap.fill(255, 255, 255, 255);

    render_display_list_with_state(
        dl,
        &mut pixmap,
        dpi_factor,
        res,
        Some(font_manager),
        glyph_cache,
        render_state,
    )?;

    Ok(pixmap)
}

/// Scroll offsets keyed by scroll_id (LocalScrollId).
/// Passed to the renderer so it can look up the current scroll position
/// for each PushScrollFrame without embedding it in the display list.
pub type ScrollOffsetMap = HashMap<LocalScrollId, (f32, f32)>;

/// Compute damage rects by comparing two display lists item by item.
///
/// Returns a list of bounding rects that need repainting, or `None` if a
/// full repaint is required (structural change, different item count, etc.).
///
/// The comparison is conservative: any item whose bounds or content changed
/// produces a damage rect covering both the old and new bounds.
pub fn compute_display_list_damage(
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

/// Are two display lists visually identical? (same length, same item
/// discriminants, every item `is_visually_equal`). Cheaper proxy than a
/// structural hash, reusing the same per-item comparison the damage diff uses.
pub fn display_lists_visually_equal(a: &DisplayList, b: &DisplayList) -> bool {
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
/// MapWidget tile arriving on a worker thread and re-invoking the VirtualView
/// in place). So `compute_display_list_damage` — which only diffs the parent —
/// reports "nothing changed", and `render_frame` would skip the frame, freezing
/// the child content. This compares each VirtualView's child DL against the
/// previous frame's and returns the on-screen bounds of every one that differs,
/// so the caller can damage exactly those regions.
///
/// `current` / `previous` are keyed by the child `DomId` (the non-root entries
/// of `layout_results`). A child that is newly present or newly absent counts
/// as changed.
pub fn compute_virtual_view_damage(
    parent: &DisplayList,
    current: &std::collections::BTreeMap<azul_core::dom::DomId, std::sync::Arc<DisplayList>>,
    previous: &std::collections::BTreeMap<azul_core::dom::DomId, std::sync::Arc<DisplayList>>,
) -> Vec<LogicalRect> {
    let mut damage = Vec::new();
    for item in parent.items.iter() {
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

fn rects_overlap_or_adjacent(a: &LogicalRect, b: &LogicalRect, gap: f32) -> bool {
    a.origin.x - gap <= b.origin.x + b.size.width
        && b.origin.x - gap <= a.origin.x + a.size.width
        && a.origin.y - gap <= b.origin.y + b.size.height
        && b.origin.y - gap <= a.origin.y + a.size.height
}

pub fn union_rect(a: &LogicalRect, b: &LogicalRect) -> LogicalRect {
    let x = a.origin.x.min(b.origin.x);
    let y = a.origin.y.min(b.origin.y);
    let right = (a.origin.x + a.size.width).max(b.origin.x + b.size.width);
    let bottom = (a.origin.y + a.size.height).max(b.origin.y + b.size.height);
    LogicalRect {
        origin: LogicalPosition { x, y },
        size: LogicalSize {
            width: right - x,
            height: bottom - y,
        },
    }
}

/// Compute damage rects for a grow-only window resize.
/// Returns the right strip and bottom strip that need rendering.
pub fn compute_resize_damage(
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
pub fn compare_region(
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
            let dr = (a.data[ai] as i16 - b.data[bi] as i16).unsigned_abs() as u8;
            let dg = (a.data[ai + 1] as i16 - b.data[bi + 1] as i16).unsigned_abs() as u8;
            let db = (a.data[ai + 2] as i16 - b.data[bi + 2] as i16).unsigned_abs() as u8;
            if dr > threshold || dg > threshold || db > threshold {
                diff_count += 1;
            }
        }
    }
    diff_count
}

/// Consolidated render-time state for CPU rendering.
///
/// Bundles scroll offsets and GPU-animated values (transforms, opacities)
/// that WebRender would normally manage internally. In cpurender these
/// are looked up from the `GpuValueCache` at screenshot time.
pub struct CpuRenderState {
    /// Scroll offsets by scroll_id
    pub scroll_offsets: ScrollOffsetMap,
    /// Transform values keyed by TransformKey.id — scrollbar thumb positions
    /// and CSS transforms that are GPU-animated in WebRender.
    pub transforms: HashMap<usize, azul_core::transform::ComputedTransform3D>,
    /// Opacity values keyed by OpacityKey.id — scrollbar fade-in/out.
    /// For WhenScrolling mode, opacity is 1.0 when recently scrolled,
    /// fades to 0.0 after idle. For Always mode, opacity is always 1.0.
    pub opacities: HashMap<usize, f32>,
    /// System style for resolving system color references inside gradient
    /// stops (e.g. `system:accent` in macOS button backgrounds). When None,
    /// system color stops fall back to a transparent color.
    pub system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
    /// Display lists of nested `VirtualView` child DOMs, keyed by their
    /// `child_dom_id`. The WebRender path composites these via separate pipelines;
    /// the CPU path has no pipelines, so the `DisplayListItem::VirtualView` arm
    /// recursively rasterises the child's display list from here (translated to the
    /// item's `bounds.origin`, clipped to `bounds`). Empty for non-window renders.
    pub virtual_view_display_lists:
        std::collections::BTreeMap<azul_core::dom::DomId, std::sync::Arc<DisplayList>>,
}

impl CpuRenderState {
    pub fn new(scroll_offsets: ScrollOffsetMap) -> Self {
        Self {
            scroll_offsets,
            transforms: HashMap::new(),
            opacities: HashMap::new(),
            system_style: None,
            virtual_view_display_lists: std::collections::BTreeMap::new(),
        }
    }

    /// Provide the nested `VirtualView` child DOM display lists so the CPU
    /// renderer can composite them (see the field doc).
    pub fn with_virtual_view_display_lists(
        mut self,
        lists: std::collections::BTreeMap<azul_core::dom::DomId, std::sync::Arc<DisplayList>>,
    ) -> Self {
        self.virtual_view_display_lists = lists;
        self
    }

    /// Attach a `SystemStyle` so the renderer can resolve `system:*` color
    /// keywords (e.g. in gradient stops) against the live OS palette.
    pub fn with_system_style(
        mut self,
        system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
    ) -> Self {
        self.system_style = system_style;
        self
    }

    /// Build from a GpuValueCache snapshot.
    pub fn from_gpu_cache(
        gpu_cache: Option<&azul_core::gpu::GpuValueCache>,
        dom_id: azul_core::dom::DomId,
        scroll_offsets: &ScrollOffsetMap,
    ) -> Self {
        let mut transforms = HashMap::new();
        let mut opacities = HashMap::new();

        if let Some(cache) = gpu_cache {
            // Scrollbar thumb transforms (vertical)
            for (node_id, key) in &cache.transform_keys {
                if let Some(value) = cache.current_transform_values.get(node_id) {
                    transforms.insert(key.id, value.clone());
                }
            }
            // Scrollbar thumb transforms (horizontal)
            for (node_id, key) in &cache.h_transform_keys {
                if let Some(value) = cache.h_current_transform_values.get(node_id) {
                    transforms.insert(key.id, value.clone());
                }
            }
            // CSS transforms
            for (node_id, key) in &cache.css_transform_keys {
                if let Some(value) = cache.css_current_transform_values.get(node_id) {
                    transforms.insert(key.id, value.clone());
                }
            }
            // Scrollbar opacity (vertical)
            for ((d, node_id), key) in &cache.scrollbar_v_opacity_keys {
                if *d == dom_id {
                    if let Some(&value) = cache.scrollbar_v_opacity_values.get(&(*d, *node_id)) {
                        opacities.insert(key.id, value);
                    }
                }
            }
            // Scrollbar opacity (horizontal)
            for ((d, node_id), key) in &cache.scrollbar_h_opacity_keys {
                if *d == dom_id {
                    if let Some(&value) = cache.scrollbar_h_opacity_values.get(&(*d, *node_id)) {
                        opacities.insert(key.id, value);
                    }
                }
            }
            // CSS opacity
            for (node_id, key) in &cache.opacity_keys {
                if let Some(&value) = cache.current_opacity_values.get(node_id) {
                    opacities.insert(key.id, value);
                }
            }
        }

        Self {
            scroll_offsets: scroll_offsets.clone(),
            transforms,
            opacities,
            system_style: None,
            virtual_view_display_lists: std::collections::BTreeMap::new(),
        }
    }
}

fn render_display_list(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
) -> Result<(), String> {
    let empty_state = CpuRenderState::new(ScrollOffsetMap::new());
    render_display_list_with_state(
        display_list,
        pixmap,
        dpi_factor,
        renderer_resources,
        font_manager,
        glyph_cache,
        &empty_state,
    )
}

fn render_display_list_with_state(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
) -> Result<(), String> {
    let mut transform_stack = vec![TransAffine::new()]; // identity
    let mut clip_stack: Vec<Option<AzRect>> = vec![None];
    let mut mask_stack: Vec<MaskEntry> = Vec::new();
    // Accumulated scroll offset stack. Each PushScrollFrame pushes
    // (parent_offset_x + scroll_x, parent_offset_y + scroll_y).
    // Items inside a scroll frame have their bounds shifted by the
    // accumulated offset before rendering.
    let mut scroll_offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];

    let _p_loop = crate::probe::Probe::span("raster_loop");
    for item in &display_list.items {
        let _p_item = crate::probe::Probe::span(probe_label_for_item(item));
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

/// Compact item-kind label for [`crate::probe`]. Names must be `'static`
/// strings (probe events store `&'static str` for cheap aggregation),
/// hence the closed match instead of formatting `Debug`.
#[inline]
fn probe_label_for_item(item: &DisplayListItem) -> &'static str {
    use crate::solver3::display_list::DisplayListItem as I;
    match item {
        I::Rect { .. } => "dl:rect",
        I::SelectionRect { .. } => "dl:sel_rect",
        I::CursorRect { .. } => "dl:cursor",
        I::Border { .. } => "dl:border",
        I::Text { .. } => "dl:text",
        I::TextLayout { .. } => "dl:text_layout",
        I::Image { .. } => "dl:image",
        I::ScrollBar { .. } => "dl:scrollbar_raw",
        I::ScrollBarStyled { .. } => "dl:scrollbar",
        I::PushClip { .. } => "dl:push_clip",
        I::PopClip => "dl:pop_clip",
        I::PushScrollFrame { .. } => "dl:push_scroll",
        I::PopScrollFrame => "dl:pop_scroll",
        I::PushStackingContext { .. } => "dl:push_stack",
        I::PopStackingContext => "dl:pop_stack",
        I::PushReferenceFrame { .. } => "dl:push_ref",
        I::PopReferenceFrame => "dl:pop_ref",
        I::PushOpacity { .. } => "dl:push_opacity",
        I::PopOpacity => "dl:pop_opacity",
        I::PushFilter { .. } => "dl:push_filter",
        I::PopFilter => "dl:pop_filter",
        I::PushBackdropFilter { .. } => "dl:push_bdfilter",
        I::PopBackdropFilter => "dl:pop_bdfilter",
        I::PushTextShadow { .. } => "dl:push_tshadow",
        I::PopTextShadow => "dl:pop_tshadow",
        I::PushImageMaskClip { .. } => "dl:push_imask",
        I::PopImageMaskClip => "dl:pop_imask",
        I::LinearGradient { .. } => "dl:linear_grad",
        I::RadialGradient { .. } => "dl:radial_grad",
        I::ConicGradient { .. } => "dl:conic_grad",
        I::BoxShadow { .. } => "dl:box_shadow",
        I::Underline { .. } => "dl:underline",
        I::Strikethrough { .. } => "dl:strike",
        I::Overline { .. } => "dl:overline",
        I::HitTestArea { .. } => "dl:hit",
        I::VirtualView { .. } => "dl:vview",
        I::VirtualViewPlaceholder { .. } => "dl:vview_ph",
    }
}

/// Render only the damaged regions of a display list into a retained pixmap.
///
/// For each damage rect:
/// 1. Clear that region in the pixmap (fill with background color).
/// 2. Iterate all display list items, skip those entirely outside the damage rect.
/// 3. Render intersecting items clipped to the damage rect.
///
/// Push/Pop state commands are always processed (they maintain clip/scroll stacks).
pub fn render_display_list_damaged(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
    damage_rects: &[LogicalRect],
) -> Result<(), String> {
    if damage_rects.is_empty() {
        return Ok(()); // nothing changed
    }

    // Clear damaged regions to white
    for dr in damage_rects {
        let px = (dr.origin.x * dpi_factor) as i32;
        let py = (dr.origin.y * dpi_factor) as i32;
        let pw = (dr.size.width * dpi_factor) as i32;
        let ph = (dr.size.height * dpi_factor) as i32;
        pixmap.fill_rect(px, py, pw, ph, 255, 255, 255, 255);
    }

    // No union needed — items are individually tested against each damage rect
    // below (line-by-line). We iterate items ONCE (not per-rect) to avoid
    // double-rendering items that span multiple rects (alpha-blending artifacts).
    let mut transform_stack = vec![TransAffine::new()];
    let mut clip_stack: Vec<Option<AzRect>> = vec![None]; // no outer clip — per-rect filtering suffices
    let mut mask_stack: Vec<MaskEntry> = Vec::new();
    let mut scroll_offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];

    for item in display_list.items.iter() {
        // Always process state-management items (Push/Pop) regardless of bounds,
        // because skipping a Push while processing its matching Pop corrupts stacks.
        if !item.is_state_management() {
            if let Some(item_bounds) = item.bounds() {
                // Items inside a scroll frame are stored at CONTENT coords but
                // RENDER at `pos - scroll_offset`. The damage rects are in viewport
                // space, so we must apply the current scroll offset to the bounds
                // before the intersection test — otherwise scrolled content is
                // filtered against the wrong position and rows that actually fall
                // in a damage strip get dropped (visible as a missing band).
                let (sdx, sdy) = *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0));
                let test_bounds = if sdx == 0.0 && sdy == 0.0 {
                    item_bounds
                } else {
                    LogicalRect {
                        origin: LogicalPosition {
                            x: item_bounds.origin.x - sdx,
                            y: item_bounds.origin.y - sdy,
                        },
                        size: item_bounds.size,
                    }
                };
                // Check if item intersects ANY damage rect (not just the union)
                let hits_damage = damage_rects
                    .iter()
                    .any(|dr| rects_overlap_or_adjacent(&test_bounds, dr, 0.0));
                if !hits_damage {
                    continue;
                }
            }
        }

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

fn render_single_item(
    item: &DisplayListItem,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
    transform_stack: &mut Vec<TransAffine>,
    clip_stack: &mut Vec<Option<AzRect>>,
    mask_stack: &mut Vec<MaskEntry>,
    scroll_offset_stack: &mut Vec<(f32, f32)>,
    render_state: &CpuRenderState,
) -> Result<(), String> {
    // Current accumulated scroll offset — applied to all item bounds.
    // Negative because scrolling down (positive offset) moves content up.
    let (scroll_dx, scroll_dy) = *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0));

    // Helper: apply scroll offset to a LogicalRect.
    // Items inside scroll frames have absolute window coordinates;
    // the scroll offset shifts them so the visible portion aligns
    // with the clip region.
    let scroll_rect = |r: &LogicalRect| -> LogicalRect {
        if scroll_dx == 0.0 && scroll_dy == 0.0 {
            return *r;
        }
        LogicalRect {
            origin: LogicalPosition {
                x: r.origin.x - scroll_dx,
                y: r.origin.y - scroll_dy,
            },
            size: r.size,
        }
    };

    match item {
        DisplayListItem::Rect {
            bounds,
            color,
            border_radius,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                border_radius,
                clip,
                dpi_factor,
            )?;
        }
        DisplayListItem::SelectionRect {
            bounds,
            color,
            border_radius,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                border_radius,
                clip,
                dpi_factor,
            )?;
        }
        DisplayListItem::CursorRect { bounds, color } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                &BorderRadius::default(),
                clip,
                dpi_factor,
            )?;
        }
        DisplayListItem::Border {
            bounds,
            widths,
            colors,
            styles,
            border_radius,
        } => {
            let default_color = ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            };

            let w_top = widths
                .top
                .and_then(|w| w.get_property().cloned())
                .map(|w| {
                    w.inner
                        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
                })
                .unwrap_or(0.0);
            let w_right = widths
                .right
                .and_then(|w| w.get_property().cloned())
                .map(|w| {
                    w.inner
                        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
                })
                .unwrap_or(0.0);
            let w_bottom = widths
                .bottom
                .and_then(|w| w.get_property().cloned())
                .map(|w| {
                    w.inner
                        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
                })
                .unwrap_or(0.0);
            let w_left = widths
                .left
                .and_then(|w| w.get_property().cloned())
                .map(|w| {
                    w.inner
                        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
                })
                .unwrap_or(0.0);

            let c_top = colors
                .top
                .and_then(|c| c.get_property().cloned())
                .map(|c| c.inner)
                .unwrap_or(default_color);
            let c_right = colors
                .right
                .and_then(|c| c.get_property().cloned())
                .map(|c| c.inner)
                .unwrap_or(default_color);
            let c_bottom = colors
                .bottom
                .and_then(|c| c.get_property().cloned())
                .map(|c| c.inner)
                .unwrap_or(default_color);
            let c_left = colors
                .left
                .and_then(|c| c.get_property().cloned())
                .map(|c| c.inner)
                .unwrap_or(default_color);

            use azul_css::props::style::border::BorderStyle;
            let s_top = styles
                .top
                .and_then(|s| s.get_property().cloned())
                .map(|s| s.inner)
                .unwrap_or(BorderStyle::Solid);
            let s_right = styles
                .right
                .and_then(|s| s.get_property().cloned())
                .map(|s| s.inner)
                .unwrap_or(BorderStyle::Solid);
            let s_bottom = styles
                .bottom
                .and_then(|s| s.get_property().cloned())
                .map(|s| s.inner)
                .unwrap_or(BorderStyle::Solid);
            let s_left = styles
                .left
                .and_then(|s| s.get_property().cloned())
                .map(|s| s.inner)
                .unwrap_or(BorderStyle::Solid);

            let simple_radius = BorderRadius {
                top_left: border_radius.top_left.to_pixels_internal(
                    bounds.0.size.width,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                top_right: border_radius.top_right.to_pixels_internal(
                    bounds.0.size.width,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                bottom_left: border_radius.bottom_left.to_pixels_internal(
                    bounds.0.size.width,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                bottom_right: border_radius.bottom_right.to_pixels_internal(
                    bounds.0.size.width,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
            };

            let clip = *clip_stack.last().unwrap();
            let b = scroll_rect(bounds.inner());

            // If all sides same color/width/style, use single render_border call
            let all_same = c_top == c_right
                && c_top == c_bottom
                && c_top == c_left
                && w_top == w_right
                && w_top == w_bottom
                && w_top == w_left
                && s_top == s_right
                && s_top == s_bottom
                && s_top == s_left;

            if all_same {
                render_border(
                    pixmap,
                    &b,
                    c_top,
                    w_top,
                    s_top,
                    &simple_radius,
                    clip,
                    dpi_factor,
                )?;
            } else {
                // Per-side rendering: render each side separately
                render_border_sides(
                    pixmap,
                    &b,
                    [c_top, c_right, c_bottom, c_left],
                    [w_top, w_right, w_bottom, w_left],
                    [s_top, s_right, s_bottom, s_left],
                    &simple_radius,
                    clip,
                    dpi_factor,
                )?;
            }
        }
        DisplayListItem::Underline {
            bounds,
            color,
            thickness: _,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                &BorderRadius::default(),
                clip,
                dpi_factor,
            )?;
        }
        DisplayListItem::Strikethrough {
            bounds,
            color,
            thickness: _,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                &BorderRadius::default(),
                clip,
                dpi_factor,
            )?;
        }
        DisplayListItem::Overline {
            bounds,
            color,
            thickness: _,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                &BorderRadius::default(),
                clip,
                dpi_factor,
            )?;
        }
        DisplayListItem::Text {
            glyphs,
            font_size_px,
            font_hash,
            color,
            clip_rect,
            ..
        } => {
            let clip = *clip_stack.last().unwrap();
            render_text(
                glyphs,
                *font_hash,
                *font_size_px,
                *color,
                pixmap,
                &scroll_rect(clip_rect.inner()),
                clip,
                renderer_resources,
                font_manager,
                dpi_factor,
                glyph_cache,
                (scroll_dx, scroll_dy),
            )?;
        }
        DisplayListItem::TextLayout {
            layout,
            bounds,
            font_hash,
            font_size_px,
            color,
        } => {
            // TextLayout is metadata for PDF/accessibility - skip in CPU rendering
        }
        DisplayListItem::Image { bounds, image, .. } => {
            let clip = *clip_stack.last().unwrap();
            render_image(
                pixmap,
                &scroll_rect(bounds.inner()),
                image,
                clip,
                dpi_factor,
            )?;
        }
        DisplayListItem::ScrollBar {
            bounds,
            color,
            orientation,
            opacity_key: _,
            hit_id: _,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                &BorderRadius::default(),
                clip,
                dpi_factor,
            )?;
        }
        DisplayListItem::ScrollBarStyled { info } => {
            let clip = *clip_stack.last().unwrap();

            // Resolve scrollbar opacity from the GPU value cache.
            // WhenScrolling mode starts at 0.0 and fades to 1.0 on scroll.
            // In cpurender we read the current value; if none is cached
            // (e.g. headless mode never ran synchronize_scrollbar_opacity)
            // default to 1.0 so the scrollbar is always visible.
            let scrollbar_opacity = info
                .opacity_key
                .and_then(|key| render_state.opacities.get(&key.id).copied())
                .unwrap_or(1.0);

            if scrollbar_opacity > 0.001 {
                // Render track
                if info.track_color.a > 0 {
                    render_rect(
                        pixmap,
                        &scroll_rect(info.track_bounds.inner()),
                        info.track_color,
                        &BorderRadius::default(),
                        clip,
                        dpi_factor,
                    )?;
                }

                // Render decrement button
                if let Some(btn_bounds) = &info.button_decrement_bounds {
                    if info.button_color.a > 0 {
                        render_rect(
                            pixmap,
                            &scroll_rect(btn_bounds.inner()),
                            info.button_color,
                            &BorderRadius::default(),
                            clip,
                            dpi_factor,
                        )?;
                    }
                }

                // Render increment button
                if let Some(btn_bounds) = &info.button_increment_bounds {
                    if info.button_color.a > 0 {
                        render_rect(
                            pixmap,
                            &scroll_rect(btn_bounds.inner()),
                            info.button_color,
                            &BorderRadius::default(),
                            clip,
                            dpi_factor,
                        )?;
                    }
                }

                // Render thumb — the thumb is wrapped in PushReferenceFrame
                // with a thumb_transform_key, so the GPU cache lookup handles
                // positioning dynamically. Here we just apply the initial
                // transform embedded in the display list item as a fallback.
                if info.thumb_color.a > 0 {
                    let thumb_rect = info.thumb_bounds.inner();
                    // Look up live transform from render_state if available
                    let transform = info
                        .thumb_transform_key
                        .and_then(|key| render_state.transforms.get(&key.id))
                        .unwrap_or(&info.thumb_initial_transform);
                    let tx = transform.m[3][0];
                    let ty = transform.m[3][1];
                    let transformed_thumb = LogicalRect {
                        origin: LogicalPosition {
                            x: thumb_rect.origin.x + tx,
                            y: thumb_rect.origin.y + ty,
                        },
                        size: thumb_rect.size,
                    };
                    render_rect(
                        pixmap,
                        &scroll_rect(&transformed_thumb),
                        info.thumb_color,
                        &info.thumb_border_radius,
                        clip,
                        dpi_factor,
                    )?;
                }
            } // end scrollbar_opacity > 0
        }
        DisplayListItem::PushClip {
            bounds,
            border_radius,
        } => {
            let new_clip = logical_rect_to_az_rect(bounds.inner(), dpi_factor);
            clip_stack.push(new_clip);
        }
        DisplayListItem::PopClip => {
            clip_stack.pop();
            if clip_stack.is_empty() {
                return Err("Clip stack underflow".to_string());
            }
        }
        DisplayListItem::PushScrollFrame { scroll_id, .. } => {
            // Scroll frame = scroll offset only.
            // The display list generator always emits PushClip before
            // PushScrollFrame with the same clip bounds, so we don't
            // need to push another clip here — that would double-clip.
            transform_stack.push(
                transform_stack
                    .last()
                    .cloned()
                    .unwrap_or_else(TransAffine::new),
            );
            let frame_offset = render_state
                .scroll_offsets
                .get(scroll_id)
                .copied()
                .unwrap_or((0.0, 0.0));
            let new_scroll = (scroll_dx + frame_offset.0, scroll_dy + frame_offset.1);
            scroll_offset_stack.push(new_scroll);
        }
        DisplayListItem::PopScrollFrame => {
            // Only pop transform and scroll offset — the clip was pushed
            // by a separate PushClip and will be popped by PopClip.
            if transform_stack.len() > 1 {
                transform_stack.pop();
            }
            if scroll_offset_stack.len() > 1 {
                scroll_offset_stack.pop();
            }
        }
        DisplayListItem::HitTestArea { bounds, tag } => {
            // Hit test areas don't render anything
        }
        DisplayListItem::PushStackingContext { z_index, bounds } => {
            // For CPU rendering, stacking contexts are already handled by display list order
        }
        DisplayListItem::PopStackingContext => {}
        DisplayListItem::VirtualView {
            child_dom_id,
            bounds,
            clip_rect,
        } => {
            let _ = clip_rect;
            // Composite the VirtualView's child DOM (a separate LayoutResult the
            // normal layout loop produced — e.g. the MapWidget's tile grid). Its
            // display list is 0-relative, so we (1) clip to the VirtualView's
            // on-screen rect and (2) push a scroll offset of -bounds.origin so the
            // renderer (which draws at `pos - accumulated_scroll`) places the child
            // content at the VirtualView origin. Then recursively rasterise it.
            // (Was: a debug-blue overlay that never drew the child — the reason the
            // CPU backend showed a blank map.)
            let child_dl = render_state.virtual_view_display_lists.get(child_dom_id).cloned();
            #[cfg(feature = "std")]
            if std::env::var("AZ_MAP_DEBUG").is_ok() {
                eprintln!(
                    "[cpu-vview] VirtualView item: child_dom_id={} found={} items={} avail_ids={:?}",
                    child_dom_id.inner,
                    child_dl.is_some(),
                    child_dl.as_ref().map(|d| d.items.len()).unwrap_or(0),
                    render_state.virtual_view_display_lists.keys().map(|k| k.inner).collect::<alloc::vec::Vec<_>>(),
                );
            }
            if let Some(child_dl) = child_dl {
                let vv_origin = bounds.inner().origin;
                clip_stack.push(logical_rect_to_az_rect(&scroll_rect(bounds.inner()), dpi_factor));
                scroll_offset_stack.push((scroll_dx - vv_origin.x, scroll_dy - vv_origin.y));
                for child_item in child_dl.items.iter() {
                    render_single_item(
                        child_item,
                        pixmap,
                        dpi_factor,
                        renderer_resources,
                        font_manager,
                        glyph_cache,
                        transform_stack,
                        clip_stack,
                        mask_stack,
                        scroll_offset_stack,
                        render_state,
                    )?;
                }
                scroll_offset_stack.pop();
                clip_stack.pop();
            }
        }
        DisplayListItem::VirtualViewPlaceholder { .. } => {
            #[cfg(feature = "std")]
            if std::env::var("AZ_MAP_DEBUG").is_ok() {
                eprintln!("[cpu-vview] VirtualViewPlaceholder hit (NOT swapped to a VirtualView item — nothing composites)");
            }
        }

        // Gradient rendering
        DisplayListItem::LinearGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_linear_gradient(
                pixmap,
                &scroll_rect(bounds.inner()),
                gradient,
                border_radius,
                clip,
                dpi_factor,
                render_state.system_style.as_deref().map(|s| &s.colors),
            )?;
        }
        DisplayListItem::RadialGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_radial_gradient(
                pixmap,
                &scroll_rect(bounds.inner()),
                gradient,
                border_radius,
                clip,
                dpi_factor,
                render_state.system_style.as_deref().map(|s| &s.colors),
            )?;
        }
        DisplayListItem::ConicGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_conic_gradient(
                pixmap,
                &scroll_rect(bounds.inner()),
                gradient,
                border_radius,
                clip,
                dpi_factor,
                render_state.system_style.as_deref().map(|s| &s.colors),
            )?;
        }

        // BoxShadow
        DisplayListItem::BoxShadow {
            bounds,
            shadow,
            border_radius,
        } => {
            render_box_shadow(
                pixmap,
                &scroll_rect(bounds.inner()),
                shadow,
                border_radius,
                dpi_factor,
            )?;
        }

        // --- Opacity layers ---
        DisplayListItem::PushOpacity { bounds, opacity } => {
            let rect = logical_rect_to_az_rect(&scroll_rect(bounds.inner()), dpi_factor);
            if let Some(r) = rect {
                let snap = snapshot_region(
                    pixmap,
                    r.x as i32,
                    r.y as i32,
                    r.width as u32,
                    r.height as u32,
                );
                mask_stack.push(MaskEntry::Opacity {
                    snapshot: snap,
                    rect: r,
                    opacity: *opacity,
                });
            }
        }
        DisplayListItem::PopOpacity => {
            if let Some(MaskEntry::Opacity {
                snapshot,
                rect,
                opacity,
            }) = mask_stack.pop()
            {
                let x = rect.x as i32;
                let y = rect.y as i32;
                let w = rect.width as u32;
                let h = rect.height as u32;
                let pw = pixmap.width as i32;
                let ph = pixmap.height as i32;
                // Blend: result = snapshot + (current - snapshot) * opacity
                for py in 0..h as i32 {
                    let dy = y + py;
                    if dy < 0 || dy >= ph {
                        continue;
                    }
                    for px in 0..w as i32 {
                        let dx = x + px;
                        if dx < 0 || dx >= pw {
                            continue;
                        }
                        let pi = ((dy as u32 * pixmap.width + dx as u32) * 4) as usize;
                        let si = ((py as u32 * w + px as u32) * 4) as usize;
                        if pi + 3 >= pixmap.data.len() || si + 3 >= snapshot.len() {
                            continue;
                        }
                        let op = (opacity * 255.0).clamp(0.0, 255.0) as u32;
                        let inv_op = 255 - op;
                        for c in 0..4 {
                            let snap_c = snapshot[si + c] as u32;
                            let cur_c = pixmap.data[pi + c] as u32;
                            pixmap.data[pi + c] = ((cur_c * op + snap_c * inv_op) / 255) as u8;
                        }
                    }
                }
            }
        }

        // --- Reference frames (CSS transforms) ---
        DisplayListItem::PushReferenceFrame {
            transform_key,
            initial_transform,
            bounds,
        } => {
            // Look up the current GPU-cached transform value for this key.
            // For scrollbar thumbs, the GpuValueCache stores the up-to-date
            // thumb translation. For CSS transforms, it stores the computed
            // matrix. Falls back to the initial_transform baked in the DL.
            let live_transform = render_state.transforms.get(&transform_key.id);
            let m = match live_transform {
                Some(t) => &t.m,
                None => &initial_transform.m,
            };
            let tf = TransAffine::new_custom(
                m[0][0] as f64,
                m[0][1] as f64, // sx, shy
                m[1][0] as f64,
                m[1][1] as f64, // shx, sy
                m[3][0] as f64,
                m[3][1] as f64, // tx, ty
            );
            let current = transform_stack
                .last()
                .cloned()
                .unwrap_or_else(TransAffine::new);
            let mut composed = tf;
            composed.premultiply(&current);
            transform_stack.push(composed);
        }
        DisplayListItem::PopReferenceFrame => {
            if transform_stack.len() > 1 {
                transform_stack.pop();
            }
        }

        // --- Filter effects ---
        // TODO: proper compositing architecture with per-layer pixbufs
        DisplayListItem::PushFilter { .. } => {}
        DisplayListItem::PopFilter => {}
        DisplayListItem::PushBackdropFilter { .. } => {}
        DisplayListItem::PopBackdropFilter => {}
        DisplayListItem::PushTextShadow { .. } => {}
        DisplayListItem::PopTextShadow => {}

        DisplayListItem::PushImageMaskClip {
            bounds,
            mask_image,
            mask_rect,
        } => {
            let mr = &scroll_rect(mask_rect.inner());
            let px_x = (mr.origin.x * dpi_factor) as i32;
            let px_y = (mr.origin.y * dpi_factor) as i32;
            let px_w = (mr.size.width * dpi_factor).ceil() as u32;
            let px_h = (mr.size.height * dpi_factor).ceil() as u32;

            if px_w > 0 && px_h > 0 {
                let snapshot = snapshot_region(pixmap, px_x, px_y, px_w, px_h);
                let mask_data = extract_mask_data(mask_image, px_w, px_h)
                    .unwrap_or_else(|| vec![255u8; (px_w * px_h) as usize]);
                mask_stack.push(MaskEntry::ImageMask {
                    snapshot,
                    mask_data,
                    origin_x: px_x,
                    origin_y: px_y,
                    width: px_w,
                    height: px_h,
                });
            }
        }
        DisplayListItem::PopImageMaskClip => {
            if let Some(entry) = mask_stack.pop() {
                apply_mask(pixmap, &entry);
            }
        }
    }

    Ok(())
}

fn render_rect(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    color: ColorU,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    if color.a == 0 {
        return Ok(());
    }

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    // Early-out if fully outside clip
    if let Some(ref c) = clip {
        if rect.clip(c).is_none() {
            return Ok(());
        }
    }

    let agg_color = Rgba8::new(
        color.r as u32,
        color.g as u32,
        color.b as u32,
        color.a as u32,
    );

    if border_radius.is_zero() {
        // Fast path: axis-aligned rectangle — use direct RendererBase::blend_bar
        // instead of the full rasterizer pipeline. This avoids path construction,
        // cell generation, sorting, and scanline rendering for simple rectangles.
        let w = pixmap.width;
        let h = pixmap.height;
        let stride = (w * 4) as i32;
        let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride) };
        let mut pf = PixfmtRgba32::new(&mut ra);
        let mut rb = RendererBase::new(pf);
        if let Some(c) = clip {
            rb.clip_box_i(
                c.x as i32,
                c.y as i32,
                (c.x + c.width) as i32 - 1,
                (c.y + c.height) as i32 - 1,
            );
        }
        rb.blend_bar(
            rect.x as i32,
            rect.y as i32,
            (rect.x + rect.width) as i32 - 1,
            (rect.y + rect.height) as i32 - 1,
            &agg_color,
            255, // cover=255: alpha is already in the color
        );
    } else {
        // Rounded rect: needs the full rasterizer for curved corners
        let mut path = build_rounded_rect_path(&rect, border_radius, dpi_factor);
        agg_fill_path_clipped(pixmap, &mut path, &agg_color, FillingRule::NonZero, clip);
    }

    Ok(())
}

fn render_text(
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    pixmap: &mut AzulPixmap,
    clip_rect: &LogicalRect,
    clip: Option<AzRect>,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
) -> Result<(), String> {
    if color.a == 0 || glyphs.is_empty() {
        return Ok(());
    }

    // Skip text entirely if its clip_rect is outside the active clip region
    if let Some(ref c) = clip {
        let text_rect = match logical_rect_to_az_rect(clip_rect, dpi_factor) {
            Some(r) => r,
            None => return Ok(()),
        };
        if text_rect.clip(c).is_none() {
            return Ok(()); // fully clipped
        }
    }

    let agg_color = Rgba8::new(
        color.r as u32,
        color.g as u32,
        color.b as u32,
        color.a as u32,
    );

    // Try to get the parsed font
    let parsed_font: &ParsedFont = if let Some(fm) = font_manager {
        match fm.get_font_by_hash(font_hash.font_hash) {
            Some(font_ref) => unsafe { &*(font_ref.get_parsed() as *const ParsedFont) },
            None => {
                eprintln!(
                    "[cpurender] Font hash {} not found in FontManager",
                    font_hash.font_hash
                );
                return Ok(());
            }
        }
    } else {
        let font_key = match renderer_resources.font_hash_map.get(&font_hash.font_hash) {
            Some(k) => k,
            None => {
                eprintln!(
                    "[cpurender] Font hash {} not found in font_hash_map (available: {:?})",
                    font_hash.font_hash,
                    renderer_resources.font_hash_map.keys().collect::<Vec<_>>()
                );
                return Ok(());
            }
        };

        let font_ref = match renderer_resources.currently_registered_fonts.get(font_key) {
            Some((font_ref, _instances)) => font_ref,
            None => {
                eprintln!(
                    "[cpurender] FontKey {:?} not found in currently_registered_fonts",
                    font_key
                );
                return Ok(());
            }
        };

        unsafe { &*(font_ref.get_parsed() as *const ParsedFont) }
    };

    let units_per_em = parsed_font.font_metrics.units_per_em as f32;
    if units_per_em <= 0.0 {
        return Ok(());
    }

    let scale = (font_size_px * dpi_factor) / units_per_em;
    let ppem = (font_size_px * dpi_factor).round() as u16;

    // Set up the rasterizer pipeline once, reuse for all glyphs
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;

    // Create renderer infrastructure once, reuse for all glyphs in this text run.
    // Batches all glyph cells into a single rasterizer pass when possible.
    let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride) };
    let mut pf = PixfmtRgba32::new(&mut ra);
    let mut rb = RendererBase::new(pf);
    if let Some(c) = clip {
        rb.clip_box_i(
            c.x as i32,
            c.y as i32,
            (c.x + c.width) as i32 - 1,
            (c.y + c.height) as i32 - 1,
        );
    }
    let mut ras = RasterizerScanlineAa::new();
    ras.filling_rule(FillingRule::NonZero);

    // Accumulate all glyph cells into one rasterizer, then render once.
    // This amortizes sort_cells cost across all glyphs in the run.
    for glyph in glyphs {
        let glyph_index = glyph.index as u16;

        // Lazy decode: first access to a given gid for this face does
        // the allsorts glyf walk + OwnedGlyph conversion; subsequent
        // accesses are an Arc bump + BTreeMap lookup.
        let glyph_data = match parsed_font.get_or_decode_glyph(glyph_index) {
            Some(d) => d,
            None => continue,
        };

        let is_hinted = glyph_cache
            .get_or_build(
                font_hash.font_hash,
                glyph_index,
                &glyph_data,
                parsed_font,
                ppem,
            )
            .map(|c| c.is_hinted)
            .unwrap_or(false);

        let glyph_x = (glyph.point.x - scroll_offset.0) * dpi_factor;
        let glyph_baseline_y = (glyph.point.y - scroll_offset.1) * dpi_factor;

        let (cells, int_x, int_y) = match glyph_cache.get_or_build_cells(
            font_hash.font_hash,
            glyph_index,
            ppem,
            glyph_x,
            glyph_baseline_y,
            scale,
            is_hinted,
        ) {
            Some(c) => c,
            None => continue,
        };

        ras.add_cells_offset(cells, int_x, int_y);
    }

    // Single render pass for all glyphs in this text run
    let mut sl = ScanlineU8::new();
    render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &agg_color);

    Ok(())
}

fn render_border(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    color: ColorU,
    width: f32,
    border_style: azul_css::props::style::border::BorderStyle,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    use azul_css::props::style::border::BorderStyle;

    if color.a == 0 || width <= 0.0 {
        return Ok(());
    }

    match border_style {
        BorderStyle::None | BorderStyle::Hidden => return Ok(()),
        _ => {}
    }

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    // Skip if fully outside clip
    if let Some(ref c) = clip {
        if rect.clip(c).is_none() {
            return Ok(());
        }
    }

    let scaled_width = width * dpi_factor;
    let agg_color = Rgba8::new(
        color.r as u32,
        color.g as u32,
        color.b as u32,
        color.a as u32,
    );

    // 1. Build outer path (rounded rect at the nominal border radii)
    let mut path = build_rounded_rect_path(&rect, border_radius, dpi_factor);

    let x = rect.x as f64;
    let y = rect.y as f64;
    let w = rect.width as f64;
    let h = rect.height as f64;
    let sw = scaled_width as f64;

    // 2. Add inner path with shrunk radii so EvenOdd fill carves the stroke
    let ir = AzRect::from_xywh(
        rect.x + scaled_width,
        rect.y + scaled_width,
        rect.width - 2.0 * scaled_width,
        rect.height - 2.0 * scaled_width,
    );

    if let Some(ir) = ir {
        let inner_radius = BorderRadius {
            top_left: (border_radius.top_left - width).max(0.0),
            top_right: (border_radius.top_right - width).max(0.0),
            bottom_right: (border_radius.bottom_right - width).max(0.0),
            bottom_left: (border_radius.bottom_left - width).max(0.0),
        };
        let mut inner = build_rounded_rect_path(&ir, &inner_radius, dpi_factor);
        path.concat_path(&mut inner, 0);
    }

    // 3. Render based on border style
    match border_style {
        BorderStyle::Dashed | BorderStyle::Dotted => {
            // For dashed/dotted: stroke the border path with dash pattern
            use agg_rust::conv_dash::ConvDash;
            use agg_rust::conv_stroke::ConvStroke;

            let half = sw / 2.0;
            let mut stroke_path = PathStorage::new();
            let (cx, cy, cw, ch) = (x + half, y + half, w - sw, h - sw);
            stroke_path.move_to(cx, cy);
            stroke_path.line_to(cx + cw, cy);
            stroke_path.line_to(cx + cw, cy + ch);
            stroke_path.line_to(cx, cy + ch);
            stroke_path.close_polygon(PATH_FLAGS_NONE);

            let mut dashed = ConvDash::new(stroke_path);
            if border_style == BorderStyle::Dashed {
                dashed.add_dash(sw * 3.0, sw);
            } else {
                dashed.add_dash(sw, sw);
            }

            let mut stroked = ConvStroke::new(dashed);
            stroked.set_width(sw);

            agg_fill_path_clipped(pixmap, &mut stroked, &agg_color, FillingRule::NonZero, clip);
        }
        _ if border_radius.is_zero() => {
            // Fast path: solid border without rounding — use blend_bar strips
            let pw = pixmap.width;
            let ph = pixmap.height;
            let stride = (pw * 4) as i32;
            let mut ra =
                unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), pw, ph, stride) };
            let mut pf = PixfmtRgba32::new(&mut ra);
            let mut rb = RendererBase::new(pf);
            if let Some(c) = clip {
                rb.clip_box_i(
                    c.x as i32,
                    c.y as i32,
                    (c.x + c.width) as i32 - 1,
                    (c.y + c.height) as i32 - 1,
                );
            }
            let (xi, yi) = (x as i32, y as i32);
            let (x2i, y2i) = ((x + w) as i32 - 1, (y + h) as i32 - 1);
            let swi = sw as i32;
            // Top strip
            rb.blend_bar(xi, yi, x2i, yi + swi - 1, &agg_color, 255);
            // Bottom strip
            rb.blend_bar(xi, y2i - swi + 1, x2i, y2i, &agg_color, 255);
            // Left strip (between top and bottom)
            rb.blend_bar(xi, yi + swi, xi + swi - 1, y2i - swi, &agg_color, 255);
            // Right strip
            rb.blend_bar(x2i - swi + 1, yi + swi, x2i, y2i - swi, &agg_color, 255);
        }
        _ => {
            // Rounded solid border: fill double-path with EvenOdd
            agg_fill_path_clipped(pixmap, &mut path, &agg_color, FillingRule::EvenOdd, clip);
        }
    }

    Ok(())
}

/// Render border with per-side colors/widths/styles using CSS trapezoid model.
/// Each side is a trapezoid: outer edge → inner edge with 45° miters at corners.
fn render_border_sides(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    colors: [ColorU; 4], // top, right, bottom, left
    widths: [f32; 4],    // top, right, bottom, left
    _styles: [azul_css::props::style::border::BorderStyle; 4],
    _border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    // Outer corners
    let ox = rect.x as f64;
    let oy = rect.y as f64;
    let ow = rect.width as f64;
    let oh = rect.height as f64;

    // Inner corners (inset by per-side widths)
    let wt = (widths[0] * dpi_factor) as f64;
    let wr = (widths[1] * dpi_factor) as f64;
    let wb = (widths[2] * dpi_factor) as f64;
    let wl = (widths[3] * dpi_factor) as f64;

    let ix = ox + wl;
    let iy = oy + wt;
    let iw = ow - wl - wr;
    let ih = oh - wt - wb;

    // Each side is a trapezoid with 4 vertices:
    // Top:    (ox, oy) → (ox+ow, oy) → (ix+iw, iy) → (ix, iy)
    // Right:  (ox+ow, oy) → (ox+ow, oy+oh) → (ix+iw, iy+ih) → (ix+iw, iy)
    // Bottom: (ox+ow, oy+oh) → (ox, oy+oh) → (ix, iy+ih) → (ix+iw, iy+ih)
    // Left:   (ox, oy+oh) → (ox, oy) → (ix, iy) → (ix, iy+ih)

    let sides: [(f64, f64, f64, f64, f64, f64, f64, f64, ColorU, f32); 4] = [
        // Top trapezoid
        (
            ox,
            oy,
            ox + ow,
            oy,
            ix + iw,
            iy,
            ix,
            iy,
            colors[0],
            widths[0],
        ),
        // Right trapezoid
        (
            ox + ow,
            oy,
            ox + ow,
            oy + oh,
            ix + iw,
            iy + ih,
            ix + iw,
            iy,
            colors[1],
            widths[1],
        ),
        // Bottom trapezoid
        (
            ox + ow,
            oy + oh,
            ox,
            oy + oh,
            ix,
            iy + ih,
            ix + iw,
            iy + ih,
            colors[2],
            widths[2],
        ),
        // Left trapezoid
        (
            ox,
            oy + oh,
            ox,
            oy,
            ix,
            iy,
            ix,
            iy + ih,
            colors[3],
            widths[3],
        ),
    ];

    if _border_radius.is_zero() {
        // Fast path: axis-aligned border strips — no rasterizer needed
        let pw = pixmap.width;
        let ph = pixmap.height;
        let stride = (pw * 4) as i32;
        let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), pw, ph, stride) };
        let mut pf = PixfmtRgba32::new(&mut ra);
        let mut rb = RendererBase::new(pf);
        if let Some(c) = clip {
            rb.clip_box_i(
                c.x as i32,
                c.y as i32,
                (c.x + c.width) as i32 - 1,
                (c.y + c.height) as i32 - 1,
            );
        }
        // Top: full width, height = wt
        if widths[0] > 0.0 && colors[0].a > 0 {
            let c = colors[0];
            let ac = Rgba8::new(c.r as u32, c.g as u32, c.b as u32, c.a as u32);
            rb.blend_bar(
                ox as i32,
                oy as i32,
                (ox + ow) as i32 - 1,
                iy as i32 - 1,
                &ac,
                255,
            );
        }
        // Bottom
        if widths[2] > 0.0 && colors[2].a > 0 {
            let c = colors[2];
            let ac = Rgba8::new(c.r as u32, c.g as u32, c.b as u32, c.a as u32);
            rb.blend_bar(
                ox as i32,
                (iy + ih) as i32,
                (ox + ow) as i32 - 1,
                (oy + oh) as i32 - 1,
                &ac,
                255,
            );
        }
        // Left: between top and bottom
        if widths[3] > 0.0 && colors[3].a > 0 {
            let c = colors[3];
            let ac = Rgba8::new(c.r as u32, c.g as u32, c.b as u32, c.a as u32);
            rb.blend_bar(
                ox as i32,
                iy as i32,
                ix as i32 - 1,
                (iy + ih) as i32 - 1,
                &ac,
                255,
            );
        }
        // Right
        if widths[1] > 0.0 && colors[1].a > 0 {
            let c = colors[1];
            let ac = Rgba8::new(c.r as u32, c.g as u32, c.b as u32, c.a as u32);
            rb.blend_bar(
                (ix + iw) as i32,
                iy as i32,
                (ox + ow) as i32 - 1,
                (iy + ih) as i32 - 1,
                &ac,
                255,
            );
        }
    } else {
        // Rounded borders: use trapezoid rasterizer
        for &(x0, y0, x1, y1, x2, y2, x3, y3, color, width) in &sides {
            if width <= 0.0 || color.a == 0 {
                continue;
            }

            let mut path = PathStorage::new();
            path.move_to(x0, y0);
            path.line_to(x1, y1);
            path.line_to(x2, y2);
            path.line_to(x3, y3);
            path.close_polygon(PATH_FLAGS_NONE);

            let agg_color = Rgba8::new(
                color.r as u32,
                color.g as u32,
                color.b as u32,
                color.a as u32,
            );
            agg_fill_path_clipped(pixmap, &mut path, &agg_color, FillingRule::NonZero, clip);
        }
    }

    Ok(())
}

fn logical_rect_to_az_rect(bounds: &LogicalRect, dpi_factor: f32) -> Option<AzRect> {
    let x = bounds.origin.x * dpi_factor;
    let y = bounds.origin.y * dpi_factor;
    let width = bounds.size.width * dpi_factor;
    let height = bounds.size.height * dpi_factor;

    AzRect::from_xywh(x, y, width, height)
}

fn render_image(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    image: &ImageRef,
    clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    // Skip if fully outside clip
    if let Some(ref c) = clip {
        if rect.clip(c).is_none() {
            return Ok(());
        }
    }

    let image_data = image.get_data();
    let (src_rgba, src_w, src_h) = match &*image_data {
        DecodedImage::Raw((descriptor, data)) => {
            let w = descriptor.width as u32;
            let h = descriptor.height as u32;
            if w == 0 || h == 0 {
                return Ok(());
            }
            let bytes = match data {
                azul_core::resources::ImageData::Raw(shared) => shared.as_ref(),
                _ => return Ok(()),
            };

            let rgba = match descriptor.format {
                azul_core::resources::RawImageFormat::BGRA8 => {
                    let mut out = Vec::with_capacity(bytes.len());
                    for chunk in bytes.chunks_exact(4) {
                        let b = chunk[0];
                        let g = chunk[1];
                        let r = chunk[2];
                        let a = chunk[3];
                        out.push(r);
                        out.push(g);
                        out.push(b);
                        out.push(a);
                    }
                    out
                }
                azul_core::resources::RawImageFormat::R8 => {
                    let mut out = Vec::with_capacity(bytes.len() * 4);
                    for &v in bytes {
                        out.push(v);
                        out.push(v);
                        out.push(v);
                        out.push(v);
                    }
                    out
                }
                _ => {
                    // Unsupported format — render gray placeholder
                    let gray = Rgba8::new(200, 200, 200, 255);
                    let mut path = build_rect_path(&rect);
                    agg_fill_path(pixmap, &mut path, &gray, FillingRule::NonZero);
                    return Ok(());
                }
            };

            (rgba, w, h)
        }
        DecodedImage::NullImage { .. } | DecodedImage::Callback(_) => {
            let gray = Rgba8::new(200, 200, 200, 255);
            let mut path = build_rect_path(&rect);
            agg_fill_path(pixmap, &mut path, &gray, FillingRule::NonZero);
            return Ok(());
        }
        _ => return Ok(()),
    };

    // Simple nearest-neighbor blit with scaling
    let dst_x = rect.x as i32;
    let dst_y = rect.y as i32;
    let dst_w = rect.width as u32;
    let dst_h = rect.height as u32;
    let pw = pixmap.width;
    let ph = pixmap.height;

    let sx = src_w as f32 / dst_w.max(1) as f32;
    let sy = src_h as f32 / dst_h.max(1) as f32;

    // Compute pixel-level clip bounds for the blit loop
    let (clip_x1, clip_y1, clip_x2, clip_y2) = if let Some(ref c) = clip {
        (
            c.x as i32,
            c.y as i32,
            (c.x + c.width) as i32,
            (c.y + c.height) as i32,
        )
    } else {
        (0, 0, pw as i32, ph as i32)
    };

    for py in 0..dst_h {
        for px in 0..dst_w {
            let tx = dst_x + px as i32;
            let ty = dst_y + py as i32;
            if tx < 0 || ty < 0 || tx >= pw as i32 || ty >= ph as i32 {
                continue;
            }
            // Clip check
            if tx < clip_x1 || ty < clip_y1 || tx >= clip_x2 || ty >= clip_y2 {
                continue;
            }

            let src_x = ((px as f32 * sx) as u32).min(src_w - 1);
            let src_y = ((py as f32 * sy) as u32).min(src_h - 1);
            let si = ((src_y * src_w + src_x) * 4) as usize;
            let di = ((ty as u32 * pw + tx as u32) * 4) as usize;

            if si + 3 < src_rgba.len() && di + 3 < pixmap.data.len() {
                let sa = src_rgba[si + 3] as u32;
                if sa == 255 {
                    pixmap.data[di] = src_rgba[si];
                    pixmap.data[di + 1] = src_rgba[si + 1];
                    pixmap.data[di + 2] = src_rgba[si + 2];
                    pixmap.data[di + 3] = 255;
                } else if sa > 0 {
                    // Alpha blend: dst = src * sa + dst * (255 - sa)
                    let da = 255 - sa;
                    pixmap.data[di] =
                        ((src_rgba[si] as u32 * sa + pixmap.data[di] as u32 * da) / 255) as u8;
                    pixmap.data[di + 1] = ((src_rgba[si + 1] as u32 * sa
                        + pixmap.data[di + 1] as u32 * da)
                        / 255) as u8;
                    pixmap.data[di + 2] = ((src_rgba[si + 2] as u32 * sa
                        + pixmap.data[di + 2] as u32 * da)
                        / 255) as u8;
                    pixmap.data[di + 3] =
                        ((sa + pixmap.data[di + 3] as u32 * da / 255).min(255)) as u8;
                }
            }
        }
    }

    Ok(())
}

fn build_rect_path(rect: &AzRect) -> PathStorage {
    let mut path = PathStorage::new();
    let x = rect.x as f64;
    let y = rect.y as f64;
    let w = rect.width as f64;
    let h = rect.height as f64;
    path.move_to(x, y);
    path.line_to(x + w, y);
    path.line_to(x + w, y + h);
    path.line_to(x, y + h);
    path.close_polygon(PATH_FLAGS_NONE);
    path
}

fn build_rounded_rect_path(
    rect: &AzRect,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> PathStorage {
    let mut path = PathStorage::new();

    let x = rect.x as f64;
    let y = rect.y as f64;
    let w = rect.width as f64;
    let h = rect.height as f64;

    let tl = (border_radius.top_left * dpi_factor) as f64;
    let tr = (border_radius.top_right * dpi_factor) as f64;
    let br = (border_radius.bottom_right * dpi_factor) as f64;
    let bl = (border_radius.bottom_left * dpi_factor) as f64;

    if tl <= 0.0 && tr <= 0.0 && br <= 0.0 && bl <= 0.0 {
        path.move_to(x, y);
        path.line_to(x + w, y);
        path.line_to(x + w, y + h);
        path.line_to(x, y + h);
        path.close_polygon(PATH_FLAGS_NONE);
        return path;
    }

    // agg::RoundedRect emits real arc vertices (MOVE_TO + LINE_TO segments)
    // via its embedded Arc generator, which the scanline rasterizer consumes
    // directly. curve3() control points are silently flattened to straight
    // lines by the rasterizer, which is why the hand-rolled path produced
    // square corners — Arc-based flattening produces smooth corners.
    //
    // agg's corner slots (rx1/ry1 .. rx4/ry4) map to screen corners as:
    //   slot 1 → top-left    (center at x1+rx1, y1+ry1)
    //   slot 2 → top-right   (center at x2-rx2, y1+ry2)
    //   slot 3 → bottom-right (center at x2-rx3, y2-ry3)
    //   slot 4 → bottom-left (center at x1+rx4, y2-ry4)
    let mut rr = RoundedRect::default_new();
    rr.rect(x, y, x + w, y + h);
    rr.radius_all(tl, tl, tr, tr, br, br, bl, bl);
    rr.normalize_radius();
    rr.set_approximation_scale(dpi_factor.max(1.0) as f64);

    path.concat_path(&mut rr, 0);
    path
}

// ============================================================================
// Component Preview Rendering
// ============================================================================

/// Options for rendering a component preview.
pub struct ComponentPreviewOptions {
    /// Optional width constraint. If None, size to content (uses 4096px max).
    pub width: Option<f32>,
    /// Optional height constraint. If None, size to content (uses 4096px max).
    pub height: Option<f32>,
    /// DPI scale factor. Default 1.0.
    pub dpi_factor: f32,
    /// Background color. Default white.
    pub background_color: ColorU,
}

impl Default for ComponentPreviewOptions {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            dpi_factor: 1.0,
            background_color: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        }
    }
}

/// Result of a component preview render.
pub struct ComponentPreviewResult {
    /// PNG-encoded image data.
    pub png_data: Vec<u8>,
    /// Actual content width (logical pixels).
    pub content_width: f32,
    /// Actual content height (logical pixels).
    pub content_height: f32,
}

/// Compute the tight bounding box of all display list items.
fn compute_content_bounds(dl: &DisplayList) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut has_items = false;

    for item in &dl.items {
        let bounds = match item {
            DisplayListItem::Rect { bounds, .. } => Some(*bounds),
            DisplayListItem::SelectionRect { bounds, .. } => Some(*bounds),
            DisplayListItem::Border { bounds, .. } => Some(*bounds),
            DisplayListItem::Text { clip_rect, .. } => Some(*clip_rect),
            DisplayListItem::Image { bounds, .. } => Some(*bounds),
            DisplayListItem::BoxShadow { bounds, .. } => Some(*bounds),
            DisplayListItem::PushClip { bounds, .. } => Some(*bounds),
            DisplayListItem::LinearGradient { bounds, .. } => Some(*bounds),
            DisplayListItem::RadialGradient { bounds, .. } => Some(*bounds),
            DisplayListItem::ConicGradient { bounds, .. } => Some(*bounds),
            DisplayListItem::VirtualView { bounds, .. } => Some(*bounds),
            DisplayListItem::ScrollBar { bounds, .. } => Some(*bounds),
            _ => None,
        };
        if let Some(b) = bounds {
            has_items = true;
            min_x = min_x.min(b.0.origin.x);
            min_y = min_y.min(b.0.origin.y);
            max_x = max_x.max(b.0.origin.x + b.0.size.width);
            max_y = max_y.max(b.0.origin.y + b.0.size.height);
        }
    }

    if has_items {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

/// Render a `StyledDom` to a PNG image for component preview.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
pub fn render_component_preview(
    styled_dom: azul_core::styled_dom::StyledDom,
    font_manager: &FontManager<azul_css::props::basic::FontRef>,
    opts: ComponentPreviewOptions,
    system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
) -> Result<ComponentPreviewResult, String> {
    use crate::{
        font_traits::TextLayoutCache,
        solver3::{self, cache::LayoutCache, display_list::DisplayList},
    };
    use azul_core::{
        dom::DomId,
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::{IdNamespace, RendererResources},
        selection::{SelectionState, TextSelection},
    };
    use std::collections::{BTreeMap, HashMap};

    const MAX_SIZE: f32 = 4096.0;

    let layout_width = opts.width.unwrap_or(MAX_SIZE);
    let layout_height = opts.height.unwrap_or(MAX_SIZE);

    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize {
            width: layout_width,
            height: layout_height,
        },
    };

    let mut preview_font_manager = FontManager::from_arc_shared(
        font_manager.fc_cache.clone(),
        font_manager.parsed_fonts.clone(),
    )
    .map_err(|e| format!("Failed to create preview font manager: {:?}", e))?;

    // --- Font resolution ---
    {
        use crate::solver3::getters::collect_and_resolve_font_chains_with_registration;
        use crate::text3::default::PathLoader;

        let platform = azul_css::system::Platform::current();

        let chains = collect_and_resolve_font_chains_with_registration(
            &styled_dom,
            &preview_font_manager.fc_cache,
            &preview_font_manager,
            &platform,
        );
        let loader = PathLoader::new();
        let _failed = preview_font_manager.load_missing_for_chains(&chains, |bytes, index| {
            loader.load_font_shared(bytes, index)
        });
        preview_font_manager.set_font_chain_cache(chains.into_fontconfig_chains());
    }

    // --- Layout ---
    let mut layout_cache = LayoutCache {
        tree: None,
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: HashMap::new(),
        scroll_id_to_node_id: HashMap::new(),
        counters: HashMap::new(),
        float_cache: HashMap::new(),
        cache_map: Default::default(),
        previous_positions: Vec::new(),
        cached_display_list: None,
        prev_dom_ptr: 0,
        prev_viewport: LogicalRect::zero(),
    };
    let mut text_cache = TextLayoutCache::new();
    let empty_scroll_offsets = BTreeMap::new();
    let empty_text_selections = BTreeMap::new();
    let renderer_resources = RendererResources::default();
    let id_namespace = IdNamespace(0xFFFF);
    let dom_id = DomId::ROOT_ID;
    let mut debug_messages = None;
    let get_system_time_fn = azul_core::task::GetSystemTimeCallback {
        cb: azul_core::task::get_system_time_libstd,
    };

    let display_list = solver3::layout_document(
        &mut layout_cache,
        &mut text_cache,
        &styled_dom,
        viewport,
        &preview_font_manager,
        &empty_scroll_offsets,
        &empty_text_selections,
        &mut debug_messages,
        None,
        &renderer_resources,
        id_namespace,
        dom_id,
        false,
        Vec::new(),
        None, // preedit_text: not needed for headless preview rendering
        &azul_core::resources::ImageCache::default(),
        system_style.clone(),
        get_system_time_fn,
    )
    .map_err(|e| format!("Layout failed: {:?}", e))?;

    // --- Determine actual render size ---
    let (render_width, render_height) = if opts.width.is_some() && opts.height.is_some() {
        (opts.width.unwrap(), opts.height.unwrap())
    } else {
        match compute_content_bounds(&display_list) {
            Some((_min_x, _min_y, max_x, max_y)) => {
                let w = if opts.width.is_some() {
                    opts.width.unwrap()
                } else {
                    max_x.max(1.0).ceil()
                };
                let h = if opts.height.is_some() {
                    opts.height.unwrap()
                } else {
                    max_y.max(1.0).ceil()
                };
                (w, h)
            }
            None => {
                return Ok(ComponentPreviewResult {
                    png_data: Vec::new(),
                    content_width: 0.0,
                    content_height: 0.0,
                });
            }
        }
    };

    let render_width = render_width.min(MAX_SIZE);
    let render_height = render_height.min(MAX_SIZE);

    // --- Render ---
    let dpi = opts.dpi_factor;
    let pixel_w = ((render_width * dpi) as u32).max(1);
    let pixel_h = ((render_height * dpi) as u32).max(1);

    let mut pixmap = AzulPixmap::new(pixel_w, pixel_h)
        .ok_or_else(|| format!("Cannot create pixmap {}x{}", pixel_w, pixel_h))?;

    let bg = opts.background_color;
    pixmap.fill(bg.r, bg.g, bg.b, bg.a);

    let mut preview_glyph_cache = GlyphCache::new();
    let preview_render_state =
        CpuRenderState::new(ScrollOffsetMap::new()).with_system_style(system_style);
    render_display_list_with_state(
        &display_list,
        &mut pixmap,
        dpi,
        &renderer_resources,
        Some(&preview_font_manager),
        &mut preview_glyph_cache,
        &preview_render_state,
    )?;

    let png_data = pixmap
        .encode_png()
        .map_err(|e| format!("PNG encoding failed: {}", e))?;

    Ok(ComponentPreviewResult {
        png_data,
        content_width: render_width,
        content_height: render_height,
    })
}

/// Render a `Dom` + `Css` to a PNG image at the given dimensions.
///
/// This is a convenience API that creates a `StyledDom`, lays it out,
/// and rasterizes via the CPU renderer.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
pub fn render_dom_to_image(
    mut dom: azul_core::dom::Dom,
    css: azul_css::css::Css,
    width: f32,
    height: f32,
    dpi: f32,
) -> Result<Vec<u8>, String> {
    use crate::font_traits::FontManager;
    use azul_core::styled_dom::StyledDom;

    let styled_dom = StyledDom::create(&mut dom, css);

    let fc_cache = crate::font::loading::build_font_cache();
    let font_manager = FontManager::new(fc_cache)
        .map_err(|e| format!("Failed to create font manager: {:?}", e))?;

    let opts = ComponentPreviewOptions {
        width: Some(width),
        height: Some(height),
        dpi_factor: dpi,
        background_color: azul_css::props::basic::ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    };

    let result = render_component_preview(styled_dom, &font_manager, opts, None)?;
    Ok(result.png_data)
}

// ============================================================================
// Direct SVG-to-image renderer (bypasses CSS layout)
// ============================================================================

/// Render raw SVG bytes to a PNG image.
///
/// Parses the SVG XML, walks the element tree, extracts path geometry +
/// fill/stroke attributes, and rasterizes via agg-rust directly (no CSS
/// layout involved).
#[cfg(all(feature = "std", feature = "xml"))]
pub fn render_svg_to_png(
    svg_data: &[u8],
    target_width: u32,
    target_height: u32,
) -> Result<Vec<u8>, String> {
    let svg_str =
        core::str::from_utf8(svg_data).map_err(|e| format!("SVG is not valid UTF-8: {e}"))?;

    let nodes =
        crate::xml::parse_xml_string(svg_str).map_err(|e| format!("XML parse error: {e}"))?;

    // Find the <svg> root
    let node_slice: &[azul_core::xml::XmlNodeChild] = nodes.as_ref();
    let svg_node = node_slice
        .iter()
        .find_map(|n| {
            if let azul_core::xml::XmlNodeChild::Element(e) = n {
                let tag = e.node_type.as_str().to_lowercase();
                if tag == "svg" {
                    Some(e)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| "No <svg> root element found".to_string())?;

    // Parse viewBox for coordinate mapping
    let vb = parse_viewbox(svg_node);
    let (vb_x, vb_y, vb_w, vb_h) =
        vb.unwrap_or((0.0, 0.0, target_width as f64, target_height as f64));

    let sx = target_width as f64 / vb_w;
    let sy = target_height as f64 / vb_h;
    let scale = sx.min(sy);

    let root_transform =
        TransAffine::new_custom(scale, 0.0, 0.0, scale, -vb_x * scale, -vb_y * scale);

    let mut pixmap = AzulPixmap::new(target_width, target_height)
        .ok_or_else(|| "Failed to create pixmap".to_string())?;
    pixmap.fill(255, 255, 255, 255);

    render_svg_group(svg_node, &mut pixmap, &root_transform);

    pixmap
        .encode_png()
        .map_err(|e| format!("PNG encode error: {e}"))
}

/// Like [`render_svg_to_png`] but returns the rendered pixmap as an [`ImageRef`]
/// (RGBA8) directly — no PNG round-trip. The MapWidget uses this to render each
/// decoded tile SVG to a colour image node: `SvgNodeData::Path` in the DOM only
/// produces a clip mask (not a filled shape), so reuse the same `render_svg_group`
/// rasteriser the tiger uses (which reads SVG fill/stroke attrs) and embed the
/// result as an image.
pub fn render_svg_to_imageref(
    svg_data: &[u8],
    target_width: u32,
    target_height: u32,
) -> Result<ImageRef, String> {
    let svg_str =
        core::str::from_utf8(svg_data).map_err(|e| format!("SVG is not valid UTF-8: {e}"))?;
    let nodes =
        crate::xml::parse_xml_string(svg_str).map_err(|e| format!("XML parse error: {e}"))?;
    let node_slice: &[azul_core::xml::XmlNodeChild] = nodes.as_ref();
    let svg_node = node_slice
        .iter()
        .find_map(|n| {
            if let azul_core::xml::XmlNodeChild::Element(e) = n {
                if e.node_type.as_str().to_lowercase() == "svg" {
                    Some(e)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| "No <svg> root element found".to_string())?;

    let vb = parse_viewbox(svg_node);
    let (vb_x, vb_y, vb_w, vb_h) =
        vb.unwrap_or((0.0, 0.0, target_width as f64, target_height as f64));
    let scale = (target_width as f64 / vb_w).min(target_height as f64 / vb_h);
    let root_transform =
        TransAffine::new_custom(scale, 0.0, 0.0, scale, -vb_x * scale, -vb_y * scale);

    let mut pixmap = AzulPixmap::new(target_width, target_height)
        .ok_or_else(|| "Failed to create pixmap".to_string())?;
    // Transparent background so the tile container shows through any gaps.
    pixmap.fill(0, 0, 0, 0);
    render_svg_group(svg_node, &mut pixmap, &root_transform);

    let rgba = pixmap.data().to_vec();
    let raw = azul_core::resources::RawImage {
        pixels: azul_core::resources::RawImageData::U8(rgba.into()),
        width: target_width as usize,
        height: target_height as usize,
        premultiplied_alpha: false,
        data_format: azul_core::resources::RawImageFormat::RGBA8,
        tag: alloc::vec::Vec::new().into(),
    };
    ImageRef::new_rawimage(raw).ok_or_else(|| "Failed to build ImageRef from pixmap".to_string())
}

#[cfg(all(feature = "std", feature = "xml"))]
fn parse_viewbox(node: &azul_core::xml::XmlNode) -> Option<(f64, f64, f64, f64)> {
    let vb = node
        .attributes
        .get_key("viewbox")
        .or_else(|| node.attributes.get_key("viewBox"))?;
    let nums: Vec<f64> = vb
        .as_str()
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() == 4 {
        Some((nums[0], nums[1], nums[2], nums[3]))
    } else {
        None
    }
}

/// Inherited SVG style (fill, stroke, stroke-width) that cascades from parent groups.
#[cfg(all(feature = "std", feature = "xml"))]
#[derive(Clone)]
struct SvgInheritedStyle {
    fill: Option<String>,   // None = not set (inherit default black)
    stroke: Option<String>, // None = not set (inherit default none)
    stroke_width: Option<f64>,
}

#[cfg(all(feature = "std", feature = "xml"))]
impl Default for SvgInheritedStyle {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            stroke_width: None,
        }
    }
}

#[cfg(all(feature = "std", feature = "xml"))]
fn render_svg_group(
    node: &azul_core::xml::XmlNode,
    pixmap: &mut AzulPixmap,
    parent_transform: &TransAffine,
) {
    render_svg_group_with_style(
        node,
        pixmap,
        parent_transform,
        &SvgInheritedStyle::default(),
    );
}

#[cfg(all(feature = "std", feature = "xml"))]
fn render_svg_group_with_style(
    node: &azul_core::xml::XmlNode,
    pixmap: &mut AzulPixmap,
    parent_transform: &TransAffine,
    parent_style: &SvgInheritedStyle,
) {
    use agg_rust::math_stroke::{LineCap, LineJoin};
    use azul_core::xml::{XmlNode, XmlNodeChild};

    let group_transform = if let Some(t) = node.attributes.get_key("transform") {
        let mut tf = parse_svg_transform(t.as_str());
        tf.premultiply(parent_transform);
        tf
    } else {
        parent_transform.clone()
    };

    // Inherit style from this group's attributes
    let group_style = SvgInheritedStyle {
        fill: node
            .attributes
            .get_key("fill")
            .map(|s| s.as_str().to_string())
            .or_else(|| parent_style.fill.clone()),
        stroke: node
            .attributes
            .get_key("stroke")
            .map(|s| s.as_str().to_string())
            .or_else(|| parent_style.stroke.clone()),
        stroke_width: node
            .attributes
            .get_key("stroke-width")
            .and_then(|s| s.as_str().parse().ok())
            .or(parent_style.stroke_width),
    };

    for child in node.children.as_ref().iter() {
        let child_node = match child {
            XmlNodeChild::Element(e) => e,
            _ => continue,
        };

        let tag = child_node.node_type.as_str().to_lowercase();

        match tag.as_str() {
            "g" | "svg" => {
                render_svg_group_with_style(child_node, pixmap, &group_transform, &group_style);
            }
            "path" | "circle" | "rect" | "ellipse" | "line" | "polygon" | "polyline" => {
                let path_storage = match build_agg_path(child_node) {
                    Some(p) => p,
                    None => continue,
                };

                // Flatten bezier curves into line segments for the rasterizer
                let mut curved = agg_rust::conv_curve::ConvCurve::new(path_storage);

                // Per-element transform
                let elem_transform = if let Some(t) = child_node.attributes.get_key("transform") {
                    let mut tf = parse_svg_transform(t.as_str());
                    tf.premultiply(&group_transform);
                    tf
                } else {
                    group_transform.clone()
                };

                // Fill: element overrides group
                let fill_attr = child_node
                    .attributes
                    .get_key("fill")
                    .map(|s| s.as_str().to_string())
                    .or_else(|| group_style.fill.clone());
                let fill_color = match fill_attr.as_deref() {
                    Some("none") => None,
                    Some(c) => parse_svg_color(c),
                    None => Some(Rgba8 {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    }), // SVG default
                };

                let fill_opacity = child_node
                    .attributes
                    .get_key("fill-opacity")
                    .and_then(|s| s.as_str().parse::<f64>().ok())
                    .unwrap_or(1.0);

                let opacity = child_node
                    .attributes
                    .get_key("opacity")
                    .and_then(|s| s.as_str().parse::<f64>().ok())
                    .unwrap_or(1.0);

                if let Some(mut color) = fill_color {
                    color.a = ((color.a as f64) * fill_opacity * opacity).min(255.0) as u8;

                    let fill_rule_str = child_node
                        .attributes
                        .get_key("fill-rule")
                        .map(|s| s.as_str().to_string());
                    let rule = match fill_rule_str.as_deref() {
                        Some("evenodd") => FillingRule::EvenOdd,
                        _ => FillingRule::NonZero,
                    };

                    let mut transformed = ConvTransform::new(&mut curved, elem_transform.clone());
                    agg_fill_path(pixmap, &mut transformed, &color, rule);
                }

                // Stroke: element overrides group
                let stroke_attr = child_node
                    .attributes
                    .get_key("stroke")
                    .map(|s| s.as_str().to_string())
                    .or_else(|| group_style.stroke.clone());
                let stroke_color = match stroke_attr.as_deref() {
                    Some("none") | None => None,
                    Some(c) => parse_svg_color(c),
                };

                if let Some(mut color) = stroke_color {
                    let stroke_opacity = child_node
                        .attributes
                        .get_key("stroke-opacity")
                        .and_then(|s| s.as_str().parse::<f64>().ok())
                        .unwrap_or(1.0);
                    color.a = ((color.a as f64) * stroke_opacity * opacity).min(255.0) as u8;

                    let stroke_width = child_node
                        .attributes
                        .get_key("stroke-width")
                        .and_then(|s| s.as_str().parse::<f64>().ok())
                        .or(group_style.stroke_width)
                        .unwrap_or(1.0);

                    let mut conv_stroke = ConvStroke::new(&mut curved);
                    conv_stroke.set_width(stroke_width);
                    conv_stroke.set_line_cap(LineCap::Round);
                    conv_stroke.set_line_join(LineJoin::Round);

                    let mut transformed =
                        ConvTransform::new(&mut conv_stroke, elem_transform.clone());
                    agg_fill_path(pixmap, &mut transformed, &color, FillingRule::NonZero);
                }
            }
            _ => {
                // Recurse into unknown containers (defs, symbol, etc.)
                render_svg_group_with_style(child_node, pixmap, &group_transform, &group_style);
            }
        }
    }
}

/// Build an agg PathStorage from an SVG shape element's attributes.
#[cfg(all(feature = "std", feature = "xml"))]
fn build_agg_path(node: &azul_core::xml::XmlNode) -> Option<PathStorage> {
    let tag = node.node_type.as_str().to_lowercase();
    match tag.as_str() {
        "path" => {
            let d = node.attributes.get_key("d")?;
            let mp = azul_core::path_parser::parse_svg_path_d(d.as_str()).ok()?;
            Some(svg_multi_polygon_to_path_storage(&mp))
        }
        "circle" => {
            let cx = attr_f64(node, "cx");
            let cy = attr_f64(node, "cy");
            let r = attr_f64(node, "r");
            if r <= 0.0 {
                return None;
            }
            let mp = azul_core::path_parser::svg_circle_to_paths(cx as f32, cy as f32, r as f32);
            let multi = azul_core::svg::SvgMultiPolygon {
                rings: azul_core::svg::SvgPathVec::from_vec(vec![mp]),
            };
            Some(svg_multi_polygon_to_path_storage(&multi))
        }
        "rect" => {
            let x = attr_f64(node, "x");
            let y = attr_f64(node, "y");
            let w = attr_f64(node, "width");
            let h = attr_f64(node, "height");
            let rx = attr_f64(node, "rx");
            let ry = if let Some(v) = node.attributes.get_key("ry") {
                v.as_str().parse().unwrap_or(rx)
            } else {
                rx
            };
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            let mp = azul_core::path_parser::svg_rect_to_path(
                x as f32, y as f32, w as f32, h as f32, rx as f32, ry as f32,
            );
            let multi = azul_core::svg::SvgMultiPolygon {
                rings: azul_core::svg::SvgPathVec::from_vec(vec![mp]),
            };
            Some(svg_multi_polygon_to_path_storage(&multi))
        }
        "ellipse" => {
            let cx = attr_f64(node, "cx");
            let cy = attr_f64(node, "cy");
            let rx = attr_f64(node, "rx");
            let ry = attr_f64(node, "ry");
            if rx <= 0.0 || ry <= 0.0 {
                return None;
            }
            // Use circle path with scaling
            let mp = azul_core::path_parser::svg_circle_to_paths(cx as f32, cy as f32, 1.0);
            let multi = azul_core::svg::SvgMultiPolygon {
                rings: azul_core::svg::SvgPathVec::from_vec(vec![mp]),
            };
            let mut ps = svg_multi_polygon_to_path_storage(&multi);
            // Scale ellipse: we'll just build it directly instead
            let mut path = PathStorage::new();
            const KAPPA: f64 = 0.5522847498;
            let kx = rx * KAPPA;
            let ky = ry * KAPPA;
            path.move_to(cx, cy - ry);
            path.curve4(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
            path.curve4(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
            path.curve4(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
            path.curve4(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
            path.close_polygon(PATH_FLAGS_NONE);
            Some(path)
        }
        "line" => {
            let x1 = attr_f64(node, "x1");
            let y1 = attr_f64(node, "y1");
            let x2 = attr_f64(node, "x2");
            let y2 = attr_f64(node, "y2");
            let mut path = PathStorage::new();
            path.move_to(x1, y1);
            path.line_to(x2, y2);
            Some(path)
        }
        "polygon" | "polyline" => {
            let pts_str = node.attributes.get_key("points")?;
            let nums: Vec<f64> = pts_str
                .as_str()
                .split(|c: char| c == ',' || c.is_ascii_whitespace())
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect();
            if nums.len() < 4 {
                return None;
            }
            let mut path = PathStorage::new();
            path.move_to(nums[0], nums[1]);
            for chunk in nums[2..].chunks_exact(2) {
                path.line_to(chunk[0], chunk[1]);
            }
            if tag == "polygon" {
                path.close_polygon(PATH_FLAGS_NONE);
            }
            Some(path)
        }
        _ => None,
    }
}

#[cfg(all(feature = "std", feature = "xml"))]
fn attr_f64(node: &azul_core::xml::XmlNode, key: &str) -> f64 {
    node.attributes
        .get_key(key)
        .and_then(|s| s.as_str().parse().ok())
        .unwrap_or(0.0)
}

/// Convert SvgMultiPolygon to agg PathStorage.
#[cfg(all(feature = "std", feature = "xml"))]
fn svg_multi_polygon_to_path_storage(mp: &azul_core::svg::SvgMultiPolygon) -> PathStorage {
    let mut path = PathStorage::new();
    for ring in mp.rings.as_ref().iter() {
        let mut first = true;
        for item in ring.items.as_ref().iter() {
            match item {
                azul_core::svg::SvgPathElement::Line(l) => {
                    if first {
                        path.move_to(l.start.x as f64, l.start.y as f64);
                        first = false;
                    }
                    path.line_to(l.end.x as f64, l.end.y as f64);
                }
                azul_core::svg::SvgPathElement::QuadraticCurve(q) => {
                    if first {
                        path.move_to(q.start.x as f64, q.start.y as f64);
                        first = false;
                    }
                    path.curve3(
                        q.ctrl.x as f64,
                        q.ctrl.y as f64,
                        q.end.x as f64,
                        q.end.y as f64,
                    );
                }
                azul_core::svg::SvgPathElement::CubicCurve(c) => {
                    if first {
                        path.move_to(c.start.x as f64, c.start.y as f64);
                        first = false;
                    }
                    path.curve4(
                        c.ctrl_1.x as f64,
                        c.ctrl_1.y as f64,
                        c.ctrl_2.x as f64,
                        c.ctrl_2.y as f64,
                        c.end.x as f64,
                        c.end.y as f64,
                    );
                }
            }
        }
        path.close_polygon(PATH_FLAGS_NONE);
    }
    path
}

/// Parse SVG transform attribute (supports matrix, translate, scale, rotate).
#[cfg(all(feature = "std", feature = "xml"))]
fn parse_svg_transform(s: &str) -> TransAffine {
    let s = s.trim();

    let parse_nums = |inner: &str| -> Vec<f64> {
        inner
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect()
    };

    if let Some(inner) = s.strip_prefix("matrix(").and_then(|s| s.strip_suffix(')')) {
        let nums = parse_nums(inner);
        if nums.len() == 6 {
            return TransAffine::new_custom(nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]);
        }
    } else if let Some(inner) = s
        .strip_prefix("translate(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let nums = parse_nums(inner);
        let tx = nums.first().copied().unwrap_or(0.0);
        let ty = nums.get(1).copied().unwrap_or(0.0);
        return TransAffine::new_custom(1.0, 0.0, 0.0, 1.0, tx, ty);
    } else if let Some(inner) = s.strip_prefix("scale(").and_then(|s| s.strip_suffix(')')) {
        let nums = parse_nums(inner);
        let sx = nums.first().copied().unwrap_or(1.0);
        let sy = nums.get(1).copied().unwrap_or(sx);
        return TransAffine::new_custom(sx, 0.0, 0.0, sy, 0.0, 0.0);
    } else if let Some(inner) = s.strip_prefix("rotate(").and_then(|s| s.strip_suffix(')')) {
        let nums = parse_nums(inner);
        let angle = nums.first().copied().unwrap_or(0.0).to_radians();
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        return TransAffine::new_custom(cos_a, sin_a, -sin_a, cos_a, 0.0, 0.0);
    }
    TransAffine::new()
}

/// Parse SVG color string (#RRGGBB, #RGB, named colors).
#[cfg(all(feature = "std", feature = "xml"))]
fn parse_svg_color(s: &str) -> Option<Rgba8> {
    let s = s.trim();
    if s.starts_with('#') {
        let hex = &s[1..];
        return match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Rgba8 { r, g, b, a: 255 })
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Rgba8 { r, g, b, a: 255 })
            }
            _ => None,
        };
    }
    match s.to_lowercase().as_str() {
        "black" => Some(Rgba8 {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }),
        "white" => Some(Rgba8 {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
        "red" => Some(Rgba8 {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }),
        "green" => Some(Rgba8 {
            r: 0,
            g: 128,
            b: 0,
            a: 255,
        }),
        "blue" => Some(Rgba8 {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        }),
        "yellow" => Some(Rgba8 {
            r: 255,
            g: 255,
            b: 0,
            a: 255,
        }),
        "orange" => Some(Rgba8 {
            r: 255,
            g: 165,
            b: 0,
            a: 255,
        }),
        "gold" => Some(Rgba8 {
            r: 255,
            g: 215,
            b: 0,
            a: 255,
        }),
        _ => None,
    }
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
    fn vertical_scroll_one_strip_and_translates() {
        let mut p = xy_pixmap(200, 100);
        // Scroll DOWN by 30 → content moves UP → bottom strip exposed.
        let strips = scroll_shift_region(&mut p, &rect(0.0, 0.0, 200.0, 100.0), (0.0, 30.0), 1.0);
        assert_eq!(strips.len(), 1, "single-axis scroll = one strip, got {:?}", strips);
        let s = &strips[0];
        assert!(
            (s.origin.y - (100.0 - s.size.height)).abs() < 0.01 && s.size.width == 200.0,
            "vertical scroll-down must expose a full-width BOTTOM strip, got {:?}",
            s
        );
        // Kept region (top): (x, y) now holds original (x, y+30).
        assert_eq!(at(&p, 50, 10), [50, 40, 0, 255], "content not translated up by 30");
    }

    #[test]
    fn diagonal_pan_two_strips_and_translates() {
        let mut p = xy_pixmap(200, 100);
        // Diagonal scroll down-right by (20, 30): content moves up-left.
        let strips =
            scroll_shift_region(&mut p, &rect(0.0, 0.0, 200.0, 100.0), (20.0, 30.0), 1.0);
        assert_eq!(
            strips.len(),
            2,
            "diagonal pan must expose TWO strips (L-shape), got {:?}",
            strips
        );
        // One full-width strip (the vertical move) + one full-height strip (horizontal).
        let has_h_strip = strips.iter().any(|s| s.size.width == 200.0);
        let has_v_strip = strips.iter().any(|s| s.size.height == 100.0);
        assert!(
            has_h_strip && has_v_strip,
            "expected a full-width AND a full-height strip, got {:?}",
            strips
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
        let _ = scroll_shift_region(&mut p, &clip, (0.0, 10.0), 1.0);
        for &(x, y) in &[(0u32, 0u32), (199, 99), (100, 5), (100, 90), (2, 50), (190, 50)] {
            assert_eq!(
                at(&p, x, y),
                [(x & 0xFF) as u8, (y & 0xFF) as u8, 0, 255],
                "pixel ({},{}) OUTSIDE the clip was modified — scroll leaked past its frame",
                x,
                y
            );
        }
        // Inside the kept region it DID move: (50,40) holds original (50,50).
        assert_eq!(at(&p, 50, 40), [50, 50, 0, 255], "inside-clip content not shifted");
    }

    #[test]
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
