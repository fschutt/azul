//! WebRender compositor integration for azul-dll
//!
//! This module bridges between azul-layout's DisplayList and WebRender's rendering pipeline.
//! It handles both GPU (hardware) and CPU (software) rendering paths.

use azul_core::{
    geom::LogicalSize,
    resources::{FontInstanceKey, GlyphOptions},
    ui_solver::GlyphInstance,
};
use azul_css::props::{
    basic::color::ColorU, style::border_radius::StyleBorderRadius,
};
use azul_layout::solver3::display_list::DisplayList;
use webrender::api::{
    units::{DeviceIntRect, DeviceIntSize, LayoutPoint, LayoutRect, LayoutSize},
    BorderRadius as WrBorderRadius, ClipId as WrClipId, ClipMode as WrClipMode, ColorF,
    CommonItemProperties, ComplexClipRegion as WrComplexClipRegion,
    DisplayListBuilder as WrDisplayListBuilder, DocumentId, Epoch, PipelineId, SpaceAndClipInfo,
    SpatialId,
};
use webrender::Transaction;

/// Translate an Azul DisplayList to WebRender Transaction
pub fn translate_displaylist_to_wr(
    display_list: &DisplayList,
    pipeline_id: PipelineId,
    viewport_size: DeviceIntSize,
) -> Result<Transaction, String> {
    use azul_core::geom::LogicalRect;
    use azul_layout::solver3::display_list::DisplayListItem;

    use crate::desktop::wr_translate2::{wr_translate_border_radius, wr_translate_scrollbar_hit_id};

    let mut txn = Transaction::new();
    let device_rect = DeviceIntRect::from_size(viewport_size);
    txn.set_document_view(device_rect);

    // Create WebRender display list builder
    let mut builder = WrDisplayListBuilder::new(pipeline_id);
    let spatial_id = SpatialId::root_scroll_node(pipeline_id);
    let root_clip_id = builder.define_clip_rect(
        spatial_id,
        LayoutRect::from_size(LayoutSize::new(
            viewport_size.width as f32,
            viewport_size.height as f32,
        )),
    );

    // Clip stack management (for PushClip/PopClip)
    let mut clip_stack: Vec<WrClipId> = vec![root_clip_id];
    
    // Spatial stack management (for PushScrollFrame/PopScrollFrame)
    let mut spatial_stack: Vec<SpatialId> = vec![spatial_id];

    // Helper to get current clip/spatial IDs
    let current_clip_id = || *clip_stack.last().unwrap();
    let current_spatial_id = || *spatial_stack.last().unwrap();

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
                    spatial_id: current_spatial_id(),
                    flags: Default::default(),
                };

                // Handle border_radius by creating clip region
                if let Some(radii) = border_radius {
                    let logical_rect = LogicalRect::new(
                        azul_core::geom::LogicalPosition::new(bounds.origin.x, bounds.origin.y),
                        azul_core::geom::LogicalSize::new(bounds.size.width, bounds.size.height),
                    );
                    let wr_border_radius = wr_translate_border_radius(
                        *radii,
                        azul_core::geom::LogicalSize::new(bounds.size.width, bounds.size.height),
                    );
                    
                    if !wr_border_radius.is_zero() {
                        let clip_id = define_border_radius_clip(
                            &mut builder,
                            logical_rect,
                            wr_border_radius,
                            current_spatial_id(),
                            current_clip_id(),
                        );
                        
                        let info_clipped = CommonItemProperties {
                            clip_rect: rect,
                            spatial_id: current_spatial_id(),
                            flags: Default::default(),
                        };
                        
                        builder.push_rect(&info_clipped, rect, color_f);
                        continue;
                    }
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
                    spatial_id: current_spatial_id(),
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
                    spatial_id: current_spatial_id(),
                    flags: Default::default(),
                };

                builder.push_rect(&info, rect, color_f);
            }

            DisplayListItem::Border {
                bounds,
                color,
                width,
                border_radius,
            } => {
                // Use proper border rendering from wr_translate.rs
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    spatial_id: current_spatial_id(),
                    flags: Default::default(),
                };

                // TODO: Implement full border support with StyleBorderWidths/Colors/Styles
                // For now, render as simple rect
                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );
                builder.push_rect(&info, rect, color_f);
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

                let mut info = CommonItemProperties {
                    clip_rect: rect,
                    spatial_id: current_spatial_id(),
                    flags: Default::default(),
                };

                // Attach hit-test tag if present
                if let Some(scrollbar_hit_id) = hit_id {
                    let (tag, _) = wr_translate_scrollbar_hit_id(*scrollbar_hit_id);
                    info.hit_info = Some((tag, 0));
                }

                builder.push_rect(&info, rect, color_f);
            }

            DisplayListItem::PushClip {
                bounds,
                border_radius,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );
                
                // Handle rounded corners if border_radius present
                if let Some(radii) = border_radius {
                    let logical_rect = LogicalRect::new(
                        azul_core::geom::LogicalPosition::new(bounds.origin.x, bounds.origin.y),
                        azul_core::geom::LogicalSize::new(bounds.size.width, bounds.size.height),
                    );
                    let wr_border_radius = wr_translate_border_radius(
                        *radii,
                        azul_core::geom::LogicalSize::new(bounds.size.width, bounds.size.height),
                    );
                    
                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        logical_rect,
                        wr_border_radius,
                        current_spatial_id(),
                        current_clip_id(),
                    );
                    
                    clip_stack.push(new_clip_id);
                } else {
                    // Rectangular clip
                    let new_clip_id = builder.define_clip_rect(current_spatial_id(), rect);
                    clip_stack.push(new_clip_id);
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
                let mut info = CommonItemProperties {
                    clip_rect: rect,
                    spatial_id: current_spatial_id(),
                    flags: Default::default(),
                };
                
                // Attach tag for DOM node hit-testing
                if let Some(node_tag) = tag {
                    // Encode NodeId into ItemTag
                    info.hit_info = Some((*node_tag, 0));
                }
                
                // Push invisible rect for hit-testing
                builder.push_rect(&info, rect, ColorF::TRANSPARENT);
            }

            DisplayListItem::Text {
                bounds,
                font_instance_key,
                color,
                glyphs,
                glyph_options,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    spatial_id: current_spatial_id(),
                    flags: Default::default(),
                };

                // Use push_text helper
                push_text(
                    &mut builder,
                    &info,
                    glyphs,
                    *font_instance_key,
                    *color,
                    *glyph_options,
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
    let (_, dl) = builder.finalize();
    txn.set_display_list(Epoch(0), dl);

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

// ========== Helper Functions from wr_translate.rs ==========

/// Define a clip region with optional border radius
#[inline]
fn define_border_radius_clip(
    builder: &mut WrDisplayListBuilder,
    layout_rect: azul_core::geom::LogicalRect,
    wr_border_radius: WrBorderRadius,
    rect_spatial_id: SpatialId,
    parent_clip_id: WrClipId,
) -> WrClipId {
    use crate::desktop::wr_translate2::wr_translate_logical_size;

    // NOTE: only translate the size, position is always (0.0, 0.0)
    let wr_layout_size = wr_translate_logical_size(layout_rect.size);
    let wr_layout_rect = LayoutRect::from_size(wr_layout_size);

    let clip = if wr_border_radius.is_zero() {
        builder.define_clip_rect(rect_spatial_id, wr_layout_rect)
    } else {
        builder.define_clip_rounded_rect(
            rect_spatial_id,
            WrComplexClipRegion::new(wr_layout_rect, wr_border_radius, WrClipMode::Clip),
        )
    };

    clip
}

/// Push text to display list
#[inline]
fn push_text(
    builder: &mut WrDisplayListBuilder,
    info: &CommonItemProperties,
    glyphs: &[GlyphInstance],
    font_instance_key: FontInstanceKey,
    color: ColorU,
    glyph_options: Option<GlyphOptions>,
) {
    use crate::desktop::wr_translate2::{
        wr_translate_color_u, wr_translate_font_instance_key, wr_translate_glyph_options,
        wr_translate_layouted_glyphs,
    };

    builder.push_text(
        &info,
        info.clip_rect,
        &wr_translate_layouted_glyphs(glyphs),
        wr_translate_font_instance_key(font_instance_key),
        wr_translate_color_u(color).into(),
        glyph_options.map(wr_translate_glyph_options),
    );
}

