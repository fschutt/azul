//! WebRender compositor integration for azul-dll
//!
//! This module bridges between azul-layout's DisplayList and WebRender's rendering pipeline.
//! It handles both GPU (hardware) and CPU (software) rendering paths.

use azul_core::{
    geom::LogicalSize,
    resources::{FontInstanceKey, GlyphOptions, PrimitiveFlags},
    ui_solver::GlyphInstance,
};
use azul_css::props::{
    basic::{color::ColorU, pixel::PixelValue},
    style::border_radius::StyleBorderRadius,
};
use azul_layout::solver3::display_list::DisplayList;
use webrender::{
    api::{
        units::{DeviceIntRect, DeviceIntSize, LayoutPoint, LayoutRect, LayoutSize},
        BorderRadius as WrBorderRadius, ClipChainId as WrClipChainId, ClipMode as WrClipMode,
        ColorF, CommonItemProperties, ComplexClipRegion as WrComplexClipRegion,
        DisplayListBuilder as WrDisplayListBuilder, DocumentId, Epoch, ItemTag, PipelineId,
        SpaceAndClipInfo, SpatialId,
    },
    Transaction,
};

use crate::desktop::wr_translate2::{wr_translate_border_radius, wr_translate_color_f};

/// Translate an Azul DisplayList to WebRender Transaction
pub fn translate_displaylist_to_wr(
    display_list: &DisplayList,
    pipeline_id: PipelineId,
    viewport_size: DeviceIntSize,
    renderer_resources: &azul_core::resources::RendererResources,
    dpi: f32,
) -> Result<Transaction, String> {
    use azul_core::geom::LogicalRect;
    use azul_layout::solver3::display_list::DisplayListItem;

    use crate::desktop::wr_translate2::wr_translate_scrollbar_hit_id;

    let mut txn = Transaction::new();
    let device_rect = DeviceIntRect::from_size(viewport_size);
    txn.set_document_view(device_rect);

    // Create WebRender display list builder
    let mut builder = WrDisplayListBuilder::new(pipeline_id);
    let spatial_id = SpatialId::root_scroll_node(pipeline_id);
    let root_clip_chain_id = WrClipChainId::INVALID;

    // Clip stack management (for PushClip/PopClip)
    let mut clip_stack: Vec<WrClipChainId> = vec![root_clip_chain_id];

    // Spatial stack management (for PushScrollFrame/PopScrollFrame)
    let mut spatial_stack: Vec<SpatialId> = vec![spatial_id];

    // Translate display list items to WebRender
    for item in &display_list.items {
        match item {
            DisplayListItem::Rect {
                bounds,
                color,
                border_radius,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );
                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                // Handle border_radius by creating clip region
                let logical_rect = LogicalRect::new(
                    azul_core::geom::LogicalPosition::new(bounds.origin.x, bounds.origin.y),
                    azul_core::geom::LogicalSize::new(bounds.size.width, bounds.size.height),
                );
                let style_border_radius = convert_border_radius_to_style(border_radius);
                let wr_border_radius = wr_translate_border_radius(
                    style_border_radius,
                    azul_core::geom::LogicalSize::new(bounds.size.width, bounds.size.height),
                );

                if !wr_border_radius.is_zero() {
                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        logical_rect,
                        wr_border_radius,
                        *spatial_stack.last().unwrap(),
                        *clip_stack.last().unwrap(),
                    );

                    let info_clipped = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: new_clip_id,
                        spatial_id: *spatial_stack.last().unwrap(),
                        flags: Default::default(),
                    };

                    builder.push_rect(&info_clipped, rect, color_f);
                    continue;
                }

                builder.push_rect(&info, rect, color_f);
            }

            DisplayListItem::SelectionRect {
                bounds,
                border_radius,
                color,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );
                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                builder.push_rect(&info, rect, color_f);
            }

            DisplayListItem::CursorRect { bounds, color } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );
                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                builder.push_rect(&info, rect, color_f);
            }

            DisplayListItem::Border {
                bounds,
                widths,
                colors,
                styles,
                border_radius,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                // Use full border rendering with per-side widths, colors, and styles
                let rect_size = azul_core::geom::LogicalSize::new(rect.width(), rect.height());

                if let Some((border_widths, border_details)) =
                    crate::desktop::wr_translate2::get_webrender_border(
                        rect_size,
                        *border_radius,
                        *widths,
                        *colors,
                        *styles,
                        1.0, // TODO: Pass actual HiDPI factor
                    )
                {
                    builder.push_border(&info, rect, border_widths, border_details);
                }
            }

            DisplayListItem::ScrollBar {
                bounds,
                color,
                orientation,
                opacity_key,
                hit_id,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );
                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                builder.push_rect(&info, rect, color_f);

                // TODO: Hit-testing for scrollbars needs separate API
                // The crates.io version 0.62.2 doesn't support hit_info field
                // May need to use push_hit_test or similar method
            }

            DisplayListItem::PushClip {
                bounds,
                border_radius,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );

                // Handle rounded corners if border_radius is non-zero
                if !border_radius.is_zero() {
                    // Convert layout BorderRadius to StyleBorderRadius for translation
                    let style_border_radius = convert_border_radius_to_style(border_radius);

                    let wr_border_radius = wr_translate_border_radius(
                        style_border_radius,
                        azul_core::geom::LogicalSize::new(bounds.size.width, bounds.size.height),
                    );

                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        *bounds,
                        wr_border_radius,
                        *spatial_stack.last().unwrap(),
                        *clip_stack.last().unwrap(),
                    );

                    clip_stack.push(new_clip_id);
                } else {
                    // Rectangular clip
                    let clip_id = builder.define_clip_rect(*spatial_stack.last().unwrap(), rect);
                    // Create a clip chain from the clip id
                    let parent = if *clip_stack.last().unwrap() == WrClipChainId::INVALID {
                        None
                    } else {
                        Some(*clip_stack.last().unwrap())
                    };
                    let new_clip_chain_id = builder.define_clip_chain(parent, vec![clip_id]);
                    clip_stack.push(new_clip_chain_id);
                }
            }

            DisplayListItem::PopClip => {
                if clip_stack.len() > 1 {
                    clip_stack.pop();
                }
            }

            DisplayListItem::PushScrollFrame {
                clip_bounds,
                content_size,
                scroll_id,
            } => {
                // TODO: Implement scroll frames properly
                // For now, just track spatial_id
                // let new_spatial_id = builder.push_reference_frame(...);
                // spatial_stack.push(new_spatial_id);
            }

            DisplayListItem::PopScrollFrame => {
                // TODO: Pop scroll frame from stack
                // if spatial_stack.len() > 1 {
                //     spatial_stack.pop();
                // }
            }

            DisplayListItem::HitTestArea { bounds, tag } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                // TODO: Hit-testing for DOM nodes needs separate API
                // The crates.io version 0.62.2 doesn't support hit_info field
                // Push invisible rect for now (hit-testing may work via other means)
                builder.push_rect(&info, rect, ColorF::TRANSPARENT);
            }

            DisplayListItem::Text {
                glyphs,
                font_size_px,
                font_hash,
                color,
                clip_rect,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(clip_rect.origin.x, clip_rect.origin.y),
                    LayoutSize::new(clip_rect.size.width, clip_rect.size.height),
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                // Use push_text helper with font_hash lookup
                let dpi_factor = azul_core::resources::DpiScaleFactor::new(dpi);
                let font_size_au = azul_core::resources::Au::from_px(*font_size_px);
                push_text(
                    &mut builder,
                    &info,
                    glyphs,
                    font_hash.font_hash,
                    *color,
                    renderer_resources,
                    dpi_factor,
                    font_size_au,
                );
            }

            DisplayListItem::Image { .. } => {
                // TODO: Implement image rendering with push_image
            }

            DisplayListItem::IFrame { .. } => {
                // TODO: Implement iframe embedding (nested pipelines)
            }
        }
    }

    // Finalize and set display list
    let (_, dl) = builder.end();
    let layout_size = LayoutSize::new(viewport_size.width as f32, viewport_size.height as f32);
    txn.set_display_list(webrender::api::Epoch(0), (pipeline_id, dl));

    Ok(txn)
}

/// Software compositor stubs
pub mod sw_compositor {
    use super::*;

    pub fn initialize_sw_compositor(viewport_size: DeviceIntSize) -> Result<(), String> {
        eprintln!("[sw_compositor] Initialize {:?} (stub)", viewport_size);
        Ok(())
    }

    pub fn composite_frame_sw(
        _framebuffer: &mut [u8],
        width: usize,
        height: usize,
    ) -> Result<(), String> {
        eprintln!("[sw_compositor] Composite {}x{} (stub)", width, height);
        Ok(())
    }
}

/// Hardware compositor stubs
pub mod hw_compositor {
    use super::*;

    pub fn initialize_hw_compositor(
        viewport_size: DeviceIntSize,
        _gl_context: *mut std::ffi::c_void,
    ) -> Result<(), String> {
        eprintln!("[hw_compositor] Initialize {:?} (stub)", viewport_size);
        Ok(())
    }

    pub fn composite_frame_hw() -> Result<(), String> {
        eprintln!("[hw_compositor] Composite (stub)");
        Ok(())
    }
}

// ========== Helper Functions ==========

/// Convert DisplayList BorderRadius to StyleBorderRadius
#[inline]
fn convert_border_radius_to_style(
    br: &azul_layout::solver3::display_list::BorderRadius,
) -> StyleBorderRadius {
    use azul_css::props::basic::pixel::PixelValue;
    StyleBorderRadius {
        top_left: PixelValue::px(br.top_left),
        top_right: PixelValue::px(br.top_right),
        bottom_left: PixelValue::px(br.bottom_left),
        bottom_right: PixelValue::px(br.bottom_right),
    }
}

// ========== Helper Functions from wr_translate.rs ==========

/// Define a clip region with optional border radius
#[inline]
fn define_border_radius_clip(
    builder: &mut WrDisplayListBuilder,
    layout_rect: azul_core::geom::LogicalRect,
    wr_border_radius: WrBorderRadius,
    rect_spatial_id: SpatialId,
    parent_clip_chain_id: WrClipChainId,
) -> WrClipChainId {
    use crate::desktop::wr_translate2::wr_translate_logical_size;

    // NOTE: only translate the size, position is always (0.0, 0.0)
    let wr_layout_size = wr_translate_logical_size(layout_rect.size);
    let wr_layout_rect = LayoutRect::from_size(wr_layout_size);

    let clip_id = if wr_border_radius.is_zero() {
        builder.define_clip_rect(rect_spatial_id, wr_layout_rect)
    } else {
        builder.define_clip_rounded_rect(
            rect_spatial_id,
            WrComplexClipRegion::new(wr_layout_rect, wr_border_radius, WrClipMode::Clip),
        )
    };

    // Create a clip chain from the clip id
    let parent = if parent_clip_chain_id == WrClipChainId::INVALID {
        None
    } else {
        Some(parent_clip_chain_id)
    };
    builder.define_clip_chain(parent, vec![clip_id])
}

/// Push text to display list
#[inline]
fn push_text(
    builder: &mut WrDisplayListBuilder,
    info: &CommonItemProperties,
    glyphs: &[GlyphInstance],
    font_hash: u64,
    color: ColorU,
    renderer_resources: &azul_core::resources::RendererResources,
    dpi: azul_core::resources::DpiScaleFactor,
    font_size: azul_core::resources::Au,
) {
    use crate::desktop::wr_translate2::wr_translate_layouted_glyphs;

    // Look up FontKey from the font_hash (which comes from the GlyphRun)
    // The font_hash is the hash of FontRef computed during layout
    let font_key = match renderer_resources.font_hash_map.get(&font_hash) {
        Some(k) => k,
        None => {
            eprintln!("[push_text] FontKey not found for font_hash: {}", font_hash);
            return;
        }
    };

    // Look up FontInstanceKey for the given font size and DPI
    let font_instance_key = match renderer_resources.currently_registered_fonts.get(font_key) {
        Some((_, instances)) => match instances.get(&(font_size, dpi)) {
            Some(k) => *k,
            None => {
                eprintln!(
                    "[push_text] FontInstanceKey not found for size {:?} @ dpi {:?}",
                    font_size, dpi
                );
                return;
            }
        },
        None => {
            eprintln!("[push_text] Font instances not found for FontKey");
            return;
        }
    };

    // Translate to WebRender types
    let wr_glyphs = wr_translate_layouted_glyphs(glyphs);
    let wr_font_instance_key =
        crate::desktop::wr_translate2::wr_translate_font_instance_key(font_instance_key);
    let wr_color = azul_css::props::basic::color::ColorF::from(color);

    // Push text to display list
    builder.push_text(
        info,
        info.clip_rect,
        &wr_glyphs,
        wr_font_instance_key,
        wr_translate_color_f(wr_color),
        None, // glyph_options
    );
}
