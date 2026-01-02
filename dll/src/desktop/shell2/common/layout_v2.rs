//! Cross-platform layout regeneration logic
//!
//! This module contains the unified layout regeneration workflow that is shared across all
//! platforms. Previously, this logic was duplicated in every platform's window implementation.
//!
//! The regenerate_layout function takes direct field references instead of using trait methods
//! to avoid borrow checker issues (similar to invoke_callbacks pattern).

use std::{cell::RefCell, sync::Arc};

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, LayoutCallbackInfoRefData},
    gl::OptionGlContextPtr,
    hit_test::DocumentId,
    refany::RefAny,
    resources::{ImageCache, RendererResources},
};
use azul_css::system::SystemStyle;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;
use webrender::{RenderApi as WrRenderApi, Transaction as WrTransaction};

use crate::{desktop::{csd, wr_translate2}, log_debug};
use super::debug_server::{self, LogCategory};
use azul_css::LayoutDebugMessage;

/// Regenerate layout after DOM changes.
///
/// This function implements the complete layout regeneration workflow:
/// 1. Invoke user's layout callback to get new DOM
/// 2. Conditionally inject Client-Side Decorations (CSD)
/// 3. Perform layout and generate display list
/// 4. Calculate scrollbar states
/// 5. Rebuild WebRender display list
/// 6. Synchronize scrollbar opacity with GPU cache
///
/// This workflow is identical across all platforms (macOS, Windows, X11, Wayland).
///
/// ## Parameters
///
/// Takes direct references to window fields to avoid borrow checker issues.
/// This is the same pattern used in `invoke_single_callback`.
///
/// ## Return Value
///
/// Returns `Ok(())` on success, or an error message on failure.
pub fn regenerate_layout(
    layout_window: &mut LayoutWindow,
    app_data: &Arc<RefCell<RefAny>>,
    current_window_state: &FullWindowState,
    renderer_resources: &mut RendererResources,
    render_api: &mut WrRenderApi,
    image_cache: &ImageCache,
    gl_context_ptr: &OptionGlContextPtr,
    fc_cache: &Arc<FcFontCache>,
    system_style: &Arc<SystemStyle>,
    document_id: DocumentId,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<(), String> {
    log_debug!(LogCategory::Layout, "[regenerate_layout] START");

    // Update layout_window's fc_cache with the shared one
    layout_window.font_manager.fc_cache = fc_cache.clone();

    // 1. Call user's layout callback to get new DOM
    log_debug!(LogCategory::Layout, "[regenerate_layout] Calling layout_callback");

    // Create reference data container (syntax sugar to reduce parameter count)
    let layout_ref_data = LayoutCallbackInfoRefData {
        image_cache,
        gl_context: gl_context_ptr,
        system_fonts: &*fc_cache,
        system_style: system_style.clone(),
    };

    let callback_info = LayoutCallbackInfo::new(
        &layout_ref_data,
        current_window_state.size.clone(),
        current_window_state.theme,
    );

    let app_data_borrowed = app_data.borrow_mut();

    let user_styled_dom =
        (current_window_state.layout_callback.cb)((*app_data_borrowed).clone(), callback_info);

    drop(app_data_borrowed); // Release borrow

    // 2. Conditionally inject Client-Side Decorations (CSD)
    let styled_dom = if csd::should_inject_csd(
        current_window_state.flags.has_decorations,
        current_window_state.flags.decorations,
    ) {
        log_debug!(LogCategory::Layout, "[regenerate_layout] Injecting CSD decorations");
        csd::wrap_user_dom_with_decorations(
            user_styled_dom,
            &current_window_state.title,
            true,         // inject titlebar
            true,         // has minimize
            true,         // has maximize
            system_style, // pass SystemStyle for native look
        )
    } else {
        user_styled_dom
    };

    log_debug!(LogCategory::Layout, "[regenerate_layout] StyledDom: {} nodes, {} hierarchy", styled_dom.styled_nodes.len(), styled_dom.node_hierarchy.len());

    // 3. Perform layout with solver3
    log_debug!(LogCategory::Layout, "[regenerate_layout] Calling layout_and_generate_display_list");
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            current_window_state,
            renderer_resources,
            &ExternalSystemCallbacks::rust_internal(),
            debug_messages,
        )
        .map_err(|e| format!("Layout error: {:?}", e))?;

    log_debug!(LogCategory::Layout, "[regenerate_layout] Layout completed, {} DOMs", layout_window.layout_results.len());

    // 4. Register scrollable nodes with scroll_manager
    // This must happen AFTER layout but BEFORE calculate_scrollbar_states
    let now: azul_core::task::Instant = std::time::Instant::now().into();
    for (dom_id, layout_result) in &layout_window.layout_results {
        for (_node_idx, node) in layout_result.layout_tree.nodes.iter().enumerate() {
            // Check if this node needs scrollbars (has scrollbar_info with needs_v or needs_h)
            if let Some(ref scrollbar_info) = node.scrollbar_info {
                if scrollbar_info.needs_vertical || scrollbar_info.needs_horizontal {
                    if let Some(dom_node_id) = node.dom_node_id {
                        // Get container size from used_size
                        let container_size = node.used_size.unwrap_or_default();
                        let container_rect = azul_core::geom::LogicalRect {
                            origin: azul_core::geom::LogicalPosition::zero(),
                            size: container_size,
                        };
                        
                        // Get content size using the node's method
                        let content_size = node.get_content_size();
                        
                        layout_window.scroll_manager.register_or_update_scroll_node(
                            *dom_id,
                            dom_node_id,
                            container_rect,
                            content_size,
                            now.clone(),
                        );
                        
                        log_debug!(LogCategory::Layout, 
                            "[regenerate_layout] Registered scroll node: dom={:?} node={:?} container={:?} content={:?}",
                            dom_id, dom_node_id, container_size, content_size);
                    }
                }
            }
        }
    }

    // 5. Calculate scrollbar states based on new layout
    // This updates scrollbar geometry (thumb position/size ratios, visibility)
    layout_window.scroll_manager.calculate_scrollbar_states();

    // 6. Synchronize scrollbar opacity with GPU cache
    // Note: Display list translation happens in generate_frame(), not here
    // This enables smooth fade-in/fade-out without display list rebuild
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    for (dom_id, layout_result) in &layout_window.layout_results {
        LayoutWindow::synchronize_scrollbar_opacity(
            &mut layout_window.gpu_state_manager,
            &layout_window.scroll_manager,
            *dom_id,
            &layout_result.layout_tree,
            &system_callbacks,
            azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(500)), /* fade_delay */
            azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(200)), /* fade_duration */
        );
    }

    log_debug!(LogCategory::Layout, "[regenerate_layout] COMPLETE");

    Ok(())
}

/// Helper function to generate WebRender frame
///
/// This should be called after regenerate_layout to submit the frame to WebRender.
/// Usually called once at the end of event processing.
pub fn generate_frame(
    layout_window: &mut LayoutWindow,
    render_api: &mut WrRenderApi,
    document_id: DocumentId,
) {
    // Process any pending IFrame updates requested by callbacks
    // This must happen BEFORE wr_translate2::generate_frame() so that the IFrame
    // callbacks can be re-invoked and their layout results are available
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let current_window_state = layout_window.current_window_state.clone();

    // Need to use unsafe pointer cast to work around borrow checker
    // This is safe because process_pending_iframe_updates doesn't modify renderer_resources
    let renderer_resources_ptr = &layout_window.renderer_resources as *const _;
    layout_window.process_pending_iframe_updates(
        &current_window_state,
        unsafe { &*renderer_resources_ptr },
        &system_callbacks,
    );

    let mut txn = WrTransaction::new();

    // Display list was rebuilt
    wr_translate2::generate_frame(&mut txn, layout_window, render_api, true);

    render_api.send_transaction(wr_translate2::wr_translate_document_id(document_id), txn);
}
