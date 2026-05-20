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

/// Delay in ms before scrollbar overlay starts fading out after scroll stops.
const SCROLLBAR_FADE_DELAY_MS: u64 = 500;
/// Duration in ms of the scrollbar fade-out animation.
const SCROLLBAR_FADE_DURATION_MS: u64 = 200;

fn register_scroll_nodes(layout_window: &mut LayoutWindow) {
    let now: azul_core::task::Instant = std::time::Instant::now().into();
    for (dom_id, layout_result) in &layout_window.layout_results {
        for (node_idx, node) in layout_result.layout_tree.nodes.iter().enumerate() {
            let scrollbar_info = layout_result.layout_tree.warm(node_idx)
                .and_then(|w| w.scrollbar_info.as_ref());
            if let Some(scrollbar_info) = scrollbar_info {
                if scrollbar_info.needs_vertical || scrollbar_info.needs_horizontal {
                    if let Some(dom_node_id) = node.dom_node_id {
                        // CSS spec: scrolling occurs within the padding box, so the
                        // viewport for scroll clamp must be padding-box, not content-box.
                        // This must match compute_scrollbar_geometry() which also uses
                        // padding-box (inner_rect = paint_rect - borders).
                        let border_box_size = node.used_size.unwrap_or_default();
                        let resolved = node.box_props.unpack();
                        let border = &resolved.border;
                        let container_size = azul_core::geom::LogicalSize {
                            width: (border_box_size.width
                                    - border.left - border.right).max(0.0),
                            height: (border_box_size.height
                                     - border.top - border.bottom).max(0.0),
                        };

                        // container_rect must use absolute window coordinates,
                        // not (0,0), so scroll_into_view calculations are correct.
                        let container_origin = layout_result
                            .calculated_positions
                            .get(node_idx)
                            .copied()
                            .unwrap_or_else(azul_core::geom::LogicalPosition::zero);

                        let container_rect = azul_core::geom::LogicalRect {
                            origin: container_origin,
                            size: container_size,
                        };

                        let content_size = layout_result.layout_tree.get_content_size(node_idx);

                        // Use the layout-computed scrollbar width, not the
                        // hardcoded default. On macOS with overlay scrollbars,
                        // scrollbar_width is 0.0 (no layout space reserved).
                        // The scroll_manager falls back to DEFAULT_SCROLLBAR_WIDTH_PX
                        // when thickness is 0 for hit-test geometry.
                        let scrollbar_thickness = scrollbar_info.scrollbar_width
                            .max(scrollbar_info.scrollbar_height);

                        layout_window.scroll_manager.register_or_update_scroll_node(
                            *dom_id,
                            dom_node_id,
                            container_rect,
                            content_size,
                            now.clone(),
                            scrollbar_thickness,
                            scrollbar_info.visual_width_px,
                            scrollbar_info.needs_horizontal,
                            scrollbar_info.needs_vertical,
                        );

                        log_debug!(LogCategory::Layout,
                            "[register_scroll_nodes] Registered scroll node: dom={:?} node={:?} container={:?} content={:?}",
                            dom_id, dom_node_id, container_size, content_size);
                    }
                }
            }
        }
    }
    layout_window.scroll_manager.calculate_scrollbar_states();
}

/// Result of `regenerate_layout()` indicating whether the DOM structure changed.
///
/// When the DOM is structurally unchanged (same node types, hierarchy, classes,
/// IDs, inline styles, callback events), the expensive layout pipeline
/// (CSS cascade, flexbox, display list) can be skipped. Only image callbacks
/// need to be re-invoked since their content (e.g. GL textures) may have changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutRegenerateResult {
    /// DOM structure changed — full layout was performed
    /// (CSS cascade, flexbox, and display list were all recomputed).
    LayoutChanged,
    /// DOM structure is unchanged — layout was reused from previous frame.
    /// Image callbacks still need to be re-invoked since their content
    /// (e.g. GL textures) may have changed, but the expensive CSS cascade
    /// and flexbox passes were skipped.
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
    relayout_reason: azul_core::callbacks::RelayoutReason,
) -> Result<LayoutRegenerateResult, String> {
    log_debug!(LogCategory::Layout, "[regenerate_layout] START");
    azul_layout::probe::emit_phase_heap("start");

    // If the async font registry is available, request commonly-used fonts
    // and block until they are ready (eliminates FOUC). On cache hits this
    // is effectively free; on first run it blocks until the Scout + Builder
    // threads have parsed the needed fonts.
    azul_layout::probe::emit_phase_heap("before_registry_check");
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
            let font_stacks = rust_fontconfig::config::tokenize_common_families(rust_fontconfig::OperatingSystem::current());
            azul_layout::probe::emit_phase_heap_extra("after_tokenize", registry.chain_cache_len() as u64);
            registry.request_fonts(&font_stacks);
            azul_layout::probe::emit_phase_heap_extra("after_request_fonts", registry.chain_cache_len() as u64);
            // Snapshot the registry into an FcFontCache for use during layout
            layout_window.font_manager.fc_cache = registry.shared_cache();
            azul_layout::probe::emit_phase_heap("after_shared_cache");
            log_debug!(LogCategory::Layout, "[regenerate_layout] Font registry snapshot complete");
        } else {
            log_debug!(LogCategory::Layout, "[regenerate_layout] Using existing font cache (build still in progress)");
        }
    } else {
        azul_layout::probe::emit_phase_heap("before_fc_clone");
        // Fallback: use the provided fc_cache directly
        layout_window.font_manager.fc_cache = (**fc_cache).clone();
        azul_layout::probe::emit_phase_heap("after_fc_clone");
    }
    azul_layout::probe::emit_phase_heap("after_font_snapshot");

    // 1. Call user's layout callback to get new DOM
    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] Calling layout_callback"
    );

    // Create reference data container (syntax sugar to reduce parameter count)
    let layout_ref_data = LayoutCallbackInfoRefData {
        image_cache,
        gl_context: gl_context_ptr,
        system_fonts: &layout_window.font_manager.fc_cache,
        system_style: system_style.clone(),
        active_route: current_window_state.active_route.as_ref(),
    };

    let mut callback_info = LayoutCallbackInfo::new_with_reason(
        &layout_ref_data,
        current_window_state.size,
        current_window_state.theme,
        relayout_reason,
    );

    // Wire the callback's stored ctx (host-handle for managed FFIs,
    // PyCallableWrapper for Python, None for native Rust) so
    // `info.get_ctx()` reaches it. Without this, the macro-generated
    // host-invoker thunk sees `OptionRefAny::None` and returns the
    // kind's default (empty body) — which is exactly the "default DOM"
    // symptom we'd otherwise observe in the rendered window.
    callback_info.set_callable_ptr(&current_window_state.layout_callback.ctx);

    let app_data_borrowed = app_data.borrow_mut();
    azul_layout::probe::emit_phase_heap("before_callback");

    let user_dom =
        (current_window_state.layout_callback.cb)((*app_data_borrowed).clone(), callback_info);

    drop(app_data_borrowed); // Release borrow
    azul_layout::probe::emit_phase_heap("after_callback");

    // 1.5. Flatten recursive Dom → StyledDom (single deferred cascade pass)
    //
    // The user callback now returns a recursive `Dom` with CSS attached via `.with_component_css()`.
    // We collect all CSS objects, flatten the tree, and run a single cascade pass.
    let mut user_styled_dom = azul_core::styled_dom::StyledDom::create_from_dom(user_dom);
    azul_layout::probe::emit_phase_heap("after_create_from_dom");

    // 2. Resolve icon nodes to their actual content (text glyphs, images, etc.)
    // This must happen after the user's layout callback and before CSD injection
    azul_core::icon::resolve_icons_in_styled_dom(&mut user_styled_dom, icon_provider, system_style);
    azul_layout::probe::emit_phase_heap("after_icons");

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
    azul_layout::probe::emit_phase_heap("after_csd");

    // 3.4. Re-compute inheritance and compact cache on the composed tree.
    //
    // The user's layout callback may have merged multiple StyledDom subtrees via
    // append_child(). Each subtree was independently styled (restyle → apply_ua_css
    // → compute_inherited_values → build_compact_cache), but append_child() only
    // concatenates the CSS caches — it does NOT re-run inheritance or rebuild the
    // compact cache. This causes two correctness bugs:
    //
    //   1. Inherited properties (color, font-size, direction) from parent nodes
    //      do not flow into appended child subtrees.
    //   2. The compact cache entries from child subtrees are stale — they reflect
    //      the child's isolated cascade, not the composed tree with parent overrides.
    //
    // Additionally, CSD injection (step 3) may have prepended titlebar nodes via
    // another append_child(), further invalidating the cache.
    //
    // Re-running inheritance + compact cache rebuild on the fully composed tree
    // fixes both issues. Cost: one extra O(n) pass — acceptable for correctness.
    styled_dom.recompute_inheritance_and_compact_cache();
    azul_layout::probe::emit_phase_heap("after_recompute_cache");

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
        let old_hierarchy: Vec<azul_core::styled_dom::NodeHierarchyItem> =
            old_layout_result.styled_dom.node_hierarchy.as_ref().to_vec();

        // Get new node data (from current frame — now also includes titlebar)
        let mut new_node_data: Vec<azul_core::dom::NodeData> = styled_dom.node_data.as_ref().to_vec();
        let new_hierarchy: Vec<azul_core::styled_dom::NodeHierarchyItem> =
            styled_dom.node_hierarchy.as_ref().to_vec();

        // Build layout maps for reconciliation (empty for now - we just need node moves)
        let old_layout_map = azul_core::OrderedMap::default();
        let new_layout_map = azul_core::OrderedMap::default();

        // Run reconciliation to find matched nodes
        let diff_result = azul_core::diff::reconcile_dom(
            &old_node_data,
            &new_node_data,
            &old_hierarchy,
            &new_hierarchy,
            &old_layout_map,
            &new_layout_map,
            azul_core::dom::DomId::ROOT_ID,
            azul_core::task::Instant::now(),
        );

        // Execute state migration for matched nodes with merge callbacks
        if !diff_result.node_moves.is_empty() {
            let mut old_node_data_mut = old_node_data.clone();
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

        // 3.7. QUEUE LIFECYCLE EVENTS FOR DISPATCH
        //
        // Mount / Update / Resize events target NEW NodeIds — they resolve
        // cleanly against the freshly-installed `layout_results` later in
        // the dispatch path.
        //
        // Unmount events are different: their `target.node` is an OLD NodeId
        // that does NOT exist in the new tree. By the time
        // `dispatch_events_propagated` runs, `layout_results` has already
        // been replaced by the new layout, so a NodeId-based lookup will
        // miss the BeforeUnmount callback. To keep that callback firing we
        // resolve it RIGHT HERE — while the OLD `old_node_data` slice is
        // still in scope — and stash a `(CoreCallbackData, SyntheticEvent)`
        // pair on the layout window. The dispatcher drains this side queue
        // and invokes the callbacks directly, bypassing the DOM lookup.
        for event in diff_result.events {
            use azul_core::events::{ComponentEventFilter, EventFilter, EventType};
            if event.event_type == EventType::Unmount {
                let old_node_id = event
                    .target
                    .node
                    .into_crate_internal()
                    .map(|nid| nid.index());
                if let Some(idx) = old_node_id {
                    if let Some(nd) = old_node_data.get(idx) {
                        for cb in nd.get_callbacks().as_ref().iter() {
                            if matches!(
                                cb.event,
                                EventFilter::Component(ComponentEventFilter::BeforeUnmount)
                            ) {
                                layout_window
                                    .pending_unmount_invocations
                                    .push((cb.clone(), event.clone()));
                            }
                        }
                    }
                }
            } else {
                layout_window.pending_lifecycle_events.push(event);
            }
        }
    }
    azul_layout::probe::emit_phase_heap("after_state_migrate");

    // NOTE: dirty_text_nodes is NOT applied to the StyledDom here.
    // The V3 architecture has two paths:
    //   - Initial Layout Path: reads from StyledDom (committed state from layout callback)
    //   - Relayout Path: reads from dirty_text_nodes (optimistic edits in LayoutCache)
    // The DOM text is intentionally stale. After layout_and_generate_display_list
    // runs on the new DOM, update_text_cache_after_edit will be called for each
    // dirty_text_node to patch the LayoutCache with the edited content.
    // dirty_text_nodes keys are remapped in update_managers_with_node_moves (step 8).

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
    azul_layout::probe::emit_phase_heap("after_runtime_states");

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
        // Half a logical pixel — below this threshold, size differences are
        // subpixel rounding noise and do not warrant a full relayout.
        const SIZE_CHANGE_THRESHOLD: f32 = 0.5;
        (old_dims.width - new_dims.width).abs() > SIZE_CHANGE_THRESHOLD
            || (old_dims.height - new_dims.height).abs() > SIZE_CHANGE_THRESHOLD
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
                let old_layout_result_mut = layout_window.layout_results.get_mut(&azul_core::dom::DomId::ROOT_ID)
                    .expect("layout_result must exist after get() succeeded");
                let old_node_data_mut = old_layout_result_mut.styled_dom.node_data.as_mut();
                for (idx, new_cb) in image_updates {
                    if let Some(old_nd) = old_node_data_mut.get_mut(idx) {
                        old_nd.node_type = azul_core::dom::NodeType::Image(azul_css::css::BoxOrStatic::heap(
                            azul_core::resources::ImageRef::callback(new_cb.callback.clone(), new_cb.refany.clone())
                        ));
                    }
                }
            }

            // Also transfer any updated callback data (RefAny) for event callbacks
            // so that future events use fresh app state references
            let mut callback_updates: Vec<(usize, azul_core::callbacks::CoreCallbackDataVec)> = Vec::new();
            {
                let old_nd_ref = layout_window.layout_results.get(&azul_core::dom::DomId::ROOT_ID)
                    .expect("layout_result must exist after get() succeeded").styled_dom.node_data.as_ref();
                let new_nd_ref = styled_dom.node_data.as_ref();
                for (idx, (_old_nd, new_nd)) in old_nd_ref.iter().zip(new_nd_ref.iter()).enumerate() {
                    if !new_nd.callbacks.as_ref().is_empty() {
                        callback_updates.push((idx, new_nd.callbacks.clone()));
                    }
                }
            }
            if !callback_updates.is_empty() {
                let old_layout_result_mut = layout_window.layout_results.get_mut(&azul_core::dom::DomId::ROOT_ID)
                    .expect("layout_result must exist after get() succeeded");
                let old_node_data_mut = old_layout_result_mut.styled_dom.node_data.as_mut();
                for (idx, new_callbacks) in callback_updates {
                    if let Some(old_nd) = old_node_data_mut.get_mut(idx) {
                        old_nd.callbacks = new_callbacks;
                    }
                }
            }

            log_debug!(LogCategory::Layout, "[regenerate_layout] COMPLETE (layout unchanged)");
            azul_layout::probe::emit_phase_heap("end_unchanged");
            return Ok(LayoutRegenerateResult::LayoutUnchanged);
        }
    }
    } // end if !window_size_changed
    azul_layout::probe::emit_phase_heap("after_equivalence_check");

    // 4. Perform layout with solver3
    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] Calling layout_and_generate_display_list"
    );

    // Update system style for resolving system color keywords (selection colors, accent, etc.)
    layout_window.set_system_style(system_style.clone());
    azul_layout::probe::emit_phase_heap("before_layout_dl");

    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            current_window_state,
            renderer_resources,
            &ExternalSystemCallbacks::rust_internal(),
            debug_messages,
        )
        .map_err(|e| format!("Layout error: {:?}", e))?;
    azul_layout::probe::emit_phase_heap("after_layout_and_dl");

    // CRITICAL: Update layout_window's cached window state so the next
    // regenerate_layout correctly detects size changes.  Without this,
    // resizing back to the original dimensions would be a no-op because
    // the stale layout_window.current_window_state still held the old size.
    layout_window.current_window_state = current_window_state.clone();

    // V3 ARCHITECTURE: Re-apply dirty_text_nodes to the layout cache.
    // The layout just ran on the stale DOM text (from the layout callback).
    // Now patch the layout cache with the edited text from dirty_text_nodes
    // so the display list shows the correct (edited) content.
    // This calls update_text_cache_after_edit for each dirty node, which
    // re-shapes the text and regenerates the inline layout result.
    let dirty_entries: Vec<_> = layout_window.dirty_text_nodes.keys().cloned().collect();
    for (dom_id, node_id) in dirty_entries {
        // update_text_cache_after_edit reads from dirty_text_nodes internally
        // (via get_text_before_textinput which checks dirty_text_nodes first)
        // and updates the inline_layout_result in the layout tree.
        layout_window.reapply_dirty_text_node(dom_id, node_id);
    }

    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] Layout completed, {} DOMs",
        layout_window.layout_results.len()
    );

    // 5. Register scrollable nodes and calculate scrollbar states
    register_scroll_nodes(layout_window);

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
            azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(SCROLLBAR_FADE_DELAY_MS)),
            azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(SCROLLBAR_FADE_DURATION_MS)),
        );
    }

    // 7. Permission diff — scan the styled DOM for permission-bearing
    // NodeTypes (GeolocationProbe / CameraPreview / SensorProbe / …) and
    // refresh the refcount on PermissionManager. Subscribe / Release diff
    // events accumulate in the manager's queue; the platform shell drains
    // and dispatches them via `crate::desktop::extra::permission::apply_diff_events`.
    //
    // Today the only permission-bearing NodeType is GeolocationProbe (P3.1);
    // CameraPreview / SensorProbe land in P6 and just add arms here. A probe
    // in the tree subscribes Capability::Geolocation, so the platform backend
    // turns it into the OS location prompt. Snapshot the (capability, node)
    // pairs first so we don't hold a borrow on `layout_results` while the
    // diff mutates `permission_manager`.
    let permission_bearing: Vec<(
        azul_layout::managers::permission::Capability,
        azul_core::dom::DomNodeId,
    )> = {
        let mut pairs = Vec::new();
        for (dom_id, layout_result) in layout_window.layout_results.iter() {
            for (i, nd) in layout_result.styled_dom.node_data.as_ref().iter().enumerate() {
                if let azul_core::dom::NodeType::GeolocationProbe(_) = nd.get_node_type() {
                    pairs.push((
                        azul_layout::managers::permission::Capability::Geolocation,
                        azul_core::dom::DomNodeId {
                            dom: *dom_id,
                            node: azul_core::dom::NodeId::from_usize(i).into(),
                        },
                    ));
                }
            }
        }
        pairs
    };
    layout_window.permission_manager.diff_layout(|emit| {
        for (capability, node_id) in &permission_bearing {
            emit(*capability, *node_id);
        }
    });
    let permission_events = layout_window.permission_manager.take_pending_events();
    if !permission_events.is_empty() {
        crate::desktop::extra::permission::apply_diff_events(&permission_events);
    }

    // 7a. Drain async permission results parked by a platform backend (an
    // OS prompt's completion handler / onRequestPermissionsResult) since
    // the last pass, and fold them into the manager. The native callback
    // runs on an arbitrary thread with no handle to this LayoutWindow, so
    // it parks the result in azul-layout's process-global channel; here is
    // where it lands in the live manager. (No producer fires yet — the
    // async request path in `permission::handle_event` is a later tick —
    // but the consumer is live and unit-tested in azul-layout.)
    {
        let async_results = azul_layout::managers::permission::drain_async_results();
        let mut changed = false;
        for (capability, state) in async_results {
            changed |= layout_window.permission_manager.set_status(capability, state);
        }
        if changed {
            // A permission flipped — permission-aware callbacks should get a
            // chance to re-render. The next regenerate_layout picks up the
            // new statuses; the relayout trigger lands with the producer.
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] applied async permission result(s)"
            );
        }
    }

    // 7b. Geolocation diff — symmetric to the permission pass. Walks
    // every DOM in this window for `NodeType::GeolocationProbe` nodes
    // and feeds their configs to the manager. Subscribe / Release /
    // Reconfigure events emitted by the manager route through the
    // platform backend, which starts or stops the native
    // CLLocationManager / FusedLocationProviderClient / geoclue
    // subscription.
    {
        // Snapshot the configs first so we don't hold a borrow on
        // `layout_window.layout_results` while mutating
        // `layout_window.geolocation_manager`.
        let mut probe_configs: Vec<azul_core::geolocation::GeolocationProbeConfig> = Vec::new();
        for layout_result in layout_window.layout_results.values() {
            for nd in layout_result.styled_dom.node_data.as_ref().iter() {
                if let azul_core::dom::NodeType::GeolocationProbe(cfg) = nd.get_node_type() {
                    probe_configs.push(*cfg);
                }
            }
        }
        layout_window.geolocation_manager.diff_layout(|emit| {
            for cfg in &probe_configs {
                emit(*cfg);
            }
        });
    }
    let geolocation_events = layout_window.geolocation_manager.take_pending_events();
    if !geolocation_events.is_empty() {
        crate::desktop::extra::geolocation::apply_diff_events(&geolocation_events);
    }

    // 7c. Drain location fixes a platform backend parked since the last
    // pass (Android FusedLocationProvider onLocationResult / iOS
    // CLLocationManagerDelegate run on arbitrary threads with no handle to
    // this LayoutWindow, so they park fixes in azul-layout's process-global
    // channel) and fold the latest into the manager. (No producer fires yet
    // — the backend `handle_event` location subscription is a later tick —
    // but the consumer is live and unit-tested in azul-layout.)
    {
        let fixes = azul_layout::managers::geolocation::drain_location_fixes();
        let mut changed = false;
        for fix in fixes {
            changed |= layout_window.geolocation_manager.set_latest_fix(fix);
        }
        if changed {
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] applied async location fix"
            );
        }
    }

    // 7d-pre (biometric availability): fold the device capability into the
    // manager so CallbackInfo::get_biometric_kind() reports the real sensor
    // (Face / Fingerprint / Iris) instead of the NotAvailable default.
    // Cached behind a OnceLock — the underlying probe is a native call, so
    // this is a cheap atomic read after the first frame.
    layout_window
        .biometric_manager
        .set_availability(crate::desktop::extra::biometric::availability_cached());

    // 7d. Dispatch biometric-auth requests a callback queued this frame.
    // CallbackInfo::request_biometric_auth parks the prompt in
    // azul-layout's process-global request channel; we drain it here and
    // hand each to the native backend (dll::desktop::extra::biometric),
    // which shows the OS prompt and asynchronously parks the outcome back
    // through the result channel drained just below. The stub backend
    // resolves every request to Unavailable for now.
    {
        let requests = azul_layout::managers::biometric::drain_biometric_requests();
        for prompt in &requests {
            crate::desktop::extra::biometric::request(prompt);
        }
    }

    // 7e. Drain biometric-auth results a platform backend parked since
    // the last pass. The OS prompt's reply (iOS/macOS LAContext reply
    // block, Android BiometricPrompt.AuthenticationCallback, Windows
    // UserConsentVerifier) fires on an arbitrary thread with no handle to
    // this LayoutWindow, so it parks the result in azul-layout's
    // process-global channel; we fold the latest into the manager here so
    // a callback can read it via CallbackInfo::get_biometric_result(). (No
    // producer fires yet — the native backend is a later tick — but the
    // consumer is live and unit-tested in azul-layout.)
    {
        let results = azul_layout::managers::biometric::drain_biometric_results();
        let mut changed = false;
        for result in results {
            changed |= layout_window.biometric_manager.set_last_result(result);
        }
        if changed {
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] applied async biometric result"
            );
        }
    }

    // 7f. Dispatch keyring ops a callback queued this frame, then drain any
    // results a backend parked. CallbackInfo::keyring_store/get/delete park
    // a KeyringRequest in azul-layout's process-global channel; we hand each
    // to the native keyring (Keychain / KeyStore / libsecret / locker), and
    // a biometry-bound Get's outcome arrives asynchronously on the result
    // channel. The stub backend resolves every op to Unavailable for now.
    {
        let requests = azul_layout::managers::keyring::drain_keyring_requests();
        for req in &requests {
            crate::desktop::extra::keyring::request(req);
        }
    }
    {
        let results = azul_layout::managers::keyring::drain_keyring_results();
        let mut changed = false;
        for result in results {
            changed |= layout_window.keyring_manager.set_last_result(result);
        }
        if changed {
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] applied async keyring result"
            );
        }
    }

    // 7g. (PDF export is now the standalone headless `Pdf::from_dom` API in
    // dll::desktop::extra::pdf — no window-coupled per-frame export drain.)

    // 7h-pre (sensor subscription): kick the device's motion-sensor
    // subscription. OnceLock-guarded inside, so only the first frame does
    // the native registration (CoreMotion start / Android registerListener);
    // every later frame is a cheap atomic read. Then pull the latest sample
    // (CoreMotion's pull API needs a per-frame read; Android pushes from its
    // JNI callback, so poll is a no-op there).
    crate::desktop::extra::sensors::ensure_started();
    crate::desktop::extra::sensors::poll();

    // 7h. Drain motion-sensor readings the platform backend parked since the
    // last pass (CoreMotion / Android SensorManager fire on arbitrary
    // threads with no handle to this LayoutWindow, so they park readings in
    // azul-layout's process-global channel) and fold the latest per kind
    // into the manager. The Android JNI backend is live (samples flow once
    // the AzulSensors.java shim ships); the Apple CoreMotion producer lands
    // next tick. The consumer is unit-tested in azul-layout.
    {
        let readings = azul_layout::managers::sensors::drain_sensor_readings();
        let mut changed = false;
        for reading in readings {
            changed |= layout_window.sensor_manager.set_reading(reading);
        }
        if changed {
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] applied async sensor reading"
            );
        }
    }

    // 7i-pre (gamepad poll): one-time native subscription (OnceLock inside)
    // + per-frame pull of each pad's current state. The desktop gilrs backend
    // pumps its event queue and snapshots connected pads here; iOS GCController
    // does likewise (pending); Android is push-based so poll is a no-op there.
    crate::desktop::extra::gamepad::ensure_started();
    crate::desktop::extra::gamepad::poll();

    // 7i. Drain gamepad states the controller backend parked since the last
    // pass (gilrs / iOS GCController / Android InputDevice run on their own
    // thread/queue with no handle to this LayoutWindow, so they park states in
    // azul-layout's process-global channel) and fold the latest per id into
    // the manager. The desktop gilrs producer is live; the mobile backends are
    // follow-ups. The consumer is unit-tested in azul-layout.
    {
        let states = azul_layout::managers::gamepad::drain_gamepad_states();
        let mut changed = false;
        for state in states {
            changed |= layout_window.gamepad_manager.set_state(state);
        }
        if changed {
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] applied async gamepad state"
            );
        }
    }

    log_debug!(LogCategory::Layout, "[regenerate_layout] COMPLETE");
    azul_layout::probe::emit_phase_heap("end");

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
    //
    // Ownership transfer: pull the existing DomLayoutResult out of the map
    // (`.remove()` instead of `.get()`), take its `styled_dom` by value, and
    // hand it to `layout_and_generate_display_list`, which will move it into
    // the freshly-inserted result. This eliminates the double clone that used
    // to happen on every resize (once here, once again inside the layout fn).
    if let Some(layout_result) = layout_window
        .layout_results
        .remove(&azul_core::dom::DomId::ROOT_ID)
    {
        // Move the StyledDom out of the old DomLayoutResult; the remaining
        // fields (positions, display list, tree) drop when `layout_result`
        // goes out of scope. `layout_and_generate_display_list` then inserts
        // a fresh DomLayoutResult built around this very StyledDom without
        // cloning it.
        let styled_dom = layout_result.styled_dom;

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

    register_scroll_nodes(layout_window);

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
    
    // 3. Update MultiCursorState node IDs
    if let Some(ref mut mc) = layout_window.text_edit_manager.multi_cursor {
        mc.remap_node_ids(dom_id, &node_id_map);
    }
    
    // 4. (SelectionManager removed — multi_cursor remap handled above in step 3)

    // 5. Update HoverManager (BUG-1 fix: hover histories contain NodeIds that must be remapped)
    layout_window.hover_manager.remap_node_ids(dom_id, &node_id_map);

    // 6. Update GestureAndDragManager (BUG-2 fix: active drags contain NodeIds)
    layout_window.gesture_drag_manager.remap_node_ids(dom_id, &node_id_map);

    // 7. Update FocusManager pending contenteditable focus (BUG-3 fix)
    layout_window.focus_manager.remap_pending_focus_node_ids(dom_id, &node_id_map);

    // 8. Remap and apply dirty_text_nodes to preserve text edits across DOM rebuilds.
    // The user's layout callback returns the original text, but if we have edits
    // in dirty_text_nodes, we need to patch the new StyledDom with the edited content.
    {
        let mut new_dirty = std::collections::BTreeMap::new();
        for ((old_dom, old_node), content) in layout_window.dirty_text_nodes.iter() {
            if *old_dom == dom_id {
                if let Some(&new_node_id) = node_id_map.get(old_node) {
                    new_dirty.insert((*old_dom, new_node_id), content.clone());
                }
            } else {
                new_dirty.insert((*old_dom, *old_node), content.clone());
            }
        }
        layout_window.dirty_text_nodes = new_dirty;
    }
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
    // Process any pending VirtualView updates requested by callbacks
    // This must happen BEFORE wr_translate2::generate_frame() so that the VirtualView
    // callbacks can be re-invoked and their layout results are available
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let current_window_state = layout_window.current_window_state.clone();

    let renderer_resources = std::mem::take(&mut layout_window.renderer_resources);
    layout_window.process_pending_virtual_view_updates(
        &current_window_state,
        &renderer_resources,
        &system_callbacks,
    );
    layout_window.renderer_resources = renderer_resources;

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
    let titlebar_styled = azul_core::styled_dom::StyledDom::create(&mut titlebar_dom, azul_css::css::Css::empty());

    // Use an Html root (not Body!) so we don't get double <body> nesting.
    // StyledDom::default() creates a Body root, and the user's DOM also starts
    // with Body — nesting body>body causes double 8px UA margin.
    // Html has display:block but no margin in the UA stylesheet.
    let mut container_dom = azul_core::dom::Dom::create_html();
    let mut container = azul_core::styled_dom::StyledDom::create(&mut container_dom, azul_css::css::Css::empty());
    container.append_child(titlebar_styled);
    container.append_child(user_dom);
    container
}
