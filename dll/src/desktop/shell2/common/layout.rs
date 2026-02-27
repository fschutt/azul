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
    icon::SharedIconProvider,
    refany::RefAny,
    resources::{ImageCache, RendererResources},
};
use azul_css::system::SystemStyle;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;
use webrender::{RenderApi as WrRenderApi, Transaction as WrTransaction};

use super::debug_server::{self, LogCategory};
use crate::{
    desktop::{csd, wr_translate2},
    log_debug,
};
use azul_css::LayoutDebugMessage;

/// Result of `regenerate_layout()` indicating whether the DOM structure changed.
///
/// When the DOM is structurally unchanged (same node types, hierarchy, classes,
/// IDs, inline styles, callback events), the expensive layout pipeline
/// (CSS cascade, flexbox, display list) can be skipped. Only image callbacks
/// need to be re-invoked since their content (e.g. GL textures) may have changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutRegenerateResult {
    /// DOM structure changed — full layout was performed.
    LayoutChanged,
    /// DOM structure is unchanged — layout was reused from previous frame.
    /// Image callbacks still need to be re-invoked.
    LayoutUnchanged,
}

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
/// Returns `Ok(LayoutChanged)` if full layout was performed,
/// `Ok(LayoutUnchanged)` if the DOM was structurally unchanged and layout was reused,
/// or an error message on failure.
pub fn regenerate_layout(
    layout_window: &mut LayoutWindow,
    app_data: &Arc<RefCell<RefAny>>,
    current_window_state: &FullWindowState,
    renderer_resources: &mut RendererResources,
    image_cache: &ImageCache,
    gl_context_ptr: &OptionGlContextPtr,
    fc_cache: &Arc<FcFontCache>,
    font_registry: &Option<Arc<FcFontRegistry>>,
    system_style: &Arc<SystemStyle>,
    icon_provider: &SharedIconProvider,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<LayoutRegenerateResult, String> {
    log_debug!(LogCategory::Layout, "[regenerate_layout] START");

    // If the async font registry is available, request commonly-used fonts
    // and block until they are ready (eliminates FOUC). On cache hits this
    // is effectively free; on first run it blocks until the Scout + Builder
    // threads have parsed the needed fonts.
    if let Some(registry) = font_registry.as_ref() {
        // Avoid replacing a complete font cache (e.g. loaded from disk cache at
        // startup) with an incomplete snapshot while background builder threads
        // are still parsing fonts.  This prevents a race condition where only
        // some variants of a font family (e.g. only the Italic variant of
        // "System Font") are available when the snapshot is taken, causing
        // incorrect font selection on some launches.
        let current_cache_empty = layout_window.font_manager.fc_cache.is_empty();
        let build_complete = registry.is_build_complete();

        if current_cache_empty || build_complete {
            log_debug!(LogCategory::Layout, "[regenerate_layout] Requesting fonts from registry...");
            let common_families = rust_fontconfig::registry::get_common_font_families();
            let font_stacks: Vec<Vec<String>> = common_families.into_iter().map(|f| vec![f]).collect();
            registry.request_fonts(&font_stacks);
            // Snapshot the registry into an FcFontCache for use during layout
            let snapshot = Arc::new(registry.into_fc_font_cache());
            layout_window.font_manager.fc_cache = snapshot.clone();
            log_debug!(LogCategory::Layout, "[regenerate_layout] Font registry snapshot complete");
        } else {
            log_debug!(LogCategory::Layout, "[regenerate_layout] Using existing font cache (build still in progress, {} faces loaded)", registry.progress().2);
        }
    } else {
        // Fallback: use the provided fc_cache directly
        layout_window.font_manager.fc_cache = fc_cache.clone();
    }

    // 1. Call user's layout callback to get new DOM
    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] Calling layout_callback"
    );

    // Create reference data container (syntax sugar to reduce parameter count)
    let layout_ref_data = LayoutCallbackInfoRefData {
        image_cache,
        gl_context: gl_context_ptr,
        system_fonts: &*layout_window.font_manager.fc_cache,
        system_style: system_style.clone(),
    };

    let callback_info = LayoutCallbackInfo::new(
        &layout_ref_data,
        current_window_state.size.clone(),
        current_window_state.theme,
    );

    let app_data_borrowed = app_data.borrow_mut();

    let mut user_styled_dom =
        (current_window_state.layout_callback.cb)((*app_data_borrowed).clone(), callback_info);

    drop(app_data_borrowed); // Release borrow

    // 2. Resolve icon nodes to their actual content (text glyphs, images, etc.)
    // This must happen after the user's layout callback and before CSD injection
    azul_core::icon::resolve_icons_in_styled_dom(&mut user_styled_dom, icon_provider, system_style);

    // 3. Conditionally inject Client-Side Decorations (CSD)
    //
    // IMPORTANT: CSD injection MUST happen BEFORE state migration (step 3.5)
    // and manager updates (step 3.6). The old layout_result.styled_dom contains
    // the full DOM *with* the titlebar from the previous frame. If we reconcile
    // old-DOM-with-titlebar vs new-DOM-without-titlebar, the node_moves will be
    // wrong (all user NodeIds would be off by the titlebar node count). By
    // injecting the titlebar first, both old and new DOMs have matching structure
    // and reconciliation produces correct node mappings.
    let mut styled_dom = if csd::should_inject_csd(
        current_window_state.flags.has_decorations,
        current_window_state.flags.decorations,
    ) {
        log_debug!(
            LogCategory::Layout,
            "[regenerate_layout] Injecting CSD decorations"
        );
        csd::wrap_user_dom_with_decorations(
            user_styled_dom,
            &current_window_state.title,
            true,         // inject titlebar
            system_style, // pass SystemStyle for native look
        )
    } else if current_window_state.flags.decorations
        == azul_core::window::WindowDecorations::NoTitleAutoInject
    {
        // Auto-inject a Titlebar at the top of the user's DOM.
        // The titlebar is a regular layout widget with DragStart/Drag/DoubleClick
        // callbacks — no special event-system hooks required.
        log_debug!(
            LogCategory::Layout,
            "[regenerate_layout] Auto-injecting Titlebar (NoTitleAutoInject)"
        );
        inject_software_titlebar(
            user_styled_dom,
            &current_window_state.title,
            system_style,
        )
    } else {
        user_styled_dom
    };

    // 3.5. STATE MIGRATION: Transfer heavy resources from old DOM to new DOM
    // This allows components like video players to preserve their decoder handles
    // across frame updates without polluting the application data model.
    //
    // ALSO: Update FocusManager, ScrollManager, etc. with new NodeIds!
    // The node_moves tell us: old NodeId X is now new NodeId Y
    //
    // NOTE: This runs AFTER CSD injection so that both old and new DOMs have
    // matching structure (both include titlebar nodes). This ensures reconciliation
    // produces correct node mappings and manager NodeIds are not invalidated by
    // a subsequent titlebar injection shifting all indices.
    if let Some(old_layout_result) = layout_window.layout_results.get(&azul_core::dom::DomId::ROOT_ID) {
        // Get old node data (from previous frame — includes titlebar if it was injected)
        let old_node_data_vec = &old_layout_result.styled_dom.node_data;
        let old_node_data: Vec<azul_core::dom::NodeData> = old_node_data_vec.as_ref().to_vec();

        // Get new node data (from current frame — now also includes titlebar)
        let mut new_node_data: Vec<azul_core::dom::NodeData> = styled_dom.node_data.as_ref().to_vec();

        // Build layout maps for reconciliation (empty for now - we just need node moves)
        let old_layout_map = azul_core::FastHashMap::default();
        let new_layout_map = azul_core::FastHashMap::default();

        // Run reconciliation to find matched nodes
        let diff_result = azul_core::diff::reconcile_dom(
            &old_node_data,
            &new_node_data,
            &old_layout_map,
            &new_layout_map,
            azul_core::dom::DomId::ROOT_ID,
            azul_core::task::Instant::now(),
        );

        // Execute state migration for matched nodes with merge callbacks
        if !diff_result.node_moves.is_empty() {
            let mut old_node_data_mut = old_node_data;
            azul_core::diff::transfer_states(
                &mut old_node_data_mut,
                &mut new_node_data,
                &diff_result.node_moves,
            );

            // Update the styled_dom with the merged node data
            styled_dom.node_data = new_node_data.into();

            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] State migration: {} node moves processed",
                diff_result.node_moves.len()
            );
        }

        // 3.6. UPDATE MANAGERS WITH NEW NODE IDS
        // The node_moves tell us which old NodeIds map to which new NodeIds.
        // We need to update FocusManager, ScrollManager, etc. so they point to
        // the correct nodes in the new DOM.
        update_managers_with_node_moves(
            layout_window,
            &diff_result.node_moves,
            azul_core::dom::DomId::ROOT_ID,
        );
    }

    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] StyledDom: {} nodes, {} hierarchy",
        styled_dom.styled_nodes.len(),
        styled_dom.node_hierarchy.len()
    );

    // 3.5 CRITICAL: Apply focus/hover/active states BEFORE layout
    // The layout callback creates a fresh StyledDom with default states (focused=false, etc.)
    // We need to synchronize the StyledNodeState with the current runtime state
    // (FocusManager.focused_node, mouse hover position, etc.) BEFORE the display list is generated
    let styled_dom = apply_runtime_states_before_layout(
        styled_dom,
        layout_window,
        current_window_state,
    );

    // 3.7 OPTIMIZATION: Check if the new DOM is structurally identical to the old DOM.
    // If so, we can skip the expensive layout pipeline (CSS cascade, flexbox, display list)
    // and reuse the layout from the previous frame. Only image callbacks need re-invocation
    // since their content (e.g. GL textures) may have changed.
    //
    // IMPORTANT: We must NOT skip layout when the window size changed, even if the DOM
    // structure is identical. Flexbox positions/sizes depend on the viewport dimensions,
    // so a resize invalidates all computed positions. Without this check, image callbacks
    // would receive stale bounds after a window resize.
    let window_size_changed = {
        let old_dims = layout_window.current_window_state.size.dimensions;
        let new_dims = current_window_state.size.dimensions;
        (old_dims.width - new_dims.width).abs() > 0.5
            || (old_dims.height - new_dims.height).abs() > 0.5
    };
    if !window_size_changed {
    if let Some(old_layout_result) = layout_window.layout_results.get(&azul_core::dom::DomId::ROOT_ID) {
        if azul_core::styled_dom::is_layout_equivalent(&old_layout_result.styled_dom, &styled_dom) {
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] DOM unchanged - skipping layout, will only refresh image callbacks"
            );

            // Transfer the new image callback RefAnys to the old DOM's nodes.
            // The old layout result keeps all its positions/sizes/display list data,
            // but the image callback data needs to be updated so that re-invocation
            // uses the freshly-created RefAny (which may reference new app state).
            let old_node_data = old_layout_result.styled_dom.node_data.as_ref();
            let new_node_data = styled_dom.node_data.as_ref();
            // Collect updates first to avoid borrow issues
            let mut image_updates: Vec<(usize, azul_core::callbacks::CoreImageCallback)> = Vec::new();
            for (idx, (old_nd, new_nd)) in old_node_data.iter().zip(new_node_data.iter()).enumerate() {
                if let (
                    azul_core::dom::NodeType::Image(ref _old_img),
                    azul_core::dom::NodeType::Image(ref new_img),
                ) = (&old_nd.node_type, &new_nd.node_type) {
                    if let azul_core::resources::DecodedImage::Callback(new_cb) = new_img.get_data() {
                        image_updates.push((idx, new_cb.clone()));
                    }
                }
            }

            // Now apply image callback updates to old DOM's node data
            if !image_updates.is_empty() {
                let old_layout_result_mut = layout_window.layout_results.get_mut(&azul_core::dom::DomId::ROOT_ID).unwrap();
                let old_node_data_mut = old_layout_result_mut.styled_dom.node_data.as_mut();
                for (idx, new_cb) in image_updates {
                    if let Some(old_nd) = old_node_data_mut.get_mut(idx) {
                        old_nd.node_type = azul_core::dom::NodeType::Image(
                            azul_core::resources::ImageRef::callback(new_cb.callback.clone(), new_cb.refany.clone())
                        );
                    }
                }
            }

            // Also transfer any updated callback data (RefAny) for event callbacks
            // so that future events use fresh app state references
            let mut callback_updates: Vec<(usize, azul_core::callbacks::CoreCallbackDataVec)> = Vec::new();
            {
                let old_nd_ref = layout_window.layout_results.get(&azul_core::dom::DomId::ROOT_ID).unwrap().styled_dom.node_data.as_ref();
                let new_nd_ref = styled_dom.node_data.as_ref();
                for (idx, (_old_nd, new_nd)) in old_nd_ref.iter().zip(new_nd_ref.iter()).enumerate() {
                    if !new_nd.callbacks.as_ref().is_empty() {
                        callback_updates.push((idx, new_nd.callbacks.clone()));
                    }
                }
            }
            if !callback_updates.is_empty() {
                let old_layout_result_mut = layout_window.layout_results.get_mut(&azul_core::dom::DomId::ROOT_ID).unwrap();
                let old_node_data_mut = old_layout_result_mut.styled_dom.node_data.as_mut();
                for (idx, new_callbacks) in callback_updates {
                    if let Some(old_nd) = old_node_data_mut.get_mut(idx) {
                        old_nd.callbacks = new_callbacks;
                    }
                }
            }

            log_debug!(LogCategory::Layout, "[regenerate_layout] COMPLETE (layout unchanged)");
            return Ok(LayoutRegenerateResult::LayoutUnchanged);
        }
    }
    } // end if !window_size_changed

    // 4. Perform layout with solver3
    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] Calling layout_and_generate_display_list"
    );
    
    // Update system style for resolving system color keywords (selection colors, accent, etc.)
    layout_window.set_system_style(system_style.clone());
    
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            current_window_state,
            renderer_resources,
            &ExternalSystemCallbacks::rust_internal(),
            debug_messages,
        )
        .map_err(|e| format!("Layout error: {:?}", e))?;

    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] Layout completed, {} DOMs",
        layout_window.layout_results.len()
    );

    // 5. Register scrollable nodes with scroll_manager
    // This must happen AFTER layout but BEFORE calculate_scrollbar_states
    let now: azul_core::task::Instant = std::time::Instant::now().into();
    for (dom_id, layout_result) in &layout_window.layout_results {
        for (node_idx, node) in layout_result.layout_tree.nodes.iter().enumerate() {
            // Check if this node needs scrollbars (has scrollbar_info with needs_v or needs_h)
            if let Some(ref scrollbar_info) = node.scrollbar_info {
                if scrollbar_info.needs_vertical || scrollbar_info.needs_horizontal {
                    if let Some(dom_node_id) = node.dom_node_id {
                        // Get container size from used_size (border-box)
                        // Convert to content-box by subtracting padding and border
                        // This is critical: content_size is content-box, so container must match
                        let border_box_size = node.used_size.unwrap_or_default();
                        let padding = &node.box_props.padding;
                        let border = &node.box_props.border;
                        let container_size = azul_core::geom::LogicalSize {
                            width: (border_box_size.width 
                                    - padding.left - padding.right
                                    - border.left - border.right).max(0.0),
                            height: (border_box_size.height 
                                     - padding.top - padding.bottom
                                     - border.top - border.bottom).max(0.0),
                        };
                        
                        // Get absolute position from calculated_positions map
                        // IMPORTANT: container_rect must use absolute window coordinates,
                        // not (0,0), so scroll_into_view calculations are correct.
                        // All rects in the scroll system use absolute window coordinates
                        // in logical pixels.
                        let container_origin = layout_result
                            .calculated_positions
                    .get(node_idx)
                            .copied()
                            .unwrap_or_else(azul_core::geom::LogicalPosition::zero);
                        
                        let container_rect = azul_core::geom::LogicalRect {
                            origin: container_origin,
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
                            16.0, // default scrollbar rendering thickness
                            scrollbar_info.needs_horizontal,
                            scrollbar_info.needs_vertical,
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

    Ok(LayoutRegenerateResult::LayoutChanged)
}

/// Incremental relayout: re-run layout on the existing StyledDom without
/// calling the user's `layout_callback()`.
///
/// This is the fast path for restyle-driven changes (hover/focus CSS changes,
/// runtime `set_css_property()`, `set_node_text()`) where the DOM structure
/// hasn't changed — only styles or text content.
///
/// The StyledDom already has updated `styled_node_state` (from `restyle_on_state_change`)
/// or updated node data (from runtime edits). We just need to re-run layout
/// and regenerate the display list.
///
/// ## When to use
///
/// - `ProcessEventResult::ShouldIncrementalRelayout`
/// - After `apply_focus_restyle` detects layout-affecting CSS changes
/// - After `words_changed` / `css_properties_changed` from callbacks
///
/// ## What it skips
///
/// - User's `layout_callback()` (DOM is unchanged)
/// - CSD injection (already done)
/// - State migration / reconciliation (NodeIds haven't changed)
/// - Manager remapping (NodeIds haven't changed)
pub fn incremental_relayout(
    layout_window: &mut LayoutWindow,
    current_window_state: &FullWindowState,
    renderer_resources: &mut RendererResources,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<(), String> {
    log_debug!(LogCategory::Layout, "[incremental_relayout] START");

    let system_callbacks = ExternalSystemCallbacks::rust_internal();

    // Re-run layout on the existing StyledDom with dirty flags already set.
    // The StyledDom in the layout_result already has updated styles/states.
    if let Some(layout_result) = layout_window.layout_results.get(&azul_core::dom::DomId::ROOT_ID) {
        let styled_dom = layout_result.styled_dom.clone();

        layout_window
            .layout_and_generate_display_list(
                styled_dom,
                current_window_state,
                renderer_resources,
                &system_callbacks,
                debug_messages,
            )
            .map_err(|e| format!("Incremental layout error: {:?}", e))?;
    }

    // Re-register scrollable nodes
    let now: azul_core::task::Instant = std::time::Instant::now().into();
    for (_dom_id, layout_result) in &layout_window.layout_results {
        for (node_idx, node) in layout_result.layout_tree.nodes.iter().enumerate() {
            if let Some(ref scrollbar_info) = node.scrollbar_info {
                if scrollbar_info.needs_vertical || scrollbar_info.needs_horizontal {
                    if let Some(dom_node_id) = node.dom_node_id {
                        let border_box_size = node.used_size.unwrap_or_default();
                        let padding = &node.box_props.padding;
                        let border = &node.box_props.border;
                        let container_size = azul_core::geom::LogicalSize {
                            width: (border_box_size.width
                                    - padding.left - padding.right
                                    - border.left - border.right).max(0.0),
                            height: (border_box_size.height
                                     - padding.top - padding.bottom
                                     - border.top - border.bottom).max(0.0),
                        };
                        let container_origin = layout_result
                            .calculated_positions
                            .get(node_idx)
                            .copied()
                            .unwrap_or_else(azul_core::geom::LogicalPosition::zero);
                        let container_rect = azul_core::geom::LogicalRect {
                            origin: container_origin,
                            size: container_size,
                        };
                        let content_size = node.get_content_size();
                        layout_window.scroll_manager.register_or_update_scroll_node(
                            *_dom_id,
                            dom_node_id,
                            container_rect,
                            content_size,
                            now.clone(),
                            16.0, // default scrollbar rendering thickness
                            scrollbar_info.needs_horizontal,
                            scrollbar_info.needs_vertical,
                        );
                    }
                }
            }
        }
    }

    layout_window.scroll_manager.calculate_scrollbar_states();

    log_debug!(LogCategory::Layout, "[incremental_relayout] COMPLETE");

    Ok(())
}

/// Apply runtime states (focus, hover, active) to the StyledDom BEFORE layout
///
/// The layout callback creates a fresh StyledDom where all StyledNodeState fields
/// are set to their defaults (focused=false, hover=false, active=false).
/// This function synchronizes those states with the current runtime state from
/// the various managers (FocusManager, mouse state, etc.) BEFORE the display list
/// is generated.
///
/// This is critical for `:focus`, `:hover`, `:active` CSS pseudo-class styling
/// to work correctly - the display list generation reads these states to determine
/// which CSS properties to apply.
fn apply_runtime_states_before_layout(
    mut styled_dom: azul_core::styled_dom::StyledDom,
    layout_window: &LayoutWindow,
    current_window_state: &FullWindowState,
) -> azul_core::styled_dom::StyledDom {
    use azul_core::dom::DomId;
    
    // The styled_dom is the ROOT_ID DOM (after CSD injection)
    let dom_id = DomId::ROOT_ID;
    
    // 1. Apply focus state
    if let Some(focused_node) = layout_window.focus_manager.get_focused_node() {
        // Only apply if the focused node is in the same DOM we're processing
        if focused_node.dom == dom_id {
            if let Some(node_id) = focused_node.node.into_crate_internal() {
                let mut styled_nodes = styled_dom.styled_nodes.as_container_mut();
                if let Some(styled_node) = styled_nodes.get_mut(node_id) {
                    styled_node.styled_node_state.focused = true;
                    log_debug!(
                        LogCategory::Layout,
                        "[apply_runtime_states_before_layout] Set focused=true for node {:?}",
                        node_id
                    );
                }
            }
        }
    }
    
    // 2. Apply hover state based on hover manager
    if let Some(last_hit_test) = layout_window.hover_manager.get_current_mouse() {
        if let Some(hit_test) = last_hit_test.hovered_nodes.get(&dom_id) {
            let mut styled_nodes = styled_dom.styled_nodes.as_container_mut();
            for (node_id, _hit_item) in hit_test.regular_hit_test_nodes.iter() {
                if let Some(styled_node) = styled_nodes.get_mut(*node_id) {
                    styled_node.styled_node_state.hover = true;
                }
            }
        }
    }
    
    // 3. Apply active state (mouse button down on a hovered element)
    if current_window_state.mouse_state.left_down {
        if let Some(last_hit_test) = layout_window.hover_manager.get_current_mouse() {
            if let Some(hit_test) = last_hit_test.hovered_nodes.get(&dom_id) {
                let mut styled_nodes = styled_dom.styled_nodes.as_container_mut();
                for (node_id, _hit_item) in hit_test.regular_hit_test_nodes.iter() {
                    if let Some(styled_node) = styled_nodes.get_mut(*node_id) {
                        styled_node.styled_node_state.active = true;
                    }
                }
            }
        }
    }

    // 4. Apply :dragging pseudo-state from gesture_drag_manager
    // When the layout callback returns RefreshDom, the DOM is rebuilt from scratch
    // and the :dragging state that was set in event.rs on DragStart is lost.
    // Re-apply it here from the authoritative drag manager state.
    if let Some(drag_ctx) = layout_window.gesture_drag_manager.get_drag_context() {
        if let Some(node_drag) = drag_ctx.as_node_drag() {
            if node_drag.dom_id == dom_id {
                let mut styled_nodes = styled_dom.styled_nodes.as_container_mut();
                if let Some(styled_node) = styled_nodes.get_mut(node_drag.node_id) {
                    styled_node.styled_node_state.dragging = true;
                    log_debug!(
                        LogCategory::Layout,
                        "[apply_runtime_states_before_layout] Set dragging=true for node {:?}",
                        node_drag.node_id
                    );
                }

                // 5. Apply :drag-over pseudo-state on current drop target
                if let Some(drop_target) = node_drag.current_drop_target.into_option() {
                    if drop_target.dom == dom_id {
                        if let Some(target_node_id) = drop_target.node.into_crate_internal() {
                            if let Some(styled_node) = styled_nodes.get_mut(target_node_id) {
                                styled_node.styled_node_state.drag_over = true;
                                log_debug!(
                                    LogCategory::Layout,
                                    "[apply_runtime_states_before_layout] Set drag_over=true for node {:?}",
                                    target_node_id
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    styled_dom
}

/// Apply runtime states (focus, hover, active) to the StyledDom after layout
/// (DEPRECATED - use apply_runtime_states_before_layout instead)
///
/// The layout callback creates a fresh StyledDom where all StyledNodeState fields
/// are set to their defaults (focused=false, hover=false, active=false).
/// This function synchronizes those states with the current runtime state from
/// the various managers (FocusManager, mouse state, etc.).
///
/// This is critical for `:focus`, `:hover`, `:active` CSS pseudo-class styling
/// to work correctly after a DOM refresh.
#[allow(dead_code)]
fn apply_runtime_states_to_styled_dom(
    layout_window: &mut LayoutWindow,
    current_window_state: &FullWindowState,
) {
    // 1. Apply focus state
    if let Some(focused_node) = layout_window.focus_manager.get_focused_node() {
        if let Some(layout_result) = layout_window.layout_results.get_mut(&focused_node.dom) {
            if let Some(node_id) = focused_node.node.into_crate_internal() {
                let mut styled_nodes = layout_result.styled_dom.styled_nodes.as_container_mut();
                if let Some(styled_node) = styled_nodes.get_mut(node_id) {
                    styled_node.styled_node_state.focused = true;
                    log_debug!(
                        LogCategory::Layout,
                        "[apply_runtime_states] Set focused=true for node {:?}",
                        node_id
                    );
                }
            }
        }
    }
    
    // 2. Apply hover state based on hover manager
    // hovered_nodes is BTreeMap<DomId, HitTest>, and HitTest contains regular_hit_test_nodes
    if let Some(last_hit_test) = layout_window.hover_manager.get_current_mouse() {
        for (dom_id, hit_test) in last_hit_test.hovered_nodes.iter() {
            if let Some(layout_result) = layout_window.layout_results.get_mut(dom_id) {
                let mut styled_nodes = layout_result.styled_dom.styled_nodes.as_container_mut();
                for (node_id, _hit_item) in hit_test.regular_hit_test_nodes.iter() {
                    if let Some(styled_node) = styled_nodes.get_mut(*node_id) {
                        styled_node.styled_node_state.hover = true;
                    }
                }
            }
        }
    }
    
    // 3. Apply active state (mouse button down on a hovered element)
    if current_window_state.mouse_state.left_down {
        if let Some(last_hit_test) = layout_window.hover_manager.get_current_mouse() {
            for (dom_id, hit_test) in last_hit_test.hovered_nodes.iter() {
                if let Some(layout_result) = layout_window.layout_results.get_mut(dom_id) {
                    let mut styled_nodes = layout_result.styled_dom.styled_nodes.as_container_mut();
                    for (node_id, _hit_item) in hit_test.regular_hit_test_nodes.iter() {
                        if let Some(styled_node) = styled_nodes.get_mut(*node_id) {
                            styled_node.styled_node_state.active = true;
                        }
                    }
                }
            }
        }
    }
}

/// Update managers (FocusManager, ScrollManager, etc.) with new NodeIds after DOM reconciliation
///
/// When the DOM is regenerated, NodeIds can change. The `node_moves` from reconciliation
/// tell us which old NodeId maps to which new NodeId. We use this to update all managers
/// that track NodeIds so they point to the correct nodes in the new DOM.
fn update_managers_with_node_moves(
    layout_window: &mut LayoutWindow,
    node_moves: &[azul_core::diff::NodeMove],
    dom_id: azul_core::dom::DomId,
) {
    use azul_core::dom::{DomNodeId, NodeId};
    use azul_core::styled_dom::NodeHierarchyItemId;
    
    // Build a quick lookup map: old_node_id -> new_node_id
    let mut node_id_map: std::collections::BTreeMap<NodeId, NodeId> = std::collections::BTreeMap::new();
    for node_move in node_moves {
        node_id_map.insert(node_move.old_node_id, node_move.new_node_id);
    }
    
    // 1. Update FocusManager
    if let Some(focused) = layout_window.focus_manager.get_focused_node() {
        if focused.dom == dom_id {
            if let Some(old_node_id) = focused.node.into_crate_internal() {
                if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                    // Update the focused node to point to the new NodeId
                    layout_window.focus_manager.set_focused_node(Some(DomNodeId {
                        dom: dom_id,
                        node: NodeHierarchyItemId::from_crate_internal(Some(new_node_id)),
                    }));
                    log_debug!(
                        LogCategory::Layout,
                        "[update_managers] FocusManager: updated focus from {:?} to {:?}",
                        old_node_id, new_node_id
                    );
                } else {
                    // The focused node was not found in the new DOM - clear focus
                    layout_window.focus_manager.clear_focus();
                    log_debug!(
                        LogCategory::Layout,
                        "[update_managers] FocusManager: focused node {:?} not found in new DOM, clearing focus",
                        old_node_id
                    );
                }
            }
        }
    }
    
    // 2. Update ScrollManager
    // The ScrollManager tracks scroll offsets by DomNodeId, which also needs to be updated
    layout_window.scroll_manager.remap_node_ids(dom_id, &node_id_map);
    
    // 3. Update CursorManager (text cursor position)
    layout_window.cursor_manager.remap_node_ids(dom_id, &node_id_map);
    
    // 4. Update SelectionManager
    layout_window.selection_manager.remap_node_ids(dom_id, &node_id_map);

    // 5. Update HoverManager (BUG-1 fix: hover histories contain NodeIds that must be remapped)
    layout_window.hover_manager.remap_node_ids(dom_id, &node_id_map);

    // 6. Update GestureAndDragManager (BUG-2 fix: active drags contain NodeIds)
    layout_window.gesture_drag_manager.remap_node_ids(dom_id, &node_id_map);

    // 7. Update FocusManager pending contenteditable focus (BUG-3 fix)
    layout_window.focus_manager.remap_pending_focus_node_ids(dom_id, &node_id_map);
}

/// Helper function to generate WebRender frame
///
/// This should be called after regenerate_layout to submit the frame to WebRender.
/// Usually called once at the end of event processing.
pub fn generate_frame(
    layout_window: &mut LayoutWindow,
    render_api: &mut WrRenderApi,
    document_id: DocumentId,
    gl_context: &azul_core::gl::OptionGlContextPtr,
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
    wr_translate2::generate_frame(&mut txn, layout_window, render_api, true, gl_context);

    render_api.send_transaction(wr_translate2::wr_translate_document_id(document_id), txn);
}

/// Wrap the user's `StyledDom` with a `Titlebar` at the top.
///
/// The titlebar carries DragStart / Drag / DoubleClick callbacks so that the
/// window can be moved and maximized through regular `CallbackInfo` APIs
/// (gesture manager + `modify_window_state`).  No special event-system hooks
/// are needed.
fn inject_software_titlebar(
    user_dom: azul_core::styled_dom::StyledDom,
    window_title: &str,
    system_style: &SystemStyle,
) -> azul_core::styled_dom::StyledDom {
    use azul_layout::widgets::titlebar::Titlebar;

    let titlebar = Titlebar::from_system_style(
        window_title.into(),
        system_style,
    );
    let mut titlebar_dom = titlebar.dom();

    // Style the titlebar DOM (all properties are inline — no external CSS needed)
    let titlebar_styled = titlebar_dom.style(azul_css::css::Css::empty());

    // Use an Html root (not Body!) so we don't get double <body> nesting.
    // StyledDom::default() creates a Body root, and the user's DOM also starts
    // with Body — nesting body>body causes double 8px UA margin.
    // Html has display:block but no margin in the UA stylesheet.
    let mut container = azul_core::dom::Dom::create_html()
        .style(azul_css::css::Css::empty());
    container.append_child(titlebar_styled);
    container.append_child(user_dom);
    container
}
