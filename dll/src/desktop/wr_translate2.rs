//! WebRender type translation functions for shell2
//!
//! This module provides translations between azul-core types and WebRender types,
//! plus hit-testing integration. Simplified version of wr_translate.rs for shell2.

use alloc::{collections::BTreeMap, sync::Arc};
use core::mem;
use std::{cell::RefCell, rc::Rc};

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect},
    hit_test::{DocumentId, PipelineId},
    window::CursorPosition,
};
use azul_layout::{hit_test::FullHitTest, window::DomLayoutResult};
use webrender::{
    api::{
        units::WorldPoint as WrWorldPoint, ApiHitTester as WrApiHitTester,
        DocumentId as WrDocumentId, HitTesterRequest as WrHitTesterRequest,
        PipelineId as WrPipelineId, RenderNotifier as WrRenderNotifier,
    },
    WebRenderOptions as WrRendererOptions,
};
// Re-exports for convenience
pub use webrender::{
    render_api::{RenderApi as WrRenderApi, Transaction as WrTransaction},
    Renderer as WrRenderer,
};

/// Asynchronous hit tester that can be in "requested" or "resolved" state
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

/// WebRender notifier (empty implementation)
#[derive(Debug, Copy, Clone)]
pub struct Notifier {}

impl WrRenderNotifier for Notifier {
    fn clone(&self) -> Box<dyn WrRenderNotifier> {
        Box::new(Notifier {})
    }
    fn wake_up(&self, _composite_needed: bool) {}
    fn new_frame_ready(
        &self,
        _: WrDocumentId,
        _scrolled: bool,
        _composite_needed: bool,
        _frame_publish_id: webrender::api::FramePublishId,
    ) {
    }
}

/// Shader cache (TODO: implement proper caching)
pub const WR_SHADER_CACHE: Option<&Rc<RefCell<webrender::Shaders>>> = None;

/// Default WebRender renderer options
pub fn default_renderer_options(
    options: &azul_layout::window_state::WindowCreateOptions,
) -> WrRendererOptions {
    use webrender::api::ColorF as WrColorF;

    WrRendererOptions {
        resource_override_path: None,
        use_optimized_shaders: true,
        enable_aa: true,
        enable_subpixel_aa: true,
        clear_color: WrColorF {
            r: options.state.background_color.r as f32 / 255.0,
            g: options.state.background_color.g as f32 / 255.0,
            b: options.state.background_color.b as f32 / 255.0,
            a: options.state.background_color.a as f32 / 255.0,
        },
        enable_multithreading: false,
        debug_flags: webrender::api::DebugFlags::empty(), /* TODO: translate from
                                                           * options.state.debug_state */
        ..WrRendererOptions::default()
    }
}

/// Compositor for external image handling (textures, etc.)
#[derive(Debug, Default, Copy, Clone)]
pub struct Compositor {}

impl webrender::api::ExternalImageHandler for Compositor {
    fn lock(
        &mut self,
        key: webrender::api::ExternalImageId,
        _channel_index: u8,
    ) -> webrender::api::ExternalImage {
        use webrender::api::{
            units::{DevicePoint as WrDevicePoint, TexelRect as WrTexelRect},
            ExternalImage as WrExternalImage, ExternalImageSource as WrExternalImageSource,
        };

        // TODO: Implement proper texture lookup using azul_core::gl::get_opengl_texture
        // For now, return invalid texture
        WrExternalImage {
            uv: WrTexelRect {
                uv0: WrDevicePoint::zero(),
                uv1: WrDevicePoint::zero(),
            },
            source: WrExternalImageSource::Invalid,
        }
    }

    fn unlock(&mut self, _key: webrender::api::ExternalImageId, _channel_index: u8) {
        // Single-threaded renderer, nothing to unlock
    }
}

/// Translate DocumentId from azul-core to WebRender
pub fn wr_translate_document_id(document_id: DocumentId) -> WrDocumentId {
    WrDocumentId {
        namespace_id: webrender::api::IdNamespace(document_id.namespace_id.0),
        id: document_id.id,
    }
}

/// Translate DocumentId from WebRender to azul-core
pub fn translate_document_id_wr(document_id: WrDocumentId) -> DocumentId {
    DocumentId {
        namespace_id: azul_core::resources::IdNamespace(document_id.namespace_id.0),
        id: document_id.id,
    }
}

/// Translate IdNamespace from WebRender to azul-core
pub fn translate_id_namespace_wr(
    id_namespace: webrender::api::IdNamespace,
) -> azul_core::resources::IdNamespace {
    azul_core::resources::IdNamespace(id_namespace.0)
}

/// Translate PipelineId from azul-core to WebRender
pub fn wr_translate_pipeline_id(pipeline_id: PipelineId) -> WrPipelineId {
    WrPipelineId(pipeline_id.0, pipeline_id.1)
}

/// Translate ExternalScrollId from azul-core to WebRender
pub fn wr_translate_external_scroll_id(
    scroll_id: azul_core::hit_test::ExternalScrollId,
) -> webrender::api::ExternalScrollId {
    webrender::api::ExternalScrollId(scroll_id.0, wr_translate_pipeline_id(scroll_id.1))
}

/// Translate LogicalPosition from azul-core to WebRender LayoutPoint
pub fn wr_translate_logical_position(
    pos: azul_core::geom::LogicalPosition,
) -> webrender::api::units::LayoutPoint {
    webrender::api::units::LayoutPoint::new(pos.x, pos.y)
}

/// Translate ScrollbarHitId to WebRender ItemTag
///
/// Encoding scheme:
/// - Bits 0-31: NodeId.index() (32 bits)
/// - Bits 32-61: DomId.inner (30 bits)
/// - Bits 62-63: Component type (2 bits)
///   - 00 = VerticalTrack
///   - 01 = VerticalThumb
///   - 10 = HorizontalTrack
///   - 11 = HorizontalThumb
pub fn wr_translate_scrollbar_hit_id(
    hit_id: azul_core::hit_test::ScrollbarHitId,
) -> (webrender::api::ItemTag, webrender::api::units::LayoutPoint) {
    use azul_core::hit_test::ScrollbarHitId;

    let (dom_id, node_id, component_type) = match hit_id {
        ScrollbarHitId::VerticalTrack(dom_id, node_id) => (dom_id, node_id, 0u64),
        ScrollbarHitId::VerticalThumb(dom_id, node_id) => (dom_id, node_id, 1u64),
        ScrollbarHitId::HorizontalTrack(dom_id, node_id) => (dom_id, node_id, 2u64),
        ScrollbarHitId::HorizontalThumb(dom_id, node_id) => (dom_id, node_id, 3u64),
    };

    let tag = (dom_id.inner as u64) << 32 | (node_id.index() as u64) | (component_type << 62);

    // Return tag as (u64, u16) tuple
    ((tag, 0), webrender::api::units::LayoutPoint::zero())
}

/// Translate WebRender ItemTag back to ScrollbarHitId
///
/// Returns None if the tag doesn't represent a scrollbar hit.
pub fn translate_item_tag_to_scrollbar_hit_id(
    tag: webrender::api::ItemTag,
) -> Option<azul_core::hit_test::ScrollbarHitId> {
    use azul_core::{dom::DomId, hit_test::ScrollbarHitId, id::NodeId};

    let (tag_value, _) = tag;
    let component_type = (tag_value >> 62) & 0x3;
    let dom_id_value = ((tag_value >> 32) & 0x3FFFFFFF) as usize;
    let node_id_value = (tag_value & 0xFFFFFFFF) as usize;

    let dom_id = DomId {
        inner: dom_id_value,
    };
    let node_id = NodeId::new(node_id_value);

    match component_type {
        0 => Some(ScrollbarHitId::VerticalTrack(dom_id, node_id)),
        1 => Some(ScrollbarHitId::VerticalThumb(dom_id, node_id)),
        2 => Some(ScrollbarHitId::HorizontalTrack(dom_id, node_id)),
        3 => Some(ScrollbarHitId::HorizontalThumb(dom_id, node_id)),
        _ => None,
    }
}

/// ScrollClamping is no longer part of WebRender API
/// Keeping as stub for compatibility
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ScrollClamping {
    ToContentBounds,
    NoClamping,
}

/// Perform WebRender-based hit testing
///
/// This is the main hit-testing function that uses WebRender's hit tester to determine
/// which DOM nodes are under the cursor. It handles nested iframes and builds a complete
/// hit test result with all hovered nodes.
pub fn fullhittest_new_webrender(
    wr_hittester: &dyn WrApiHitTester,
    document_id: DocumentId,
    old_focus_node: Option<DomNodeId>,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    cursor_position: &CursorPosition,
    hidpi_factor: f32,
) -> FullHitTest {
    use alloc::collections::BTreeMap;

    use azul_core::{
        hit_test::{HitTestItem, ScrollHitTestItem},
        styled_dom::NodeHierarchyItemId,
    };

    let mut cursor_location = match cursor_position {
        CursorPosition::OutOfWindow(_) | CursorPosition::Uninitialized => {
            return FullHitTest::empty(old_focus_node);
        }
        CursorPosition::InWindow(pos) => LogicalPosition::new(pos.x, pos.y),
    };

    // Initialize empty result (focus will be updated if focusable node is found)
    let mut ret = FullHitTest::empty(None);

    let wr_document_id = wr_translate_document_id(document_id);

    // Start with root DOM (DomId 0), will recursively check iframes
    let mut dom_ids = vec![(DomId { inner: 0 }, cursor_location)];

    loop {
        let mut new_dom_ids = Vec::new();

        for (dom_id, cursor_relative_to_dom) in dom_ids.iter() {
            // Each DOM gets its own pipeline ID (DomID is the pipeline source ID)
            let pipeline_id = PipelineId(
                dom_id.inner.min(core::u32::MAX as usize) as u32,
                document_id.id,
            );

            let layout_result = match layout_results.get(dom_id) {
                Some(s) => s,
                None => break,
            };

            // Perform WebRender hit test at cursor position
            let wr_result = wr_hittester.hit_test(WrWorldPoint::new(
                cursor_relative_to_dom.x * hidpi_factor,
                cursor_relative_to_dom.y * hidpi_factor,
            ));

            // Convert WebRender hit test results to azul hit test items
            let hit_items = wr_result
                .items
                .iter()
                .filter_map(|i| {
                    // Map WebRender tag to DOM node ID
                    let node_id = layout_result
                        .styled_dom
                        .tag_ids_to_node_ids
                        .iter()
                        .find(|q| q.tag_id.inner == i.tag.0)?
                        .node_id
                        .into_crate_internal()?;

                    // Use point_relative_to_item from WebRender - this correctly accounts for
                    // all CSS transforms, scroll offsets, and stacking contexts
                    let point_relative_to_item = LogicalPosition::new(
                        i.point_relative_to_item.x,
                        i.point_relative_to_item.y,
                    );

                    Some((
                        node_id,
                        HitTestItem {
                            point_in_viewport: *cursor_relative_to_dom,
                            point_relative_to_item,
                            is_iframe_hit: None, // TODO: Re-enable iframe support when needed
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

            // Process all hit items for this DOM
            for (node_id, item) in hit_items.into_iter() {
                use azul_core::hit_test::{HitTest, OverflowingScrollNode, ScrollHitTestItem};

                // If this is an iframe, queue it for next iteration
                if let Some(i) = item.is_iframe_hit.as_ref() {
                    new_dom_ids.push(*i);
                    continue;
                }

                // Update focused node if this item is focusable
                if item.is_focusable {
                    ret.focused_node = Some((*dom_id, node_id));
                }

                // Always insert into regular_hit_test_nodes
                ret.hovered_nodes
                    .entry(*dom_id)
                    .or_insert_with(|| HitTest::empty())
                    .regular_hit_test_nodes
                    .insert(node_id, item);

                // Check if this node is scrollable using the scroll_id_to_node_id mapping
                // This mapping was precomputed during layout and only contains scrollable nodes
                let is_scrollable = layout_result
                    .scroll_id_to_node_id
                    .values()
                    .any(|&nid| nid == node_id);

                if !is_scrollable {
                    continue;
                }

                // Get node's absolute position and size from layout tree
                let layout_indices = match layout_result.layout_tree.dom_to_layout.get(&node_id) {
                    Some(indices) => indices,
                    None => continue,
                };

                let layout_idx = match layout_indices.first() {
                    Some(&idx) => idx,
                    None => continue,
                };

                let layout_node = match layout_result.layout_tree.get(layout_idx) {
                    Some(node) => node,
                    None => continue,
                };

                // Get node's absolute position and size
                let node_pos = layout_result
                    .absolute_positions
                    .get(&layout_idx)
                    .copied()
                    .unwrap_or_default();

                let node_size = layout_node.used_size.unwrap_or_default();

                let parent_rect = LogicalRect::new(node_pos, node_size);

                // Content size is the child bounds
                // TODO: Calculate actual content bounds from children
                let child_rect = parent_rect;

                // Get the scroll ID from the precomputed mapping
                let scroll_id = layout_result
                    .scroll_ids
                    .get(&layout_idx)
                    .copied()
                    .unwrap_or(0);

                let scroll_node = OverflowingScrollNode {
                    parent_rect,
                    child_rect,
                    virtual_child_rect: child_rect,
                    parent_external_scroll_id: azul_core::hit_test::ExternalScrollId(
                        scroll_id,
                        pipeline_id,
                    ),
                    parent_dom_hash: azul_core::dom::DomNodeHash(node_id.index() as u64),
                    scroll_tag_id: azul_core::dom::ScrollTagId(azul_core::dom::TagId(
                        node_id.index() as u64,
                    )),
                };

                ret.hovered_nodes
                    .entry(*dom_id)
                    .or_insert_with(|| HitTest::empty())
                    .scroll_hit_test_nodes
                    .insert(
                        node_id,
                        ScrollHitTestItem {
                            point_in_viewport: item.point_in_viewport,
                            point_relative_to_item: item.point_relative_to_item,
                            scroll_node,
                        },
                    );
            }
        }

        // Continue with iframes if any were found
        if new_dom_ids.is_empty() {
            break;
        } else {
            dom_ids = new_dom_ids;
        }
    }

    ret
}

// ==================== DISPLAY LIST TRANSLATION STUBS ====================
//
// These functions are stubs for now and will be fully implemented later.
// They provide the basic structure for translating azul layout results
// to WebRender display lists and managing frames.

use azul_core::resources::{ImageCache, ResourceUpdate};
use azul_layout::window::LayoutWindow;

/// Rebuild display list from layout results and send to WebRender
///
/// This is a stub - full implementation will translate DomLayoutResult
/// display lists to WebRender format using compositor2.
pub fn rebuild_display_list(
    layout_window: &mut LayoutWindow,
    render_api: &mut WrRenderApi,
    image_cache: &ImageCache,
    resources: Vec<ResourceUpdate>,
    renderer_resources: &azul_core::resources::RendererResources,
    dpi: f32,
) {
    use webrender::api::units::DeviceIntSize;

    let mut txn = WrTransaction::new();

    // Get viewport size for display list translation
    let physical_size = layout_window.current_window_state.size.get_physical_size();
    let viewport_size = DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

    // Translate display lists for all DOMs (root + iframes)
    for (dom_id, layout_result) in &layout_window.layout_results {
        // DomId maps to PipelineId namespace
        let pipeline_id = wr_translate_pipeline_id(PipelineId(
            dom_id.inner as u32,
            layout_window.document_id.id,
        ));

        // Translate Azul DisplayList to WebRender Transaction
        match crate::desktop::compositor2::translate_displaylist_to_wr(
            &layout_result.display_list,
            pipeline_id,
            viewport_size,
            renderer_resources,
            dpi,
        ) {
            Ok(dl_txn) => {
                eprintln!(
                    "[rebuild_display_list] Sending display list transaction for DOM {} to \
                     document {:?}",
                    dom_id.inner, layout_window.document_id
                );
                // Merge display list transaction into main transaction
                // Note: WebRender Transaction doesn't support merging,
                // so we need to rebuild the transaction with display list
                // For now, just send it separately
                render_api
                    .send_transaction(wr_translate_document_id(layout_window.document_id), dl_txn);
                eprintln!("[rebuild_display_list] Display list transaction sent");
            }
            Err(e) => {
                eprintln!(
                    "[rebuild_display_list] Error translating display list for DOM {}: {}",
                    dom_id.inner, e
                );
            }
        }
    }

    // Update resources (images, fonts)
    if !resources.is_empty() {
        // Convert azul_core ResourceUpdates to webrender ResourceUpdates
        let wr_resources: Vec<webrender::ResourceUpdate> = resources
            .into_iter()
            .filter_map(|r| translate_resource_update(r))
            .collect();

        if !wr_resources.is_empty() {
            txn.update_resources(wr_resources);
            render_api.send_transaction(wr_translate_document_id(layout_window.document_id), txn);
        }
    }
}

/// Translate azul-core ResourceUpdate to WebRender ResourceUpdate
fn translate_resource_update(
    update: azul_core::resources::ResourceUpdate,
) -> Option<webrender::ResourceUpdate> {
    use azul_core::resources::ResourceUpdate as AzResourceUpdate;
    use webrender::ResourceUpdate as WrResourceUpdate;

    match update {
        AzResourceUpdate::AddImage(add_image) => {
            Some(WrResourceUpdate::AddImage(translate_add_image(add_image)?))
        }
        AzResourceUpdate::UpdateImage(update_image) => Some(WrResourceUpdate::UpdateImage(
            translate_update_image(update_image)?,
        )),
        AzResourceUpdate::DeleteImage(key) => {
            Some(WrResourceUpdate::DeleteImage(translate_image_key(key)))
        }
        AzResourceUpdate::AddFont(add_font) => {
            Some(WrResourceUpdate::AddFont(translate_add_font(add_font)?))
        }
        AzResourceUpdate::DeleteFont(key) => {
            Some(WrResourceUpdate::DeleteFont(translate_font_key(key)))
        }
        AzResourceUpdate::AddFontInstance(add_instance) => Some(WrResourceUpdate::AddFontInstance(
            translate_add_font_instance(add_instance)?,
        )),
        AzResourceUpdate::DeleteFontInstance(key) => Some(WrResourceUpdate::DeleteFontInstance(
            wr_translate_font_instance_key(key),
        )),
    }
}

/// Convert azul-core RawImageFormat to WebRender ImageFormat
fn translate_image_format(
    format: azul_core::resources::RawImageFormat,
) -> webrender::api::ImageFormat {
    use azul_core::resources::RawImageFormat;
    use webrender::api::ImageFormat;

    match format {
        RawImageFormat::R8 => ImageFormat::R8,
        RawImageFormat::R16 => ImageFormat::R16,
        RawImageFormat::RG8 => ImageFormat::RG8,
        RawImageFormat::RG16 => ImageFormat::RG16,
        RawImageFormat::RGBA8 => ImageFormat::RGBA8,
        RawImageFormat::BGRA8 => ImageFormat::BGRA8,
        RawImageFormat::RGBAF32 => ImageFormat::RGBAF32,
        // Formats not supported by WebRender - convert to closest equivalent
        RawImageFormat::RGB8 => ImageFormat::RGBA8, // Add alpha channel
        RawImageFormat::RGB16 => ImageFormat::RGBA8, // Convert to 8-bit with alpha
        RawImageFormat::RGBA16 => ImageFormat::RGBA8, // Convert to 8-bit
        RawImageFormat::BGR8 => ImageFormat::BGRA8, // Add alpha channel
        RawImageFormat::RGBF32 => ImageFormat::RGBAF32, // Add alpha channel
    }
}

/// Translate AddImage from azul-core to WebRender
fn translate_add_image(add_image: azul_core::resources::AddImage) -> Option<webrender::AddImage> {
    use webrender::api::{
        units::DeviceIntSize, ImageDescriptor as WrImageDescriptor,
        ImageDescriptorFlags as WrImageDescriptorFlags,
    };

    let mut flags = WrImageDescriptorFlags::empty();
    if add_image.descriptor.flags.is_opaque {
        flags |= WrImageDescriptorFlags::IS_OPAQUE;
    }
    if add_image.descriptor.flags.allow_mipmaps {
        flags |= WrImageDescriptorFlags::ALLOW_MIPMAPS;
    }

    Some(webrender::AddImage {
        key: translate_image_key(add_image.key),
        descriptor: WrImageDescriptor {
            format: translate_image_format(add_image.descriptor.format),
            size: DeviceIntSize::new(
                add_image.descriptor.width as i32,
                add_image.descriptor.height as i32,
            ),
            stride: add_image.descriptor.stride.into_option(),
            offset: add_image.descriptor.offset,
            flags,
        },
        data: translate_image_data(add_image.data),
        tiling: add_image.tiling,
    })
}

/// Translate UpdateImage from azul-core to WebRender
fn translate_update_image(
    update_image: azul_core::resources::UpdateImage,
) -> Option<webrender::UpdateImage> {
    use azul_core::resources::ImageDirtyRect;
    use webrender::api::{
        units::{DeviceIntPoint, DeviceIntSize},
        DirtyRect, ImageDescriptor as WrImageDescriptor,
        ImageDescriptorFlags as WrImageDescriptorFlags,
    };

    let mut flags = WrImageDescriptorFlags::empty();
    if update_image.descriptor.flags.is_opaque {
        flags |= WrImageDescriptorFlags::IS_OPAQUE;
    }
    if update_image.descriptor.flags.allow_mipmaps {
        flags |= WrImageDescriptorFlags::ALLOW_MIPMAPS;
    }

    // ImageDirtyRect is an enum in azul-core
    let dirty_rect = match update_image.dirty_rect {
        ImageDirtyRect::All => DirtyRect::All,
        ImageDirtyRect::Partial(rect) => {
            use webrender::{
                api::units::DevicePixel,
                euclid::{Box2D, Point2D},
            };

            DirtyRect::Partial(Box2D::new(
                Point2D::new(rect.origin.x as i32, rect.origin.y as i32),
                Point2D::new(
                    (rect.origin.x + rect.size.width) as i32,
                    (rect.origin.y + rect.size.height) as i32,
                ),
            ))
        }
    };

    Some(webrender::UpdateImage {
        key: translate_image_key(update_image.key),
        descriptor: WrImageDescriptor {
            format: translate_image_format(update_image.descriptor.format),
            size: DeviceIntSize::new(
                update_image.descriptor.width as i32,
                update_image.descriptor.height as i32,
            ),
            stride: update_image.descriptor.stride.into_option(),
            offset: update_image.descriptor.offset,
            flags,
        },
        data: translate_image_data(update_image.data),
        dirty_rect,
    })
}

/// Translate AddFont from azul-core to WebRender
fn translate_add_font(add_font: azul_core::resources::AddFont) -> Option<webrender::AddFont> {
    // WebRender's AddFont is an enum with Parsed variant
    // azul-core's AddFont already has both key and FontRef
    Some(webrender::AddFont::Parsed(
        translate_font_key(add_font.key),
        add_font.font, // FontRef is Clone
    ))
}

/// Translate AddFontInstance from azul-core to WebRender  
fn translate_add_font_instance(
    add_instance: azul_core::resources::AddFontInstance,
) -> Option<webrender::AddFontInstance> {
    use webrender::api::FontInstanceOptions as WrFontInstanceOptions;

    // Convert Au to f32 pixels: Au units are 1/60th of a pixel
    let glyph_size_px = (add_instance.glyph_size.0 .0 as f32) / 60.0;

    Some(webrender::AddFontInstance {
        key: wr_translate_font_instance_key(add_instance.key),
        font_key: translate_font_key(add_instance.font_key),
        glyph_size: glyph_size_px,
        options: add_instance.options.map(|opts| WrFontInstanceOptions {
            flags: wr_translate_font_instance_flags(opts.flags),
            synthetic_italics: wr_translate_synthetic_italics(opts.synthetic_italics),
            render_mode: wr_translate_font_render_mode(opts.render_mode),
            _padding: 0,
        }),
        platform_options: add_instance.platform_options.map(|_opts| {
            // Platform options are platform-specific, for now use defaults
            webrender::api::FontInstancePlatformOptions::default()
        }),
        variations: add_instance
            .variations
            .iter()
            .map(|v| webrender::api::FontVariation {
                tag: v.tag,
                value: v.value,
            })
            .collect(),
    })
}

/// Translate ImageKey from azul-core to WebRender
fn translate_image_key(key: azul_core::resources::ImageKey) -> webrender::api::ImageKey {
    webrender::api::ImageKey(wr_translate_id_namespace(key.namespace), key.key)
}

/// Translate FontKey from azul-core to WebRender
fn translate_font_key(key: azul_core::resources::FontKey) -> webrender::api::FontKey {
    webrender::api::FontKey(wr_translate_id_namespace(key.namespace), key.key)
}

/// Translate ImageData from azul-core to WebRender
fn translate_image_data(data: azul_core::resources::ImageData) -> webrender::api::ImageData {
    use azul_core::resources::ImageData as AzImageData;
    use webrender::api::ImageData as WrImageData;

    match data {
        // TODO: remove this cloning the image data once imagedata is migrated to ImageRef
        AzImageData::Raw(arc_vec) => WrImageData::Raw(Arc::new(arc_vec.as_slice().to_vec())),
        AzImageData::External(ext_data) => {
            // External images need special handling
            // For now, treat as raw empty data
            eprintln!("[translate_image_data] External image data not yet supported");
            WrImageData::Raw(std::sync::Arc::new(Vec::new()))
        }
    }
}

/// Translate SyntheticItalics from azul-core to WebRender
fn wr_translate_synthetic_italics(
    italics: azul_core::resources::SyntheticItalics,
) -> webrender::api::SyntheticItalics {
    webrender::api::SyntheticItalics {
        angle: italics.angle,
    }
}

/// Generate a new WebRender frame
///
/// This function sets up the scene and tells WebRender to render.
/// Uses DomId-based pipeline management for iframe support.
pub fn generate_frame(
    layout_window: &mut LayoutWindow,
    render_api: &mut WrRenderApi,
    display_list_was_rebuilt: bool,
) {
    use webrender::api::units::{
        DeviceIntPoint as WrDeviceIntPoint, DeviceIntRect as WrDeviceIntRect,
        DeviceIntSize as WrDeviceIntSize,
    };

    let physical_size = layout_window.current_window_state.size.get_physical_size();
    let framebuffer_size =
        WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

    // Don't render if window is minimized (width/height = 0)
    if framebuffer_size.width == 0 || framebuffer_size.height == 0 {
        return;
    }

    let mut txn = WrTransaction::new();

    // Update document view size (in case window was resized)
    txn.set_document_view(WrDeviceIntRect::from_origin_and_size(
        WrDeviceIntPoint::new(0, 0),
        framebuffer_size,
    ));

    // Scroll all nodes to their current positions
    scroll_all_nodes(layout_window, &mut txn);

    // Synchronize GPU values (transforms, opacities, etc.)
    synchronize_gpu_values(layout_window, &mut txn);

    if !display_list_was_rebuilt {
        txn.skip_scene_builder(); // Optimization: skip scene rebuild if DL unchanged
    }

    txn.generate_frame(0, webrender::api::RenderReasons::SCENE);

    eprintln!(
        "[generate_frame] Sending frame transaction to document {:?}",
        layout_window.document_id
    );
    render_api.send_transaction(wr_translate_document_id(layout_window.document_id), txn);
}

/// Synchronize scroll positions from ScrollManager to WebRender
pub fn scroll_all_nodes(layout_window: &LayoutWindow, txn: &mut WrTransaction) {
    use webrender::api::{units::LayoutVector2D as WrLayoutVector2D, SampledScrollOffset};

    // Iterate through all DOMs
    for (dom_id, layout_result) in &layout_window.layout_results {
        let pipeline_id = PipelineId(dom_id.inner as u32, layout_window.document_id.id);

        // Get scroll states for this DOM
        let scroll_states = layout_window
            .scroll_states
            .get_scroll_states_for_dom(*dom_id);

        // Update each scrollable node
        for (node_id, scroll_position) in scroll_states {
            // Get the scroll ID from the layout result
            let scroll_id = layout_result
                .scroll_id_to_node_id
                .iter()
                .find(|(_, &nid)| nid == node_id)
                .map(|(&sid, _)| sid);

            let Some(scroll_id) = scroll_id else {
                continue;
            };

            let external_scroll_id = wr_translate_external_scroll_id(
                azul_core::hit_test::ExternalScrollId(scroll_id, pipeline_id),
            );

            // Calculate scroll offset (origin of children_rect within parent_rect)
            let scroll_offset = WrLayoutVector2D::new(
                scroll_position.children_rect.origin.x,
                scroll_position.children_rect.origin.y,
            );

            // WebRender expects scroll offsets as sampled offsets
            txn.set_scroll_offsets(
                external_scroll_id,
                vec![SampledScrollOffset {
                    offset: scroll_offset,
                    generation: 0, // Generation counter for APZ
                }],
            );
        }
    }
}

/// Synchronize GPU-animated values (transforms, opacities) to WebRender
pub fn synchronize_gpu_values(layout_window: &mut LayoutWindow, txn: &mut WrTransaction) {
    // TODO: Implement transform synchronization
    // This would iterate through GPU value cache and update property values

    // For now, just synchronize scrollbar opacities as an example
    for (dom_id, _layout_result) in &layout_window.layout_results {
        let gpu_cache = layout_window.gpu_state_manager.get_or_create_cache(*dom_id);

        // Synchronize vertical scrollbar opacities
        for ((cache_dom_id, node_id), &opacity) in &gpu_cache.scrollbar_v_opacity_values {
            if cache_dom_id != dom_id {
                continue;
            }

            let opacity_key = match gpu_cache.scrollbar_v_opacity_keys.get(&(*dom_id, *node_id)) {
                Some(&key) => key,
                None => continue,
            };

            // TODO: Actually send opacity update to WebRender
            // This would require a property animation API in WebRender
            // For now, this is a placeholder
            eprintln!(
                "[synchronize_gpu_values] Would set opacity for {:?}:{:?} to {}",
                dom_id, node_id, opacity
            );
        }

        // Synchronize horizontal scrollbar opacities
        for ((cache_dom_id, node_id), &opacity) in &gpu_cache.scrollbar_h_opacity_values {
            if cache_dom_id != dom_id {
                continue;
            }

            let opacity_key = match gpu_cache.scrollbar_h_opacity_keys.get(&(*dom_id, *node_id)) {
                Some(&key) => key,
                None => continue,
            };

            // TODO: Actually send opacity update to WebRender
            eprintln!(
                "[synchronize_gpu_values] Would set opacity for {:?}:{:?} to {}",
                dom_id, node_id, opacity
            );
        }
    }
}

// ========== Additional Translation Functions ==========

use azul_core::{
    geom::LogicalSize,
    resources::{FontInstanceKey, GlyphOptions},
    ui_solver::GlyphInstance,
};
use azul_css::props::{
    basic::color::{ColorF as CssColorF, ColorU as CssColorU},
    style::border_radius::StyleBorderRadius,
};
use webrender::api::{
    units::LayoutSize as WrLayoutSize, BorderRadius as WrBorderRadius, ColorF as WrColorF,
    ColorU as WrColorU, FontInstanceKey as WrFontInstanceKey, GlyphInstance as WrGlyphInstance,
    GlyphOptions as WrGlyphOptions,
};

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
pub fn wr_translate_logical_size(size: LogicalSize) -> WrLayoutSize {
    WrLayoutSize::new(size.width, size.height)
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

    let top_left_px_h = top_left.to_pixels(w);
    let top_left_px_v = top_left.to_pixels(h);

    let top_right_px_h = top_right.to_pixels(w);
    let top_right_px_v = top_right.to_pixels(h);

    let bottom_left_px_h = bottom_left.to_pixels(w);
    let bottom_left_px_v = bottom_left.to_pixels(h);

    let bottom_right_px_h = bottom_right.to_pixels(w);
    let bottom_right_px_v = bottom_right.to_pixels(h);

    WrBorderRadius {
        top_left: WrLayoutSize::new(top_left_px_h as f32, top_left_px_v as f32),
        top_right: WrLayoutSize::new(top_right_px_h as f32, top_right_px_v as f32),
        bottom_left: WrLayoutSize::new(bottom_left_px_h as f32, bottom_left_px_v as f32),
        bottom_right: WrLayoutSize::new(bottom_right_px_h as f32, bottom_right_px_v as f32),
    }
}

#[inline]
const fn wr_translate_id_namespace(
    ns: azul_core::resources::IdNamespace,
) -> webrender::api::IdNamespace {
    webrender::api::IdNamespace(ns.0)
}

#[inline]
pub fn wr_translate_font_instance_key(key: FontInstanceKey) -> WrFontInstanceKey {
    WrFontInstanceKey(wr_translate_id_namespace(key.namespace), key.key)
}

#[inline]
pub fn wr_translate_glyph_options(opts: GlyphOptions) -> WrGlyphOptions {
    WrGlyphOptions {
        render_mode: wr_translate_font_render_mode(opts.render_mode),
        flags: wr_translate_font_instance_flags(opts.flags),
    }
}

#[inline]
fn wr_translate_font_render_mode(
    mode: azul_core::resources::FontRenderMode,
) -> webrender::api::FontRenderMode {
    use azul_core::resources::FontRenderMode::*;
    match mode {
        Mono => webrender::api::FontRenderMode::Mono,
        Alpha => webrender::api::FontRenderMode::Alpha,
        Subpixel => webrender::api::FontRenderMode::Subpixel,
    }
}

#[inline]
fn wr_translate_font_instance_flags(
    flags: azul_core::resources::FontInstanceFlags,
) -> webrender::api::FontInstanceFlags {
    use azul_core::resources::*;

    let mut wr_flags = webrender::api::FontInstanceFlags::empty();

    if flags & FONT_INSTANCE_FLAG_SYNTHETIC_BOLD != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::SYNTHETIC_BOLD;
    }
    if flags & FONT_INSTANCE_FLAG_EMBEDDED_BITMAPS != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::EMBEDDED_BITMAPS;
    }
    if flags & FONT_INSTANCE_FLAG_SUBPIXEL_BGR != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::SUBPIXEL_BGR;
    }
    if flags & FONT_INSTANCE_FLAG_TRANSPOSE != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::TRANSPOSE;
    }
    if flags & FONT_INSTANCE_FLAG_FLIP_X != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::FLIP_X;
    }
    if flags & FONT_INSTANCE_FLAG_FLIP_Y != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::FLIP_Y;
    }

    wr_flags
}

#[inline]
pub fn wr_translate_layouted_glyphs(glyphs: &[GlyphInstance]) -> Vec<WrGlyphInstance> {
    glyphs
        .iter()
        .map(|g| WrGlyphInstance {
            index: g.index,
            point: webrender::api::units::LayoutPoint::new(g.point.x, g.point.y),
        })
        .collect()
}

/// Translate border radius from azul-css to WebRender

/// Translate border style from azul-css to WebRender
#[inline]
fn wr_translate_border_style(
    style: azul_css::props::style::border::BorderStyle,
) -> webrender::api::BorderStyle {
    use azul_css::props::style::border::BorderStyle::*;
    match style {
        None => webrender::api::BorderStyle::None,
        Solid => webrender::api::BorderStyle::Solid,
        Double => webrender::api::BorderStyle::Double,
        Dotted => webrender::api::BorderStyle::Dotted,
        Dashed => webrender::api::BorderStyle::Dashed,
        Hidden => webrender::api::BorderStyle::Hidden,
        Groove => webrender::api::BorderStyle::Groove,
        Ridge => webrender::api::BorderStyle::Ridge,
        Inset => webrender::api::BorderStyle::Inset,
        Outset => webrender::api::BorderStyle::Outset,
    }
}

/// Get WebRender border from Azul border properties
/// Returns None if no border should be rendered
pub fn get_webrender_border(
    rect_size: azul_core::geom::LogicalSize,
    radii: azul_css::props::style::border_radius::StyleBorderRadius,
    widths: azul_layout::solver3::display_list::StyleBorderWidths,
    colors: azul_layout::solver3::display_list::StyleBorderColors,
    styles: azul_layout::solver3::display_list::StyleBorderStyles,
    hidpi: f32,
) -> Option<(
    webrender::api::units::LayoutSideOffsets,
    webrender::api::BorderDetails,
)> {
    use azul_css::{css::CssPropertyValue, props::basic::color::ColorU};
    use webrender::api::{
        units::LayoutSideOffsets as WrLayoutSideOffsets, BorderDetails as WrBorderDetails,
        BorderRadius as WrBorderRadius, BorderSide as WrBorderSide, NormalBorder as WrNormalBorder,
    };

    let (width_top, width_right, width_bottom, width_left) = (
        widths
            .top
            .and_then(|w| w.get_property().cloned())
            .map(|w| w.inner),
        widths
            .right
            .and_then(|w| w.get_property().cloned())
            .map(|w| w.inner),
        widths
            .bottom
            .and_then(|w| w.get_property().cloned())
            .map(|w| w.inner),
        widths
            .left
            .and_then(|w| w.get_property().cloned())
            .map(|w| w.inner),
    );

    let (style_top, style_right, style_bottom, style_left) = (
        styles
            .top
            .and_then(|s| s.get_property().cloned())
            .map(|s| s.inner),
        styles
            .right
            .and_then(|s| s.get_property().cloned())
            .map(|s| s.inner),
        styles
            .bottom
            .and_then(|s| s.get_property().cloned())
            .map(|s| s.inner),
        styles
            .left
            .and_then(|s| s.get_property().cloned())
            .map(|s| s.inner),
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

    let has_no_border_radius = radii.top_left.to_pixels(rect_size.width) == 0.0
        && radii.top_right.to_pixels(rect_size.width) == 0.0
        && radii.bottom_left.to_pixels(rect_size.width) == 0.0
        && radii.bottom_right.to_pixels(rect_size.width) == 0.0;

    let (color_top, color_right, color_bottom, color_left) = (
        colors
            .top
            .and_then(|ct| ct.get_property().cloned())
            .map(|c| c.inner)
            .unwrap_or(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
        colors
            .right
            .and_then(|cr| cr.get_property().cloned())
            .map(|c| c.inner)
            .unwrap_or(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
        colors
            .bottom
            .and_then(|cb| cb.get_property().cloned())
            .map(|c| c.inner)
            .unwrap_or(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
        colors
            .left
            .and_then(|cl| cl.get_property().cloned())
            .map(|c| c.inner)
            .unwrap_or(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
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
            color: wr_translate_color_u(color_top).into(),
            style: style_top
                .map(wr_translate_border_style)
                .unwrap_or(webrender::api::BorderStyle::None),
        },
        left: WrBorderSide {
            color: wr_translate_color_u(color_left).into(),
            style: style_left
                .map(wr_translate_border_style)
                .unwrap_or(webrender::api::BorderStyle::None),
        },
        right: WrBorderSide {
            color: wr_translate_color_u(color_right).into(),
            style: style_right
                .map(wr_translate_border_style)
                .unwrap_or(webrender::api::BorderStyle::None),
        },
        bottom: WrBorderSide {
            color: wr_translate_color_u(color_bottom).into(),
            style: style_bottom
                .map(wr_translate_border_style)
                .unwrap_or(webrender::api::BorderStyle::None),
        },
        radius: if has_no_border_radius {
            WrBorderRadius::zero()
        } else {
            wr_translate_border_radius(radii, rect_size)
        },
        do_aa: true,
    });

    Some((border_widths, border_details))
}
