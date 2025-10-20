//! WebRender compositor integration for azul-dll
//!
//! This module bridges between azul-layout's DisplayList and WebRender's rendering pipeline.
//! It handles both GPU (hardware) and CPU (software) rendering paths.

use azul_core::{
    geom::LogicalSize,
    resources::{FontInstanceKey, GlyphOptions, PrimitiveFlags},
    ui_solver::GlyphInstance,
};
use azul_css::props::{basic::color::ColorU, style::border_radius::StyleBorderRadius};
use azul_layout::solver3::display_list::DisplayList;
use webrender::{
    api::{
        units::{DeviceIntRect, DeviceIntSize, LayoutPoint, LayoutRect, LayoutSize},
        BorderRadius as WrBorderRadius, ClipId as WrClipId, ClipMode as WrClipMode, ColorF,
        CommonItemProperties, ComplexClipRegion as WrComplexClipRegion,
        DisplayListBuilder as WrDisplayListBuilder, DocumentId, Epoch, ItemTag, PipelineId,
        SpaceAndClipInfo, SpatialId,
    },
    Transaction,
};

use crate::desktop::wr_translate2::wr_translate_border_radius;

/// Translate an Azul DisplayList to WebRender Transaction
pub fn translate_displaylist_to_wr(
    display_list: &DisplayList,
    pipeline_id: PipelineId,
    viewport_size: DeviceIntSize,
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
    let root_clip_id = WrClipId::root(pipeline_id);

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
                    clip_id: current_clip_id(),
                    spatial_id: current_spatial_id(),
                    flags: Default::default(),
                };

                // Handle border_radius by creating clip region
                let logical_rect = LogicalRect::new(
                    azul_core::geom::LogicalPosition::new(bounds.origin.x, bounds.origin.y),
                    azul_core::geom::LogicalSize::new(bounds.size.width, bounds.size.height),
                );
                let wr_border_radius = wr_translate_border_radius(
                    *border_radius,
                    azul_core::geom::LogicalSize::new(bounds.size.width, bounds.size.height),
                );

                if !wr_border_radius.is_zero() {
                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        logical_rect,
                        wr_border_radius,
                        current_spatial_id(),
                        current_clip_id(),
                    );

                    let info_clipped = CommonItemProperties {
                        clip_rect: rect,
                        clip_id: new_clip_id,
                        spatial_id: current_spatial_id(),
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
                    clip_id: current_clip_id(),
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
                    clip_id: current_clip_id(),
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
                    clip_id: current_clip_id(),
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

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_id: current_clip_id(),
                    spatial_id: current_spatial_id(),
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
                    let style_border_radius =
                        azul_css::props::style::border_radius::StyleBorderRadius {
                            top_left: azul_css::props::basic::PixelValue {
                                metric: azul_css::props::basic::SizeMetric::Px,
                                number: azul_css::props::basic::FloatValue::new(
                                    border_radius.top_left,
                                ),
                            },
                            top_right: azul_css::props::basic::PixelValue {
                                metric: azul_css::props::basic::SizeMetric::Px,
                                number: azul_css::props::basic::FloatValue::new(
                                    border_radius.top_right,
                                ),
                            },
                            bottom_left: azul_css::props::basic::PixelValue {
                                metric: azul_css::props::basic::SizeMetric::Px,
                                number: azul_css::props::basic::FloatValue::new(
                                    border_radius.bottom_left,
                                ),
                            },
                            bottom_right: azul_css::props::basic::PixelValue {
                                metric: azul_css::props::basic::SizeMetric::Px,
                                number: azul_css::props::basic::FloatValue::new(
                                    border_radius.bottom_right,
                                ),
                            },
                        };

                    let wr_border_radius = wr_translate_border_radius(
                        style_border_radius,
                        azul_core::geom::LogicalSize::new(bounds.size.width, bounds.size.height),
                    );

                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        *bounds,
                        wr_border_radius,
                        current_spatial_id(),
                        current_clip_id(),
                    );

                    clip_stack.push(new_clip_id);
                } else {
                    // Rectangular clip
                    let new_clip_id = builder.define_clip_rect(
                        &SpaceAndClipInfo {
                            spatial_id: current_spatial_id(),
                            clip_id: current_clip_id(),
                        },
                        rect,
                    );
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

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_id: current_clip_id(),
                    spatial_id: current_spatial_id(),
                    flags: Default::default(),
                };

                // TODO: Hit-testing for DOM nodes needs separate API
                // The crates.io version 0.62.2 doesn't support hit_info field
                // Push invisible rect for now (hit-testing may work via other means)
                builder.push_rect(&info, rect, ColorF::TRANSPARENT);
            }

            DisplayListItem::Text {
                glyphs,
                font,
                color,
                clip_rect,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(clip_rect.origin.x, clip_rect.origin.y),
                    LayoutSize::new(clip_rect.size.width, clip_rect.size.height),
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_id: current_clip_id(),
                    spatial_id: current_spatial_id(),
                    flags: Default::default(),
                };

                // Use push_text helper
                push_text(&mut builder, &info, glyphs, font, *color);
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
    let layout_size = LayoutSize::new(viewport_size.width as f32, viewport_size.height as f32);
    txn.set_display_list(
        Epoch(0),
        None, // background color
        layout_size,
        (pipeline_id, dl),
        true, // preserve frame state
    );

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
        builder.define_clip_rect(
            &SpaceAndClipInfo {
                spatial_id: rect_spatial_id,
                clip_id: parent_clip_id,
            },
            wr_layout_rect,
        )
    } else {
        builder.define_clip_rounded_rect(
            &SpaceAndClipInfo {
                spatial_id: rect_spatial_id,
                clip_id: parent_clip_id,
            },
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
    font: &azul_layout::text3::cache::FontRef,
    color: ColorU,
) {
    // TODO: Need to resolve FontRef to FontInstanceKey via font cache
    // For now, skip text rendering
    // This requires access to the font cache to map FontRef -> FontInstanceKey
}
