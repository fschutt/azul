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
        BoxShadowClipMode as WrBoxShadowClipMode,
        BuiltDisplayList as WrBuiltDisplayList, ClipChainId as WrClipChainId,
        ClipMode as WrClipMode, ColorF, CommonItemProperties,
        ComplexClipRegion as WrComplexClipRegion, ConicGradient as WrConicGradient,
        DisplayListBuilder as WrDisplayListBuilder, DocumentId, Epoch, ExtendMode as WrExtendMode,
        ExternalScrollId, FilterOp as WrFilterOp, Gradient as WrGradient,
        GradientStop as WrGradientStop,
        HasScrollLinkedEffect, ItemTag, PipelineId, PrimitiveFlags as WrPrimitiveFlags,
        PropertyBinding, RadialGradient as WrRadialGradient, ReferenceFrameKind,
        Shadow as WrShadow, SpaceAndClipInfo,
        SpatialId, SpatialTreeItemKey, TransformStyle,
    },
    render_api::ResourceUpdate as WrResourceUpdate,
    Transaction,
};

use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::desktop::wr_translate2::{
    translate_image_key, wr_translate_border_radius, wr_translate_color_f,
    wr_translate_logical_size, wr_translate_pipeline_id,
};
use crate::log_debug;

/// Convert logical pixel bounds to physical pixel LayoutRect for WebRender.
/// All display list coordinates are in logical CSS pixels and need to be scaled
/// by the HiDPI factor for correct rendering on HiDPI displays.
#[inline]
fn scale_bounds_to_layout_rect(bounds: &azul_core::geom::LogicalRect, dpi: f32) -> LayoutRect {
    LayoutRect::from_origin_and_size(
        LayoutPoint::new(bounds.origin.x * dpi, bounds.origin.y * dpi),
        LayoutSize::new(bounds.size.width * dpi, bounds.size.height * dpi),
    )
}

/// Convert a [`WindowLogicalRect`] (absolute window coordinates) to a WebRender
/// `LayoutRect` (frame-relative physical pixels) in one step.
///
/// This combines DPI scaling **and** scroll-frame offset subtraction so that
/// callers cannot accidentally forget one of the two conversion steps.
///
/// See `doc/SCROLL_COORDINATE_ARCHITECTURE.md` for design rationale.
#[inline]
fn resolve_rect(
    bounds: &azul_layout::solver3::display_list::WindowLogicalRect,
    dpi: f32,
    offset: (f32, f32),
) -> LayoutRect {
    let raw = scale_bounds_to_layout_rect(bounds.inner(), dpi);
    LayoutRect::from_origin_and_size(
        LayoutPoint::new(raw.min.x - offset.0, raw.min.y - offset.1),
        LayoutSize::new(raw.width(), raw.height()),
    )
}

/// Convert a [`WindowLogicalRect`]'s origin to a WebRender `LayoutPoint`
/// (frame-relative physical pixels), combining DPI scaling and offset subtraction.
///
/// Used for Push* items that only need an origin point (e.g., stacking contexts).
#[inline]
fn resolve_point(
    bounds: &azul_layout::solver3::display_list::WindowLogicalRect,
    dpi: f32,
    offset: (f32, f32),
) -> LayoutPoint {
    LayoutPoint::new(
        bounds.0.origin.x * dpi - offset.0,
        bounds.0.origin.y * dpi - offset.1,
    )
}

/// Scale a single f32 value from logical to physical pixels
#[inline]
fn scale_px(val: f32, dpi: f32) -> f32 {
    val * dpi
}

/// Translate an Azul DisplayList to WebRender DisplayList and resources
/// Returns (resources, display_list, nested_pipelines) tuple that can be added to a transaction
/// by caller. nested_pipelines contains all child virtualized view pipelines that were recursively built.
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
    log_debug!(
        LogCategory::DisplayList,
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

    // Collect nested virtualized view pipelines as we process them
    let mut nested_pipelines: Vec<(PipelineId, WrBuiltDisplayList)> = Vec::new();

    // Create WebRender display list builder
    let mut builder = WrDisplayListBuilder::new(pipeline_id);

    // CRITICAL: Begin building the display list before pushing items
    log_debug!(
        LogCategory::DisplayList,
        "[compositor2] Calling builder.begin()"
    );
    builder.begin();
    log_debug!(
        LogCategory::DisplayList,
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

    // Coordinate offset stack - tracks the origin offset for each spatial context.
    // When we enter a scroll frame, items inside have absolute coordinates but
    // WebRender expects coordinates relative to the scroll frame's content_rect origin.
    // We push the scroll frame's origin when entering, and subtract it from all coordinates.
    let mut offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];

    // Helper to apply current offset to a rect
    let apply_offset = |rect: LayoutRect, offset: (f32, f32)| -> LayoutRect {
        LayoutRect::from_origin_and_size(
            LayoutPoint::new(rect.min.x - offset.0, rect.min.y - offset.1),
            LayoutSize::new(rect.width(), rect.height()),
        )
    };

    // Helper macros to safely get current stack values with defaults
    // This prevents panics when stacks become unexpectedly empty
    macro_rules! current_spatial {
        () => {
            spatial_stack.last().copied().unwrap_or(spatial_id)
        };
    }
    macro_rules! current_clip {
        () => {
            clip_stack.last().copied().unwrap_or(root_clip_chain_id)
        };
    }
    macro_rules! current_offset {
        () => {
            offset_stack.last().copied().unwrap_or((0.0, 0.0))
        };
    }

    // Translate display list items to WebRender
    let mut in_reference_frame = false;
    for item in &display_list.items {
        match item {
            DisplayListItem::Rect {
                bounds,
                color,
                border_radius,
            } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] Rect item: bounds={:?}, color={:?}, dpi={}",
                    bounds,
                    color,
                    dpi_scale
                );
                let rect = resolve_rect(bounds, dpi_scale, current_offset!());

                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                let current_clip_chain = current_clip!();
                let current_spatial = current_spatial!();

                log_debug!(LogCategory::DisplayList,
                    "[CLIP DEBUG] Rect: adjusted={:?}, clip_chain={:?}, spatial={:?}",
                    rect, current_clip_chain, current_spatial
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] Rect push: rect={:?}, clip_chain_id={:?}, spatial_id={:?}, clip_stack.len={}, spatial_stack.len={}",
                    rect, current_clip_chain, current_spatial, clip_stack.len(), spatial_stack.len()
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip_chain,
                    spatial_id: current_spatial,
                    flags: Default::default(),
                };
                // Handle border_radius by creating clip region
                // Note: LogicalRect here is used for clipping calculations, but we pass
                // the scaled rect to WebRender
                let logical_rect = LogicalRect::new(
                    azul_core::geom::LogicalPosition::new(
                        scale_px(bounds.0.origin.x, dpi_scale),
                        scale_px(bounds.0.origin.y, dpi_scale),
                    ),
                    azul_core::geom::LogicalSize::new(
                        scale_px(bounds.0.size.width, dpi_scale),
                        scale_px(bounds.0.size.height, dpi_scale),
                    ),
                );
                let style_border_radius = convert_border_radius_to_style(border_radius);
                let wr_border_radius = wr_translate_border_radius(
                    style_border_radius,
                    azul_core::geom::LogicalSize::new(
                        scale_px(bounds.0.size.width, dpi_scale),
                        scale_px(bounds.0.size.height, dpi_scale),
                    ),
                );

                if !wr_border_radius.is_zero() {
                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        logical_rect,
                        wr_border_radius,
                        current_spatial!(),
                        current_clip!(),
                    );

                    let info_clipped = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: new_clip_id,
                        spatial_id: current_spatial!(),
                        flags: Default::default(),
                    };

                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] push_rect (with clip): {:?}",
                        rect
                    );
                    builder.push_rect(&info_clipped, rect, color_f);
                    continue;
                }

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] push_rect: {:?}",
                    rect
                );
                builder.push_rect(&info, rect, color_f);
            }

            DisplayListItem::SelectionRect {
                bounds,
                border_radius,
                color,
            } => {
                let rect = resolve_rect(bounds, dpi_scale, current_offset!());
                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip!(),
                    spatial_id: current_spatial!(),
                    flags: Default::default(),
                };

                builder.push_rect(&info, rect, color_f);
            }

            DisplayListItem::CursorRect { bounds, color } => {
                let rect = resolve_rect(bounds, dpi_scale, current_offset!());
                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip!(),
                    spatial_id: current_spatial!(),
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
                let rect = resolve_rect(bounds, dpi_scale, current_offset!());

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip!(),
                    spatial_id: current_spatial!(),
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
                // ScrollBars are painted in parent space (after pop_node_clips)
                // Apply current offset to convert from absolute to parent-relative coords
                let current_spatial_id = current_spatial!();
                let current_clip_chain = current_clip!();
                let current_off = current_offset!();
                let rect = resolve_rect(bounds, dpi_scale, current_off);

                // If we have an opacity key, wrap in animatable opacity stacking context
                let has_opacity = opacity_key.is_some();
                if let Some(ok) = opacity_key {
                    let opacity_filter = WrFilterOp::Opacity(
                        PropertyBinding::Binding(
                            webrender::api::PropertyBindingKey::new(ok.id as u64),
                            1.0,
                        ),
                        1.0,
                    );
                    builder.push_simple_stacking_context_with_filters(
                        resolve_point(bounds, dpi_scale, current_off),
                        current_spatial_id,
                        WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                        &[opacity_filter],
                        &[],
                        &[],
                    );
                }

                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] ScrollBar: bounds={:?}, rect={:?}",
                    bounds,
                    rect
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip_chain,
                    spatial_id: current_spatial_id,
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
                        current_clip_chain,
                        current_spatial_id,
                        Default::default(),
                        tag,
                    );
                }

                // Pop opacity stacking context if we pushed one
                if has_opacity {
                    builder.pop_stacking_context();
                }
            }

            DisplayListItem::ScrollBarStyled { info } => {
                let spatial_id = current_spatial!();
                let base_clip_chain_id = current_clip!();
                let current_offset = current_offset!();

                log_debug!(LogCategory::DisplayList,
                    "[compositor2] ScrollBarStyled: bounds={:?}, offset={:?}, track={:?}, thumb={:?}",
                    info.bounds, current_offset, info.track_bounds, info.thumb_bounds
                );

                // If we have an opacity key, wrap the entire scrollbar in an
                // animatable opacity stacking context. WebRender will update the
                // opacity via DynamicProperties without rebuilding the display list.
                let has_opacity_binding = info.opacity_key.is_some();
                if let Some(opacity_key) = &info.opacity_key {
                    let opacity_filter = WrFilterOp::Opacity(
                        PropertyBinding::Binding(
                            webrender::api::PropertyBindingKey::new(opacity_key.id as u64),
                            1.0, // initial opacity (fully visible)
                        ),
                        1.0,
                    );
                    builder.push_simple_stacking_context_with_filters(
                        resolve_point(&info.bounds, dpi_scale, current_offset),
                        spatial_id,
                        WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                        &[opacity_filter],
                        &[],
                        &[],
                    );
                }

                // If clip_to_container_border is enabled and container has border-radius,
                // create a clip chain for the container's rounded corners
                let clip_chain_id =
                    if info.clip_to_container_border && !info.container_border_radius.is_zero() {
                        let style_border_radius =
                            convert_border_radius_to_style(&info.container_border_radius);
                        let wr_border_radius = wr_translate_border_radius(
                            style_border_radius,
                            azul_core::geom::LogicalSize::new(
                                scale_px(info.bounds.0.size.width, dpi_scale),
                                scale_px(info.bounds.0.size.height, dpi_scale),
                            ),
                        );

                        // Apply offset to scaled bounds for clip definition
                        let raw_scaled_bounds = azul_core::geom::LogicalRect::new(
                            azul_core::geom::LogicalPosition::new(
                                scale_px(info.bounds.0.origin.x, dpi_scale),
                                scale_px(info.bounds.0.origin.y, dpi_scale),
                            ),
                            azul_core::geom::LogicalSize::new(
                                scale_px(info.bounds.0.size.width, dpi_scale),
                                scale_px(info.bounds.0.size.height, dpi_scale),
                            ),
                        );
                        let scaled_bounds = azul_core::geom::LogicalRect::new(
                            azul_core::geom::LogicalPosition::new(
                                raw_scaled_bounds.origin.x - current_offset.0,
                                raw_scaled_bounds.origin.y - current_offset.1,
                            ),
                            raw_scaled_bounds.size,
                        );

                        define_border_radius_clip(
                            &mut builder,
                            scaled_bounds,
                            wr_border_radius,
                            spatial_id,
                            base_clip_chain_id,
                        )
                    } else {
                        base_clip_chain_id
                    };

                // Render track (background)
                if info.track_color.a > 0 {
                    let track_rect = resolve_rect(&info.track_bounds, dpi_scale, current_offset);
                    let track_color = ColorF::new(
                        info.track_color.r as f32 / 255.0,
                        info.track_color.g as f32 / 255.0,
                        info.track_color.b as f32 / 255.0,
                        info.track_color.a as f32 / 255.0,
                    );
                    let track_info = CommonItemProperties {
                        clip_rect: track_rect,
                        clip_chain_id,
                        spatial_id,
                        flags: Default::default(),
                    };
                    builder.push_rect(&track_info, track_rect, track_color);

                    // Add hit-test for scrollbar track (for page up/down on click)
                    // This is pushed BEFORE the thumb hit-test so thumb gets priority
                    if let Some(scrollbar_hit_id) = &info.hit_id {
                        use azul_core::hit_test::ScrollbarHitId;
                        // Convert thumb hit_id to track hit_id
                        let track_hit_id = match scrollbar_hit_id {
                            ScrollbarHitId::VerticalThumb(dom_id, node_id) => {
                                ScrollbarHitId::VerticalTrack(*dom_id, *node_id)
                            }
                            ScrollbarHitId::HorizontalThumb(dom_id, node_id) => {
                                ScrollbarHitId::HorizontalTrack(*dom_id, *node_id)
                            }
                            other => *other, // Already a track ID
                        };
                        let (track_tag, _) =
                            crate::desktop::wr_translate2::wr_translate_scrollbar_hit_id(
                                track_hit_id,
                            );
                        builder.push_hit_test(
                            track_rect,
                            clip_chain_id,
                            spatial_id,
                            Default::default(),
                            track_tag,
                        );
                    }
                }

                // Render decrement button (if present)
                if let Some(btn_bounds) = &info.button_decrement_bounds {
                    if info.button_color.a > 0 {
                        let btn_rect = resolve_rect(btn_bounds, dpi_scale, current_offset);
                        let btn_color = ColorF::new(
                            info.button_color.r as f32 / 255.0,
                            info.button_color.g as f32 / 255.0,
                            info.button_color.b as f32 / 255.0,
                            info.button_color.a as f32 / 255.0,
                        );
                        let btn_info = CommonItemProperties {
                            clip_rect: btn_rect,
                            clip_chain_id,
                            spatial_id,
                            flags: Default::default(),
                        };
                        builder.push_rect(&btn_info, btn_rect, btn_color);
                    }
                }

                // Render increment button (if present)
                if let Some(btn_bounds) = &info.button_increment_bounds {
                    if info.button_color.a > 0 {
                        let btn_rect = resolve_rect(btn_bounds, dpi_scale, current_offset);
                        let btn_color = ColorF::new(
                            info.button_color.r as f32 / 255.0,
                            info.button_color.g as f32 / 255.0,
                            info.button_color.b as f32 / 255.0,
                            info.button_color.a as f32 / 255.0,
                        );
                        let btn_info = CommonItemProperties {
                            clip_rect: btn_rect,
                            clip_chain_id,
                            spatial_id,
                            flags: Default::default(),
                        };
                        builder.push_rect(&btn_info, btn_rect, btn_color);
                    }
                }

                // Determine the spatial_id for the thumb. If we have a GPU transform key,
                // wrap in a reference frame for dynamic positioning.
                let thumb_spatial_id = if let Some(transform_key) = &info.thumb_transform_key {
                    // Convert initial transform to WR LayoutTransform with DPI scaling on translation
                    let t = &info.thumb_initial_transform;
                    let wr_transform = LayoutTransform::new(
                        t.m[0][0], t.m[0][1], t.m[0][2], t.m[0][3],
                        t.m[1][0], t.m[1][1], t.m[1][2], t.m[1][3],
                        t.m[2][0], t.m[2][1], t.m[2][2], t.m[2][3],
                        t.m[3][0] * dpi_scale, t.m[3][1] * dpi_scale,
                        t.m[3][2] * dpi_scale, t.m[3][3],
                    );

                    let binding = PropertyBinding::Binding(
                        webrender::api::PropertyBindingKey::new(transform_key.id as u64),
                        wr_transform,
                    );

                    let spatial_key = SpatialTreeItemKey::new(transform_key.id as u64, 0);

                    builder.push_reference_frame(
                        LayoutPoint::zero(),
                        spatial_id,
                        TransformStyle::Flat,
                        binding,
                        ReferenceFrameKind::Transform {
                            is_2d_scale_translation: false,
                            should_snap: false,
                            paired_with_perspective: false,
                        },
                        spatial_key,
                    )
                } else {
                    spatial_id
                };

                // Render thumb (the draggable part)
                if info.thumb_color.a > 0 {
                    let thumb_rect = resolve_rect(&info.thumb_bounds, dpi_scale, current_offset);
                    let thumb_color = ColorF::new(
                        info.thumb_color.r as f32 / 255.0,
                        info.thumb_color.g as f32 / 255.0,
                        info.thumb_color.b as f32 / 255.0,
                        info.thumb_color.a as f32 / 255.0,
                    );

                    // Handle rounded thumb corners
                    if !info.thumb_border_radius.is_zero() {
                        // Scale the border radius by DPI (border radius is in logical pixels)
                        let scaled_border_radius = BorderRadius {
                            top_left: scale_px(info.thumb_border_radius.top_left, dpi_scale),
                            top_right: scale_px(info.thumb_border_radius.top_right, dpi_scale),
                            bottom_left: scale_px(info.thumb_border_radius.bottom_left, dpi_scale),
                            bottom_right: scale_px(
                                info.thumb_border_radius.bottom_right,
                                dpi_scale,
                            ),
                        };
                        let style_border_radius =
                            convert_border_radius_to_style(&scaled_border_radius);
                        let wr_border_radius = wr_translate_border_radius(
                            style_border_radius,
                            azul_core::geom::LogicalSize::new(
                                scale_px(info.thumb_bounds.0.size.width, dpi_scale),
                                scale_px(info.thumb_bounds.0.size.height, dpi_scale),
                            ),
                        );

                        // Create clip for rounded thumb (with offset applied)
                        let scaled_thumb_bounds = azul_core::geom::LogicalRect::new(
                            azul_core::geom::LogicalPosition::new(
                                scale_px(info.thumb_bounds.0.origin.x, dpi_scale) - current_offset.0,
                                scale_px(info.thumb_bounds.0.origin.y, dpi_scale) - current_offset.1,
                            ),
                            azul_core::geom::LogicalSize::new(
                                scale_px(info.thumb_bounds.0.size.width, dpi_scale),
                                scale_px(info.thumb_bounds.0.size.height, dpi_scale),
                            ),
                        );

                        let thumb_clip_id = define_border_radius_clip(
                            &mut builder,
                            scaled_thumb_bounds,
                            wr_border_radius,
                            thumb_spatial_id,
                            clip_chain_id,
                        );

                        let thumb_info = CommonItemProperties {
                            clip_rect: thumb_rect,
                            clip_chain_id: thumb_clip_id,
                            spatial_id: thumb_spatial_id,
                            flags: Default::default(),
                        };
                        builder.push_rect(&thumb_info, thumb_rect, thumb_color);
                    } else {
                        let thumb_info = CommonItemProperties {
                            clip_rect: thumb_rect,
                            clip_chain_id,
                            spatial_id: thumb_spatial_id,
                            flags: Default::default(),
                        };
                        builder.push_rect(&thumb_info, thumb_rect, thumb_color);
                    }
                }

                // Add hit-test for scrollbar thumb
                if let Some(scrollbar_hit_id) = &info.hit_id {
                    let thumb_rect = resolve_rect(&info.thumb_bounds, dpi_scale, current_offset);
                    let (tag, _) = crate::desktop::wr_translate2::wr_translate_scrollbar_hit_id(
                        *scrollbar_hit_id,
                    );
                    builder.push_hit_test(
                        thumb_rect,
                        clip_chain_id,
                        thumb_spatial_id,
                        Default::default(),
                        tag,
                    );
                }

                // Pop the reference frame if we pushed one for the thumb
                if info.thumb_transform_key.is_some() {
                    builder.pop_reference_frame();
                }

                // Pop the opacity stacking context if we pushed one
                if has_opacity_binding {
                    builder.pop_stacking_context();
                }
            }

            DisplayListItem::PushClip {
                bounds,
                border_radius,
            } => {
                let current_offset = current_offset!();
                let rect = resolve_rect(bounds, dpi_scale, current_offset);
                let current_spatial = current_spatial!();
                let current_clip = current_clip!();

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushClip: bounds={:?} -> rect={:?}, spatial_stack.len={}, clip_stack.len={}, \
                     current_spatial={:?}, current_clip={:?}",
                    bounds, rect, spatial_stack.len(), clip_stack.len(), current_spatial, current_clip
                );

                // Handle rounded corners if border_radius is non-zero
                if !border_radius.is_zero() {
                    // Convert layout BorderRadius to StyleBorderRadius for translation
                    let style_border_radius = convert_border_radius_to_style(border_radius);

                    let wr_border_radius = wr_translate_border_radius(
                        style_border_radius,
                        azul_core::geom::LogicalSize::new(
                            scale_px(bounds.0.size.width, dpi_scale),
                            scale_px(bounds.0.size.height, dpi_scale),
                        ),
                    );

                    // Create scaled bounds for clip (offset-corrected for scroll frames)
                    let scaled_bounds = azul_core::geom::LogicalRect::new(
                        azul_core::geom::LogicalPosition::new(
                            scale_px(bounds.0.origin.x, dpi_scale) - current_offset.0,
                            scale_px(bounds.0.origin.y, dpi_scale) - current_offset.1,
                        ),
                        azul_core::geom::LogicalSize::new(
                            scale_px(bounds.0.size.width, dpi_scale),
                            scale_px(bounds.0.size.height, dpi_scale),
                        ),
                    );

                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        scaled_bounds,
                        wr_border_radius,
                        current_spatial,
                        current_clip,
                    );

                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] PushClip (rounded): created new_clip_id={:?}, pushing to clip_stack",
                        new_clip_id
                    );

                    clip_stack.push(new_clip_id);
                } else {
                    // Rectangular clip
                    let clip_id = builder.define_clip_rect(current_spatial, rect);
                    // Create a clip chain from the clip id
                    let parent = if current_clip == WrClipChainId::INVALID {
                        None
                    } else {
                        Some(current_clip)
                    };
                    let new_clip_chain_id = builder.define_clip_chain(parent, vec![clip_id]);

                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] PushClip (rect): clip_id={:?}, parent={:?}, new_clip_chain_id={:?}, pushing to clip_stack",
                        clip_id, parent, new_clip_chain_id
                    );

                    clip_stack.push(new_clip_chain_id);
                }

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushClip DONE: clip_stack.len now = {}",
                    clip_stack.len()
                );
            }

            DisplayListItem::PopClip => {
                let before_len = clip_stack.len();
                if clip_stack.len() > 1 {
                    let popped = clip_stack.pop();
                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] PopClip: popped {:?}, clip_stack.len {} -> {}",
                        popped,
                        before_len,
                        clip_stack.len()
                    );
                } else {
                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] PopClip: SKIPPED (clip_stack.len={}, would underflow)",
                        before_len
                    );
                }
            }

            DisplayListItem::PushScrollFrame {
                clip_bounds,
                content_size,
                scroll_id,
            } => {
                // Create a scroll frame with proper clipping and content size
                // The frame_rect is in PARENT space (where the visible viewport is)
                let frame_rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(
                        scale_px(clip_bounds.0.origin.x, dpi_scale),
                        scale_px(clip_bounds.0.origin.y, dpi_scale),
                    ),
                    LayoutSize::new(
                        scale_px(clip_bounds.0.size.width, dpi_scale),
                        scale_px(clip_bounds.0.size.height, dpi_scale),
                    ),
                );

                // Define scroll frame using WebRender API
                let parent_space = current_spatial!();
                let current_clip = current_clip!();
                let current_offset = current_offset!();
                let external_scroll_id = ExternalScrollId(*scroll_id, pipeline_id);

                // Apply parent offset to frame_rect for correct positioning in parent space
                let adjusted_frame_rect = apply_offset(frame_rect, current_offset);

                // The content_rect is in PARENT space (same coordinate system as frame_rect)
                // Origin should match frame_rect.origin, size is the total scrollable content
                // This ensures that child coordinates (which are in parent-space after offset adjustment)
                // are correctly positioned relative to the scroll frame
                let content_rect = LayoutRect::from_origin_and_size(
                    adjusted_frame_rect.min, // Content origin matches frame origin
                    LayoutSize::new(
                        scale_px(content_size.width, dpi_scale),
                        scale_px(content_size.height, dpi_scale),
                    ),
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushScrollFrame START: frame_rect={:?}, content_rect={:?}, \
                     scroll_id={}, parent_space={:?}, current_clip={:?}, \
                     spatial_stack.len={}, clip_stack.len={}",
                    frame_rect,
                    content_rect,
                    scroll_id,
                    parent_space,
                    current_clip,
                    spatial_stack.len(),
                    clip_stack.len()
                );

                log_debug!(LogCategory::DisplayList,
                    "[CLIP DEBUG] PushScrollFrame: frame_rect={:?}, adjusted={:?}, content_rect={:?}",
                    frame_rect, adjusted_frame_rect, content_rect
                );

                let scroll_spatial_id = builder.define_scroll_frame(
                    parent_space,
                    external_scroll_id,
                    content_rect,
                    adjusted_frame_rect,
                    LayoutVector2D::zero(), // external_scroll_offset
                    0,                      // scroll_offset_generation (APZScrollGeneration)
                    HasScrollLinkedEffect::No,
                    SpatialTreeItemKey::new(*scroll_id, 0),
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushScrollFrame: defined scroll_spatial_id={:?}",
                    scroll_spatial_id
                );

                // Push the new spatial ID onto the stack
                spatial_stack.push(scroll_spatial_id);

                // Fix: Push the scroll frame's origin onto the offset stack.
                // 
                // The layout engine produces primitives with ABSOLUTE WINDOW coordinates.
                // However, WebRender's scroll frame creates a NEW SPATIAL NODE with its own
                // coordinate system, where (0,0) is the top-left of the scrollable content.
                // 
                // To correctly position primitives inside the scroll frame, we need to
                // SUBTRACT the scroll frame's origin from all child coordinates.
                // The apply_offset function does this by subtracting the stacked offsets.
                //
                // Previous approach (keeping same offset) caused content to appear at
                // absolute window position inside the scroll frame, which means the first
                // ~N pixels of content were "above" the scroll frame's viewport.
                //
                // CoordinateSpace transformation: Window -> ScrollFrame
                // Formula: scroll_frame_pos = window_pos - scroll_frame_origin
                let frame_origin_offset = (adjusted_frame_rect.min.x, adjusted_frame_rect.min.y);
                offset_stack.push(frame_origin_offset);

                log_debug!(LogCategory::DisplayList,
                    "[CLIP DEBUG] PushScrollFrame: NEW offset={:?} (scroll frame origin) [CoordinateSpace::Window -> ScrollFrame]",
                    frame_origin_offset
                );

                // Define clip for the scroll frame in PARENT SPACE (where the viewport is)
                // CRITICAL: The clip must be in parent space so it stays stationary while content scrolls!
                // If we define it in scroll space, the clip would scroll with the content (wrong).
                let scroll_clip_id = builder.define_clip_rect(parent_space, adjusted_frame_rect);

                log_debug!(LogCategory::DisplayList,
                    "[CLIP DEBUG] PushScrollFrame: scroll_clip_id={:?}, parent_space={:?}, adjusted_frame_rect={:?}",
                    scroll_clip_id, parent_space, adjusted_frame_rect
                );

                log_debug!(LogCategory::DisplayList,
                    "[compositor2] PushScrollFrame: defined scroll_clip_id={:?} on parent_space={:?} with frame_rect={:?}",
                    scroll_clip_id, parent_space, frame_rect
                );

                // Create a clip chain with this clip, parented to the current clip chain
                let parent_clip = if current_clip == WrClipChainId::INVALID {
                    None
                } else {
                    Some(current_clip)
                };
                let scroll_clip_chain = builder.define_clip_chain(parent_clip, [scroll_clip_id]);

                log_debug!(
                    LogCategory::DisplayList,
                    "[CLIP DEBUG] PushScrollFrame: scroll_clip_chain={:?}, parent_clip={:?}",
                    scroll_clip_chain,
                    parent_clip
                );

                log_debug!(LogCategory::DisplayList,
                    "[compositor2] PushScrollFrame: defined scroll_clip_chain={:?} with parent={:?}",
                    scroll_clip_chain, parent_clip
                );

                clip_stack.push(scroll_clip_chain);

                // Push a hit-test area for this scroll container so the scroll manager can find it
                // during wheel/trackpad events. This uses TAG_TYPE_SCROLL_CONTAINER (0x0500).
                // The tag encodes: tag.0 = scroll_id, tag.1 = 0x0500
                const TAG_TYPE_SCROLL_CONTAINER: u16 = 0x0500;
                let scroll_container_tag: ItemTag = (*scroll_id, TAG_TYPE_SCROLL_CONTAINER);
                builder.push_hit_test(
                    adjusted_frame_rect,
                    scroll_clip_chain, // Use the scroll clip chain we just created
                    parent_space,       // Push in parent space (stationary viewport)
                    WrPrimitiveFlags::default(),
                    scroll_container_tag,
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushScrollFrame: pushed scroll container hit-test tag=({}, 0x{:04x})",
                    scroll_id,
                    TAG_TYPE_SCROLL_CONTAINER
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushScrollFrame DONE: spatial_stack.len={}, clip_stack.len={}",
                    spatial_stack.len(),
                    clip_stack.len()
                );
            }

            DisplayListItem::PopScrollFrame => {
                let spatial_before = spatial_stack.len();
                let clip_before = clip_stack.len();
                let offset_before = offset_stack.len();

                // Pop spatial, clip, and offset stacks
                let popped_clip = clip_stack.pop();
                let popped_spatial = spatial_stack.pop();
                let popped_offset = offset_stack.pop();

                log_debug!(
                    LogCategory::DisplayList,
                    "[CLIP DEBUG] PopScrollFrame: popped_offset={:?}",
                    popped_offset
                );

                log_debug!(LogCategory::DisplayList,
                    "[compositor2] PopScrollFrame: popped_clip={:?}, popped_spatial={:?}, popped_offset={:?}, \
                     spatial_stack {} -> {}, clip_stack {} -> {}, offset_stack {} -> {}",
                    popped_clip, popped_spatial, popped_offset,
                    spatial_before, spatial_stack.len(),
                    clip_before, clip_stack.len(),
                    offset_before, offset_stack.len()
                );

                if spatial_stack.is_empty() || clip_stack.is_empty() || offset_stack.is_empty() {
                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] ERROR: PopScrollFrame caused stack underflow"
                    );
                    return Err("Scroll frame stack underflow".to_string());
                }
            }

            DisplayListItem::HitTestArea { bounds, tag } => {
                let rect = resolve_rect(bounds, dpi_scale, current_offset!());

                // DEBUG: Draw a semi-transparent red rectangle to visualize hit-test areas
                #[cfg(debug_assertions)]
                {
                    let debug_color = ColorF::new(1.0, 0.0, 0.0, 0.3); // Red with 30% opacity
                    let debug_info = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: current_clip!(),
                        spatial_id: current_spatial!(),
                        flags: Default::default(),
                    };
                    builder.push_rect(&debug_info, rect, debug_color);
                }

                // Use the tag directly - it's already a (u64, u16) tuple
                // where u16 contains the namespace marker:
                // - 0x0100 = DOM Node (regular interactive elements)
                // - 0x0200 = Scrollbar component
                let item_tag: ItemTag = *tag;

                // Use WebRender's push_hit_test to create a hittable area
                // This is required for the hit tester to find items at cursor position
                builder.push_hit_test(
                    rect,
                    current_clip!(),
                    current_spatial!(),
                    WrPrimitiveFlags::default(),
                    item_tag,
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] HitTestArea: bounds={:?}, tag={:?}",
                    bounds,
                    tag
                );
            }

            DisplayListItem::Underline {
                bounds,
                color,
                thickness,
            } => {
                let raw_rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(
                        scale_px(bounds.0.origin.x, dpi_scale),
                        scale_px(bounds.0.origin.y, dpi_scale),
                    ),
                    LayoutSize::new(
                        scale_px(bounds.0.size.width, dpi_scale),
                        scale_px(*thickness, dpi_scale),
                    ),
                );
                let rect = apply_offset(raw_rect, current_offset!());
                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip!(),
                    spatial_id: current_spatial!(),
                    flags: Default::default(),
                };

                builder.push_rect(&info, rect, color_f);
            }

            DisplayListItem::Strikethrough {
                bounds,
                color,
                thickness,
            } => {
                let raw_rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(
                        scale_px(bounds.0.origin.x, dpi_scale),
                        scale_px(bounds.0.origin.y, dpi_scale),
                    ),
                    LayoutSize::new(
                        scale_px(bounds.0.size.width, dpi_scale),
                        scale_px(*thickness, dpi_scale),
                    ),
                );
                let rect = apply_offset(raw_rect, current_offset!());
                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip!(),
                    spatial_id: current_spatial!(),
                    flags: Default::default(),
                };

                builder.push_rect(&info, rect, color_f);
            }

            DisplayListItem::Overline {
                bounds,
                color,
                thickness,
            } => {
                let raw_rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(
                        scale_px(bounds.0.origin.x, dpi_scale),
                        scale_px(bounds.0.origin.y, dpi_scale),
                    ),
                    LayoutSize::new(
                        scale_px(bounds.0.size.width, dpi_scale),
                        scale_px(*thickness, dpi_scale),
                    ),
                );
                let rect = apply_offset(raw_rect, current_offset!());
                let color_f = ColorF::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                );

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip!(),
                    spatial_id: current_spatial!(),
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
                log_debug!(LogCategory::DisplayList,
                    "[compositor2] Text item: {} glyphs, font_size={}, color={:?}, clip_rect={:?}, dpi={}",
                    glyphs.len(),
                    font_size_px,
                    color,
                    clip_rect,
                    dpi_scale
                );

                // Log first few glyph positions for debugging
                if !glyphs.is_empty() {
                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] First 3 glyphs (before scaling):"
                    );
                    for (i, g) in glyphs.iter().take(3).enumerate() {
                        log_debug!(
                            LogCategory::DisplayList,
                            "  [{}] index={}, pos=({}, {})",
                            i,
                            g.index,
                            g.point.x,
                            g.point.y
                        );
                    }
                }

                // Scale clip_rect from logical to physical pixels, then apply
                // the offset stack so text coordinates are relative to the
                // current scroll frame (matching how Rect items are handled).
                let current_offset = current_offset!();
                let rect = resolve_rect(clip_rect, dpi_scale, current_offset);

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip!(),
                    spatial_id: current_spatial!(),
                    flags: Default::default(),
                };

                // Use push_text helper with font_hash lookup
                // NOTE: font_size_px is logical, Au conversion and DPI scaling happens in
                // translate_add_font_instance
                let font_size_au = azul_core::resources::Au::from_px(*font_size_px);

                // Scale container origin for glyph positioning, then subtract
                // the offset so glyphs land inside the scroll frame.
                let scaled_origin = azul_core::geom::LogicalPosition::new(
                    scale_px(clip_rect.0.origin.x, dpi_scale) - current_offset.0,
                    scale_px(clip_rect.0.origin.y, dpi_scale) - current_offset.1,
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
                    scaled_origin, // Pass offset-corrected origin to position glyphs
                    current_offset, // Pass scroll frame offset for glyph position correction
                );
            }

            DisplayListItem::Image { bounds, image } => {
                // Get the ImageRefHash from the ImageRef
                let image_ref_hash = image.get_hash();

                if let Some(resolved_image) = renderer_resources.get_image(&image_ref_hash) {
                    let wr_image_key = translate_image_key(resolved_image.key);

                    let rect = resolve_rect(bounds, dpi_scale, current_offset!());

                    let info = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: current_clip!(),
                        spatial_id: current_spatial!(),
                        flags: Default::default(),
                    };

                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] push_image: bounds={:?}, key={:?}",
                        bounds,
                        wr_image_key
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
                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] WARNING: Image key {:?} not found in renderer_resources",
                        image_ref_hash
                    );
                }
            }

            DisplayListItem::PushStackingContext { z_index, bounds } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushStackingContext: z_index={}, bounds={:?}, dpi={}",
                    z_index,
                    bounds,
                    dpi_scale
                );

                // Push a simple stacking context at the bounds origin
                // (offset-corrected so it's relative to the current scroll frame)
                let current_spatial_id = current_spatial!();
                let current_offset = current_offset!();
                let scaled_origin = resolve_point(bounds, dpi_scale, current_offset);
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] >>>>> push_simple_stacking_context at ({}, {}) <<<<<",
                    scaled_origin.x,
                    scaled_origin.y
                );
                builder.push_simple_stacking_context(
                    scaled_origin,
                    current_spatial_id,
                    WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                );
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] >>>>> push_simple_stacking_context RETURNED <<<<<"
                );
            }

            DisplayListItem::PopStackingContext => {
                log_debug!(LogCategory::DisplayList, "[compositor2] PopStackingContext");
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] >>>>> pop_stacking_context <<<<<"
                );
                builder.pop_stacking_context();
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] >>>>> pop_stacking_context RETURNED <<<<<"
                );
            }

            DisplayListItem::PushReferenceFrame {
                transform_key,
                initial_transform,
                bounds,
            } => {
                // GPU-accelerated transform reference frame.
                // This creates a new spatial node with a PropertyBinding for the transform,
                // allowing WebRender to animate the transform via append_dynamic_properties
                // without rebuilding the display list.

                let parent_spatial_id = current_spatial!();

                // Convert ComputedTransform3D (row-major [[f32;4];4]) to WR LayoutTransform.
                // IMPORTANT: Scale the translation components (m[3][0], m[3][1], m[3][2]) by DPI
                // because the transform operates in the same coordinate space as display list
                // items, which are all scaled from CSS (logical) pixels to physical pixels.
                let wr_transform = LayoutTransform::new(
                    initial_transform.m[0][0], initial_transform.m[0][1],
                    initial_transform.m[0][2], initial_transform.m[0][3],
                    initial_transform.m[1][0], initial_transform.m[1][1],
                    initial_transform.m[1][2], initial_transform.m[1][3],
                    initial_transform.m[2][0], initial_transform.m[2][1],
                    initial_transform.m[2][2], initial_transform.m[2][3],
                    initial_transform.m[3][0] * dpi_scale, initial_transform.m[3][1] * dpi_scale,
                    initial_transform.m[3][2] * dpi_scale, initial_transform.m[3][3],
                );

                // Use PropertyBinding::Binding so we can update this transform
                // dynamically via append_dynamic_properties
                let binding = PropertyBinding::Binding(
                    webrender::api::PropertyBindingKey::new(transform_key.id as u64),
                    wr_transform,
                );

                // Use the transform_key.id as the spatial tree item key
                let spatial_key = SpatialTreeItemKey::new(transform_key.id as u64, 0);

                // Push reference frame at ZERO origin.
                // The reference frame is purely for the dynamic transform (drag delta / CSS transform).
                // Items inside keep their absolute (DPI-scaled) coordinates.
                // We do NOT shift the coordinate origin - the transform handles all movement.
                let new_spatial_id = builder.push_reference_frame(
                    LayoutPoint::zero(),
                    parent_spatial_id,
                    TransformStyle::Flat,
                    binding,
                    ReferenceFrameKind::Transform {
                        is_2d_scale_translation: false,
                        should_snap: false,
                        paired_with_perspective: false,
                    },
                    spatial_key,
                );

                // Push the new spatial ID so all children use this transform space.
                // NO offset push - origin is (0,0), items keep absolute coordinates.
                spatial_stack.push(new_spatial_id);

                in_reference_frame = true;
            }

            DisplayListItem::PopReferenceFrame => {
                in_reference_frame = false;
                builder.pop_reference_frame();
                spatial_stack.pop();
                // NO offset_stack.pop() - we didn't push one
            }

            DisplayListItem::VirtualizedView {
                child_dom_id,
                bounds,
                clip_rect,
            } => {
                // VirtualizedView rendering implementation:
                // 1. Create PipelineId from child_dom_id
                // 2. Look up child display list from layout_results
                // 3. Recursively translate child display list
                // 4. Store child pipeline for later registration
                // 5. Push virtualized view to current display list

                let child_pipeline_id = wr_translate_pipeline_id(AzulPipelineId(
                    child_dom_id.inner as u32,
                    document_id,
                ));

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] VirtualizedView: child_dom_id={:?}, child_pipeline_id={:?}, \
                     bounds={:?}, clip_rect={:?}",
                    child_dom_id,
                    child_pipeline_id,
                    bounds,
                    clip_rect
                );

                // Look up child layout result
                if let Some(child_layout_result) = layout_results.get(child_dom_id) {
                    log_debug!(
                        LogCategory::DisplayList,
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
                            log_debug!(
                                LogCategory::DisplayList,
                                "[compositor2] Successfully translated child display list with {} \
                                 nested pipelines",
                                child_nested.len()
                            );

                            // Store this child pipeline
                            nested_pipelines.push((child_pipeline_id, child_dl));

                            // Store all deeply nested pipelines
                            nested_pipelines.append(&mut child_nested);

                            // Push virtualized view to current display list
                            let space_and_clip = SpaceAndClipInfo {
                                spatial_id: current_spatial!(),
                                clip_chain_id: current_clip!(),
                            };

                            let wr_bounds = scale_bounds_to_layout_rect(bounds.inner(), dpi_scale);
                            let wr_clip_rect = scale_bounds_to_layout_rect(clip_rect.inner(), dpi_scale);

                            builder.push_iframe(
                                wr_bounds,
                                wr_clip_rect,
                                &space_and_clip,
                                child_pipeline_id,
                                false, // ignore_missing_pipeline
                            );

                            log_debug!(
                                LogCategory::DisplayList,
                                "[compositor2] Pushed virtualized view for pipeline {:?}",
                                child_pipeline_id
                            );
                        }
                        Err(e) => {
                            log_debug!(
                                LogCategory::DisplayList,
                                "[compositor2] Error translating child display list for \
                                 dom_id={:?}: {}",
                                child_dom_id,
                                e
                            );
                        }
                    }
                } else {
                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] WARNING: Child DOM {:?} not found in layout_results",
                        child_dom_id
                    );
                }
            }

            DisplayListItem::VirtualizedViewPlaceholder { node_id, .. } => {
                // VirtualizedViewPlaceholder should have been replaced by VirtualizedView in window.rs.
                // If we reach here, the VirtualizedView callback was not invoked for this node.
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] WARNING: VirtualizedViewPlaceholder for node {:?} was not replaced",
                    node_id
                );
            }

            DisplayListItem::TextLayout { .. } => {
                // TextLayout items are handled elsewhere (via PushCachedTextRuns)
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] TextLayout item (handled via cached text runs)"
                );
            }

            // gradient rendering
            DisplayListItem::LinearGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] LinearGradient: bounds={:?}",
                    bounds
                );

                // Convert CSS gradient to WebRender gradient
                let rect = resolve_rect(bounds, dpi_scale, current_offset!());

                // Create layout rect for computing gradient points (use scaled size)
                use azul_css::props::basic::{
                    LayoutPoint as CssLayoutPoint, LayoutRect as CssLayoutRect,
                    LayoutSize as CssLayoutSize,
                };
                let scaled_width = scale_px(bounds.0.size.width, dpi_scale);
                let scaled_height = scale_px(bounds.0.size.height, dpi_scale);
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
                                azul_css::props::basic::color::ColorF::from(stop.color.to_color_u_default()),
                            ),
                        }
                    })
                    .collect();

                // Create WebRender gradient
                let wr_gradient = WrGradient {
                    start_point,
                    end_point,
                    extend_mode,
                };

                // Handle border-radius clipping
                let style_border_radius = convert_border_radius_to_style(border_radius);
                let wr_border_radius = wr_translate_border_radius(
                    style_border_radius,
                    azul_core::geom::LogicalSize::new(scaled_width, scaled_height),
                );

                let tile_size = LayoutSize::new(scaled_width, scaled_height);
                let tile_spacing = LayoutSize::zero();

                if !wr_border_radius.is_zero() {
                    let logical_rect = LogicalRect::new(
                        azul_core::geom::LogicalPosition::new(
                            scale_px(bounds.0.origin.x, dpi_scale),
                            scale_px(bounds.0.origin.y, dpi_scale),
                        ),
                        azul_core::geom::LogicalSize::new(scaled_width, scaled_height),
                    );
                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        logical_rect,
                        wr_border_radius,
                        current_spatial!(),
                        current_clip!(),
                    );
                    let info = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: new_clip_id,
                        spatial_id: current_spatial!(),
                        flags: Default::default(),
                    };
                    // Push stops immediately before gradient to avoid clip items interleaving
                    builder.push_stops(&wr_stops);
                    builder.push_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
                } else {
                    let info = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: current_clip!(),
                        spatial_id: current_spatial!(),
                        flags: Default::default(),
                    };
                    builder.push_stops(&wr_stops);
                    builder.push_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
                }
            }

            DisplayListItem::RadialGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] RadialGradient: bounds={:?}",
                    bounds
                );

                let rect = resolve_rect(bounds, dpi_scale, current_offset!());
                let scaled_width = scale_px(bounds.0.size.width, dpi_scale);
                let scaled_height = scale_px(bounds.0.size.height, dpi_scale);

                // Compute center based on background position (in DPI-scaled coordinates)
                use azul_css::props::style::background::{
                    BackgroundPositionHorizontal, BackgroundPositionVertical,
                };
                let center_x = match &gradient.position.horizontal {
                    BackgroundPositionHorizontal::Left => 0.0,
                    BackgroundPositionHorizontal::Center => scaled_width / 2.0,
                    BackgroundPositionHorizontal::Right => scaled_width,
                    BackgroundPositionHorizontal::Exact(px) => {
                        scale_px(px.to_pixels_internal(bounds.0.size.width, 16.0), dpi_scale)
                    }
                };
                let center_y = match &gradient.position.vertical {
                    BackgroundPositionVertical::Top => 0.0,
                    BackgroundPositionVertical::Center => scaled_height / 2.0,
                    BackgroundPositionVertical::Bottom => scaled_height,
                    BackgroundPositionVertical::Exact(px) => {
                        scale_px(px.to_pixels_internal(bounds.0.size.height, 16.0), dpi_scale)
                    }
                };
                let center = LayoutPoint::new(center_x, center_y);

                // Compute radius based on shape and size keyword (in DPI-scaled coordinates)
                use azul_css::props::style::background::RadialGradientSize;
                let radius = match (&gradient.shape, &gradient.size) {
                    // Circle: same radius in both directions
                    (
                        azul_css::props::style::background::Shape::Circle,
                        RadialGradientSize::ClosestSide,
                    ) => {
                        let r = center_x
                            .min(center_y)
                            .min(scaled_width - center_x)
                            .min(scaled_height - center_y);
                        LayoutSize::new(r, r)
                    }
                    (
                        azul_css::props::style::background::Shape::Circle,
                        RadialGradientSize::FarthestSide,
                    ) => {
                        let r = center_x
                            .max(center_y)
                            .max(scaled_width - center_x)
                            .max(scaled_height - center_y);
                        LayoutSize::new(r, r)
                    }
                    (
                        azul_css::props::style::background::Shape::Circle,
                        RadialGradientSize::ClosestCorner,
                    ) => {
                        let dx = center_x.min(scaled_width - center_x);
                        let dy = center_y.min(scaled_height - center_y);
                        let r = (dx * dx + dy * dy).sqrt();
                        LayoutSize::new(r, r)
                    }
                    (
                        azul_css::props::style::background::Shape::Circle,
                        RadialGradientSize::FarthestCorner,
                    ) => {
                        let dx = center_x.max(scaled_width - center_x);
                        let dy = center_y.max(scaled_height - center_y);
                        let r = (dx * dx + dy * dy).sqrt();
                        LayoutSize::new(r, r)
                    }
                    // Ellipse: different radius for x and y
                    (
                        azul_css::props::style::background::Shape::Ellipse,
                        RadialGradientSize::ClosestSide,
                    ) => {
                        let rx = center_x.min(scaled_width - center_x);
                        let ry = center_y.min(scaled_height - center_y);
                        LayoutSize::new(rx, ry)
                    }
                    (
                        azul_css::props::style::background::Shape::Ellipse,
                        RadialGradientSize::FarthestSide,
                    ) => {
                        let rx = center_x.max(scaled_width - center_x);
                        let ry = center_y.max(scaled_height - center_y);
                        LayoutSize::new(rx, ry)
                    }
                    (
                        azul_css::props::style::background::Shape::Ellipse,
                        RadialGradientSize::ClosestCorner,
                    ) => {
                        let dx = center_x.min(scaled_width - center_x);
                        let dy = center_y.min(scaled_height - center_y);
                        LayoutSize::new(dx, dy)
                    }
                    (
                        azul_css::props::style::background::Shape::Ellipse,
                        RadialGradientSize::FarthestCorner,
                    ) => {
                        let dx = center_x.max(scaled_width - center_x);
                        let dy = center_y.max(scaled_height - center_y);
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
                            stop.color.to_color_u_default(),
                        )),
                    })
                    .collect();

                let wr_gradient = WrRadialGradient {
                    center,
                    radius,
                    start_offset: 0.0,
                    end_offset: 1.0,
                    extend_mode,
                };

                // Handle border-radius clipping
                let style_border_radius = convert_border_radius_to_style(border_radius);
                let wr_border_radius = wr_translate_border_radius(
                    style_border_radius,
                    azul_core::geom::LogicalSize::new(scaled_width, scaled_height),
                );

                let tile_size = LayoutSize::new(scaled_width, scaled_height);
                let tile_spacing = LayoutSize::zero();

                if !wr_border_radius.is_zero() {
                    let logical_rect = LogicalRect::new(
                        azul_core::geom::LogicalPosition::new(
                            scale_px(bounds.0.origin.x, dpi_scale),
                            scale_px(bounds.0.origin.y, dpi_scale),
                        ),
                        azul_core::geom::LogicalSize::new(scaled_width, scaled_height),
                    );
                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        logical_rect,
                        wr_border_radius,
                        current_spatial!(),
                        current_clip!(),
                    );
                    let info = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: new_clip_id,
                        spatial_id: current_spatial!(),
                        flags: Default::default(),
                    };
                    // Push stops immediately before gradient to avoid clip items interleaving
                    builder.push_stops(&wr_stops);
                    builder.push_radial_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
                } else {
                    let info = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: current_clip!(),
                        spatial_id: current_spatial!(),
                        flags: Default::default(),
                    };
                    builder.push_stops(&wr_stops);
                    builder.push_radial_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
                }
            }

            DisplayListItem::ConicGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] ConicGradient: bounds={:?}",
                    bounds
                );

                let rect = resolve_rect(bounds, dpi_scale, current_offset!());
                let scaled_width = scale_px(bounds.0.size.width, dpi_scale);
                let scaled_height = scale_px(bounds.0.size.height, dpi_scale);

                // Compute center based on CSS position (in DPI-scaled coordinates)
                use azul_css::props::style::background::{
                    BackgroundPositionHorizontal, BackgroundPositionVertical,
                };
                let center_x = match &gradient.center.horizontal {
                    BackgroundPositionHorizontal::Left => 0.0,
                    BackgroundPositionHorizontal::Center => scaled_width / 2.0,
                    BackgroundPositionHorizontal::Right => scaled_width,
                    BackgroundPositionHorizontal::Exact(px) => {
                        scale_px(px.to_pixels_internal(bounds.0.size.width, 16.0), dpi_scale)
                    }
                };
                let center_y = match &gradient.center.vertical {
                    BackgroundPositionVertical::Top => 0.0,
                    BackgroundPositionVertical::Center => scaled_height / 2.0,
                    BackgroundPositionVertical::Bottom => scaled_height,
                    BackgroundPositionVertical::Exact(px) => {
                        scale_px(px.to_pixels_internal(bounds.0.size.height, 16.0), dpi_scale)
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
                                azul_css::props::basic::color::ColorF::from(stop.color.to_color_u_default()),
                            ),
                        }
                    })
                    .collect();

                let wr_gradient = WrConicGradient {
                    center,
                    angle,
                    start_offset: 0.0,
                    end_offset: 1.0,
                    extend_mode,
                };

                // Handle border-radius clipping
                let style_border_radius = convert_border_radius_to_style(border_radius);
                let wr_border_radius = wr_translate_border_radius(
                    style_border_radius,
                    azul_core::geom::LogicalSize::new(scaled_width, scaled_height),
                );

                let tile_size = LayoutSize::new(scaled_width, scaled_height);
                let tile_spacing = LayoutSize::zero();

                if !wr_border_radius.is_zero() {
                    let logical_rect = LogicalRect::new(
                        azul_core::geom::LogicalPosition::new(
                            scale_px(bounds.0.origin.x, dpi_scale),
                            scale_px(bounds.0.origin.y, dpi_scale),
                        ),
                        azul_core::geom::LogicalSize::new(scaled_width, scaled_height),
                    );
                    let new_clip_id = define_border_radius_clip(
                        &mut builder,
                        logical_rect,
                        wr_border_radius,
                        current_spatial!(),
                        current_clip!(),
                    );
                    let info = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: new_clip_id,
                        spatial_id: current_spatial!(),
                        flags: Default::default(),
                    };
                    // Push stops immediately before gradient to avoid clip items interleaving
                    builder.push_stops(&wr_stops);
                    builder.push_conic_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
                } else {
                    let info = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: current_clip!(),
                        spatial_id: current_spatial!(),
                        flags: Default::default(),
                    };
                    builder.push_stops(&wr_stops);
                    builder.push_conic_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
                }
            }

            // box shadow
            DisplayListItem::BoxShadow {
                bounds,
                shadow,
                border_radius,
            } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] BoxShadow: bounds={:?}, shadow={:?}",
                    bounds,
                    shadow
                );

                let rect = resolve_rect(bounds, dpi_scale, current_offset!());
                let scaled_width = scale_px(bounds.0.size.width, dpi_scale);
                let scaled_height = scale_px(bounds.0.size.height, dpi_scale);

                let offset = LayoutVector2D::new(
                    scale_px(shadow.offset_x.inner.to_pixels_internal(0.0, 16.0), dpi_scale),
                    scale_px(shadow.offset_y.inner.to_pixels_internal(0.0, 16.0), dpi_scale),
                );
                let color_f =
                    wr_translate_color_f(azul_css::props::basic::color::ColorF::from(shadow.color));
                let blur_radius = scale_px(
                    shadow.blur_radius.inner.to_pixels_internal(0.0, 16.0),
                    dpi_scale,
                );
                let spread_radius = scale_px(
                    shadow.spread_radius.inner.to_pixels_internal(0.0, 16.0),
                    dpi_scale,
                );

                let style_border_radius = convert_border_radius_to_style(border_radius);
                let wr_border_radius = wr_translate_border_radius(
                    style_border_radius,
                    azul_core::geom::LogicalSize::new(scaled_width, scaled_height),
                );

                let clip_mode = match shadow.clip_mode {
                    azul_css::props::style::box_shadow::BoxShadowClipMode::Outset => {
                        WrBoxShadowClipMode::Outset
                    }
                    azul_css::props::style::box_shadow::BoxShadowClipMode::Inset => {
                        WrBoxShadowClipMode::Inset
                    }
                };

                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip!(),
                    spatial_id: current_spatial!(),
                    flags: Default::default(),
                };
                builder.push_box_shadow(
                    &info,
                    rect,
                    offset,
                    color_f,
                    blur_radius,
                    spread_radius,
                    wr_border_radius,
                    clip_mode,
                );
            }

            // filter effects
            DisplayListItem::PushFilter { bounds, filters } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushFilter: bounds={:?}, {} filters",
                    bounds,
                    filters.len()
                );
                let wr_filters = translate_style_filters_to_wr(filters, dpi_scale);
                let current_spatial_id = current_spatial!();
                let current_offset = current_offset!();
                builder.push_simple_stacking_context_with_filters(
                    resolve_point(bounds, dpi_scale, current_offset),
                    current_spatial_id,
                    WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                    &wr_filters,
                    &[],
                    &[],
                );
            }
            DisplayListItem::PopFilter => {
                log_debug!(LogCategory::DisplayList, "[compositor2] PopFilter");
                builder.pop_stacking_context();
            }

            DisplayListItem::PushBackdropFilter { bounds, filters } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushBackdropFilter: bounds={:?}, {} filters",
                    bounds,
                    filters.len()
                );
                let wr_filters = translate_style_filters_to_wr(filters, dpi_scale);
                let rect = resolve_rect(bounds, dpi_scale, current_offset!());
                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: current_clip!(),
                    spatial_id: current_spatial!(),
                    flags: Default::default(),
                };
                builder.push_backdrop_filter(&info, &wr_filters, &[], &[]);
            }
            DisplayListItem::PopBackdropFilter => {
                log_debug!(LogCategory::DisplayList, "[compositor2] PopBackdropFilter");
                // backdrop_filter doesn't push a stacking context, no pop needed
            }

            DisplayListItem::PushOpacity { bounds, opacity } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushOpacity: bounds={:?}, opacity={}",
                    bounds,
                    opacity
                );
                let current_spatial_id = current_spatial!();
                let current_offset = current_offset!();
                let opacity_filter = WrFilterOp::Opacity(
                    PropertyBinding::Value(*opacity),
                    *opacity,
                );
                builder.push_simple_stacking_context_with_filters(
                    resolve_point(bounds, dpi_scale, current_offset),
                    current_spatial_id,
                    WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                    &[opacity_filter],
                    &[],
                    &[],
                );
            }
            DisplayListItem::PopOpacity => {
                log_debug!(LogCategory::DisplayList, "[compositor2] PopOpacity");
                builder.pop_stacking_context();
            }
            DisplayListItem::PushTextShadow { shadow } => {
                log_debug!(LogCategory::DisplayList, "[compositor2] PushTextShadow: {:?}", shadow);
                let current_spatial_id = current_spatial!();
                let current_clip_chain = current_clip!();
                let offset_x = shadow.offset_x.inner.to_pixels_internal(0.0, 16.0) * dpi_scale;
                let offset_y = shadow.offset_y.inner.to_pixels_internal(0.0, 16.0) * dpi_scale;
                let blur_radius = shadow.blur_radius.inner.to_pixels_internal(0.0, 16.0) * dpi_scale;
                let wr_shadow = WrShadow {
                    offset: LayoutVector2D::new(offset_x, offset_y),
                    color: ColorF::new(
                        shadow.color.r as f32 / 255.0,
                        shadow.color.g as f32 / 255.0,
                        shadow.color.b as f32 / 255.0,
                        shadow.color.a as f32 / 255.0,
                    ),
                    blur_radius,
                };
                let space_and_clip = SpaceAndClipInfo {
                    spatial_id: current_spatial_id,
                    clip_chain_id: current_clip_chain,
                };
                builder.push_shadow(&space_and_clip, wr_shadow, true);
            }
            DisplayListItem::PopTextShadow => {
                log_debug!(LogCategory::DisplayList, "[compositor2] PopTextShadow");
                builder.pop_all_shadows();
            }
        }
    }

    // NOTE: We DON'T pop a stacking context here anymore!
    // The display list now includes PopStackingContext items that match the Push items.

    // Finalize and return display list
    log_debug!(
        LogCategory::DisplayList,
        "[compositor2] >>>>> CALLING builder.end() <<<<<"
    );
    let (_, dl) = builder.end();
    log_debug!(
        LogCategory::DisplayList,
        "[compositor2] >>>>> builder.end() RETURNED, dl.size_in_bytes()={} <<<<<",
        dl.size_in_bytes()
    );

    log_debug!(
        LogCategory::DisplayList,
        "[compositor2] Builder finished, returning ({} resources, display_list, {} nested \
         pipelines)",
        wr_resources.len(),
        nested_pipelines.len()
    );

    // Print detailed display list summary before returning
    log_debug!(LogCategory::DisplayList, "Display List Summary:");
    log_debug!(LogCategory::DisplayList, "  Pipeline: {:?}", pipeline_id);
    log_debug!(LogCategory::DisplayList, "  Viewport: {:?}", viewport_size);
    log_debug!(
        LogCategory::DisplayList,
        "  Total items in source: {}",
        display_list.items.len()
    );
    for (idx, item) in display_list.items.iter().enumerate() {
        log_debug!(LogCategory::DisplayList, "    Item {}: {:?}", idx + 1, item);
    }
    log_debug!(LogCategory::DisplayList, "");

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
    // Create the clip at the actual position of the element
    let wr_layout_rect = LayoutRect::from_origin_and_size(
        LayoutPoint::new(layout_rect.origin.x, layout_rect.origin.y),
        LayoutSize::new(layout_rect.size.width, layout_rect.size.height),
    );

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

/// Convert azul StyleFilter list to WebRender FilterOp list
fn translate_style_filters_to_wr(
    filters: &[azul_css::props::style::filter::StyleFilter],
    dpi_scale: f32,
) -> Vec<WrFilterOp> {
    use azul_css::props::style::filter::StyleFilter;

    filters
        .iter()
        .filter_map(|f| match f {
            StyleFilter::Blur(blur) => {
                let w = scale_px(blur.width.to_pixels_internal(0.0, 16.0), dpi_scale);
                let h = scale_px(blur.height.to_pixels_internal(0.0, 16.0), dpi_scale);
                Some(WrFilterOp::Blur(w, h))
            }
            StyleFilter::Opacity(o) => {
                let v = o.normalized();
                Some(WrFilterOp::Opacity(PropertyBinding::Value(v), v))
            }
            StyleFilter::Brightness(v) => Some(WrFilterOp::Brightness(v.normalized())),
            StyleFilter::Contrast(v) => Some(WrFilterOp::Contrast(v.normalized())),
            StyleFilter::Grayscale(v) => Some(WrFilterOp::Grayscale(v.normalized())),
            StyleFilter::HueRotate(a) => Some(WrFilterOp::HueRotate(a.to_degrees())),
            StyleFilter::Invert(v) => Some(WrFilterOp::Invert(v.normalized())),
            StyleFilter::Saturate(v) => Some(WrFilterOp::Saturate(v.normalized())),
            StyleFilter::Sepia(v) => Some(WrFilterOp::Sepia(v.normalized())),
            StyleFilter::ColorMatrix(m) => {
                let vals = m.as_slice();
                let mut arr = [0.0f32; 20];
                for (i, v) in vals.iter().enumerate() {
                    arr[i] = v.get();
                }
                Some(WrFilterOp::ColorMatrix(arr))
            }
            StyleFilter::DropShadow(s) => {
                let offset = LayoutVector2D::new(
                    scale_px(s.offset_x.inner.to_pixels_internal(0.0, 16.0), dpi_scale),
                    scale_px(s.offset_y.inner.to_pixels_internal(0.0, 16.0), dpi_scale),
                );
                let color = wr_translate_color_f(
                    azul_css::props::basic::color::ColorF::from(s.color),
                );
                let blur_radius = scale_px(
                    s.blur_radius.inner.to_pixels_internal(0.0, 16.0),
                    dpi_scale,
                );
                Some(WrFilterOp::DropShadow(WrShadow {
                    offset,
                    color,
                    blur_radius,
                }))
            }
            StyleFilter::Flood(color) => {
                let c = wr_translate_color_f(
                    azul_css::props::basic::color::ColorF::from(*color),
                );
                Some(WrFilterOp::Flood(c))
            }
            StyleFilter::Blend(_) | StyleFilter::ComponentTransfer
            | StyleFilter::Offset(_) | StyleFilter::Composite(_) => {
                // These SVG-specific filters don't map directly to WR FilterOp
                None
            }
        })
        .collect()
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
    scroll_offset: (f32, f32), // Offset to subtract from glyph positions for scroll frames
) {
    let dpi_scale = dpi.inner.get();

    // Look up FontKey from the font_hash (which comes from the GlyphRun)
    // The font_hash is the hash of FontRef computed during layout
    let font_key = match renderer_resources.font_hash_map.get(&font_hash) {
        Some(k) => k,
        None => {
            log_debug!(
                LogCategory::DisplayList,
                "[push_text] FontKey not found for font_hash: {}",
                font_hash
            );
            return;
        }
    };

    // Look up FontInstanceKey for the given font size and DPI
    let font_instance_key = match renderer_resources.currently_registered_fonts.get(font_key) {
        Some((_, instances)) => match instances.get(&(font_size, dpi)) {
            Some(k) => *k,
            None => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[push_text] FontInstanceKey not found for size {:?} @ dpi {:?}",
                    font_size,
                    dpi
                );
                return;
            }
        },
        None => {
            log_debug!(
                LogCategory::DisplayList,
                "[push_text] Font instances not found for FontKey"
            );
            return;
        }
    };

    // Glyph positions are already absolute (container origin was added in paint_inline_content).
    // Scale from logical to physical pixels, then subtract the scroll frame offset
    // so glyphs render at the correct position inside scroll frames.
    let wr_glyphs: Vec<_> = glyphs
        .iter()
        .map(|g| webrender::api::GlyphInstance {
            index: g.index,
            point: webrender::api::units::LayoutPoint::new(
                g.point.x * dpi_scale - scroll_offset.0,
                g.point.y * dpi_scale - scroll_offset.1,
            ),
        })
        .collect();

    let wr_font_instance_key =
        crate::desktop::wr_translate2::wr_translate_font_instance_key(font_instance_key);
    let wr_color = azul_css::props::basic::color::ColorF::from(color);

    log_debug!(LogCategory::DisplayList,
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
