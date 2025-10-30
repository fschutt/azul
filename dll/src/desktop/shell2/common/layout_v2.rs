//! Cross-platform layout regeneration logic
//!
//! This module contains the unified layout regeneration workflow that is shared across all
//! platforms. Previously, this logic was duplicated in every platform's window implementation.

use alloc::sync::Arc;

use azul_core::{
    callbacks::LayoutCallbackInfo,
    resources::{ImageCache, RendererResources},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

use super::event_v2::PlatformWindowV2;
use crate::desktop::{csd, wr_translate2};

/// Regenerate layout after DOM changes.
///
/// This function implements the complete layout regeneration workflow:
/// 1. Invoke user's layout callback to get new DOM
/// 2. Conditionally inject Client-Side Decorations (CSD)
/// 3. Perform layout and generate display list
/// 4. Calculate scrollbar states
/// 5. Rebuild WebRender display list
/// 6. Synchronize scrollbar opacity with GPU cache
/// 7. Mark frame for regeneration
///
/// This workflow is identical across all platforms (macOS, Windows, X11, Wayland).
pub fn regenerate_layout<W: PlatformWindowV2>(
    window: &mut W,
    render_api: &mut webrender::RenderApi,
    document_id: webrender::api::DocumentId,
) -> Result<(), String> {
    let layout_window = match window.get_layout_window_mut() {
        Some(lw) => lw,
        None => return Err("No layout window".into()),
    };

    // 1. Call user's layout callback to get new DOM
    let layout_callback = window.get_current_window_state().layout_callback.clone();
    let image_cache = window.get_image_cache_mut();
    let renderer_resources = window.get_renderer_resources_mut();
    
    let mut callback_info = LayoutCallbackInfo::new(
        window.get_current_window_state().size,
        window.get_current_window_state().theme,
        image_cache,
        window.get_gl_context_ptr(),
        &*window.get_fc_cache(),
        window.get_system_style().clone(),
    );

    let user_dom = match &layout_callback {
        azul_core::callbacks::LayoutCallback::Raw(inner) => {
            (inner.cb)(&mut window.get_app_data().borrow_mut(), &mut callback_info)
        }
        azul_core::callbacks::LayoutCallback::Marshaled(marshaled) => {
            (marshaled.cb.cb)(
                &mut marshaled.marshal_data.clone(),
                &mut window.get_app_data().borrow_mut(),
                &mut callback_info,
            )
        }
    };

    // 2. Conditionally inject Client-Side Decorations (CSD)
    let current_state = window.get_current_window_state();
    let should_inject_csd = csd::should_inject_csd(
        current_state.flags.has_decorations,
        current_state.flags.decorations,
    );
    let has_minimize = true;
    let has_maximize = true;

    let final_dom = if should_inject_csd {
        csd::wrap_user_dom_with_decorations(
            user_dom,
            &current_state.title.as_str(),
            should_inject_csd,
            has_minimize,
            has_maximize,
            window.get_system_style(),
        )
    } else {
        user_dom
    };

    // Get renderer_resources again (borrow checker)
    let renderer_resources = window.get_renderer_resources_mut();

    // 3. Perform layout with LayoutWindow
    layout_window
        .layout_and_generate_display_list(
            final_dom,
            window.get_current_window_state(),
            renderer_resources,
            &ExternalSystemCallbacks::rust_internal(),
            &mut None, // debug_messages
        )
        .map_err(|e| format!("Layout failed: {:?}", e))?;

    // 4. Calculate scrollbar states based on new layout
    layout_window.scroll_states.calculate_scrollbar_states();

    // 5. Rebuild display list and send to WebRender
    let dpi_factor = window.get_current_window_state().size.get_hidpi_factor();
    let mut txn = webrender::Transaction::new();
    
    let image_cache = window.get_image_cache_mut();
    let renderer_resources = window.get_renderer_resources_mut();
    
    wr_translate2::rebuild_display_list(
        &mut txn,
        layout_window,
        render_api,
        image_cache,
        alloc::vec::Vec::new(),
        renderer_resources,
        dpi_factor,
    );

    // Send transaction
    render_api.send_transaction(document_id, txn);

    // 6. Synchronize scrollbar opacity with GPU cache
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    for (dom_id, layout_result) in &layout_window.layout_results {
        LayoutWindow::synchronize_scrollbar_opacity(
            &mut layout_window.gpu_state_manager,
            &layout_window.scroll_states,
            *dom_id,
            &layout_result.layout_tree,
            &system_callbacks,
            azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(500)), // fade_delay
            azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(200)), // fade_duration
        );
    }

    // 7. Mark frame needs regeneration
    window.mark_frame_needs_regeneration();

    Ok(())
}
