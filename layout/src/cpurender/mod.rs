use azul_core::{
    app_resources::{GlyphOutlineOperation, RendererResources},
    display_list::{
        CachedDisplayList, DisplayListFrame, DisplayListMsg, DisplayListScrollFrame,
        LayoutRectContent, RectBackground, StyleBorderColors, StyleBorderStyles, StyleBorderWidths,
    },
};
use azul_css::{BorderStyle, ColorU};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

use crate::parsedfont::ParsedFont;

pub struct RenderOptions {
    pub width: f32,
    pub height: f32,
    pub dpi_factor: f32,
}

pub fn render(
    dl: &CachedDisplayList,
    res: &RendererResources,
    opts: RenderOptions,
) -> Result<Pixmap, String> {
    let RenderOptions {
        width,
        height,
        dpi_factor,
    } = opts;

    // Create a pixmap with a white background
    let mut pixmap = Pixmap::new(
        (width as f32 * dpi_factor) as u32,
        (height as f32 * dpi_factor) as u32,
    )
    .ok_or_else(|| format!("cannot create pixmap"))?;

    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    // Render the display list to the pixmap
    render_display_list(&dl, &mut pixmap, &res)?;

    Ok(pixmap)
}

fn render_display_list(
    display_list: &CachedDisplayList,
    pixmap: &mut Pixmap,
    renderer_resources: &RendererResources,
) -> Result<(), String> {
    // Start with root position and identity transform
    let transform = Transform::identity();

    match &display_list.root {
        DisplayListMsg::Frame(frame) => {
            render_frame(frame, pixmap, transform, None, renderer_resources)?;
        }
        DisplayListMsg::ScrollFrame(scroll_frame) => {
            render_scroll_frame(scroll_frame, pixmap, transform, renderer_resources)?;
        }
        DisplayListMsg::IFrame(_, _, _, cached_dl) => {
            render_display_list(cached_dl, pixmap, renderer_resources)?;
        }
    }

    Ok(())
}

fn render_frame(
    frame: &DisplayListFrame,
    pixmap: &mut Pixmap,
    transform: Transform,
    clip_rect: Option<Rect>,
    renderer_resources: &RendererResources,
) -> Result<(), String> {
    // Calculate the frame rectangle
    let frame_rect = match Rect::from_xywh(0.0, 0.0, frame.size.width, frame.size.height) {
        Some(rect) => rect,
        None => return Ok(()), // Invalid rect dimensions
    };

    // Render the frame content
    for content in &frame.content {
        render_content(
            content,
            pixmap,
            frame_rect,
            transform,
            clip_rect,
            renderer_resources,
        )?;
    }

    // Handle box shadow if any
    if let Some(box_shadow) = &frame.box_shadow {
        // Box shadow rendering would go here in a full implementation
    }

    // Render children
    for child in &frame.children {
        let child_pos = child.get_position();
        let rel_offset = child_pos.get_relative_offset();
        let offset_x = rel_offset.x;
        let offset_y = rel_offset.y;

        // Apply transform based on child position
        let child_transform = transform.pre_translate(offset_x, offset_y);

        match child {
            DisplayListMsg::Frame(child_frame) => {
                render_frame(
                    child_frame,
                    pixmap,
                    child_transform,
                    clip_rect,
                    renderer_resources,
                )?;
            }
            DisplayListMsg::ScrollFrame(scroll_frame) => {
                render_scroll_frame(scroll_frame, pixmap, child_transform, renderer_resources)?;
            }
            DisplayListMsg::IFrame(_, iframe_size, _, cached_dl) => {
                // Create a clip rect for the iframe
                let iframe_rect = match Rect::from_xywh(
                    offset_x,
                    offset_y,
                    iframe_size.width,
                    iframe_size.height,
                ) {
                    Some(rect) => rect,
                    None => continue,
                };

                // Recursively render the iframe with clipping
                render_display_list(cached_dl, pixmap, renderer_resources)?;
            }
        }
    }

    Ok(())
}

fn render_scroll_frame(
    scroll_frame: &DisplayListScrollFrame,
    pixmap: &mut Pixmap,
    transform: Transform,
    renderer_resources: &RendererResources,
) -> Result<(), String> {
    // Calculate scroll frame clip rectangle
    let clip_rect = match Rect::from_xywh(
        0.0,
        0.0,
        scroll_frame.parent_rect.size.width,
        scroll_frame.parent_rect.size.height,
    ) {
        Some(rect) => rect,
        None => return Ok(()), // Invalid rect dimensions
    };

    // Apply scroll offset
    let scroll_transform = transform.pre_translate(
        scroll_frame.content_rect.origin.x - scroll_frame.parent_rect.origin.x,
        scroll_frame.content_rect.origin.y - scroll_frame.parent_rect.origin.y,
    );

    // Render the frame with clipping
    render_frame(
        &scroll_frame.frame,
        pixmap,
        scroll_transform,
        Some(clip_rect),
        renderer_resources,
    )?;

    Ok(())
}

fn render_content(
    content: &LayoutRectContent,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    clip_rect: Option<Rect>,
    renderer_resources: &RendererResources,
) -> Result<(), String> {
    match content {
        LayoutRectContent::Background {
            content,
            size,
            offset,
            repeat,
        } => {
            render_background(
                content, *size, *offset, *repeat, pixmap, rect, transform, clip_rect,
            )?;
        }
        LayoutRectContent::Border {
            widths,
            colors,
            styles,
        } => {
            render_border(
                *widths, *colors, *styles, pixmap, rect, transform, clip_rect,
            )?;
        }
        LayoutRectContent::Text {
            glyphs,
            font_instance_key,
            color,
            glyph_options,
            overflow,
            text_shadow,
        } => {
            render_text(
                glyphs,
                *font_instance_key,
                *color,
                pixmap,
                rect,
                transform,
                clip_rect,
                renderer_resources,
            )?;
        }
        LayoutRectContent::Image {
            size,
            offset,
            image_rendering,
            alpha_type,
            image_key,
            background_color,
        } => {
            render_image(
                *size,
                *offset,
                *image_key,
                *background_color,
                pixmap,
                rect,
                transform,
                clip_rect,
            )?;
        }
    }

    Ok(())
}

fn render_background(
    content: &RectBackground,
    size: Option<azul_css::StyleBackgroundSize>,
    offset: Option<azul_css::StyleBackgroundPosition>,
    repeat: Option<azul_css::StyleBackgroundRepeat>,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    clip_rect: Option<Rect>,
) -> Result<(), String> {
    let mut paint = Paint::default();

    match content {
        RectBackground::Color(color) => {
            paint.set_color_rgba8(color.r, color.g, color.b, color.a);

            // Calculate background rectangle based on size and offset
            let bg_rect = calculate_background_rect(rect, size, offset);

            if let Some(bg_rect) = bg_rect {
                // Apply transforms and draw
                draw_rect_with_clip(pixmap, bg_rect, &paint, transform, clip_rect)?;
            }
        }
        RectBackground::LinearGradient(gradient) => {
            // Basic linear gradient rendering (simplified)
            if gradient.stops.as_slice().len() >= 2 {
                paint.set_color_rgba8(
                    gradient.stops.as_slice()[0].color.r,
                    gradient.stops.as_slice()[0].color.g,
                    gradient.stops.as_slice()[0].color.b,
                    gradient.stops.as_slice()[0].color.a,
                );

                let bg_rect = calculate_background_rect(rect, size, offset);
                if let Some(bg_rect) = bg_rect {
                    draw_rect_with_clip(pixmap, bg_rect, &paint, transform, clip_rect)?;
                }
            }
        }
        // For other background types, implement similar rendering logic
        _ => {
            // Default: draw a semi-transparent gray background as placeholder
            paint.set_color_rgba8(200, 200, 200, 100);
            draw_rect_with_clip(pixmap, rect, &paint, transform, clip_rect)?;
        }
    }

    Ok(())
}

fn calculate_background_rect(
    rect: Rect,
    size: Option<azul_css::StyleBackgroundSize>,
    offset: Option<azul_css::StyleBackgroundPosition>,
) -> Option<Rect> {
    // Default: use the entire rect
    let (width, height) = (rect.width(), rect.height());

    // Calculate size if specified
    let (bg_width, bg_height) = match size {
        Some(azul_css::StyleBackgroundSize::ExactSize([w, h])) => {
            let width_px = w.to_pixels(width) as f32;
            let height_px = h.to_pixels(height) as f32;
            (width_px, height_px)
        }
        Some(azul_css::StyleBackgroundSize::Contain) => {
            // Simplified contain logic - not fully implemented
            (width, height)
        }
        Some(azul_css::StyleBackgroundSize::Cover) => {
            // Simplified cover logic - not fully implemented
            (width, height)
        }
        None => (width, height),
    };

    // Calculate position if specified
    let (x_offset, y_offset) = match offset {
        Some(pos) => {
            // Simple horizontal position
            let x = match pos.horizontal {
                azul_css::BackgroundPositionHorizontal::Left => 0.0,
                azul_css::BackgroundPositionHorizontal::Center => (width - bg_width) / 2.0,
                azul_css::BackgroundPositionHorizontal::Right => width - bg_width,
                azul_css::BackgroundPositionHorizontal::Exact(val) => val.to_pixels(width) as f32,
            };

            // Simple vertical position
            let y = match pos.vertical {
                azul_css::BackgroundPositionVertical::Top => 0.0,
                azul_css::BackgroundPositionVertical::Center => (height - bg_height) / 2.0,
                azul_css::BackgroundPositionVertical::Bottom => height - bg_height,
                azul_css::BackgroundPositionVertical::Exact(val) => val.to_pixels(height) as f32,
            };

            (x, y)
        }
        None => (0.0, 0.0),
    };

    Rect::from_xywh(
        rect.x() + x_offset,
        rect.y() + y_offset,
        bg_width,
        bg_height,
    )
}

/// Translates a CSS border style to a StrokeDash pattern
fn translate_dash(style: &BorderStyle) -> Option<Vec<f32>> {
    match style {
        BorderStyle::None | BorderStyle::Hidden => None,
        BorderStyle::Solid => None, // No dash pattern for solid lines
        BorderStyle::Dotted => {
            // Dotted pattern: small on, small off
            Some(vec![1.0, 1.0])
        }
        BorderStyle::Dashed => {
            // Dashed pattern: longer on, small off
            Some(vec![3.0, 3.0])
        }
        // For these complex styles, we'll use solid lines as a fallback
        BorderStyle::Double
        | BorderStyle::Groove
        | BorderStyle::Ridge
        | BorderStyle::Inset
        | BorderStyle::Outset => None,
    }
}

fn render_border(
    widths: StyleBorderWidths,
    colors: StyleBorderColors,
    styles: StyleBorderStyles,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    _clip_rect: Option<Rect>,
) -> Result<(), String> {
    // Helper function to create a rounded corner path
    fn add_rounded_corner(
        pb: &mut PathBuilder,
        cx: f32,
        cy: f32,
        radius: f32,
        start_angle: f32,
        sweep_angle: f32,
    ) {
        if radius <= 0.0 {
            pb.line_to(cx, cy);
            return;
        }

        // Convert angles to radians
        let start_rad = start_angle * std::f32::consts::PI / 180.0;
        let end_rad = (start_angle + sweep_angle) * std::f32::consts::PI / 180.0;

        // Approximate a quarter circle with a cubic Bezier curve
        let kappa = 0.5522847498; // Magic constant for approximating a circle with cubics
        let control_dist = radius * kappa;

        let start_x = cx + radius * start_rad.cos();
        let start_y = cy + radius * start_rad.sin();

        let end_x = cx + radius * end_rad.cos();
        let end_y = cy + radius * end_rad.sin();

        // Calculate control points
        let ctrl1_x = start_x - control_dist * start_rad.sin();
        let ctrl1_y = start_y + control_dist * start_rad.cos();

        let ctrl2_x = end_x + control_dist * end_rad.sin();
        let ctrl2_y = end_y - control_dist * end_rad.cos();

        pb.line_to(start_x, start_y);
        pb.cubic_to(ctrl1_x, ctrl1_y, ctrl2_x, ctrl2_y, end_x, end_y);
    }

    // Helper function to render a border segment
    fn render_border_segment(
        width: f32,
        color: ColorU,
        style: BorderStyle,
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
        pixmap: &mut Pixmap,
        transform: Transform,
    ) -> Result<(), String> {
        if width <= 0.0 {
            return Ok(());
        }

        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);

        let mut pb = PathBuilder::new();
        pb.move_to(start_x, start_y);
        pb.line_to(end_x, end_y);

        if let Some(path) = pb.finish() {
            let transformed_path = path
                .transform(transform)
                .ok_or_else(|| "Failed to transform path".to_string())?;

            // Create stroke options with or without dash pattern
            let dash = translate_dash(&style);

            let stroke = tiny_skia::Stroke {
                width,
                miter_limit: 4.0,
                line_cap: tiny_skia::LineCap::Butt,
                line_join: tiny_skia::LineJoin::Miter,
                dash: dash.and_then(|sd| tiny_skia::StrokeDash::new(sd, 0.0)),
            };

            pixmap.stroke_path(
                &transformed_path,
                &paint,
                &stroke,
                Transform::identity(),
                None,
            );
        }

        Ok(())
    }

    // Helper to get border radius for a corner (top-left, top-right, etc.)
    // We should extract this from CSS properties, but for this example we'll use a simple approach
    let border_radius = 0.0; // Default to no radius

    // Get border widths
    let top_width = widths
        .top
        .and_then(|w| w.get_property().cloned())
        .map(|w| w.inner.to_pixels(rect.height()))
        .unwrap_or(0.0);

    let right_width = widths
        .right
        .and_then(|w| w.get_property().cloned())
        .map(|w| w.inner.to_pixels(rect.width()))
        .unwrap_or(0.0);

    let bottom_width = widths
        .bottom
        .and_then(|w| w.get_property().cloned())
        .map(|w| w.inner.to_pixels(rect.height()))
        .unwrap_or(0.0);

    let left_width = widths
        .left
        .and_then(|w| w.get_property().cloned())
        .map(|w| w.inner.to_pixels(rect.width()))
        .unwrap_or(0.0);

    // Get border styles
    let top_style = styles
        .top
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::BorderStyle::Solid);

    let right_style = styles
        .right
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::BorderStyle::Solid);

    let bottom_style = styles
        .bottom
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::BorderStyle::Solid);

    let left_style = styles
        .left
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::BorderStyle::Solid);

    let top_color = colors
        .top
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::ColorU::BLACK);

    let left_color = colors
        .left
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::ColorU::BLACK);

    let right_color = colors
        .right
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::ColorU::BLACK);

    let bottom_color = colors
        .bottom
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::ColorU::BLACK);

    // Render all four borders using our helper function
    // Top border
    render_border_segment(
        top_width,
        top_color,
        top_style,
        rect.x() + border_radius,
        rect.y() + top_width / 2.0,
        rect.x() + rect.width() - border_radius,
        rect.y() + top_width / 2.0,
        pixmap,
        transform,
    )?;

    // Right border
    render_border_segment(
        right_width,
        right_color,
        right_style,
        rect.x() + rect.width() - right_width / 2.0,
        rect.y() + border_radius,
        rect.x() + rect.width() - right_width / 2.0,
        rect.y() + rect.height() - border_radius,
        pixmap,
        transform,
    )?;

    // Bottom border
    render_border_segment(
        bottom_width,
        bottom_color,
        bottom_style,
        rect.x() + rect.width() - border_radius,
        rect.y() + rect.height() - bottom_width / 2.0,
        rect.x() + border_radius,
        rect.y() + rect.height() - bottom_width / 2.0,
        pixmap,
        transform,
    )?;

    // Left border
    render_border_segment(
        left_width,
        left_color,
        left_style,
        rect.x() + left_width / 2.0,
        rect.y() + rect.height() - border_radius,
        rect.x() + left_width / 2.0,
        rect.y() + border_radius,
        pixmap,
        transform,
    )?;

    Ok(())
}

fn render_text(
    glyphs: &[azul_core::display_list::GlyphInstance],
    font_instance_key: azul_core::app_resources::FontInstanceKey,
    color: azul_css::ColorU,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    _clip_rect: Option<Rect>,
    renderer_resources: &RendererResources,
) -> Result<(), String> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);

    // Find the font and font size from the font_instance_key
    let font_instance = renderer_resources.get_renderable_font_data(&font_instance_key);

    if let Some((font_ref, au, dpi)) = font_instance {
        // Get the parsed font data
        let font_data = font_ref.get_data();
        let parsed_font = unsafe { &*(font_data.parsed as *const ParsedFont) };
        let units_per_em = parsed_font.font_metrics.units_per_em as f32;

        // Calculate font scale factor
        let font_size_px = au.into_px() * dpi.inner.get();
        let scale_factor = font_size_px / units_per_em;

        // Calculate baseline position (normally this would come from the font metrics)
        let baseline_y = rect.y() + parsed_font.font_metrics.ascender as f32 * scale_factor;

        // Draw each glyph
        for glyph in glyphs {
            let glyph_index = glyph.index as u16;

            // Find the glyph outline in the parsed font
            if let Some(glyph_data) = parsed_font.glyph_records_decoded.get(&glyph_index) {
                let mut pb = PathBuilder::new();

                for outline in glyph_data.outline.iter() {
                    // Create path from outline
                    let mut is_first = true;

                    for op in outline.operations.as_ref() {
                        match op {
                            GlyphOutlineOperation::MoveTo(pt) => {
                                // Scale and position the point
                                let x = rect.x() + glyph.point.x + pt.x as f32 * scale_factor;
                                let y = baseline_y - pt.y as f32 * scale_factor;

                                if is_first {
                                    pb.move_to(x, y);
                                    is_first = false;
                                } else {
                                    pb.move_to(x, y);
                                }
                            }
                            GlyphOutlineOperation::LineTo(pt) => {
                                let x = rect.x() + glyph.point.x + pt.x as f32 * scale_factor;
                                let y = baseline_y - pt.y as f32 * scale_factor;
                                pb.line_to(x, y);
                            }
                            GlyphOutlineOperation::QuadraticCurveTo(qt) => {
                                let ctrl_x =
                                    rect.x() + glyph.point.x + qt.ctrl_1_x as f32 * scale_factor;
                                let ctrl_y = baseline_y - qt.ctrl_1_y as f32 * scale_factor;
                                let end_x =
                                    rect.x() + glyph.point.x + qt.end_x as f32 * scale_factor;
                                let end_y = baseline_y - qt.end_y as f32 * scale_factor;
                                pb.quad_to(ctrl_x, ctrl_y, end_x, end_y);
                            }
                            GlyphOutlineOperation::CubicCurveTo(ct) => {
                                let ctrl1_x =
                                    rect.x() + glyph.point.x + ct.ctrl_1_x as f32 * scale_factor;
                                let ctrl1_y = baseline_y - ct.ctrl_1_y as f32 * scale_factor;
                                let ctrl2_x =
                                    rect.x() + glyph.point.x + ct.ctrl_2_x as f32 * scale_factor;
                                let ctrl2_y = baseline_y - ct.ctrl_2_y as f32 * scale_factor;
                                let end_x =
                                    rect.x() + glyph.point.x + ct.end_x as f32 * scale_factor;
                                let end_y = baseline_y - ct.end_y as f32 * scale_factor;
                                pb.cubic_to(ctrl1_x, ctrl1_y, ctrl2_x, ctrl2_y, end_x, end_y);
                            }
                            GlyphOutlineOperation::ClosePath => {
                                pb.close();
                            }
                        }
                    }
                }

                if let Some(path) = pb.finish() {
                    let transformed_path = path
                        .transform(transform)
                        .ok_or_else(|| "Failed to transform text path".to_string())?;
                    pixmap.fill_path(
                        &transformed_path,
                        &paint,
                        tiny_skia::FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
    } else {
        // Fallback: just draw a simple line for text baseline
        if let Some(text_rect) =
            Rect::from_xywh(rect.x(), rect.y() + rect.height() * 0.75, rect.width(), 1.0)
        {
            let mut pb = PathBuilder::new();
            if let Some(text_rect2) = Rect::from_xywh(
                text_rect.x(),
                text_rect.y(),
                text_rect.width(),
                text_rect.height(),
            ) {
                pb.push_rect(text_rect2);
            }

            if let Some(path) = pb.finish() {
                let transformed_path = path
                    .transform(transform)
                    .ok_or_else(|| "Failed to transform text path".to_string())?;
                pixmap.fill_path(
                    &transformed_path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    Ok(())
}

fn render_image(
    size: azul_core::window::LogicalSize,
    offset: azul_core::window::LogicalPosition,
    image_key: azul_core::app_resources::ImageKey,
    bg_color: azul_css::ColorU,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    clip_rect: Option<Rect>,
) -> Result<(), String> {
    // Simplified image rendering - just draws a colored rectangle with a border
    let img_rect = match Rect::from_xywh(
        rect.x() + offset.x,
        rect.y() + offset.y,
        size.width,
        size.height,
    ) {
        Some(r) => r,
        None => return Ok(()),
    };

    // Draw background color
    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(bg_color.r, bg_color.g, bg_color.b, bg_color.a);
    draw_rect_with_clip(pixmap, img_rect, &bg_paint, transform, clip_rect)?;

    // Draw border to indicate it's an image
    let mut border_paint = Paint::default();
    border_paint.set_color_rgba8(100, 100, 100, 200);

    // Create a path for the border
    let mut pb = PathBuilder::new();
    pb.move_to(img_rect.x(), img_rect.y());
    pb.line_to(img_rect.x() + img_rect.width(), img_rect.y());
    pb.line_to(
        img_rect.x() + img_rect.width(),
        img_rect.y() + img_rect.height(),
    );
    pb.line_to(img_rect.x(), img_rect.y() + img_rect.height());
    pb.close();

    if let Some(path) = pb.finish() {
        // Apply transform
        let transformed_path = path
            .transform(transform)
            .ok_or_else(|| format!("cannot transform path"))?;

        // Apply clipping
        if let Some(clip) = clip_rect {
            let mut mask = tiny_skia::Mask::new(pixmap.width(), pixmap.height())
                .ok_or_else(|| format!("cannot create clip maps {clip:?}"))?;

            // Create clip path
            let mut clip_pb = PathBuilder::new();
            clip_pb.move_to(clip.x(), clip.y());
            clip_pb.line_to(clip.x() + clip.width(), clip.y());
            clip_pb.line_to(clip.x() + clip.width(), clip.y() + clip.height());
            clip_pb.line_to(clip.x(), clip.y() + clip.height());
            clip_pb.close();

            if let Some(clip_path) = clip_pb.finish() {
                mask.fill_path(&clip_path, FillRule::Winding, true, Transform::identity());
                pixmap.stroke_path(
                    &transformed_path,
                    &border_paint,
                    &tiny_skia::Stroke::default(),
                    Transform::identity(),
                    Some(&mask),
                );
            }
        } else {
            pixmap.stroke_path(
                &transformed_path,
                &border_paint,
                &tiny_skia::Stroke::default(),
                Transform::identity(),
                None,
            );
        }
    }

    Ok(())
}

fn draw_rect_with_clip(
    pixmap: &mut Pixmap,
    rect: Rect,
    paint: &Paint,
    transform: Transform,
    clip_rect: Option<Rect>,
) -> Result<(), String> {
    // Create a path for the rectangle
    let mut pb = PathBuilder::new();
    pb.move_to(rect.x(), rect.y());
    pb.line_to(rect.x() + rect.width(), rect.y());
    pb.line_to(rect.x() + rect.width(), rect.y() + rect.height());
    pb.line_to(rect.x(), rect.y() + rect.height());
    pb.close();

    if let Some(path) = pb.finish() {
        // Apply transform
        let transformed_path = path
            .transform(transform)
            .ok_or_else(|| format!("cannot draw rect with transformed clip"))?;

        // Apply clipping
        if let Some(clip) = clip_rect {
            let mut mask = tiny_skia::Mask::new(pixmap.width(), pixmap.height())
                .ok_or_else(|| format!("cannot draw rect with transformed clip {clip:?}"))?;

            // Create clip path
            let mut clip_pb = PathBuilder::new();
            clip_pb.move_to(clip.x(), clip.y());
            clip_pb.line_to(clip.x() + clip.width(), clip.y());
            clip_pb.line_to(clip.x() + clip.width(), clip.y() + clip.height());
            clip_pb.line_to(clip.x(), clip.y() + clip.height());
            clip_pb.close();

            if let Some(clip_path) = clip_pb.finish() {
                mask.fill_path(&clip_path, FillRule::Winding, true, Transform::identity());
                pixmap.fill_path(
                    &transformed_path,
                    paint,
                    FillRule::Winding,
                    Transform::identity(),
                    Some(&mask),
                );
            }
        } else {
            pixmap.fill_path(
                &transformed_path,
                paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    Ok(())
}
