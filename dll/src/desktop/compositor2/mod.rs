//! WebRender compositor integration for azul-dll
//!
//! This module bridges between azul-layout's DisplayList and WebRender's rendering pipeline.
//! It handles both GPU (hardware) and CPU (software) rendering paths.

use azul_layout::solver3::display_list::DisplayList;
use webrender::{
    api::{
        units::{DeviceIntRect, DeviceIntSize, LayoutPoint, LayoutRect, LayoutSize},
        ColorF, CommonItemProperties, DocumentId, Epoch, PipelineId, 
        SpaceAndClipInfo, SpatialId, DisplayListBuilder as WrDisplayListBuilder,
    },
    Transaction,
};

/// Translate an Azul DisplayList to WebRender Transaction
pub fn translate_displaylist_to_wr(
    display_list: &DisplayList,
    pipeline_id: PipelineId,
    viewport_size: DeviceIntSize,
) -> Result<Transaction, String> {
    use azul_layout::solver3::display_list::DisplayListItem;
    use crate::desktop::wr_translate2::wr_translate_scrollbar_hit_id;

    let mut txn = Transaction::new();
    let device_rect = DeviceIntRect::from_size(viewport_size);
    txn.set_document_view(device_rect);

    // Create WebRender display list builder
    let mut builder = WrDisplayListBuilder::new(pipeline_id);
    let spatial_id = SpatialId::root_scroll_node(pipeline_id);
    let clip_id = builder.define_clip_rect(
        spatial_id,
        LayoutRect::from_size(LayoutSize::new(
            viewport_size.width as f32,
            viewport_size.height as f32,
        )),
    );

    // Translate display list items to WebRender
    for item in &display_list.items {
        match item {
            DisplayListItem::Rect { bounds, color, border_radius } => {
                let space_and_clip = SpaceAndClipInfo {
                    spatial_id,
                    clip_id,
                };
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
                    spatial_id,
                    flags: Default::default(),
                };
                
                builder.push_rect(&info, rect, color_f);
            }
            
            DisplayListItem::ScrollBar { bounds, color, orientation, opacity_key, hit_id } => {
                let space_and_clip = SpaceAndClipInfo {
                    spatial_id,
                    clip_id,
                };
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
                    spatial_id,
                    flags: Default::default(),
                };
                
                // Attach hit-test tag if present
                if let Some(scrollbar_hit_id) = hit_id {
                    let (tag, _) = wr_translate_scrollbar_hit_id(*scrollbar_hit_id);
                    info.hit_info = Some((tag, 0));
                }
                
                builder.push_rect(&info, rect, color_f);
            }
            
            DisplayListItem::HitTestArea { bounds, tag } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );
                let info = CommonItemProperties {
                    clip_rect: rect,
                    spatial_id,
                    flags: Default::default(),
                };
                // TODO: Attach tag for DOM node hit-testing
            }
            
            _ => {
                // TODO: Implement other DisplayListItem variants
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
