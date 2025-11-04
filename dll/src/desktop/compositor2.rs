//! WebRender compositor integration for azul-dll
//!
//! This module bridges between azul-layout's DisplayList and WebRender's rendering pipeline.
//! It handles both GPU (hardware) and CPU (software) rendering paths.

use alloc::collections::BTreeMap;

use azul_core::{
    dom::DomId,
    geom::LogicalSize,
    hit_test::PipelineId as AzulPipelineId,
    resources::{DpiScaleFactor, FontInstanceKey, GlyphOptions, ImageRefHash, PrimitiveFlags},
    ui_solver::GlyphInstance,
};
use azul_css::props::{
    basic::{color::ColorU, pixel::PixelValue},
    style::border_radius::StyleBorderRadius,
};
use azul_layout::{
    solver3::display_list::{BorderRadius, DisplayList},
    window::DomLayoutResult,
};
use webrender::{
    api::{
        units::{
            DeviceIntRect, DeviceIntSize, LayoutPoint, LayoutRect, LayoutSize, LayoutTransform,
            LayoutVector2D,
        },
        APZScrollGeneration, AlphaType as WrAlphaType, BorderRadius as WrBorderRadius,
        BuiltDisplayList as WrBuiltDisplayList, ClipChainId as WrClipChainId,
        ClipMode as WrClipMode, ColorF, CommonItemProperties,
        ComplexClipRegion as WrComplexClipRegion, DisplayListBuilder as WrDisplayListBuilder,
        DocumentId, Epoch, ExternalScrollId, HasScrollLinkedEffect, ItemTag, PipelineId,
        PrimitiveFlags as WrPrimitiveFlags, PropertyBinding, ReferenceFrameKind, SpaceAndClipInfo,
        SpatialId, SpatialTreeItemKey, TransformStyle,
    },
    render_api::ResourceUpdate as WrResourceUpdate,
    Transaction,
};

use crate::desktop::wr_translate2::{
    translate_image_key, wr_translate_border_radius, wr_translate_color_f,
    wr_translate_layouted_glyphs, wr_translate_logical_size, wr_translate_pipeline_id,
};

/// Translate an Azul DisplayList to WebRender DisplayList and resources
/// Returns (resources, display_list, nested_pipelines) tuple that can be added to a transaction
/// by caller. nested_pipelines contains all child iframe pipelines that were recursively built.
pub fn translate_displaylist_to_wr(
    display_list: &DisplayList,
    pipeline_id: PipelineId,
    viewport_size: DeviceIntSize,
    renderer_resources: &azul_core::resources::RendererResources,
    dpi: DpiScaleFactor,
    wr_resources: Vec<WrResourceUpdate>,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    document_id: u32,
) -> Result<
    (
        Vec<WrResourceUpdate>,
        WrBuiltDisplayList,
        Vec<(PipelineId, WrBuiltDisplayList)>,
    ),
    String,
> {
    eprintln!(
        "[compositor2::translate_displaylist_to_wr] START - {} items, viewport={:?}, \
         dpi_factor={}, {} resources",
        display_list.items.len(),
        viewport_size,
        dpi.inner.get(),
        wr_resources.len()
    );
    use azul_core::geom::LogicalRect;
    use azul_layout::solver3::display_list::DisplayListItem;

    use crate::desktop::wr_translate2::wr_translate_scrollbar_hit_id;

    // NOTE: Caller (generate_frame) will add resources to transaction
    // NOTE: Caller (generate_frame) will set document_view
    // We just build the display list here

    // Collect nested iframe pipelines as we process them
    let mut nested_pipelines: Vec<(PipelineId, WrBuiltDisplayList)> = Vec::new();

    // Create WebRender display list builder
    let mut builder = WrDisplayListBuilder::new(pipeline_id);

    // CRITICAL: Begin building the display list before pushing items
    eprintln!("[compositor2] >>>>> CALLING builder.begin() <<<<<");
    builder.begin();
    eprintln!("[compositor2] >>>>> builder.begin() RETURNED <<<<<");
    eprintln!(
        "[compositor2] Builder started, translating {} items",
        display_list.items.len()
    );

    let spatial_id = SpatialId::root_scroll_node(pipeline_id);
    let root_clip_chain_id = WrClipChainId::INVALID;

    // NOTE: We DON'T push a stacking context here anymore!
    // The display list generation now includes PushStackingContext items.
    // Pushing one here would create an extra nested context that isn't needed.

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
                eprintln!(
                    "[compositor2] Rect item: bounds={:?}, color={:?}",
                    bounds, color
                );
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );
                eprintln!("[compositor2] Translated to LayoutRect: {:?}", rect);
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

                    eprintln!(
                        "[compositor2] >>>>> push_rect (with clip): {:?} <<<<<",
                        rect
                    );
                    builder.push_rect(&info_clipped, rect, color_f);
                    continue;
                }

                eprintln!("[compositor2] >>>>> push_rect: {:?} <<<<<", rect);
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
                        dpi.inner.get(),
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

                // Add hit-test item for scrollbar interaction
                if let Some(scrollbar_hit_id) = hit_id {
                    let (tag, _) = crate::desktop::wr_translate2::wr_translate_scrollbar_hit_id(
                        *scrollbar_hit_id,
                    );

                    builder.push_hit_test(
                        rect,
                        *clip_stack.last().unwrap(),
                        *spatial_stack.last().unwrap(),
                        Default::default(),
                        tag,
                    );
                }
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
                // Create a scroll frame with proper clipping and content size
                let frame_rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(clip_bounds.origin.x, clip_bounds.origin.y),
                    LayoutSize::new(clip_bounds.size.width, clip_bounds.size.height),
                );

                let content_rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(clip_bounds.origin.x, clip_bounds.origin.y),
                    LayoutSize::new(content_size.width, content_size.height),
                );

                // Define scroll frame using WebRender API
                let parent_space = *spatial_stack.last().unwrap();
                let external_scroll_id = ExternalScrollId(*scroll_id, pipeline_id);

                let scroll_spatial_id = builder.define_scroll_frame(
                    parent_space,
                    external_scroll_id,
                    content_rect,
                    frame_rect,
                    LayoutVector2D::zero(), // external_scroll_offset
                    0,                      // scroll_offset_generation (APZScrollGeneration)
                    HasScrollLinkedEffect::No,
                    SpatialTreeItemKey::new(*scroll_id, 0),
                );

                // Push the new spatial ID onto the stack
                spatial_stack.push(scroll_spatial_id);

                // Define clip for the scroll frame
                let scroll_clip_id = builder.define_clip_rect(scroll_spatial_id, frame_rect);

                // Create a clip chain with this clip
                let scroll_clip_chain = builder.define_clip_chain(None, [scroll_clip_id]);
                clip_stack.push(scroll_clip_chain);

                eprintln!(
                    "[compositor2] PushScrollFrame: frame_rect={:?}, content_rect={:?}, \
                     scroll_id={}, spatial_id={:?}",
                    frame_rect, content_rect, scroll_id, scroll_spatial_id
                );
            }

            DisplayListItem::PopScrollFrame => {
                // Pop both spatial and clip stacks
                clip_stack.pop();
                spatial_stack.pop();

                if spatial_stack.is_empty() || clip_stack.is_empty() {
                    eprintln!("[compositor2] ERROR: PopScrollFrame caused stack underflow");
                    return Err("Scroll frame stack underflow".to_string());
                }

                eprintln!("[compositor2] PopScrollFrame: spatial and clip stacks popped");
            }

            DisplayListItem::HitTestArea { bounds, tag } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );

                // Create a hit test item with the provided tag
                // The tag is a tuple of (u64, u16) where:
                // - u64 encodes DomId and NodeId
                // - u16 is for additional data (usually 0)
                let item_tag: ItemTag = (*tag, 0);

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: WrPrimitiveFlags::default(),
                };

                // Push a transparent rect with the hit test tag
                // This creates a hittable area without visible rendering
                builder.push_rect(&info, rect, ColorF::TRANSPARENT);

                // Note: In newer WebRender versions, there's a dedicated push_hit_test() method
                // For version 0.62.2, we use transparent rects with ItemTags in
                // CommonItemProperties The tag will be returned in hit test results

                eprintln!(
                    "[compositor2] HitTestArea: bounds={:?}, tag={:?}",
                    bounds, tag
                );
            }

            DisplayListItem::Underline {
                bounds,
                color,
                thickness,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, *thickness),
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

            DisplayListItem::Strikethrough {
                bounds,
                color,
                thickness,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, *thickness),
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

            DisplayListItem::Overline {
                bounds,
                color,
                thickness,
            } => {
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, *thickness),
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

            DisplayListItem::Text {
                glyphs,
                font_size_px,
                font_hash,
                color,
                clip_rect,
            } => {
                eprintln!(
                    "[compositor2] Text item: {} glyphs, font_size={}, color={:?}, clip_rect={:?}",
                    glyphs.len(),
                    font_size_px,
                    color,
                    clip_rect
                );

                // Log first few glyph positions for debugging
                if !glyphs.is_empty() {
                    eprintln!("[compositor2] First 3 glyphs:");
                    for (i, g) in glyphs.iter().take(3).enumerate() {
                        eprintln!(
                            "  [{}] index={}, pos=({}, {})",
                            i, g.index, g.point.x, g.point.y
                        );
                    }
                }

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
                let font_size_au = azul_core::resources::Au::from_px(*font_size_px);
                push_text(
                    &mut builder,
                    &info,
                    glyphs,
                    font_hash.font_hash,
                    *color,
                    renderer_resources,
                    dpi,
                    font_size_au,
                );
            }

            DisplayListItem::Image { bounds, key } => {
                // Look up the ImageKey in renderer_resources
                let image_ref_hash = ImageRefHash(key.key as usize);

                if let Some(resolved_image) = renderer_resources.get_image(&image_ref_hash) {
                    let wr_image_key = translate_image_key(resolved_image.key);

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

                    eprintln!(
                        "[compositor2] >>>>> push_image: bounds={:?}, key={:?} <<<<<",
                        bounds, wr_image_key
                    );

                    // Use push_image from WebRender
                    // ImageRendering::Auto and PremultipliedAlpha are reasonable defaults
                    use webrender::api::ImageRendering as WrImageRendering;

                    builder.push_image(
                        &info,
                        rect,
                        WrImageRendering::Auto,
                        WrAlphaType::PremultipliedAlpha,
                        wr_image_key,
                        ColorF::WHITE, // No tint by default
                    );
                } else {
                    eprintln!(
                        "[compositor2] WARNING: Image key {:?} not found in renderer_resources",
                        key
                    );
                }
            }

            DisplayListItem::PushStackingContext { z_index, bounds } => {
                eprintln!(
                    "[compositor2] PushStackingContext: z_index={}, bounds={:?}",
                    z_index, bounds
                );

                // Just push a simple stacking context at the bounds origin
                // Use the current spatial_id from the stack (don't create a new reference frame)
                let current_spatial_id = *spatial_stack.last().unwrap();
                eprintln!(
                    "[compositor2] >>>>> push_simple_stacking_context at ({}, {}) <<<<<",
                    bounds.origin.x, bounds.origin.y
                );
                builder.push_simple_stacking_context(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    current_spatial_id,
                    WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                );
                eprintln!("[compositor2] >>>>> push_simple_stacking_context RETURNED <<<<<");
            }

            DisplayListItem::PopStackingContext => {
                eprintln!("[compositor2] PopStackingContext");
                eprintln!("[compositor2] >>>>> pop_stacking_context <<<<<");
                builder.pop_stacking_context();
                eprintln!("[compositor2] >>>>> pop_stacking_context RETURNED <<<<<");
            }

            DisplayListItem::IFrame {
                child_dom_id,
                bounds,
                clip_rect,
            } => {
                // IFrame rendering implementation:
                // 1. Create PipelineId from child_dom_id
                // 2. Look up child display list from layout_results
                // 3. Recursively translate child display list
                // 4. Store child pipeline for later registration
                // 5. Push iframe to current display list

                let child_pipeline_id = wr_translate_pipeline_id(AzulPipelineId(
                    child_dom_id.inner as u32,
                    document_id,
                ));

                eprintln!(
                    "[compositor2] IFrame: child_dom_id={:?}, child_pipeline_id={:?}, \
                     bounds={:?}, clip_rect={:?}",
                    child_dom_id, child_pipeline_id, bounds, clip_rect
                );

                // Look up child layout result
                if let Some(child_layout_result) = layout_results.get(child_dom_id) {
                    eprintln!(
                        "[compositor2] Found child layout result with {} display list items",
                        child_layout_result.display_list.items.len()
                    );

                    // Recursively translate child display list
                    match translate_displaylist_to_wr(
                        &child_layout_result.display_list,
                        child_pipeline_id,
                        viewport_size,
                        renderer_resources,
                        dpi,
                        Vec::new(), // No resources for child - they're already in parent
                        layout_results,
                        document_id,
                    ) {
                        Ok((_, child_dl, mut child_nested)) => {
                            eprintln!(
                                "[compositor2] Successfully translated child display list with {} \
                                 nested pipelines",
                                child_nested.len()
                            );

                            // Store this child pipeline
                            nested_pipelines.push((child_pipeline_id, child_dl));

                            // Store all deeply nested pipelines
                            nested_pipelines.append(&mut child_nested);

                            // Push iframe to current display list
                            let space_and_clip = SpaceAndClipInfo {
                                spatial_id: *spatial_stack.last().unwrap(),
                                clip_chain_id: *clip_stack.last().unwrap(),
                            };

                            let wr_bounds = LayoutRect::from_origin_and_size(
                                LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                                LayoutSize::new(bounds.size.width, bounds.size.height),
                            );
                            let wr_clip_rect = LayoutRect::from_origin_and_size(
                                LayoutPoint::new(clip_rect.origin.x, clip_rect.origin.y),
                                LayoutSize::new(clip_rect.size.width, clip_rect.size.height),
                            );

                            builder.push_iframe(
                                wr_bounds,
                                wr_clip_rect,
                                &space_and_clip,
                                child_pipeline_id,
                                false, // ignore_missing_pipeline
                            );

                            eprintln!(
                                "[compositor2] Pushed iframe for pipeline {:?}",
                                child_pipeline_id
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "[compositor2] Error translating child display list for \
                                 dom_id={:?}: {}",
                                child_dom_id, e
                            );
                        }
                    }
                } else {
                    eprintln!(
                        "[compositor2] WARNING: Child DOM {:?} not found in layout_results",
                        child_dom_id
                    );
                }
            }
        }
    }

    // NOTE: We DON'T pop a stacking context here anymore!
    // The display list now includes PopStackingContext items that match the Push items.

    // Finalize and return display list
    eprintln!("[compositor2] >>>>> CALLING builder.end() <<<<<");
    let (_, dl) = builder.end();
    eprintln!("[compositor2] >>>>> builder.end() RETURNED <<<<<");

    eprintln!(
        "[compositor2] Builder finished, returning ({} resources, display_list, {} nested \
         pipelines)",
        wr_resources.len(),
        nested_pipelines.len()
    );

    // Print detailed display list summary before returning
    eprintln!("=== Display List Summary ===");
    eprintln!("Pipeline: {:?}", pipeline_id);
    eprintln!("Viewport: {:?}", viewport_size);
    eprintln!("Total items in source: {}", display_list.items.len());
    for (idx, item) in display_list.items.iter().enumerate() {
        eprintln!("  Item {}: {:?}", idx + 1, item);
    }
    eprintln!("============================");

    Ok((wr_resources, dl, nested_pipelines))
}

// ========== Helper Functions ==========

/// Convert DisplayList BorderRadius to StyleBorderRadius
#[inline]
fn convert_border_radius_to_style(br: &BorderRadius) -> StyleBorderRadius {
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

    eprintln!(
        "[push_text] ✓ Pushing {} glyphs with FontInstanceKey {:?}, color={:?}",
        wr_glyphs.len(),
        wr_font_instance_key,
        wr_color
    );

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
