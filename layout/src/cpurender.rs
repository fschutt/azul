//! CPU rendering for solver3 DisplayList
//!
//! This module renders a flat DisplayList (from solver3) to an AzulPixmap using agg-rust.
//! Unlike the old hierarchical CachedDisplayList, the new DisplayList is a simple
//! flat vector of rendering commands that can be executed sequentially.

use azul_core::{
    dom::ScrollbarOrientation,
    geom::{LogicalPosition, LogicalRect},
    resources::{
        DecodedImage, FontInstanceKey, ImageRef,
        RendererResources,
    },
    ui_solver::GlyphInstance,
};
use azul_css::props::basic::{ColorU, ColorOrSystem, FontRef};

use agg_rust::{
    basics::{FillingRule, VertexSource, PATH_FLAGS_NONE},
    path_storage::PathStorage,
    color::Rgba8,
    conv_stroke::ConvStroke,
    conv_transform::ConvTransform,
    pixfmt_rgba::{PixfmtRgba32, PixelFormat},
    rasterizer_scanline_aa::RasterizerScanlineAa,
    renderer_base::RendererBase,
    renderer_scanline::render_scanlines_aa_solid,
    rendering_buffer::RowAccessor,
    rounded_rect::RoundedRect,
    scanline_u::ScanlineU8,
    trans_affine::TransAffine,
};

use crate::{
    font::parsed::ParsedFont,
    glyph_cache::GlyphCache,
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem},
    text3::cache::{FontHash, FontManager},
};

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
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;
    let mut ra = unsafe {
        RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride)
    };
    let mut pf = PixfmtRgba32::new(&mut ra);
    let mut rb = RendererBase::new(pf);
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
    if transform.is_identity(0.0001) {
        agg_fill_path(pixmap, path, color, rule);
    } else {
        let mut transformed = ConvTransform::new(path, transform.clone());
        agg_fill_path(pixmap, &mut transformed, color, rule);
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

    for item in &display_list.items {
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
                content_size,
                scroll_id,
            } => {
                let new_clip = logical_rect_to_az_rect(clip_bounds.inner(), dpi_factor);
                clip_stack.push(new_clip);
            }
            DisplayListItem::PopScrollFrame => {
                clip_stack.pop();
                if clip_stack.is_empty() {
                    return Err("Clip stack underflow from scroll frame".to_string());
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

            // Gradient rendering - simplified for CPU render
            DisplayListItem::LinearGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                // TODO: Implement proper gradient rendering with agg SpanGradient
                let default_color = ColorU {
                    r: 128,
                    g: 128,
                    b: 128,
                    a: 255,
                };
                let color = gradient
                    .stops
                    .as_ref()
                    .first()
                    .map(|s| match s.color {
                        ColorOrSystem::Color(c) => c,
                        ColorOrSystem::System(_) => default_color,
                    })
                    .unwrap_or(default_color);
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    color,
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
                // TODO: Implement proper radial gradient rendering
                let default_color = ColorU {
                    r: 128,
                    g: 128,
                    b: 128,
                    a: 255,
                };
                let color = gradient
                    .stops
                    .as_ref()
                    .first()
                    .map(|s| match s.color {
                        ColorOrSystem::Color(c) => c,
                        ColorOrSystem::System(_) => default_color,
                    })
                    .unwrap_or(default_color);
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    color,
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
                // TODO: Implement proper conic gradient rendering
                let default_color = ColorU {
                    r: 128,
                    g: 128,
                    b: 128,
                    a: 255,
                };
                let color = gradient
                    .stops
                    .as_ref()
                    .first()
                    .map(|s| match s.color {
                        ColorOrSystem::Color(c) => c,
                        ColorOrSystem::System(_) => default_color,
                    })
                    .unwrap_or(default_color);
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    color,
                    border_radius,
                    clip,
                    dpi_factor,
                )?;
            }

            // BoxShadow - simplified
            DisplayListItem::BoxShadow {
                bounds,
                shadow,
                border_radius,
            } => {
                // TODO: Implement proper box shadow rendering with agg stack_blur
                let offset_bounds = LogicalRect {
                    origin: LogicalPosition {
                        x: bounds.0.origin.x + shadow.offset_x.inner.to_pixels_internal(0.0, 16.0),
                        y: bounds.0.origin.y + shadow.offset_y.inner.to_pixels_internal(0.0, 16.0),
                    },
                    size: bounds.0.size,
                };
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    &offset_bounds,
                    shadow.color,
                    border_radius,
                    clip,
                    dpi_factor,
                )?;
            }

            // Filter effects - not supported in CPU render
            DisplayListItem::PushFilter { .. } => {}
            DisplayListItem::PopFilter => {}
            DisplayListItem::PushBackdropFilter { .. } => {}
            DisplayListItem::PopBackdropFilter => {}
            DisplayListItem::PushOpacity { bounds, opacity } => {}
            DisplayListItem::PopOpacity => {}
            DisplayListItem::PushReferenceFrame { .. } => {}
            DisplayListItem::PopReferenceFrame => {}
            DisplayListItem::PushTextShadow { .. } => {}
            DisplayListItem::PopTextShadow => {}

            DisplayListItem::PushImageMaskClip {
                bounds,
                mask_image,
                mask_rect,
            } => {
                // TODO: implement image mask clipping with agg AlphaMaskU8
            }
            DisplayListItem::PopImageMaskClip => {}
        }
    }

    Ok(())
}

fn render_rect(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    color: ColorU,
    border_radius: &BorderRadius,
    _clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    if color.a == 0 {
        return Ok(());
    }

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let agg_color = Rgba8::new(color.r as u32, color.g as u32, color.b as u32, color.a as u32);

    if border_radius.is_zero() {
        let mut path = build_rect_path(&rect);
        agg_fill_path(pixmap, &mut path, &agg_color, FillingRule::NonZero);
    } else {
        let mut path = build_rounded_rect_path(&rect, border_radius, dpi_factor);
        agg_fill_path(pixmap, &mut path, &agg_color, FillingRule::NonZero);
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
    _clip: Option<AzRect>,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
) -> Result<(), String> {
    if color.a == 0 || glyphs.is_empty() {
        return Ok(());
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
        agg_fill_transformed_path(
            pixmap,
            &mut path_clone,
            &agg_color,
            FillingRule::NonZero,
            &glyph_transform,
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
    _clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    if color.a == 0 || width <= 0.0 {
        return Ok(());
    }

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

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
    agg_fill_path(pixmap, &mut path, &agg_color, FillingRule::EvenOdd);

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
    _clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

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

    for py in 0..dst_h {
        for px in 0..dst_w {
            let tx = dst_x + px as i32;
            let ty = dst_y + py as i32;
            if tx < 0 || ty < 0 || tx >= pw as i32 || ty >= ph as i32 {
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
