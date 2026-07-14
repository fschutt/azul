//! CSS Paged Media layout integration with integrated fragmentation
//!
//! This module provides functionality for laying out documents with pagination,
//! such as for PDF generation. It uses the new integrated architecture where:
//!
//! 1. `page_index` is assigned to nodes DURING layout based on Y position
//! 2. `generate_display_lists_paged()` creates per-page `DisplayLists` by filtering
//! 3. No post-hoc fragmentation is needed
//!
//! **Note**: Full CSS `@page` rule parsing is not yet implemented. The `FakePageConfig`
//! provides programmatic control over page decoration as a temporary solution.

use crate::debug_log;
use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    resources::RendererResources,
    selection::TextSelection,
    styled_dom::StyledDom,
};
use azul_css::LayoutDebugMessage;

use crate::{
    font_traits::{ParsedFontTrait, TextLayoutCache},
    paged::FragmentationContext,
    solver3::{
        cache::LayoutCache,
        display_list::DisplayList,
        pagination::FakePageConfig,
        LayoutContext, LayoutError, Result,
    },
};

/// Layout a document with integrated pagination, returning one `DisplayList` per page.
///
/// +spec:positioning:a4936a - Absolutely positioned elements positioned relative to containing block ignoring page breaks
/// Layout is performed on a continuous document; pages are split afterward by Y position,
/// so absolutely positioned elements are positioned as if the document were continuous.
///
/// This function performs CSS Paged Media layout with fragmentation integrated
/// into the layout process itself, using the new architecture where:
///
/// 1. The `FragmentationContext` is passed to `layout_document` via `LayoutContext`
/// 2. Nodes get their `page_index` assigned during layout based on absolute Y position
/// 3. `DisplayLists` are generated per-page by filtering items based on page bounds
///
/// Uses default page header/footer configuration (page numbers in footer).
/// For custom headers/footers, use `layout_document_paged_with_config`.
///
/// # Arguments
/// * `fragmentation_context` - Controls page size and fragmentation behavior
/// * Other arguments same as `layout_document()`
///
/// # Returns
/// A vector of `DisplayLists`, one per page. Each `DisplayList` contains the
/// elements that fit on that page, with Y-coordinates relative to the page origin.
#[cfg(feature = "text_layout")]
/// # Errors
///
/// Returns a `LayoutError` if paged layout fails.
pub fn layout_document_paged<T, F>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    fragmentation_context: FragmentationContext,
    new_dom: &StyledDom,
    viewport: LogicalRect,
    font_manager: &mut crate::font_traits::FontManager<T>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
    font_loader: F,
    image_cache: &azul_core::resources::ImageCache,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
) -> Result<Vec<DisplayList>>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(
        std::sync::Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> std::result::Result<T, crate::text3::cache::LayoutError>,
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
        debug_messages,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
        font_loader,
        page_config,
        image_cache,
        get_system_time_fn,
        false,
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
// page_config is a small owned config struct passed once per paged-layout invocation by the
// dll PDF backend and the test suite; taking it by value keeps that one-shot API ergonomic.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if paged layout fails.
pub fn layout_document_paged_with_config<T, F>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    mut fragmentation_context: FragmentationContext,
    new_dom: &StyledDom,
    viewport: LogicalRect,
    font_manager: &mut crate::font_traits::FontManager<T>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
    font_loader: F,
    page_config: FakePageConfig,
    image_cache: &azul_core::resources::ImageCache,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
    print_timing: bool,
) -> Result<Vec<DisplayList>>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(
        std::sync::Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> std::result::Result<T, crate::text3::cache::LayoutError>,
{
    use crate::solver3::display_list::{
        generate_display_list, paginate_display_list_with_slicer_and_breaks, SlicerConfig,
    };

    // Font Resolution And Loading
    {
        use crate::solver3::getters::{
            collect_and_resolve_font_chains_with_registration, collect_font_ids_from_chains,
            compute_fonts_to_load, load_fonts_from_disk,
        };

        // TODO: Accept platform as parameter instead of using ::current()
        let platform = azul_css::system::Platform::current();

        let chains = collect_and_resolve_font_chains_with_registration(
            new_dom, &font_manager.fc_cache, font_manager, &platform,
        );

        let required_fonts = collect_font_ids_from_chains(&chains);
        let already_loaded = font_manager.get_loaded_font_ids();
        let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);

        if !fonts_to_load.is_empty() {
            let load_result =
                load_fonts_from_disk(&fonts_to_load, &font_manager.fc_cache, &font_loader);

            font_manager.insert_fonts(load_result.loaded);
            for (font_id, error) in &load_result.failed {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::warning(format!(
                        "[FontLoading] Failed to load font {font_id:?}: {error}"
                    )));
                }
            }
        }
        font_manager.set_font_chain_cache(chains.into_fontconfig_chains());
    }

    // Get page dimensions from fragmentation context
    let page_content_height = fragmentation_context.page_content_height();

    // Handle continuous media (no pagination)
    if !fragmentation_context.is_paged() {
        compute_layout_with_fragmentation(
            cache,
            text_cache,
            &mut fragmentation_context,
            new_dom,
            viewport,
            font_manager,
            debug_messages,
            image_cache,
            get_system_time_fn,
            print_timing,
        )?;

        // Generate display list from cached tree/positions
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;
        let mut counter_values = cache.counters.clone();
        let empty_text_selections: BTreeMap<DomId, TextSelection> = BTreeMap::new();
        let mut ctx = LayoutContext {
            scrollbar_style_cache: core::cell::RefCell::new(std::collections::HashMap::new()),
            styled_dom: new_dom,
            font_manager: &*font_manager,
            text_selections: &empty_text_selections,
            debug_messages,
            counters: &mut counter_values,
            viewport_size: viewport.size,
            fragmentation_context: Some(&mut fragmentation_context),
            cursor_is_visible: true,
            cursor_locations: Vec::new(),
            preedit_text: None,
            dirty_text_overrides: BTreeMap::new(),
            cache_map: std::mem::take(&mut cache.cache_map),
            image_cache,
            system_style: None,
            get_system_time_fn,
        };

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
    compute_layout_with_fragmentation(
        cache,
        text_cache,
        &mut fragmentation_context,
        new_dom,
        viewport,
        font_manager,
        debug_messages,
        image_cache,
        get_system_time_fn,
        print_timing,
    )?;

    // Get the layout tree and positions
    let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;
    let calculated_positions = &cache.calculated_positions;

    // Debug: log page layout info
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PagedLayout] Page content height: {page_content_height}"
        )));
    }

    // Use scroll IDs computed by compute_layout_with_fragmentation (stored in cache)
    let scroll_ids = &cache.scroll_ids;

    // Create temporary context for display list generation
    let mut counter_values = cache.counters.clone();
    let empty_text_selections: BTreeMap<DomId, TextSelection> = BTreeMap::new();
    let mut ctx = LayoutContext {
        scrollbar_style_cache: core::cell::RefCell::new(std::collections::HashMap::new()),
        styled_dom: new_dom,
        font_manager: &*font_manager,
        text_selections: &empty_text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(&mut fragmentation_context),
        cursor_is_visible: true, // Paged layout: cursor always visible
        cursor_locations: Vec::new(),   // Paged layout: no cursor
        preedit_text: None,
        dirty_text_overrides: BTreeMap::new(),
        cache_map: std::mem::take(&mut cache.cache_map),
        image_cache,
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

    // Step 1: Generate ONE complete display list (infinite canvas)
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
        table_headers: crate::solver3::pagination::TableHeaderTracker::default(),
    };

    // Step 3: Paginate with CSS break property support
    let pages = paginate_display_list_with_slicer_and_breaks(
        full_display_list,
        &slicer_config,
        renderer_resources,
    )?;

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
/// The tree, positions, and scroll IDs are stored in `cache`. To generate a display list,
/// call `generate_display_list` separately using the tree/positions from the cache.
#[cfg(feature = "text_layout")]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn compute_layout_with_fragmentation<T: ParsedFontTrait + Sync + 'static>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    fragmentation_context: &mut FragmentationContext,
    new_dom: &StyledDom,
    viewport: LogicalRect,
    font_manager: &crate::font_traits::FontManager<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    image_cache: &azul_core::resources::ImageCache,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
    _print_timing: bool,
) -> Result<()> {
    use crate::solver3::cache;
    use crate::window::LayoutWindow;

    // Create temporary context without counters for tree generation
    let mut counter_values = std::collections::HashMap::new();
    let empty_text_selections: BTreeMap<DomId, TextSelection> = BTreeMap::new();
    let mut ctx_temp = LayoutContext {
        scrollbar_style_cache: core::cell::RefCell::new(std::collections::HashMap::new()),
        styled_dom: new_dom,
        font_manager,
        text_selections: &empty_text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(fragmentation_context),
        cursor_is_visible: true, // Paged layout: cursor always visible
        cursor_locations: Vec::new(),   // Paged layout: no cursor
        preedit_text: None,
        dirty_text_overrides: BTreeMap::new(),
        cache_map: cache::LayoutCacheMap::default(),
        image_cache,
        system_style: None,
        get_system_time_fn,
    };

    // --- Step 1: Tree Building & Invalidation ---
    let is_fresh_dom = cache.tree.is_none();
    let (mut new_tree, mut recon_result) = if is_fresh_dom {
        // Fast path: no old tree to diff against — build tree directly.
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

    // Step 1.2: Clear Taffy Caches for Dirty Nodes
    for &node_idx in &recon_result.intrinsic_dirty {
        if let Some(warm) = new_tree.warm_mut(node_idx) {
            warm.taffy_cache.clear();
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
        scrollbar_style_cache: core::cell::RefCell::new(std::collections::HashMap::new()),
        styled_dom: new_dom,
        font_manager,
        text_selections: &empty_text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(fragmentation_context),
        cursor_is_visible: true, // Paged layout: cursor always visible
        cursor_locations: Vec::new(),   // Paged layout: no cursor
        preedit_text: None,
        dirty_text_overrides: BTreeMap::new(),
        cache_map,
        image_cache,
        system_style: None,
        get_system_time_fn,
    };

    // --- Step 1.5: Early Exit Optimization ---
    if recon_result.is_clean() {
        debug_log!(ctx, "No changes, layout cache is clean");
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;

        let (scroll_ids, scroll_id_to_node_id) = LayoutWindow::compute_scroll_ids(tree, new_dom);
        cache.scroll_ids = scroll_ids;
        cache.scroll_id_to_node_id = scroll_id_to_node_id;

        return Ok(());
    }

    // --- Step 2: Incremental Layout Loop ---
    let mut calculated_positions = cache.calculated_positions.clone();
    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > 10 {
            break;
        }

        calculated_positions.clone_from(&cache.calculated_positions);
        let mut reflow_needed_for_scrollbars = false;

        crate::solver3::sizing::calculate_intrinsic_sizes(
            &mut ctx,
            &mut new_tree,
            text_cache,
            &recon_result.intrinsic_dirty,
        )?;

        for &root_idx in &recon_result.layout_roots {
            let (cb_pos, cb_size) = super::get_containing_block_for_node(
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
            let root_bp = root_node.box_props.unpack();
            let is_root_with_margin = root_node.parent.is_none()
                && (root_bp.margin.left != 0.0 || root_bp.margin.top != 0.0);

            let adjusted_cb_pos = if is_root_with_margin {
                LogicalPosition::new(
                    cb_pos.x + root_bp.margin.left,
                    cb_pos.y + root_bp.margin.top,
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
            debug_log!(ctx, "Scrollbars changed container size, starting full reflow...");
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
    );

    crate::solver3::positioning::position_out_of_flow_elements(
        &mut ctx,
        &mut new_tree,
        text_cache,
        &mut calculated_positions,
        viewport,
    );

    // --- Step 3.75: Compute Stable Scroll IDs ---
    let (scroll_ids, scroll_id_to_node_id) = LayoutWindow::compute_scroll_ids(&new_tree, new_dom);

    // --- Step 4: Update Cache ---
    let cache_map_back = std::mem::take(&mut ctx.cache_map);

    cache.tree = Some(new_tree);
    cache.previous_positions = std::mem::replace(&mut cache.calculated_positions, calculated_positions);
    cache.viewport = Some(viewport);
    cache.scroll_ids = scroll_ids;
    cache.scroll_id_to_node_id = scroll_id_to_node_id;
    cache.counters = counter_values;
    cache.cache_map = cache_map_back;

    Ok(())
}

#[cfg(all(test, feature = "text_layout"))]
#[allow(clippy::float_cmp)]
mod autotest_generated {
    use azul_core::{
        dom::Dom,
        resources::{IdNamespace, ImageCache},
        task::{get_system_time_libstd, GetSystemTimeCallback},
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    use crate::{font_traits::FontManager, text3::default::PathLoader};

    // ---------------------------------------------------------------------
    // Harness
    //
    // Every DOM below is deliberately TEXT-FREE, so no font ever has to be
    // resolved and the font cache can stay empty (no system-font I/O, so the
    // tests are hermetic and identical on every machine).
    // ---------------------------------------------------------------------

    /// The crate's only `ParsedFontTrait` impl (`text3::default`).
    type TestFont = azul_css::props::basic::FontRef;

    fn time_fn() -> GetSystemTimeCallback {
        GetSystemTimeCallback {
            cb: get_system_time_libstd,
        }
    }

    fn font_manager() -> FontManager<TestFont> {
        FontManager::new(FcFontCache::default()).expect("FontManager::new must not fail")
    }

    fn viewport(width: f32, height: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::new(width, height),
        }
    }

    fn paged(width: f32, height: f32) -> FragmentationContext {
        FragmentationContext::new_paged(LogicalSize::new(width, height))
    }

    /// `<body>` with `n` painted, 200px-tall divs — a document ~`n * 200`px tall.
    /// The background is what makes each div emit a display-list item, and the
    /// paginator derives the document height from those items.
    fn doc(n: usize) -> StyledDom {
        let children: Vec<Dom> = (0..n).map(|_| Dom::create_div()).collect();
        let mut dom = Dom::create_body().with_children(children.into());
        let css = azul_css::parser2::new_from_str(
            "div { height: 200px; width: 100px; background-color: red; }",
        )
        .0;
        StyledDom::create(&mut dom, css)
    }

    fn run_with(
        cache: &mut LayoutCache,
        font_manager: &mut FontManager<TestFont>,
        fragmentation_context: FragmentationContext,
        dom: &StyledDom,
        vp: LogicalRect,
        page_config: FakePageConfig,
    ) -> Result<Vec<DisplayList>> {
        let loader = PathLoader::new();
        let mut text_cache = TextLayoutCache::new();
        let mut debug_messages = None;
        layout_document_paged_with_config(
            cache,
            &mut text_cache,
            fragmentation_context,
            dom,
            vp,
            font_manager,
            &BTreeMap::new(),
            &mut debug_messages,
            None,
            &RendererResources::default(),
            IdNamespace(0),
            DomId::ROOT_ID,
            |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
                loader.load_font_shared(bytes, index)
            },
            page_config,
            &ImageCache::default(),
            time_fn(),
            false,
        )
    }

    /// One-shot paged layout against a fresh cache.
    fn run(
        fragmentation_context: FragmentationContext,
        dom: &StyledDom,
        vp: LogicalRect,
        page_config: FakePageConfig,
    ) -> Result<Vec<DisplayList>> {
        let mut cache = LayoutCache::default();
        let mut font_manager = font_manager();
        run_with(
            &mut cache,
            &mut font_manager,
            fragmentation_context,
            dom,
            vp,
            page_config,
        )
    }

    /// Number of pages for a fresh, default-configured paged layout.
    fn page_count(fragmentation_context: FragmentationContext, dom: &StyledDom, vp: LogicalRect) -> usize {
        run(fragmentation_context, dom, vp, FakePageConfig::new())
            .expect("paged layout must not fail")
            .len()
    }

    fn item_counts(pages: &[DisplayList]) -> Vec<usize> {
        pages.iter().map(|p| p.items.len()).collect()
    }

    fn compute(
        cache: &mut LayoutCache,
        fragmentation_context: &mut FragmentationContext,
        dom: &StyledDom,
        vp: LogicalRect,
    ) -> Result<()> {
        let font_manager = font_manager();
        let mut text_cache = TextLayoutCache::new();
        let mut debug_messages = None;
        compute_layout_with_fragmentation(
            cache,
            &mut text_cache,
            fragmentation_context,
            dom,
            vp,
            &font_manager,
            &mut debug_messages,
            &ImageCache::default(),
            time_fn(),
            false,
        )
    }

    fn tree_node_count(cache: &LayoutCache) -> usize {
        cache.tree.as_ref().expect("layout must cache a tree").nodes.len()
    }

    // ---------------------------------------------------------------------
    // Baseline invariants
    //
    // NOTE (not tested here — the assertions would hang the suite):
    // `calculate_page_break_positions` (display_list.rs) advances by
    // `y += normal_page_height` while `y < total_height`. Two reachable
    // inputs make that loop non-terminating while pushing into an unbounded
    // Vec (hang → OOM), and both are reachable from these two entry points:
    //   1. a tiny positive page height (e.g. 1e-30) — it clears the
    //      `page_content_height <= 0.0` guard, but `y += 1e-30` stops moving
    //      `y` as soon as the step falls below `y`'s ULP;
    //   2. `skip_first_page(true)` with `header_height + footer_height`
    //      >= the page height — `normal_page_height` goes negative, so `y`
    //      walks *backwards* away from `total_height` forever.
    // The tests below stay strictly on the safe side of both, and the guarded
    // variants (0 / negative / NaN / inf / f32::MAX heights, and an oversized
    // header WITHOUT skip_first_page) are asserted instead.
    // ---------------------------------------------------------------------

    #[test]
    fn continuous_context_returns_exactly_one_display_list() {
        let dom = doc(5);
        let pages = run(
            FragmentationContext::new_continuous(600.0),
            &dom,
            viewport(600.0, 400.0),
            FakePageConfig::new(),
        )
        .expect("continuous layout must not fail");

        assert_eq!(pages.len(), 1, "continuous media is never paginated");
        assert!(
            !pages[0].items.is_empty(),
            "painted divs must produce display-list items — the rest of this \
             module's page-count assertions depend on it"
        );
    }

    #[test]
    fn tall_document_splits_into_multiple_pages() {
        // ~1000px of content, 200px pages.
        let pages = run(
            paged(600.0, 200.0),
            &doc(5),
            viewport(600.0, 400.0),
            FakePageConfig::new(),
        )
        .expect("paged layout must not fail");

        assert!(
            pages.len() >= 2,
            "1000px of content on 200px pages must paginate, got {} page(s)",
            pages.len()
        );
    }

    #[test]
    fn empty_document_still_yields_one_page() {
        // A document with nothing to paint has height 0 — the paginator must
        // still hand back a page rather than an empty vec (a zero-page PDF).
        let pages = run(
            paged(600.0, 200.0),
            &StyledDom::default(),
            viewport(600.0, 400.0),
            FakePageConfig::new(),
        )
        .expect("empty document must lay out");

        assert_eq!(pages.len(), 1, "a zero-height document is still one page");
    }

    // ---------------------------------------------------------------------
    // Numeric: degenerate page sizes (zero / negative / NaN / inf / MIN / MAX)
    // ---------------------------------------------------------------------

    #[test]
    fn zero_page_height_yields_a_single_page() {
        let pages = run(
            paged(600.0, 0.0),
            &doc(5),
            viewport(600.0, 400.0),
            FakePageConfig::new(),
        )
        .expect("a zero-height page must not fail layout");

        assert_eq!(
            pages.len(),
            1,
            "a page of height 0 cannot be filled — the slicer must bail out to \
             a single unpaginated page instead of dividing by zero"
        );
    }

    #[test]
    fn zero_page_size_in_both_axes_does_not_panic() {
        let pages = run(
            paged(0.0, 0.0),
            &doc(3),
            viewport(0.0, 0.0),
            FakePageConfig::new(),
        )
        .expect("a fully degenerate 0x0 page must not fail layout");

        assert_eq!(pages.len(), 1);
    }

    #[test]
    fn negative_page_height_yields_a_single_page() {
        let pages = run(
            paged(600.0, -500.0),
            &doc(5),
            viewport(600.0, 400.0),
            FakePageConfig::new(),
        )
        .expect("a negative page height must not fail layout");

        assert_eq!(
            pages.len(),
            1,
            "a negative page height must not produce a negative/infinite page count"
        );
    }

    #[test]
    fn nan_page_size_does_not_panic_and_yields_at_least_one_page() {
        // NaN slips past BOTH `<= 0.0` and `>= f32::MAX` guards (every NaN
        // comparison is false), so this is the case most likely to reach the
        // break-position math with a poisoned step.
        let pages = run(
            paged(f32::NAN, f32::NAN),
            &doc(5),
            viewport(600.0, 400.0),
            FakePageConfig::new(),
        )
        .expect("a NaN page size must not fail layout");

        assert_eq!(
            pages.len(),
            1,
            "a NaN page height cannot advance the break cursor, so the whole \
             document must stay on one page (and the break sort must not see a NaN)"
        );
    }

    #[test]
    fn infinite_page_height_yields_a_single_page() {
        let pages = run(
            paged(600.0, f32::INFINITY),
            &doc(5),
            viewport(600.0, 400.0),
            FakePageConfig::new(),
        )
        .expect("an infinite page height must not fail layout");

        assert_eq!(pages.len(), 1, "an infinitely tall page holds everything");
    }

    #[test]
    fn f32_max_page_height_yields_a_single_page() {
        // f32::MAX is the sentinel `FragmentationContext::Continuous` reports,
        // so a *paged* context carrying it must degrade to the same behaviour
        // rather than attempting MAX/step pages.
        let pages = run(
            paged(600.0, f32::MAX),
            &doc(5),
            viewport(600.0, 400.0),
            FakePageConfig::new(),
        )
        .expect("f32::MAX page height must not fail layout");

        assert_eq!(pages.len(), 1);
    }

    #[test]
    fn f32_min_page_height_yields_a_single_page() {
        // f32::MIN is the most-negative finite float, not the smallest positive.
        let pages = run(
            paged(f32::MIN, f32::MIN),
            &doc(5),
            viewport(600.0, 400.0),
            FakePageConfig::new(),
        )
        .expect("f32::MIN page size must not fail layout");

        assert_eq!(pages.len(), 1);
    }

    // ---------------------------------------------------------------------
    // Numeric: degenerate viewports
    // ---------------------------------------------------------------------

    #[test]
    fn negative_viewport_size_does_not_panic() {
        let pages = run(
            paged(600.0, 200.0),
            &doc(5),
            viewport(-100.0, -100.0),
            FakePageConfig::new(),
        )
        .expect("a negative viewport must not fail layout");

        assert!(!pages.is_empty(), "layout must always emit at least one page");
    }

    #[test]
    fn nan_viewport_does_not_panic() {
        let pages = run(
            paged(600.0, 200.0),
            &doc(5),
            viewport(f32::NAN, f32::NAN),
            FakePageConfig::new(),
        )
        .expect("a NaN viewport must not fail layout");

        assert!(!pages.is_empty(), "layout must always emit at least one page");
    }

    #[test]
    fn huge_viewport_does_not_panic() {
        // Paired with an f32::MAX page height on purpose: pagination short-circuits,
        // so this exercises the layout/display-list path at the numeric limit
        // without asking the slicer to walk MAX-sized content in finite steps.
        let result = run(
            paged(f32::MAX, f32::MAX),
            &doc(3),
            viewport(f32::MAX, f32::MAX),
            FakePageConfig::new(),
        );

        match result {
            Ok(pages) => assert_eq!(pages.len(), 1),
            // Failing cleanly at the numeric limit is acceptable; panicking is not.
            Err(e) => {
                let _ = e.to_string();
            }
        }
    }

    // ---------------------------------------------------------------------
    // Numeric: monotonicity of the page count
    // ---------------------------------------------------------------------

    #[test]
    fn shorter_pages_never_produce_fewer_pages() {
        let dom = doc(5);
        let vp = viewport(600.0, 400.0);

        let tall = page_count(paged(600.0, 400.0), &dom, vp);
        let short = page_count(paged(600.0, 100.0), &dom, vp);

        assert!(
            short >= tall,
            "halving the page height must not shrink the page count ({short} < {tall})"
        );
    }

    #[test]
    fn more_content_never_produces_fewer_pages() {
        let vp = viewport(600.0, 400.0);
        let frag = paged(600.0, 200.0);

        let few = page_count(frag, &doc(3), vp);
        let many = page_count(frag, &doc(12), vp);

        assert!(
            many >= few,
            "4x the content must not shrink the page count ({many} < {few})"
        );
    }

    // ---------------------------------------------------------------------
    // Headers / footers
    // ---------------------------------------------------------------------

    #[test]
    fn header_and_footer_taller_than_the_page_yield_a_single_page() {
        // header + footer >= page height leaves negative room for content.
        // Without `skip_first_page`, the first-page height goes <= 0 and the
        // slicer must bail out to one page rather than dividing the document
        // into a negative-height grid.
        let config = FakePageConfig::new()
            .with_header_page_numbers()
            .with_footer_page_numbers()
            .with_header_height(f32::MAX)
            .with_footer_height(f32::MAX);

        let pages = run(paged(600.0, 200.0), &doc(5), viewport(600.0, 400.0), config)
            .expect("an oversized header/footer must not fail layout");

        assert_eq!(
            pages.len(),
            1,
            "no content fits once the header/footer exceed the page — one page, not zero, \
             not an unbounded number"
        );
    }

    #[test]
    fn skip_first_page_with_sane_header_and_footer_still_paginates() {
        let config = FakePageConfig::new()
            .with_header_and_footer_page_numbers()
            .with_header_height(20.0)
            .with_footer_height(20.0)
            .skip_first_page(true);

        let pages = run(paged(600.0, 300.0), &doc(5), viewport(600.0, 400.0), config)
            .expect("paged layout with headers/footers must not fail");

        assert!(
            pages.len() >= 2,
            "1000px of content on 300px pages (260px usable after the first) must \
             paginate, got {} page(s)",
            pages.len()
        );
    }

    // ---------------------------------------------------------------------
    // Determinism / cache reuse / wrapper equivalence
    // ---------------------------------------------------------------------

    #[test]
    fn paged_layout_is_deterministic_across_fresh_runs() {
        let dom = doc(5);
        let vp = viewport(600.0, 400.0);
        let frag = paged(600.0, 200.0);

        let first = run(frag, &dom, vp, FakePageConfig::new()).expect("layout must not fail");
        let second = run(frag, &dom, vp, FakePageConfig::new()).expect("layout must not fail");

        assert_eq!(first.len(), second.len(), "page count must be deterministic");
        assert_eq!(
            item_counts(&first),
            item_counts(&second),
            "per-page item counts must be deterministic"
        );
    }

    #[test]
    fn reusing_a_warm_cache_reproduces_the_cold_result() {
        // Adversarial: the second call takes the incremental/early-exit path
        // through `compute_layout_with_fragmentation`. Same DOM, same viewport,
        // same page size => byte-identical pagination, or the cache is stale.
        let dom = doc(5);
        let vp = viewport(600.0, 400.0);
        let frag = paged(600.0, 200.0);

        let mut cache = LayoutCache::default();
        let mut fm = font_manager();

        let cold = run_with(&mut cache, &mut fm, frag, &dom, vp, FakePageConfig::new())
            .expect("cold layout must not fail");
        let warm = run_with(&mut cache, &mut fm, frag, &dom, vp, FakePageConfig::new())
            .expect("warm layout must not fail");

        assert_eq!(cold.len(), warm.len(), "cache reuse changed the page count");
        assert_eq!(
            item_counts(&cold),
            item_counts(&warm),
            "cache reuse changed the per-page item counts"
        );
    }

    #[test]
    fn a_reused_cache_relaid_out_with_a_different_dom_matches_a_cold_run() {
        // Adversarial: feed a cache warmed on a SHORT document a much longer
        // one. The reconciled result must equal what a cold cache produces —
        // page count must not depend on layout history.
        let vp = viewport(600.0, 400.0);
        let frag = paged(600.0, 200.0);
        let short = doc(2);
        let long = doc(9);

        let mut cache = LayoutCache::default();
        let mut fm = font_manager();
        let _ = run_with(&mut cache, &mut fm, frag, &short, vp, FakePageConfig::new())
            .expect("first layout must not fail");
        let reused = run_with(&mut cache, &mut fm, frag, &long, vp, FakePageConfig::new())
            .expect("relayout must not fail");

        let cold = page_count(frag, &long, vp);

        assert_eq!(
            reused.len(),
            cold,
            "a cache warmed on a 2-div document produced {} page(s) for the 9-div \
             document, but a cold cache produces {}",
            reused.len(),
            cold
        );
    }

    #[test]
    fn layout_document_paged_matches_its_documented_default_config() {
        // `layout_document_paged` is documented as `..._with_config` with
        // footer page numbers and no timing output. Assert that equivalence
        // holds, so the wrapper can't silently drift from the delegate.
        let dom = doc(5);
        let vp = viewport(600.0, 400.0);
        let frag = paged(600.0, 200.0);

        let mut cache = LayoutCache::default();
        let mut text_cache = TextLayoutCache::new();
        let mut fm = font_manager();
        let mut debug_messages = None;
        let loader = PathLoader::new();

        let via_wrapper = layout_document_paged(
            &mut cache,
            &mut text_cache,
            frag,
            &dom,
            vp,
            &mut fm,
            &BTreeMap::new(),
            &mut debug_messages,
            None,
            &RendererResources::default(),
            IdNamespace(0),
            DomId::ROOT_ID,
            |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
                loader.load_font_shared(bytes, index)
            },
            &ImageCache::default(),
            time_fn(),
        )
        .expect("layout_document_paged must not fail");

        let via_config = run(
            frag,
            &dom,
            vp,
            FakePageConfig::new().with_footer_page_numbers(),
        )
        .expect("layout_document_paged_with_config must not fail");

        assert_eq!(via_wrapper.len(), via_config.len());
        assert_eq!(item_counts(&via_wrapper), item_counts(&via_config));
    }

    // ---------------------------------------------------------------------
    // compute_layout_with_fragmentation (private)
    // ---------------------------------------------------------------------

    #[test]
    fn compute_layout_with_fragmentation_populates_the_cache() {
        let dom = doc(3);
        let vp = viewport(600.0, 400.0);
        let mut cache = LayoutCache::default();
        let mut frag = paged(600.0, 200.0);

        compute(&mut cache, &mut frag, &dom, vp).expect("layout must not fail");

        assert!(cache.tree.is_some(), "the layout tree must be cached");
        assert!(
            tree_node_count(&cache) >= 4,
            "<body> plus 3 <div>s is at least 4 layout nodes"
        );
        assert!(
            !cache.calculated_positions.is_empty(),
            "positions must be cached alongside the tree"
        );
        assert_eq!(cache.viewport, Some(vp), "the layout viewport must be recorded");
        assert!(
            crate::solver3::pos_get(&cache.calculated_positions, 0).is_some(),
            "the root node must have a position"
        );
    }

    #[test]
    fn compute_layout_with_fragmentation_is_idempotent() {
        let dom = doc(3);
        let vp = viewport(600.0, 400.0);
        let mut cache = LayoutCache::default();
        let mut frag = paged(600.0, 200.0);

        compute(&mut cache, &mut frag, &dom, vp).expect("first layout must not fail");
        let nodes = tree_node_count(&cache);
        let positions = cache.calculated_positions.clone();

        // Second pass takes the "cache is clean" early-exit branch.
        compute(&mut cache, &mut frag, &dom, vp).expect("second layout must not fail");

        assert_eq!(tree_node_count(&cache), nodes, "relayout changed the tree size");
        assert_eq!(
            cache.calculated_positions, positions,
            "relayout of an unchanged DOM moved nodes"
        );
    }

    #[test]
    fn compute_layout_with_fragmentation_tree_shape_is_independent_of_pagination() {
        // Layout is continuous; pages are sliced afterwards by Y position. So a
        // paged context must not add, drop, or split any layout node.
        let dom = doc(4);
        let vp = viewport(600.0, 400.0);

        let mut continuous_cache = LayoutCache::default();
        let mut continuous = FragmentationContext::new_continuous(600.0);
        compute(&mut continuous_cache, &mut continuous, &dom, vp)
            .expect("continuous layout must not fail");

        let mut paged_cache = LayoutCache::default();
        let mut paged_ctx = paged(600.0, 50.0);
        compute(&mut paged_cache, &mut paged_ctx, &dom, vp).expect("paged layout must not fail");

        assert_eq!(
            tree_node_count(&continuous_cache),
            tree_node_count(&paged_cache),
            "fragmentation must not change the layout tree"
        );
        assert_eq!(
            continuous_cache.calculated_positions, paged_cache.calculated_positions,
            "fragmentation must not move nodes — pages are sliced from the same \
             continuous canvas"
        );
    }

    #[test]
    fn compute_layout_with_fragmentation_survives_degenerate_viewports() {
        let dom = doc(3);

        for vp in [
            viewport(0.0, 0.0),
            viewport(-1.0, -1.0),
            viewport(f32::NAN, f32::NAN),
            viewport(f32::MIN, f32::MIN),
        ] {
            let mut cache = LayoutCache::default();
            let mut frag = paged(600.0, 200.0);

            compute(&mut cache, &mut frag, &dom, vp)
                .unwrap_or_else(|e| panic!("viewport {vp:?} failed layout: {e}"));

            assert!(
                cache.tree.is_some(),
                "viewport {vp:?} must still produce a layout tree"
            );
        }
    }

    #[test]
    fn compute_layout_with_fragmentation_survives_degenerate_page_sizes() {
        let dom = doc(3);
        let vp = viewport(600.0, 400.0);

        for mut frag in [
            paged(0.0, 0.0),
            paged(600.0, -1.0),
            paged(f32::NAN, f32::NAN),
            paged(f32::INFINITY, f32::INFINITY),
            paged(f32::MAX, f32::MAX),
            paged(f32::MIN, f32::MIN),
        ] {
            let mut cache = LayoutCache::default();

            compute(&mut cache, &mut frag, &dom, vp)
                .unwrap_or_else(|e| panic!("page size {frag:?} failed layout: {e}"));

            assert!(
                cache.tree.is_some(),
                "page size {frag:?} must still produce a layout tree"
            );
        }
    }
}
