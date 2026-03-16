//! CPU rendering for solver3 DisplayList
//!
//! This module renders a flat DisplayList (from solver3) to an AzulPixmap using agg-rust.
//! Unlike the old hierarchical CachedDisplayList, the new DisplayList is a simple
//! flat vector of rendering commands that can be executed sequentially.

use std::collections::HashMap;

use azul_core::{
    dom::ScrollbarOrientation,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::{
        DecodedImage, FontInstanceKey, ImageRef,
        RendererResources,
    },
    ui_solver::GlyphInstance,
};
use azul_css::props::basic::{ColorU, ColorOrSystem, FontRef};
use azul_css::props::style::filter::StyleFilter;

use agg_rust::{
    basics::{FillingRule, VertexSource, PATH_FLAGS_NONE},
    blur::stack_blur_rgba32,
    path_storage::PathStorage,
    color::Rgba8,
    conv_stroke::ConvStroke,
    conv_transform::ConvTransform,
    gradient_lut::GradientLut,
    pixfmt_rgba::{PixfmtRgba32, PixelFormat},
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
                size: LogicalSize { width: width as f32, height: height as f32 },
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
                DisplayListItem::PushScrollFrame { clip_bounds, content_size, scroll_id } => {
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
                DisplayListItem::PushReferenceFrame { initial_transform, bounds, .. } => {
                    let m = &initial_transform.m;
                    let is_identity =
                        (m[0][0] - 1.0).abs() < 0.0001 &&
                        m[0][1].abs() < 0.0001 &&
                        m[1][0].abs() < 0.0001 &&
                        (m[1][1] - 1.0).abs() < 0.0001 &&
                        m[3][0].abs() < 0.0001 &&
                        m[3][1].abs() < 0.0001;
                    if !is_identity {
                        let b = *bounds.inner();
                        let pw = (b.size.width * dpi_factor).ceil().max(1.0) as u32;
                        let ph = (b.size.height * dpi_factor).ceil().max(1.0) as u32;
                        let new_id = self.alloc_layer_id();
                        let mut layer = Layer::new(new_id, b, pw, ph);
                        layer.transform = TransAffine::new_custom(
                            m[0][0] as f64, m[0][1] as f64,
                            m[1][0] as f64, m[1][1] as f64,
                            m[3][0] as f64, m[3][1] as f64,
                        );
                        let end = find_matching_pop(&display_list.items, i, MatchKind::ReferenceFrame);
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
                            if !layer.transform.is_identity(0.0001) {
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
    ) -> Result<(), String> {
        // Collect layer IDs and their display list ranges
        let layer_ranges: Vec<(LayerId, (usize, usize), LogicalRect)> = self.layers
            .iter()
            .map(|(id, layer)| (*id, layer.display_list_range, layer.bounds))
            .collect();

        for (layer_id, range, layer_bounds) in &layer_ranges {
            let (start, end) = *range;
            if start >= end || start >= display_list.items.len() {
                continue;
            }

            let layer = self.layers.get_mut(layer_id).unwrap();

            // Clear the layer pixbuf (transparent for non-root, white for root)
            if *layer_id == self.root_layer {
                layer.pixbuf.fill(255, 255, 255, 255);
            } else {
                layer.pixbuf.fill(0, 0, 0, 0);
            }

            // Render the display list slice into this layer's pixbuf
            let offset_x = layer_bounds.origin.x;
            let offset_y = layer_bounds.origin.y;
            render_display_list_range(
                display_list,
                &mut layer.pixbuf,
                start,
                end.min(display_list.items.len()),
                offset_x,
                offset_y,
                dpi_factor,
                renderer_resources,
                font_manager,
                glyph_cache,
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
                if layer_id == self.root_layer { 0.0 } else { abs_x },
                if layer_id == self.root_layer { 0.0 } else { abs_y },
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
        let layer_id = self.layers.iter()
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

        // Compute exposed strip and re-render it
        let exposed = compute_exposed_rect(&layer.bounds, dx, dy);
        if let Some(exposed_rect) = exposed {
            layer.damage.push(exposed_rect);
        }

        layer.scroll_offset = new_offset;
        layer.composite_dirty = true;

        // Re-render damaged regions
        let range = layer.display_list_range;
        let bounds = layer.bounds;
        let offset_x = bounds.origin.x;
        let offset_y = bounds.origin.y;
        render_display_list_range(
            display_list,
            &mut self.layers.get_mut(&layer_id).unwrap().pixbuf,
            range.0,
            range.1.min(display_list.items.len()),
            offset_x,
            offset_y,
            dpi_factor,
            renderer_resources,
            font_manager,
            glyph_cache,
        )?;

        Ok(())
    }
}

impl Layer {
    fn new(id: LayerId, bounds: LogicalRect, pixel_width: u32, pixel_height: u32) -> Self {
        Layer {
            id,
            pixbuf: AzulPixmap::new(pixel_width.max(1), pixel_height.max(1))
                .unwrap_or_else(|| AzulPixmap { data: vec![0; 4], width: 1, height: 1 }),
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
                if depth == 0 { return i; }
            }
            (DisplayListItem::PushOpacity { .. }, MatchKind::Opacity) => depth += 1,
            (DisplayListItem::PopOpacity, MatchKind::Opacity) => {
                depth -= 1;
                if depth == 0 { return i; }
            }
            (DisplayListItem::PushFilter { .. }, MatchKind::Filter) => depth += 1,
            (DisplayListItem::PopFilter, MatchKind::Filter) => {
                depth -= 1;
                if depth == 0 { return i; }
            }
            (DisplayListItem::PushReferenceFrame { .. }, MatchKind::ReferenceFrame) => depth += 1,
            (DisplayListItem::PopReferenceFrame, MatchKind::ReferenceFrame) => {
                depth -= 1;
                if depth == 0 { return i; }
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
            size: LogicalSize { width: x2 - x1, height: y2 - y1 },
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
        if dy < 0 || dy >= dh { continue; }
        for sx in 0..sw {
            let dx = px_x + sx;
            if dx < 0 || dx >= dw { continue; }
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((dy * dw + dx) * 4) as usize;
            if si + 3 >= src.data.len() || di + 3 >= dst.data.len() { continue; }

            let sr = src.data[si] as u32;
            let sg = src.data[si + 1] as u32;
            let sb = src.data[si + 2] as u32;
            let sa = (src.data[si + 3] as u32 * op) / 255;

            if sa == 0 { continue; }
            if sa == 255 {
                dst.data[di] = sr as u8;
                dst.data[di + 1] = sg as u8;
                dst.data[di + 2] = sb as u8;
                dst.data[di + 3] = 255;
            } else {
                let inv_sa = 255 - sa;
                dst.data[di]     = ((sr * sa + dst.data[di] as u32 * inv_sa) / 255) as u8;
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

/// Compute the exposed rectangle after a scroll of (dx, dy) in logical coords.
fn compute_exposed_rect(bounds: &LogicalRect, dx: f32, dy: f32) -> Option<LogicalRect> {
    let w = bounds.size.width;
    let h = bounds.size.height;

    // Vertical exposed strip
    if dy.abs() > 0.5 {
        let strip = if dy > 0.0 {
            // Scrolled down — top strip exposed
            LogicalRect {
                origin: LogicalPosition { x: bounds.origin.x, y: bounds.origin.y },
                size: LogicalSize { width: w, height: dy.min(h) },
            }
        } else {
            // Scrolled up — bottom strip exposed
            LogicalRect {
                origin: LogicalPosition { x: bounds.origin.x, y: bounds.origin.y + h + dy },
                size: LogicalSize { width: w, height: (-dy).min(h) },
            }
        };
        return Some(strip);
    }

    // Horizontal exposed strip
    if dx.abs() > 0.5 {
        let strip = if dx > 0.0 {
            LogicalRect {
                origin: LogicalPosition { x: bounds.origin.x, y: bounds.origin.y },
                size: LogicalSize { width: dx.min(w), height: h },
            }
        } else {
            LogicalRect {
                origin: LogicalPosition { x: bounds.origin.x + w + dx, y: bounds.origin.y },
                size: LogicalSize { width: (-dx).min(w), height: h },
            }
        };
        return Some(strip);
    }

    None
}

/// Apply CSS filters to a pixbuf at composite time.
fn apply_layer_filters(pixmap: &mut AzulPixmap, filters: &[StyleFilter], dpi_factor: f32) {
    for filter in filters {
        match filter {
            StyleFilter::Blur(blur) => {
                let rx = blur.width.to_pixels_internal(0.0, 16.0) * dpi_factor;
                let ry = blur.height.to_pixels_internal(0.0, 16.0) * dpi_factor;
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
                    chunk[0] = ((((chunk[0] as f32 / 255.0) - 0.5) * factor + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
                    chunk[1] = ((((chunk[1] as f32 / 255.0) - 0.5) * factor + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
                    chunk[2] = ((((chunk[2] as f32 / 255.0) - 0.5) * factor + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
                }
            }
            StyleFilter::Invert(pct) => {
                let amount = pct.normalized().clamp(0.0, 1.0);
                for chunk in pixmap.data.chunks_exact_mut(4) {
                    chunk[0] = (chunk[0] as f32 + (255.0 - 2.0 * chunk[0] as f32) * amount).clamp(0.0, 255.0) as u8;
                    chunk[1] = (chunk[1] as f32 + (255.0 - 2.0 * chunk[1] as f32) * amount).clamp(0.0, 255.0) as u8;
                    chunk[2] = (chunk[2] as f32 + (255.0 - 2.0 * chunk[2] as f32) * amount).clamp(0.0, 255.0) as u8;
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
            _ => {} // Other filters not yet implemented
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
    offset_x: f32,
    offset_y: f32,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
) -> Result<(), String> {
    // Create a sub-display-list view and render it
    // For now, we render the full range using the existing render_display_list logic
    // Items have absolute coordinates, so non-root layers need coordinate offset
    let mut transform_stack = vec![TransAffine::new()];
    let mut clip_stack: Vec<Option<AzRect>> = vec![None];
    let mut mask_stack: Vec<MaskEntry> = Vec::new();

    for i in start..end {
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
        let mut data = vec![255u8; len]; // opaque white
        Some(Self { data, width, height })
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

    /// Encode to PNG using the `png` crate.
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header()
                .map_err(|e| format!("PNG header error: {}", e))?;
            writer.write_image_data(&self.data)
                .map_err(|e| format!("PNG write error: {}", e))?;
        }
        Ok(buf)
    }

    /// Decode a PNG byte slice into an AzulPixmap.
    pub fn decode_png(png_bytes: &[u8]) -> Result<Self, String> {
        let decoder = png::Decoder::new(png_bytes);
        let mut reader = decoder.read_info()
            .map_err(|e| format!("PNG decode error: {}", e))?;
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf)
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

        Ok(Self { data, width, height })
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
        if self.total_pixels == 0 { 0.0 }
        else { self.diff_count as f64 / self.total_pixels as f64 }
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

    for (ref_chunk, test_chunk) in reference.data.chunks_exact(4).zip(test.data.chunks_exact(4)) {
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
        if w <= 0.0 || h <= 0.0 || !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
            return None;
        }
        Some(Self { x, y, width: w, height: h })
    }

    /// Intersect this rect with a clip rect. Returns None if fully clipped.
    fn clip(&self, clip: &AzRect) -> Option<AzRect> {
        let x1 = self.x.max(clip.x);
        let y1 = self.y.max(clip.y);
        let x2 = (self.x + self.width).min(clip.x + clip.width);
        let y2 = (self.y + self.height).min(clip.y + clip.height);
        if x2 > x1 && y2 > y1 {
            Some(AzRect { x: x1, y: y1, width: x2 - x1, height: y2 - y1 })
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
    let mut ra = unsafe {
        RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride)
    };
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
    if transform.is_identity(0.0001) {
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
    let mut ra = unsafe {
        RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride)
    };
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

/// Resolve a ColorOrSystem to a concrete ColorU (system colors fall back to gray).
fn resolve_color(color: &ColorOrSystem) -> ColorU {
    match color {
        ColorOrSystem::Color(c) => *c,
        ColorOrSystem::System(_) => ColorU { r: 128, g: 128, b: 128, a: 255 },
    }
}

/// Build a GradientLut from normalized linear color stops.
fn build_gradient_lut_linear(
    stops: &azul_css::props::style::background::NormalizedLinearColorStopVec,
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
        let c = resolve_color(&stop.color);
        lut.add_color(offset, Rgba8::new(c.r as u32, c.g as u32, c.b as u32, c.a as u32));
    }
    lut.build_lut();
    lut
}

/// Build a GradientLut from normalized radial (conic) color stops.
fn build_gradient_lut_radial(
    stops: &azul_css::props::style::background::NormalizedRadialColorStopVec,
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
        let c = resolve_color(&stop.color);
        lut.add_color(offset, Rgba8::new(c.r as u32, c.g as u32, c.b as u32, c.a as u32));
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
    use azul_css::props::style::background::{BackgroundPositionHorizontal, BackgroundPositionVertical};

    let x = match pos.horizontal {
        BackgroundPositionHorizontal::Left => 0.0,
        BackgroundPositionHorizontal::Center => 0.5,
        BackgroundPositionHorizontal::Right => 1.0,
        BackgroundPositionHorizontal::Exact(px) => {
            let val = px.to_pixels_internal(width, 16.0);
            if width > 0.0 { val / width } else { 0.5 }
        }
    };
    let y = match pos.vertical {
        BackgroundPositionVertical::Top => 0.0,
        BackgroundPositionVertical::Center => 0.5,
        BackgroundPositionVertical::Bottom => 1.0,
        BackgroundPositionVertical::Exact(px) => {
            let val = px.to_pixels_internal(height, 16.0);
            if height > 0.0 { val / height } else { 0.5 }
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

    let lut = build_gradient_lut_linear(&gradient.stops);

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

    // Build transform: maps gradient line to X axis
    // We need the inverse: pixel space -> gradient space
    let angle = dy.atan2(dx);
    let mut transform = TransAffine::new_translation(x1, y1);
    transform.rotate(angle);
    transform.scale(len / 100.0, len / 100.0); // scale so d1=0, d2=100 maps to gradient length
    transform.invert();

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient_clipped(pixmap, &mut path, &lut, GradientX, transform, 0.0, 100.0, clip);
    Ok(())
}

fn render_radial_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::RadialGradient,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
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

    let lut = build_gradient_lut_linear(&gradient.stops);

    let w = rect.width as f64;
    let h = rect.height as f64;

    // Compute center from position
    let (cx_frac, cy_frac) = resolve_background_position(&gradient.position, rect.width, rect.height);
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

    // Build transform: maps center to origin, scales radius to 100
    let mut transform = TransAffine::new_translation(cx, cy);
    transform.scale(radius / 100.0, radius / 100.0);
    transform.invert();

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient_clipped(pixmap, &mut path, &lut, GradientRadialD, transform, 0.0, 100.0, clip);
    Ok(())
}

fn render_conic_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::ConicGradient,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let stops = gradient.stops.as_ref();
    if stops.is_empty() {
        return Ok(());
    }

    let lut = build_gradient_lut_radial(&gradient.stops);

    let w = rect.width as f64;
    let h = rect.height as f64;

    // Compute center
    let (cx_frac, cy_frac) = resolve_background_position(&gradient.center, rect.width, rect.height);
    let cx = rect.x as f64 + cx_frac as f64 * w;
    let cy = rect.y as f64 + cy_frac as f64 * h;

    // Start angle (CSS conic gradients start at 12 o'clock = -90deg in math coords)
    let start_angle_deg = gradient.angle.to_degrees();
    let start_angle_rad = ((start_angle_deg - 90.0) as f64).to_radians();

    // Build transform: translate center to origin, rotate by start angle
    let mut transform = TransAffine::new_translation(cx, cy);
    transform.rotate(start_angle_rad);
    transform.invert();

    // GradientConic maps atan2(y,x) * d / pi, covering [0, d] for the half-circle.
    // We use d2 = 100 as the range; the LUT maps 0..1 over that.
    let d2 = 100.0;

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient_clipped(pixmap, &mut path, &lut, GradientConic, transform, 0.0, d2, clip);
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

    let offset_x = shadow.offset_x.inner.to_pixels_internal(0.0, 16.0) * dpi_factor;
    let offset_y = shadow.offset_y.inner.to_pixels_internal(0.0, 16.0) * dpi_factor;
    let blur_r = (shadow.blur_radius.inner.to_pixels_internal(0.0, 16.0) * dpi_factor).max(0.0);
    let spread = shadow.spread_radius.inner.to_pixels_internal(0.0, 16.0) * dpi_factor;

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

    if sw == 0 || sh == 0 || sw > 4096 || sh > 4096 {
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

    let agg_color = Rgba8::new(color.r as u32, color.g as u32, color.b as u32, color.a as u32);
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
        let mut ra = unsafe {
            RowAccessor::new_with_buf(tmp.data.as_mut_ptr(), sw, sh, stride)
        };
        stack_blur_rgba32(&mut ra, blur_radius, blur_radius);
    }

    // Blit the shadow buffer onto the main pixmap
    let dst_x = shadow_x as i32;
    let dst_y = shadow_y as i32;
    blit_buffer(pixmap, &tmp.data, sw, sh, dst_x, dst_y);

    Ok(())
}

/// Alpha-blend one RGBA buffer onto another at (dx, dy).
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
                let da = 255 - sa;
                dst.data[di] = ((src[si] as u32 * sa + dst.data[di] as u32 * da) / 255) as u8;
                dst.data[di + 1] = ((src[si + 1] as u32 * sa + dst.data[di + 1] as u32 * da) / 255) as u8;
                dst.data[di + 2] = ((src[si + 2] as u32 * sa + dst.data[di + 2] as u32 * da) / 255) as u8;
                dst.data[di + 3] = ((sa + dst.data[di + 3] as u32 * da / 255).min(255)) as u8;
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
                azul_core::resources::RawImageFormat::R8 => {
                    (bytes.to_vec(), w, h)
                }
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
        MaskEntry::ImageMask { snapshot, mask_data, origin_x, origin_y, width, height } => {
            (snapshot, mask_data.as_slice(), *origin_x, *origin_y, *width, *height)
        }
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

    let mut pixmap = AzulPixmap::new((width * dpi_factor) as u32, (height * dpi_factor) as u32)
        .ok_or_else(|| "cannot create pixmap".to_string())?;

    pixmap.fill(255, 255, 255, 255);

    render_display_list(dl, &mut pixmap, dpi_factor, res, None, glyph_cache)?;

    Ok(pixmap)
}

/// Render a display list using fonts from FontManager directly
/// This is used in reftest scenarios where RendererResources doesn't have fonts registered
pub fn render_with_font_manager(
    dl: &DisplayList,
    res: &RendererResources,
    font_manager: &FontManager<FontRef>,
    opts: RenderOptions,
    glyph_cache: &mut GlyphCache,
) -> Result<AzulPixmap, String> {
    let RenderOptions {
        width,
        height,
        dpi_factor,
    } = opts;

    let mut pixmap = AzulPixmap::new((width * dpi_factor) as u32, (height * dpi_factor) as u32)
        .ok_or_else(|| "cannot create pixmap".to_string())?;

    pixmap.fill(255, 255, 255, 255);

    render_display_list(dl, &mut pixmap, dpi_factor, res, Some(font_manager), glyph_cache)?;

    Ok(pixmap)
}

fn render_display_list(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
) -> Result<(), String> {
    let mut transform_stack = vec![TransAffine::new()]; // identity
    let mut clip_stack: Vec<Option<AzRect>> = vec![None];
    let mut mask_stack: Vec<MaskEntry> = Vec::new();

    for item in &display_list.items {
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
) -> Result<(), String> {
    match item {
            DisplayListItem::Rect {
                bounds,
                color,
                border_radius,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
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
                    bounds.inner(),
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
                    bounds.inner(),
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
                use azul_css::{css::CssPropertyValue, props::basic::pixel::DEFAULT_FONT_SIZE};

                let width = widths
                    .top
                    .and_then(|w| w.get_property().cloned())
                    .map(|w| w.inner.to_pixels_internal(0.0, DEFAULT_FONT_SIZE))
                    .unwrap_or(0.0);

                let color = colors
                    .top
                    .and_then(|c| c.get_property().cloned())
                    .map(|c| c.inner)
                    .unwrap_or(ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    });

                let simple_radius = BorderRadius {
                    top_left: border_radius
                        .top_left
                        .to_pixels_internal(bounds.0.size.width, DEFAULT_FONT_SIZE),
                    top_right: border_radius
                        .top_right
                        .to_pixels_internal(bounds.0.size.width, DEFAULT_FONT_SIZE),
                    bottom_left: border_radius
                        .bottom_left
                        .to_pixels_internal(bounds.0.size.width, DEFAULT_FONT_SIZE),
                    bottom_right: border_radius
                        .bottom_right
                        .to_pixels_internal(bounds.0.size.width, DEFAULT_FONT_SIZE),
                };

                let clip = *clip_stack.last().unwrap();
                render_border(
                    pixmap,
                    bounds.inner(),
                    color,
                    width,
                    &simple_radius,
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Underline {
                bounds,
                color,
                thickness,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    *color,
                    &BorderRadius::default(),
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Strikethrough {
                bounds,
                color,
                thickness,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    *color,
                    &BorderRadius::default(),
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Overline {
                bounds,
                color,
                thickness,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
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
            } => {
                let clip = *clip_stack.last().unwrap();
                render_text(
                    glyphs,
                    *font_hash,
                    *font_size_px,
                    *color,
                    pixmap,
                    clip_rect.inner(),
                    clip,
                    renderer_resources,
                    font_manager,
                    dpi_factor,
                    glyph_cache,
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
                    bounds.inner(),
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
                    bounds.inner(),
                    *color,
                    &BorderRadius::default(),
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::ScrollBarStyled { info } => {
                let clip = *clip_stack.last().unwrap();

                // Render track
                if info.track_color.a > 0 {
                    render_rect(
                        pixmap,
                        info.track_bounds.inner(),
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
                            btn_bounds.inner(),
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
                            btn_bounds.inner(),
                            info.button_color,
                            &BorderRadius::default(),
                            clip,
                            dpi_factor,
                        )?;
                    }
                }

                // Render thumb
                if info.thumb_color.a > 0 {
                    render_rect(
                        pixmap,
                        info.thumb_bounds.inner(),
                        info.thumb_color,
                        &info.thumb_border_radius,
                        clip,
                        dpi_factor,
                    )?;
                }
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
            DisplayListItem::PushScrollFrame {
                clip_bounds,
                content_size: _,
                scroll_id: _,
            } => {
                // Scroll frame = clip + translation
                // The display list builder already offsets child positions by scroll amount,
                // so we only need the clip. But we push a transform identity marker
                // so PopScrollFrame can restore the transform stack.
                let new_clip = logical_rect_to_az_rect(clip_bounds.inner(), dpi_factor);
                clip_stack.push(new_clip);
                transform_stack.push(transform_stack.last().cloned().unwrap_or_else(TransAffine::new));
            }
            DisplayListItem::PopScrollFrame => {
                clip_stack.pop();
                if clip_stack.is_empty() {
                    return Err("Clip stack underflow from scroll frame".to_string());
                }
                if transform_stack.len() > 1 {
                    transform_stack.pop();
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
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    ColorU {
                        r: 200,
                        g: 200,
                        b: 255,
                        a: 128,
                    },
                    &BorderRadius::default(),
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::VirtualViewPlaceholder { .. } => {}

            // Gradient rendering
            DisplayListItem::LinearGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_linear_gradient(
                    pixmap,
                    bounds.inner(),
                    gradient,
                    border_radius,
                    clip,
                    dpi_factor,
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
                    bounds.inner(),
                    gradient,
                    border_radius,
                    clip,
                    dpi_factor,
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
                    bounds.inner(),
                    gradient,
                    border_radius,
                    clip,
                    dpi_factor,
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
                    bounds.inner(),
                    shadow,
                    border_radius,
                    dpi_factor,
                )?;
            }

            // --- Opacity layers ---
            DisplayListItem::PushOpacity { bounds, opacity } => {
                let rect = logical_rect_to_az_rect(bounds.inner(), dpi_factor);
                if let Some(r) = rect {
                    let snap = snapshot_region(pixmap, r.x as i32, r.y as i32, r.width as u32, r.height as u32);
                    mask_stack.push(MaskEntry::Opacity {
                        snapshot: snap,
                        rect: r,
                        opacity: *opacity,
                    });
                }
            }
            DisplayListItem::PopOpacity => {
                if let Some(MaskEntry::Opacity { snapshot, rect, opacity }) = mask_stack.pop() {
                    let x = rect.x as i32;
                    let y = rect.y as i32;
                    let w = rect.width as u32;
                    let h = rect.height as u32;
                    let pw = pixmap.width as i32;
                    let ph = pixmap.height as i32;
                    // Blend: result = snapshot + (current - snapshot) * opacity
                    for py in 0..h as i32 {
                        let dy = y + py;
                        if dy < 0 || dy >= ph { continue; }
                        for px in 0..w as i32 {
                            let dx = x + px;
                            if dx < 0 || dx >= pw { continue; }
                            let pi = ((dy as u32 * pixmap.width + dx as u32) * 4) as usize;
                            let si = ((py as u32 * w + px as u32) * 4) as usize;
                            if pi + 3 >= pixmap.data.len() || si + 3 >= snapshot.len() { continue; }
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
                transform_key: _,
                initial_transform,
                bounds,
            } => {
                // Extract 2D affine from the 4x4 matrix and compose with current transform
                let m = &initial_transform.m;
                let tf = TransAffine::new_custom(
                    m[0][0] as f64, m[0][1] as f64, // sx, shy
                    m[1][0] as f64, m[1][1] as f64, // shx, sy
                    m[3][0] as f64, m[3][1] as f64, // tx, ty
                );
                let current = transform_stack.last().cloned().unwrap_or_else(TransAffine::new);
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
                let mr = mask_rect.inner();
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

    let agg_color = Rgba8::new(color.r as u32, color.g as u32, color.b as u32, color.a as u32);

    if border_radius.is_zero() {
        let mut path = build_rect_path(&rect);
        agg_fill_path_clipped(pixmap, &mut path, &agg_color, FillingRule::NonZero, clip);
    } else {
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

    let agg_color = Rgba8::new(color.r as u32, color.g as u32, color.b as u32, color.a as u32);

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

    // Draw each glyph using cached paths
    for glyph in glyphs {
        let glyph_index = glyph.index as u16;

        let glyph_data = match parsed_font.glyph_records_decoded.get(&glyph_index) {
            Some(d) => d,
            None => continue,
        };

        let cached = match glyph_cache.get_or_build(
            font_hash.font_hash, glyph_index, glyph_data, parsed_font, ppem,
        ) {
            Some(c) => c,
            None => continue,
        };

        let glyph_x = glyph.point.x * dpi_factor;
        let glyph_baseline_y = glyph.point.y * dpi_factor;

        let glyph_transform = if cached.is_hinted {
            // Hinted path is in pixel coordinates — snap to pixel grid
            TransAffine::new_translation(glyph_x.round() as f64, glyph_baseline_y.round() as f64)
        } else {
            // Unhinted path is in font units — apply scale + translate
            let mut t = TransAffine::new_scaling_uniform(scale as f64);
            t.multiply(&TransAffine::new_translation(glyph_x as f64, glyph_baseline_y as f64));
            t
        };

        let mut path_clone = cached.path.clone();
        agg_fill_transformed_path_clipped(
            pixmap,
            &mut path_clone,
            &agg_color,
            FillingRule::NonZero,
            &glyph_transform,
            clip,
        );
    }

    Ok(())
}

fn render_border(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    color: ColorU,
    width: f32,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    if color.a == 0 || width <= 0.0 {
        return Ok(());
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
    let agg_color = Rgba8::new(color.r as u32, color.g as u32, color.b as u32, color.a as u32);

    let mut path = PathStorage::new();

    // 1. Add Outer Path
    let x = rect.x as f64;
    let y = rect.y as f64;
    let w = rect.width as f64;
    let h = rect.height as f64;

    if border_radius.is_zero() {
        path.move_to(x, y);
        path.line_to(x + w, y);
        path.line_to(x + w, y + h);
        path.line_to(x, y + h);
        path.close_polygon(PATH_FLAGS_NONE);
    } else {
        let tl = (border_radius.top_left * dpi_factor) as f64;
        let tr = (border_radius.top_right * dpi_factor) as f64;
        let br = (border_radius.bottom_right * dpi_factor) as f64;
        let bl = (border_radius.bottom_left * dpi_factor) as f64;

        path.move_to(x + tl, y);
        path.line_to(x + w - tr, y);
        if tr > 0.0 {
            path.curve3(x + w, y, x + w, y + tr);
        }
        path.line_to(x + w, y + h - br);
        if br > 0.0 {
            path.curve3(x + w, y + h, x + w - br, y + h);
        }
        path.line_to(x + bl, y + h);
        if bl > 0.0 {
            path.curve3(x, y + h, x, y + h - bl);
        }
        path.line_to(x, y + tl);
        if tl > 0.0 {
            path.curve3(x, y, x + tl, y);
        }
        path.close_polygon(PATH_FLAGS_NONE);
    }

    // 2. Add Inner Path (same winding — EvenOdd fill creates the hole)
    let sw = scaled_width as f64;
    let ir = AzRect::from_xywh(
        rect.x + scaled_width,
        rect.y + scaled_width,
        rect.width - 2.0 * scaled_width,
        rect.height - 2.0 * scaled_width,
    );

    if let Some(ir) = ir {
        let ix = ir.x as f64;
        let iy = ir.y as f64;
        let iw = ir.width as f64;
        let ih = ir.height as f64;

        if border_radius.is_zero() {
            path.move_to(ix, iy);
            path.line_to(ix + iw, iy);
            path.line_to(ix + iw, iy + ih);
            path.line_to(ix, iy + ih);
            path.close_polygon(PATH_FLAGS_NONE);
        } else {
            let tl = ((border_radius.top_left * dpi_factor - scaled_width).max(0.0)) as f64;
            let tr = ((border_radius.top_right * dpi_factor - scaled_width).max(0.0)) as f64;
            let br = ((border_radius.bottom_right * dpi_factor - scaled_width).max(0.0)) as f64;
            let bl = ((border_radius.bottom_left * dpi_factor - scaled_width).max(0.0)) as f64;

            path.move_to(ix + tl, iy);
            path.line_to(ix + iw - tr, iy);
            if tr > 0.0 {
                path.curve3(ix + iw, iy, ix + iw, iy + tr);
            }
            path.line_to(ix + iw, iy + ih - br);
            if br > 0.0 {
                path.curve3(ix + iw, iy + ih, ix + iw - br, iy + ih);
            }
            path.line_to(ix + bl, iy + ih);
            if bl > 0.0 {
                path.curve3(ix, iy + ih, ix, iy + ih - bl);
            }
            path.line_to(ix, iy + tl);
            if tl > 0.0 {
                path.curve3(ix, iy, ix + tl, iy);
            }
            path.close_polygon(PATH_FLAGS_NONE);
        }
    }

    // 3. Fill with EvenOdd to create the hole
    agg_fill_path_clipped(pixmap, &mut path, &agg_color, FillingRule::EvenOdd, clip);

    Ok(())
}

fn logical_rect_to_az_rect(
    bounds: &LogicalRect,
    dpi_factor: f32,
) -> Option<AzRect> {
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
            if w == 0 || h == 0 { return Ok(()); }
            let bytes = match data {
                azul_core::resources::ImageData::Raw(shared) => shared.as_ref(),
                _ => return Ok(()),
            };

            let rgba = match descriptor.format {
                azul_core::resources::RawImageFormat::BGRA8 => {
                    let mut out = Vec::with_capacity(bytes.len());
                    for chunk in bytes.chunks_exact(4) {
                        let b = chunk[0]; let g = chunk[1]; let r = chunk[2]; let a = chunk[3];
                        out.push(r); out.push(g); out.push(b); out.push(a);
                    }
                    out
                }
                azul_core::resources::RawImageFormat::R8 => {
                    let mut out = Vec::with_capacity(bytes.len() * 4);
                    for &v in bytes {
                        out.push(v); out.push(v); out.push(v); out.push(v);
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
        (c.x as i32, c.y as i32, (c.x + c.width) as i32, (c.y + c.height) as i32)
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
                    pixmap.data[di]     = src_rgba[si];
                    pixmap.data[di + 1] = src_rgba[si + 1];
                    pixmap.data[di + 2] = src_rgba[si + 2];
                    pixmap.data[di + 3] = 255;
                } else if sa > 0 {
                    // Alpha blend: dst = src * sa + dst * (255 - sa)
                    let da = 255 - sa;
                    pixmap.data[di]     = ((src_rgba[si] as u32 * sa + pixmap.data[di] as u32 * da) / 255) as u8;
                    pixmap.data[di + 1] = ((src_rgba[si + 1] as u32 * sa + pixmap.data[di + 1] as u32 * da) / 255) as u8;
                    pixmap.data[di + 2] = ((src_rgba[si + 2] as u32 * sa + pixmap.data[di + 2] as u32 * da) / 255) as u8;
                    pixmap.data[di + 3] = ((sa + pixmap.data[di + 3] as u32 * da / 255).min(255)) as u8;
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

    // Start at top-left corner (after radius)
    path.move_to(x + tl, y);

    // Top edge
    path.line_to(x + w - tr, y);

    // Top-right corner
    if tr > 0.0 {
        path.curve3(x + w, y, x + w, y + tr);
    }

    // Right edge
    path.line_to(x + w, y + h - br);

    // Bottom-right corner
    if br > 0.0 {
        path.curve3(x + w, y + h, x + w - br, y + h);
    }

    // Bottom edge
    path.line_to(x + bl, y + h);

    // Bottom-left corner
    if bl > 0.0 {
        path.curve3(x, y + h, x, y + h - bl);
    }

    // Left edge
    path.line_to(x, y + tl);

    // Top-left corner
    if tl > 0.0 {
        path.curve3(x, y, x + tl, y);
    }

    path.close_polygon(PATH_FLAGS_NONE);
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
            background_color: ColorU { r: 255, g: 255, b: 255, a: 255 },
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
    use std::collections::{BTreeMap, HashMap};
    use azul_core::{
        dom::DomId,
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::{IdNamespace, RendererResources},
        selection::{SelectionState, TextSelection},
    };
    use crate::{
        solver3::{
            self,
            cache::LayoutCache,
            display_list::DisplayList,
        },
        font_traits::TextLayoutCache,
    };

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
    ).map_err(|e| format!("Failed to create preview font manager: {:?}", e))?;

    // --- Font resolution ---
    {
        use crate::solver3::getters::{
            collect_and_resolve_font_chains, collect_font_ids_from_chains,
            compute_fonts_to_load, load_fonts_from_disk, register_embedded_fonts_from_styled_dom,
        };

        let platform = azul_css::system::Platform::current();

        register_embedded_fonts_from_styled_dom(&styled_dom, &preview_font_manager, &platform);

        let chains = collect_and_resolve_font_chains(&styled_dom, &preview_font_manager.fc_cache, &platform);
        let required_fonts = collect_font_ids_from_chains(&chains);
        let already_loaded = preview_font_manager.get_loaded_font_ids();
        let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);

        if !fonts_to_load.is_empty() {
            use crate::text3::default::PathLoader;
            let loader = PathLoader::new();
            let load_result = load_fonts_from_disk(
                &fonts_to_load,
                &preview_font_manager.fc_cache,
                |bytes, index| loader.load_font(bytes, index),
            );
            preview_font_manager.insert_fonts(load_result.loaded);
        }

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
    };
    let mut text_cache = TextLayoutCache::new();
    let empty_scroll_offsets = BTreeMap::new();
    let empty_selections = BTreeMap::new();
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
        styled_dom,
        viewport,
        &preview_font_manager,
        &empty_scroll_offsets,
        &empty_selections,
        &empty_text_selections,
        &mut debug_messages,
        None,
        &renderer_resources,
        id_namespace,
        dom_id,
        false,
        None,
        &azul_core::resources::ImageCache::default(),
        system_style,
        get_system_time_fn,
    ).map_err(|e| format!("Layout failed: {:?}", e))?;

    // --- Determine actual render size ---
    let (render_width, render_height) = if opts.width.is_some() && opts.height.is_some() {
        (opts.width.unwrap(), opts.height.unwrap())
    } else {
        match compute_content_bounds(&display_list) {
            Some((_min_x, _min_y, max_x, max_y)) => {
                let w = if opts.width.is_some() { opts.width.unwrap() } else { max_x.max(1.0).ceil() };
                let h = if opts.height.is_some() { opts.height.unwrap() } else { max_y.max(1.0).ceil() };
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
    render_display_list(
        &display_list,
        &mut pixmap,
        dpi,
        &renderer_resources,
        Some(&preview_font_manager),
        &mut preview_glyph_cache,
    )?;

    let png_data = pixmap.encode_png()
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
    use azul_core::styled_dom::StyledDom;
    use crate::font_traits::FontManager;

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
    let svg_str = core::str::from_utf8(svg_data)
        .map_err(|e| format!("SVG is not valid UTF-8: {e}"))?;

    let nodes = crate::xml::parse_xml_string(svg_str)
        .map_err(|e| format!("XML parse error: {e}"))?;

    // Find the <svg> root
    let node_slice: &[azul_core::xml::XmlNodeChild] = nodes.as_ref();
    let svg_node = node_slice.iter().find_map(|n| {
        if let azul_core::xml::XmlNodeChild::Element(e) = n {
            let tag = e.node_type.as_str().to_lowercase();
            if tag == "svg" { Some(e) } else { None }
        } else { None }
    }).ok_or_else(|| "No <svg> root element found".to_string())?;

    // Parse viewBox for coordinate mapping
    let vb = parse_viewbox(svg_node);
    let (vb_x, vb_y, vb_w, vb_h) = vb.unwrap_or((0.0, 0.0, target_width as f64, target_height as f64));

    let sx = target_width as f64 / vb_w;
    let sy = target_height as f64 / vb_h;
    let scale = sx.min(sy);

    let root_transform = TransAffine::new_custom(scale, 0.0, 0.0, scale, -vb_x * scale, -vb_y * scale);

    let mut pixmap = AzulPixmap::new(target_width, target_height)
        .ok_or_else(|| "Failed to create pixmap".to_string())?;
    pixmap.fill(255, 255, 255, 255);

    render_svg_group(svg_node, &mut pixmap, &root_transform);

    pixmap.encode_png().map_err(|e| format!("PNG encode error: {e}"))
}

#[cfg(all(feature = "std", feature = "xml"))]
fn parse_viewbox(node: &azul_core::xml::XmlNode) -> Option<(f64, f64, f64, f64)> {
    let vb = node.attributes.get_key("viewbox")
        .or_else(|| node.attributes.get_key("viewBox"))?;
    let nums: Vec<f64> = vb.as_str()
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() == 4 { Some((nums[0], nums[1], nums[2], nums[3])) } else { None }
}

/// Inherited SVG style (fill, stroke, stroke-width) that cascades from parent groups.
#[cfg(all(feature = "std", feature = "xml"))]
#[derive(Clone)]
struct SvgInheritedStyle {
    fill: Option<String>,       // None = not set (inherit default black)
    stroke: Option<String>,     // None = not set (inherit default none)
    stroke_width: Option<f64>,
}

#[cfg(all(feature = "std", feature = "xml"))]
impl Default for SvgInheritedStyle {
    fn default() -> Self {
        Self { fill: None, stroke: None, stroke_width: None }
    }
}

#[cfg(all(feature = "std", feature = "xml"))]
fn render_svg_group(
    node: &azul_core::xml::XmlNode,
    pixmap: &mut AzulPixmap,
    parent_transform: &TransAffine,
) {
    render_svg_group_with_style(node, pixmap, parent_transform, &SvgInheritedStyle::default());
}

#[cfg(all(feature = "std", feature = "xml"))]
fn render_svg_group_with_style(
    node: &azul_core::xml::XmlNode,
    pixmap: &mut AzulPixmap,
    parent_transform: &TransAffine,
    parent_style: &SvgInheritedStyle,
) {
    use azul_core::xml::{XmlNodeChild, XmlNode};
    use agg_rust::math_stroke::{LineCap, LineJoin};

    let group_transform = if let Some(t) = node.attributes.get_key("transform") {
        let mut tf = parse_svg_transform(t.as_str());
        tf.premultiply(parent_transform);
        tf
    } else {
        parent_transform.clone()
    };

    // Inherit style from this group's attributes
    let group_style = SvgInheritedStyle {
        fill: node.attributes.get_key("fill")
            .map(|s| s.as_str().to_string())
            .or_else(|| parent_style.fill.clone()),
        stroke: node.attributes.get_key("stroke")
            .map(|s| s.as_str().to_string())
            .or_else(|| parent_style.stroke.clone()),
        stroke_width: node.attributes.get_key("stroke-width")
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
                let fill_attr = child_node.attributes.get_key("fill")
                    .map(|s| s.as_str().to_string())
                    .or_else(|| group_style.fill.clone());
                let fill_color = match fill_attr.as_deref() {
                    Some("none") => None,
                    Some(c) => parse_svg_color(c),
                    None => Some(Rgba8 { r: 0, g: 0, b: 0, a: 255 }), // SVG default
                };

                let fill_opacity = child_node.attributes.get_key("fill-opacity")
                    .and_then(|s| s.as_str().parse::<f64>().ok())
                    .unwrap_or(1.0);

                let opacity = child_node.attributes.get_key("opacity")
                    .and_then(|s| s.as_str().parse::<f64>().ok())
                    .unwrap_or(1.0);

                if let Some(mut color) = fill_color {
                    color.a = ((color.a as f64) * fill_opacity * opacity).min(255.0) as u8;

                    let fill_rule_str = child_node.attributes.get_key("fill-rule")
                        .map(|s| s.as_str().to_string());
                    let rule = match fill_rule_str.as_deref() {
                        Some("evenodd") => FillingRule::EvenOdd,
                        _ => FillingRule::NonZero,
                    };

                    let mut transformed = ConvTransform::new(&mut curved, elem_transform.clone());
                    agg_fill_path(pixmap, &mut transformed, &color, rule);
                }

                // Stroke: element overrides group
                let stroke_attr = child_node.attributes.get_key("stroke")
                    .map(|s| s.as_str().to_string())
                    .or_else(|| group_style.stroke.clone());
                let stroke_color = match stroke_attr.as_deref() {
                    Some("none") | None => None,
                    Some(c) => parse_svg_color(c),
                };

                if let Some(mut color) = stroke_color {
                    let stroke_opacity = child_node.attributes.get_key("stroke-opacity")
                        .and_then(|s| s.as_str().parse::<f64>().ok())
                        .unwrap_or(1.0);
                    color.a = ((color.a as f64) * stroke_opacity * opacity).min(255.0) as u8;

                    let stroke_width = child_node.attributes.get_key("stroke-width")
                        .and_then(|s| s.as_str().parse::<f64>().ok())
                        .or(group_style.stroke_width)
                        .unwrap_or(1.0);

                    let mut conv_stroke = ConvStroke::new(&mut curved);
                    conv_stroke.set_width(stroke_width);
                    conv_stroke.set_line_cap(LineCap::Round);
                    conv_stroke.set_line_join(LineJoin::Round);

                    let mut transformed = ConvTransform::new(&mut conv_stroke, elem_transform.clone());
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
            let mp = azul_core::svg_path_parser::parse_svg_path_d(d.as_str()).ok()?;
            Some(svg_multi_polygon_to_path_storage(&mp))
        }
        "circle" => {
            let cx = attr_f64(node, "cx");
            let cy = attr_f64(node, "cy");
            let r = attr_f64(node, "r");
            if r <= 0.0 { return None; }
            let mp = azul_core::svg_path_parser::svg_circle_to_paths(cx as f32, cy as f32, r as f32);
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
            } else { rx };
            if w <= 0.0 || h <= 0.0 { return None; }
            let mp = azul_core::svg_path_parser::svg_rect_to_path(x as f32, y as f32, w as f32, h as f32, rx as f32, ry as f32);
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
            if rx <= 0.0 || ry <= 0.0 { return None; }
            // Use circle path with scaling
            let mp = azul_core::svg_path_parser::svg_circle_to_paths(cx as f32, cy as f32, 1.0);
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
            let nums: Vec<f64> = pts_str.as_str()
                .split(|c: char| c == ',' || c.is_ascii_whitespace())
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect();
            if nums.len() < 4 { return None; }
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
    node.attributes.get_key(key)
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
                    path.curve3(q.ctrl.x as f64, q.ctrl.y as f64, q.end.x as f64, q.end.y as f64);
                }
                azul_core::svg::SvgPathElement::CubicCurve(c) => {
                    if first {
                        path.move_to(c.start.x as f64, c.start.y as f64);
                        first = false;
                    }
                    path.curve4(
                        c.ctrl_1.x as f64, c.ctrl_1.y as f64,
                        c.ctrl_2.x as f64, c.ctrl_2.y as f64,
                        c.end.x as f64, c.end.y as f64,
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
    if s.starts_with("matrix(") {
        let inner = &s[7..s.len()-1];
        let nums: Vec<f64> = inner
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        if nums.len() == 6 {
            return TransAffine::new_custom(nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]);
        }
    } else if s.starts_with("translate(") {
        let inner = &s[10..s.len()-1];
        let nums: Vec<f64> = inner
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        let tx = nums.first().copied().unwrap_or(0.0);
        let ty = nums.get(1).copied().unwrap_or(0.0);
        return TransAffine::new_custom(1.0, 0.0, 0.0, 1.0, tx, ty);
    } else if s.starts_with("scale(") {
        let inner = &s[6..s.len()-1];
        let nums: Vec<f64> = inner
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        let sx = nums.first().copied().unwrap_or(1.0);
        let sy = nums.get(1).copied().unwrap_or(sx);
        return TransAffine::new_custom(sx, 0.0, 0.0, sy, 0.0, 0.0);
    } else if s.starts_with("rotate(") {
        let inner = &s[7..s.len()-1];
        let nums: Vec<f64> = inner
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
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
        "black" => Some(Rgba8 { r: 0, g: 0, b: 0, a: 255 }),
        "white" => Some(Rgba8 { r: 255, g: 255, b: 255, a: 255 }),
        "red" => Some(Rgba8 { r: 255, g: 0, b: 0, a: 255 }),
        "green" => Some(Rgba8 { r: 0, g: 128, b: 0, a: 255 }),
        "blue" => Some(Rgba8 { r: 0, g: 0, b: 255, a: 255 }),
        "yellow" => Some(Rgba8 { r: 255, g: 255, b: 0, a: 255 }),
        "orange" => Some(Rgba8 { r: 255, g: 165, b: 0, a: 255 }),
        "gold" => Some(Rgba8 { r: 255, g: 215, b: 0, a: 255 }),
        _ => None,
    }
}
