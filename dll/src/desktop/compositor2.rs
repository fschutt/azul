//! WebRender compositor integration for azul-dll
//!
//! This module bridges between azul-layout's DisplayList and WebRender's rendering pipeline.
//! It handles both GPU (hardware) and CPU (software) rendering paths.
//!
//! The main entry point is [`translate_displaylist_to_wr`], which iterates over an azul
//! `DisplayList` and emits the corresponding WebRender display list primitives.  Three
//! stacks manage the coordinate translation:
//!
//! - **clip_stack** – tracks nested clip chain IDs (`PushClip` / `PopClip`)
//! - **spatial_stack** – tracks spatial nodes (`PushScrollFrame` / `PushReferenceFrame`)
//! - **offset_stack** – converts absolute window coordinates to frame-relative coordinates
//!
//! Called from `generate_frame` in `wr_translate2.rs`.

use alloc::collections::BTreeMap;

use azul_core::{
    dom::DomId,
    hit_test::{PipelineId as AzulPipelineId, TAG_TYPE_SCROLL_CONTAINER},
    resources::{DpiScaleFactor, FontInstanceKey, ImageRefHash, RendererResourcesTrait},
    ui_solver::GlyphInstance,
};
use azul_css::props::{
    basic::{color::ColorU, pixel::PixelValue},
    style::border_radius::StyleBorderRadius,
};
use azul_layout::{
    cpurender::{text_halos, TextHalo},
    solver3::display_list::{BorderRadius, DisplayList},
    window::DomLayoutResult,
};
use webrender::{
    api::{
        units::{
            DeviceIntSize, LayoutPoint, LayoutRect, LayoutSize, LayoutTransform, LayoutVector2D,
        },
        AlphaType as WrAlphaType, BorderRadius as WrBorderRadius,
        BoxShadowClipMode as WrBoxShadowClipMode, BuiltDisplayList as WrBuiltDisplayList,
        ClipChainId as WrClipChainId, ClipMode as WrClipMode, ColorF, CommonItemProperties,
        ComplexClipRegion as WrComplexClipRegion, DisplayListBuilder as WrDisplayListBuilder,
        ExtendMode as WrExtendMode, ExternalScrollId, FilterOp as WrFilterOp,
        GradientStop as WrGradientStop, HasScrollLinkedEffect, ItemTag, PipelineId,
        PrimitiveFlags as WrPrimitiveFlags, PropertyBinding, ReferenceFrameKind,
        Shadow as WrShadow, SpaceAndClipInfo, SpatialId, SpatialTreeItemKey, TransformStyle,
    },
    render_api::ResourceUpdate as WrResourceUpdate,
};

use crate::{
    desktop::{
        shell2::common::debug_server::LogCategory,
        wr_translate2::{
            translate_image_key, wr_translate_border_radius, wr_translate_color_f,
            wr_translate_pipeline_id,
        },
    },
    log_debug,
};

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
///.
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

/// Scale a single f32 value from logical to physical pixels
#[inline]
fn scale_px(val: f32, dpi: f32) -> f32 {
    val * dpi
}

const DEFAULT_ROOT_FONT_SIZE_PX: f32 = 16.0;

/// The second half of the `SpatialTreeItemKey` of the plain frame that brings
/// a transformed box's items back to window coordinates; the transformed
/// frame itself is `(transform key, 0)`. Scroll frames count their second
/// half up from 0, so they never reach it.
const WINDOW_SPACE_FRAME: u64 = u64::MAX;

#[inline]
fn color_u_to_wr(c: &ColorU) -> ColorF {
    ColorF::new(
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    )
}

#[inline]
fn push_text_decoration_rect(
    builder: &mut WrDisplayListBuilder,
    bounds: &azul_layout::solver3::display_list::WindowLogicalRect,
    color: &ColorU,
    thickness: f32,
    dpi_scale: f32,
    offset: (f32, f32),
    clip_chain_id: WrClipChainId,
    spatial_id: SpatialId,
) {
    let raw_rect = LayoutRect::from_origin_and_size(
        LayoutPoint::new(
            scale_px(bounds.0.origin.x, dpi_scale),
            scale_px(bounds.0.origin.y, dpi_scale),
        ),
        LayoutSize::new(
            scale_px(bounds.0.size.width, dpi_scale),
            scale_px(thickness, dpi_scale),
        ),
    );
    let rect = LayoutRect::from_origin_and_size(
        LayoutPoint::new(raw_rect.min.x - offset.0, raw_rect.min.y - offset.1),
        LayoutSize::new(raw_rect.width(), raw_rect.height()),
    );
    let color_f = color_u_to_wr(color);
    let info = CommonItemProperties {
        clip_rect: rect,
        clip_chain_id,
        spatial_id,
        flags: Default::default(),
    };
    builder.push_rect(&info, rect, color_f);
}

/// Translate an Azul DisplayList to WebRender DisplayList and resources
/// Returns (resources, display_list, nested_pipelines) tuple that can be added to a transaction
/// by caller. nested_pipelines contains all child virtualized view pipelines that were recursively
/// built. Whether `AZ_OVERLAY=hit-test` asked for the hit-test overlay.
///
/// Read ONCE per process: the environment cannot change under a running app,
/// and this is consulted for every `HitTestArea` in every display list, so a
/// `getenv` per item would be a real cost in a large DOM.
fn show_hit_test_overlay() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| azul_core::window::DebugState::from_az_overlay_env().show_hit_test_areas)
}

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

    // One entry per open PushImageMaskClip: whether it opened a masked
    // stacking context, which its PopImageMaskClip has to close. A mask
    // whose image is not registered opens none.
    let mut mask_contexts: Vec<bool> = Vec::new();

    // How often each scroll id has been pushed so far. The display list
    // closes a frame around a box painted outside it and reopens it after
    // (`DisplayListGenerator::enter_scroll_chain` - a position:fixed header
    // on a scrolled page, a translucent item inside a plain scroll box), so
    // one id can open several frames. Each is a spatial node of its own, and
    // WebRender asserts that no two nodes of a scene share a
    // `SpatialTreeItemKey`; `set_scroll_offsets` moves every node carrying
    // the id, so they still scroll as one.
    let mut scroll_frame_pushes: BTreeMap<u64, u64> = BTreeMap::new();

    // Coordinate offset stack - tracks the origin offset for each spatial context.
    // When we enter a scroll frame, items inside have absolute coordinates but
    // WebRender expects coordinates relative to the scroll frame's content_rect origin.
    // We push the scroll frame's origin when entering, and subtract it from all coordinates.
    let offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];

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

    // The text shadows drawn as kept pictures (`text_halos`; their images
    // were drawn and registered with the frame's resources), by the index of
    // their PushTextShadow. Every other shadow goes to WebRender's own.
    let halos: BTreeMap<usize, TextHalo> = text_halos(&display_list.items, dpi_scale, None)
        .into_iter()
        .map(|halo| (halo.push, halo))
        .collect();
    // One entry per open PushTextShadow: whether it went to WebRender's
    // shadow stack, which its PopTextShadow has to pop.
    let mut wr_shadows: Vec<bool> = Vec::new();

    // Translate display list items to WebRender
    for (item_index, item) in display_list.items.iter().enumerate() {
        match item {
            DisplayListItem::Rect {
                bounds,
                color,
                border_radius,
            } => {
                let rect = resolve_rect(bounds, dpi_scale, current_offset!());

                let color_f = color_u_to_wr(color);

                let current_clip_chain = current_clip!();
                let current_spatial = current_spatial!();
                log_debug!(
                    LogCategory::DisplayList,
                    "[CLIP DEBUG] Rect: adjusted={:?}, clip_chain={:?}, spatial={:?}",
                    rect,
                    current_clip_chain,
                    current_spatial
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] Rect push: rect={:?}, clip_chain_id={:?}, spatial_id={:?}, \
                     clip_stack.len={}, spatial_stack.len={}",
                    rect,
                    current_clip_chain,
                    current_spatial,
                    clip_stack.len(),
                    spatial_stack.len()
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
                let style_border_radius = convert_border_radius_to_style(border_radius, dpi_scale);
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
                let color_f = color_u_to_wr(color);

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
                let color_f = color_u_to_wr(color);

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
                    // Use zero origin: children use absolute coordinates.
                    builder.push_simple_stacking_context_with_filters(
                        LayoutPoint::zero(),
                        current_spatial_id,
                        WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                        &[opacity_filter],
                        &[],
                        &[],
                    );
                }

                let color_f = color_u_to_wr(color);

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

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] ScrollBarStyled: bounds={:?}, offset={:?}, track={:?}, \
                     thumb={:?}",
                    info.bounds,
                    current_offset,
                    info.track_bounds,
                    info.thumb_bounds
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
                    // Use zero origin: children use absolute coordinates already.
                    // Using the scrollbar's position would double-offset children
                    // since WebRender offsets stacking context children by origin.
                    builder.push_simple_stacking_context_with_filters(
                        LayoutPoint::zero(),
                        spatial_id,
                        WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                        &[opacity_filter],
                        &[],
                        &[],
                    );
                }

                // If clip_to_container_border is enabled and container has border-radius,
                // create a clip chain for the container's rounded corners
                let clip_chain_id = if info.clip_to_container_border
                    && !info.container_border_radius.is_zero()
                {
                    let style_border_radius =
                        convert_border_radius_to_style(&info.container_border_radius, dpi_scale);
                    let wr_border_radius = wr_translate_border_radius(
                        style_border_radius,
                        azul_core::geom::LogicalSize::new(
                            scale_px(info.bounds.0.size.width, dpi_scale),
                            scale_px(info.bounds.0.size.height, dpi_scale),
                        ),
                    );

                    // Apply offset to scaled bounds for clip definition
                    let raw_scaled_bounds = LogicalRect::new(
                        azul_core::geom::LogicalPosition::new(
                            scale_px(info.bounds.0.origin.x, dpi_scale),
                            scale_px(info.bounds.0.origin.y, dpi_scale),
                        ),
                        azul_core::geom::LogicalSize::new(
                            scale_px(info.bounds.0.size.width, dpi_scale),
                            scale_px(info.bounds.0.size.height, dpi_scale),
                        ),
                    );
                    let scaled_bounds = LogicalRect::new(
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
                    let track_color = color_u_to_wr(&info.track_color);
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
                        let btn_color = color_u_to_wr(&info.button_color);
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
                        let btn_color = color_u_to_wr(&info.button_color);
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
                    // Convert initial transform to WR LayoutTransform with DPI scaling on
                    // translation
                    let t = &info.thumb_initial_transform;
                    let wr_transform = LayoutTransform::new(
                        t.m[0][0],
                        t.m[0][1],
                        t.m[0][2],
                        t.m[0][3],
                        t.m[1][0],
                        t.m[1][1],
                        t.m[1][2],
                        t.m[1][3],
                        t.m[2][0],
                        t.m[2][1],
                        t.m[2][2],
                        t.m[2][3],
                        t.m[3][0] * dpi_scale,
                        t.m[3][1] * dpi_scale,
                        t.m[3][2] * dpi_scale,
                        t.m[3][3],
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
                    let thumb_color = color_u_to_wr(&info.thumb_color);

                    // Handle rounded thumb corners
                    if !info.thumb_border_radius.is_zero() {
                        let style_border_radius =
                            convert_border_radius_to_style(&info.thumb_border_radius, dpi_scale);
                        let wr_border_radius = wr_translate_border_radius(
                            style_border_radius,
                            azul_core::geom::LogicalSize::new(
                                scale_px(info.thumb_bounds.0.size.width, dpi_scale),
                                scale_px(info.thumb_bounds.0.size.height, dpi_scale),
                            ),
                        );

                        // Create clip for rounded thumb (with offset applied)
                        let scaled_thumb_bounds = LogicalRect::new(
                            azul_core::geom::LogicalPosition::new(
                                scale_px(info.thumb_bounds.0.origin.x, dpi_scale)
                                    - current_offset.0,
                                scale_px(info.thumb_bounds.0.origin.y, dpi_scale)
                                    - current_offset.1,
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
                    "[compositor2] PushClip: bounds={:?} -> rect={:?}, spatial_stack.len={}, \
                     clip_stack.len={}, current_spatial={:?}, current_clip={:?}",
                    bounds,
                    rect,
                    spatial_stack.len(),
                    clip_stack.len(),
                    current_spatial,
                    current_clip
                );

                // Handle rounded corners if border_radius is non-zero
                if !border_radius.is_zero() {
                    // Convert layout BorderRadius to StyleBorderRadius for translation
                    let style_border_radius =
                        convert_border_radius_to_style(border_radius, dpi_scale);

                    let wr_border_radius = wr_translate_border_radius(
                        style_border_radius,
                        azul_core::geom::LogicalSize::new(
                            scale_px(bounds.0.size.width, dpi_scale),
                            scale_px(bounds.0.size.height, dpi_scale),
                        ),
                    );

                    // Create scaled bounds for clip (offset-corrected for scroll frames)
                    let scaled_bounds = LogicalRect::new(
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
                        "[compositor2] PushClip (rounded): created new_clip_id={:?}, pushing to \
                         clip_stack",
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
                        "[compositor2] PushClip (rect): clip_id={:?}, parent={:?}, \
                         new_clip_chain_id={:?}, pushing to clip_stack",
                        clip_id,
                        parent,
                        new_clip_chain_id
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
                let push_index = {
                    let pushes = scroll_frame_pushes.entry(*scroll_id).or_insert(0);
                    let index = *pushes;
                    *pushes += 1;
                    index
                };

                // Apply parent offset to frame_rect for correct positioning in parent space
                let adjusted_frame_rect = apply_offset(frame_rect, current_offset);

                // The content_rect is in PARENT space (same coordinate system as frame_rect)
                // Origin should match frame_rect.origin, size is the total scrollable content
                // This ensures that child coordinates (which are in parent-space after offset
                // adjustment) are correctly positioned relative to the scroll frame
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
                     scroll_id={}, parent_space={:?}, current_clip={:?}, spatial_stack.len={}, \
                     clip_stack.len={}",
                    frame_rect,
                    content_rect,
                    scroll_id,
                    parent_space,
                    current_clip,
                    spatial_stack.len(),
                    clip_stack.len()
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[CLIP DEBUG] PushScrollFrame: frame_rect={:?}, adjusted={:?}, \
                     content_rect={:?}",
                    frame_rect,
                    adjusted_frame_rect,
                    content_rect
                );

                let scroll_spatial_id = builder.define_scroll_frame(
                    parent_space,
                    external_scroll_id,
                    content_rect,
                    adjusted_frame_rect,
                    LayoutVector2D::zero(), // external_scroll_offset
                    0,                      // scroll_offset_generation (APZScrollGeneration)
                    HasScrollLinkedEffect::No,
                    SpatialTreeItemKey::new(*scroll_id, push_index),
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushScrollFrame: defined scroll_spatial_id={:?}",
                    scroll_spatial_id
                );

                // Push the new spatial ID onto the stack
                spatial_stack.push(scroll_spatial_id);

                // IMPORTANT: Do NOT push a new offset to offset_stack here.
                //
                // WebRender scroll frames do NOT create a new coordinate origin.
                // They are NOT reference frames — they share the same coordinate space
                // as their parent. The scroll frame only applies a translation (scroll
                // offset) to child content, and clips it to the viewport (frame_rect).
                //
                // Our display list uses ABSOLUTE WINDOW coordinates for all primitives.
                // Inside a scroll frame, those absolute coordinates ARE the correct
                // scroll-frame-local coordinates. A rect at window position (11, 53)
                // should appear at scroll-frame-local (11, 53), which renders at
                // world (11, 53) when scroll_y=0 — exactly at the top of the viewport.
                //
                // Subtracting frame_rect.origin would wrongly move Row 0 from (11, 53)
                // to (0, 0), placing it ABOVE the viewport clip at y=53.

                // Define clip for the scroll frame in PARENT SPACE (where the viewport is)
                // CRITICAL: The clip must be in parent space so it stays stationary while content
                // scrolls! If we define it in scroll space, the clip would scroll
                // with the content (wrong).
                let scroll_clip_id = builder.define_clip_rect(parent_space, adjusted_frame_rect);

                log_debug!(
                    LogCategory::DisplayList,
                    "[CLIP DEBUG] PushScrollFrame: scroll_clip_id={:?}, parent_space={:?}, \
                     adjusted_frame_rect={:?}",
                    scroll_clip_id,
                    parent_space,
                    adjusted_frame_rect
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushScrollFrame: defined scroll_clip_id={:?} on \
                     parent_space={:?} with frame_rect={:?}",
                    scroll_clip_id,
                    parent_space,
                    frame_rect
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

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushScrollFrame: defined scroll_clip_chain={:?} with \
                     parent={:?}",
                    scroll_clip_chain,
                    parent_clip
                );

                clip_stack.push(scroll_clip_chain);

                // Push a hit-test area for this scroll container so the scroll manager can find it
                // during wheel/trackpad events. This uses TAG_TYPE_SCROLL_CONTAINER (0x0500).
                // The tag encodes: tag.0 = scroll_id, tag.1 = 0x0500
                let scroll_container_tag: ItemTag = (*scroll_id, TAG_TYPE_SCROLL_CONTAINER);
                builder.push_hit_test(
                    adjusted_frame_rect,
                    scroll_clip_chain, // Use the scroll clip chain we just created
                    parent_space,      // Push in parent space (stationary viewport)
                    WrPrimitiveFlags::default(),
                    scroll_container_tag,
                );

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushScrollFrame: pushed scroll container hit-test tag=({}, \
                     0x{:04x})",
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

                // Pop spatial and clip stacks (no offset stack — scroll frames don't push one)
                let popped_clip = clip_stack.pop();
                let popped_spatial = spatial_stack.pop();

                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PopScrollFrame: popped_clip={:?}, popped_spatial={:?}, \
                     spatial_stack {} -> {}, clip_stack {} -> {}",
                    popped_clip,
                    popped_spatial,
                    spatial_before,
                    spatial_stack.len(),
                    clip_before,
                    clip_stack.len()
                );

                if spatial_stack.is_empty() || clip_stack.is_empty() {
                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] ERROR: PopScrollFrame caused stack underflow"
                    );
                    return Err("Scroll frame stack underflow".to_string());
                }
            }

            DisplayListItem::HitTestArea { bounds, tag } => {
                let rect = resolve_rect(bounds, dpi_scale, current_offset!());

                // Draw a translucent red rectangle over the hit-test area when
                // `AZ_OVERLAY=hit-test` is set.
                //
                // This used to be `#[cfg(debug_assertions)]`: a debug build
                // painted EVERY hit-test area red and a release build painted
                // none, with no way to ask for either. That is a debug/release
                // divergence in visual output, and it misleads — running
                // hello-world showed a red window and the first hypothesis was
                // "this linked the wrong DLL". Now it is a runtime switch, and
                // it works in release too, because the build you need to inspect
                // hit regions on is usually the one a user is running.
                if show_hit_test_overlay() {
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
            }
            | DisplayListItem::Strikethrough {
                bounds,
                color,
                thickness,
            }
            | DisplayListItem::Overline {
                bounds,
                color,
                thickness,
            } => {
                push_text_decoration_rect(
                    &mut builder,
                    bounds,
                    color,
                    *thickness,
                    dpi_scale,
                    current_offset!(),
                    current_clip!(),
                    current_spatial!(),
                );
            }

            DisplayListItem::Text {
                glyphs,
                font_size_px,
                font_hash,
                color,
                clip_rect,
                source_node_index: _,
            } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] Text item: {} glyphs, font_size={}, color={:?}, \
                     clip_rect={:?}, dpi={}",
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
                    font_hash.synthetic_bold,
                    *color,
                    renderer_resources,
                    dpi,
                    font_size_au,
                    scaled_origin,  // Pass offset-corrected origin to position glyphs
                    current_offset, // Pass scroll frame offset for glyph position correction
                );
            }

            DisplayListItem::Image {
                bounds,
                image,
                border_radius,
            } => {
                // Get the ImageRefHash from the ImageRef
                let image_ref_hash = image.get_hash();

                if let Some(resolved_image) = renderer_resources.get_image(&image_ref_hash) {
                    let wr_image_key = translate_image_key(resolved_image.key);

                    let rect = resolve_rect(bounds, dpi_scale, current_offset!());

                    // Handle border_radius by creating a clip region (same as Rect)
                    let style_border_radius =
                        convert_border_radius_to_style(border_radius, dpi_scale);
                    let wr_border_radius = wr_translate_border_radius(
                        style_border_radius,
                        azul_core::geom::LogicalSize::new(
                            scale_px(bounds.0.size.width, dpi_scale),
                            scale_px(bounds.0.size.height, dpi_scale),
                        ),
                    );

                    let info = if !wr_border_radius.is_zero() {
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
                        let new_clip_id = define_border_radius_clip(
                            &mut builder,
                            logical_rect,
                            wr_border_radius,
                            current_spatial!(),
                            current_clip!(),
                        );
                        CommonItemProperties {
                            clip_rect: rect,
                            clip_chain_id: new_clip_id,
                            spatial_id: current_spatial!(),
                            flags: Default::default(),
                        }
                    } else {
                        CommonItemProperties {
                            clip_rect: rect,
                            clip_chain_id: current_clip!(),
                            spatial_id: current_spatial!(),
                            flags: Default::default(),
                        }
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

                    // An NV12 frame (camera / decoder) is a YUV image: its two
                    // planes (R8 luma + RG8 chroma, see
                    // `wr_translate2::nv12_plane_descriptors`) are converted to
                    // RGB in the shader, in the frame's own matrix and range.
                    let format = resolved_image.descriptor.format;
                    let chroma_key = if format.is_nv12() {
                        renderer_resources
                            .nv12_chroma_keys
                            .get(&resolved_image.key)
                            .copied()
                    } else {
                        None
                    };
                    match chroma_key {
                        Some(chroma_key) => {
                            use webrender::api::{
                                ColorDepth as WrColorDepth, ColorRange as WrColorRange,
                                YuvColorSpace as WrYuvColorSpace, YuvData as WrYuvData,
                            };
                            builder.push_yuv_image(
                                &info,
                                rect,
                                WrYuvData::NV12(wr_image_key, translate_image_key(chroma_key)),
                                WrColorDepth::Color8,
                                if format.is_rec709() {
                                    WrYuvColorSpace::Rec709
                                } else {
                                    WrYuvColorSpace::Rec601
                                },
                                if format.is_full_range() {
                                    WrColorRange::Full
                                } else {
                                    WrColorRange::Limited
                                },
                                WrImageRendering::Auto,
                            );
                        }
                        None => builder.push_image(
                            &info,
                            rect,
                            WrImageRendering::Auto,
                            WrAlphaType::PremultipliedAlpha,
                            wr_image_key,
                            ColorF::WHITE, // No tint by default
                        ),
                    }
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

                // WebRender moves everything inside a stacking context by its
                // origin, and the items inside are already in window
                // coordinates: opened at the box's position, the box would be
                // drawn that far off a second time. So the context opens at
                // zero, like the opacity and filter groups below.
                let current_spatial_id = current_spatial!();
                builder.push_simple_stacking_context(
                    LayoutPoint::zero(),
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
                    initial_transform.m[0][0],
                    initial_transform.m[0][1],
                    initial_transform.m[0][2],
                    initial_transform.m[0][3],
                    initial_transform.m[1][0],
                    initial_transform.m[1][1],
                    initial_transform.m[1][2],
                    initial_transform.m[1][3],
                    initial_transform.m[2][0],
                    initial_transform.m[2][1],
                    initial_transform.m[2][2],
                    initial_transform.m[2][3],
                    initial_transform.m[3][0] * dpi_scale,
                    initial_transform.m[3][1] * dpi_scale,
                    initial_transform.m[3][2] * dpi_scale,
                    initial_transform.m[3][3],
                );

                // Use PropertyBinding::Binding so we can update this transform
                // dynamically via append_dynamic_properties
                let binding = PropertyBinding::Binding(
                    webrender::api::PropertyBindingKey::new(transform_key.id as u64),
                    wr_transform,
                );

                // Use the transform_key.id as the spatial tree item key
                let spatial_key = SpatialTreeItemKey::new(transform_key.id as u64, 0);

                // The transform works about the box's own top-left corner
                // (`transform-origin` resolves against the box, and the CPU
                // renderer applies it about `bounds`' origin); the items
                // inside stay in window coordinates. WebRender draws a frame's
                // content at `transform(p) + origin`, so the transformed frame
                // opens at the box's corner and a plain frame inside it opens
                // at minus that corner: the transform turns the box about its
                // corner and the items keep their coordinates. The live value
                // `synchronize_gpu_values` sends for the key replaces the
                // outer frame's matrix as it is.
                let corner = LayoutPoint::new(
                    scale_px(bounds.0.origin.x, dpi_scale),
                    scale_px(bounds.0.origin.y, dpi_scale),
                );
                let transformed = builder.push_reference_frame(
                    corner,
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
                let window_space = builder.push_reference_frame(
                    LayoutPoint::new(-corner.x, -corner.y),
                    transformed,
                    TransformStyle::Flat,
                    PropertyBinding::Value(LayoutTransform::identity()),
                    ReferenceFrameKind::Transform {
                        is_2d_scale_translation: true,
                        should_snap: false,
                        paired_with_perspective: false,
                    },
                    SpatialTreeItemKey::new(transform_key.id as u64, WINDOW_SPACE_FRAME),
                );

                // Children are placed in the window-space frame; the pop
                // closes both.
                spatial_stack.push(transformed);
                spatial_stack.push(window_space);
            }

            DisplayListItem::PopReferenceFrame => {
                // The window-space frame and the transformed frame around it.
                for _ in 0..2 {
                    builder.pop_reference_frame();
                    spatial_stack.pop();
                }
            }

            DisplayListItem::VirtualView {
                child_dom_id,
                bounds,
                clip_rect,
                // The scroll is not baked in here: `scroll_all_nodes` moves
                // the child pipeline's root scroll frame by the live offset
                // every frame, so a scroll needs no new display list.
                content_offset: _,
            } => {
                // VirtualView rendering implementation:
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
                    "[compositor2] VirtualView: child_dom_id={:?}, child_pipeline_id={:?}, \
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
                            let wr_clip_rect =
                                scale_bounds_to_layout_rect(clip_rect.inner(), dpi_scale);

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

            DisplayListItem::VirtualViewPlaceholder { node_id, .. } => {
                // VirtualViewPlaceholder should have been replaced by VirtualView in window.rs.
                // If we reach here, the VirtualView callback was not invoked for this node.
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] WARNING: VirtualViewPlaceholder for node {:?} was not replaced",
                    node_id
                );
            }

            // A `<webview>`'s rect: the native view is a child view the
            // platform composites over the window (placed by the shell's
            // web view pump), so WebRender draws nothing here.
            DisplayListItem::WebView { .. } => {}

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

                // The gradient line and every stop on it, from the resolver the
                // CPU renderer shares (CSS Images 3: `90deg` runs like `to right`,
                // a stop at a length sits at that length, a hard stop is two stops
                // at one offset - WebRender draws those as a hard change). It
                // works in the box's CSS px; the line is scaled to device px.
                use azul_css::props::style::background::{color_stops_on_the_line, ExtendMode};
                let mut resolved =
                    gradient.resolve_in_box(bounds.0.size.width, bounds.0.size.height);
                let extend_mode = match gradient.extend_mode {
                    ExtendMode::Clamp => WrExtendMode::Clamp,
                    ExtendMode::Repeat => {
                        // The first..last stop span is what repeats.
                        resolved = resolved.to_repeat_period();
                        WrExtendMode::Repeat
                    }
                };
                let start_point = LayoutPoint::new(
                    scale_px(resolved.start.0, dpi_scale),
                    scale_px(resolved.start.1, dpi_scale),
                );
                let end_point = LayoutPoint::new(
                    scale_px(resolved.end.0, dpi_scale),
                    scale_px(resolved.end.1, dpi_scale),
                );

                // Convert gradient stops (a clamped gradient only samples the
                // line itself, so its stops are cut to 0..=1 there, mixed in
                // premultiplied space as WebRender mixes between them).
                use azul_css::props::basic::color::ColorU as CssColorU;
                let stops: Vec<(f32, CssColorU)> = resolved
                    .stops
                    .iter()
                    .map(|(t, color)| (*t, color.to_color_u_default()))
                    .collect();
                let stops = match gradient.extend_mode {
                    ExtendMode::Clamp => {
                        color_stops_on_the_line(&stops, CssColorU::interpolate_premultiplied)
                    }
                    ExtendMode::Repeat => stops,
                };
                let wr_stops: Vec<WrGradientStop> = stops
                    .iter()
                    .map(|&(offset, color)| WrGradientStop {
                        offset,
                        color: wr_translate_color_f(azul_css::props::basic::color::ColorF::from(
                            color,
                        )),
                    })
                    .collect();

                if resolved.stops.len() >= 2 {
                    let info = gradient_item_properties(
                        &mut builder,
                        bounds,
                        border_radius,
                        rect,
                        dpi_scale,
                        current_spatial!(),
                        current_clip!(),
                    );
                    // WebRender fills its gradient table from a stop at 0 to
                    // one at 1 and paints white past the last stop it gets;
                    // `create_gradient` moves the line onto the stops' span.
                    let wr_gradient =
                        builder.create_gradient(start_point, end_point, wr_stops, extend_mode);
                    builder.push_gradient(
                        &info,
                        rect,
                        wr_gradient,
                        info.clip_rect.size(),
                        LayoutSize::zero(),
                    );
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
                    BackgroundPositionHorizontal::Exact(px) => scale_px(
                        px.to_pixels_internal(bounds.0.size.width, 16.0, 16.0),
                        dpi_scale,
                    ),
                };
                let center_y = match &gradient.position.vertical {
                    BackgroundPositionVertical::Top => 0.0,
                    BackgroundPositionVertical::Center => scaled_height / 2.0,
                    BackgroundPositionVertical::Bottom => scaled_height,
                    BackgroundPositionVertical::Exact(px) => scale_px(
                        px.to_pixels_internal(bounds.0.size.height, 16.0, 16.0),
                        dpi_scale,
                    ),
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

                if wr_stops.len() >= 2 {
                    let info = gradient_item_properties(
                        &mut builder,
                        bounds,
                        border_radius,
                        rect,
                        dpi_scale,
                        current_spatial!(),
                        current_clip!(),
                    );
                    // Stops that end early (flora's bloom fades out at 45%) or
                    // run past the ellipse reach WebRender as 0..1, mapped
                    // through the gradient's start and end offsets.
                    let wr_gradient =
                        builder.create_radial_gradient(center, radius, wr_stops, extend_mode);
                    builder.push_radial_gradient(
                        &info,
                        rect,
                        wr_gradient,
                        info.clip_rect.size(),
                        LayoutSize::zero(),
                    );
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
                    BackgroundPositionHorizontal::Exact(px) => scale_px(
                        px.to_pixels_internal(bounds.0.size.width, 16.0, 16.0),
                        dpi_scale,
                    ),
                };
                let center_y = match &gradient.center.vertical {
                    BackgroundPositionVertical::Top => 0.0,
                    BackgroundPositionVertical::Center => scaled_height / 2.0,
                    BackgroundPositionVertical::Bottom => scaled_height,
                    BackgroundPositionVertical::Exact(px) => scale_px(
                        px.to_pixels_internal(bounds.0.size.height, 16.0, 16.0),
                        dpi_scale,
                    ),
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
                                azul_css::props::basic::color::ColorF::from(
                                    stop.color.to_color_u_default(),
                                ),
                            ),
                        }
                    })
                    .collect();

                if wr_stops.len() >= 2 {
                    let info = gradient_item_properties(
                        &mut builder,
                        bounds,
                        border_radius,
                        rect,
                        dpi_scale,
                        current_spatial!(),
                        current_clip!(),
                    );
                    // As a radial gradient: stops mapped onto 0..1 of the turn.
                    let wr_gradient =
                        builder.create_conic_gradient(center, angle, wr_stops, extend_mode);
                    builder.push_conic_gradient(
                        &info,
                        rect,
                        wr_gradient,
                        info.clip_rect.size(),
                        LayoutSize::zero(),
                    );
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
                    scale_px(
                        shadow.offset_x.inner.to_pixels_internal(
                            0.0,
                            DEFAULT_ROOT_FONT_SIZE_PX,
                            DEFAULT_ROOT_FONT_SIZE_PX,
                        ),
                        dpi_scale,
                    ),
                    scale_px(
                        shadow.offset_y.inner.to_pixels_internal(
                            0.0,
                            DEFAULT_ROOT_FONT_SIZE_PX,
                            DEFAULT_ROOT_FONT_SIZE_PX,
                        ),
                        dpi_scale,
                    ),
                );
                let color_f =
                    wr_translate_color_f(azul_css::props::basic::color::ColorF::from(shadow.color));
                let blur_radius = scale_px(
                    shadow.blur_radius.inner.to_pixels_internal(
                        0.0,
                        DEFAULT_ROOT_FONT_SIZE_PX,
                        DEFAULT_ROOT_FONT_SIZE_PX,
                    ),
                    dpi_scale,
                );
                let spread_radius = scale_px(
                    shadow.spread_radius.inner.to_pixels_internal(
                        0.0,
                        DEFAULT_ROOT_FONT_SIZE_PX,
                        DEFAULT_ROOT_FONT_SIZE_PX,
                    ),
                    dpi_scale,
                );

                let style_border_radius = convert_border_radius_to_style(border_radius, dpi_scale);
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

                // For outset shadows, expand clip_rect to accommodate the shadow
                // extending beyond element bounds (offset + blur + spread).
                // Inset shadows are contained within the element, so no expansion needed.
                let shadow_clip_rect = if clip_mode == WrBoxShadowClipMode::Outset {
                    let extent =
                        blur_radius + spread_radius.max(0.0) + offset.x.abs().max(offset.y.abs());
                    LayoutRect::from_origin_and_size(
                        LayoutPoint::new(rect.min.x - extent, rect.min.y - extent),
                        LayoutSize::new(rect.width() + 2.0 * extent, rect.height() + 2.0 * extent),
                    )
                } else {
                    rect
                };

                let info = CommonItemProperties {
                    clip_rect: shadow_clip_rect,
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
                // Use zero origin: children use absolute coordinates.
                builder.push_simple_stacking_context_with_filters(
                    LayoutPoint::zero(),
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

            DisplayListItem::PushOpacity {
                bounds,
                opacity,
                opacity_key,
            } => {
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushOpacity: bounds={:?}, opacity={}, key={:?}",
                    bounds,
                    opacity,
                    opacity_key
                );
                let current_spatial_id = current_spatial!();
                // An animated group binds its key so the fade advances GPU-side
                // without a display-list rebuild — the same
                // PropertyBindingKey scheme the scrollbar fade uses above.
                let opacity_filter = match opacity_key {
                    Some(k) => WrFilterOp::Opacity(
                        PropertyBinding::Binding(
                            webrender::api::PropertyBindingKey::new(k.id as u64),
                            *opacity,
                        ),
                        *opacity,
                    ),
                    None => WrFilterOp::Opacity(PropertyBinding::Value(*opacity), *opacity),
                };
                // Use zero origin: children use absolute coordinates.
                builder.push_simple_stacking_context_with_filters(
                    LayoutPoint::zero(),
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
                log_debug!(
                    LogCategory::DisplayList,
                    "[compositor2] PushTextShadow: {:?}",
                    shadow
                );
                // A kept halo is painted as an image tinted with the shadow's
                // colour, below the text that follows, as WebRender paints
                // its own shadow: no surface and no blur pass this frame.
                let halo = halos.get(&item_index).and_then(|halo| {
                    renderer_resources
                        .get_image(&halo.image.get_hash())
                        .map(|resolved| (halo, resolved.key))
                });
                if let Some((halo, key)) = halo {
                    let offset = current_offset!();
                    let rect = LayoutRect::from_origin_and_size(
                        LayoutPoint::new(halo.origin.0 - offset.0, halo.origin.1 - offset.1),
                        LayoutSize::new(halo.width as f32, halo.height as f32),
                    );
                    let info = CommonItemProperties {
                        clip_rect: rect,
                        clip_chain_id: current_clip!(),
                        spatial_id: current_spatial!(),
                        flags: Default::default(),
                    };
                    builder.push_image(
                        &info,
                        rect,
                        webrender::api::ImageRendering::Auto,
                        WrAlphaType::PremultipliedAlpha,
                        translate_image_key(key),
                        color_u_to_wr(&halo.color),
                    );
                    wr_shadows.push(false);
                    continue;
                }
                let current_spatial_id = current_spatial!();
                let current_clip_chain = current_clip!();
                let offset_x = shadow.offset_x.inner.to_pixels_internal(
                    0.0,
                    DEFAULT_ROOT_FONT_SIZE_PX,
                    DEFAULT_ROOT_FONT_SIZE_PX,
                ) * dpi_scale;
                let offset_y = shadow.offset_y.inner.to_pixels_internal(
                    0.0,
                    DEFAULT_ROOT_FONT_SIZE_PX,
                    DEFAULT_ROOT_FONT_SIZE_PX,
                ) * dpi_scale;
                let blur_radius = shadow.blur_radius.inner.to_pixels_internal(
                    0.0,
                    DEFAULT_ROOT_FONT_SIZE_PX,
                    DEFAULT_ROOT_FONT_SIZE_PX,
                ) * dpi_scale;
                let wr_shadow = WrShadow {
                    offset: LayoutVector2D::new(offset_x, offset_y),
                    color: color_u_to_wr(&shadow.color),
                    blur_radius,
                };
                let space_and_clip = SpaceAndClipInfo {
                    spatial_id: current_spatial_id,
                    clip_chain_id: current_clip_chain,
                };
                builder.push_shadow(&space_and_clip, wr_shadow, true);
                wr_shadows.push(true);
            }
            DisplayListItem::PopTextShadow => {
                log_debug!(LogCategory::DisplayList, "[compositor2] PopTextShadow");
                if wr_shadows.pop() == Some(true) {
                    builder.pop_all_shadows();
                }
            }

            // A STROKED PATH. WebRender has no vector rasteriser, so the
            // stroke arrives pre-rasterised as an R8 coverage mask and is
            // painted as a colour THROUGH it - the same primitive pair the
            // clip-mask path already uses, so no new WebRender concept is
            // needed. (The CPU backend reads the same item and strokes the
            // real outline instead, which stays sharp at any zoom.)
            DisplayListItem::StrokedPath {
                bounds,
                color,
                mask,
                ..
            } => {
                let Some(mask) = mask.as_ref() else {
                    // No rasteriser in this build: draw nothing rather than a
                    // filled box where an outline belongs.
                    continue;
                };
                let Some(resolved_mask) = renderer_resources.get_image(&mask.get_hash()) else {
                    continue;
                };
                let rect = resolve_rect(bounds, dpi_scale, current_offset!());
                let wr_image_mask = webrender::api::ImageMask {
                    image: translate_image_key(resolved_mask.key),
                    rect,
                };
                let current_spatial = current_spatial!();
                let current_clip = current_clip!();
                let clip_id = builder.define_clip_image_mask(
                    current_spatial,
                    wr_image_mask,
                    &[],
                    webrender::api::FillRule::Nonzero,
                );
                let parent = if current_clip == WrClipChainId::INVALID {
                    None
                } else {
                    Some(current_clip)
                };
                let masked_chain = builder.define_clip_chain(parent, vec![clip_id]);
                let info = CommonItemProperties {
                    clip_rect: rect,
                    clip_chain_id: masked_chain,
                    spatial_id: current_spatial,
                    flags: WrPrimitiveFlags::empty(),
                };
                builder.push_rect(&info, rect, color_u_to_wr(color));
            }

            DisplayListItem::PushImageMaskClip {
                bounds,
                mask_image,
                mask_rect,
            } => {
                let current_spatial = current_spatial!();
                let current_clip = current_clip!();
                let current_offset = current_offset!();

                // Resolve the mask image to a WebRender ImageKey
                let image_ref_hash = mask_image.get_hash();
                if let Some(resolved_image) = renderer_resources.get_image(&image_ref_hash) {
                    let wr_image_key = translate_image_key(resolved_image.key);

                    // The mask rect in physical pixels
                    let wr_mask_rect = LayoutRect::from_origin_and_size(
                        LayoutPoint::new(
                            scale_px(mask_rect.0.origin.x, dpi_scale) - current_offset.0,
                            scale_px(mask_rect.0.origin.y, dpi_scale) - current_offset.1,
                        ),
                        LayoutSize::new(
                            scale_px(mask_rect.0.size.width, dpi_scale),
                            scale_px(mask_rect.0.size.height, dpi_scale),
                        ),
                    );

                    let wr_image_mask = webrender::api::ImageMask {
                        image: wr_image_key,
                        rect: wr_mask_rect,
                    };

                    // Define the image mask clip
                    let clip_id = builder.define_clip_image_mask(
                        current_spatial,
                        wr_image_mask,
                        &[], // no polygon points
                        webrender::api::FillRule::Nonzero,
                    );

                    // Create a clip chain from the clip id
                    let parent = if current_clip == WrClipChainId::INVALID {
                        None
                    } else {
                        Some(current_clip)
                    };
                    let new_clip_chain_id = builder.define_clip_chain(parent, vec![clip_id]);

                    // WebRender masks a stacking context with an image mask:
                    // it draws the context to a surface and masks that. In an
                    // item's own clip chain the mask is skipped by every item
                    // that takes the brush path (gradients, text, images), so
                    // the content goes into a context that carries the mask,
                    // and the items inside keep the clip around it. Opened at
                    // zero, like every context here: the items are already in
                    // frame coordinates.
                    builder.push_stacking_context(
                        LayoutPoint::zero(),
                        current_spatial,
                        WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
                        Some(new_clip_chain_id),
                        TransformStyle::Flat,
                        webrender::api::MixBlendMode::Normal,
                        &[],
                        &[],
                        &[],
                        webrender::api::RasterSpace::Screen,
                        webrender::api::StackingContextFlags::empty(),
                    );
                    mask_contexts.push(true);
                } else {
                    mask_contexts.push(false);
                }
                // Push the clip around the mask to keep the stack balanced.
                clip_stack.push(current_clip);
            }

            DisplayListItem::PopImageMaskClip => {
                if mask_contexts.pop() == Some(true) {
                    builder.pop_stacking_context();
                }
                let before_len = clip_stack.len();
                if clip_stack.len() > 1 {
                    let popped = clip_stack.pop();
                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] PopImageMaskClip: popped {:?}, clip_stack.len {} -> {}",
                        popped,
                        before_len,
                        clip_stack.len()
                    );
                } else {
                    log_debug!(
                        LogCategory::DisplayList,
                        "[compositor2] PopImageMaskClip: SKIPPED (clip_stack.len={}, would \
                         underflow)",
                        before_len
                    );
                }
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

/// A display list `BorderRadius` (CSS px) as the device px radii of a
/// WebRender clip, whose rect is in device px too.
#[inline]
fn convert_border_radius_to_style(br: &BorderRadius, dpi: f32) -> StyleBorderRadius {
    StyleBorderRadius {
        top_left: PixelValue::px(scale_px(br.top_left, dpi)),
        top_right: PixelValue::px(scale_px(br.top_right, dpi)),
        bottom_left: PixelValue::px(scale_px(br.bottom_left, dpi)),
        bottom_right: PixelValue::px(scale_px(br.bottom_right, dpi)),
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

/// The clip chain for a box painted with rounded corners: `parent` plus a
/// rounded rect at the box, or `parent` itself when the corners are square.
fn rounded_box_clip(
    builder: &mut WrDisplayListBuilder,
    bounds: &azul_layout::solver3::display_list::WindowLogicalRect,
    border_radius: &BorderRadius,
    dpi_scale: f32,
    spatial_id: SpatialId,
    parent: WrClipChainId,
) -> WrClipChainId {
    let size = azul_core::geom::LogicalSize::new(
        scale_px(bounds.0.size.width, dpi_scale),
        scale_px(bounds.0.size.height, dpi_scale),
    );
    let wr_border_radius = wr_translate_border_radius(
        convert_border_radius_to_style(border_radius, dpi_scale),
        size,
    );
    if wr_border_radius.is_zero() {
        return parent;
    }
    let rect = azul_core::geom::LogicalRect::new(
        azul_core::geom::LogicalPosition::new(
            scale_px(bounds.0.origin.x, dpi_scale),
            scale_px(bounds.0.origin.y, dpi_scale),
        ),
        size,
    );
    define_border_radius_clip(builder, rect, wr_border_radius, spatial_id, parent)
}

/// The item properties of a gradient painted at `rect` (device px, relative
/// to the current frame), clipped to its box's rounded corners.
///
/// This defines the corner clip, so call it before the gradient's stops are
/// pushed: WebRender gives a stops marker to the item right after it, and a
/// clip in between would take them.
fn gradient_item_properties(
    builder: &mut WrDisplayListBuilder,
    bounds: &azul_layout::solver3::display_list::WindowLogicalRect,
    border_radius: &BorderRadius,
    rect: LayoutRect,
    dpi_scale: f32,
    spatial_id: SpatialId,
    clip_chain_id: WrClipChainId,
) -> CommonItemProperties {
    CommonItemProperties {
        clip_rect: rect,
        clip_chain_id: rounded_box_clip(
            builder,
            bounds,
            border_radius,
            dpi_scale,
            spatial_id,
            clip_chain_id,
        ),
        spatial_id,
        flags: Default::default(),
    }
}

/// Convert azul StyleFilter list to WebRender FilterOp list
fn translate_style_filters_to_wr(
    filters: &[azul_css::props::style::filter::StyleFilter],
    dpi_scale: f32,
) -> Vec<WrFilterOp> {
    use azul_css::props::style::filter::{fold_flood_in, StyleFilter};

    // WebRender's `Flood` REPLACES its input with the colour and it has no
    // composite step, so `flood(c) composite(in)` - an icon tint - would paint
    // a filled box (ledger E15). The pair is one colour matrix; fold it first.
    let filters = fold_flood_in(filters);
    filters
        .iter()
        .filter_map(|f| match f {
            StyleFilter::Blur(blur) => {
                let w = scale_px(
                    blur.width.to_pixels_internal(
                        0.0,
                        DEFAULT_ROOT_FONT_SIZE_PX,
                        DEFAULT_ROOT_FONT_SIZE_PX,
                    ),
                    dpi_scale,
                );
                let h = scale_px(
                    blur.height.to_pixels_internal(
                        0.0,
                        DEFAULT_ROOT_FONT_SIZE_PX,
                        DEFAULT_ROOT_FONT_SIZE_PX,
                    ),
                    dpi_scale,
                );
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
            // The azul matrix is row-major (SVG order); WebRender takes the
            // input-channel columns, then the offsets. Passed through as-is
            // (as it was) the matrix was scrambled - the icon grayscale
            // matrix came out as a different colour transform entirely.
            StyleFilter::ColorMatrix(m) => Some(WrFilterOp::ColorMatrix(m.to_column_major())),
            StyleFilter::DropShadow(s) => {
                let offset = LayoutVector2D::new(
                    scale_px(
                        s.offset_x.inner.to_pixels_internal(
                            0.0,
                            DEFAULT_ROOT_FONT_SIZE_PX,
                            DEFAULT_ROOT_FONT_SIZE_PX,
                        ),
                        dpi_scale,
                    ),
                    scale_px(
                        s.offset_y.inner.to_pixels_internal(
                            0.0,
                            DEFAULT_ROOT_FONT_SIZE_PX,
                            DEFAULT_ROOT_FONT_SIZE_PX,
                        ),
                        dpi_scale,
                    ),
                );
                let color =
                    wr_translate_color_f(azul_css::props::basic::color::ColorF::from(s.color));
                let blur_radius = scale_px(
                    s.blur_radius.inner.to_pixels_internal(
                        0.0,
                        DEFAULT_ROOT_FONT_SIZE_PX,
                        DEFAULT_ROOT_FONT_SIZE_PX,
                    ),
                    dpi_scale,
                );
                Some(WrFilterOp::DropShadow(WrShadow {
                    offset,
                    color,
                    blur_radius,
                }))
            }
            StyleFilter::Flood(color) => {
                let c = wr_translate_color_f(azul_css::props::basic::color::ColorF::from(*color));
                Some(WrFilterOp::Flood(c))
            }
            StyleFilter::Blend(_)
            | StyleFilter::ComponentTransfer
            | StyleFilter::Offset(_)
            | StyleFilter::Composite(_) => {
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
    // The run is drawn in synthetic bold: its face has no bold cut, so the
    // glyph rasterizer thickens the outlines (WebRender's SYNTHETIC_BOLD).
    synthetic_bold: bool,
    color: ColorU,
    renderer_resources: &azul_core::resources::RendererResources,
    dpi: DpiScaleFactor,
    font_size: azul_core::resources::Au,
    container_origin: azul_core::geom::LogicalPosition, // Container origin (already scaled)
    scroll_offset: (f32, f32),                          /* Offset to subtract from glyph
                                                         * positions for scroll frames */
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
            point: LayoutPoint::new(
                g.point.x * dpi_scale - scroll_offset.0,
                g.point.y * dpi_scale - scroll_offset.1,
            ),
        })
        .collect();

    let wr_font_instance_key =
        crate::desktop::wr_translate2::wr_translate_font_instance_key(font_instance_key);
    let wr_color = azul_css::props::basic::color::ColorF::from(color);

    log_debug!(
        LogCategory::DisplayList,
        "[push_text] Pushing {} glyphs with FontInstanceKey {:?}, color={:?}, \
         container_origin=({}, {}), dpi={}",
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
        synthetic_bold.then_some(webrender::api::GlyphOptions {
            render_mode: webrender::api::FontRenderMode::Subpixel,
            flags: webrender::api::FontInstanceFlags::SYNTHETIC_BOLD,
        }),
    );
}

#[cfg(test)]
mod stacking_context_tests {
    //! What WebRender is handed for a box painted inside a stacking context.
    //! The display list gives every item in window coordinates, and
    //! WebRender moves everything inside a stacking context by the context's
    //! origin, so the rects it ends up with are where the box is drawn.

    use alloc::collections::BTreeMap;

    use azul_core::{
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::{DpiScaleFactor, RendererResources, TransformKey},
        transform::ComputedTransform3D,
    };
    use azul_css::props::basic::color::ColorU;
    use azul_layout::solver3::display_list::{BorderRadius, DisplayList, DisplayListItem};
    use webrender::api::{
        units::{DeviceIntSize, LayoutPoint, LayoutRect, LayoutSize},
        BuiltDisplayList, DisplayItem, PipelineId,
    };

    use super::translate_displaylist_to_wr;

    pub(super) fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    fn device(r: LogicalRect, dpi: f32) -> LayoutRect {
        LayoutRect::from_origin_and_size(
            LayoutPoint::new(r.origin.x * dpi, r.origin.y * dpi),
            LayoutSize::new(r.size.width * dpi, r.size.height * dpi),
        )
    }

    fn fill(r: LogicalRect) -> DisplayListItem {
        DisplayListItem::Rect {
            bounds: r.into(),
            color: ColorU::new(30, 60, 170, 255),
            border_radius: BorderRadius::default(),
        }
    }

    /// The WebRender display list built from `items` in a 400 x 100 window.
    pub(super) fn built(items: Vec<DisplayListItem>, dpi: f32) -> BuiltDisplayList {
        built_with(items, dpi, &RendererResources::default())
    }

    /// [`built`] with the images and fonts of `resources` registered.
    pub(super) fn built_with(
        items: Vec<DisplayListItem>,
        dpi: f32,
        resources: &RendererResources,
    ) -> BuiltDisplayList {
        let list = DisplayList {
            items,
            ..DisplayList::default()
        };
        let (_, built, _) = translate_displaylist_to_wr(
            &list,
            PipelineId(0, 0),
            DeviceIntSize::new(400, 100),
            resources,
            DpiScaleFactor::new(dpi),
            Vec::new(),
            &BTreeMap::new(),
            0,
        )
        .expect("a balanced list translates");
        built
    }

    /// The rects and the clip rects WebRender is handed for `items`, each in
    /// list order.
    fn translated(items: Vec<DisplayListItem>, dpi: f32) -> (Vec<LayoutRect>, Vec<LayoutRect>) {
        let built = built(items, dpi);
        let (mut rects, mut clips) = (Vec::new(), Vec::new());
        let mut iter = built.iter();
        while let Some(item) = iter.next() {
            match item.item() {
                DisplayItem::Rectangle(r) => rects.push(r.bounds),
                DisplayItem::RectClip(c) => clips.push(c.clip_rect),
                _ => {}
            }
        }
        (rects, clips)
    }

    /// A selected tab is `position: relative; z-index: 1`, a stacking
    /// context of its own. Its face and its clip are drawn where the layout
    /// put them, as the CPU renderer and Chrome draw them, and not a second
    /// time the tab's offset further right and down.
    #[test]
    fn a_box_inside_a_stacking_context_is_drawn_where_the_layout_put_it() {
        let tab = rect(60.0, 4.0, 80.0, 28.0);
        let label = rect(78.0, 8.0, 44.0, 20.0);
        for dpi in [1.0, 2.0] {
            let (rects, clips) = translated(
                vec![
                    DisplayListItem::PushStackingContext {
                        z_index: 1,
                        bounds: tab.into(),
                    },
                    fill(tab),
                    DisplayListItem::PushClip {
                        bounds: label.into(),
                        border_radius: BorderRadius::default(),
                    },
                    fill(label),
                    DisplayListItem::PopClip,
                    DisplayListItem::PopStackingContext,
                ],
                dpi,
            );
            assert_eq!(
                rects,
                vec![device(tab, dpi), device(label, dpi)],
                "dpi {dpi}"
            );
            assert_eq!(clips, vec![device(label, dpi)], "dpi {dpi}");
        }
    }

    /// Stacking contexts nest (an icon with a tint filter inside the
    /// selected tab): the inner one moves nothing either.
    #[test]
    fn a_box_inside_nested_stacking_contexts_is_drawn_where_the_layout_put_it() {
        let tab = rect(60.0, 4.0, 80.0, 28.0);
        let icon = rect(66.0, 10.0, 16.0, 16.0);
        let (rects, _) = translated(
            vec![
                DisplayListItem::PushStackingContext {
                    z_index: 1,
                    bounds: tab.into(),
                },
                DisplayListItem::PushStackingContext {
                    z_index: 0,
                    bounds: icon.into(),
                },
                fill(icon),
                DisplayListItem::PopStackingContext,
                DisplayListItem::PopStackingContext,
            ],
            1.0,
        );
        assert_eq!(rects, vec![device(icon, 1.0)]);
    }

    /// A transformed box gets a reference frame at the window origin (the
    /// transform does the moving) and its stacking context inside it: the box
    /// under an identity transform is drawn where the layout put it.
    #[test]
    fn a_box_inside_a_transformed_stacking_context_is_drawn_where_the_layout_put_it() {
        let card = rect(120.0, 30.0, 100.0, 40.0);
        let (rects, _) = translated(
            vec![
                DisplayListItem::PushReferenceFrame {
                    transform_key: TransformKey { id: 1 },
                    initial_transform: ComputedTransform3D::IDENTITY,
                    bounds: card.into(),
                },
                DisplayListItem::PushStackingContext {
                    z_index: 0,
                    bounds: card.into(),
                },
                fill(card),
                DisplayListItem::PopStackingContext,
                DisplayListItem::PopReferenceFrame,
            ],
            1.0,
        );
        assert_eq!(rects, vec![device(card, 1.0)]);
    }
}

#[cfg(test)]
mod gradient_stop_tests {
    //! The stops WebRender is handed for a gradient. WebRender fills its
    //! gradient table from a first stop at 0 to a last stop at 1 and paints
    //! white wherever the stops stop short. A gradient whose stops end early
    //! reaches it mapped onto that range: a linear one through its line, a
    //! radial or conic one through its start and end offsets. The CPU
    //! renderer paints the stops where the CSS puts them, so the two draw the
    //! same gradient.

    use azul_css::props::{
        basic::{angle::AngleValue, color::ColorU, direction::Direction, length::PercentageValue},
        style::background::{
            BackgroundPositionHorizontal, BackgroundPositionVertical, ConicGradient, ExtendMode,
            LinearGradient, NormalizedLinearColorStop, NormalizedLinearColorStopVec,
            NormalizedRadialColorStop, NormalizedRadialColorStopVec, RadialGradient,
            RadialGradientSize, Shape, StyleBackgroundPosition,
        },
    };
    use azul_layout::solver3::display_list::{BorderRadius, DisplayListItem};
    use webrender::api::DisplayItem;

    use super::stacking_context_tests::{built, rect};

    const GLOW: ColorU = ColorU::new(255, 253, 238, 82);
    const CLEAR: ColorU = ColorU::new(255, 253, 238, 0);

    fn linear_stops(stops: &[(f32, ColorU)]) -> NormalizedLinearColorStopVec {
        NormalizedLinearColorStopVec::from_vec(
            stops
                .iter()
                .map(|&(percent, color)| {
                    NormalizedLinearColorStop::new(PercentageValue::new(percent), color)
                })
                .collect(),
        )
    }

    fn conic_stops(stops: &[(f32, ColorU)]) -> NormalizedRadialColorStopVec {
        NormalizedRadialColorStopVec::from_vec(
            stops
                .iter()
                .map(|&(deg, color)| NormalizedRadialColorStop::new(AngleValue::deg(deg), color))
                .collect(),
        )
    }

    fn centre() -> StyleBackgroundPosition {
        StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Center,
            vertical: BackgroundPositionVertical::Center,
        }
    }

    fn radial(extend_mode: ExtendMode, stops: &[(f32, ColorU)]) -> DisplayListItem {
        DisplayListItem::RadialGradient {
            bounds: rect(10.0, 10.0, 200.0, 40.0).into(),
            gradient: RadialGradient {
                shape: Shape::Ellipse,
                size: RadialGradientSize::FarthestSide,
                position: centre(),
                extend_mode,
                stops: linear_stops(stops),
            },
            border_radius: BorderRadius::default(),
        }
    }

    fn conic(extend_mode: ExtendMode, stops: &[(f32, ColorU)]) -> DisplayListItem {
        DisplayListItem::ConicGradient {
            bounds: rect(10.0, 10.0, 80.0, 80.0).into(),
            gradient: ConicGradient {
                extend_mode,
                center: centre(),
                angle: AngleValue::deg(0.0),
                stops: conic_stops(stops),
            },
            border_radius: BorderRadius::default(),
        }
    }

    fn linear(
        direction: Direction,
        extend_mode: ExtendMode,
        stops: &[(f32, ColorU)],
    ) -> DisplayListItem {
        DisplayListItem::LinearGradient {
            bounds: rect(10.0, 10.0, 200.0, 40.0).into(),
            gradient: LinearGradient {
                direction,
                extend_mode,
                stops: linear_stops(stops),
            },
            border_radius: BorderRadius::default(),
        }
    }

    /// One gradient as WebRender is handed it: the offsets of its stops and,
    /// for a radial or conic gradient, the start and end offsets its stops
    /// are mapped from.
    #[derive(Debug)]
    struct Handed {
        offsets: Vec<f32>,
        range: Option<(f32, f32)>,
    }

    fn handed(item: DisplayListItem) -> Vec<Handed> {
        let list = built(vec![item], 1.0);
        let mut out = Vec::new();
        let mut iter = list.iter();
        while let Some(item) = iter.next() {
            let range = match item.item() {
                DisplayItem::Gradient(_) => None,
                DisplayItem::RadialGradient(g) => {
                    Some((g.gradient.start_offset, g.gradient.end_offset))
                }
                DisplayItem::ConicGradient(g) => {
                    Some((g.gradient.start_offset, g.gradient.end_offset))
                }
                _ => continue,
            };
            let offsets = item.gradient_stops().iter().map(|s| s.offset).collect();
            out.push(Handed { offsets, range });
        }
        out
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// flora's blue stone lays a bloom over its face that fades out 45% of
    /// the way to the ellipse. WebRender is handed it as stops from 0 to 1
    /// over the first 45% of the ray, so past that ring the bloom is
    /// transparent, as the CPU renderer paints it, and not white.
    #[test]
    fn a_radial_gradient_that_fades_out_at_45_percent_reaches_webrender_as_stops_from_0_to_1() {
        let got = handed(radial(ExtendMode::Clamp, &[(0.0, GLOW), (45.0, CLEAR)]));
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].offsets, vec![0.0, 1.0], "{got:?}");
        let (start, end) = got[0].range.expect("a radial gradient has a range");
        assert!(close(start, 0.0) && close(end, 0.45), "{got:?}");
    }

    /// A radial gradient that runs on past its ellipse keeps changing out to
    /// the last stop, so its stops are mapped from beyond 1, not cut there.
    #[test]
    fn a_radial_gradient_with_stops_past_its_ellipse_reaches_webrender_mapped_from_beyond_1() {
        let got = handed(radial(ExtendMode::Clamp, &[(20.0, GLOW), (150.0, CLEAR)]));
        assert_eq!(got[0].offsets, vec![0.0, 1.0], "{got:?}");
        let (start, end) = got[0].range.expect("a radial gradient has a range");
        assert!(close(start, 0.2) && close(end, 1.5), "{got:?}");
    }

    /// A conic gradient that ends half way round is mapped from 0..0.5 of
    /// the turn.
    #[test]
    fn a_conic_gradient_that_ends_half_way_round_reaches_webrender_as_stops_from_0_to_1() {
        let got = handed(conic(ExtendMode::Clamp, &[(0.0, GLOW), (180.0, CLEAR)]));
        assert_eq!(got[0].offsets, vec![0.0, 1.0], "{got:?}");
        let (start, end) = got[0].range.expect("a conic gradient has a range");
        assert!(close(start, 0.0) && close(end, 0.5), "{got:?}");
    }

    /// Every kind of gradient, clamped and repeating, with stops that end
    /// early, start late, or change hard: WebRender is handed stops that run
    /// from 0 to 1 and never go back.
    #[test]
    fn every_gradient_reaches_webrender_with_stops_that_run_from_0_to_1() {
        let hard = [(10.0, GLOW), (30.0, GLOW), (30.0, CLEAR), (45.0, CLEAR)];
        let early = [(0.0, GLOW), (45.0, CLEAR)];
        let late = [(30.0, GLOW), (80.0, CLEAR)];
        let cases = [
            (
                "linear to bottom",
                linear(Direction::default(), ExtendMode::Clamp, &early),
            ),
            (
                "linear 135deg",
                linear(
                    Direction::Angle(AngleValue::deg(135.0)),
                    ExtendMode::Clamp,
                    &late,
                ),
            ),
            (
                "linear hard stops",
                linear(Direction::default(), ExtendMode::Clamp, &hard),
            ),
            (
                "repeating linear",
                linear(Direction::default(), ExtendMode::Repeat, &late),
            ),
            ("radial", radial(ExtendMode::Clamp, &late)),
            ("radial hard stops", radial(ExtendMode::Clamp, &hard)),
            ("repeating radial", radial(ExtendMode::Repeat, &early)),
            (
                "conic",
                conic(ExtendMode::Clamp, &[(90.0, GLOW), (270.0, CLEAR)]),
            ),
            (
                "repeating conic",
                conic(ExtendMode::Repeat, &[(0.0, GLOW), (60.0, CLEAR)]),
            ),
        ];
        for (name, item) in cases {
            let got = handed(item);
            assert_eq!(got.len(), 1, "{name}: {got:?}");
            let offsets = &got[0].offsets;
            assert!(
                offsets.len() >= 2
                    && close(offsets[0], 0.0)
                    && close(*offsets.last().unwrap(), 1.0)
                    && offsets.windows(2).all(|w| w[0] <= w[1]),
                "{name}: {got:?}"
            );
        }
    }
}

#[cfg(test)]
mod image_mask_tests {
    //! What WebRender is handed for content clipped by an image mask: an SVG
    //! path clip, such as the S-curves of flora's selected tab. WebRender
    //! applies an image mask to a stacking context, which it draws to a
    //! surface and masks as a whole (Gecko hands it `clip-path` that way).
    //! An item that only carries the mask in its own clip chain is drawn
    //! unmasked when it takes the brush path: a gradient, text, an image.

    use azul_core::resources::{
        IdNamespace, ImageDescriptor, ImageDescriptorFlags, ImageKey, ImageRef, RawImageFormat,
        RendererResources, ResolvedImage,
    };
    use azul_css::props::{
        basic::{color::ColorU, direction::Direction, length::PercentageValue},
        style::background::{
            ExtendMode, LinearGradient, NormalizedLinearColorStop, NormalizedLinearColorStopVec,
        },
    };
    use azul_layout::solver3::display_list::{BorderRadius, DisplayListItem};
    use webrender::api::{ClipChainId, ClipId, DisplayItem};

    use super::stacking_context_tests::{built_with, rect};

    /// An R8 mask registered with the renderer, as `register_frame_resources`
    /// registers one before the list is translated.
    fn registered_mask() -> (ImageRef, RendererResources) {
        let mask = ImageRef::null_image(18, 28, RawImageFormat::R8, Vec::new());
        let mut resources = RendererResources::default();
        resources.currently_registered_images.insert(
            mask.get_hash(),
            ResolvedImage {
                key: ImageKey {
                    namespace: IdNamespace(1),
                    key: 7,
                },
                descriptor: ImageDescriptor {
                    format: RawImageFormat::R8,
                    width: 18,
                    height: 28,
                    stride: azul_css::OptionI32::None,
                    offset: 0,
                    flags: ImageDescriptorFlags {
                        is_opaque: false,
                        allow_mipmaps: false,
                    },
                },
            },
        );
        (mask, resources)
    }

    /// The metal of a tab's curve: a gradient over the curve's whole box.
    fn metal() -> DisplayListItem {
        DisplayListItem::LinearGradient {
            bounds: rect(42.0, 4.0, 18.0, 28.0).into(),
            gradient: LinearGradient {
                direction: Direction::default(),
                extend_mode: ExtendMode::Clamp,
                stops: NormalizedLinearColorStopVec::from_vec(vec![
                    NormalizedLinearColorStop::new(
                        PercentageValue::new(0.0),
                        ColorU::new(236, 222, 180, 255),
                    ),
                    NormalizedLinearColorStop::new(
                        PercentageValue::new(100.0),
                        ColorU::new(198, 178, 121, 255),
                    ),
                ]),
            },
            border_radius: BorderRadius::default(),
        }
    }

    /// What happened to the image mask in a built list, in list order.
    #[derive(Debug, PartialEq)]
    enum Seen {
        /// A stacking context whose clip chain holds the image mask.
        MaskedContext,
        /// Any other stacking context.
        Context,
        Pop,
        Gradient,
    }

    fn seen(items: Vec<DisplayListItem>, resources: &RendererResources) -> Vec<Seen> {
        let list = built_with(items, 1.0, resources);
        let mut masks: Vec<ClipId> = Vec::new();
        let mut masked_chains: Vec<ClipChainId> = Vec::new();
        let mut out = Vec::new();
        let mut iter = list.iter();
        while let Some(item) = iter.next() {
            match item.item() {
                DisplayItem::ImageMaskClip(m) => masks.push(m.id),
                DisplayItem::ClipChain(chain) => {
                    let holds_mask = item.clip_chain_items().iter().any(|c| masks.contains(c))
                        || chain.parent.is_some_and(|p| masked_chains.contains(&p));
                    if holds_mask {
                        masked_chains.push(chain.id);
                    }
                }
                DisplayItem::PushStackingContext(sc) => {
                    out.push(match sc.stacking_context.clip_chain_id {
                        Some(chain) if masked_chains.contains(&chain) => Seen::MaskedContext,
                        _ => Seen::Context,
                    })
                }
                DisplayItem::PopStackingContext => out.push(Seen::Pop),
                DisplayItem::Gradient(_) => out.push(Seen::Gradient),
                _ => {}
            }
        }
        out
    }

    /// The metal of flora's tab curve is cut to the S by the path's mask, as
    /// the CPU renderer and Chrome cut it, not drawn as the curve's whole
    /// box.
    #[test]
    fn a_gradient_under_an_image_mask_is_drawn_in_a_stacking_context_that_carries_the_mask() {
        let (mask, resources) = registered_mask();
        let curve = rect(42.0, 4.0, 18.0, 28.0);
        let got = seen(
            vec![
                DisplayListItem::PushImageMaskClip {
                    bounds: curve.into(),
                    mask_image: mask,
                    mask_rect: curve.into(),
                },
                metal(),
                DisplayListItem::PopImageMaskClip,
                metal(),
            ],
            &resources,
        );
        assert_eq!(
            got,
            vec![
                Seen::MaskedContext,
                Seen::Gradient,
                Seen::Pop,
                Seen::Gradient
            ]
        );
    }

    /// The selected tab is a stacking context of its own and its curves are
    /// masked inside it: the masked context opens and closes inside the
    /// tab's.
    #[test]
    fn an_image_mask_inside_a_stacking_context_opens_and_closes_inside_it() {
        let (mask, resources) = registered_mask();
        let tab = rect(60.0, 4.0, 80.0, 28.0);
        let curve = rect(42.0, 4.0, 18.0, 28.0);
        let got = seen(
            vec![
                DisplayListItem::PushStackingContext {
                    z_index: 1,
                    bounds: tab.into(),
                },
                DisplayListItem::PushImageMaskClip {
                    bounds: curve.into(),
                    mask_image: mask,
                    mask_rect: curve.into(),
                },
                metal(),
                DisplayListItem::PopImageMaskClip,
                DisplayListItem::PopStackingContext,
            ],
            &resources,
        );
        assert_eq!(
            got,
            vec![
                Seen::Context,
                Seen::MaskedContext,
                Seen::Gradient,
                Seen::Pop,
                Seen::Pop
            ]
        );
    }

    /// A mask whose image never reached the renderer clips nothing, and the
    /// list stays balanced.
    #[test]
    fn an_unregistered_image_mask_leaves_its_content_unmasked_and_the_list_balanced() {
        let mask = ImageRef::null_image(18, 28, RawImageFormat::R8, Vec::new());
        let curve = rect(42.0, 4.0, 18.0, 28.0);
        let got = seen(
            vec![
                DisplayListItem::PushImageMaskClip {
                    bounds: curve.into(),
                    mask_image: mask,
                    mask_rect: curve.into(),
                },
                metal(),
                DisplayListItem::PopImageMaskClip,
            ],
            &RendererResources::default(),
        );
        assert_eq!(got, vec![Seen::Gradient]);
    }
}

#[cfg(test)]
mod rounded_corner_tests {
    //! The corners WebRender is handed for a box with `border-radius`. The
    //! display list gives radii in CSS px, as it gives every length;
    //! WebRender works in device px, so at 2x a 6px corner is 12 device px,
    //! as the CPU renderer draws it.

    use azul_css::props::basic::pixel::PixelValue;
    use azul_css::props::{
        basic::{color::ColorU, direction::Direction, length::PercentageValue},
        style::background::{
            ExtendMode, LinearGradient, NormalizedLinearColorStop, NormalizedLinearColorStopVec,
        },
    };
    use azul_layout::solver3::display_list::{BorderRadius, DisplayListItem};
    use webrender::api::DisplayItem;

    use super::stacking_context_tests::{built, rect};

    fn corners(r: f32) -> BorderRadius {
        BorderRadius {
            top_left: r,
            top_right: r,
            bottom_left: r,
            bottom_right: r,
        }
    }

    /// The top-left radius of every rounded clip WebRender is handed, in
    /// list order.
    fn radii(items: Vec<DisplayListItem>, dpi: f32) -> Vec<f32> {
        let list = built(items, dpi);
        let mut out = Vec::new();
        let mut iter = list.iter();
        while let Some(item) = iter.next() {
            if let DisplayItem::RoundedRectClip(c) = item.item() {
                out.push(c.clip.radii.top_left.width);
            }
        }
        out
    }

    /// A button face, a rounded overflow clip and a rounded gradient: each
    /// one's corner is its radius times the scale factor.
    #[test]
    fn a_rounded_box_is_clipped_with_its_corners_in_device_pixels() {
        let face = rect(20.0, 10.0, 120.0, 30.0);
        for dpi in [1.0, 2.0] {
            let got = radii(
                vec![
                    DisplayListItem::Rect {
                        bounds: face.into(),
                        color: ColorU::new(30, 60, 170, 255),
                        border_radius: corners(6.0),
                    },
                    DisplayListItem::PushClip {
                        bounds: face.into(),
                        border_radius: corners(6.0),
                    },
                    DisplayListItem::PopClip,
                    DisplayListItem::LinearGradient {
                        bounds: face.into(),
                        gradient: LinearGradient {
                            direction: Direction::default(),
                            extend_mode: ExtendMode::Clamp,
                            stops: NormalizedLinearColorStopVec::from_vec(vec![
                                NormalizedLinearColorStop::new(
                                    PercentageValue::new(0.0),
                                    ColorU::new(30, 60, 170, 255),
                                ),
                                NormalizedLinearColorStop::new(
                                    PercentageValue::new(100.0),
                                    ColorU::new(10, 20, 90, 255),
                                ),
                            ]),
                        },
                        border_radius: corners(6.0),
                    },
                ],
                dpi,
            );
            assert_eq!(got, vec![6.0 * dpi; 3], "dpi {dpi}");
        }
    }

    /// A 1px hairline with 4px corners: WebRender is handed 2 device px
    /// and 8 device px corners at 2x, as the CPU renderer strokes it.
    #[test]
    fn a_border_is_drawn_with_its_widths_and_corners_in_device_pixels() {
        use azul_css::{
            css::CssPropertyValue,
            props::style::{
                border::{
                    BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth,
                    LayoutBorderRightWidth, LayoutBorderTopWidth, StyleBorderBottomColor,
                    StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
                    StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
                    StyleBorderTopStyle,
                },
                border_radius::StyleBorderRadius,
            },
        };
        use azul_layout::solver3::display_list::{
            StyleBorderColors, StyleBorderStyles, StyleBorderWidths,
        };
        use webrender::api::BorderDetails;

        let ink = ColorU::new(120, 110, 90, 255);
        let border = DisplayListItem::Border {
            bounds: rect(20.0, 10.0, 18.0, 18.0).into(),
            widths: StyleBorderWidths {
                top: Some(CssPropertyValue::Exact(LayoutBorderTopWidth::const_px(1))),
                right: Some(CssPropertyValue::Exact(LayoutBorderRightWidth::const_px(1))),
                bottom: Some(CssPropertyValue::Exact(LayoutBorderBottomWidth::const_px(
                    1,
                ))),
                left: Some(CssPropertyValue::Exact(LayoutBorderLeftWidth::const_px(1))),
            },
            colors: StyleBorderColors {
                top: Some(CssPropertyValue::Exact(StyleBorderTopColor { inner: ink })),
                right: Some(CssPropertyValue::Exact(StyleBorderRightColor {
                    inner: ink,
                })),
                bottom: Some(CssPropertyValue::Exact(StyleBorderBottomColor {
                    inner: ink,
                })),
                left: Some(CssPropertyValue::Exact(StyleBorderLeftColor { inner: ink })),
            },
            styles: StyleBorderStyles {
                top: Some(CssPropertyValue::Exact(StyleBorderTopStyle {
                    inner: BorderStyle::Solid,
                })),
                right: Some(CssPropertyValue::Exact(StyleBorderRightStyle {
                    inner: BorderStyle::Solid,
                })),
                bottom: Some(CssPropertyValue::Exact(StyleBorderBottomStyle {
                    inner: BorderStyle::Solid,
                })),
                left: Some(CssPropertyValue::Exact(StyleBorderLeftStyle {
                    inner: BorderStyle::Solid,
                })),
            },
            border_radius: StyleBorderRadius {
                top_left: PixelValue::px(4.0),
                top_right: PixelValue::px(4.0),
                bottom_left: PixelValue::px(4.0),
                bottom_right: PixelValue::px(4.0),
            },
        };
        for dpi in [1.0, 2.0] {
            let list = built(vec![border.clone()], dpi);
            let mut got = Vec::new();
            let mut iter = list.iter();
            while let Some(item) = iter.next() {
                if let DisplayItem::Border(b) = item.item() {
                    let BorderDetails::Normal(normal) = &b.details else {
                        panic!("a solid border is a normal border");
                    };
                    got.push((b.widths.top, b.widths.left, normal.radius.top_left.width));
                }
            }
            assert_eq!(got, vec![(dpi, dpi, 4.0 * dpi)], "dpi {dpi}");
        }
    }
}

#[cfg(test)]
mod reference_frame_tests {
    //! Where WebRender draws a box under a CSS transform. The transform is
    //! computed about the box's own top-left corner (`transform-origin`
    //! resolves against the box, `GpuValueCache` keeps it that way), and the
    //! CPU renderer applies it about that corner; the display list gives the
    //! box itself in window coordinates.

    use azul_core::{
        resources::TransformKey,
        transform::{ComputedTransform3D, RotationMode},
    };
    use azul_css::props::{
        basic::{angle::AngleValue, color::ColorU},
        style::{StyleTransform, StyleTransformOrigin},
    };
    use azul_layout::solver3::display_list::{BorderRadius, DisplayListItem};
    use webrender::api::{
        units::{LayoutPoint, LayoutTransform},
        DisplayItem, PropertyBinding, ReferenceTransformBinding, SpatialId, SpatialTreeItem,
    };

    use super::stacking_context_tests::{built, rect};

    /// Where the WebRender list built from `items` at `dpi` draws `point`
    /// (device px) of its first rect: through every reference frame the
    /// rect sits in, out to the root.
    fn drawn_at(items: Vec<DisplayListItem>, dpi: f32, point: LayoutPoint) -> LayoutPoint {
        let list = built(items, dpi);
        // (frame, its parent, its origin, its transform) of every frame.
        let mut frames: Vec<(SpatialId, SpatialId, LayoutPoint, LayoutTransform)> = Vec::new();
        list.iter_spatial_tree(|item| {
            if let SpatialTreeItem::ReferenceFrame(f) = item {
                let ReferenceTransformBinding::Static { binding } = &f.reference_frame.transform
                else {
                    panic!("compositor2 opens static frames");
                };
                let transform = match binding {
                    PropertyBinding::Value(t) | PropertyBinding::Binding(_, t) => *t,
                };
                frames.push((
                    f.reference_frame.id,
                    f.parent_spatial_id,
                    f.origin,
                    transform,
                ));
            }
        });
        let mut iter = list.iter();
        let mut spatial = None;
        while let Some(item) = iter.next() {
            if let DisplayItem::Rectangle(r) = item.item() {
                spatial = Some(r.common.spatial_id);
                break;
            }
        }
        let mut at = point;
        let mut id = spatial.expect("the rect reaches WebRender");
        while let Some(&(_, parent, origin, transform)) = frames.iter().find(|f| f.0 == id) {
            let moved = transform
                .transform_point2d(at)
                .expect("an affine transform maps every point");
            at = LayoutPoint::new(moved.x + origin.x, moved.y + origin.y);
            id = parent;
        }
        at
    }

    /// flora's tick: a 4 x 8 box with a right and a bottom border, turned
    /// 45 degrees about its centre. Turned, its centre stays where it is, as
    /// the CPU renderer and Chrome draw it - not swung round the window's
    /// corner and out of its check box.
    #[test]
    fn a_rotated_box_turns_about_its_own_centre() {
        let tick = rect(80.0, 190.0, 4.0, 8.0);
        let turn = ComputedTransform3D::from_style_transform_vec(
            &[StyleTransform::Rotate(AngleValue::deg(45.0))],
            &StyleTransformOrigin::default(),
            4.0,
            8.0,
            RotationMode::ForWebRender,
        );
        for dpi in [1.0, 2.0] {
            let centre = LayoutPoint::new(82.0 * dpi, 194.0 * dpi);
            let got = drawn_at(
                vec![
                    DisplayListItem::PushReferenceFrame {
                        transform_key: TransformKey { id: 41 },
                        initial_transform: turn,
                        bounds: tick.into(),
                    },
                    DisplayListItem::Rect {
                        bounds: tick.into(),
                        color: ColorU::new(253, 250, 241, 255),
                        border_radius: BorderRadius::default(),
                    },
                    DisplayListItem::PopReferenceFrame,
                ],
                dpi,
                centre,
            );
            assert!(
                (got.x - centre.x).abs() < 0.01 && (got.y - centre.y).abs() < 0.01,
                "dpi {dpi}: the centre {centre:?} is drawn at {got:?}"
            );
        }
    }

    /// A drag moves a box by a translation alone: it lands that far off its
    /// place, wherever the box is.
    #[test]
    fn a_translated_box_moves_by_its_translation() {
        let card = rect(120.0, 30.0, 100.0, 40.0);
        let got = drawn_at(
            vec![
                DisplayListItem::PushReferenceFrame {
                    transform_key: TransformKey { id: 42 },
                    initial_transform: ComputedTransform3D::new_translation(15.0, -5.0, 0.0),
                    bounds: card.into(),
                },
                DisplayListItem::Rect {
                    bounds: card.into(),
                    color: ColorU::new(30, 60, 170, 255),
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopReferenceFrame,
            ],
            2.0,
            LayoutPoint::new(240.0, 60.0),
        );
        assert!(
            (got.x - 270.0).abs() < 0.01 && (got.y - 50.0).abs() < 0.01,
            "{got:?}"
        );
    }
}

#[cfg(test)]
mod text_halo_tests {
    //! What WebRender is handed for text under a blurred shadow, a map
    //! label's halo. A map pans by drawing every label again where it moved,
    //! on every frame; WebRender draws a blurred shadow as an offscreen
    //! surface blurred in two passes, so a frame of a pan paid that for each
    //! label. A halo is drawn once as a picture and painted as an image.

    use alloc::collections::BTreeMap;

    use azul_core::{
        dom::{Dom, DomId},
        geom::LogicalSize,
        resources::{DpiScaleFactor, RendererResources},
        styled_dom::StyledDom,
    };
    use azul_layout::{
        callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
    };
    use rust_fontconfig::FcFontCache;
    use webrender::{
        api::{units::DeviceIntSize, BuiltDisplayList, DisplayItem, ImageKey, PipelineId},
        render_api::Transaction,
    };

    use super::translate_displaylist_to_wr;
    use crate::desktop::wr_translate2::register_frame_resources;

    const LABELS: usize = 6;
    /// Ink that leaves the labels' shadows as all that paints: at an alpha
    /// of 0.01 it darkens white by three at most.
    const FAINT_INK: &str = "rgba(0, 0, 0, 0.01)";

    /// Six map labels as the map draws them (`label_dom`): absolutely placed,
    /// each in its halo, every other one turned along its road when `turned`;
    /// `dx` moves them all, as a pan does.
    pub(super) fn labels(dx: f32, ink: &str, halo: &str, turned: bool) -> Dom {
        let mut grid = Dom::create_div().with_css(
            "position: absolute; left: 0px; top: 0px; width: 100%; height: 100%; \
             overflow: hidden; background: #ffffff;",
        );
        for i in 0..LABELS {
            let turn = if turned && i % 2 == 1 { " transform: rotate(12.00deg);" } else { "" };
            let style = format!(
                "position: absolute; left: {:.1}px; top: {}px; width: 80px; height: 14px; \
                 margin: 0px; font-size: 12px; line-height: 14px; color: {ink}; \
                 text-align: center; white-space: nowrap; text-shadow: 0px 0px 2px {halo};{turn}",
                20.0 + dx + 90.0 * i as f32,
                40 + 20 * i,
            );
            grid = grid.with_child(
                Dom::create_p()
                    .with_css(style.as_str())
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        format!("Main St {i}"),
                    )),
            );
        }
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div()
                .with_css("position: relative; width: 600px; height: 200px;")
                .with_child(grid),
        )
    }

    /// The WebRender list of one frame of `dom` in `window`, at 2x, its fonts
    /// and images registered as a frame registers them.
    fn frame(window: &mut LayoutWindow, dom: Dom) -> BuiltDisplayList {
        let mut state = FullWindowState::default();
        state.size.dimensions = LogicalSize::new(600.0, 200.0);
        state.size.dpi = 192;
        window.current_window_state = state.clone();
        let mut messages = None;
        window
            .layout_and_generate_display_list(
                StyledDom::create_from_dom(dom),
                &state,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut messages,
            )
            .expect("the labels lay out");
        let dpi = DpiScaleFactor::new(2.0);
        register_frame_resources(window, &mut Transaction::new(), dpi);
        let list = window
            .get_layout_result(&DomId::ROOT_ID)
            .expect("the root DOM is laid out")
            .display_list
            .clone();
        let (_, built, _) = translate_displaylist_to_wr(
            &list,
            PipelineId(0, 0),
            DeviceIntSize::new(1200, 400),
            &window.renderer_resources,
            dpi,
            Vec::new(),
            &BTreeMap::new(),
            0,
        )
        .expect("the frame translates");
        built
    }

    /// The shadows and the images of a built list, in list order.
    fn shadows_and_images(list: &BuiltDisplayList) -> (usize, Vec<ImageKey>) {
        let (mut shadows, mut images) = (0, Vec::new());
        let mut iter = list.iter();
        while let Some(item) = iter.next() {
            match item.item() {
                DisplayItem::PushShadow(_) => shadows += 1,
                DisplayItem::Image(image) => images.push(image.image_key),
                _ => {}
            }
        }
        (shadows, images)
    }

    #[test]
    fn a_panned_maps_label_halos_reach_webrender_as_kept_pictures_not_blurs() {
        let mut window = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        let first = frame(&mut window, labels(0.0, "#333333", "#ffffff", true));
        let moved = frame(&mut window, labels(37.5, "#333333", "#ffffff", true));
        let (shadows, halos) = shadows_and_images(&first);
        assert_eq!(shadows, 0, "no label's halo is a WebRender shadow, a blurred surface");
        assert_eq!(halos.len(), LABELS, "every label's halo is a picture");
        assert_eq!(
            shadows_and_images(&moved),
            (0, halos),
            "the labels moved, their halos did not change: the same pictures"
        );
    }

    /// A halo tinted with its colour is the CPU renderer's text shadow, so the
    /// GPU paints what the CPU paints. Upright labels in all but clear ink -
    /// their shadows are what paints; clear text is not drawn at all, nor its
    /// shadow - and black halos on white: each picture laid over
    /// white where it lands holds the CPU frame's pixels.
    #[test]
    fn a_halo_tinted_with_its_colour_is_the_cpu_renderers_text_shadow() {
        use azul_layout::{
            cpurender::{render, text_halos, RenderOptions},
            glyph_cache::GlyphCache,
        };

        let mut window = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        let _ = frame(&mut window, labels(0.0, FAINT_INK, "#000000", false));
        let list = window
            .get_layout_result(&DomId::ROOT_ID)
            .expect("the root DOM is laid out")
            .display_list
            .clone();
        let cpu = render(
            &list,
            &window.renderer_resources,
            &window.font_manager,
            RenderOptions {
                width: 600.0,
                height: 200.0,
                dpi_factor: 2.0,
            },
            &mut GlyphCache::new(),
        )
        .expect("the CPU renderer draws the frame");
        let halos = text_halos(&list.items, 2.0, None);
        assert_eq!(halos.len(), LABELS, "every label's halo is kept");
        let mut worst = (0u8, (0, 0));
        for halo in &halos {
            let picture = halo.image.get_bytes().expect("a halo is a picture in memory");
            let (x0, y0) = (halo.origin.0 as i64, halo.origin.1 as i64);
            for y in 0..i64::from(halo.height) {
                for x in 0..i64::from(halo.width) {
                    let (px, py) = (x0 + x, y0 + y);
                    if px < 0 || py < 0 || px >= i64::from(cpu.width) || py >= i64::from(cpu.height) {
                        continue;
                    }
                    let coverage = picture[((y * i64::from(halo.width) + x) * 4 + 3) as usize];
                    let over_white = 255 - coverage;
                    let at = ((py * i64::from(cpu.width) + px) * 4) as usize;
                    let difference = cpu.data()[at].abs_diff(over_white);
                    if difference > worst.0 {
                        worst = (difference, (px, py));
                    }
                }
            }
        }
        assert!(
            worst.0 <= 24,
            "a halo laid over white differs from the CPU's shadow by {} at {:?}",
            worst.0,
            worst.1
        );
    }

    /// The same frame drawn by WebRender: the GPU's halos are the CPU's.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_labels_halo_on_the_gpu_is_the_cpu_renderers_halo() {
        use crate::desktop::shell2::macos::headless_gpu::{draw_both, keep, largest_difference};

        let dom = labels(0.0, FAINT_INK, "#000000", false);
        let Some(frames) = draw_both(dom, 600.0, 200.0, 2.0) else {
            return;
        };
        keep("label-halos", &frames);
        let (difference, at) = largest_difference(&frames.gpu, &frames.cpu, (0, 0, 1200, 400));
        assert!(difference <= 24, "the GPU's halos differ from the CPU's by {difference} at {at:?}");
    }
}

#[cfg(test)]
mod flora_window_tests {
    //! A whole flora window as WebRender is handed it: the ribbon's tab strip
    //! (its selected tab's S-curves are SVG path masks over gradients), a tab
    //! bar and the stone buttons, at 1x and 2x. The gradients and the masks
    //! are what the GPU drew wrong; these hold every one of them to the rules
    //! the single-item tests above pin.

    use alloc::collections::BTreeMap;

    use azul_core::{
        dom::{Dom, DomId},
        geom::LogicalSize,
        resources::{DpiScaleFactor, RendererResources},
        styled_dom::StyledDom,
    };
    use azul_css::{AzString, StringVec};
    use azul_layout::{
        callbacks::ExternalSystemCallbacks,
        widgets::{
            button::{Button, ButtonType},
            ribbon::{Ribbon, RibbonAppButton, RibbonTab, RibbonTabVec},
            tabs::TabHeader,
            themes::UiTheme,
        },
        window::LayoutWindow,
        window_state::FullWindowState,
    };
    use rust_fontconfig::FcFontCache;
    use webrender::{
        api::{units::DeviceIntSize, BuiltDisplayList, ClipChainId, ClipId, DisplayItem, PipelineId},
        render_api::Transaction,
    };

    use super::translate_displaylist_to_wr;
    use crate::desktop::wr_translate2::register_frame_resources;

    /// AzDrive's ribbon (the application button, the second tab selected), a
    /// flora tab bar and a primary, a default and a danger button.
    pub(super) fn flora_window() -> Dom {
        let ribbon = Ribbon::new(RibbonTabVec::from_vec(vec![
            RibbonTab::new(AzString::from("Computer")),
            RibbonTab::new(AzString::from("Ansicht")),
        ]))
        .with_theme(UiTheme::Flora)
        .with_app_button(RibbonAppButton::new(AzString::from("Datei")))
        .with_active_tab(1)
        .dom_desktop();
        let tab_bar = TabHeader::create(StringVec::from_vec(vec![
            AzString::from("Allgemein"),
            AzString::from("Freigabe"),
            AzString::from("Sicherheit"),
        ]))
        .with_theme(UiTheme::Flora)
        .with_active_tab(1)
        .dom();
        let mut buttons = Dom::create_div().with_css("display: flex; column-gap: 8px; padding: 8px;");
        for (label, kind) in [
            ("OK", ButtonType::Primary),
            ("Abbrechen", ButtonType::Default),
            ("Entfernen", ButtonType::Danger),
        ] {
            buttons = buttons.with_child(
                Button::create(AzString::from(label))
                    .with_theme(UiTheme::Flora)
                    .with_button_type(kind)
                    .dom(),
            );
        }
        Dom::create_body()
            .with_css("margin: 0px;")
            .with_child(ribbon)
            .with_child(tab_bar)
            .with_child(buttons)
    }

    /// The WebRender list of the flora window at `dpi`, its fonts and images
    /// (the masks among them) registered as a frame registers them.
    fn built(dpi: f32) -> BuiltDisplayList {
        let mut window = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        let mut state = FullWindowState::default();
        state.size.dimensions = LogicalSize::new(500.0, 260.0);
        state.size.dpi = (96.0 * dpi) as u32;
        window.current_window_state = state.clone();
        let mut messages = None;
        window
            .layout_and_generate_display_list(
                StyledDom::create_from_dom(flora_window()),
                &state,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut messages,
            )
            .expect("the window lays out");
        register_frame_resources(&mut window, &mut Transaction::new(), DpiScaleFactor::new(dpi));
        let list = window
            .get_layout_result(&DomId::ROOT_ID)
            .expect("the root DOM is laid out")
            .display_list
            .clone();
        let (_, built, _) = translate_displaylist_to_wr(
            &list,
            PipelineId(0, 0),
            DeviceIntSize::new((500.0 * dpi) as i32, (260.0 * dpi) as i32),
            &window.renderer_resources,
            DpiScaleFactor::new(dpi),
            Vec::new(),
            &BTreeMap::new(),
            0,
        )
        .expect("the window translates");
        built
    }

    /// WebRender fills its gradient table from a stop at 0 to a stop at 1
    /// and logs "Gradient stops abruptly ..., auto-completing to white" for
    /// a gradient whose stops end early - and paints that white. Every
    /// gradient of the window reaches it with stops from 0 to 1, in order.
    #[test]
    fn every_gradient_of_a_flora_window_reaches_webrender_with_stops_from_0_to_1() {
        for dpi in [1.0, 2.0] {
            let list = built(dpi);
            let mut gradients = 0;
            let mut iter = list.iter();
            while let Some(item) = iter.next() {
                let kind = match item.item() {
                    DisplayItem::Gradient(_) => "linear",
                    DisplayItem::RadialGradient(_) => "radial",
                    DisplayItem::ConicGradient(_) => "conic",
                    _ => continue,
                };
                gradients += 1;
                let offsets: Vec<f32> = item.gradient_stops().iter().map(|s| s.offset).collect();
                let ordered = offsets.windows(2).all(|pair| pair[0] <= pair[1]);
                let (first, last) = (offsets.first().copied(), offsets.last().copied());
                assert!(
                    ordered
                        && first.is_some_and(|f| f.abs() < 1e-4)
                        && last.is_some_and(|l| (l - 1.0).abs() < 1e-4),
                    "a {kind} gradient at {dpi}x reaches WebRender with stops {offsets:?}"
                );
            }
            assert!(gradients > 20, "the flora window paints its gradients ({gradients} at {dpi}x)");
        }
    }

    /// WebRender applies an image mask to a stacking context, which it draws
    /// to a surface and masks as a whole; a gradient, text or image that only
    /// carries the mask in its own clip chain is drawn unmasked - the selected
    /// tab's curves were drawn as boxes. Every mask of the window's curves
    /// clips a stacking context and no gradient, text or image. (A plain
    /// colour is masked in its own chain: a stroke is painted that way.)
    #[test]
    fn every_image_mask_of_a_flora_window_clips_a_stacking_context_and_no_single_item() {
        for dpi in [1.0, 2.0] {
            let list = built(dpi);
            let mut masks: Vec<ClipId> = Vec::new();
            let mut masked_chains: Vec<ClipChainId> = Vec::new();
            let mut masked_contexts = 0;
            let mut iter = list.iter();
            while let Some(item) = iter.next() {
                let masked = |chain: ClipChainId, chains: &[ClipChainId]| chains.contains(&chain);
                match item.item() {
                    DisplayItem::ImageMaskClip(mask) => masks.push(mask.id),
                    DisplayItem::ClipChain(chain) => {
                        if item.clip_chain_items().iter().any(|c| masks.contains(c))
                            || chain.parent.is_some_and(|p| masked_chains.contains(&p))
                        {
                            masked_chains.push(chain.id);
                        }
                    }
                    DisplayItem::PushStackingContext(sc) => {
                        if sc
                            .stacking_context
                            .clip_chain_id
                            .is_some_and(|c| masked(c, &masked_chains))
                        {
                            masked_contexts += 1;
                        }
                    }
                    DisplayItem::Gradient(g) => {
                        assert!(!masked(g.common.clip_chain_id, &masked_chains), "a gradient carries a mask at {dpi}x")
                    }
                    DisplayItem::RadialGradient(g) => {
                        assert!(!masked(g.common.clip_chain_id, &masked_chains), "a radial gradient carries a mask")
                    }
                    DisplayItem::Text(t) => {
                        assert!(!masked(t.common.clip_chain_id, &masked_chains), "text carries a mask at {dpi}x")
                    }
                    DisplayItem::Image(i) => {
                        assert!(!masked(i.common.clip_chain_id, &masked_chains), "an image carries a mask at {dpi}x")
                    }
                    _ => {}
                }
            }
            assert!(
                masked_contexts >= 4,
                "the selected tab's and the application button's curves are masked at {dpi}x \
                 ({masked_contexts} masked contexts, {} masks)",
                masks.len()
            );
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tab_curve_pixel_tests {
    //! The curves of flora's selected ribbon tab and application button as
    //! the GPU draws them, held to the CPU renderer's frame. Each curve is a
    //! stone and a band of metal, both cut by an SVG mask; a mask, clip or
    //! border slice that WebRender is handed wrong shows inside the curve's
    //! box as a block, a seam or a bar, where the two frames then part.

    use azul_core::dom::Dom;
    use azul_css::AzString;
    use azul_layout::{
        solver3::display_list::DisplayListItem,
        widgets::{
            ribbon::{Ribbon, RibbonAppButton, RibbonTab, RibbonTabVec},
            themes::UiTheme,
            titlebar::TabsInTitlebar,
        },
    };

    use crate::desktop::shell2::macos::headless_gpu::{draw_both, keep, pixels_differing};

    /// AzDrive's ribbon: the application button and the second tab
    /// selected, the strip in the titlebar or below it.
    fn ribbon(in_titlebar: bool) -> Dom {
        let tabs = RibbonTabVec::from_vec(vec![
            RibbonTab::new(AzString::from("Computer")),
            RibbonTab::new(AzString::from("Ansicht")),
        ]);
        let mut ribbon = Ribbon::new(tabs)
            .with_theme(UiTheme::Flora)
            .with_app_button(RibbonAppButton::new(AzString::from("Datei")))
            .with_active_tab(1);
        if in_titlebar {
            ribbon = ribbon.with_tabs_in_titlebar(TabsInTitlebar::platform());
        }
        Dom::create_body()
            .with_css("margin: 0px;")
            .with_child(ribbon.dom_desktop())
    }

    /// Inside every curve's box at most one pixel in twenty-five differs from
    /// the CPU's frame by more than an edge's antialiasing (32 in a channel)
    /// once the curve's edges may lie a device pixel apart; a bar three CSS
    /// px wide down a curve is still ten in a hundred.
    #[test]
    fn the_ribbons_tab_curves_are_drawn_on_the_gpu_as_on_the_cpu() {
        const DPI: f32 = 2.0;
        for in_titlebar in [false, true] {
            let Some(frames) = draw_both(ribbon(in_titlebar), 500.0, 140.0, DPI) else {
                return;
            };
            keep(&format!("ribbon-curves-titlebar-{in_titlebar}"), &frames);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let curves: Vec<(u32, u32, u32, u32)> = frames
                .display_list
                .items
                .iter()
                .filter_map(|item| match item {
                    DisplayListItem::PushImageMaskClip { mask_rect, .. } => {
                        let r = mask_rect.inner();
                        let (x0, y0) = ((r.origin.x * DPI).floor(), (r.origin.y * DPI).floor());
                        let x1 = ((r.origin.x + r.size.width) * DPI).ceil();
                        let y1 = ((r.origin.y + r.size.height) * DPI).ceil();
                        (x0 >= 0.0 && y0 >= 0.0 && x1 > x0 && y1 > y0).then(|| {
                            (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32)
                        })
                    }
                    _ => None,
                })
                .collect();
            assert!(
                curves.len() >= 8,
                "the application button's and the selected tab's curves are masked \
                 ({} masks, titlebar {in_titlebar})",
                curves.len()
            );
            for curve in curves {
                let (differing, worst) = pixels_differing(&frames.gpu, &frames.cpu, curve, 32);
                assert!(
                    differing * 25 <= curve.2 * curve.3,
                    "{differing} of the {} pixels of the curve at {curve:?} differ from the \
                     CPU's (titlebar {in_titlebar}), the worst by {} at {:?}",
                    curve.2 * curve.3,
                    worst.0,
                    worst.1
                );
            }
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod material_gpu_tests {
    //! The built-in materials as a GPU window draws them - the material
    //! shader run with the window's GL context into a texture of its own -
    //! held to the CPU renderer's frame of the same surfaces: the two
    //! renderers share every formula, so their pictures match to the dither.

    use azul_core::dom::Dom;

    use crate::desktop::shell2::macos::headless_gpu::{draw_gpu_materials, keep, pixels_differing};

    /// A pillowed leather area with a golden line under it, a bevelled denim
    /// button and a vellum leaf.
    fn surfaces() -> Dom {
        Dom::create_body()
            .with_css("margin: 0px;")
            .with_child(Dom::create_div().with_css(
                "width: 240px; height: 60px; background: builtin(leather); \
                 border-radius: 10px 10px 0px 0px; --azul-pillow: 76px .16;",
            ))
            .with_child(
                Dom::create_div()
                    .with_css("width: 240px; height: 4px; background: builtin(aluminium); --azul-bevel: 1px;"),
            )
            .with_child(Dom::create_div().with_css(
                "width: 120px; height: 40px; margin: 8px; border-radius: 6px; \
                 background: builtin(bluejeans); --azul-bevel: 3px;",
            ))
            .with_child(Dom::create_div().with_css("width: 120px; height: 40px; background: builtin(vellum-leaf);"))
    }

    #[test]
    fn a_material_on_the_gpu_is_the_cpu_renderers_material() {
        for dpi in [1.0_f32, 2.0] {
            let Some((frames, on_gpu)) = draw_gpu_materials(surfaces, 260.0, 180.0, dpi) else {
                return;
            };
            keep(&format!("materials-{dpi}x"), &frames);
            assert!(on_gpu, "the GPU drew the materials: the material shader compiled ({dpi}x)");
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (w, h) = ((260.0 * dpi) as u32, (180.0 * dpi) as u32);
            let (differing, worst) = pixels_differing(&frames.gpu, &frames.cpu, (0, 0, w, h), 6);
            assert!(
                differing * 100 <= w * h,
                "{differing} of {} pixels differ by more than 6 levels at {dpi}x, the worst by {} at {:?}",
                w * h,
                worst.0,
                worst.1
            );
        }
    }
}
