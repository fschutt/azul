//! CPU rendering for solver3 DisplayList
//!
//! This module renders a flat DisplayList (from solver3) to a tiny-skia Pixmap.
//! Unlike the old hierarchical CachedDisplayList, the new DisplayList is a simple
//! flat vector of rendering commands that can be executed sequentially.

use azul_core::{dom::ScrollbarOrientation, geom::LogicalRect, resources::RendererResources};
use azul_css::props::basic::ColorU;
use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, Pixmap, Rect, Transform};

use crate::{
    font::parsed::ParsedFont,
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem},
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
    render_display_list(dl, &mut pixmap, dpi_factor, res)?;

    Ok(pixmap)
}

fn render_display_list(
    display_list: &DisplayList,
    pixmap: &mut Pixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
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
                color,
                width,
                border_radius,
            } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                render_border(
                    pixmap,
                    bounds,
                    *color,
                    *width,
                    border_radius,
                    *transform,
                    *clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Text {
                glyphs,
                font,
                color,
                clip_rect,
            } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                // TODO: Implement text rendering with tiny-skia
                // This requires loading the font outlines and rendering glyphs
            }
            DisplayListItem::Image { bounds, key } => {
                let transform = transform_stack.last().unwrap();
                let clip = clip_stack.last().unwrap();
                // TODO: Implement image rendering
                // Need to look up the image in renderer_resources and draw it
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

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(color.r, color.g, color.b, color.a));
    paint.anti_alias = true;

    let scaled_width = width * dpi_factor;

    // Build outer path
    let outer_path = if border_radius.is_zero() {
        build_rect_path(rect, border_radius, dpi_factor)
    } else {
        build_rounded_rect_path(rect, border_radius, dpi_factor)
    };

    // Build inner path (shrunk by border width)
    let inner_rect = Rect::from_xywh(
        rect.x() + scaled_width,
        rect.y() + scaled_width,
        rect.width() - 2.0 * scaled_width,
        rect.height() - 2.0 * scaled_width,
    );

    let inner_path = if let Some(ir) = inner_rect {
        if border_radius.is_zero() {
            build_rect_path(rect, border_radius, dpi_factor)
        } else {
            let inner_radius = BorderRadius {
                top_left: (border_radius.top_left * dpi_factor - scaled_width).max(0.0),
                top_right: (border_radius.top_right * dpi_factor - scaled_width).max(0.0),
                bottom_left: (border_radius.bottom_left * dpi_factor - scaled_width).max(0.0),
                bottom_right: (border_radius.bottom_right * dpi_factor - scaled_width).max(0.0),
            };
            build_rounded_rect_path(ir, &inner_radius, 1.0) // dpi already applied
        }
    } else {
        return Ok(()); // Border too thick for rect
    };

    // Render outer path
    if let Some(op) = outer_path {
        pixmap.fill_path(&op, &paint, FillRule::Winding, Transform::identity(), None);
    }

    // "Erase" inner path by drawing it in the background color
    // Note: This is a simplification. A proper implementation would use path subtraction.
    if let Some(ip) = inner_path {
        let mut bg_paint = Paint::default();
        bg_paint.set_color(Color::from_rgba8(255, 255, 255, 255)); // White background
        bg_paint.anti_alias = true;
        pixmap.fill_path(
            &ip,
            &bg_paint,
            FillRule::Winding,
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
