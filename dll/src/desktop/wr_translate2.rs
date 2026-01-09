//! WebRender type translation functions for shell2
//!
//! This module provides translations between azul-core types and WebRender types,
//! plus hit-testing integration.

use alloc::{collections::BTreeMap, sync::Arc};
use core::mem;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Condvar, Mutex},
};

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect},
    hit_test::{DocumentId, PipelineId},
    resources::{
        AddImage, ImageData as AzImageData, ImageDirtyRect, ImageKey, SyntheticItalics, UpdateImage,
    },
    window::{CursorPosition, DebugState},
};
use azul_layout::{
    hit_test::FullHitTest,
    text3::cache::ParsedFontTrait, // For get_hash() method
    window::DomLayoutResult,
};
use webrender::{
    api::{
        units::{DeviceIntPoint, DeviceIntRect, DeviceIntSize, DevicePixelScale, WorldPoint as WrWorldPoint},
        ApiHitTester as WrApiHitTester, DebugFlags as WrDebugFlags, DirtyRect,
        DocumentId as WrDocumentId, FontInstanceKey as WrFontInstanceKey,
        FontInstanceOptions as WrFontInstanceOptions,
        FontInstancePlatformOptions as WrFontInstancePlatformOptions, FontKey as WrFontKey,
        FontVariation as WrFontVariation, HitTesterRequest as WrHitTesterRequest,
        ImageData as WrImageData, ImageDescriptor as WrImageDescriptor,
        ImageDescriptorFlags as WrImageDescriptorFlags, ImageKey as WrImageKey,
        PipelineId as WrPipelineId, RenderNotifier as WrRenderNotifier,
        RenderReasons as WrRenderReasons, SyntheticItalics as WrSyntheticItalics,
    },
    render_api::{
        AddFontInstance as WrAddFontInstance, AddImage as WrAddImage, UpdateImage as WrUpdateImage,
    },
    WebRenderOptions as WrRendererOptions,
};
// Re-exports for convenience
pub use webrender::{
    render_api::{RenderApi as WrRenderApi, Transaction as WrTransaction},
    Renderer as WrRenderer,
};

use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::log_debug;

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

/// Notifier for WebRender to signal when a new frame is ready
#[derive(Clone)]
pub struct Notifier {
    pub new_frame_ready: Arc<(Mutex<bool>, Condvar)>,
}

impl WrRenderNotifier for Notifier {
    fn clone(&self) -> Box<dyn WrRenderNotifier> {
        Box::new(Notifier {
            new_frame_ready: self.new_frame_ready.clone(),
        })
    }

    fn wake_up(&self, _composite_needed: bool) {
        // Signal that something happened (non-frame-generating update)
        let &(ref lock, ref cvar) = &*self.new_frame_ready;
        let mut new_frame_ready = lock.lock().unwrap();
        *new_frame_ready = true;
        cvar.notify_one();
    }

    fn new_frame_ready(
        &self,
        _doc_id: WrDocumentId,
        _scrolled: bool,
        _composite_needed: bool,
        _frame_publish_id: webrender::api::FramePublishId,
    ) {
        // Signal that a new frame is ready to be rendered
        log_debug!(LogCategory::Rendering, 
            "[Notifier] new_frame_ready called - signaling main thread {_doc_id:?} _scrolled: \
             {_scrolled:?} _composite_needed: {_composite_needed:?} _frame_publish_id: \
             {_frame_publish_id:?}"
        );
        let &(ref lock, ref cvar) = &*self.new_frame_ready;
        let mut new_frame_ready = lock.lock().unwrap();
        *new_frame_ready = true;
        cvar.notify_one();
    }
}

/// Shader cache (TODO: implement proper caching)
pub const WR_SHADER_CACHE: Option<&Rc<RefCell<webrender::Shaders>>> = None;

/// Default WebRender renderer options
pub fn default_renderer_options(
    options: &azul_layout::window_state::WindowCreateOptions,
) -> WrRendererOptions {
    use webrender::{api::ColorF as WrColorF, ShaderPrecacheFlags};

    WrRendererOptions {
        resource_override_path: None,
        use_optimized_shaders: true,
        enable_aa: true,
        enable_subpixel_aa: true,
        clear_color: WrColorF {
            r: options.window_state.background_color.r as f32 / 255.0,
            g: options.window_state.background_color.g as f32 / 255.0,
            b: options.window_state.background_color.b as f32 / 255.0,
            a: options.window_state.background_color.a as f32 / 255.0,
        },
        enable_multithreading: false,
        debug_flags: wr_translate_debug_flags(&options.window_state.debug_state),
        // Enable shader precaching: compile all shaders at initialization
        // This prevents stuttering when shaders are compiled on-demand
        precache_flags: ShaderPrecacheFlags::FULL_COMPILE,
        ..WrRendererOptions::default()
    }
}

/// Compositor for external image handling (textures, etc.)
///
/// This allows WebRender to use externally-managed OpenGL textures (e.g., from ImageCallbacks)
/// by looking them up in the global texture cache.
#[derive(Debug, Default, Copy, Clone)]
pub struct Compositor {}

impl webrender::api::ExternalImageHandler for Compositor {
    fn lock(
        &mut self,
        key: webrender::api::ExternalImageId,
        _channel_index: u8,
    ) -> webrender::api::ExternalImage {
        use azul_core::resources::ExternalImageId;
        use webrender::api::{
            units::{DevicePoint as WrDevicePoint, TexelRect as WrTexelRect},
            ExternalImage as WrExternalImage, ExternalImageSource as WrExternalImageSource,
        };

        // Convert WebRender's external image ID to our type
        let external_image_id = ExternalImageId { inner: key.0 };

        // Look up the texture in the global cache
        match crate::desktop::gl_texture_cache::get_texture(&external_image_id) {
            Some((texture_id, (width, height))) => {
                // Return the native OpenGL texture
                WrExternalImage {
                    uv: WrTexelRect {
                        uv0: WrDevicePoint::zero(),
                        uv1: WrDevicePoint::new(width, height),
                    },
                    source: WrExternalImageSource::NativeTexture(texture_id),
                }
            }
            None => {
                // Texture not found, return invalid
                WrExternalImage {
                    uv: WrTexelRect {
                        uv0: WrDevicePoint::zero(),
                        uv1: WrDevicePoint::zero(),
                    },
                    source: WrExternalImageSource::Invalid,
                }
            }
        }
    }

    fn unlock(&mut self, _key: webrender::api::ExternalImageId, _channel_index: u8) {
        // Single-threaded renderer, nothing to unlock
        // Textures are managed by the global cache with refcounting
    }
}

pub fn wr_translate_debug_flags(new_flags: &DebugState) -> WrDebugFlags {
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
    // Note: TILE_CACHE flag doesn't exist in this WebRender version
    // debug_flags.set(WrDebugFlags::TILE_CACHE, new_flags.tile_cache_logging_dbg);
    debug_flags.set(WrDebugFlags::PROFILER_CAPTURE, new_flags.profiler_capture);
    debug_flags.set(
        WrDebugFlags::FORCE_PICTURE_INVALIDATION,
        new_flags.force_picture_invalidation,
    );

    debug_flags
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

/// Translate physical position to WebRender WorldPoint
/// Used for hit-testing with physical coordinates
pub fn translate_world_point(
    pos: azul_core::geom::PhysicalPosition<f32>,
) -> webrender::api::units::WorldPoint {
    webrender::api::units::WorldPoint::new(pos.x, pos.y)
}

/// Translate WebRender hit test to azul-core FullHitTest
/// This converts the raw hit-test result from WebRender to our internal representation
///
/// NOTE: This is a partial implementation that handles basic hit testing.
/// Full implementation would need to:
/// - Convert WebRender item tags to (DomId, NodeId) pairs
/// - Handle IFrame hits
/// - Extract scrollable nodes from hit results
/// - Properly calculate point_relative_to_item coordinates
pub fn translate_hit_test_result(
    wr_result: webrender::api::HitTestResult,
    _focused_node: Option<azul_core::dom::DomNodeId>,
) -> azul_core::hit_test::FullHitTest {
    use alloc::collections::BTreeMap;

    use azul_core::{
        dom::{DomId, NodeId},
        geom::LogicalPosition,
        hit_test::{FullHitTest, HitTest, HitTestItem},
    };

    let mut hovered_nodes: BTreeMap<DomId, HitTest> = BTreeMap::new();

    for item in wr_result.items {
        // Extract DomId and NodeId from tag
        // Tag encoding: (tag_value, tag_type)
        // For DOM nodes, we encode: (dom_id << 32) | node_id
        let (tag_value, _tag_type) = item.tag;

        // Decode DomId and NodeId from tag
        let dom_id_value = ((tag_value >> 32) & 0xFFFFFFFF) as usize;
        let node_id_value = (tag_value & 0xFFFFFFFF) as usize;

        let dom_id = DomId {
            inner: dom_id_value,
        };
        let node_id = NodeId::new(node_id_value);

        // WebRender changed: point_in_viewport is now point_relative_to_item
        let point_in_viewport =
            LogicalPosition::new(item.point_relative_to_item.x, item.point_relative_to_item.y);

        let point_relative_to_item =
            LogicalPosition::new(item.point_relative_to_item.x, item.point_relative_to_item.y);

        let hit_test_item = HitTestItem {
            point_in_viewport,
            point_relative_to_item,
            is_focusable: false, // TODO: Determine from node data
            is_iframe_hit: None, // IFrames handled via DisplayListItem::IFrame
        };

        hovered_nodes
            .entry(dom_id)
            .or_insert_with(HitTest::empty)
            .regular_hit_test_nodes
            .insert(node_id, hit_test_item);
    }

    FullHitTest {
        hovered_nodes,
        focused_node: _focused_node.into(),
    }
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
    hidpi_factor: DpiScaleFactor,
) -> FullHitTest {
    use alloc::collections::BTreeMap;

    use azul_core::{
        hit_test::{HitTestItem, ScrollHitTestItem},
        styled_dom::NodeHierarchyItemId,
    };

    let mut cursor_location = match cursor_position {
        CursorPosition::OutOfWindow(_) | CursorPosition::Uninitialized => {
            println!("[DEBUG fullhittest] cursor out of window or uninitialized");
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
                None => {
                    println!("[DEBUG fullhittest] no layout_result for dom_id={}", dom_id.inner);
                    break;
                }
            };

            // Perform WebRender hit test at cursor position
            let physical_pos = WrWorldPoint::new(
                cursor_relative_to_dom.x * hidpi_factor.inner.get(),
                cursor_relative_to_dom.y * hidpi_factor.inner.get(),
            );
            let wr_result = wr_hittester.hit_test(physical_pos);
            
            println!("[DEBUG fullhittest] cursor_logical=({}, {}), physical=({}, {}), hidpi={}, wr_result.items.len()={}", 
                cursor_relative_to_dom.x, cursor_relative_to_dom.y,
                physical_pos.x, physical_pos.y,
                hidpi_factor.inner.get(),
                wr_result.items.len());

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
                            is_iframe_hit: None, // IFrames handled via DisplayListItem::IFrame
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
                    ret.focused_node = Some(azul_core::dom::DomNodeId { dom: *dom_id, node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node_id)) }).into();
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

                // Get node's calculated layout position and size
                let node_pos = layout_result
                    .calculated_positions
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
                    parent_dom_hash: azul_core::dom::DomNodeHash { inner: node_id.index() as u64 },
                    scroll_tag_id: azul_core::dom::ScrollTagId {
                        inner: azul_core::dom::TagId {
                            inner: node_id.index() as u64,
                        },
                    },
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

// DISPLAY LIST TRANSLATION STUBS
//
// These functions are stubs for now and will be fully implemented later.
// They provide the basic structure for translating azul layout results
// to WebRender display lists and managing frames.

use std::collections::{HashMap, HashSet};

use azul_core::resources::{
    AddFont, AddFontInstance, Au, DpiScaleFactor, FontKey, ImageCache, ResourceUpdate,
};
use azul_layout::window::LayoutWindow;

/// Generate FontKey deterministically from font hash
/// This ensures the same font always gets the same key across frames
fn font_key_from_hash(font_hash: u64) -> FontKey {
    // Split the 64-bit hash into namespace (upper 32 bits) and key (lower 32 bits)
    let namespace = ((font_hash >> 32) & 0xFFFFFFFF) as u32;
    let key = (font_hash & 0xFFFFFFFF) as u32;

    // Ensure namespace is non-zero (WebRender requirement)
    let namespace = if namespace == 0 { 1 } else { namespace };

    FontKey {
        namespace: azul_core::resources::IdNamespace(namespace),
        key,
    }
}

/// Collect all fonts used in layout results and generate ResourceUpdates
///
/// Helper function to store GL textures - used as function pointer
fn store_gl_texture(
    _doc_id: azul_core::hit_test::DocumentId,
    _epoch: azul_core::resources::Epoch,
    _texture: azul_core::gl::Texture,
) -> azul_core::resources::ExternalImageId {
    // TODO: Actually store the texture in gl_texture_cache
    // For now, just generate a unique ID
    azul_core::resources::ExternalImageId::new()
}

/// Collects all ImageRefs from display lists and creates AddImage ResourceUpdates
/// for images that aren't already registered.
///
/// Unlike fonts, ImageKeys are generated directly from ImageRefHash using
/// image_ref_hash_to_image_key(), so no separate mapping table is needed.
pub fn collect_image_resource_updates(
    layout_window: &LayoutWindow,
    renderer_resources: &azul_core::resources::RendererResources,
) -> Vec<(
    azul_core::resources::ImageRefHash,
    azul_core::resources::AddImageMsg,
)> {
    use azul_core::{
        resources::{build_add_image_resource_updates, image_ref_get_hash},
        FastBTreeSet,
    };
    use azul_layout::solver3::display_list::DisplayListItem;

    log_debug!(LogCategory::Rendering, 
        "[collect_image_resource_updates] Scanning {} DOMs for images",
        layout_window.layout_results.len()
    );

    // Collect all unique ImageRefs from display lists
    let mut images_in_dom = FastBTreeSet::new();

    for (dom_id, layout_result) in &layout_window.layout_results {
        // Scan display list for Image items
        for item in &layout_result.display_list.items {
            if let DisplayListItem::Image { key, .. } = item {
                // Look up ImageRefHash from ImageKey using the reverse map
                if let Some(image_ref_hash) = renderer_resources.image_key_map.get(key) {
                    // Get the actual ImageRef from currently_registered_images
                    if let Some(resolved_image) = renderer_resources
                        .currently_registered_images
                        .get(image_ref_hash)
                    {
                        // ImageRef needs to be reconstructed from ResolvedImage
                        // For now, we skip images already in display list as they're already
                        // registered New images will be caught by the
                        // StyledDom scan below
                    }
                }
            }
        }

        // Also scan the StyledDom for Image nodes
        use azul_core::dom::NodeType;
        let node_data_container = layout_result.styled_dom.node_data.as_container();
        for i in 0..node_data_container.len() {
            if let Some(node_data) = node_data_container.get(azul_core::id::NodeId::new(i)) {
                if let NodeType::Image(image_ref) = node_data.get_node_type() {
                    images_in_dom.insert(image_ref.clone());
                }
            }
        }
    }

    log_debug!(LogCategory::Rendering, 
        "[collect_image_resource_updates] Found {} unique images",
        images_in_dom.len()
    );

    // Build AddImage messages for new images using our gl_texture_integration
    let image_updates = build_add_image_resource_updates(
        renderer_resources,
        layout_window.id_namespace,
        layout_window.epoch,
        &layout_window.document_id,
        &images_in_dom,
        crate::desktop::gl_texture_integration::insert_into_active_gl_textures,
    );

    log_debug!(LogCategory::Rendering, 
        "[collect_image_resource_updates] Generated {} AddImage messages",
        image_updates.len()
    );

    image_updates
}

/// This scans all display lists for Text items, extracts their font_hashes,
/// loads the fonts from the FontManager, and creates AddFont + AddFontInstance ResourceUpdates.
///
/// CRITICAL: FontKey is generated deterministically from font hash to ensure
/// consistency between layout (which uses hash) and rendering (which uses key).
pub fn collect_font_resource_updates(
    layout_window: &LayoutWindow,
    renderer_resources: &azul_core::resources::RendererResources,
    dpi_factor: DpiScaleFactor,
) -> Vec<ResourceUpdate> {
    use std::collections::BTreeMap;

    use azul_core::resources::{
        AddFontInstance, FontInstanceKey, FontInstanceOptions, FontInstancePlatformOptions,
        FontRenderMode, IdNamespace, FONT_INSTANCE_FLAG_NO_AUTOHINT,
    };
    use azul_layout::solver3::display_list::{DisplayList, DisplayListItem};

    log_debug!(LogCategory::Rendering, 
        "[collect_font_resource_updates] Scanning {} DOMs for fonts",
        layout_window.layout_results.len()
    );

    // Map from font_hash to set of font sizes
    let mut font_hash_sizes: BTreeMap<u64, HashSet<Au>> = BTreeMap::new();
    let mut resource_updates = Vec::new();

    // Collect all unique font hash + size combinations from display lists
    for (dom_id, layout_result) in &layout_window.layout_results {
        for item in &layout_result.display_list.items {
            if let DisplayListItem::Text {
                font_hash,
                font_size_px,
                ..
            } = item
            {
                let font_sizes = font_hash_sizes
                    .entry(font_hash.font_hash)
                    .or_insert_with(HashSet::new);
                let font_size_au = Au::from_px(*font_size_px);
                font_sizes.insert(font_size_au);
            }
        }
    }

    log_debug!(LogCategory::Rendering, 
        "[collect_font_resource_updates] Found {} unique fonts with various sizes",
        font_hash_sizes.len()
    );

    // For each font hash, check if it's already registered
    for (&font_hash, font_sizes) in &font_hash_sizes {
        let font_key = font_key_from_hash(font_hash);

        // Check if font itself is already registered
        let font_needs_registration = !renderer_resources.font_hash_map.contains_key(&font_hash);

        if font_needs_registration {
            // Get the FontRef from the layout's font manager
            if let Some(font_ref) = layout_window.font_manager.get_font_by_hash(font_hash) {
                log_debug!(LogCategory::Rendering, 
                    "[collect_font_resource_updates] Font found, parsed ptr: {:?}",
                    font_ref.get_parsed()
                );

                resource_updates.push(ResourceUpdate::AddFont(AddFont {
                    key: font_key,
                    font: font_ref.clone(),
                }));

                log_debug!(LogCategory::Rendering, 
                    "[collect_font_resource_updates] ✓ Created AddFont for hash {} -> key {:?}",
                    font_hash, font_key
                );
            } else {
                log_debug!(LogCategory::Rendering, 
                    "[collect_font_resource_updates] ✗ WARNING: Font {} not found in FontManager!",
                    font_hash
                );
                continue;
            }
        }

        // Register font instances for each size
        for &font_size in font_sizes {
            // Check if this font instance already exists
            let instance_exists = renderer_resources
                .currently_registered_fonts
                .get(&font_key)
                .and_then(|(_, instances)| instances.get(&(font_size, dpi_factor)))
                .is_some();

            if !instance_exists {
                let font_instance_key =
                    FontInstanceKey::unique(IdNamespace((font_hash >> 32) as u32));

                #[cfg(target_os = "macos")]
                let platform_options = FontInstancePlatformOptions::default();

                #[cfg(target_os = "windows")]
                let platform_options = FontInstancePlatformOptions {
                    gamma: 300,
                    contrast: 100,
                    cleartype_level: 100,
                };

                #[cfg(target_os = "linux")]
                let platform_options = FontInstancePlatformOptions {
                    lcd_filter: azul_core::resources::FontLCDFilter::Default,
                    hinting: azul_core::resources::FontHinting::Normal,
                };

                let options = FontInstanceOptions {
                    render_mode: FontRenderMode::Subpixel,
                    flags: FONT_INSTANCE_FLAG_NO_AUTOHINT,
                    ..Default::default()
                };

                resource_updates.push(ResourceUpdate::AddFontInstance(AddFontInstance {
                    key: font_instance_key,
                    font_key,
                    glyph_size: (font_size, dpi_factor),
                    options: Some(options),
                    platform_options: Some(platform_options),
                    variations: Vec::new(),
                }));

                log_debug!(LogCategory::Rendering, 
                    "[collect_font_resource_updates] ✓ Created AddFontInstance for size {:?} @ \
                     dpi {:?}",
                    font_size, dpi_factor
                );
            }
        }
    }

    log_debug!(LogCategory::Rendering, 
        "[collect_font_resource_updates] Generated {} resource updates",
        resource_updates.len()
    );
    resource_updates
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
fn translate_add_image(add_image: AddImage) -> Option<WrAddImage> {
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
fn translate_update_image(update_image: UpdateImage) -> Option<WrUpdateImage> {
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

    Some(WrUpdateImage {
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
    log_debug!(LogCategory::Rendering, 
        "[translate_add_font] Translating FontKey {:?}, parsed ptr: {:?}",
        add_font.key,
        add_font.font.get_parsed()
    );

    Some(webrender::AddFont::Parsed(
        translate_font_key(add_font.key),
        add_font.font, // FontRef is Clone
    ))
}

/// Translate AddFontInstance from azul-core to WebRender  
fn translate_add_font_instance(add_instance: AddFontInstance) -> Option<WrAddFontInstance> {
    // Convert Au to f32 pixels: Au units are 1/60th of a pixel
    // glyph_size is (Au, DpiScaleFactor) 
    let font_size_au = add_instance.glyph_size.0;
    let dpi_factor = add_instance.glyph_size.1.inner.get();
    
    // Convert Au to logical pixels (1 Au = 1/60 px), then multiply by DPI factor
    // to get the physical pixel size for rasterization.
    // NOTE: We multiply by DPI factor here because azul's layout outputs coordinates
    // in physical pixels, so WebRender's global_device_pixel_scale is 1.0.
    // Therefore we need to pre-scale fonts to the correct physical size.
    let glyph_size_px = (font_size_au.0 as f32) / 60.0 * dpi_factor;

    log_debug!(LogCategory::Rendering, 
        "[translate_add_font_instance] Converting Au({}) to {}px (physical), dpi={}",
        font_size_au.0, glyph_size_px, dpi_factor
    );

    Some(WrAddFontInstance {
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
            WrFontInstancePlatformOptions::default()
        }),
        variations: add_instance
            .variations
            .iter()
            .map(|v| WrFontVariation {
                tag: v.tag,
                value: v.value,
            })
            .collect(),
    })
}

/// Translate ImageKey from azul-core to WebRender
pub fn translate_image_key(key: ImageKey) -> WrImageKey {
    WrImageKey(wr_translate_id_namespace(key.namespace), key.key)
}

/// Collect all ImageKeys used in a display list
fn collect_image_keys_from_display_list(
    display_list: &azul_layout::solver3::display_list::DisplayList,
) -> Vec<ImageKey> {
    use azul_layout::solver3::display_list::DisplayListItem;

    let mut image_keys = Vec::new();

    for item in &display_list.items {
        if let DisplayListItem::Image { key, .. } = item {
            image_keys.push(*key);
        }
    }

    image_keys
}

/// Translate FontKey from azul-core to WebRender
fn translate_font_key(key: FontKey) -> WrFontKey {
    WrFontKey(wr_translate_id_namespace(key.namespace), key.key)
}

/// Translate ImageData from azul-core to WebRender's ImageData
///
/// Note: Both types now use SharedRawImageData for the Raw variant,
/// so we only need to translate the External variant's structure.
fn translate_image_data(data: azul_core::resources::ImageData) -> webrender::api::ImageData {
    use azul_core::resources::ImageData as AzImageData;

    match data {
        AzImageData::Raw(shared_data) => {
            // SharedRawImageData can be passed directly
            webrender::api::ImageData::Raw(shared_data)
        }
        AzImageData::External(ext_data) => {
            // External images need structure translation
            webrender::api::ImageData::External(webrender::api::ExternalImageData {
                id: webrender::api::ExternalImageId(ext_data.id.inner),
                channel_index: ext_data.channel_index,
                image_type: match ext_data.image_type {
                    azul_core::resources::ExternalImageType::TextureHandle(kind) => {
                        webrender::api::ExternalImageType::TextureHandle(match kind {
                            azul_core::resources::ImageBufferKind::Texture2D => {
                                webrender::api::ImageBufferKind::Texture2D
                            }
                            azul_core::resources::ImageBufferKind::TextureRect => {
                                webrender::api::ImageBufferKind::TextureRect
                            }
                            azul_core::resources::ImageBufferKind::TextureExternal => {
                                webrender::api::ImageBufferKind::TextureExternal
                            }
                        })
                    }
                    azul_core::resources::ExternalImageType::Buffer => {
                        webrender::api::ExternalImageType::Buffer
                    }
                },
                normalized_uvs: false, // azul-core doesn't track this, default to false
            })
        }
    }
}

/// Translate SyntheticItalics from azul-core to WebRender
fn wr_translate_synthetic_italics(italics: SyntheticItalics) -> WrSyntheticItalics {
    WrSyntheticItalics {
        angle: italics.angle,
    }
}

/// Generate a new WebRender frame
///
/// This function sets up the scene and tells WebRender to render.
/// Uses DomId-based pipeline management for iframe support.
pub fn generate_frame(
    txn: &mut WrTransaction,
    layout_window: &mut LayoutWindow,
    render_api: &mut WrRenderApi,
    display_list_was_rebuilt: bool,
) {
    let physical_size = layout_window.current_window_state.size.get_physical_size();
    let framebuffer_size =
        DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

    // Don't render if window is minimized (width/height = 0)
    if framebuffer_size.width == 0 || framebuffer_size.height == 0 {
        return;
    }

    // CRITICAL: Build display list FIRST, then set root pipeline (matching upstream WebRender
    // order)
    let root_pipeline_id = wr_translate_pipeline_id(PipelineId(0, layout_window.document_id.id));

    // If display list was rebuilt, add resources and display lists to this transaction FIRST
    if display_list_was_rebuilt {
        log_debug!(LogCategory::Rendering, 
            "[generate_frame] Display list was rebuilt - adding resources and display lists to \
             transaction"
        );

        // Re-collect font resources (already cached in renderer_resources)
        let font_updates = collect_font_resource_updates(
            layout_window,
            &layout_window.renderer_resources,
            layout_window.current_window_state.size.get_hidpi_factor(),
        );

        // Collect image resources
        let image_updates =
            collect_image_resource_updates(layout_window, &layout_window.renderer_resources);

        log_debug!(LogCategory::Rendering, 
            "[generate_frame] Collected {} image updates",
            image_updates.len()
        );

        // Update currently_registered_images with new images
        for (image_ref_hash, add_image_msg) in &image_updates {
            use azul_core::resources::ResolvedImage;

            let resolved_image = ResolvedImage {
                key: add_image_msg.0.key,
                descriptor: add_image_msg.0.descriptor,
            };

            layout_window
                .renderer_resources
                .currently_registered_images
                .insert(*image_ref_hash, resolved_image);

            // Also update reverse lookup map
            layout_window
                .renderer_resources
                .image_key_map
                .insert(add_image_msg.0.key, *image_ref_hash);

            log_debug!(LogCategory::Rendering, 
                "[generate_frame] Registered ImageRefHash({}) -> ImageKey {:?}",
                image_ref_hash.inner, add_image_msg.0.key
            );
        }

        // Update font_hash_map and currently_registered_fonts as we process resources
        // This is CRITICAL for push_text() to look up FontKey from font_hash
        for resource in &font_updates {
            match resource {
                ResourceUpdate::AddFont(ref add_font) => {
                    // Update font_hash_map
                    layout_window
                        .renderer_resources
                        .font_hash_map
                        .insert(add_font.font.get_hash(), add_font.key);

                    // Update currently_registered_fonts with empty instance map
                    layout_window
                        .renderer_resources
                        .currently_registered_fonts
                        .entry(add_font.key)
                        .or_insert_with(|| (add_font.font.clone(), BTreeMap::default()));

                    log_debug!(LogCategory::Rendering, 
                        "[generate_frame] Registered font_hash {} -> FontKey {:?}",
                        add_font.font.get_hash(),
                        add_font.key
                    );
                }
                ResourceUpdate::AddFontInstance(ref add_font_instance) => {
                    // Update currently_registered_fonts with font instance
                    if let Some((_, instances)) = layout_window
                        .renderer_resources
                        .currently_registered_fonts
                        .get_mut(&add_font_instance.font_key)
                    {
                        let size = add_font_instance.glyph_size;
                        instances.insert(size, add_font_instance.key);
                        log_debug!(LogCategory::Rendering, 
                            "[generate_frame] Registered FontInstanceKey {:?} for FontKey {:?} at \
                             size {:?}",
                            add_font_instance.key, add_font_instance.font_key, size
                        );
                    }
                }
                _ => {}
            }
        }

        // Translate to WebRender resources
        if !font_updates.is_empty() {
            let wr_resources: Vec<webrender::ResourceUpdate> = font_updates
                .into_iter()
                .filter_map(|r| translate_resource_update(r))
                .collect();

            log_debug!(LogCategory::Rendering, 
                "[generate_frame] Adding {} font resources to transaction",
                wr_resources.len()
            );
            txn.update_resources(wr_resources);
        }

        // Translate image updates to WebRender resources
        if !image_updates.is_empty() {
            let wr_image_resources: Vec<webrender::ResourceUpdate> = image_updates
                .into_iter()
                .map(|(_, add_image_msg)| {
                    translate_resource_update(add_image_msg.into_resource_update())
                })
                .filter_map(|x| x)
                .collect();

            log_debug!(LogCategory::Rendering, 
                "[generate_frame] Adding {} image resources to transaction",
                wr_image_resources.len()
            );
            txn.update_resources(wr_image_resources);
        }

        // Build display lists for all DOMs and add to transaction
        let viewport_size =
            DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
        let dpi = layout_window.current_window_state.size.get_hidpi_factor();

        for (dom_id, layout_result) in &layout_window.layout_results {
            let pipeline_id = wr_translate_pipeline_id(PipelineId(
                dom_id.inner as u32,
                layout_window.document_id.id,
            ));

            match crate::desktop::compositor2::translate_displaylist_to_wr(
                &layout_result.display_list,
                pipeline_id,
                viewport_size,
                &layout_window.renderer_resources,
                dpi,
                Vec::new(), // Resources already added above
                &layout_window.layout_results,
                layout_window.document_id.id,
            ) {
                Ok((_, built_display_list, nested_pipelines)) => {
                    log_debug!(LogCategory::Rendering, 
                        "[generate_frame] Adding display list for DOM {} to transaction (with {} \
                         nested pipelines), display_list_size_in_bytes={}",
                        dom_id.inner,
                        nested_pipelines.len(),
                        built_display_list.size_in_bytes(),
                    );

                    // Add main pipeline
                    txn.set_display_list(
                        webrender::api::Epoch(layout_window.epoch.into_u32()),
                        (pipeline_id, built_display_list),
                    );

                    // Add all nested iframe pipelines
                    for (nested_pipeline_id, nested_display_list) in nested_pipelines {
                        log_debug!(LogCategory::Rendering, 
                            "[generate_frame] Adding nested pipeline {:?} to transaction",
                            nested_pipeline_id
                        );
                        txn.set_display_list(
                            webrender::api::Epoch(layout_window.epoch.into_u32()),
                            (nested_pipeline_id, nested_display_list),
                        );
                    }
                }
                Err(e) => {
                    log_debug!(LogCategory::Rendering, 
                        "[generate_frame] Error building display list for DOM {}: {}",
                        dom_id.inner, e
                    );
                }
            }
        }

        // Increment epoch after using it
        layout_window.epoch.increment();
    } else {
        log_debug!(LogCategory::Rendering, "[generate_frame] Display list unchanged - skipping scene builder");
        txn.skip_scene_builder();
    }

    // CRITICAL: Set root pipeline AFTER display list (matching upstream WebRender order)
    log_debug!(LogCategory::Rendering, 
        "[generate_frame] Setting root pipeline to {:?}",
        root_pipeline_id
    );
    txn.set_root_pipeline(root_pipeline_id);

    // Update document view size (in case window was resized)
    let view_rect =
        DeviceIntRect::from_origin_and_size(DeviceIntPoint::new(0, 0), framebuffer_size);
    let hidpi_factor = layout_window.current_window_state.size.get_hidpi_factor();
    log_debug!(LogCategory::Rendering, "[generate_frame] Setting document view: {:?}, hidpi: {}", view_rect, hidpi_factor.inner.get());
    txn.set_document_view(view_rect, DevicePixelScale::new(hidpi_factor.inner.get()));

    // Process image callback updates (if any callbacks requested re-rendering)
    process_image_callback_updates(layout_window, txn);

    // Process IFrame updates (if any callbacks requested re-rendering)
    process_iframe_updates(layout_window, txn);

    // Scroll all nodes to their current positions
    scroll_all_nodes(layout_window, txn);

    // Synchronize GPU values (transforms, opacities, etc.)
    synchronize_gpu_values(layout_window, txn);

    log_debug!(LogCategory::Rendering, "[generate_frame] Calling generate_frame on transaction");
    txn.generate_frame(0, WrRenderReasons::empty());

    log_debug!(LogCategory::Rendering, 
        "[generate_frame] Sending unified transaction (root_pipeline + document_view + resources \
         + display_lists) to document {:?}",
        layout_window.document_id
    );
}

/// Synchronize scroll positions from ScrollManager to WebRender
pub fn scroll_all_nodes(layout_window: &LayoutWindow, txn: &mut WrTransaction) {
    use webrender::api::{units::LayoutVector2D as WrLayoutVector2D, SampledScrollOffset};

    // Iterate through all DOMs
    for (dom_id, layout_result) in &layout_window.layout_results {
        let pipeline_id = PipelineId(dom_id.inner as u32, layout_window.document_id.id);

        // Get scroll states for this DOM
        let scroll_states = layout_window
            .scroll_manager
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
    use webrender::api::{DynamicProperties, PropertyBinding, PropertyValue};

    // Collect all dynamic properties to update
    let mut properties = DynamicProperties {
        transforms: Vec::new(),
        floats: Vec::new(),
        colors: Vec::new(),
    };

    // Synchronize opacity values from GPU cache
    for (dom_id, _layout_result) in &layout_window.layout_results {
        let gpu_cache = layout_window.gpu_state_manager.get_or_create_cache(*dom_id);

        // Synchronize vertical scrollbar opacities
        for ((cache_dom_id, node_id), &opacity) in &gpu_cache.scrollbar_v_opacity_values {
            if cache_dom_id != dom_id {
                continue;
            }

            if let Some(&opacity_key) = gpu_cache.scrollbar_v_opacity_keys.get(&(*dom_id, *node_id))
            {
                // Add opacity property update
                // Convert OpacityKey to PropertyBindingKey<f32> using its id field (usize -> u64)
                properties.floats.push(PropertyValue {
                    key: webrender::api::PropertyBindingKey::new(opacity_key.id as u64),
                    value: opacity,
                });

                log_debug!(LogCategory::Rendering, 
                    "[synchronize_gpu_values] Set vertical scrollbar opacity for {:?}:{:?} to {} \
                     (key={:?})",
                    dom_id, node_id, opacity, opacity_key
                );
            }
        }

        // Synchronize horizontal scrollbar opacities
        for ((cache_dom_id, node_id), &opacity) in &gpu_cache.scrollbar_h_opacity_values {
            if cache_dom_id != dom_id {
                continue;
            }

            if let Some(&opacity_key) = gpu_cache.scrollbar_h_opacity_keys.get(&(*dom_id, *node_id))
            {
                // Add opacity property update
                // Convert OpacityKey to PropertyBindingKey<f32> using its id field (usize -> u64)
                properties.floats.push(PropertyValue {
                    key: webrender::api::PropertyBindingKey::new(opacity_key.id as u64),
                    value: opacity,
                });

                log_debug!(LogCategory::Rendering, 
                    "[synchronize_gpu_values] Set horizontal scrollbar opacity for {:?}:{:?} to \
                     {} (key={:?})",
                    dom_id, node_id, opacity, opacity_key
                );
            }
        }

        // TODO: Synchronize transform values
        // This would work similarly:
        // for ((cache_dom_id, node_id), &transform) in &gpu_cache.transform_values {
        //     if let Some(&transform_key) = gpu_cache.transform_keys.get(&(*dom_id, *node_id)) {
        //         properties.transforms.push(PropertyValue {
        //             key: transform_key,
        //             value: wr_translate_transform(transform),
        //         });
        //     }
        // }
    }

    // Apply all property updates to the transaction
    if !properties.floats.is_empty()
        || !properties.transforms.is_empty()
        || !properties.colors.is_empty()
    {
        // Store lengths before moving properties
        let float_count = properties.floats.len();
        let transform_count = properties.transforms.len();
        let color_count = properties.colors.len();

        // WebRender renamed update_dynamic_properties to append_dynamic_properties
        txn.append_dynamic_properties(properties);

        log_debug!(LogCategory::Rendering, 
            "[synchronize_gpu_values] Updated {} float properties, {} transforms, {} colors",
            float_count, transform_count, color_count
        );
    }
}

// Additional Translation Functions

use azul_core::{
    geom::LogicalSize,
    resources::{FontInstanceKey, GlyphOptions},
    ui_solver::GlyphInstance,
};
use azul_css::props::{
    basic::{
        color::{ColorF as CssColorF, ColorU as CssColorU},
        pixel::DEFAULT_FONT_SIZE,
    },
    style::border_radius::StyleBorderRadius,
};
use webrender::api::{
    units::LayoutSize as WrLayoutSize, BorderRadius as WrBorderRadius, ColorF as WrColorF,
    ColorU as WrColorU, GlyphInstance as WrGlyphInstance, GlyphOptions as WrGlyphOptions,
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

    let top_left_px_h = top_left.to_pixels_internal(w, DEFAULT_FONT_SIZE);
    let top_left_px_v = top_left.to_pixels_internal(h, DEFAULT_FONT_SIZE);

    let top_right_px_h = top_right.to_pixels_internal(w, DEFAULT_FONT_SIZE);
    let top_right_px_v = top_right.to_pixels_internal(h, DEFAULT_FONT_SIZE);

    let bottom_left_px_h = bottom_left.to_pixels_internal(w, DEFAULT_FONT_SIZE);
    let bottom_left_px_v = bottom_left.to_pixels_internal(h, DEFAULT_FONT_SIZE);

    let bottom_right_px_h = bottom_right.to_pixels_internal(w, DEFAULT_FONT_SIZE);
    let bottom_right_px_v = bottom_right.to_pixels_internal(h, DEFAULT_FONT_SIZE);

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

    let has_no_border_radius = radii
        .top_left
        .to_pixels_internal(rect_size.width, DEFAULT_FONT_SIZE)
        == 0.0
        && radii
            .top_right
            .to_pixels_internal(rect_size.width, DEFAULT_FONT_SIZE)
            == 0.0
        && radii
            .bottom_left
            .to_pixels_internal(rect_size.width, DEFAULT_FONT_SIZE)
            == 0.0
        && radii
            .bottom_right
            .to_pixels_internal(rect_size.width, DEFAULT_FONT_SIZE)
            == 0.0;

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
            .map(|v| {
                (v.to_pixels_internal(rect_size.height, DEFAULT_FONT_SIZE) * hidpi).floor() / hidpi
            })
            .unwrap_or(0.0),
        width_right
            .map(|v| {
                (v.to_pixels_internal(rect_size.width, DEFAULT_FONT_SIZE) * hidpi).floor() / hidpi
            })
            .unwrap_or(0.0),
        width_bottom
            .map(|v| {
                (v.to_pixels_internal(rect_size.height, DEFAULT_FONT_SIZE) * hidpi).floor() / hidpi
            })
            .unwrap_or(0.0),
        width_left
            .map(|v| {
                (v.to_pixels_internal(rect_size.width, DEFAULT_FONT_SIZE) * hidpi).floor() / hidpi
            })
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

/// Build a complete atomic WebRender transaction (matching upstream WebRender pattern)
/// This creates ONE transaction with: resources + display lists + root_pipeline + document_view
pub fn build_webrender_transaction(
    txn: &mut WrTransaction,
    layout_window: &mut LayoutWindow,
    render_api: &mut WrRenderApi,
    image_cache: &ImageCache,
) -> Result<(), &'static str> {
    log_debug!(LogCategory::Rendering, "[build_atomic_txn] Building atomic transaction");

    // Get sizes
    let physical_size = layout_window.current_window_state.size.get_physical_size();
    let framebuffer_size =
        DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
    let viewport_size = framebuffer_size;
    let dpi = layout_window.current_window_state.size.get_hidpi_factor();

    // Get root pipeline ID
    let root_pipeline_id = wr_translate_pipeline_id(PipelineId(0, layout_window.document_id.id));

    // Step 1: Collect and add font resources to transaction
    log_debug!(LogCategory::Rendering, "[build_atomic_txn] Step 1: Collecting font resources");
    let font_updates =
        collect_font_resource_updates(layout_window, &layout_window.renderer_resources, dpi);

    if !font_updates.is_empty() {
        log_debug!(LogCategory::Rendering, 
            "[build_atomic_txn] Adding {} font resources",
            font_updates.len()
        );

        // Update font_hash_map and currently_registered_fonts
        for resource in &font_updates {
            match resource {
                azul_core::resources::ResourceUpdate::AddFont(ref add_font) => {
                    layout_window
                        .renderer_resources
                        .font_hash_map
                        .insert(add_font.font.get_hash(), add_font.key);
                    layout_window
                        .renderer_resources
                        .currently_registered_fonts
                        .entry(add_font.key)
                        .or_insert_with(|| (add_font.font.clone(), BTreeMap::default()));
                    log_debug!(LogCategory::Rendering, 
                        "[build_atomic_txn] Font registered: hash {} -> key {:?}",
                        add_font.font.get_hash(),
                        add_font.key
                    );
                }
                azul_core::resources::ResourceUpdate::AddFontInstance(ref add_font_instance) => {
                    if let Some((_, instances)) = layout_window
                        .renderer_resources
                        .currently_registered_fonts
                        .get_mut(&add_font_instance.font_key)
                    {
                        instances.insert(add_font_instance.glyph_size, add_font_instance.key);
                        log_debug!(LogCategory::Rendering, 
                            "[build_atomic_txn] Font instance registered: key {:?} at size {:?}",
                            add_font_instance.key, add_font_instance.glyph_size
                        );
                    }
                }
                _ => {}
            }
        }

        // Translate to WebRender resources and add to transaction
        let wr_resources: Vec<webrender::ResourceUpdate> = font_updates
            .into_iter()
            .filter_map(|r| translate_resource_update(r))
            .collect();

        if !wr_resources.is_empty() {
            log_debug!(LogCategory::Rendering, 
                "[build_atomic_txn] Adding {} WebRender resources to transaction",
                wr_resources.len()
            );
            txn.update_resources(wr_resources);
        }
    }

    // Step 2: Build and add display lists for all DOMs to transaction
    log_debug!(LogCategory::Rendering, 
        "[build_atomic_txn] Step 2: Building display lists for {} DOMs",
        layout_window.layout_results.len()
    );

    for (dom_id, layout_result) in &layout_window.layout_results {
        let pipeline_id = wr_translate_pipeline_id(PipelineId(
            dom_id.inner as u32,
            layout_window.document_id.id,
        ));

        log_debug!(LogCategory::Rendering, 
            "[build_atomic_txn] Building display list for DOM {}",
            dom_id.inner
        );

        match crate::desktop::compositor2::translate_displaylist_to_wr(
            &layout_result.display_list,
            pipeline_id,
            viewport_size,
            &layout_window.renderer_resources,
            dpi,
            Vec::new(), // Resources already added above
            &layout_window.layout_results,
            layout_window.document_id.id,
        ) {
            Ok((_, built_display_list, nested_pipelines)) => {
                let epoch = webrender::api::Epoch(layout_window.epoch.into_u32());
                log_debug!(LogCategory::Rendering, 
                    "[build_atomic_txn] Adding display list for DOM {} (pipeline {:?}, epoch \
                     {:?}, {} nested)",
                    dom_id.inner,
                    pipeline_id,
                    epoch,
                    nested_pipelines.len()
                );

                // Add main pipeline
                txn.set_display_list(epoch, (pipeline_id, built_display_list));

                // Add all nested iframe pipelines
                for (nested_pipeline_id, nested_display_list) in nested_pipelines {
                    log_debug!(LogCategory::Rendering, 
                        "[build_atomic_txn] Adding nested pipeline {:?} (epoch {:?})",
                        nested_pipeline_id, epoch
                    );
                    txn.set_display_list(epoch, (nested_pipeline_id, nested_display_list));
                }
            }
            Err(e) => {
                log_debug!(LogCategory::Rendering, 
                    "[build_atomic_txn] Error building display list for DOM {}: {}",
                    dom_id.inner, e
                );
                return Err("Failed to build display list");
            }
        }
    }

    // Step 3: Set root pipeline
    log_debug!(LogCategory::Rendering, 
        "[build_atomic_txn] Step 3: Setting root pipeline {:?}",
        root_pipeline_id
    );
    txn.set_root_pipeline(root_pipeline_id);

    // Step 4: Set document view
    let view_rect =
        DeviceIntRect::from_origin_and_size(DeviceIntPoint::new(0, 0), framebuffer_size);
    let hidpi_factor = layout_window.current_window_state.size.get_hidpi_factor();
    log_debug!(LogCategory::Rendering, 
        "[build_atomic_txn] Step 4: Setting document view {:?}, hidpi: {}",
        view_rect, hidpi_factor.inner.get()
    );
    txn.set_document_view(view_rect, DevicePixelScale::new(hidpi_factor.inner.get()));

    // Step 5: Add scroll offsets
    log_debug!(LogCategory::Rendering, "[build_atomic_txn] Step 5: Adding scroll offsets");
    scroll_all_nodes(layout_window, txn);

    // Step 6: Synchronize GPU values
    log_debug!(LogCategory::Rendering, "[build_atomic_txn] Step 6: Synchronizing GPU values");
    synchronize_gpu_values(layout_window, txn);

    // Step 7: Generate frame
    log_debug!(LogCategory::Rendering, "[build_atomic_txn] Step 7: Calling generate_frame");
    txn.generate_frame(0, webrender::api::RenderReasons::empty());

    // Increment epoch for next frame
    layout_window.epoch.increment();

    log_debug!(LogCategory::Rendering, "[build_atomic_txn] Transaction ready to send");
    Ok(())
}

/// Process image callback updates and add UpdateImage resource updates to the transaction.
///
/// This function is called after callbacks have been processed and image_callbacks_changed
/// has been populated. It re-invokes the image callbacks to get new textures and sends
/// UpdateImage resource updates to WebRender without rebuilding the entire display list.
///
/// # Arguments
///
/// * `layout_window` - The layout window with image callback state
/// * `txn` - The WebRender transaction to add updates to
fn process_image_callback_updates(layout_window: &mut LayoutWindow, txn: &mut WrTransaction) {
    use std::collections::BTreeMap;

    use azul_core::{
        resources::{ResourceUpdate, UpdateImageResult},
        FastBTreeSet,
    };

    // NOTE: This function currently doesn't have access to image_callbacks_changed
    // from CallCallbacksResult. In a complete implementation, this would be stored
    // in LayoutWindow and passed through here. For now, we check if there are any
    // pending updates in the gl_texture_cache that were marked dirty.

    // TODO: Pass CallCallbacksResult.image_callbacks_changed to this function
    // For now, this is a no-op until the callback result flow is connected

    log_debug!(LogCategory::Rendering, "[process_image_callback_updates] Checking for pending image callback updates");
}

/// Process IFrame updates requested by callbacks
///
/// This function handles manual IFrame re-rendering triggered by `trigger_iframe_rerender()`.
/// It rebuilds display lists for IFrames that were already re-rendered during layout,
/// then submits only those pipelines to WebRender without rebuilding the entire scene.
///
/// # Architecture
///
/// Each IFrame gets its own WebRender pipeline with a stable PipelineId based on
/// (dom_id, node_id). When an IFrame needs updating:
///
/// 1. The IFrame callback was already re-invoked during the layout phase
/// 2. The layout result for that IFrame's DOM exists in layout_results
/// 3. We just need to rebuild and submit that specific IFrame's display list
/// 4. Other pipelines remain untouched
///
/// This allows efficient updates without full scene rebuilds.
///
/// # Arguments
///
/// * `layout_window` - The layout window with IFrame state
/// * `txn` - The WebRender transaction to add updates to
fn process_iframe_updates(layout_window: &mut LayoutWindow, txn: &mut WrTransaction) {
    // Check if there are any pending IFrame updates
    if layout_window.pending_iframe_updates.is_empty() {
        return;
    }

    log_debug!(LogCategory::Rendering, 
        "[process_iframe_updates] Processing {} pending IFrame updates",
        layout_window.pending_iframe_updates.len()
    );

    use webrender::api::Epoch as WrEpoch;

    let dpi = layout_window.current_window_state.size.get_hidpi_factor();
    let physical_size = layout_window.current_window_state.size.get_physical_size();
    let viewport_size = DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

    // Collect all child DOM IDs that need their display lists rebuilt
    let mut child_dom_ids = Vec::new();

    for (parent_dom_id, node_ids) in &layout_window.pending_iframe_updates {
        for node_id in node_ids {
            if let Some(child_dom_id) = layout_window
                .iframe_manager
                .get_nested_dom_id(*parent_dom_id, *node_id)
            {
                child_dom_ids.push((child_dom_id, *parent_dom_id, *node_id));
            }
        }
    }

    // Clear pending updates
    layout_window.pending_iframe_updates.clear();

    // For each IFrame, rebuild and submit its display list
    for (child_dom_id, parent_dom_id, node_id) in child_dom_ids {
        // Get the layout result for the IFrame's content
        let layout_result = match layout_window.layout_results.get(&child_dom_id) {
            Some(lr) => lr,
            None => {
                log_debug!(LogCategory::Rendering, 
                    "[process_iframe_updates] No layout result for child DOM {:?} (parent {:?}, \
                     node {:?})",
                    child_dom_id, parent_dom_id, node_id
                );
                continue;
            }
        };

        // Build the pipeline ID for this IFrame
        let pipeline_id = wr_translate_pipeline_id(PipelineId(
            child_dom_id.inner as u32,
            layout_window.document_id.id,
        ));

        // Translate display list to WebRender
        match crate::desktop::compositor2::translate_displaylist_to_wr(
            &layout_result.display_list,
            pipeline_id,
            viewport_size,
            &layout_window.renderer_resources,
            dpi,
            Vec::new(), // Resources should already be registered
            &layout_window.layout_results,
            layout_window.document_id.id,
        ) {
            Ok((_, built_display_list, nested_pipelines)) => {
                log_debug!(LogCategory::Rendering, 
                    "[process_iframe_updates] Submitting display list for IFrame DOM {} (pipeline \
                     {:?})",
                    child_dom_id.inner, pipeline_id
                );

                // Submit the updated display list
                txn.set_display_list(
                    WrEpoch(layout_window.epoch.into_u32()),
                    (pipeline_id, built_display_list),
                );

                // Submit any nested pipelines (IFrames within IFrames)
                for (nested_pipeline_id, nested_display_list) in nested_pipelines {
                    log_debug!(LogCategory::Rendering, 
                        "[process_iframe_updates] Submitting nested pipeline {:?}",
                        nested_pipeline_id
                    );
                    txn.set_display_list(
                        WrEpoch(layout_window.epoch.into_u32()),
                        (nested_pipeline_id, nested_display_list),
                    );
                }
            }
            Err(e) => {
                log_debug!(LogCategory::Rendering, 
                    "[process_iframe_updates] Error building display list for IFrame DOM {}: {}",
                    child_dom_id.inner, e
                );
            }
        }
    }
}
