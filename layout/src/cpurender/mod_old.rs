use azul_core::{
    app_resources::{GlyphOutlineOperation, RendererResources},
    display_list::{
        CachedDisplayList, DisplayListFrame, DisplayListMsg, DisplayListScrollFrame,
        LayoutRectContent, RectBackground, StyleBorderColors, StyleBorderStyles, StyleBorderWidths,
    },
};
use azul_css::props::{
    basic::ColorU,
    style::{
        BackgroundPositionHorizontal, BackgroundPositionVertical, BorderStyle,
        StyleBackgroundPosition, StyleBackgroundRepeat, StyleBackgroundSize,
    },
};
use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, Pixmap, Rect, Transform};

use crate::font::parsed::ParsedFont;

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

        // Determine the transformation matrix based on the child's positioning scheme
        let child_transform = match child_pos {
            azul_core::ui_solver::PositionInfo::Fixed(_) => {
                // For FIXED elements, the transform is ALWAYS relative to the viewport (root).
                // We ignore the parent's `transform` and create a new one from the
                // absolute coordinates calculated by the layout engine.
                let static_offset = child_pos.get_static_offset();
                Transform::from_translate(static_offset.x, static_offset.y)
            }
            _ => {
                // For all other elements (static, relative, absolute), the existing
                // logic is sufficient for a hierarchical renderer. Position them
                // relative to their parent's transform.
                let rel_offset = child_pos.get_relative_offset();
                transform.pre_translate(rel_offset.x, rel_offset.y)
            }
        };

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
    size: Option<StyleBackgroundSize>,
    offset: Option<StyleBackgroundPosition>,
    repeat: Option<StyleBackgroundRepeat>,
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
    size: Option<StyleBackgroundSize>,
    offset: Option<StyleBackgroundPosition>,
) -> Option<Rect> {
    // Default: use the entire rect
    let (width, height) = (rect.width(), rect.height());

    // Calculate size if specified
    let (bg_width, bg_height) = match size {
        Some(StyleBackgroundSize::ExactSize([w, h])) => {
            let width_px = w.to_pixels(width) as f32;
            let height_px = h.to_pixels(height) as f32;
            (width_px, height_px)
        }
        Some(StyleBackgroundSize::Contain) => {
            // Simplified contain logic - not fully implemented
            (width, height)
        }
        Some(StyleBackgroundSize::Cover) => {
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
                BackgroundPositionHorizontal::Left => 0.0,
                BackgroundPositionHorizontal::Center => (width - bg_width) / 2.0,
                BackgroundPositionHorizontal::Right => width - bg_width,
                BackgroundPositionHorizontal::Exact(val) => val.to_pixels(width) as f32,
            };

            // Simple vertical position
            let y = match pos.vertical {
                BackgroundPositionVertical::Top => 0.0,
                BackgroundPositionVertical::Center => (height - bg_height) / 2.0,
                BackgroundPositionVertical::Bottom => height - bg_height,
                BackgroundPositionVertical::Exact(val) => val.to_pixels(height) as f32,
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
            // Dotted pattern: small on, small off. Stroke width is a good measure.
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

/// Helper function to build a rounded rectangle path.
/// Can draw clockwise or counter-clockwise.
fn build_rounded_rect_path(
    rect: Rect,
    r_tl: f32,
    r_tr: f32,
    r_br: f32,
    r_bl: f32,
    clockwise: bool,
) -> Option<Path> {
    let mut pb = PathBuilder::new();

    if clockwise {
        pb.move_to(rect.left() + r_tl, rect.top());
        pb.line_to(rect.right() - r_tr, rect.top());
        if r_tr > 0.0 {
            pb.quad_to(rect.right(), rect.top(), rect.right(), rect.top() + r_tr);
        }
        pb.line_to(rect.right(), rect.bottom() - r_br);
        if r_br > 0.0 {
            pb.quad_to(
                rect.right(),
                rect.bottom(),
                rect.right() - r_br,
                rect.bottom(),
            );
        }
        pb.line_to(rect.left() + r_bl, rect.bottom());
        if r_bl > 0.0 {
            pb.quad_to(
                rect.left(),
                rect.bottom(),
                rect.left(),
                rect.bottom() - r_bl,
            );
        }
        pb.line_to(rect.left(), rect.top() + r_tl);
        if r_tl > 0.0 {
            pb.quad_to(rect.left(), rect.top(), rect.left() + r_tl, rect.top());
        }
    } else {
        // Counter-clockwise
        pb.move_to(rect.left() + r_tl, rect.top());
        if r_tl > 0.0 {
            pb.quad_to(rect.left(), rect.top(), rect.left(), rect.top() + r_tl);
        }
        pb.line_to(rect.left(), rect.bottom() - r_bl);
        if r_bl > 0.0 {
            pb.quad_to(
                rect.left(),
                rect.bottom(),
                rect.left() + r_bl,
                rect.bottom(),
            );
        }
        pb.line_to(rect.right() - r_br, rect.bottom());
        if r_br > 0.0 {
            pb.quad_to(
                rect.right(),
                rect.bottom(),
                rect.right(),
                rect.bottom() - r_br,
            );
        }
        pb.line_to(rect.right(), rect.top() + r_tr);
        if r_tr > 0.0 {
            pb.quad_to(rect.right(), rect.top(), rect.right() - r_tr, rect.top());
        }
        pb.line_to(rect.left() + r_tl, rect.top());
    }

    pb.close();
    pb.finish()
}

fn render_border(
    widths: StyleBorderWidths,
    colors: StyleBorderColors,
    styles: StyleBorderStyles,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    _clip_rect: Option<Rect>, // The clip rect is handled by the caller's transform and mask.
) -> Result<(), String> {
    // 1. Extract border properties (widths, colors, styles)
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

    if top_width <= 0.0 && right_width <= 0.0 && bottom_width <= 0.0 && left_width <= 0.0 {
        return Ok(());
    }

    let top_color = colors
        .top
        .and_then(|c| c.get_property().cloned())
        .map(|c| c.inner)
        .unwrap_or_else(|| ColorU::BLACK);
    let right_color = colors
        .right
        .and_then(|c| c.get_property().cloned())
        .map(|c| c.inner)
        .unwrap_or_else(|| ColorU::BLACK);
    let bottom_color = colors
        .bottom
        .and_then(|c| c.get_property().cloned())
        .map(|c| c.inner)
        .unwrap_or_else(|| ColorU::BLACK);
    let left_color = colors
        .left
        .and_then(|c| c.get_property().cloned())
        .map(|c| c.inner)
        .unwrap_or_else(|| ColorU::BLACK);

    let top_style = styles
        .top
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or(BorderStyle::Solid);
    let right_style = styles
        .right
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or(BorderStyle::Solid);
    let bottom_style = styles
        .bottom
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or(BorderStyle::Solid);
    let left_style = styles
        .left
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or(BorderStyle::Solid);

    // TODO: Extract border radius from the style properties.
    // For this example, we'll use a hardcoded value. In a real implementation,
    // this would come from `frame.border_radius` or a similar field.
    let (r_tl, r_tr, r_br, r_bl) = (5.0, 5.0, 5.0, 5.0);

    // 2. Create a clipping path that defines the entire border area.
    // This is done by creating an outer rounded rectangle and subtracting an inner one.
    let outer_rect = rect;
    let inner_rect = Rect::from_xywh(
        rect.x() + left_width,
        rect.y() + top_width,
        (rect.width() - left_width - right_width).max(0.0),
        (rect.height() - top_width - bottom_width).max(0.0),
    )
    .unwrap_or_else(|| Rect::from_xywh(0.0, 0.0, 0.0, 0.0).unwrap());

    // Heuristic for inner radii: `outer_radius - average_adjacent_border_width`.
    // This is an approximation but works well visually. The CSS spec is more complex.
    let r_tl_inner = (r_tl - (left_width + top_width) / 2.0).max(0.0);
    let r_tr_inner = (r_tr - (right_width + top_width) / 2.0).max(0.0);
    let r_br_inner = (r_br - (right_width + bottom_width) / 2.0).max(0.0);
    let r_bl_inner = (r_bl - (left_width + bottom_width) / 2.0).max(0.0);

    let outer_path = build_rounded_rect_path(outer_rect, r_tl, r_tr, r_br, r_bl, true)
        .ok_or("Failed to build outer border path")?;
    let inner_path = build_rounded_rect_path(
        inner_rect, r_tl_inner, r_tr_inner, r_br_inner, r_bl_inner,
        false, // Draw counter-clockwise for subtraction
    )
    .ok_or("Failed to build inner border path")?;

    let mut clip_pb = PathBuilder::new();
    clip_pb.push_path(&outer_path);
    clip_pb.push_path(&inner_path);
    let clip_path = clip_pb
        .finish()
        .ok_or("Failed to build combined clip path")?;

    // 3. Create a mask from the clipping path.
    let mut mask = tiny_skia::Mask::new(pixmap.width(), pixmap.height())
        .ok_or("Cannot create border clip mask")?;
    mask.fill_path(
        &clip_path,
        FillRule::Winding, // Winding rule works because inner path is reversed
        true,
        transform,
    );

    // 4. Render each border side as a thick, clipped line.
    let sides = [
        (top_width, top_color, top_style, 'T'),
        (right_width, right_color, right_style, 'R'),
        (bottom_width, bottom_color, bottom_style, 'B'),
        (left_width, left_color, left_style, 'L'),
    ];

    let max_border_width = top_width.max(right_width).max(bottom_width).max(left_width);

    for (width, color, style, side) in &sides {
        if *width <= 0.0 {
            continue;
        }

        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = true;

        // Create a path for the centerline of the border segment.
        // We extend it slightly to ensure it covers the corners before clipping.
        let mut pb = PathBuilder::new();
        match side {
            'T' => {
                pb.move_to(rect.left() - 1.0, rect.top() + width / 2.0);
                pb.line_to(rect.right() + 1.0, rect.top() + width / 2.0);
            }
            'R' => {
                pb.move_to(rect.right() - width / 2.0, rect.top() - 1.0);
                pb.line_to(rect.right() - width / 2.0, rect.bottom() + 1.0);
            }
            'B' => {
                pb.move_to(rect.left() - 1.0, rect.bottom() - width / 2.0);
                pb.line_to(rect.right() + 1.0, rect.bottom() - width / 2.0);
            }
            'L' => {
                pb.move_to(rect.left() + width / 2.0, rect.top() - 1.0);
                pb.line_to(rect.left() + width / 2.0, rect.bottom() + 1.0);
            }
            _ => unreachable!(),
        }

        if let Some(line_path) = pb.finish() {
            let dash = translate_dash(style);
            let stroke = tiny_skia::Stroke {
                width: *width,
                line_cap: tiny_skia::LineCap::Butt,
                dash: dash.and_then(|sd| tiny_skia::StrokeDash::new(sd, 0.0)),
                ..Default::default()
            };

            pixmap.stroke_path(
                &line_path,
                &paint,
                &stroke,
                transform,
                Some(&mask), // Apply the clip mask here
            );
        }
    }

    Ok(())
}

fn render_text(
    glyphs: &[azul_core::display_list::GlyphInstance],
    font_instance_key: azul_core::app_resources::FontInstanceKey,
    color: ColorU,
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
    bg_color: ColorU,
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
