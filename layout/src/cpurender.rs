//! CPU rendering for solver3 DisplayList
//!
//! This module renders a flat DisplayList (from solver3) to a tiny-skia Pixmap.
//! Unlike the old hierarchical CachedDisplayList, the new DisplayList is a simple
//! flat vector of rendering commands that can be executed sequentially.

use azul_core::{
    dom::ScrollbarOrientation,
    geom::{LogicalPosition, LogicalRect},
    resources::{
        DecodedImage, FontInstanceKey, GlyphOutlineOperation, ImageKey, ImageRefHash,
        RendererResources,
    },
    ui_solver::GlyphInstance,
};
use azul_css::props::basic::{ColorU, FontRef};
use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, Pixmap, Rect, Transform};

use crate::{
    font::parsed::ParsedFont,
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem},
    text3::cache::{FontHash, FontManager},
};

pub struct RenderOptions {
    pub width: f32,
    pub height: f32,
    pub dpi_factor: f32,
}

pub fn render(
    dl: &DisplayList,
    res: &RendererResources,
    opts: RenderOptions,
) -> Result<Pixmap, String> {
    let RenderOptions {
        width,
        height,
        dpi_factor,
    } = opts;

    // Create a pixmap with a white background
    let mut pixmap = Pixmap::new((width * dpi_factor) as u32, (height * dpi_factor) as u32)
        .ok_or_else(|| "cannot create pixmap".to_string())?;

    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    // Render the display list to the pixmap
    render_display_list(dl, &mut pixmap, dpi_factor, res, None)?;

    Ok(pixmap)
}

/// Render a display list using fonts from FontManager directly
/// This is used in reftest scenarios where RendererResources doesn't have fonts registered
pub fn render_with_font_manager(
    dl: &DisplayList,
    res: &RendererResources,
    font_manager: &FontManager<FontRef>,
    opts: RenderOptions,
) -> Result<Pixmap, String> {
    let RenderOptions {
        width,
        height,
        dpi_factor,
    } = opts;

    // Create a pixmap with a white background
    let mut pixmap = Pixmap::new((width * dpi_factor) as u32, (height * dpi_factor) as u32)
        .ok_or_else(|| "cannot create pixmap".to_string())?;

    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    // Render the display list to the pixmap using FontManager for fonts
    render_display_list(dl, &mut pixmap, dpi_factor, res, Some(font_manager))?;

    Ok(pixmap)
}

fn render_display_list(
    display_list: &DisplayList,
    pixmap: &mut Pixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
) -> Result<(), String> {
    // The display list is already sorted in paint order, so we just render sequentially
    let mut transform_stack = vec![Transform::identity()];
    let mut clip_stack: Vec<Option<Rect>> = vec![None];

    for item in &display_list.items {
        match item {
            DisplayListItem::Rect {
                bounds,
                color,
                border_radius,
            } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds,
                    *color,
                    border_radius,
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::SelectionRect {
                bounds,
                color,
                border_radius,
            } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds,
                    *color,
                    border_radius,
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::CursorRect { bounds, color } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds,
                    *color,
                    &BorderRadius::default(),
                    *transform,
                    *clip,
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
                // Simplified: Use top border as representative for CPU rendering
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

                // Convert StyleBorderRadius to BorderRadius for rendering
                let simple_radius = BorderRadius {
                    top_left: border_radius
                        .top_left
                        .to_pixels_internal(bounds.size.width, DEFAULT_FONT_SIZE),
                    top_right: border_radius
                        .top_right
                        .to_pixels_internal(bounds.size.width, DEFAULT_FONT_SIZE),
                    bottom_left: border_radius
                        .bottom_left
                        .to_pixels_internal(bounds.size.width, DEFAULT_FONT_SIZE),
                    bottom_right: border_radius
                        .bottom_right
                        .to_pixels_internal(bounds.size.width, DEFAULT_FONT_SIZE),
                };

                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_border(
                    pixmap,
                    bounds,
                    color,
                    width,
                    &simple_radius,
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Underline {
                bounds,
                color,
                thickness,
            } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                // Render underline as a simple filled rectangle
                render_rect(
                    pixmap,
                    bounds,
                    *color,
                    &BorderRadius::default(),
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Strikethrough {
                bounds,
                color,
                thickness,
            } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                // Render strikethrough as a simple filled rectangle
                render_rect(
                    pixmap,
                    bounds,
                    *color,
                    &BorderRadius::default(),
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Overline {
                bounds,
                color,
                thickness,
            } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                // Render overline as a simple filled rectangle
                render_rect(
                    pixmap,
                    bounds,
                    *color,
                    &BorderRadius::default(),
                    *transform,
                    *clip,
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
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_text(
                    glyphs,
                    *font_hash,
                    *font_size_px,
                    *color,
                    pixmap,
                    clip_rect,
                    *transform,
                    *clip,
                    renderer_resources,
                    font_manager,
                    dpi_factor,
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
                // The actual glyphs are rendered via Text items
            }
            DisplayListItem::Image { bounds, key } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_image(
                    pixmap,
                    bounds,
                    *key,
                    *transform,
                    *clip,
                    dpi_factor,
                    renderer_resources,
                )?;
            }
            DisplayListItem::ScrollBar {
                bounds,
                color,
                orientation,
                opacity_key: _, // Ignored in CPU rendering - use color.a directly
                hit_id: _,      // Ignored in CPU rendering - hit testing is done in platform layer
            } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds,
                    *color,
                    &BorderRadius::default(),
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::ScrollBarStyled { info } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                
                // Render track
                if info.track_color.a > 0 {
                    render_rect(
                        pixmap,
                        &info.track_bounds,
                        info.track_color,
                        &BorderRadius::default(),
                        *transform,
                        *clip,
                        dpi_factor,
                    )?;
                }
                
                // Render decrement button
                if let Some(btn_bounds) = &info.button_decrement_bounds {
                    if info.button_color.a > 0 {
                        render_rect(
                            pixmap,
                            btn_bounds,
                            info.button_color,
                            &BorderRadius::default(),
                            *transform,
                            *clip,
                            dpi_factor,
                        )?;
                    }
                }
                
                // Render increment button
                if let Some(btn_bounds) = &info.button_increment_bounds {
                    if info.button_color.a > 0 {
                        render_rect(
                            pixmap,
                            btn_bounds,
                            info.button_color,
                            &BorderRadius::default(),
                            *transform,
                            *clip,
                            dpi_factor,
                        )?;
                    }
                }
                
                // Render thumb
                if info.thumb_color.a > 0 {
                    render_rect(
                        pixmap,
                        &info.thumb_bounds,
                        info.thumb_color,
                        &info.thumb_border_radius,
                        *transform,
                        *clip,
                        dpi_factor,
                    )?;
                }
            }
            DisplayListItem::PushClip {
                bounds,
                border_radius,
            } => {
                let transform = *transform_stack.last().unwrap();
                let new_clip = logical_rect_to_tiny_skia_rect(bounds, transform, dpi_factor);
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
                // For CPU rendering without scroll interaction, we just treat this as a clip
                let transform = *transform_stack.last().unwrap();
                let new_clip = logical_rect_to_tiny_skia_rect(clip_bounds, transform, dpi_factor);
                clip_stack.push(new_clip);
                // Note: We're not handling scroll offset here. In a full implementation,
                // we'd look up the scroll state for scroll_id and apply it as a transform.
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
                // For CPU rendering, we don't need to do anything special for stacking contexts
                // They're already handled by the display list generation order
                // We could push a transform if we wanted to implement transform support
            }
            DisplayListItem::PopStackingContext => {
                // For CPU rendering, no action needed
            }
            DisplayListItem::IFrame {
                child_dom_id,
                bounds,
                clip_rect,
            } => {
                // TODO: Implement IFrame rendering
                // This would require looking up the child display list by child_dom_id
                // and recursively rendering it within the bounds/clip_rect.
                // For now, just render a placeholder rectangle to show where it would be
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds,
                    ColorU {
                        r: 200,
                        g: 200,
                        b: 255,
                        a: 128,
                    }, // Light blue placeholder
                    &BorderRadius::default(),
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }

            // Gradient rendering - simplified for CPU render
            DisplayListItem::LinearGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                // TODO: Implement proper gradient rendering
                // For now, render a placeholder with the first stop color
                let color = gradient
                    .stops
                    .as_ref()
                    .first()
                    .map(|s| s.color)
                    .unwrap_or(ColorU {
                        r: 128,
                        g: 128,
                        b: 128,
                        a: 255,
                    });
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds,
                    color,
                    border_radius,
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::RadialGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                // TODO: Implement proper radial gradient rendering
                let color = gradient
                    .stops
                    .as_ref()
                    .first()
                    .map(|s| s.color)
                    .unwrap_or(ColorU {
                        r: 128,
                        g: 128,
                        b: 128,
                        a: 255,
                    });
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds,
                    color,
                    border_radius,
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::ConicGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                // TODO: Implement proper conic gradient rendering
                let color = gradient
                    .stops
                    .as_ref()
                    .first()
                    .map(|s| s.color)
                    .unwrap_or(ColorU {
                        r: 128,
                        g: 128,
                        b: 128,
                        a: 255,
                    });
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds,
                    color,
                    border_radius,
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }

            // BoxShadow - simplified
            DisplayListItem::BoxShadow {
                bounds,
                shadow,
                border_radius,
            } => {
                // TODO: Implement proper box shadow rendering
                // For now, render a slightly offset rectangle with the shadow color
                let offset_bounds = LogicalRect {
                    origin: LogicalPosition {
                        x: bounds.origin.x + shadow.offset_x.inner.to_pixels_internal(0.0, 16.0),
                        y: bounds.origin.y + shadow.offset_y.inner.to_pixels_internal(0.0, 16.0),
                    },
                    size: bounds.size,
                };
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    &offset_bounds,
                    shadow.color,
                    border_radius,
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }

            // Filter effects - not supported in CPU render
            DisplayListItem::PushFilter { .. } => {
                // TODO: Implement filter effects for CPU rendering
            }
            DisplayListItem::PopFilter => {}
            DisplayListItem::PushBackdropFilter { .. } => {
                // Backdrop filter requires compositing - not supported in CPU render
            }
            DisplayListItem::PopBackdropFilter => {}
            DisplayListItem::PushOpacity { bounds, opacity } => {
                // TODO: Implement opacity layers for CPU rendering
            }
            DisplayListItem::PopOpacity => {}
        }
    }

    Ok(())
}

fn render_rect(
    pixmap: &mut Pixmap,
    bounds: &LogicalRect,
    color: ColorU,
    border_radius: &BorderRadius,
    transform: Transform,
    clip: Option<Rect>,
    dpi_factor: f32,
) -> Result<(), String> {
    if color.a == 0 {
        return Ok(()); // Fully transparent, skip
    }

    let rect = logical_rect_to_tiny_skia_rect(bounds, transform, dpi_factor);
    let rect = match rect {
        Some(r) => r,
        None => return Ok(()), // Invalid rect
    };

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(color.r, color.g, color.b, color.a));
    paint.anti_alias = true;

    // Build path (with border radius if needed)
    let path = if border_radius.is_zero() {
        build_rect_path(rect, border_radius, dpi_factor)
    } else {
        build_rounded_rect_path(rect, border_radius, dpi_factor)
    };

    let path = match path {
        Some(p) => p,
        None => return Ok(()),
    };

    // Apply clipping if needed
    if let Some(clip_rect) = clip {
        // tiny-skia doesn't have native clipping, so we'd need to implement it manually
        // For now, we'll skip clipping support
    }

    pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);

    Ok(())
}

fn render_text(
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    pixmap: &mut Pixmap,
    clip_rect: &LogicalRect,
    transform: Transform,
    _clip: Option<Rect>,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    dpi_factor: f32,
) -> Result<(), String> {
    if color.a == 0 || glyphs.is_empty() {
        return Ok(());
    }

    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);

    // Try to get the parsed font - first from FontManager (for reftests), then from
    // RendererResources
    let parsed_font: &ParsedFont = if let Some(fm) = font_manager {
        // Use FontManager directly (reftest path)
        match fm.get_font_by_hash(font_hash.font_hash) {
            Some(font_ref) => {
                // Get the ParsedFont pointer from FontRef
                unsafe { &*(font_ref.get_parsed() as *const ParsedFont) }
            }
            None => {
                eprintln!(
                    "[cpurender] Font hash {} not found in FontManager",
                    font_hash.font_hash
                );
                return Ok(());
            }
        }
    } else {
        // Use RendererResources (normal rendering path)
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

    // Use the actual font size from the display list (already adjusted for DPI)
    let scale_factor = (font_size_px * dpi_factor) / units_per_em;

    // Draw each glyph
    for glyph in glyphs {
        let glyph_index = glyph.index as u16;

        // glyph.point is the absolute baseline position of the glyph
        let glyph_x = glyph.point.x * dpi_factor;
        let glyph_baseline_y = glyph.point.y * dpi_factor;

        // Find the glyph outline in the parsed font
        if let Some(glyph_data) = parsed_font.glyph_records_decoded.get(&glyph_index) {
            let mut pb = PathBuilder::new();

            for outline in glyph_data.outline.iter() {
                for op in outline.operations.as_ref() {
                    match op {
                        GlyphOutlineOperation::MoveTo(pt) => {
                            // Scale and position the point relative to the glyph's baseline
                            let x = glyph_x + pt.x as f32 * scale_factor;
                            let y = glyph_baseline_y - pt.y as f32 * scale_factor;
                            pb.move_to(x, y);
                        }
                        GlyphOutlineOperation::LineTo(pt) => {
                            let x = glyph_x + pt.x as f32 * scale_factor;
                            let y = glyph_baseline_y - pt.y as f32 * scale_factor;
                            pb.line_to(x, y);
                        }
                        GlyphOutlineOperation::QuadraticCurveTo(qt) => {
                            let ctrl_x = glyph_x + qt.ctrl_1_x as f32 * scale_factor;
                            let ctrl_y = glyph_baseline_y - qt.ctrl_1_y as f32 * scale_factor;
                            let end_x = glyph_x + qt.end_x as f32 * scale_factor;
                            let end_y = glyph_baseline_y - qt.end_y as f32 * scale_factor;
                            pb.quad_to(ctrl_x, ctrl_y, end_x, end_y);
                        }
                        GlyphOutlineOperation::CubicCurveTo(ct) => {
                            let ctrl1_x = glyph_x + ct.ctrl_1_x as f32 * scale_factor;
                            let ctrl1_y = glyph_baseline_y - ct.ctrl_1_y as f32 * scale_factor;
                            let ctrl2_x = glyph_x + ct.ctrl_2_x as f32 * scale_factor;
                            let ctrl2_y = glyph_baseline_y - ct.ctrl_2_y as f32 * scale_factor;
                            let end_x = glyph_x + ct.end_x as f32 * scale_factor;
                            let end_y = glyph_baseline_y - ct.end_y as f32 * scale_factor;
                            pb.cubic_to(ctrl1_x, ctrl1_y, ctrl2_x, ctrl2_y, end_x, end_y);
                        }
                        GlyphOutlineOperation::ClosePath => {
                            pb.close();
                        }
                    }
                }
            }

            if let Some(path) = pb.finish() {
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(), // Already transformed coordinates
                    None,
                );
            }
        }
    }

    Ok(())
}

fn render_border(
    pixmap: &mut Pixmap,
    bounds: &LogicalRect,
    color: ColorU,
    width: f32,
    border_radius: &BorderRadius,
    transform: Transform,
    clip: Option<Rect>,
    dpi_factor: f32,
) -> Result<(), String> {
    if color.a == 0 || width <= 0.0 {
        return Ok(());
    }

    let rect = logical_rect_to_tiny_skia_rect(bounds, transform, dpi_factor);
    let rect = match rect {
        Some(r) => r,
        None => return Ok(()),
    };

    let scaled_width = width * dpi_factor;
    let mut pb = PathBuilder::new();

    // 1. Add Outer Path
    let x = rect.x();
    let y = rect.y();
    let w = rect.width();
    let h = rect.height();

    if border_radius.is_zero() {
        pb.move_to(x, y);
        pb.line_to(x + w, y);
        pb.line_to(x + w, y + h);
        pb.line_to(x, y + h);
        pb.close();
    } else {
        let tl = border_radius.top_left * dpi_factor;
        let tr = border_radius.top_right * dpi_factor;
        let br = border_radius.bottom_right * dpi_factor;
        let bl = border_radius.bottom_left * dpi_factor;

        pb.move_to(x + tl, y);
        pb.line_to(x + w - tr, y);
        if tr > 0.0 {
            pb.quad_to(x + w, y, x + w, y + tr);
        }
        pb.line_to(x + w, y + h - br);
        if br > 0.0 {
            pb.quad_to(x + w, y + h, x + w - br, y + h);
        }
        pb.line_to(x + bl, y + h);
        if bl > 0.0 {
            pb.quad_to(x, y + h, x, y + h - bl);
        }
        pb.line_to(x, y + tl);
        if tl > 0.0 {
            pb.quad_to(x, y, x + tl, y);
        }
        pb.close();
    }

    // 2. Add Inner Path (wound in same direction - EvenOdd fill will create the hole)
    let inner_rect = Rect::from_xywh(
        rect.x() + scaled_width,
        rect.y() + scaled_width,
        rect.width() - 2.0 * scaled_width,
        rect.height() - 2.0 * scaled_width,
    );

    if let Some(ir) = inner_rect {
        let ix = ir.x();
        let iy = ir.y();
        let iw = ir.width();
        let ih = ir.height();

        if border_radius.is_zero() {
            pb.move_to(ix, iy);
            pb.line_to(ix + iw, iy);
            pb.line_to(ix + iw, iy + ih);
            pb.line_to(ix, iy + ih);
            pb.close();
        } else {
            // Inner radius is max(0, outer - width)
            let tl = (border_radius.top_left * dpi_factor - scaled_width).max(0.0);
            let tr = (border_radius.top_right * dpi_factor - scaled_width).max(0.0);
            let br = (border_radius.bottom_right * dpi_factor - scaled_width).max(0.0);
            let bl = (border_radius.bottom_left * dpi_factor - scaled_width).max(0.0);

            pb.move_to(ix + tl, iy);
            pb.line_to(ix + iw - tr, iy);
            if tr > 0.0 {
                pb.quad_to(ix + iw, iy, ix + iw, iy + tr);
            }
            pb.line_to(ix + iw, iy + ih - br);
            if br > 0.0 {
                pb.quad_to(ix + iw, iy + ih, ix + iw - br, iy + ih);
            }
            pb.line_to(ix + bl, iy + ih);
            if bl > 0.0 {
                pb.quad_to(ix, iy + ih, ix, iy + ih - bl);
            }
            pb.line_to(ix, iy + tl);
            if tl > 0.0 {
                pb.quad_to(ix, iy, ix + tl, iy);
            }
            pb.close();
        }
    }

    // 3. Fill with EvenOdd to create the hole (inner path becomes transparent)
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(color.r, color.g, color.b, color.a));
    paint.anti_alias = true;

    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::EvenOdd,
            Transform::identity(),
            None,
        );
    }

    Ok(())
}

fn logical_rect_to_tiny_skia_rect(
    bounds: &LogicalRect,
    transform: Transform,
    dpi_factor: f32,
) -> Option<Rect> {
    let x = bounds.origin.x * dpi_factor;
    let y = bounds.origin.y * dpi_factor;
    let width = bounds.size.width * dpi_factor;
    let height = bounds.size.height * dpi_factor;

    Rect::from_xywh(x, y, width, height)
}

fn render_image(
    pixmap: &mut Pixmap,
    bounds: &LogicalRect,
    key: ImageKey,
    transform: Transform,
    _clip: Option<Rect>,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
) -> Result<(), String> {
    // Look up the image in renderer_resources
    let image_ref_hash = ImageRefHash { inner: key.key as usize };

    let resolved_image = match renderer_resources.get_image(&image_ref_hash) {
        Some(img) => img,
        None => {
            // Image not found - skip rendering
            return Ok(());
        }
    };

    // The image data is stored in renderer_resources, but we need to access it through ImageRef
    // For CPU rendering, we'd need to decode the image data and blit it to the pixmap
    // This is a complex operation that requires image decoding support in tiny-skia

    // For now, render a placeholder rectangle to show where the image would be
    let rect = logical_rect_to_tiny_skia_rect(bounds, transform, dpi_factor);
    let rect = match rect {
        Some(r) => r,
        None => return Ok(()),
    };

    let mut paint = Paint::default();
    // Light gray placeholder for images
    paint.set_color(Color::from_rgba8(200, 200, 200, 255));
    paint.anti_alias = true;

    let path = build_rect_path(rect, &BorderRadius::default(), dpi_factor);
    if let Some(path) = path {
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    // TODO: Implement actual image blitting
    // This would require:
    // 1. Accessing the ImageRef from renderer_resources
    // 2. Getting the decoded image data (DecodedImage::Raw or DecodedImage::Gl)
    // 3. Converting it to a tiny-skia Pixmap
    // 4. Blitting it to the target pixmap with proper scaling

    Ok(())
}

fn build_rect_path(rect: Rect, _border_radius: &BorderRadius, _dpi_factor: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(rect.x(), rect.y());
    pb.line_to(rect.x() + rect.width(), rect.y());
    pb.line_to(rect.x() + rect.width(), rect.y() + rect.height());
    pb.line_to(rect.x(), rect.y() + rect.height());
    pb.close();
    pb.finish()
}

fn build_rounded_rect_path(
    rect: Rect,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> Option<Path> {
    let mut pb = PathBuilder::new();

    let x = rect.x();
    let y = rect.y();
    let width = rect.width();
    let height = rect.height();

    let tl = border_radius.top_left * dpi_factor;
    let tr = border_radius.top_right * dpi_factor;
    let br = border_radius.bottom_right * dpi_factor;
    let bl = border_radius.bottom_left * dpi_factor;

    // Start at top-left corner (after radius)
    pb.move_to(x + tl, y);

    // Top edge
    pb.line_to(x + width - tr, y);

    // Top-right corner
    if tr > 0.0 {
        pb.quad_to(x + width, y, x + width, y + tr);
    }

    // Right edge
    pb.line_to(x + width, y + height - br);

    // Bottom-right corner
    if br > 0.0 {
        pb.quad_to(x + width, y + height, x + width - br, y + height);
    }

    // Bottom edge
    pb.line_to(x + bl, y + height);

    // Bottom-left corner
    if bl > 0.0 {
        pb.quad_to(x, y + height, x, y + height - bl);
    }

    // Left edge
    pb.line_to(x, y + tl);

    // Top-left corner
    if tl > 0.0 {
        pb.quad_to(x, y, x + tl, y);
    }

    pb.close();
    pb.finish()
}
