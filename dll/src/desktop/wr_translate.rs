//! Type translation functions (from azul-css to webrender types)
//!
//! The reason for doing this is so that azul-core doesn't depend on webrender
//! (since webrender is a huge dependency) just to use the types. Only if you depend on
//! azul (not azul-core), you have to depend on webrender.

use alloc::sync::Arc;
use core::mem;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use azul_core::app_resources::{FontHinting, FontLCDFilter};
use azul_core::{
    app_resources::{
        AddFont, AddFontInstance, AddImage, Au, DpiScaleFactor, Epoch, ExternalImageData,
        ExternalImageId, ExternalImageType, FontInstanceFlags, FontInstanceKey,
        FontInstanceOptions, FontInstancePlatformOptions, FontKey, FontRenderMode, FontVariation,
        GlyphOptions, IdNamespace, ImageBufferKind, ImageCache, ImageData, ImageDescriptor,
        ImageDescriptorFlags, ImageDirtyRect, ImageKey, PrimitiveFlags,
        RawImageFormat as ImageFormat, ResourceUpdate, SyntheticItalics, TransformKey, UpdateImage,
        UpdateImageResult,
    },
    callbacks::{DocumentId, DomNodeId, PipelineId},
    display_list::{
        AlphaType, BoxShadow, CachedDisplayList, DisplayListFrame, DisplayListImageMask,
        DisplayListMsg, DisplayListScrollFrame, GlyphInstance, ImageRendering, LayoutRectContent,
        StyleBorderRadius,
    },
    dom::TagId,
    ui_solver::{
        ComputedTransform3D, ExternalScrollId, LayoutResult, PositionInfo, QuickResizeResult,
    },
    window::{
        CursorPosition, DebugState, FullHitTest, LogicalPosition, LogicalRect, LogicalSize,
        ScrollStates, WindowInternal,
    },
};
use azul_css::{
    BorderSide as CssBorderSide, BorderStyle as CssBorderStyle,
    BoxShadowClipMode as CssBoxShadowClipMode, ColorF as CssColorF, ColorU as CssColorU,
    ExtendMode as CssExtendMode, LayoutPoint as CssLayoutPoint, LayoutRect as CssLayoutRect,
    LayoutSideOffsets as CssLayoutSideOffsets, LayoutSize as CssLayoutSize,
    StyleMixBlendMode as CssMixBlendMode, U8Vec,
};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use webrender::api::{FontHinting as WrFontHinting, FontLCDFilter as WrFontLCDFilter};
use webrender::{
    api::{
        units::{
            DeviceIntPoint as WrDeviceIntPoint, DeviceIntRect as WrDeviceIntRect,
            DeviceIntSize as WrDeviceIntSize, ImageDirtyRect as WrImageDirtyRect,
            LayoutPoint as WrLayoutPoint, LayoutRect as WrLayoutRect,
            LayoutSideOffsets as WrLayoutSideOffsets, LayoutSize as WrLayoutSize,
            LayoutTransform as WrLayoutTransform, LayoutVector2D as WrLayoutVector2D,
        },
        AlphaType as WrAlphaType, ApiHitTester as WrApiHitTester, BorderRadius as WrBorderRadius,
        BorderSide as WrBorderSide, BorderStyle as WrBorderStyle,
        BoxShadowClipMode as WrBoxShadowClipMode, BuiltDisplayList as WrBuiltDisplayList,
        ClipId as WrClipId, ColorF as WrColorF, ColorU as WrColorU,
        CommonItemProperties as WrCommonItemProperties, DebugFlags as WrDebugFlags,
        DisplayListBuilder as WrDisplayListBuilder, DocumentId as WrDocumentId, Epoch as WrEpoch,
        ExtendMode as WrExtendMode, ExternalImageData as WrExternalImageData,
        ExternalImageId as WrExternalImageId, ExternalImageType as WrExternalImageType,
        ExternalScrollId as WrExternalScrollId, FontInstanceFlags as WrFontInstanceFlags,
        FontInstanceKey as WrFontInstanceKey, FontInstanceOptions as WrFontInstanceOptions,
        FontInstancePlatformOptions as WrFontInstancePlatformOptions, FontKey as WrFontKey,
        FontRenderMode as WrFontRenderMode, FontVariation as WrFontVariation,
        GlyphInstance as WrGlyphInstance, GlyphOptions as WrGlyphOptions,
        HitTesterRequest as WrHitTesterRequest, IdNamespace as WrIdNamespace,
        ImageBufferKind as WrImageBufferKind, ImageData as WrImageData,
        ImageDescriptor as WrImageDescriptor, ImageDescriptorFlags as WrImageDescriptorFlags,
        ImageFormat as WrImageFormat, ImageKey as WrImageKey, ImageMask as WrImageMask,
        ImageRendering as WrImageRendering, MixBlendMode as WrMixBlendMode,
        PipelineId as WrPipelineId, PrimitiveFlags as WrPrimitiveFlags,
        PropertyBinding as WrPropertyBinding, ReferenceFrameKind as WrReferenceFrameKind,
        SpaceAndClipInfo as WrSpaceAndClipInfo, SpatialId as WrSpatialId,
        SyntheticItalics as WrSyntheticItalics, TransformStyle as WrTransformStyle,
    },
    render_api::{
        AddFont as WrAddFont, AddFontInstance as WrAddFontInstance, AddImage as WrAddImage,
        RenderApi as WrRenderApi, ResourceUpdate as WrResourceUpdate, Transaction as WrTransaction,
        UpdateImage as WrUpdateImage,
    },
    Renderer,
};

pub enum AsyncHitTester {
    Requested(WrHitTesterRequest),
    Resolved(Arc<dyn WrApiHitTester>),
}

impl AsyncHitTester {
    pub fn resolve(&mut self) -> Arc<dyn WrApiHitTester> {
        let mut _swap: Self = unsafe { mem::zeroed() };
        mem::swap(self, &mut _swap);
        let mut new = match _swap {
            AsyncHitTester::Requested(r) => r.resolve(),
            AsyncHitTester::Resolved(r) => r.clone(),
        };
        let r = new.clone();
        let mut swap_back = AsyncHitTester::Resolved(new.clone());
        mem::swap(self, &mut swap_back);
        mem::forget(swap_back);
        return r;
    }
}

/// Same interface as azul-core: FullHitTest::new
/// but uses webrender to compare the results of the two hit-testing implementations
pub(crate) fn fullhittest_new_webrender(
    wr_hittester: &dyn WrApiHitTester,
    document_id: DocumentId,
    old_focus_node: Option<DomNodeId>,

    layout_results: &[LayoutResult],
    cursor_position: &CursorPosition,
    hidpi_factor: f32,
) -> FullHitTest {
    use alloc::collections::BTreeMap;

    use azul_core::{
        callbacks::{HitTestItem, ScrollHitTestItem},
        styled_dom::{DomId, NodeHierarchyItemId},
    };
    use webrender::api::units::WorldPoint as WrWorldPoint;

    let mut cursor_location = match cursor_position {
        CursorPosition::OutOfWindow(_) | CursorPosition::Uninitialized => {
            return FullHitTest::empty(old_focus_node);
        }
        CursorPosition::InWindow(pos) => LogicalPosition::new(pos.x, pos.y),
    };

    // If there was no new focus found then the focus is set to none
    // NOTE: The following code should NOT use this field for updating,
    // but rather check if the event was a MouseUp event first
    let mut ret = FullHitTest::empty(None);

    let wr_document_id = wr_translate_document_id(document_id);

    let mut dom_ids = vec![(DomId { inner: 0 }, cursor_location)];

    loop {
        let mut new_dom_ids = Vec::new();

        for (dom_id, cursor_relative_to_dom) in dom_ids.iter() {
            let pipeline_id = PipelineId(
                dom_id.inner.min(core::u32::MAX as usize) as u32,
                document_id.id,
            );

            let layout_result = match layout_results.get(dom_id.inner) {
                Some(s) => s,
                None => break,
            };

            let wr_result = wr_hittester.hit_test(
                Some(wr_translate_pipeline_id(pipeline_id)),
                WrWorldPoint::new(
                    cursor_relative_to_dom.x * hidpi_factor,
                    cursor_relative_to_dom.y * hidpi_factor,
                ),
            );

            let hit_items = wr_result
                .items
                .iter()
                .filter_map(|i| {
                    let node_id = layout_result
                        .styled_dom
                        .tag_ids_to_node_ids
                        .iter()
                        .find(|q| q.tag_id.inner == i.tag.0)?
                        .node_id
                        .into_crate_internal()?;

                    let relative_to_item = LogicalPosition::new(
                        i.point_relative_to_item.x / hidpi_factor,
                        i.point_relative_to_item.y / hidpi_factor,
                    );
                    Some((
                        node_id,
                        HitTestItem {
                            point_in_viewport: LogicalPosition::new(
                                i.point_in_viewport.x / hidpi_factor,
                                i.point_in_viewport.y / hidpi_factor,
                            ),
                            point_relative_to_item: relative_to_item,
                            is_iframe_hit: layout_result
                                .iframe_mapping
                                .get(&node_id)
                                .map(|iframe_dom_id| (*iframe_dom_id, relative_to_item)),
                            is_focusable: layout_result
                                .styled_dom
                                .node_data
                                .as_container()
                                .get(node_id)?
                                .get_tab_index()
                                .is_some(),
                        },
                    ))
                })
                .collect::<Vec<_>>();

            for (node_id, item) in hit_items.into_iter() {
                use azul_core::ui_solver::HitTest;

                if let Some(i) = item.is_iframe_hit.as_ref() {
                    new_dom_ids.push(*i);
                }

                if item.is_focusable {
                    ret.focused_node = Some((*dom_id, node_id));
                }

                let az_node_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));

                // NOTE: in order to filter the events correctly,
                // a hit node has to ALWAYS be inserted into the regular_hit_test_nodes
                //
                // It may ADDITIONALLY inserted into the scroll_hit_test_nodes,
                // but not as a replacement!
                ret.hovered_nodes
                    .entry(*dom_id)
                    .or_insert_with(|| HitTest::empty())
                    .regular_hit_test_nodes
                    .insert(node_id, item);

                if let Some(scroll_node) = layout_result
                    .scrollable_nodes
                    .overflowing_nodes
                    .get(&az_node_id)
                {
                    ret.hovered_nodes
                        .entry(*dom_id)
                        .or_insert_with(|| HitTest::empty())
                        .scroll_hit_test_nodes
                        .insert(
                            node_id,
                            ScrollHitTestItem {
                                point_in_viewport: item.point_in_viewport,
                                point_relative_to_item: item.point_relative_to_item,
                                scroll_node: scroll_node.clone(),
                            },
                        );
                }
            }
        }

        if new_dom_ids.is_empty() {
            break;
        } else {
            dom_ids = new_dom_ids;
        }
    }

    ret
}

/// Scroll all nodes in the ScrollStates to their correct position and insert
/// the positions into the transaction
///
/// NOTE: scroll_states has to be mutable, since every key has a "visited" field, to
/// indicate whether it was used during the current frame or not.
pub(crate) fn scroll_all_nodes(scroll_states: &ScrollStates, txn: &mut WrTransaction) {
    use webrender::api::ScrollClamping;

    use crate::desktop::wr_translate::{
        wr_translate_external_scroll_id, wr_translate_logical_position,
    };
    for (key, value) in scroll_states.0.iter() {
        txn.scroll_node_with_id(
            wr_translate_logical_position(value.get()),
            wr_translate_external_scroll_id(*key),
            ScrollClamping::ToContentBounds,
        );
    }
}

/// Synchronize transform / opacity keys
pub(crate) fn synchronize_gpu_values(
    layout_results: &[LayoutResult],
    dpi: &DpiScaleFactor,
    txn: &mut WrTransaction,
) {
    use webrender::api::{
        DynamicProperties as WrDynamicProperties, PropertyBindingKey as WrPropertyBindingKey,
        PropertyValue as WrPropertyValue,
    };

    use crate::desktop::wr_translate::wr_translate_layout_transform;

    let transforms = layout_results
        .iter()
        .flat_map(|lr| {
            lr.gpu_value_cache
                .transform_keys
                .iter()
                .filter_map(|(nid, key)| {
                    let mut value = lr
                        .gpu_value_cache
                        .current_transform_values
                        .get(nid)
                        .cloned()?;
                    value.scale_for_dpi(dpi.inner.get());
                    Some((key, value))
                })
                .collect::<Vec<_>>()
                .into_iter()
        })
        .map(|(k, v)| WrPropertyValue {
            key: WrPropertyBindingKey::new(k.id as u64),
            value: wr_translate_layout_transform(&v),
        })
        .collect::<Vec<_>>();

    let floats = layout_results
        .iter()
        .flat_map(|lr| {
            lr.gpu_value_cache
                .opacity_keys
                .iter()
                .filter_map(|(nid, key)| {
                    let value = lr.gpu_value_cache.current_opacity_values.get(nid)?;
                    Some((key, *value))
                })
                .collect::<Vec<_>>()
                .into_iter()
        })
        .map(|(k, v)| WrPropertyValue {
            key: WrPropertyBindingKey::new(k.id as u64),
            value: v,
        })
        .collect::<Vec<_>>();

    txn.update_dynamic_properties(WrDynamicProperties {
        transforms,
        floats,
        colors: Vec::new(), // TODO: animate colors?
    });
}

pub(crate) fn wr_synchronize_updated_images(
    updated_images: Vec<UpdateImageResult>,
    txn: &mut WrTransaction,
) {
    if updated_images.is_empty() {
        return;
    }

    for updated_image in updated_images {
        if let Some(descriptor) = wr_translate_image_descriptor(updated_image.new_descriptor) {
            txn.update_image(
                wr_translate_image_key(updated_image.key_to_update),
                descriptor,
                wr_translate_image_data(updated_image.new_image_data),
                &WrImageDirtyRect::All,
            );
        }
    }
}

/// Returns the size fo the built display list
#[cfg(not(any(all(macos, test), all(windows, test))))]
pub(crate) fn rebuild_display_list(
    internal: &mut WindowInternal,
    render_api: &mut WrRenderApi,
    image_cache: &ImageCache,
    resources: Vec<ResourceUpdate>,
) {
    use azul_core::{callbacks::PipelineId, styled_dom::DomId, ui_solver::LayoutResult};

    use crate::desktop::wr_translate::{
        wr_translate_display_list, wr_translate_document_id, wr_translate_epoch,
        wr_translate_pipeline_id, wr_translate_resource_update,
    };

    let mut txn = WrTransaction::new();

    // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
    let root_id = DomId { inner: 0 };
    let mut cached_display_list = LayoutResult::get_cached_display_list(
        &internal.document_id,
        root_id,
        internal.epoch,
        &internal.layout_results,
        &internal.current_window_state,
        &internal.gl_texture_cache,
        &internal.renderer_resources,
        image_cache,
    );

    // Scale everything in the display list to the DPI of the window
    cached_display_list.scale_for_dpi(internal.current_window_state.size.get_hidpi_factor());

    let root_pipeline_id = PipelineId(0, internal.document_id.id);
    let display_list = wr_translate_display_list(
        internal.document_id,
        render_api,
        cached_display_list,
        root_pipeline_id,
        internal.current_window_state.size.get_hidpi_factor(),
    );

    let physical_size = internal.current_window_state.size.get_physical_size();
    let physical_size = WrLayoutSize::new(physical_size.width as f32, physical_size.height as f32);

    txn.update_resources(
        resources
            .into_iter()
            .filter_map(wr_translate_resource_update)
            .collect(),
    );
    txn.set_display_list(
        wr_translate_epoch(internal.epoch),
        None,
        physical_size.clone(),
        (wr_translate_pipeline_id(root_pipeline_id), display_list),
        true,
    );

    render_api.send_transaction(wr_translate_document_id(internal.document_id), txn);
}

/// Generates a new frame for webrender
// #[cfg(not(test))]
pub(crate) fn generate_frame(
    internal: &mut WindowInternal,
    render_api: &mut WrRenderApi,
    display_list_was_rebuilt: bool,
) {
    use azul_core::callbacks::PipelineId;

    use crate::desktop::wr_translate::{wr_translate_document_id, wr_translate_pipeline_id};

    let mut txn = WrTransaction::new();

    let physical_size = internal.current_window_state.size.get_physical_size();
    let framebuffer_size =
        WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

    // Especially during minimization / maximization of a window, it can happen that the window
    // width or height is zero. In that case, no rendering is necessary (doing so would crash
    // the application, since glTexImage2D may never have a 0 as the width or height.
    if framebuffer_size.width == 0 || framebuffer_size.height == 0 {
        return;
    }

    txn.set_root_pipeline(wr_translate_pipeline_id(PipelineId(
        0,
        internal.document_id.id,
    )));
    txn.set_document_view(WrDeviceIntRect::from_origin_and_size(
        WrDeviceIntPoint::new(0, 0),
        framebuffer_size,
    ));
    scroll_all_nodes(&mut internal.scroll_states, &mut txn);
    synchronize_gpu_values(
        &internal.layout_results,
        &internal.get_dpi_scale_factor(),
        &mut txn,
    );

    if !display_list_was_rebuilt {
        txn.skip_scene_builder(); // avoid rebuilding the scene if DL hasn't changed
    }

    txn.generate_frame(0);

    render_api.send_transaction(wr_translate_document_id(internal.document_id), txn);
}

#[inline]
fn wr_translate_image_mask(input: &DisplayListImageMask) -> WrImageMask {
    WrImageMask {
        image: wr_translate_image_key(input.image),
        rect: wr_translate_logical_rect(input.rect),
        repeat: input.repeat,
    }
}

#[inline]
fn wr_translate_layouted_glyphs(input: &[GlyphInstance]) -> Vec<WrGlyphInstance> {
    input
        .iter()
        .map(|glyph| WrGlyphInstance {
            index: glyph.index,
            point: WrLayoutPoint::new(glyph.point.x, glyph.point.y),
        })
        .collect()
}

#[inline(always)]
pub(crate) const fn wr_translate_mix_blend_mode(mix_blend_mode: CssMixBlendMode) -> WrMixBlendMode {
    match mix_blend_mode {
        CssMixBlendMode::Normal => WrMixBlendMode::Normal,
        CssMixBlendMode::Multiply => WrMixBlendMode::Multiply,
        CssMixBlendMode::Screen => WrMixBlendMode::Screen,
        CssMixBlendMode::Overlay => WrMixBlendMode::Overlay,
        CssMixBlendMode::Darken => WrMixBlendMode::Darken,
        CssMixBlendMode::Lighten => WrMixBlendMode::Lighten,
        CssMixBlendMode::ColorDodge => WrMixBlendMode::ColorDodge,
        CssMixBlendMode::ColorBurn => WrMixBlendMode::ColorBurn,
        CssMixBlendMode::HardLight => WrMixBlendMode::HardLight,
        CssMixBlendMode::SoftLight => WrMixBlendMode::SoftLight,
        CssMixBlendMode::Difference => WrMixBlendMode::Difference,
        CssMixBlendMode::Exclusion => WrMixBlendMode::Exclusion,
        CssMixBlendMode::Hue => WrMixBlendMode::Hue,
        CssMixBlendMode::Saturation => WrMixBlendMode::Saturation,
        CssMixBlendMode::Color => WrMixBlendMode::Color,
        CssMixBlendMode::Luminosity => WrMixBlendMode::Luminosity,
    }
}

#[inline(always)]
pub(crate) const fn wr_translate_epoch(epoch: Epoch) -> WrEpoch {
    WrEpoch(epoch.into_u32())
}

// webrender -> core

#[inline(always)]
pub(crate) const fn translate_id_namespace_wr(ns: WrIdNamespace) -> IdNamespace {
    IdNamespace(ns.0)
}

#[inline(always)]
pub(crate) const fn translate_pipeline_id_wr(pipeline_id: WrPipelineId) -> PipelineId {
    PipelineId(pipeline_id.0, pipeline_id.1)
}

#[inline(always)]
pub(crate) const fn translate_document_id_wr(document_id: WrDocumentId) -> DocumentId {
    DocumentId {
        namespace_id: translate_id_namespace_wr(document_id.namespace_id),
        id: document_id.id,
    }
}

#[inline(always)]
pub(crate) const fn translate_font_key_wr(font_key: WrFontKey) -> FontKey {
    FontKey {
        key: font_key.1,
        namespace: translate_id_namespace_wr(font_key.0),
    }
}

#[inline(always)]
pub(crate) const fn translate_font_instance_key_wr(
    font_instance_key: WrFontInstanceKey,
) -> FontInstanceKey {
    FontInstanceKey {
        key: font_instance_key.1,
        namespace: translate_id_namespace_wr(font_instance_key.0),
    }
}

#[inline(always)]
pub(crate) const fn translate_image_key_wr(image_key: WrImageKey) -> ImageKey {
    ImageKey {
        key: image_key.1,
        namespace: translate_id_namespace_wr(image_key.0),
    }
}

#[inline(always)]
pub(crate) const fn translate_epoch_wr(epoch: WrEpoch) -> Epoch {
    Epoch::from(epoch.0)
}

/*
#[inline]
pub(crate) fn translate_image_descriptor_wr(descriptor: WrImageDescriptor) -> ImageDescriptor {
    ImageDescriptor {
        format: translate_image_format_wr(descriptor.format),
        width: descriptor.size.width as usize,
        height: descriptor.size.height as usize,
        stride: descriptor.stride.into(),
        offset: descriptor.offset,
        flags: translate_image_descriptor_flags_wr(descriptor.flags),
    }
}

#[inline]
pub(crate) fn translate_image_descriptor_flags_wr(flags: WrImageDescriptorFlags) -> ImageDescriptorFlags {
    ImageDescriptorFlags {
        is_opaque: flags.contains(WrImageDescriptorFlags::IS_OPAQUE),
        allow_mipmaps: flags.contains(WrImageDescriptorFlags::ALLOW_MIPMAPS),
    }
}

#[inline]
pub fn translate_image_format_wr(input: WrImageFormat) -> ImageFormat {
    match input {
        WrImageFormat::R8 => ImageFormat::R8,
        WrImageFormat::R16 => ImageFormat::R16,
        WrImageFormat::RG16 => ImageFormat::RG16,
        WrImageFormat::BGRA8 => ImageFormat::BGRA8,
        WrImageFormat::RGBAF32 => ImageFormat::RGBAF32,
        WrImageFormat::RG8 => ImageFormat::RG8,
        WrImageFormat::RGBAI32 => ImageFormat::RGBAI32,
        WrImageFormat::RGBA8 => ImageFormat::RGBA8,
    }
}
*/

// core -> webrender

#[inline(always)]
const fn wr_translate_id_namespace(ns: IdNamespace) -> WrIdNamespace {
    WrIdNamespace(ns.0)
}

#[inline(always)]
pub(crate) const fn wr_translate_font_key(font_key: FontKey) -> WrFontKey {
    WrFontKey(wr_translate_id_namespace(font_key.namespace), font_key.key)
}

#[inline(always)]
pub(crate) const fn wr_translate_font_instance_key(
    font_instance_key: FontInstanceKey,
) -> WrFontInstanceKey {
    WrFontInstanceKey(
        wr_translate_id_namespace(font_instance_key.namespace),
        font_instance_key.key,
    )
}

#[inline(always)]
pub(crate) const fn wr_translate_image_key(image_key: ImageKey) -> WrImageKey {
    WrImageKey(
        wr_translate_id_namespace(image_key.namespace),
        image_key.key,
    )
}

#[inline(always)]
pub(crate) const fn wr_translate_pipeline_id(pipeline_id: PipelineId) -> WrPipelineId {
    WrPipelineId(pipeline_id.0, pipeline_id.1)
}

#[inline(always)]
pub(crate) const fn wr_translate_document_id(document_id: DocumentId) -> WrDocumentId {
    WrDocumentId {
        namespace_id: wr_translate_id_namespace(document_id.namespace_id),
        id: document_id.id,
    }
}

#[inline(always)]
pub(crate) fn wr_translate_logical_size(logical_size: LogicalSize) -> WrLayoutSize {
    WrLayoutSize::new(logical_size.width, logical_size.height)
}

#[inline]
pub(crate) fn wr_translate_image_descriptor(
    descriptor: ImageDescriptor,
) -> Option<WrImageDescriptor> {
    use webrender::api::units::DeviceIntSize;
    Some(WrImageDescriptor {
        format: wr_translate_image_format(descriptor.format)?,
        size: DeviceIntSize::new(descriptor.width as i32, descriptor.height as i32),
        stride: descriptor.stride.into(),
        offset: descriptor.offset,
        flags: wr_translate_image_descriptor_flags(descriptor.flags),
    })
}

#[inline]
pub(crate) fn wr_translate_image_descriptor_flags(
    flags: ImageDescriptorFlags,
) -> WrImageDescriptorFlags {
    let mut f = WrImageDescriptorFlags::empty();
    f.set(WrImageDescriptorFlags::IS_OPAQUE, flags.is_opaque);
    f.set(WrImageDescriptorFlags::ALLOW_MIPMAPS, flags.allow_mipmaps);
    f
}

#[inline(always)]
pub(crate) fn wr_translate_add_font_instance(
    add_font_instance: AddFontInstance,
) -> WrAddFontInstance {
    WrAddFontInstance {
        key: wr_translate_font_instance_key(add_font_instance.key),
        font_key: wr_translate_font_key(add_font_instance.font_key),
        glyph_size: add_font_instance.glyph_size.0.into_px()
            * add_font_instance.glyph_size.1.inner.get(), // note: Au is now in pixels (f32)
        options: add_font_instance
            .options
            .map(wr_translate_font_instance_options),
        platform_options: add_font_instance
            .platform_options
            .map(wr_translate_font_instance_platform_options),
        variations: add_font_instance
            .variations
            .into_iter()
            .map(wr_translate_font_variation)
            .collect(),
    }
}

#[inline(always)]
fn wr_translate_font_instance_options(fio: FontInstanceOptions) -> WrFontInstanceOptions {
    WrFontInstanceOptions {
        render_mode: wr_translate_font_render_mode(fio.render_mode),
        flags: wr_translate_font_instance_flags(fio.flags),
        bg_color: wr_translate_color_u(fio.bg_color),
        synthetic_italics: wr_translate_synthetic_italics(fio.synthetic_italics),
    }
}

const fn wr_translate_synthetic_italics(si: SyntheticItalics) -> WrSyntheticItalics {
    WrSyntheticItalics { angle: si.angle }
}

#[cfg(target_os = "windows")]
#[inline(always)]
const fn wr_translate_font_instance_platform_options(
    fio: FontInstancePlatformOptions,
) -> WrFontInstancePlatformOptions {
    WrFontInstancePlatformOptions {
        gamma: fio.gamma,
        contrast: fio.contrast,
        cleartype_level: fio.cleartype_level,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[inline(always)]
fn wr_translate_font_hinting(lcd: FontHinting) -> WrFontHinting {
    match lcd {
        FontHinting::None => WrFontHinting::None,
        FontHinting::Mono => WrFontHinting::Mono,
        FontHinting::Light => WrFontHinting::Light,
        FontHinting::Normal => WrFontHinting::Normal,
        FontHinting::LCD => WrFontHinting::LCD,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[inline(always)]
fn wr_translate_font_lcd_filter(lcd: FontLCDFilter) -> WrFontLCDFilter {
    match lcd {
        FontLCDFilter::None => WrFontLCDFilter::None,
        FontLCDFilter::Default => WrFontLCDFilter::Default,
        FontLCDFilter::Light => WrFontLCDFilter::Light,
        FontLCDFilter::Legacy => WrFontLCDFilter::Legacy,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[inline]
fn wr_translate_font_instance_platform_options(
    fio: FontInstancePlatformOptions,
) -> WrFontInstancePlatformOptions {
    WrFontInstancePlatformOptions {
        lcd_filter: wr_translate_font_lcd_filter(fio.lcd_filter),
        hinting: wr_translate_font_hinting(fio.hinting),
    }
}

#[cfg(target_os = "macos")]
#[inline(always)]
const fn wr_translate_font_instance_platform_options(
    fio: FontInstancePlatformOptions,
) -> WrFontInstancePlatformOptions {
    WrFontInstancePlatformOptions { unused: fio.unused }
}

#[inline(always)]
const fn wr_translate_font_variation(variation: FontVariation) -> WrFontVariation {
    WrFontVariation {
        tag: variation.tag,
        value: variation.value,
    }
}

#[inline(always)]
pub fn wr_translate_box_shadow_clip_mode(input: CssBoxShadowClipMode) -> WrBoxShadowClipMode {
    match input {
        CssBoxShadowClipMode::Outset => WrBoxShadowClipMode::Outset,
        CssBoxShadowClipMode::Inset => WrBoxShadowClipMode::Inset,
    }
}

#[inline(always)]
pub fn wr_translate_extend_mode(input: CssExtendMode) -> WrExtendMode {
    match input {
        CssExtendMode::Clamp => WrExtendMode::Clamp,
        CssExtendMode::Repeat => WrExtendMode::Repeat,
    }
}

#[inline(always)]
pub fn wr_translate_border_style(input: CssBorderStyle) -> WrBorderStyle {
    match input {
        CssBorderStyle::None => WrBorderStyle::None,
        CssBorderStyle::Solid => WrBorderStyle::Solid,
        CssBorderStyle::Double => WrBorderStyle::Double,
        CssBorderStyle::Dotted => WrBorderStyle::Dotted,
        CssBorderStyle::Dashed => WrBorderStyle::Dashed,
        CssBorderStyle::Hidden => WrBorderStyle::Hidden,
        CssBorderStyle::Groove => WrBorderStyle::Groove,
        CssBorderStyle::Ridge => WrBorderStyle::Ridge,
        CssBorderStyle::Inset => WrBorderStyle::Inset,
        CssBorderStyle::Outset => WrBorderStyle::Outset,
    }
}

#[inline(always)]
pub fn wr_translate_image_format(input: ImageFormat) -> Option<WrImageFormat> {
    // TODO: re-code the image formats !
    match input {
        ImageFormat::R8 => Some(WrImageFormat::R8),
        ImageFormat::RG8 => Some(WrImageFormat::RG8),
        ImageFormat::RGBA8 => Some(WrImageFormat::RGBA8),
        ImageFormat::R16 => Some(WrImageFormat::R16),
        ImageFormat::RG16 => Some(WrImageFormat::RG16),
        ImageFormat::BGRA8 => Some(WrImageFormat::BGRA8),
        _ => None,
    }
}

#[inline(always)]
pub fn wr_translate_layout_side_offsets(input: CssLayoutSideOffsets) -> WrLayoutSideOffsets {
    WrLayoutSideOffsets::new(
        input.top.get(),
        input.right.get(),
        input.bottom.get(),
        input.left.get(),
    )
}

#[inline(always)]
pub const fn wr_translate_color_u(input: CssColorU) -> WrColorU {
    WrColorU {
        r: input.r,
        g: input.g,
        b: input.b,
        a: input.a,
    }
}

#[inline(always)]
pub const fn wr_translate_color_f(input: CssColorF) -> WrColorF {
    WrColorF {
        r: input.r,
        g: input.g,
        b: input.b,
        a: input.a,
    }
}

#[inline]
pub fn wr_translate_border_radius(
    border_radius: StyleBorderRadius,
    rect_size: LogicalSize,
) -> WrBorderRadius {
    let StyleBorderRadius {
        top_left,
        top_right,
        bottom_left,
        bottom_right,
    } = border_radius;

    let w = rect_size.width;
    let h = rect_size.height;

    // The "w / h" is necessary to convert percentage-based values into pixels, for example
    // "border-radius: 50%;"

    let top_left_px_h = top_left
        .and_then(|tl| tl.get_property_or_default())
        .unwrap_or_default()
        .inner
        .to_pixels(w);
    let top_left_px_v = top_left
        .and_then(|tl| tl.get_property_or_default())
        .unwrap_or_default()
        .inner
        .to_pixels(h);

    let top_right_px_h = top_right
        .and_then(|tr| tr.get_property_or_default())
        .unwrap_or_default()
        .inner
        .to_pixels(w);
    let top_right_px_v = top_right
        .and_then(|tr| tr.get_property_or_default())
        .unwrap_or_default()
        .inner
        .to_pixels(h);

    let bottom_left_px_h = bottom_left
        .and_then(|bl| bl.get_property_or_default())
        .unwrap_or_default()
        .inner
        .to_pixels(w);
    let bottom_left_px_v = bottom_left
        .and_then(|bl| bl.get_property_or_default())
        .unwrap_or_default()
        .inner
        .to_pixels(h);

    let bottom_right_px_h = bottom_right
        .and_then(|br| br.get_property_or_default())
        .unwrap_or_default()
        .inner
        .to_pixels(w);
    let bottom_right_px_v = bottom_right
        .and_then(|br| br.get_property_or_default())
        .unwrap_or_default()
        .inner
        .to_pixels(h);

    WrBorderRadius {
        top_left: WrLayoutSize::new(top_left_px_h as f32, top_left_px_v as f32),
        top_right: WrLayoutSize::new(top_right_px_h as f32, top_right_px_v as f32),
        bottom_left: WrLayoutSize::new(bottom_left_px_h as f32, bottom_left_px_v as f32),
        bottom_right: WrLayoutSize::new(bottom_right_px_h as f32, bottom_right_px_v as f32),
    }
}

#[inline]
pub fn wr_translate_border_side(input: CssBorderSide) -> WrBorderSide {
    WrBorderSide {
        color: wr_translate_color_u(input.color).into(),
        style: wr_translate_border_style(input.style),
    }
}

// NOTE: Reverse direction: Translate from webrender::LayoutRect to css::LayoutRect
#[inline(always)]
pub fn wr_translate_css_layout_rect(input: WrLayoutRect) -> CssLayoutRect {
    let size = input.size();
    CssLayoutRect {
        origin: CssLayoutPoint {
            x: input.min.x.round() as isize,
            y: input.min.y.round() as isize,
        },
        size: CssLayoutSize {
            width: size.width.round() as isize,
            height: size.height.round() as isize,
        },
    }
}

#[inline]
fn wr_translate_layout_size(input: CssLayoutSize) -> WrLayoutSize {
    WrLayoutSize::new(input.width as f32, input.height as f32)
}

#[inline]
pub(crate) fn wr_translate_layout_point(input: CssLayoutPoint) -> WrLayoutPoint {
    WrLayoutPoint::new(input.x as f32, input.y as f32)
}

#[inline]
pub(crate) fn wr_translate_logical_position(input: LogicalPosition) -> WrLayoutPoint {
    WrLayoutPoint::new(input.x, input.y)
}

#[inline]
fn wr_translate_logical_rect(input: LogicalRect) -> WrLayoutRect {
    WrLayoutRect::from_origin_and_size(
        wr_translate_logical_position(input.origin),
        wr_translate_logical_size(input.size),
    )
}

#[inline]
fn wr_translate_layout_rect(input: CssLayoutRect) -> WrLayoutRect {
    WrLayoutRect::from_origin_and_size(
        wr_translate_layout_point(input.origin),
        wr_translate_layout_size(input.size),
    )
}

#[inline]
fn translate_layout_size_wr(input: WrLayoutSize) -> CssLayoutSize {
    CssLayoutSize::new(input.width.round() as isize, input.height.round() as isize)
}

#[inline]
fn translate_layout_point_wr(input: WrLayoutPoint) -> CssLayoutPoint {
    CssLayoutPoint::new(input.x.round() as isize, input.y.round() as isize)
}

#[inline]
fn translate_layout_rect_wr(input: WrLayoutRect) -> CssLayoutRect {
    CssLayoutRect::new(
        translate_layout_point_wr(input.min),
        translate_layout_size_wr(input.size()),
    )
}

#[inline]
fn wr_translate_font_instance_flags(font_instance_flags: FontInstanceFlags) -> WrFontInstanceFlags {
    WrFontInstanceFlags::from_bits_truncate(font_instance_flags)
}

#[inline]
fn wr_translate_font_render_mode(font_render_mode: FontRenderMode) -> WrFontRenderMode {
    match font_render_mode {
        FontRenderMode::Mono => WrFontRenderMode::Mono,
        FontRenderMode::Alpha => WrFontRenderMode::Alpha,
        FontRenderMode::Subpixel => WrFontRenderMode::Subpixel,
    }
}

#[inline]
fn wr_translate_primitive_flags(flags: PrimitiveFlags) -> WrPrimitiveFlags {
    let mut f = WrPrimitiveFlags::empty();
    f.set(
        WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
        flags.is_backface_visible,
    );
    f.set(
        WrPrimitiveFlags::IS_SCROLLBAR_CONTAINER,
        flags.is_scrollbar_container,
    );
    f.set(
        WrPrimitiveFlags::IS_SCROLLBAR_THUMB,
        flags.is_scrollbar_thumb,
    );
    f.set(
        WrPrimitiveFlags::PREFER_COMPOSITOR_SURFACE,
        flags.prefer_compositor_surface,
    );
    f.set(
        WrPrimitiveFlags::SUPPORTS_EXTERNAL_COMPOSITOR_SURFACE,
        flags.supports_external_compositor_surface,
    );
    f
}

#[inline]
fn translate_primitive_flags_wr(flags: WrPrimitiveFlags) -> PrimitiveFlags {
    PrimitiveFlags {
        is_backface_visible: flags.contains(WrPrimitiveFlags::IS_BACKFACE_VISIBLE),
        is_scrollbar_container: flags.contains(WrPrimitiveFlags::IS_SCROLLBAR_CONTAINER),
        is_scrollbar_thumb: flags.contains(WrPrimitiveFlags::IS_SCROLLBAR_THUMB),
        prefer_compositor_surface: flags.contains(WrPrimitiveFlags::PREFER_COMPOSITOR_SURFACE),
        supports_external_compositor_surface: flags
            .contains(WrPrimitiveFlags::SUPPORTS_EXTERNAL_COMPOSITOR_SURFACE),
    }
}

#[inline]
fn wr_translate_glyph_options(glyph_options: GlyphOptions) -> WrGlyphOptions {
    WrGlyphOptions {
        render_mode: wr_translate_font_render_mode(glyph_options.render_mode),
        flags: wr_translate_font_instance_flags(glyph_options.flags),
    }
}

#[inline]
fn wr_translate_image_rendering(image_rendering: ImageRendering) -> WrImageRendering {
    match image_rendering {
        ImageRendering::Auto => WrImageRendering::Auto,
        ImageRendering::CrispEdges => WrImageRendering::CrispEdges,
        ImageRendering::Pixelated => WrImageRendering::Pixelated,
    }
}

#[inline]
fn wr_translate_alpha_type(alpha_type: AlphaType) -> WrAlphaType {
    match alpha_type {
        AlphaType::Alpha => WrAlphaType::Alpha,
        AlphaType::PremultipliedAlpha => WrAlphaType::PremultipliedAlpha,
    }
}

pub(crate) fn wr_translate_debug_flags(new_flags: &DebugState) -> WrDebugFlags {
    let mut debug_flags = WrDebugFlags::empty();

    debug_flags.set(WrDebugFlags::PROFILER_DBG, new_flags.profiler_dbg);
    debug_flags.set(WrDebugFlags::RENDER_TARGET_DBG, new_flags.render_target_dbg);
    debug_flags.set(WrDebugFlags::TEXTURE_CACHE_DBG, new_flags.texture_cache_dbg);
    debug_flags.set(WrDebugFlags::GPU_TIME_QUERIES, new_flags.gpu_time_queries);
    debug_flags.set(
        WrDebugFlags::GPU_SAMPLE_QUERIES,
        new_flags.gpu_sample_queries,
    );
    debug_flags.set(WrDebugFlags::DISABLE_BATCHING, new_flags.disable_batching);
    debug_flags.set(WrDebugFlags::EPOCHS, new_flags.epochs);
    debug_flags.set(
        WrDebugFlags::ECHO_DRIVER_MESSAGES,
        new_flags.echo_driver_messages,
    );
    debug_flags.set(WrDebugFlags::SHOW_OVERDRAW, new_flags.show_overdraw);
    debug_flags.set(WrDebugFlags::GPU_CACHE_DBG, new_flags.gpu_cache_dbg);
    debug_flags.set(
        WrDebugFlags::TEXTURE_CACHE_DBG_CLEAR_EVICTED,
        new_flags.texture_cache_dbg_clear_evicted,
    );
    debug_flags.set(
        WrDebugFlags::PICTURE_CACHING_DBG,
        new_flags.picture_caching_dbg,
    );
    debug_flags.set(WrDebugFlags::PRIMITIVE_DBG, new_flags.primitive_dbg);
    debug_flags.set(WrDebugFlags::ZOOM_DBG, new_flags.zoom_dbg);
    debug_flags.set(WrDebugFlags::SMALL_SCREEN, new_flags.small_screen);
    debug_flags.set(
        WrDebugFlags::DISABLE_OPAQUE_PASS,
        new_flags.disable_opaque_pass,
    );
    debug_flags.set(
        WrDebugFlags::DISABLE_ALPHA_PASS,
        new_flags.disable_alpha_pass,
    );
    debug_flags.set(
        WrDebugFlags::DISABLE_CLIP_MASKS,
        new_flags.disable_clip_masks,
    );
    debug_flags.set(
        WrDebugFlags::DISABLE_TEXT_PRIMS,
        new_flags.disable_text_prims,
    );
    debug_flags.set(
        WrDebugFlags::DISABLE_GRADIENT_PRIMS,
        new_flags.disable_gradient_prims,
    );
    debug_flags.set(WrDebugFlags::OBSCURE_IMAGES, new_flags.obscure_images);
    debug_flags.set(WrDebugFlags::GLYPH_FLASHING, new_flags.glyph_flashing);
    debug_flags.set(WrDebugFlags::SMART_PROFILER, new_flags.smart_profiler);
    debug_flags.set(WrDebugFlags::INVALIDATION_DBG, new_flags.invalidation_dbg);
    debug_flags.set(
        WrDebugFlags::TILE_CACHE_LOGGING_DBG,
        new_flags.tile_cache_logging_dbg,
    );
    debug_flags.set(WrDebugFlags::PROFILER_CAPTURE, new_flags.profiler_capture);
    debug_flags.set(
        WrDebugFlags::FORCE_PICTURE_INVALIDATION,
        new_flags.force_picture_invalidation,
    );

    debug_flags
}

#[inline(always)]
pub(crate) fn wr_translate_resource_update(
    resource_update: ResourceUpdate,
) -> Option<WrResourceUpdate> {
    match resource_update {
        ResourceUpdate::AddFont(af) => Some(WrResourceUpdate::AddFont(wr_translate_add_font(af))),
        ResourceUpdate::DeleteFont(fk) => {
            Some(WrResourceUpdate::DeleteFont(wr_translate_font_key(fk)))
        }
        ResourceUpdate::AddFontInstance(fi) => Some(WrResourceUpdate::AddFontInstance(
            wr_translate_add_font_instance(fi),
        )),
        ResourceUpdate::DeleteFontInstance(fi) => Some(WrResourceUpdate::DeleteFontInstance(
            wr_translate_font_instance_key(fi),
        )),
        ResourceUpdate::AddImage(ai) => {
            Some(WrResourceUpdate::AddImage(wr_translate_add_image(ai)?))
        }
        ResourceUpdate::UpdateImage(ui) => Some(WrResourceUpdate::UpdateImage(
            wr_translate_update_image(ui)?,
        )),
        ResourceUpdate::DeleteImage(k) => {
            Some(WrResourceUpdate::DeleteImage(wr_translate_image_key(k)))
        }
    }
}

#[inline(always)]
fn wr_translate_add_font(add_font: AddFont) -> WrAddFont {
    WrAddFont::Raw(
        wr_translate_font_key(add_font.key),
        u8vec_into_wr_type(add_font.font_bytes),
        add_font.font_index,
    )
}

#[inline(always)]
fn wr_translate_add_image(add_image: AddImage) -> Option<WrAddImage> {
    Some(WrAddImage {
        key: wr_translate_image_key(add_image.key),
        descriptor: wr_translate_image_descriptor(add_image.descriptor)?,
        data: wr_translate_image_data(add_image.data),
        tiling: add_image.tiling,
    })
}

#[inline(always)]
pub(crate) fn wr_translate_image_data(image_data: ImageData) -> WrImageData {
    match image_data {
        ImageData::Raw(data) => WrImageData::Raw(u8vec_into_wr_type(data)),
        ImageData::External(external) => {
            WrImageData::External(wr_translate_external_image_data(external))
        }
    }
}

// TODO: Use -> Cow<'static, [u8]> once webrender PR is merged!
fn u8vec_into_wr_type(data: U8Vec) -> Arc<Vec<u8>> {
    Arc::new(data.into_library_owned_vec())
}

#[inline(always)]
fn wr_translate_external_image_data(external: ExternalImageData) -> WrExternalImageData {
    WrExternalImageData {
        id: wr_translate_external_image_id(external.id),
        channel_index: external.channel_index,
        image_type: wr_translate_external_image_type(external.image_type),
    }
}

#[inline(always)]
pub(crate) const fn wr_translate_external_image_id(external: ExternalImageId) -> WrExternalImageId {
    WrExternalImageId(external.inner)
}

#[inline(always)]
pub(crate) const fn translate_external_image_id_wr(external: WrExternalImageId) -> ExternalImageId {
    ExternalImageId { inner: external.0 }
}

#[inline(always)]
fn wr_translate_external_image_type(external: ExternalImageType) -> WrExternalImageType {
    match external {
        ExternalImageType::TextureHandle(tt) => {
            WrExternalImageType::TextureHandle(wr_translate_image_buffer_kind(tt))
        }
        ExternalImageType::Buffer => WrExternalImageType::Buffer,
    }
}

#[inline(always)]
fn wr_translate_image_buffer_kind(buffer_kind: ImageBufferKind) -> WrImageBufferKind {
    match buffer_kind {
        ImageBufferKind::Texture2D => WrImageBufferKind::Texture2D,
        ImageBufferKind::TextureRect => WrImageBufferKind::TextureRect,
        ImageBufferKind::TextureExternal => WrImageBufferKind::TextureExternal,
    }
}

#[inline(always)]
fn wr_translate_update_image(update_image: UpdateImage) -> Option<WrUpdateImage> {
    Some(WrUpdateImage {
        key: wr_translate_image_key(update_image.key),
        descriptor: wr_translate_image_descriptor(update_image.descriptor)?,
        data: wr_translate_image_data(update_image.data),
        dirty_rect: wr_translate_image_dirty_rect(update_image.dirty_rect),
    })
}

#[inline(always)]
fn wr_translate_image_dirty_rect(dirty_rect: ImageDirtyRect) -> WrImageDirtyRect {
    use webrender::api::{
        units::{
            DeviceIntPoint as WrDeviceIntPoint, DeviceIntRect as WrDeviceIntRect,
            DeviceIntSize as WrDeviceIntSize,
        },
        DirtyRect as WrDirtyRect,
    };
    match dirty_rect {
        ImageDirtyRect::All => WrDirtyRect::All,
        ImageDirtyRect::Partial(rect) => {
            WrDirtyRect::Partial(WrDeviceIntRect::from_origin_and_size(
                WrDeviceIntPoint::new(rect.origin.x as i32, rect.origin.y as i32),
                WrDeviceIntSize::new(rect.size.width as i32, rect.size.height as i32),
            ))
        }
    }
}

#[inline]
pub(crate) const fn wr_translate_layout_transform(t: &ComputedTransform3D) -> WrLayoutTransform {
    WrLayoutTransform::new(
        t.m[0][0], t.m[0][1], t.m[0][2], t.m[0][3], t.m[1][0], t.m[1][1], t.m[1][2], t.m[1][3],
        t.m[2][0], t.m[2][1], t.m[2][2], t.m[2][3], t.m[3][0], t.m[3][1], t.m[3][2], t.m[3][3],
    )
}

#[inline(always)]
pub(crate) fn wr_translate_external_scroll_id(scroll_id: ExternalScrollId) -> WrExternalScrollId {
    WrExternalScrollId(scroll_id.0, wr_translate_pipeline_id(scroll_id.1))
}

pub(crate) fn wr_translate_display_list(
    document_id: DocumentId,
    render_api: &mut WrRenderApi,
    input: CachedDisplayList,
    pipeline_id: PipelineId,
    current_hidpi_factor: f32,
) -> WrBuiltDisplayList {
    let root_space_and_clip =
        WrSpaceAndClipInfo::root_scroll(wr_translate_pipeline_id(pipeline_id));
    let mut positioned_items = Vec::new();
    let mut builder = WrDisplayListBuilder::new(wr_translate_pipeline_id(pipeline_id));
    push_display_list_msg(
        document_id,
        render_api,
        &mut builder,
        input.root,
        root_space_and_clip.spatial_id,
        root_space_and_clip.clip_id,
        &mut positioned_items,
        current_hidpi_factor,
    );
    let (_pipeline_id, built_display_list) = builder.finalize();
    built_display_list
}

#[inline]
fn push_display_list_msg(
    document_id: DocumentId,
    render_api: &mut WrRenderApi,
    builder: &mut WrDisplayListBuilder,
    msg: DisplayListMsg,
    parent_spatial_id: WrSpatialId,
    parent_clip_id: WrClipId,
    positioned_items: &mut Vec<(WrSpatialId, WrClipId)>,
    current_hidpi_factor: f32,
) {
    use azul_core::{display_list::DisplayListMsg::*, ui_solver::PositionInfo::*};
    use webrender::api::{FillRule as WrFillRule, PropertyBindingKey as WrPropertyBindingKey};

    let msg_position = msg.get_position();

    let relative_x;
    let relative_y;

    let (parent_spatial_id, parent_clip_id) = match msg_position {
        Static(p) | Relative(p) => {
            relative_x = p.x_offset;
            relative_y = p.y_offset;
            (parent_spatial_id, parent_clip_id)
        }
        Absolute(p) => {
            let (last_positioned_spatial_id, last_positioned_clip_id) =
                positioned_items.last().copied().unwrap_or((
                    WrSpatialId::root_scroll_node(builder.pipeline_id),
                    WrClipId::root(builder.pipeline_id),
                ));
            relative_x = p.x_offset;
            relative_y = p.y_offset;
            (last_positioned_spatial_id, last_positioned_clip_id)
        }
        Fixed(p) => {
            relative_x = p.x_offset;
            relative_y = p.y_offset;
            (
                WrSpatialId::root_scroll_node(builder.pipeline_id),
                WrClipId::root(builder.pipeline_id),
            )
        }
    };

    // All rectangles are transformed in relation to the parent node,
    // so we have to push the parent as a "reference frame", optionally
    // adding an (animatable) transformation on top
    let transform = msg.get_transform_key();
    let opacity = msg.get_opacity_key();
    let mix_blend_mode = msg.get_mix_blend_mode();
    let has_mix_blend_mode_children = msg.has_mix_blend_mode_children();
    let should_push_stacking_context = transform.is_some()
        || opacity.is_some()
        || mix_blend_mode.is_some()
        || has_mix_blend_mode_children;

    let property_binding = match transform {
        Some(s) => WrPropertyBinding::Binding(
            WrPropertyBindingKey::new(s.0.id as u64),
            wr_translate_layout_transform(&s.1),
        ),
        None => WrPropertyBinding::Value(WrLayoutTransform::identity()),
    };

    let rect_spatial_id = builder.push_reference_frame(
        WrLayoutPoint::new(relative_x, relative_y),
        parent_spatial_id,
        WrTransformStyle::Flat,
        property_binding,
        WrReferenceFrameKind::Transform {
            is_2d_scale_translation: false,
            should_snap: false,
        },
    );

    if should_push_stacking_context {
        use webrender::api::{
            FilterOp as WrFilterOp, RasterSpace as WrRasterSpace,
            StackingContextFlags as WrStackingContextFlags,
        };

        let opacity_filters = match opacity {
            None => Vec::new(),
            Some(s) => vec![WrFilterOp::Opacity(
                WrPropertyBinding::Binding(WrPropertyBindingKey::new(s.0.id as u64), s.1),
                s.1,
            )],
        };

        // let filters = ...
        // let backdrop_filters = ...

        let mut stacking_context_flags = WrStackingContextFlags::empty();
        if has_mix_blend_mode_children {
            stacking_context_flags.set(WrStackingContextFlags::IS_BLEND_CONTAINER, true);
        }

        builder.push_stacking_context(
            WrLayoutPoint::zero(),
            rect_spatial_id,
            WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
            None,
            WrTransformStyle::Flat,
            wr_translate_mix_blend_mode(mix_blend_mode.copied().unwrap_or_default()),
            &opacity_filters,
            &[],
            &[],
            WrRasterSpace::Screen,
            stacking_context_flags,
        );
    }

    if msg_position.is_positioned() {
        positioned_items.push((rect_spatial_id, parent_clip_id));
    }

    // push the clip image mask before pushing the scroll frame
    let clip_mask_id = msg.get_image_mask().map(|im| {
        builder.define_clip_image_mask(
            &WrSpaceAndClipInfo {
                spatial_id: rect_spatial_id,
                clip_id: parent_clip_id,
            },
            wr_translate_image_mask(im),
            &Vec::new(),
            WrFillRule::Nonzero,
        )
    });

    let parent_clip_id = match clip_mask_id {
        None => parent_clip_id,
        Some(s) => s,
    };

    match msg {
        IFrame(iframe_pipeline_id, iframe_clip_size, epoch, cached_display_list) => {
            let iframe_root_size = cached_display_list.root_size;

            let built_display_list = wr_translate_display_list(
                document_id,
                render_api,
                *cached_display_list,
                iframe_pipeline_id,
                current_hidpi_factor,
            );

            let wr_pipeline_id = wr_translate_pipeline_id(iframe_pipeline_id);
            let mut transaction = WrTransaction::new();
            transaction.set_display_list(
                wr_translate_epoch(epoch),
                None,                                        // background
                wr_translate_logical_size(iframe_clip_size), // viewport size
                (wr_pipeline_id, built_display_list),
                true, // preserve frame scroll state
            );
            render_api.send_transaction(wr_translate_document_id(document_id), transaction);

            builder.push_iframe(
                WrLayoutRect::from_size(wr_translate_logical_size(iframe_root_size)), // bounds
                WrLayoutRect::from_size(wr_translate_logical_size(iframe_clip_size)), // clip_bounds
                &WrSpaceAndClipInfo {
                    clip_id: parent_clip_id,
                    spatial_id: rect_spatial_id,
                },
                wr_translate_pipeline_id(iframe_pipeline_id),
                false, // the iframe is already submitted into the render API
            );
        }
        Frame(f) => push_frame(
            document_id,
            render_api,
            builder,
            f,
            rect_spatial_id,
            parent_clip_id,
            positioned_items,
            current_hidpi_factor,
        ),
        ScrollFrame(sf) => push_scroll_frame(
            document_id,
            render_api,
            builder,
            sf,
            rect_spatial_id,
            parent_clip_id,
            positioned_items,
            current_hidpi_factor,
        ),
    }

    if msg_position.is_positioned() {
        positioned_items.pop();
    }

    if should_push_stacking_context {
        builder.pop_stacking_context();
    }

    builder.pop_reference_frame();
}

#[inline]
fn push_frame(
    document_id: DocumentId,
    render_api: &mut WrRenderApi,
    builder: &mut WrDisplayListBuilder,
    frame: DisplayListFrame,
    rect_spatial_id: WrSpatialId,
    parent_clip_id: WrClipId,
    positioned_items: &mut Vec<(WrSpatialId, WrClipId)>,
    current_hidpi_factor: f32,
) {
    let content_clip_id = push_display_list_content(
        builder,
        &frame.box_shadow,
        &frame.content,
        frame.size,
        frame.border_radius,
        frame.flags,
        rect_spatial_id,
        current_hidpi_factor,
        Some(parent_clip_id),
    );

    let wr_border_radius = wr_translate_border_radius(frame.border_radius, frame.size);

    // If the rect has an overflow:* property set, clip the children accordingly
    let children_clip_id = match frame.clip_children {
        Some(size) => content_clip_id,
        None => parent_clip_id, // no clipping
    };

    // push the hit-testing tag if any
    if let Some(hit_tag) = frame.tag {
        builder.push_hit_test(
            &WrCommonItemProperties {
                clip_rect: WrLayoutRect::from_size(WrLayoutSize::new(
                    frame.size.width,
                    frame.size.height,
                )),
                spatial_id: rect_spatial_id,
                clip_id: parent_clip_id,
                flags: WrPrimitiveFlags::empty(),
            },
            (hit_tag.0, 0),
        );
    }

    // if let Some(image_mask) -> define_image_mask_clip()
    for child in frame.children {
        push_display_list_msg(
            document_id,
            render_api,
            builder,
            child,
            rect_spatial_id,
            children_clip_id,
            positioned_items,
            current_hidpi_factor,
        );
    }
}

#[inline]
fn push_scroll_frame(
    document_id: DocumentId,
    render_api: &mut WrRenderApi,
    builder: &mut WrDisplayListBuilder,
    scroll_frame: DisplayListScrollFrame,
    rect_spatial_id: WrSpatialId,
    parent_clip_id: WrClipId,
    positioned_items: &mut Vec<(WrSpatialId, WrClipId)>,
    current_hidpi_factor: f32,
) {
    use azul_css::ColorU;
    use webrender::api::{
        ClipMode as WrClipMode, ComplexClipRegion as WrComplexClipRegion,
        ScrollSensitivity as WrScrollSensitivity,
    };

    // if let Some(image_mask) = scroll_frame.frame.image_mask { push_image_mask_clip() }

    // Only children should scroll, not the frame itself!
    let content_clip_id = push_display_list_content(
        builder,
        &scroll_frame.frame.box_shadow,
        &scroll_frame.frame.content,
        scroll_frame.frame.size,
        scroll_frame.frame.border_radius,
        scroll_frame.frame.flags,
        rect_spatial_id,
        current_hidpi_factor,
        Some(parent_clip_id),
    );

    // Push hit-testing + scrolling children

    // scroll frame has the hit-testing clip as a parent
    let scroll_frame_clip_info = builder.define_scroll_frame(
        /* parent_space_and_clip */
        &WrSpaceAndClipInfo {
            clip_id: content_clip_id,
            spatial_id: rect_spatial_id,
        },
        /* external_id */ wr_translate_external_scroll_id(scroll_frame.scroll_id),
        /* content_rect */
        WrLayoutRect::from_size(wr_translate_logical_size(scroll_frame.content_rect.size)),
        /* clip_rect */
        WrLayoutRect::from_size(wr_translate_logical_size(scroll_frame.parent_rect.size)),
        /* sensitivity */ WrScrollSensitivity::Script,
        /* external_scroll_offset */
        WrLayoutVector2D::new(
            scroll_frame.content_rect.origin.x - scroll_frame.parent_rect.origin.x,
            scroll_frame.content_rect.origin.y - scroll_frame.parent_rect.origin.y,
        ),
    );

    // push the scroll hit-testing tag if any
    builder.push_hit_test(
        &WrCommonItemProperties {
            clip_rect: WrLayoutRect::from_size(WrLayoutSize::new(
                scroll_frame.content_rect.size.width,
                scroll_frame.content_rect.size.height,
            )),
            spatial_id: scroll_frame_clip_info.spatial_id,
            clip_id: scroll_frame_clip_info.clip_id,
            flags: WrPrimitiveFlags::empty(),
        },
        (scroll_frame.scroll_tag.0 .0, 0),
    );

    // additionally push the hit tag of the frame if there is any
    if let Some(hit_tag) = scroll_frame.frame.tag {
        builder.push_hit_test(
            &WrCommonItemProperties {
                clip_rect: WrLayoutRect::from_size(WrLayoutSize::new(
                    scroll_frame.frame.size.width,
                    scroll_frame.frame.size.height,
                )),
                spatial_id: scroll_frame_clip_info.spatial_id,
                clip_id: scroll_frame_clip_info.clip_id,
                flags: WrPrimitiveFlags::empty(),
            },
            (hit_tag.0, 0),
        );
    }

    for child in scroll_frame.frame.children {
        push_display_list_msg(
            document_id,
            render_api,
            builder,
            child,
            scroll_frame_clip_info.spatial_id,
            scroll_frame_clip_info.clip_id,
            positioned_items,
            current_hidpi_factor,
        );
    }
}

#[inline]
fn define_border_radius_clip(
    builder: &mut WrDisplayListBuilder,
    layout_rect: LogicalRect,
    wr_border_radius: WrBorderRadius,
    rect_spatial_id: WrSpatialId,
    parent_clip_id: WrClipId,
) -> WrClipId {
    use webrender::api::{ClipMode as WrClipMode, ComplexClipRegion as WrComplexClipRegion};

    // NOTE: only translate the size, position is always (0.0, 0.0)
    let wr_layout_size = wr_translate_logical_size(layout_rect.size);
    let wr_layout_rect = WrLayoutRect::from_size(wr_layout_size);

    let clip = if wr_border_radius.is_zero() {
        builder.define_clip_rect(
            // TODO: optimize - if border radius = 0,
            &WrSpaceAndClipInfo {
                spatial_id: rect_spatial_id,
                clip_id: parent_clip_id,
            },
            wr_layout_rect,
        )
    } else {
        builder.define_clip_rounded_rect(
            // TODO: optimize - if border radius = 0,
            &WrSpaceAndClipInfo {
                spatial_id: rect_spatial_id,
                clip_id: parent_clip_id,
            },
            WrComplexClipRegion::new(wr_layout_rect, wr_border_radius, WrClipMode::Clip),
        )
    };

    clip
}

// returns the clip of the content (i.e. the current rect)
#[inline]
fn push_display_list_content(
    builder: &mut WrDisplayListBuilder,
    box_shadow: &Option<BoxShadow>,
    content: &[LayoutRectContent],
    rect_size: LogicalSize,
    border_radius: StyleBorderRadius,
    flags: PrimitiveFlags,
    rect_spatial_id: WrSpatialId,
    current_hidpi_factor: f32,
    // clip of the parent item (if any) or None to use the root clip
    // if frame.clip_children is set, this should be Some(clip_id)
    parent_clip: Option<WrClipId>,
) -> WrClipId {
    use azul_core::display_list::LayoutRectContent::*;

    let clip_rect = LogicalRect::new(LogicalPosition::zero(), rect_size);
    let parent_clip_id = parent_clip.unwrap_or_else(|| WrClipId::root(builder.pipeline_id));
    let normal_info = WrCommonItemProperties {
        clip_rect: wr_translate_logical_rect(clip_rect),
        clip_id: parent_clip_id, // default: no clipping
        spatial_id: rect_spatial_id,
        flags: wr_translate_primitive_flags(flags),
    };

    let wr_border_radius = wr_translate_border_radius(border_radius, clip_rect.size);

    if let Some(box_shadow) = box_shadow.as_ref() {
        // push outset box shadow before the item clip is pushed
        if box_shadow.clip_mode == CssBoxShadowClipMode::Outset {
            // If the content is a shadow, it needs to be clipped by the root
            box_shadow::push_box_shadow(
                builder,
                clip_rect,
                CssBoxShadowClipMode::Outset,
                box_shadow,
                border_radius,
                normal_info.spatial_id,
                parent_clip_id,
            );
        }
    }

    let mut content_clip: Option<WrClipId> = None;

    for content in content {
        // Border and BoxShadow::Outset get a root clip, since they
        // are outside of the rect contents
        // All other content types get the regular clip
        match content {
            Text {
                glyphs,
                font_instance_key,
                color,
                glyph_options,
                overflow,
                text_shadow,
            } => {
                let mut text_info = normal_info.clone();
                if overflow.0 || overflow.1 {
                    text_info.clip_id = content_clip
                        .get_or_insert_with(|| {
                            define_border_radius_clip(
                                builder,
                                clip_rect,
                                wr_border_radius,
                                normal_info.spatial_id,
                                parent_clip_id,
                            )
                        })
                        .clone();
                }

                // push text shadow: push glyphs + blur filter
                use azul_css::StyleBoxShadow;

                if let Some(StyleBoxShadow {
                    offset,
                    color,
                    blur_radius,
                    spread_radius,
                    clip_mode,
                }) = text_shadow.as_ref()
                {
                    use webrender::api::{
                        FilterOp as WrFilterOp, RasterSpace as WrRasterSpace, Shadow as WrShadow,
                        StackingContextFlags as WrStackingContextFlags,
                    };

                    builder.push_stacking_context(
                        WrLayoutPoint::zero(),
                        normal_info.spatial_id,
                        WrPrimitiveFlags::empty(),
                        None,
                        WrTransformStyle::Flat,
                        WrMixBlendMode::Normal,
                        &[WrFilterOp::DropShadow(WrShadow {
                            offset: WrLayoutVector2D::new(
                                offset[0].to_pixels(),
                                offset[1].to_pixels(),
                            ),
                            color: wr_translate_color_f(color.clone().into()),
                            blur_radius: blur_radius.to_pixels(),
                        })],
                        &[],
                        &[],
                        WrRasterSpace::Screen,
                        WrStackingContextFlags::empty(),
                    );
                }

                text::push_text(
                    builder,
                    &text_info,
                    glyphs,
                    *font_instance_key,
                    *color,
                    *glyph_options,
                );

                if text_shadow.is_some() {
                    builder.pop_stacking_context();
                }
            }
            Background {
                content,
                size,
                offset,
                repeat,
            } => {
                let mut background_info = normal_info.clone();
                background_info.clip_id = content_clip
                    .get_or_insert_with(|| {
                        define_border_radius_clip(
                            builder,
                            clip_rect,
                            wr_border_radius,
                            normal_info.spatial_id,
                            parent_clip_id,
                        )
                    })
                    .clone();
                background::push_background(
                    builder,
                    &background_info,
                    content,
                    *size,
                    *offset,
                    *repeat,
                );
            }
            Image {
                size,
                offset,
                image_rendering,
                alpha_type,
                image_key,
                background_color,
            } => {
                let mut image_info = normal_info.clone();
                image_info.clip_id = content_clip
                    .get_or_insert_with(|| {
                        define_border_radius_clip(
                            builder,
                            clip_rect,
                            wr_border_radius,
                            normal_info.spatial_id,
                            parent_clip_id,
                        )
                    })
                    .clone();
                image::push_image(
                    builder,
                    &image_info,
                    *size,
                    *offset,
                    *image_key,
                    *alpha_type,
                    *image_rendering,
                    *background_color,
                );
            }
            Border {
                widths,
                colors,
                styles,
            } => {
                // no clip necessary because item will always be in parent bounds
                border::push_border(
                    builder,
                    &normal_info,
                    border_radius,
                    *widths,
                    *colors,
                    *styles,
                    current_hidpi_factor,
                );
            }
        }
    }

    if let Some(box_shadow) = box_shadow.as_ref() {
        // push outset box shadow before the item clip is pushed
        if box_shadow.clip_mode == CssBoxShadowClipMode::Inset {
            let inset_clip_id = content_clip
                .get_or_insert_with(|| {
                    define_border_radius_clip(
                        builder,
                        clip_rect,
                        wr_border_radius,
                        normal_info.spatial_id,
                        parent_clip_id,
                    )
                })
                .clone();
            box_shadow::push_box_shadow(
                builder,
                clip_rect,
                CssBoxShadowClipMode::Inset,
                box_shadow,
                border_radius,
                normal_info.spatial_id,
                inset_clip_id,
            );
        }
    }

    return content_clip
        .get_or_insert_with(|| {
            define_border_radius_clip(
                builder,
                clip_rect,
                wr_border_radius,
                normal_info.spatial_id,
                parent_clip_id,
            )
        })
        .clone();
}

mod text {

    use azul_core::{
        app_resources::{FontInstanceKey, GlyphOptions},
        display_list::GlyphInstance,
        window::LogicalSize,
    };
    use azul_css::ColorU;
    use webrender::api::{
        CommonItemProperties as WrCommonItemProperties, DisplayListBuilder as WrDisplayListBuilder,
    };

    pub(super) fn push_text(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        glyphs: &[GlyphInstance],
        font_instance_key: FontInstanceKey,
        color: ColorU,
        glyph_options: Option<GlyphOptions>,
    ) {
        use super::{
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
}

mod background {

    use azul_core::{
        app_resources::ImageKey,
        display_list::RectBackground,
        window::{LogicalPosition, LogicalSize},
    };
    use azul_css::{
        ColorU, ConicGradient, LayoutPoint, LayoutSize, LinearGradient, RadialGradient,
        StyleBackgroundPosition, StyleBackgroundRepeat, StyleBackgroundSize,
    };
    use webrender::api::{
        units::{
            LayoutPoint as WrLayoutPoint, LayoutRect as WrLayoutRect, LayoutSize as WrLayoutSize,
        },
        CommonItemProperties as WrCommonItemProperties, DisplayListBuilder as WrDisplayListBuilder,
        GradientStop as WrGradientStop,
    };

    use super::image;

    struct Ratio {
        width: f32,
        height: f32,
    }

    #[inline]
    pub(super) fn push_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        background: &RectBackground,
        background_size: Option<StyleBackgroundSize>,
        background_position: Option<StyleBackgroundPosition>,
        background_repeat: Option<StyleBackgroundRepeat>,
    ) {
        use azul_core::display_list::RectBackground::*;

        let content_size = background.get_content_size();

        match background {
            LinearGradient(g) => push_linear_gradient_background(
                builder,
                &info,
                g.clone(),
                background_position,
                background_size,
                content_size,
            ),
            RadialGradient(rg) => push_radial_gradient_background(
                builder,
                &info,
                rg.clone(),
                background_position,
                background_size,
                content_size,
            ),
            ConicGradient(cg) => push_conic_gradient_background(
                builder,
                &info,
                cg.clone(),
                background_position,
                background_size,
                content_size,
            ),
            Image((key, _)) => push_image_background(
                builder,
                &info,
                *key,
                background_position,
                background_size,
                background_repeat,
                content_size,
            ),
            Color(col) => push_color_background(
                builder,
                &info,
                *col,
                background_position,
                background_size,
                background_repeat,
                content_size,
            ),
        }
    }

    fn push_conic_gradient_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        conic_gradient: ConicGradient,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        content_size: Option<(f32, f32)>,
    ) {
        use webrender::api::units::LayoutPoint as WrLayoutPoint;

        use super::{wr_translate_color_u, wr_translate_extend_mode, wr_translate_logical_size};

        let clip_rect_size = info.clip_rect.size();
        let width = clip_rect_size.width.round();
        let height = clip_rect_size.height.round();
        let background_position = background_position.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let offset =
            calculate_background_position(width, height, background_position, background_size);

        let mut offset_info = *info;
        offset_info.clip_rect.min.x += offset.x;
        offset_info.clip_rect.min.y += offset.y;

        let stops: Vec<WrGradientStop> = conic_gradient
            .stops
            .iter()
            .map(|gradient_pre| WrGradientStop {
                offset: gradient_pre.angle.to_degrees() / 360.0,
                color: wr_translate_color_u(gradient_pre.color).into(),
            })
            .collect();

        if stops.len() < 2 {
            return;
        }

        let center =
            calculate_background_position(width, height, conic_gradient.center, background_size);
        let center = WrLayoutPoint::new(center.x, center.y);

        let gradient = builder.create_conic_gradient(
            center,
            conic_gradient.angle.to_degrees() / 360.0,
            stops,
            wr_translate_extend_mode(conic_gradient.extend_mode),
        );

        builder.push_conic_gradient(
            &offset_info,
            offset_info.clip_rect,
            gradient,
            wr_translate_logical_size(background_size),
            WrLayoutSize::zero(),
        );
    }

    fn push_radial_gradient_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        radial_gradient: RadialGradient,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        content_size: Option<(f32, f32)>,
    ) {
        use azul_css::Shape;
        use webrender::api::units::LayoutPoint as WrLayoutPoint;

        use super::{wr_translate_color_u, wr_translate_extend_mode, wr_translate_logical_size};

        let clip_rect_size = info.clip_rect.size();
        let width = clip_rect_size.width.round();
        let height = clip_rect_size.height.round();
        let background_position = background_position.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let offset =
            calculate_background_position(width, height, background_position, background_size);

        let mut offset_info = *info;
        offset_info.clip_rect.min.x += offset.x;
        offset_info.clip_rect.min.y += offset.y;

        let center =
            calculate_background_position(width, height, radial_gradient.position, background_size);
        let center = WrLayoutPoint::new(center.x, center.y);

        let stops: Vec<WrGradientStop> = radial_gradient
            .stops
            .iter()
            .map(|gradient_pre| WrGradientStop {
                offset: gradient_pre.offset.normalized(),
                color: wr_translate_color_u(gradient_pre.color).into(),
            })
            .collect();

        if stops.len() < 2 {
            return;
        }

        // Note: division by 2.0 because it's the radius, not the diameter
        let radius = match radial_gradient.shape {
            Shape::Ellipse => WrLayoutSize::new(background_size.width, background_size.height),
            Shape::Circle => {
                let largest_bound_size = background_size.width.max(background_size.height);
                WrLayoutSize::new(largest_bound_size, largest_bound_size)
            }
        };

        let gradient = builder.create_radial_gradient(
            center,
            radius,
            stops,
            wr_translate_extend_mode(radial_gradient.extend_mode),
        );

        builder.push_radial_gradient(
            &offset_info,
            offset_info.clip_rect,
            gradient,
            wr_translate_logical_size(background_size),
            WrLayoutSize::zero(),
        );
    }

    fn push_linear_gradient_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        linear_gradient: LinearGradient,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        content_size: Option<(f32, f32)>,
    ) {
        use super::{
            wr_translate_color_u, wr_translate_css_layout_rect, wr_translate_extend_mode,
            wr_translate_layout_point, wr_translate_logical_size,
        };

        let background_position = background_position.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let clip_rect_size = info.clip_rect.size();
        let offset = calculate_background_position(
            clip_rect_size.width.round(),
            clip_rect_size.height.round(),
            background_position,
            background_size,
        );

        let mut offset_info = *info;
        offset_info.clip_rect.min.x += offset.x;
        offset_info.clip_rect.min.y += offset.y;

        let stops: Vec<WrGradientStop> = linear_gradient
            .stops
            .iter()
            .map(|gradient_pre| WrGradientStop {
                offset: gradient_pre.offset.get() / 100.0,
                color: wr_translate_color_u(gradient_pre.color).into(),
            })
            .collect();

        if stops.len() < 2 {
            return;
        }

        let (begin_pt, end_pt) = linear_gradient
            .direction
            .to_points(&wr_translate_css_layout_rect(offset_info.clip_rect));
        let gradient = builder.create_gradient(
            wr_translate_layout_point(begin_pt),
            wr_translate_layout_point(end_pt),
            stops,
            wr_translate_extend_mode(linear_gradient.extend_mode),
        );

        builder.push_gradient(
            &offset_info,
            offset_info.clip_rect,
            gradient,
            wr_translate_logical_size(background_size),
            WrLayoutSize::zero(),
        );
    }

    fn push_image_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        image_key: ImageKey,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
    ) {
        use azul_core::display_list::{AlphaType, ImageRendering};

        let background_position = background_position.unwrap_or_default();
        let background_repeat = background_repeat.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let clip_rect_size = info.clip_rect.size();
        let background_position = calculate_background_position(
            clip_rect_size.width.round(),
            clip_rect_size.height.round(),
            background_position,
            background_size,
        );
        let background_repeat_info =
            get_background_repeat_info(info, background_repeat, background_size);

        // TODO: customize this for image backgrounds?
        let alpha_type = AlphaType::PremultipliedAlpha;
        let image_rendering = ImageRendering::Auto;
        let background_color = ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };

        image::push_image(
            builder,
            &background_repeat_info,
            background_size,
            background_position,
            image_key,
            alpha_type,
            image_rendering,
            background_color,
        );
    }

    fn push_color_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        color: ColorU,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
    ) {
        use super::wr_translate_color_u;

        let background_position = background_position.unwrap_or_default();
        let _background_repeat = background_repeat.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let clip_rect_size = info.clip_rect.size();
        let offset = calculate_background_position(
            clip_rect_size.width.round(),
            clip_rect_size.height.round(),
            background_position,
            background_size,
        );

        let mut offset_info = *info;
        offset_info.clip_rect.min.x += offset.x;
        offset_info.clip_rect.min.y += offset.y;
        offset_info.clip_rect.max.x = offset_info.clip_rect.min.x + background_size.width;
        offset_info.clip_rect.max.y = offset_info.clip_rect.min.y + background_size.height;

        builder.push_rect(
            &offset_info,
            offset_info.clip_rect,
            wr_translate_color_u(color).into(),
        );
    }

    fn get_background_repeat_info(
        info: &WrCommonItemProperties,
        background_repeat: StyleBackgroundRepeat,
        background_size: LogicalSize,
    ) -> WrCommonItemProperties {
        use azul_css::StyleBackgroundRepeat::*;

        match background_repeat {
            NoRepeat => WrCommonItemProperties {
                clip_rect: WrLayoutRect::from_origin_and_size(
                    WrLayoutPoint::new(info.clip_rect.min.x, info.clip_rect.min.y),
                    WrLayoutSize::new(background_size.width, background_size.height),
                ),
                ..*info
            },
            Repeat => *info,
            RepeatX => WrCommonItemProperties {
                clip_rect: WrLayoutRect::from_origin_and_size(
                    WrLayoutPoint::new(info.clip_rect.min.x, info.clip_rect.min.y),
                    WrLayoutSize::new(info.clip_rect.size().width, background_size.height),
                ),
                ..*info
            },
            RepeatY => WrCommonItemProperties {
                clip_rect: WrLayoutRect::from_origin_and_size(
                    WrLayoutPoint::new(info.clip_rect.min.x, info.clip_rect.min.y),
                    WrLayoutSize::new(background_size.width, info.clip_rect.size().height),
                ),
                ..*info
            },
        }
    }

    /// Transform a background size such as "cover" or "contain" into actual pixels
    fn calculate_background_size(
        info: &WrCommonItemProperties,
        bg_size: Option<StyleBackgroundSize>,
        content_size: Option<(f32, f32)>,
    ) -> LogicalSize {
        let default_content_size = info.clip_rect.size();
        let content_size =
            content_size.unwrap_or((default_content_size.width, default_content_size.height));

        let bg_size = match bg_size {
            None => return LogicalSize::new(content_size.0, content_size.1),
            Some(s) => s,
        };

        let clip_rect_size = info.clip_rect.size();
        let content_aspect_ratio = Ratio {
            width: clip_rect_size.width / content_size.0,
            height: clip_rect_size.height / content_size.1,
        };

        let ratio = match bg_size {
            StyleBackgroundSize::ExactSize([w, h]) => {
                let w = w.to_pixels(clip_rect_size.width);
                let h = h.to_pixels(clip_rect_size.height);
                w.min(h)
            }
            StyleBackgroundSize::Contain => {
                content_aspect_ratio.width.min(content_aspect_ratio.height)
            }
            StyleBackgroundSize::Cover => {
                content_aspect_ratio.width.max(content_aspect_ratio.height)
            }
        };

        LogicalSize::new(content_size.0 * ratio, content_size.1 * ratio)
    }

    /// Transforma background-position attribute into pixel coordinates
    fn calculate_background_position(
        width: f32,
        height: f32,
        background_position: StyleBackgroundPosition,
        background_size: LogicalSize,
    ) -> LogicalPosition {
        use azul_css::{BackgroundPositionHorizontal, BackgroundPositionVertical};

        let horizontal_offset = match background_position.horizontal {
            BackgroundPositionHorizontal::Right => 0.0,
            BackgroundPositionHorizontal::Center => (width - background_size.width) / 2.0,
            BackgroundPositionHorizontal::Left => (width - background_size.width),
            BackgroundPositionHorizontal::Exact(e) => e.to_pixels(width),
        };

        let vertical_offset = match background_position.vertical {
            BackgroundPositionVertical::Top => 0.0,
            BackgroundPositionVertical::Center => (height - background_size.height) / 2.0,
            BackgroundPositionVertical::Bottom => (height - background_size.height),
            BackgroundPositionVertical::Exact(e) => e.to_pixels(height),
        };

        LogicalPosition {
            x: horizontal_offset,
            y: vertical_offset,
        }
    }
}

mod image {

    use azul_core::{
        app_resources::ImageKey,
        display_list::{AlphaType, ImageRendering},
        window::{LogicalPosition, LogicalSize},
    };
    use azul_css::{ColorU, LayoutPoint, LayoutSize};
    use webrender::api::{
        CommonItemProperties as WrCommonItemProperties, DisplayListBuilder as WrDisplayListBuilder,
    };

    #[inline]
    pub(super) fn push_image(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        size: LogicalSize,
        offset: LogicalPosition,
        image_key: ImageKey,
        alpha_type: AlphaType,
        image_rendering: ImageRendering,
        background_color: ColorU,
    ) {
        use webrender::api::units::LayoutSize as WrLayoutSize;

        use super::{
            wr_translate_alpha_type, wr_translate_color_u, wr_translate_image_key,
            wr_translate_image_rendering, wr_translate_logical_size,
        };

        let mut offset_info = *info;
        offset_info.clip_rect.min.x += offset.x;
        offset_info.clip_rect.min.y += offset.y;

        let tile_spacing = WrLayoutSize::zero();

        builder.push_repeating_image(
            &offset_info,
            offset_info.clip_rect,
            wr_translate_logical_size(size),
            tile_spacing,
            wr_translate_image_rendering(image_rendering),
            wr_translate_alpha_type(alpha_type),
            wr_translate_image_key(image_key),
            wr_translate_color_u(background_color).into(),
        );
    }
}

mod box_shadow {

    use azul_core::{
        display_list::{BoxShadow, StyleBorderRadius},
        window::{LogicalRect, LogicalSize},
    };
    use azul_css::{BoxShadowClipMode, ColorF, LayoutRect, StyleBoxShadow};
    use webrender::api::{
        ClipId as WrClipId, CommonItemProperties as WrCommonItemProperties,
        DisplayListBuilder as WrDisplayListBuilder, SpatialId as WrSpatialId,
    };

    enum ShouldPushShadow {
        OneShadow,
        TwoShadows,
        AllShadows,
    }

    /// WARNING: For "inset" shadows, you must push a clip ID first, otherwise the
    /// shadow will not show up.
    ///
    /// To prevent a shadow from being pushed twice, you have to annotate the clip
    /// mode for this - outset or inset.
    #[inline]
    pub(super) fn push_box_shadow(
        builder: &mut WrDisplayListBuilder,
        bounds: LogicalRect,
        shadow_type: BoxShadowClipMode,
        box_shadow: &BoxShadow,
        border_radius: StyleBorderRadius,
        parent_spatial_id: WrSpatialId,
        parent_clip_id: WrClipId,
    ) {
        use azul_css::CssPropertyValue;

        use self::ShouldPushShadow::*;

        let BoxShadow {
            clip_mode,
            top,
            left,
            bottom,
            right,
        } = box_shadow;

        fn translate_shadow_side(
            input: &Option<CssPropertyValue<StyleBoxShadow>>,
        ) -> Option<StyleBoxShadow> {
            input.and_then(|prop| prop.get_property().cloned())
        }

        let (top, left, bottom, right) = (
            translate_shadow_side(top),
            translate_shadow_side(left),
            translate_shadow_side(bottom),
            translate_shadow_side(right),
        );

        let what_shadow_to_push = match [top, left, bottom, right]
            .iter()
            .filter(|x| x.is_some())
            .count()
        {
            1 => OneShadow,
            2 => TwoShadows,
            4 => AllShadows,
            _ => return,
        };

        match what_shadow_to_push {
            OneShadow => {
                let current_shadow = match (top, left, bottom, right) {
                    (Some(shadow), None, None, None)
                    | (None, Some(shadow), None, None)
                    | (None, None, Some(shadow), None)
                    | (None, None, None, Some(shadow)) => shadow,
                    _ => return, // reachable, but invalid box-shadow
                };

                push_single_box_shadow_edge(
                    builder,
                    &current_shadow,
                    bounds,
                    border_radius,
                    shadow_type,
                    &top,
                    &bottom,
                    &left,
                    &right,
                    parent_spatial_id,
                    parent_clip_id,
                );
            }
            // Two shadows in opposite directions:
            //
            // box-shadow-top: 0px 0px 5px red;
            // box-shadow-bottom: 0px 0px 5px blue;
            TwoShadows => {
                match (top, left, bottom, right) {
                    // top + bottom box-shadow pair
                    (Some(t), None, Some(b), None) => {
                        push_single_box_shadow_edge(
                            builder,
                            &t,
                            bounds,
                            border_radius,
                            shadow_type,
                            &top,
                            &None,
                            &None,
                            &None,
                            parent_spatial_id,
                            parent_clip_id,
                        );
                        push_single_box_shadow_edge(
                            builder,
                            &b,
                            bounds,
                            border_radius,
                            shadow_type,
                            &None,
                            &bottom,
                            &None,
                            &None,
                            parent_spatial_id,
                            parent_clip_id,
                        );
                    }
                    // left + right box-shadow pair
                    (None, Some(l), None, Some(r)) => {
                        push_single_box_shadow_edge(
                            builder,
                            &l,
                            bounds,
                            border_radius,
                            shadow_type,
                            &None,
                            &None,
                            &left,
                            &None,
                            parent_spatial_id,
                            parent_clip_id,
                        );
                        push_single_box_shadow_edge(
                            builder,
                            &r,
                            bounds,
                            border_radius,
                            shadow_type,
                            &None,
                            &None,
                            &None,
                            &right,
                            parent_spatial_id,
                            parent_clip_id,
                        );
                    }
                    _ => return, // reachable, but invalid
                }
            }
            AllShadows => {
                // Assumes that all box shadows are the same, so just use the top shadow
                let top_shadow = top.unwrap();
                let clip_rect = get_clip_rect(&top_shadow, bounds);

                push_box_shadow_inner(
                    builder,
                    top_shadow,
                    border_radius,
                    bounds,
                    clip_rect,
                    shadow_type,
                    parent_spatial_id,
                    parent_clip_id,
                );
            }
        }
    }

    #[inline]
    #[allow(clippy::collapsible_if)]
    fn push_single_box_shadow_edge(
        builder: &mut WrDisplayListBuilder,
        current_shadow: &StyleBoxShadow,
        bounds: LogicalRect,
        border_radius: StyleBorderRadius,
        shadow_type: BoxShadowClipMode,
        top: &Option<StyleBoxShadow>,
        bottom: &Option<StyleBoxShadow>,
        left: &Option<StyleBoxShadow>,
        right: &Option<StyleBoxShadow>,
        parent_spatial_id: WrSpatialId,
        parent_clip_id: WrClipId,
    ) {
        let is_inset_shadow = current_shadow.clip_mode == BoxShadowClipMode::Inset;
        let origin_displace = (current_shadow.spread_radius.to_pixels()
            + current_shadow.blur_radius.to_pixels())
            * 2.0;

        let mut shadow_bounds = bounds;
        let mut clip_rect = bounds;

        if is_inset_shadow {
            // If the shadow is inset, we adjust the clip rect to be
            // exactly the amount of the shadow
            if let Some(_top) = top {
                clip_rect.size.height = origin_displace;
                shadow_bounds.size.width += origin_displace;
                shadow_bounds.origin.x -= origin_displace / 2.0;
            } else if let Some(_bottom) = bottom {
                clip_rect.size.height = origin_displace;
                clip_rect.origin.y += bounds.size.height - origin_displace;
                shadow_bounds.size.width += origin_displace;
                shadow_bounds.origin.x -= origin_displace / 2.0;
            } else if let Some(_left) = left {
                clip_rect.size.width = origin_displace;
                shadow_bounds.size.height += origin_displace;
                shadow_bounds.origin.y -= origin_displace / 2.0;
            } else if let Some(_right) = right {
                clip_rect.size.width = origin_displace;
                clip_rect.origin.x += bounds.size.width - origin_displace;
                shadow_bounds.size.height += origin_displace;
                shadow_bounds.origin.y -= origin_displace / 2.0;
            }
        } else {
            if let Some(_top) = top {
                clip_rect.size.height = origin_displace;
                clip_rect.origin.y -= origin_displace;
                shadow_bounds.size.width += origin_displace;
                shadow_bounds.origin.x -= origin_displace / 2.0;
            } else if let Some(_bottom) = bottom {
                clip_rect.size.height = origin_displace;
                clip_rect.origin.y += bounds.size.height;
                shadow_bounds.size.width += origin_displace;
                shadow_bounds.origin.x -= origin_displace / 2.0;
            } else if let Some(_left) = left {
                clip_rect.size.width = origin_displace;
                clip_rect.origin.x -= origin_displace;
                shadow_bounds.size.height += origin_displace;
                shadow_bounds.origin.y -= origin_displace / 2.0;
            } else if let Some(_right) = right {
                clip_rect.size.width = origin_displace;
                clip_rect.origin.x += bounds.size.width;
                shadow_bounds.size.height += origin_displace;
                shadow_bounds.origin.y -= origin_displace / 2.0;
            }
        }

        push_box_shadow_inner(
            builder,
            *current_shadow,
            border_radius,
            shadow_bounds,
            clip_rect,
            shadow_type,
            parent_spatial_id,
            parent_clip_id,
        );
    }

    #[inline]
    fn push_box_shadow_inner(
        builder: &mut WrDisplayListBuilder,
        pre_shadow: StyleBoxShadow,
        border_radius: StyleBorderRadius,
        bounds: LogicalRect,
        clip_rect: LogicalRect,
        shadow_type: BoxShadowClipMode,
        parent_spatial_id: WrSpatialId,
        parent_clip_id: WrClipId,
    ) {
        use webrender::api::{units::LayoutVector2D, PrimitiveFlags as WrPrimitiveFlags};

        use super::{
            wr_translate_border_radius, wr_translate_box_shadow_clip_mode, wr_translate_color_f,
            wr_translate_logical_rect,
        };

        // The pre_shadow is missing the StyleBorderRadius & LayoutRect
        if pre_shadow.clip_mode != shadow_type {
            return;
        }

        let info = WrCommonItemProperties {
            clip_rect: wr_translate_logical_rect(clip_rect),
            spatial_id: parent_spatial_id,
            clip_id: parent_clip_id,
            flags: WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
        };

        builder.push_box_shadow(
            &info,
            wr_translate_logical_rect(bounds),
            LayoutVector2D::new(
                pre_shadow.offset[0].to_pixels(),
                pre_shadow.offset[1].to_pixels(),
            ),
            wr_translate_color_f(apply_gamma(pre_shadow.color.into())),
            pre_shadow.blur_radius.to_pixels(),
            pre_shadow.spread_radius.to_pixels(),
            wr_translate_border_radius(border_radius, bounds.size),
            wr_translate_box_shadow_clip_mode(pre_shadow.clip_mode),
        );
    }

    // Apply a gamma of 2.2 to the original value
    //
    // NOTE: strangely box-shadow is the only thing that needs to be gamma-corrected...
    #[inline(always)]
    fn apply_gamma(color: ColorF) -> ColorF {
        const GAMMA: f32 = 2.2;
        const GAMMA_F: f32 = 1.0 / GAMMA;

        ColorF {
            r: color.r.powf(GAMMA_F),
            g: color.g.powf(GAMMA_F),
            b: color.b.powf(GAMMA_F),
            a: color.a,
        }
    }

    fn get_clip_rect(pre_shadow: &StyleBoxShadow, bounds: LogicalRect) -> LogicalRect {
        if pre_shadow.clip_mode == BoxShadowClipMode::Inset {
            // inset shadows do not work like outset shadows
            // for inset shadows, you have to push a clip ID first, so that they are
            // clipped to the bounds -we trust that the calling function knows to do this
            bounds
        } else {
            // calculate the maximum extent of the outset shadow
            let mut clip_rect = bounds;

            let origin_displace =
                (pre_shadow.spread_radius.to_pixels() + pre_shadow.blur_radius.to_pixels()) * 2.0;
            clip_rect.origin.x =
                clip_rect.origin.x - pre_shadow.offset[0].to_pixels() - origin_displace;
            clip_rect.origin.y =
                clip_rect.origin.y - pre_shadow.offset[1].to_pixels() - origin_displace;

            clip_rect.size.height = clip_rect.size.height + (origin_displace * 2.0);
            clip_rect.size.width = clip_rect.size.width + (origin_displace * 2.0);
            clip_rect
        }
    }
}

mod border {

    use azul_core::{
        display_list::{
            StyleBorderColors, StyleBorderRadius, StyleBorderStyles, StyleBorderWidths,
        },
        window::LogicalSize,
    };
    use azul_css::{BorderStyle, BorderStyleNoNone, CssPropertyValue, LayoutSize, PixelValue};
    use webrender::api::{
        units::LayoutSideOffsets as WrLayoutSideOffsets, BorderDetails as WrBorderDetails,
        BorderSide as WrBorderSide, BorderStyle as WrBorderStyle,
        CommonItemProperties as WrCommonItemProperties, DisplayListBuilder as WrDisplayListBuilder,
    };

    pub(super) fn is_zero_border_radius(border_radius: &StyleBorderRadius) -> bool {
        border_radius.top_left.is_none()
            && border_radius.top_right.is_none()
            && border_radius.bottom_left.is_none()
            && border_radius.bottom_right.is_none()
    }

    pub(super) fn push_border(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        radii: StyleBorderRadius,
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
        current_hidpi_factor: f32,
    ) {
        let clip_rect_size = info.clip_rect.size();
        let rect_size = LogicalSize::new(clip_rect_size.width, clip_rect_size.height);

        if let Some((border_widths, border_details)) = get_webrender_border(
            rect_size,
            radii,
            widths,
            colors,
            styles,
            current_hidpi_factor,
        ) {
            builder.push_border(&info, info.clip_rect, border_widths, border_details);
        }
    }

    /// Returns the merged offsets and details for the top, left,
    /// right and bottom styles - necessary, so we can combine `border-top`,
    /// `border-left`, etc. into one border
    fn get_webrender_border(
        rect_size: LogicalSize,
        radii: StyleBorderRadius,
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
        hidpi: f32,
    ) -> Option<(WrLayoutSideOffsets, WrBorderDetails)> {
        use webrender::api::{BorderRadius as WrBorderRadius, NormalBorder as WrNormalBorder};

        use super::{wr_translate_border_radius, wr_translate_color_u};

        let (width_top, width_right, width_bottom, width_left) = (
            widths
                .top
                .map(|w| w.map_property(|w| w.inner))
                .and_then(CssPropertyValue::get_property_or_default),
            widths
                .right
                .map(|w| w.map_property(|w| w.inner))
                .and_then(CssPropertyValue::get_property_or_default),
            widths
                .bottom
                .map(|w| w.map_property(|w| w.inner))
                .and_then(CssPropertyValue::get_property_or_default),
            widths
                .left
                .map(|w| w.map_property(|w| w.inner))
                .and_then(CssPropertyValue::get_property_or_default),
        );

        let (style_top, style_right, style_bottom, style_left) = (
            get_border_style_normalized(styles.top.map(|s| s.map_property(|s| s.inner))),
            get_border_style_normalized(styles.right.map(|s| s.map_property(|s| s.inner))),
            get_border_style_normalized(styles.bottom.map(|s| s.map_property(|s| s.inner))),
            get_border_style_normalized(styles.left.map(|s| s.map_property(|s| s.inner))),
        );

        let no_border_style = style_top.is_none()
            && style_right.is_none()
            && style_bottom.is_none()
            && style_left.is_none();

        let no_border_width = width_top.is_none()
            && width_right.is_none()
            && width_bottom.is_none()
            && width_left.is_none();

        // border has all borders set to border: none; or all border-widths set to none
        if no_border_style || no_border_width {
            return None;
        }

        let has_no_border_radius = radii.top_left.is_none()
            && radii.top_right.is_none()
            && radii.bottom_left.is_none()
            && radii.bottom_right.is_none();

        let (color_top, color_right, color_bottom, color_left) = (
            colors
                .top
                .and_then(|ct| ct.get_property_or_default())
                .unwrap_or_default(),
            colors
                .right
                .and_then(|cr| cr.get_property_or_default())
                .unwrap_or_default(),
            colors
                .bottom
                .and_then(|cb| cb.get_property_or_default())
                .unwrap_or_default(),
            colors
                .left
                .and_then(|cl| cl.get_property_or_default())
                .unwrap_or_default(),
        );

        // NOTE: if the HiDPI factor is not set to an even number, this will result
        // in uneven border widths. In order to reduce this bug, we multiply the border width
        // with the HiDPI factor, then round the result (to get an even number), then divide again
        let border_widths = WrLayoutSideOffsets::new(
            width_top
                .map(|v| (v.to_pixels(rect_size.height) * hidpi).floor() / hidpi)
                .unwrap_or(0.0),
            width_right
                .map(|v| (v.to_pixels(rect_size.width) * hidpi).floor() / hidpi)
                .unwrap_or(0.0),
            width_bottom
                .map(|v| (v.to_pixels(rect_size.height) * hidpi).floor() / hidpi)
                .unwrap_or(0.0),
            width_left
                .map(|v| (v.to_pixels(rect_size.width) * hidpi).floor() / hidpi)
                .unwrap_or(0.0),
        );

        let border_details = WrBorderDetails::Normal(WrNormalBorder {
            top: WrBorderSide {
                color: wr_translate_color_u(color_top.inner).into(),
                style: translate_wr_border(style_top, width_top),
            },
            left: WrBorderSide {
                color: wr_translate_color_u(color_left.inner).into(),
                style: translate_wr_border(style_left, width_left),
            },
            right: WrBorderSide {
                color: wr_translate_color_u(color_right.inner).into(),
                style: translate_wr_border(style_right, width_right),
            },
            bottom: WrBorderSide {
                color: wr_translate_color_u(color_bottom.inner).into(),
                style: translate_wr_border(style_bottom, width_bottom),
            },
            radius: if has_no_border_radius {
                WrBorderRadius::zero()
            } else {
                wr_translate_border_radius(radii, rect_size)
            },
            do_aa: true, // it isn't known when it's possible to set this to false
        });

        Some((border_widths, border_details))
    }

    #[inline]
    fn get_border_style_normalized(
        style: Option<CssPropertyValue<BorderStyle>>,
    ) -> Option<BorderStyleNoNone> {
        match style {
            None => None,
            Some(s) => s
                .get_property_or_default()
                .and_then(|prop| prop.normalize_border()),
        }
    }

    #[inline]
    fn translate_wr_border(
        style: Option<BorderStyleNoNone>,
        border_width: Option<PixelValue>,
    ) -> WrBorderStyle {
        if border_width.is_none() {
            WrBorderStyle::None
        } else {
            match style {
                None => WrBorderStyle::None,
                Some(BorderStyleNoNone::Solid) => WrBorderStyle::Solid,
                Some(BorderStyleNoNone::Double) => WrBorderStyle::Double,
                Some(BorderStyleNoNone::Dotted) => WrBorderStyle::Dotted,
                Some(BorderStyleNoNone::Dashed) => WrBorderStyle::Dashed,
                Some(BorderStyleNoNone::Hidden) => WrBorderStyle::Hidden,
                Some(BorderStyleNoNone::Groove) => WrBorderStyle::Groove,
                Some(BorderStyleNoNone::Ridge) => WrBorderStyle::Ridge,
                Some(BorderStyleNoNone::Inset) => WrBorderStyle::Inset,
                Some(BorderStyleNoNone::Outset) => WrBorderStyle::Outset,
            }
        }
    }
}
