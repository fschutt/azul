//! CSS Paged Media layout integration with integrated fragmentation
//!
//! This module provides functionality for laying out documents with pagination,
//! such as for PDF generation. It uses the new integrated architecture where:
//!
//! 1. page_index is assigned to nodes DURING layout based on Y position
//! 2. generate_display_lists_paged() creates per-page DisplayLists by filtering
//! 3. No post-hoc fragmentation is needed
//!
//! **Note**: Full CSS `@page` rule parsing is not yet implemented. The `FakePageConfig`
//! provides programmatic control over page decoration as a temporary solution.

use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    resources::RendererResources,
    selection::{SelectionState, TextSelection},
    styled_dom::StyledDom,
};
use azul_css::LayoutDebugMessage;

use crate::{
    font_traits::{ParsedFontTrait, TextLayoutCache},
    fragmentation::PageMargins,
    paged::FragmentationContext,
    solver3::{
        cache::LayoutCache,
        display_list::DisplayList,
        getters::{get_break_after, get_break_before, get_break_inside},
        pagination::FakePageConfig,
        LayoutContext, LayoutError, Result,
    },
};

/// Result of `compute_layout_with_fragmentation`: contains the data
/// needed to generate a display list afterwards. The tree and
/// calculated_positions are stored in the `LayoutCache` that was passed in.
pub struct FragmentationLayoutResult {
    pub scroll_ids: BTreeMap<usize, u64>,
}

/// Layout a document with integrated pagination, returning one DisplayList per page.
///
/// This function performs CSS Paged Media layout with fragmentation integrated
/// into the layout process itself, using the new architecture where:
///
/// 1. The FragmentationContext is passed to layout_document via LayoutContext
/// 2. Nodes get their page_index assigned during layout based on absolute Y position
/// 3. DisplayLists are generated per-page by filtering items based on page bounds
///
/// Uses default page header/footer configuration (page numbers in footer).
/// For custom headers/footers, use `layout_document_paged_with_config`.
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
    fragmentation_context: FragmentationContext,
    new_dom: &StyledDom,
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
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
) -> Result<Vec<DisplayList>>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(&[u8], usize) -> std::result::Result<T, crate::text3::cache::LayoutError>,
{
    // Use default page config (page numbers in footer)
    let page_config = FakePageConfig::new().with_footer_page_numbers();

    layout_document_paged_with_config(
        cache,
        text_cache,
        fragmentation_context,
        new_dom,
        viewport,
        font_manager,
        scroll_offsets,
        selections,
        debug_messages,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
        font_loader,
        page_config,
        get_system_time_fn,
    )
}

/// Layout a document with integrated pagination and custom page configuration.
///
/// This function is the same as `layout_document_paged` but allows you to
/// specify custom headers and footers via `FakePageConfig`.
///
/// # Arguments
/// * `page_config` - Configuration for page headers/footers (see `FakePageConfig`)
/// * Other arguments same as `layout_document_paged()`
#[cfg(feature = "text_layout")]
pub fn layout_document_paged_with_config<T, F>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    mut fragmentation_context: FragmentationContext,
    new_dom: &StyledDom,
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
    page_config: FakePageConfig,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
) -> Result<Vec<DisplayList>>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(&[u8], usize) -> std::result::Result<T, crate::text3::cache::LayoutError>,
{
    // Font Resolution And Loading
    let _paged_t0 = std::time::Instant::now();
    {
        use crate::solver3::getters::{
            collect_and_resolve_font_chains, collect_font_ids_from_chains, compute_fonts_to_load,
            load_fonts_from_disk, register_embedded_fonts_from_styled_dom,
        };

        // TODO: Accept platform as parameter instead of using ::current()
        let platform = azul_css::system::Platform::current();

        // Register embedded FontRefs (e.g. Material Icons) before resolving chains
        register_embedded_fonts_from_styled_dom(new_dom, font_manager, &platform);

        let _fc0 = std::time::Instant::now();
        let chains = collect_and_resolve_font_chains(new_dom, &font_manager.fc_cache, &platform);
        eprintln!("      [font_resolution] collect_and_resolve_font_chains: {:?}", _fc0.elapsed());
        let required_fonts = collect_font_ids_from_chains(&chains);
        let already_loaded = font_manager.get_loaded_font_ids();
        let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);
        eprintln!("      [font_resolution] {} required, {} already loaded, {} to load", required_fonts.len(), already_loaded.len(), fonts_to_load.len());

        if !fonts_to_load.is_empty() {
            let _fc1 = std::time::Instant::now();
            let load_result =
                load_fonts_from_disk(&fonts_to_load, &font_manager.fc_cache, &font_loader);
            eprintln!("      [font_resolution] load_fonts_from_disk: {:?} ({} loaded, {} failed)", _fc1.elapsed(), load_result.loaded.len(), load_result.failed.len());
            font_manager.insert_fonts(load_result.loaded);
            for (font_id, error) in &load_result.failed {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::warning(format!(
                        "[FontLoading] Failed to load font {:?}: {}",
                        font_id, error
                    )));
                }
            }
        }
        font_manager.set_font_chain_cache(chains.into_fontconfig_chains());
    }
    eprintln!("    [paged_layout] font resolution: {:?}", _paged_t0.elapsed());

    // Get page dimensions from fragmentation context
    let page_content_height = fragmentation_context.page_content_height();

    // Handle continuous media (no pagination)
    if !fragmentation_context.is_paged() {
        let _result = compute_layout_with_fragmentation(
            cache,
            text_cache,
            &mut fragmentation_context,
            new_dom,
            viewport,
            font_manager,
            selections,
            debug_messages,
            get_system_time_fn,
        )?;

        // Generate display list from cached tree/positions
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;
        let mut counter_values = cache.counters.clone();
        let empty_text_selections: BTreeMap<DomId, TextSelection> = BTreeMap::new();
        let mut ctx = LayoutContext {
            styled_dom: new_dom,
            font_manager: &*font_manager,
            selections,
            text_selections: &empty_text_selections,
            debug_messages,
            counters: &mut counter_values,
            viewport_size: viewport.size,
            fragmentation_context: Some(&mut fragmentation_context),
            cursor_is_visible: true,
            cursor_location: None,
            cache_map: std::mem::take(&mut cache.cache_map),
            system_style: None,
            get_system_time_fn,
        };

        use crate::solver3::display_list::generate_display_list;
        let display_list = generate_display_list(
            &mut ctx,
            tree,
            &cache.calculated_positions,
            scroll_offsets,
            &cache.scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
            dom_id,
        )?;
        cache.cache_map = std::mem::take(&mut ctx.cache_map);
        return Ok(vec![display_list]);
    }

    // Paged Layout

    // Perform layout with fragmentation context (layout only, no display list)
    let _paged_t1 = std::time::Instant::now();
    let _result = compute_layout_with_fragmentation(
        cache,
        text_cache,
        &mut fragmentation_context,
        new_dom,
        viewport,
        font_manager,
        selections,
        debug_messages,
        get_system_time_fn,
    )?;
    eprintln!("    [paged_layout] layout_with_fragmentation: {:?}", _paged_t1.elapsed());

    // Get the layout tree and positions
    let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;
    let calculated_positions = &cache.calculated_positions;

    // Debug: log page layout info
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PagedLayout] Page content height: {}",
            page_content_height
        )));
    }

    // Use scroll IDs computed by compute_layout_with_fragmentation (stored in cache)
    let scroll_ids = &cache.scroll_ids;

    // Create temporary context for display list generation
    let mut counter_values = cache.counters.clone();
    let empty_text_selections: BTreeMap<DomId, TextSelection> = BTreeMap::new();
    let mut ctx = LayoutContext {
        styled_dom: new_dom,
        font_manager: &*font_manager,
        selections,
        text_selections: &empty_text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(&mut fragmentation_context),
        cursor_is_visible: true, // Paged layout: cursor always visible
        cursor_location: None,   // Paged layout: no cursor
        cache_map: std::mem::take(&mut cache.cache_map),
        system_style: None,
        get_system_time_fn,
    };

    // NEW: Use the commitment-based pagination approach with CSS break properties
    //
    // This treats pages as viewports into a single infinite canvas:
    // 1. Generate ONE complete display list on infinite vertical strip
    // 2. Analyze CSS break properties (break-before, break-after, break-inside)
    // 3. Calculate page boundaries based on break properties
    // 4. Slice content to page boundaries (items are NEVER shifted, only clipped)
    // 5. Headers and footers are injected per-page
    //
    // Benefits over the old approach:
    // - No coordinate desynchronization between page_index and actual Y position
    // - Backgrounds render correctly (clipped, not torn/duplicated)
    // - Simple mental model: pages are just views into continuous content
    // - Headers/footers with page numbers are automatically generated
    // - CSS fragmentation properties are respected

    use crate::solver3::display_list::{
        generate_display_list, paginate_display_list_with_slicer_and_breaks,
        SlicerConfig,
    };

    // Step 1: Generate ONE complete display list (infinite canvas)
    let _paged_t2 = std::time::Instant::now();
    let full_display_list = generate_display_list(
        &mut ctx,
        tree,
        calculated_positions,
        scroll_offsets,
        scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
    )?;

    eprintln!("    [paged_layout] generate_display_list: {:?} ({} items)", _paged_t2.elapsed(), full_display_list.items.len());
    if let Some(msgs) = ctx.debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PagedLayout] Generated master display list with {} items",
            full_display_list.items.len()
        )));
    }

    // Step 2: Configure the slicer with page dimensions and headers/footers
    let page_width = viewport.size.width;
    let header_footer = page_config.to_header_footer_config();

    if let Some(msgs) = ctx.debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PagedLayout] Page config: header={}, footer={}, skip_first={}",
            header_footer.show_header, header_footer.show_footer, header_footer.skip_first_page
        )));
    }

    let slicer_config = SlicerConfig {
        page_content_height,
        page_gap: 0.0,
        allow_clipping: true,
        header_footer,
        page_width,
        table_headers: Default::default(),
    };

    // Step 3: Paginate with CSS break property support
    // Break properties (break-before, break-after) are now collected during display list
    // generation and stored in DisplayList::forced_page_breaks
    let _paged_t3 = std::time::Instant::now();
    let pages = paginate_display_list_with_slicer_and_breaks(
        full_display_list,
        &slicer_config,
    )?;
    eprintln!("    [paged_layout] paginate: {:?} ({} pages)", _paged_t3.elapsed(), pages.len());

    if let Some(msgs) = ctx.debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PagedLayout] Paginated into {} pages with CSS break support",
            pages.len()
        )));
    }

    cache.cache_map = std::mem::take(&mut ctx.cache_map);

    Ok(pages)
}

/// Internal helper: Perform layout with a fragmentation context (layout only, no display list)
///
/// Returns a `FragmentationLayoutResult` containing the computed scroll IDs.
/// The tree & positions are stored in `cache`. To generate a display list,
/// call `generate_display_list` separately using the tree/positions from the cache.
#[cfg(feature = "text_layout")]
fn compute_layout_with_fragmentation<T: ParsedFontTrait + Sync + 'static>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    fragmentation_context: &mut FragmentationContext,
    new_dom: &StyledDom,
    viewport: LogicalRect,
    font_manager: &crate::font_traits::FontManager<T>,
    selections: &BTreeMap<DomId, SelectionState>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
) -> Result<FragmentationLayoutResult> {
    use crate::solver3::{
        cache, getters::get_writing_mode,
        layout_tree::DirtyFlag,
    };

    // Create temporary context without counters for tree generation
    let mut counter_values = BTreeMap::new();
    let empty_text_selections: BTreeMap<DomId, TextSelection> = BTreeMap::new();
    let mut ctx_temp = LayoutContext {
        styled_dom: new_dom,
        font_manager,
        selections,
        text_selections: &empty_text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(fragmentation_context),
        cursor_is_visible: true, // Paged layout: cursor always visible
        cursor_location: None,   // Paged layout: no cursor
        cache_map: cache::LayoutCacheMap::default(),
        system_style: None,
        get_system_time_fn,
    };

    // --- Step 1: Tree Building & Invalidation ---
    let _frag_t0 = std::time::Instant::now();
    let is_fresh_dom = cache.tree.is_none();
    let (mut new_tree, mut recon_result) = if is_fresh_dom {
        // Fast path: no old tree to diff against — build tree directly.
        // This avoids the per-node hash comparison in reconcile_and_invalidate().
        use crate::solver3::layout_tree::generate_layout_tree;
        let new_tree = generate_layout_tree(&mut ctx_temp)?;
        let n = new_tree.nodes.len();
        let mut result = cache::ReconciliationResult::default();
        result.layout_roots.insert(new_tree.root);
        result.intrinsic_dirty = (0..n).collect::<std::collections::BTreeSet<_>>();
        (new_tree, result)
    } else {
        // Incremental path: diff old tree vs new DOM
        cache::reconcile_and_invalidate(&mut ctx_temp, cache, viewport)?
    };
    eprintln!("      [fragmentation] {} tree build: {:?} ({} nodes, {} dirty)",
        if is_fresh_dom { "fresh" } else { "reconcile" },
        _frag_t0.elapsed(), new_tree.nodes.len(), recon_result.intrinsic_dirty.len());

    // Step 1.2: Clear Taffy Caches for Dirty Nodes
    for &node_idx in &recon_result.intrinsic_dirty {
        if let Some(node) = new_tree.get_mut(node_idx) {
            node.taffy_cache.clear();
        }
    }

    // Step 1.3: Compute CSS Counters
    cache::compute_counters(new_dom, &new_tree, &mut counter_values);

    // Step 1.4: Resize and invalidate per-node cache (Taffy-inspired 9+1 slot cache)
    // Move cache_map out of LayoutCache for the duration of layout.
    let mut cache_map = std::mem::take(&mut cache.cache_map);
    cache_map.resize_to_tree(new_tree.nodes.len());
    for &node_idx in &recon_result.intrinsic_dirty {
        cache_map.mark_dirty(node_idx, &new_tree.nodes);
    }
    for &node_idx in &recon_result.layout_roots {
        cache_map.mark_dirty(node_idx, &new_tree.nodes);
    }

    // Now create the real context with computed counters and fragmentation
    let mut ctx = LayoutContext {
        styled_dom: new_dom,
        font_manager,
        selections,
        text_selections: &empty_text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(fragmentation_context),
        cursor_is_visible: true, // Paged layout: cursor always visible
        cursor_location: None,   // Paged layout: no cursor
        cache_map,
        system_style: None,
        get_system_time_fn,
    };

    // --- Step 1.5: Early Exit Optimization ---
    if recon_result.is_clean() {
        ctx.debug_log("No changes, layout cache is clean");
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;

        use crate::window::LayoutWindow;
        let (scroll_ids, scroll_id_to_node_id) = LayoutWindow::compute_scroll_ids(tree, new_dom);
        cache.scroll_ids = scroll_ids.clone();
        cache.scroll_id_to_node_id = scroll_id_to_node_id;

        return Ok(FragmentationLayoutResult {
            scroll_ids,
        });
    }

    // --- Step 2: Incremental Layout Loop ---
    let _frag_t1 = std::time::Instant::now();
    let mut calculated_positions = cache.calculated_positions.clone();
    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > 10 {
            break;
        }

        calculated_positions = cache.calculated_positions.clone();
        let mut reflow_needed_for_scrollbars = false;

        crate::solver3::sizing::calculate_intrinsic_sizes(
            &mut ctx,
            &mut new_tree,
            &recon_result.intrinsic_dirty,
        )?;

        for &root_idx in &recon_result.layout_roots {
            let (cb_pos, cb_size) = get_containing_block_for_node(
                &new_tree,
                new_dom,
                root_idx,
                &calculated_positions,
                viewport,
            );

            // For ROOT nodes (no parent), we need to account for their margin.
            // The containing block position from viewport is (0, 0), but the root's
            // content starts at (margin + border + padding, margin + border + padding).
            let root_node = &new_tree.nodes[root_idx];
            let is_root_with_margin = root_node.parent.is_none()
                && (root_node.box_props.margin.left != 0.0 || root_node.box_props.margin.top != 0.0);

            let adjusted_cb_pos = if is_root_with_margin {
                LogicalPosition::new(
                    cb_pos.x + root_node.box_props.margin.left,
                    cb_pos.y + root_node.box_props.margin.top,
                )
            } else {
                cb_pos
            };

            cache::calculate_layout_for_subtree(
                &mut ctx,
                &mut new_tree,
                text_cache,
                root_idx,
                adjusted_cb_pos,
                cb_size,
                &mut calculated_positions,
                &mut reflow_needed_for_scrollbars,
                &mut cache.float_cache,
                cache::ComputeMode::PerformLayout,
            )?;

            // For root nodes, the position should be at (margin.left, margin.top) relative
            // to the viewport origin, because the margin creates space between the viewport
            // edge and the element's border-box.
            if !super::pos_contains(&calculated_positions, root_idx) {
                let root_position = if is_root_with_margin {
                    adjusted_cb_pos
                } else {
                    cb_pos
                };
                super::pos_set(&mut calculated_positions, root_idx, root_position);
            }
        }

        cache::reposition_clean_subtrees(
            new_dom,
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

    eprintln!("      [fragmentation] layout loop: {:?} ({} iterations)", _frag_t1.elapsed(), loop_count);

    // --- Step 3: Adjust Positions ---
    let _frag_t2 = std::time::Instant::now();
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

    eprintln!("      [fragmentation] position adjustments: {:?}", _frag_t2.elapsed());

    // --- Step 3.75: Compute Stable Scroll IDs ---
    use crate::window::LayoutWindow;
    let (scroll_ids, scroll_id_to_node_id) = LayoutWindow::compute_scroll_ids(&new_tree, new_dom);

    // --- Step 4: Update Cache ---
    let cache_map_back = std::mem::take(&mut ctx.cache_map);

    cache.tree = Some(new_tree);
    cache.calculated_positions = calculated_positions;
    cache.viewport = Some(viewport);
    cache.scroll_ids = scroll_ids.clone();
    cache.scroll_id_to_node_id = scroll_id_to_node_id;
    cache.counters = counter_values;
    cache.cache_map = cache_map_back;

    Ok(FragmentationLayoutResult {
        scroll_ids,
    })
}

// Helper function (copy from mod.rs)
fn get_containing_block_for_node(
    tree: &crate::solver3::layout_tree::LayoutTree,
    styled_dom: &StyledDom,
    node_idx: usize,
    calculated_positions: &super::PositionVec,
    viewport: LogicalRect,
) -> (LogicalPosition, LogicalSize) {
    use crate::solver3::getters::get_writing_mode;

    if let Some(parent_idx) = tree.get(node_idx).and_then(|n| n.parent) {
        if let Some(parent_node) = tree.get(parent_idx) {
            let pos = calculated_positions
                .get(parent_idx)
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
                    .map(|n| &n.styled_node_state)
                    .cloned()
                    .unwrap_or_default();
                let writing_mode =
                    get_writing_mode(styled_dom, dom_id, styled_node_state).unwrap_or_default();
                let content_size = parent_node.box_props.inner_size(size, writing_mode);
                return (content_pos, content_size);
            }

            return (content_pos, size);
        }
    }
    
    // For ROOT nodes: the containing block is the viewport.
    // Do NOT subtract margin here - margins are handled in calculate_used_size().
    (viewport.origin, viewport.size)
}
