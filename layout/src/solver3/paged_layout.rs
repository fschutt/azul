//! CSS Paged Media layout integration with integrated fragmentation
//!
//! This module provides functionality for laying out documents with pagination,
//! such as for PDF generation. It uses the new integrated architecture where:
//! 
//! 1. page_index is assigned to nodes DURING layout based on Y position
//! 2. generate_display_lists_paged() creates per-page DisplayLists by filtering
//! 3. No post-hoc fragmentation is needed

use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    resources::RendererResources,
    selection::SelectionState,
    styled_dom::StyledDom,
};
use azul_css::LayoutDebugMessage;

use crate::{
    font_traits::{ParsedFontTrait, TextLayoutCache},
    fragmentation::PageMargins,
    paged::FragmentationContext,
    solver3::{
        cache::LayoutCache,
        display_list::{DisplayList, generate_display_lists_paged},
        LayoutContext, LayoutError, Result,
    },
};

/// Layout a document with integrated pagination, returning one DisplayList per page.
///
/// This function performs CSS Paged Media layout with fragmentation integrated
/// into the layout process itself, using the new architecture where:
/// 
/// 1. The FragmentationContext is passed to layout_document via LayoutContext
/// 2. Nodes get their page_index assigned during layout based on absolute Y position
/// 3. DisplayLists are generated per-page by filtering items based on page bounds
///
/// # Arguments
/// * `fragmentation_context` - Controls page size and fragmentation behavior
/// * Other arguments same as `layout_document()`
///
/// # Returns
/// A vector of DisplayLists, one per page. Each DisplayList contains the
/// elements that fit on that page, with Y-coordinates relative to the page origin.
#[cfg(feature = "text_layout")]
pub fn layout_document_paged<T, F>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    mut fragmentation_context: FragmentationContext,
    new_dom: StyledDom,
    viewport: LogicalRect,
    font_manager: &mut crate::font_traits::FontManager<T>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    selections: &BTreeMap<DomId, SelectionState>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
    font_loader: F,
) -> Result<Vec<DisplayList>> 
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(&[u8], usize) -> std::result::Result<T, crate::text3::cache::LayoutError>,
{
    // Font Resolution And Loading
    {
        use crate::solver3::getters::{
            collect_and_resolve_font_chains, 
            collect_font_ids_from_chains,
            compute_fonts_to_load,
            load_fonts_from_disk,
        };
        
        let chains = collect_and_resolve_font_chains(&new_dom, &font_manager.fc_cache);
        let required_fonts = collect_font_ids_from_chains(&chains);
        let already_loaded = font_manager.get_loaded_font_ids();
        let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);
        
        if !fonts_to_load.is_empty() {
            let load_result = load_fonts_from_disk(
                &fonts_to_load,
                &font_manager.fc_cache,
                &font_loader,
            );
            font_manager.insert_fonts(load_result.loaded);
            for (font_id, error) in &load_result.failed {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::warning(format!(
                        "[FontLoading] Failed to load font {:?}: {}", font_id, error
                    )));
                }
            }
        }
        font_manager.set_font_chain_cache(chains.into_inner());
    }

    // Get page dimensions from fragmentation context
    let page_content_height = fragmentation_context.page_content_height();
    
    // Handle continuous media (no pagination)
    if !fragmentation_context.is_paged() {
        let display_list = layout_document_with_fragmentation(
            cache, text_cache, &mut fragmentation_context, new_dom, viewport, font_manager,
            scroll_offsets, selections, debug_messages,
            gpu_value_cache, renderer_resources, id_namespace, dom_id,
        )?;
        return Ok(vec![display_list]);
    }
    
    // Paged Layout
    
    // Perform layout with fragmentation context
    // This will assign page_index to nodes based on their Y position
    let _display_list = layout_document_with_fragmentation(
        cache, text_cache, &mut fragmentation_context, new_dom.clone(), viewport, font_manager,
        scroll_offsets, selections, debug_messages,
        gpu_value_cache, renderer_resources, id_namespace, dom_id,
    )?;
    
    // Get the layout tree and positions (we need mutable ctx for generate_display_lists_paged)
    let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;
    let calculated_positions = &cache.calculated_positions;
    
    // Debug: print page assignments
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PagedLayout] Page content height: {}", page_content_height
        )));
        
        for (idx, node) in tree.nodes.iter().enumerate() {
            if node.page_index > 0 {
                let node_type = node.dom_node_id
                    .and_then(|id| new_dom.node_data.as_container().internal.get(id.index()))
                    .map(|n| format!("{:?}", n.node_type))
                    .unwrap_or_else(|| "Anonymous".to_string());
                let pos = calculated_positions.get(&idx)
                    .map(|p| format!("({:.1}, {:.1})", p.x, p.y))
                    .unwrap_or_else(|| "?".to_string());
                msgs.push(LayoutDebugMessage::info(format!(
                    "[PagedLayout] Node {} {} at {} -> page {}",
                    idx, node_type, pos, node.page_index
                )));
            }
        }
    }
    
    // Compute scroll IDs (needed for generate_display_lists_paged)
    use crate::window::LayoutWindow;
    let (scroll_ids, _scroll_id_to_node_id) = LayoutWindow::compute_scroll_ids(tree, &new_dom);
    
    // Create temporary context for display list generation
    let mut counter_values = cache.counters.clone();
    let mut ctx = LayoutContext {
        styled_dom: &new_dom,
        font_manager: &*font_manager,
        selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(&mut fragmentation_context),
    };
    
    // Generate per-page display lists using the new integrated approach
    let pages = generate_display_lists_paged(
        &mut ctx,
        tree,
        calculated_positions,
        scroll_offsets,
        &scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
        page_content_height,
    )?;
    
    Ok(pages)
}

/// Internal helper: Perform layout with a fragmentation context
#[cfg(feature = "text_layout")]
fn layout_document_with_fragmentation<T: ParsedFontTrait + Sync + 'static>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    fragmentation_context: &mut FragmentationContext,
    new_dom: StyledDom,
    viewport: LogicalRect,
    font_manager: &crate::font_traits::FontManager<T>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    selections: &BTreeMap<DomId, SelectionState>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
) -> Result<DisplayList> {
    use crate::solver3::{
        cache,
        display_list::generate_display_list,
        layout_tree::DirtyFlag,
        getters::get_writing_mode,
    };
    
    // Create temporary context without counters for tree generation
    let mut counter_values = BTreeMap::new();
    let mut ctx_temp = LayoutContext {
        styled_dom: &new_dom,
        font_manager,
        selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(fragmentation_context),
    };

    // --- Step 1: Reconciliation & Invalidation ---
    let (mut new_tree, mut recon_result) =
        cache::reconcile_and_invalidate(&mut ctx_temp, cache, viewport)?;

    // Step 1.2: Clear Taffy Caches for Dirty Nodes
    for &node_idx in &recon_result.intrinsic_dirty {
        if let Some(node) = new_tree.get_mut(node_idx) {
            node.taffy_cache.clear();
        }
    }
    
    // Step 1.3: Compute CSS Counters
    cache::compute_counters(&new_dom, &new_tree, &mut counter_values);
    
    // Now create the real context with computed counters and fragmentation
    let mut ctx = LayoutContext {
        styled_dom: &new_dom,
        font_manager,
        selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(fragmentation_context),
    };

    // --- Step 1.5: Early Exit Optimization ---
    if recon_result.is_clean() {
        ctx.debug_log("No changes, returning existing display list");
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;

        use crate::window::LayoutWindow;
        let (scroll_ids, scroll_id_to_node_id) =
            LayoutWindow::compute_scroll_ids(tree, &new_dom);
        cache.scroll_ids = scroll_ids.clone();
        cache.scroll_id_to_node_id = scroll_id_to_node_id;

        return generate_display_list(
            &mut ctx,
            tree,
            &cache.calculated_positions,
            scroll_offsets,
            &scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
            dom_id,
        );
    }

    // --- Step 2: Incremental Layout Loop ---
    let mut calculated_positions = cache.calculated_positions.clone();
    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > 10 {
            break;
        }
        
        calculated_positions = cache.calculated_positions.clone();
        let mut reflow_needed_for_scrollbars = false;

        crate::solver3::sizing::calculate_intrinsic_sizes(&mut ctx, &mut new_tree, &recon_result.intrinsic_dirty)?;

        for &root_idx in &recon_result.layout_roots {
            let (cb_pos, cb_size) = get_containing_block_for_node(
                &new_tree,
                &new_dom,
                root_idx,
                &calculated_positions,
                viewport,
            );

            cache::calculate_layout_for_subtree(
                &mut ctx,
                &mut new_tree,
                text_cache,
                root_idx,
                cb_pos,
                cb_size,
                &mut calculated_positions,
                &mut reflow_needed_for_scrollbars,
                &mut cache.float_cache,
            )?;

            if !calculated_positions.contains_key(&root_idx) {
                calculated_positions.insert(root_idx, cb_pos);
            }
        }

        cache::reposition_clean_subtrees(
            &new_dom,
            &new_tree,
            &recon_result.layout_roots,
            &mut calculated_positions,
        );

        if reflow_needed_for_scrollbars {
            ctx.debug_log("Scrollbars changed container size, starting full reflow...");
            recon_result.layout_roots.clear();
            recon_result.layout_roots.insert(new_tree.root);
            recon_result.intrinsic_dirty = (0..new_tree.nodes.len()).collect();
            continue;
        }

        break;
    }

    // --- Step 3: Adjust Positions ---
    crate::solver3::positioning::adjust_relative_positions(
        &mut ctx,
        &new_tree,
        &mut calculated_positions,
        viewport,
    )?;

    crate::solver3::positioning::position_out_of_flow_elements(
        &mut ctx,
        &mut new_tree,
        &mut calculated_positions,
        viewport,
    )?;

    // --- Step 3.75: Compute Stable Scroll IDs ---
    use crate::window::LayoutWindow;
    let (scroll_ids, scroll_id_to_node_id) = LayoutWindow::compute_scroll_ids(&new_tree, &new_dom);

    // --- Step 4: Generate Display List & Update Cache ---
    let display_list = generate_display_list(
        &mut ctx,
        &new_tree,
        &calculated_positions,
        scroll_offsets,
        &scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
    )?;

    cache.tree = Some(new_tree);
    cache.calculated_positions = calculated_positions;
    cache.viewport = Some(viewport);
    cache.scroll_ids = scroll_ids;
    cache.scroll_id_to_node_id = scroll_id_to_node_id;
    cache.counters = counter_values;

    Ok(display_list)
}

// Helper function (copy from mod.rs)
fn get_containing_block_for_node(
    tree: &crate::solver3::layout_tree::LayoutTree,
    styled_dom: &StyledDom,
    node_idx: usize,
    calculated_positions: &BTreeMap<usize, LogicalPosition>,
    viewport: LogicalRect,
) -> (LogicalPosition, LogicalSize) {
    use crate::solver3::getters::get_writing_mode;
    
    if let Some(parent_idx) = tree.get(node_idx).and_then(|n| n.parent) {
        if let Some(parent_node) = tree.get(parent_idx) {
            let pos = calculated_positions
                .get(&parent_idx)
                .copied()
                .unwrap_or_default();
            let size = parent_node.used_size.unwrap_or_default();
            let content_pos = LogicalPosition::new(
                pos.x + parent_node.box_props.border.left + parent_node.box_props.padding.left,
                pos.y + parent_node.box_props.border.top + parent_node.box_props.padding.top,
            );

            if let Some(dom_id) = parent_node.dom_node_id {
                let styled_node_state = &styled_dom
                    .styled_nodes
                    .as_container()
                    .get(dom_id)
                    .map(|n| &n.state)
                    .cloned()
                    .unwrap_or_default();
                let writing_mode = get_writing_mode(styled_dom, dom_id, styled_node_state).unwrap_or_default();
                let content_size = parent_node.box_props.inner_size(size, writing_mode);
                return (content_pos, content_size);
            }

            return (content_pos, size);
        }
    }
    (viewport.origin, viewport.size)
}
