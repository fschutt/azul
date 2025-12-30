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
        ComplexClipRegion as WrComplexClipRegion, ConicGradient as WrConicGradient,
        DisplayListBuilder as WrDisplayListBuilder, DocumentId, Epoch, ExtendMode as WrExtendMode,
        ExternalScrollId, Gradient as WrGradient, GradientStop as WrGradientStop,
        HasScrollLinkedEffect, ItemTag, PipelineId, PrimitiveFlags as WrPrimitiveFlags,
        PropertyBinding, RadialGradient as WrRadialGradient, ReferenceFrameKind, SpaceAndClipInfo,
        SpatialId, SpatialTreeItemKey, TransformStyle,
    },
    render_api::ResourceUpdate as WrResourceUpdate,
    Transaction,
};

use crate::desktop::wr_translate2::{
    translate_image_key, wr_translate_border_radius, wr_translate_color_f,
    wr_translate_logical_size, wr_translate_pipeline_id,
};

/// Convert logical pixel bounds to physical pixel LayoutRect for WebRender.
/// All display list coordinates are in logical CSS pixels and need to be scaled
/// by the HiDPI factor for correct rendering on HiDPI displays.
#[inline]
fn scale_bounds_to_layout_rect(
    bounds: &azul_core::geom::LogicalRect,
    dpi: f32,
) -> LayoutRect {
    LayoutRect::from_origin_and_size(
        LayoutPoint::new(bounds.origin.x * dpi, bounds.origin.y * dpi),
        LayoutSize::new(bounds.size.width * dpi, bounds.size.height * dpi),
    )
}

/// Scale a single f32 value from logical to physical pixels
#[inline]
fn scale_px(val: f32, dpi: f32) -> f32 {
    val * dpi
}

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

    // Extract DPI scale factor for converting logical to physical pixels
    // All display list coordinates are in logical CSS pixels
    let dpi_scale = dpi.inner.get();

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
                    "[compositor2] Rect item: bounds={:?}, color={:?}, dpi={}",
                    bounds, color, dpi_scale
                );
                let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);
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
                // Note: LogicalRect here is used for clipping calculations, but we pass
                // the scaled rect to WebRender
                let logical_rect = LogicalRect::new(
                    azul_core::geom::LogicalPosition::new(
                        scale_px(bounds.origin.x, dpi_scale),
                        scale_px(bounds.origin.y, dpi_scale),
                    ),
                    azul_core::geom::LogicalSize::new(
                        scale_px(bounds.size.width, dpi_scale),
                        scale_px(bounds.size.height, dpi_scale),
                    ),
                );
                let style_border_radius = convert_border_radius_to_style(border_radius);
                let wr_border_radius = wr_translate_border_radius(
                    style_border_radius,
                    azul_core::geom::LogicalSize::new(
                        scale_px(bounds.size.width, dpi_scale),
                        scale_px(bounds.size.height, dpi_scale),
                    ),
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
                let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);
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
                let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);
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
                let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);

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
                let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);
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
                let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);

                // Handle rounded corners if border_radius is non-zero
                if !border_radius.is_zero() {
                    // Convert layout BorderRadius to StyleBorderRadius for translation
                    let style_border_radius = convert_border_radius_to_style(border_radius);

                    let wr_border_radius = wr_translate_border_radius(
                        style_border_radius,
                        azul_core::geom::LogicalSize::new(
                            scale_px(bounds.size.width, dpi_scale),
                            scale_px(bounds.size.height, dpi_scale),
                        ),
                    );

                    // Create scaled bounds for clip
                    let scaled_bounds = azul_core::geom::LogicalRect::new(
                        azul_core::geom::LogicalPosition::new(
                            scale_px(bounds.origin.x, dpi_scale),
                            scale_px(bounds.origin.y, dpi_scale),
                        ),
                        azul_core::geom::LogicalSize::new(
                            scale_px(bounds.size.width, dpi_scale),
                            scale_px(bounds.size.height, dpi_scale),
                        ),
                    );

                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        scaled_bounds,
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
                    LayoutPoint::new(
                        scale_px(clip_bounds.origin.x, dpi_scale),
                        scale_px(clip_bounds.origin.y, dpi_scale),
                    ),
                    LayoutSize::new(
                        scale_px(clip_bounds.size.width, dpi_scale),
                        scale_px(clip_bounds.size.height, dpi_scale),
                    ),
                );

                let content_rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(
                        scale_px(clip_bounds.origin.x, dpi_scale),
                        scale_px(clip_bounds.origin.y, dpi_scale),
                    ),
                    LayoutSize::new(
                        scale_px(content_size.width, dpi_scale),
                        scale_px(content_size.height, dpi_scale),
                    ),
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
                let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);

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
                    LayoutPoint::new(
                        scale_px(bounds.origin.x, dpi_scale),
                        scale_px(bounds.origin.y, dpi_scale),
                    ),
                    LayoutSize::new(
                        scale_px(bounds.size.width, dpi_scale),
                        scale_px(*thickness, dpi_scale),
                    ),
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
                    LayoutPoint::new(
                        scale_px(bounds.origin.x, dpi_scale),
                        scale_px(bounds.origin.y, dpi_scale),
                    ),
                    LayoutSize::new(
                        scale_px(bounds.size.width, dpi_scale),
                        scale_px(*thickness, dpi_scale),
                    ),
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
                    LayoutPoint::new(
                        scale_px(bounds.origin.x, dpi_scale),
                        scale_px(bounds.origin.y, dpi_scale),
                    ),
                    LayoutSize::new(
                        scale_px(bounds.size.width, dpi_scale),
                        scale_px(*thickness, dpi_scale),
                    ),
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
                    "[compositor2] Text item: {} glyphs, font_size={}, color={:?}, clip_rect={:?}, dpi={}",
                    glyphs.len(),
                    font_size_px,
                    color,
                    clip_rect,
                    dpi_scale
                );

                // Log first few glyph positions for debugging
                if !glyphs.is_empty() {
                    eprintln!("[compositor2] First 3 glyphs (before scaling):");
                    for (i, g) in glyphs.iter().take(3).enumerate() {
                        eprintln!(
                            "  [{}] index={}, pos=({}, {})",
                            i, g.index, g.point.x, g.point.y
                        );
                    }
                }

                // Scale clip_rect from logical to physical pixels
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(
                        scale_px(clip_rect.origin.x, dpi_scale),
                        scale_px(clip_rect.origin.y, dpi_scale),
                    ),
                    LayoutSize::new(
                        scale_px(clip_rect.size.width, dpi_scale),
                        scale_px(clip_rect.size.height, dpi_scale),
                    ),
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                // Use push_text helper with font_hash lookup
                // NOTE: font_size_px is logical, Au conversion and DPI scaling happens in
                // translate_add_font_instance
                let font_size_au = azul_core::resources::Au::from_px(*font_size_px);
                
                // Scale container origin for glyph positioning
                let scaled_origin = azul_core::geom::LogicalPosition::new(
                    scale_px(clip_rect.origin.x, dpi_scale),
                    scale_px(clip_rect.origin.y, dpi_scale),
                );
                
                push_text(
                    &mut builder,
                    &info,
                    glyphs,
                    font_hash.font_hash,
                    *color,
                    renderer_resources,
                    dpi,
                    font_size_au,
                    scaled_origin, // Pass scaled container origin to offset glyphs
                );
            }

            DisplayListItem::Image { bounds, key } => {
                // Look up the ImageKey in renderer_resources
                let image_ref_hash = ImageRefHash { inner: key.key as usize };

                if let Some(resolved_image) = renderer_resources.get_image(&image_ref_hash) {
                    let wr_image_key = translate_image_key(resolved_image.key);

                    let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);

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
                    "[compositor2] PushStackingContext: z_index={}, bounds={:?}, dpi={}",
                    z_index, bounds, dpi_scale
                );

                // Just push a simple stacking context at the bounds origin
                // Use the current spatial_id from the stack (don't create a new reference frame)
                let current_spatial_id = *spatial_stack.last().unwrap();
                let scaled_origin = LayoutPoint::new(
                    scale_px(bounds.origin.x, dpi_scale),
                    scale_px(bounds.origin.y, dpi_scale),
                );
                eprintln!(
                    "[compositor2] >>>>> push_simple_stacking_context at ({}, {}) <<<<<",
                    scaled_origin.x, scaled_origin.y
                );
                builder.push_simple_stacking_context(
                    scaled_origin,
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

                            let wr_bounds = scale_bounds_to_layout_rect(bounds, dpi_scale);
                            let wr_clip_rect = scale_bounds_to_layout_rect(clip_rect, dpi_scale);

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

            DisplayListItem::TextLayout { .. } => {
                // TextLayout items are handled elsewhere (via PushCachedTextRuns)
                eprintln!("[compositor2] TextLayout item (handled via cached text runs)");
            }

            // gradient rendering
            DisplayListItem::LinearGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                eprintln!("[compositor2] LinearGradient: bounds={:?}", bounds);

                // Convert CSS gradient to WebRender gradient
                let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);

                // Create layout rect for computing gradient points (use scaled size)
                use azul_css::props::basic::{
                    LayoutPoint as CssLayoutPoint, LayoutRect as CssLayoutRect,
                    LayoutSize as CssLayoutSize,
                };
                let scaled_width = scale_px(bounds.size.width, dpi_scale);
                let scaled_height = scale_px(bounds.size.height, dpi_scale);
                let layout_rect = CssLayoutRect {
                    origin: CssLayoutPoint::new(0, 0),
                    size: CssLayoutSize {
                        width: scaled_width as isize,
                        height: scaled_height as isize,
                    },
                };

                // Get start and end points from direction
                let (start, end) = gradient.direction.to_points(&layout_rect);
                let start_point = LayoutPoint::new(start.x as f32, start.y as f32);
                let end_point = LayoutPoint::new(end.x as f32, end.y as f32);

                // Convert extend mode
                let extend_mode = match gradient.extend_mode {
                    azul_css::props::style::background::ExtendMode::Clamp => WrExtendMode::Clamp,
                    azul_css::props::style::background::ExtendMode::Repeat => WrExtendMode::Repeat,
                };

                // Convert gradient stops
                let wr_stops: Vec<WrGradientStop> = gradient
                    .stops
                    .as_ref()
                    .iter()
                    .map(|stop| {
                        WrGradientStop {
                            offset: stop.offset.normalized(), // normalized() returns 0-1 range
                            color: wr_translate_color_f(
                                azul_css::props::basic::color::ColorF::from(stop.color),
                            ),
                        }
                    })
                    .collect();

                // Push stops first
                builder.push_stops(&wr_stops);

                // Create WebRender gradient
                let wr_gradient = WrGradient {
                    start_point,
                    end_point,
                    extend_mode,
                };

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                // Push gradient (tile_size = bounds size, tile_spacing = 0 for no tiling)
                let tile_size = LayoutSize::new(scaled_width, scaled_height);
                let tile_spacing = LayoutSize::zero();
                builder.push_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
            }

            DisplayListItem::RadialGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                eprintln!("[compositor2] RadialGradient: bounds={:?}", bounds);

                let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);
                let scaled_width = scale_px(bounds.size.width, dpi_scale);
                let scaled_height = scale_px(bounds.size.height, dpi_scale);

                // Compute center based on background position
                use azul_css::props::style::background::{
                    BackgroundPositionHorizontal, BackgroundPositionVertical,
                };
                let center_x = match &gradient.position.horizontal {
                    BackgroundPositionHorizontal::Left => 0.0,
                    BackgroundPositionHorizontal::Center => bounds.size.width / 2.0,
                    BackgroundPositionHorizontal::Right => bounds.size.width,
                    BackgroundPositionHorizontal::Exact(px) => {
                        px.to_pixels_internal(bounds.size.width, 16.0)
                    }
                };
                let center_y = match &gradient.position.vertical {
                    BackgroundPositionVertical::Top => 0.0,
                    BackgroundPositionVertical::Center => bounds.size.height / 2.0,
                    BackgroundPositionVertical::Bottom => bounds.size.height,
                    BackgroundPositionVertical::Exact(px) => {
                        px.to_pixels_internal(bounds.size.height, 16.0)
                    }
                };
                let center = LayoutPoint::new(center_x, center_y);

                // Compute radius based on shape and size keyword
                use azul_css::props::style::background::RadialGradientSize;
                let radius = match (&gradient.shape, &gradient.size) {
                    // Circle: same radius in both directions
                    (
                        azul_css::props::style::background::Shape::Circle,
                        RadialGradientSize::ClosestSide,
                    ) => {
                        let r = center_x
                            .min(center_y)
                            .min(bounds.size.width - center_x)
                            .min(bounds.size.height - center_y);
                        LayoutSize::new(r, r)
                    }
                    (
                        azul_css::props::style::background::Shape::Circle,
                        RadialGradientSize::FarthestSide,
                    ) => {
                        let r = center_x
                            .max(center_y)
                            .max(bounds.size.width - center_x)
                            .max(bounds.size.height - center_y);
                        LayoutSize::new(r, r)
                    }
                    (
                        azul_css::props::style::background::Shape::Circle,
                        RadialGradientSize::ClosestCorner,
                    ) => {
                        let dx = center_x.min(bounds.size.width - center_x);
                        let dy = center_y.min(bounds.size.height - center_y);
                        let r = (dx * dx + dy * dy).sqrt();
                        LayoutSize::new(r, r)
                    }
                    (
                        azul_css::props::style::background::Shape::Circle,
                        RadialGradientSize::FarthestCorner,
                    ) => {
                        let dx = center_x.max(bounds.size.width - center_x);
                        let dy = center_y.max(bounds.size.height - center_y);
                        let r = (dx * dx + dy * dy).sqrt();
                        LayoutSize::new(r, r)
                    }
                    // Ellipse: different radius for x and y
                    (
                        azul_css::props::style::background::Shape::Ellipse,
                        RadialGradientSize::ClosestSide,
                    ) => {
                        let rx = center_x.min(bounds.size.width - center_x);
                        let ry = center_y.min(bounds.size.height - center_y);
                        LayoutSize::new(rx, ry)
                    }
                    (
                        azul_css::props::style::background::Shape::Ellipse,
                        RadialGradientSize::FarthestSide,
                    ) => {
                        let rx = center_x.max(bounds.size.width - center_x);
                        let ry = center_y.max(bounds.size.height - center_y);
                        LayoutSize::new(rx, ry)
                    }
                    (
                        azul_css::props::style::background::Shape::Ellipse,
                        RadialGradientSize::ClosestCorner,
                    ) => {
                        // For ellipse, scale to reach corner while maintaining aspect ratio
                        let dx = center_x.min(bounds.size.width - center_x);
                        let dy = center_y.min(bounds.size.height - center_y);
                        LayoutSize::new(dx, dy)
                    }
                    (
                        azul_css::props::style::background::Shape::Ellipse,
                        RadialGradientSize::FarthestCorner,
                    ) => {
                        let dx = center_x.max(bounds.size.width - center_x);
                        let dy = center_y.max(bounds.size.height - center_y);
                        LayoutSize::new(dx, dy)
                    }
                };

                // Convert extend mode
                let extend_mode = match gradient.extend_mode {
                    azul_css::props::style::background::ExtendMode::Clamp => WrExtendMode::Clamp,
                    azul_css::props::style::background::ExtendMode::Repeat => WrExtendMode::Repeat,
                };

                // Convert gradient stops
                let wr_stops: Vec<WrGradientStop> = gradient
                    .stops
                    .as_ref()
                    .iter()
                    .map(|stop| WrGradientStop {
                        offset: stop.offset.normalized(),
                        color: wr_translate_color_f(azul_css::props::basic::color::ColorF::from(
                            stop.color,
                        )),
                    })
                    .collect();

                builder.push_stops(&wr_stops);

                let wr_gradient = WrRadialGradient {
                    center,
                    radius,
                    start_offset: 0.0,
                    end_offset: 1.0,
                    extend_mode,
                };

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                let tile_size = LayoutSize::new(scaled_width, scaled_height);
                let tile_spacing = LayoutSize::zero();
                builder.push_radial_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
            }

            DisplayListItem::ConicGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                eprintln!("[compositor2] ConicGradient: bounds={:?}", bounds);

                let rect = scale_bounds_to_layout_rect(bounds, dpi_scale);
                let scaled_width = scale_px(bounds.size.width, dpi_scale);
                let scaled_height = scale_px(bounds.size.height, dpi_scale);

                // Compute center based on CSS position (default is center)
                use azul_css::props::style::background::{
                    BackgroundPositionHorizontal, BackgroundPositionVertical,
                };
                let center_x = match &gradient.center.horizontal {
                    BackgroundPositionHorizontal::Left => 0.0,
                    BackgroundPositionHorizontal::Center => bounds.size.width / 2.0,
                    BackgroundPositionHorizontal::Right => bounds.size.width,
                    BackgroundPositionHorizontal::Exact(px) => {
                        px.to_pixels_internal(bounds.size.width, 16.0)
                    }
                };
                let center_y = match &gradient.center.vertical {
                    BackgroundPositionVertical::Top => 0.0,
                    BackgroundPositionVertical::Center => bounds.size.height / 2.0,
                    BackgroundPositionVertical::Bottom => bounds.size.height,
                    BackgroundPositionVertical::Exact(px) => {
                        px.to_pixels_internal(bounds.size.height, 16.0)
                    }
                };
                let center = LayoutPoint::new(center_x, center_y);

                // Get angle in radians (CSS uses degrees, WebRender expects radians)
                // Use to_degrees_raw() to preserve 360deg as distinct from 0deg
                let angle = gradient.angle.to_degrees_raw().to_radians();

                // Convert extend mode
                let extend_mode = match gradient.extend_mode {
                    azul_css::props::style::background::ExtendMode::Clamp => WrExtendMode::Clamp,
                    azul_css::props::style::background::ExtendMode::Repeat => WrExtendMode::Repeat,
                };

                // Convert gradient stops (conic uses angle-based stops normalized to 0-1)
                // Use to_degrees_raw() to preserve 360deg (1.0) as distinct from 0deg (0.0)
                let wr_stops: Vec<WrGradientStop> = gradient
                    .stops
                    .as_ref()
                    .iter()
                    .map(|stop| {
                        WrGradientStop {
                            offset: stop.angle.to_degrees_raw() / 360.0, /* Convert angle to 0-1
                                                                          * range */
                            color: wr_translate_color_f(
                                azul_css::props::basic::color::ColorF::from(stop.color),
                            ),
                        }
                    })
                    .collect();

                builder.push_stops(&wr_stops);

                let wr_gradient = WrConicGradient {
                    center,
                    angle,
                    start_offset: 0.0,
                    end_offset: 1.0,
                    extend_mode,
                };

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };

                let tile_size = LayoutSize::new(scaled_width, scaled_height);
                let tile_spacing = LayoutSize::zero();
                builder.push_conic_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
            }

            // box shadow
            DisplayListItem::BoxShadow {
                bounds,
                shadow,
                border_radius,
            } => {
                eprintln!(
                    "[compositor2] BoxShadow: bounds={:?}, shadow={:?}",
                    bounds, shadow
                );
                // TODO: Implement proper WebRender box shadow using builder.push_box_shadow()
                // For now, render a simplified shadow as an offset rectangle
                let offset_x = scale_px(shadow.offset_x.inner.to_pixels_internal(0.0, 16.0), dpi_scale);
                let offset_y = scale_px(shadow.offset_y.inner.to_pixels_internal(0.0, 16.0), dpi_scale);
                let rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(
                        scale_px(bounds.origin.x, dpi_scale) + offset_x,
                        scale_px(bounds.origin.y, dpi_scale) + offset_y,
                    ),
                    LayoutSize::new(
                        scale_px(bounds.size.width, dpi_scale),
                        scale_px(bounds.size.height, dpi_scale),
                    ),
                );
                let color_f =
                    wr_translate_color_f(azul_css::props::basic::color::ColorF::from(shadow.color));
                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: *clip_stack.last().unwrap(),
                    spatial_id: *spatial_stack.last().unwrap(),
                    flags: Default::default(),
                };
                builder.push_rect(&info, rect, color_f);
            }

            // filter effects
            DisplayListItem::PushFilter { bounds, filters } => {
                eprintln!(
                    "[compositor2] PushFilter: bounds={:?}, {} filters",
                    bounds,
                    filters.len()
                );
                // TODO: Implement proper WebRender filter stacking context
                // For now, just push a simple stacking context
                let current_spatial_id = *spatial_stack.last().unwrap();
                builder.push_simple_stacking_context(
                    LayoutPoint::new(
                        scale_px(bounds.origin.x, dpi_scale),
                        scale_px(bounds.origin.y, dpi_scale),
                    ),
                    current_spatial_id,
                    WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                );
            }
            DisplayListItem::PopFilter => {
                eprintln!("[compositor2] PopFilter");
                builder.pop_stacking_context();
            }

            DisplayListItem::PushBackdropFilter { bounds, filters } => {
                eprintln!(
                    "[compositor2] PushBackdropFilter: bounds={:?}, {} filters",
                    bounds,
                    filters.len()
                );
                // TODO: Implement proper WebRender backdrop filter
                // Backdrop filters require special handling in WebRender
                let current_spatial_id = *spatial_stack.last().unwrap();
                builder.push_simple_stacking_context(
                    LayoutPoint::new(
                        scale_px(bounds.origin.x, dpi_scale),
                        scale_px(bounds.origin.y, dpi_scale),
                    ),
                    current_spatial_id,
                    WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                );
            }
            DisplayListItem::PopBackdropFilter => {
                eprintln!("[compositor2] PopBackdropFilter");
                builder.pop_stacking_context();
            }

            DisplayListItem::PushOpacity { bounds, opacity } => {
                eprintln!(
                    "[compositor2] PushOpacity: bounds={:?}, opacity={}",
                    bounds, opacity
                );
                // TODO: Implement proper WebRender opacity stacking context
                let current_spatial_id = *spatial_stack.last().unwrap();
                builder.push_simple_stacking_context(
                    LayoutPoint::new(
                        scale_px(bounds.origin.x, dpi_scale),
                        scale_px(bounds.origin.y, dpi_scale),
                    ),
                    current_spatial_id,
                    WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                );
            }
            DisplayListItem::PopOpacity => {
                eprintln!("[compositor2] PopOpacity");
                builder.pop_stacking_context();
            }
        }
    }

    // NOTE: We DON'T pop a stacking context here anymore!
    // The display list now includes PopStackingContext items that match the Push items.

    // Finalize and return display list
    eprintln!("[compositor2] >>>>> CALLING builder.end() <<<<<");
    let (_, dl) = builder.end();
    eprintln!(
        "[compositor2] >>>>> builder.end() RETURNED, dl.size_in_bytes()={} <<<<<",
        dl.size_in_bytes()
    );

    eprintln!(
        "[compositor2] Builder finished, returning ({} resources, display_list, {} nested \
         pipelines)",
        wr_resources.len(),
        nested_pipelines.len()
    );

    // Print detailed display list summary before returning
    eprintln!("Display List Summary:");
    eprintln!("  Pipeline: {:?}", pipeline_id);
    eprintln!("  Viewport: {:?}", viewport_size);
    eprintln!("  Total items in source: {}", display_list.items.len());
    for (idx, item) in display_list.items.iter().enumerate() {
        eprintln!("    Item {}: {:?}", idx + 1, item);
    }
    eprintln!("");

    Ok((wr_resources, dl, nested_pipelines))
}

// Helper Functions

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

// Helper Functions from wr_translate.rs

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
    container_origin: azul_core::geom::LogicalPosition, // Container origin (already scaled)
) {
    let dpi_scale = dpi.inner.get();
    
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

    // Translate glyph positions to absolute coordinates by adding container origin
    // The glyphs have positions relative to (0,0) in LOGICAL pixels
    // We need to:
    // 1. Scale the glyph positions from logical to physical pixels
    // 2. Add the container offset (which is already in physical pixels)
    let wr_glyphs: Vec<_> = glyphs
        .iter()
        .map(|g| webrender::api::GlyphInstance {
            index: g.index,
            point: webrender::api::units::LayoutPoint::new(
                container_origin.x + g.point.x * dpi_scale,
                container_origin.y + g.point.y * dpi_scale,
            ),
        })
        .collect();

    let wr_font_instance_key =
        crate::desktop::wr_translate2::wr_translate_font_instance_key(font_instance_key);
    let wr_color = azul_css::props::basic::color::ColorF::from(color);

    eprintln!(
        "[push_text] ✓ Pushing {} glyphs with FontInstanceKey {:?}, color={:?}, container_origin=({}, {}), dpi={}",
        wr_glyphs.len(),
        wr_font_instance_key,
        wr_color,
        container_origin.x,
        container_origin.y,
        dpi_scale
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
