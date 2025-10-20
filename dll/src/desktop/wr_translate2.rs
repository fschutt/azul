//! WebRender type translation functions for shell2
//!
//! This module provides translations between azul-core types and WebRender types,
//! plus hit-testing integration. Simplified version of wr_translate.rs for shell2.

use alloc::sync::Arc;
use core::mem;
use std::{cell::RefCell, rc::Rc};

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    geom::LogicalPosition,
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
    RendererOptions as WrRendererOptions,
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
        _render_time: Option<u64>,
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
        force_subpixel_aa: true,
        clear_color: WrColorF {
            r: options.state.background_color.r as f32 / 255.0,
            g: options.state.background_color.g as f32 / 255.0,
            b: options.state.background_color.b as f32 / 255.0,
            a: options.state.background_color.a as f32 / 255.0,
        },
        enable_multithreading: true,
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
        _rendering: webrender::api::ImageRendering,
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

/// Perform WebRender-based hit testing
///
/// This is the main hit-testing function that uses WebRender's hit tester to determine
/// which DOM nodes are under the cursor. It handles nested iframes and builds a complete
/// hit test result with all hovered nodes.
pub fn fullhittest_new_webrender(
    wr_hittester: &dyn WrApiHitTester,
    document_id: DocumentId,
    old_focus_node: Option<DomNodeId>,
    layout_results: &[&DomLayoutResult],
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

            let layout_result = match layout_results.get(dom_id.inner) {
                Some(s) => s,
                None => break,
            };

            // Perform WebRender hit test at cursor position
            let wr_result = wr_hittester.hit_test(
                Some(wr_translate_pipeline_id(pipeline_id)),
                WrWorldPoint::new(
                    cursor_relative_to_dom.x * hidpi_factor,
                    cursor_relative_to_dom.y * hidpi_factor,
                ),
            );

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
                use azul_core::hit_test::HitTest;

                // If this is an iframe, queue it for next iteration
                if let Some(i) = item.is_iframe_hit.as_ref() {
                    new_dom_ids.push(*i);
                }

                // Update focused node if this item is focusable
                if item.is_focusable {
                    ret.focused_node = Some((*dom_id, node_id));
                }

                let az_node_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));

                // Always insert into regular_hit_test_nodes
                ret.hovered_nodes
                    .entry(*dom_id)
                    .or_insert_with(|| HitTest::empty())
                    .regular_hit_test_nodes
                    .insert(node_id, item);

                // TODO: Re-enable scroll hit testing once scrollable_nodes is available
                // The new layout system may store this differently
                /*
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
                */
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
