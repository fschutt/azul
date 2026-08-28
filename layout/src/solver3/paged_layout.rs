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
use crate::solver3::layout_tree::LayoutNodeId;
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
        cache::LayoutCache, display_list::DisplayList, pagination::FakePageConfig, LayoutContext,
        LayoutError, Result,
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

/// The full result of a paged layout: the analysis alongside the pages.
///
/// `pages[i].node_mapping` carries the per-item source `NodeId`s of page `i`
/// (paged hit-testing / diagnostics); `breaks` is the same analysis a
/// document editor gets from [`compute_document_pagination`] without
/// materializing any page.
#[derive(Debug)]
pub struct PagedLayoutResult {
    /// One display list per page.
    pub pages: Vec<DisplayList>,
    /// The break analysis the pages were sliced by (empty for continuous media).
    pub breaks: Vec<crate::solver3::page_breaks::PageBreakPosition>,
    /// Total document-space content height of the un-sliced document.
    pub total_content_height: f32,
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
#[allow(clippy::too_many_arguments)]
/// # Errors
///
/// Returns a `LayoutError` if paged layout fails.
pub fn layout_document_paged_with_config<T, F>(
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
    // Thin wrapper over the analysis-returning entry so printpdf 0.12.x
    // compiles unchanged; migrate to `layout_document_paged_v2` to get the
    // break analysis alongside the pages.
    layout_document_paged_v2(
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
        print_timing,
    )
    .map(|r| r.pages)
}

/// [`layout_document_paged_with_config`], returning the break ANALYSIS
/// alongside the pages (the document-editor/printpdf-diagnostics upgrade).
#[cfg(feature = "text_layout")]
#[allow(clippy::too_many_arguments)]
/// # Errors
///
/// Returns a `LayoutError` if paged layout fails.
pub fn layout_document_paged_v2<T, F>(
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
    page_config: FakePageConfig,
    image_cache: &azul_core::resources::ImageCache,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
    print_timing: bool,
) -> Result<PagedLayoutResult>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(
        std::sync::Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> std::result::Result<T, crate::text3::cache::LayoutError>,
{
    layout_document_paged_impl(
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
        print_timing,
        true,
    )
}

#[cfg(feature = "text_layout")]
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn layout_document_paged_impl<T, F>(
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
    materialize_pages: bool,
) -> Result<PagedLayoutResult>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(
        std::sync::Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> std::result::Result<T, crate::text3::cache::LayoutError>,
{
    use crate::solver3::display_list::{
        calculate_display_list_height, generate_display_list, paginate_display_list_with_breaks,
        SlicerConfig,
    };
    use crate::solver3::page_breaks;

    // Font Resolution And Loading
    {
        use crate::solver3::getters::{
            collect_and_resolve_font_chains_with_registration, collect_font_ids_from_chains,
            compute_fonts_to_load, load_fonts_from_disk,
        };

        // SKIP THE RESOLVER when this DOM asks for the same font stacks the
        // manager already resolved. `LayoutWindow` has done this since the
        // beginning (window.rs, `font_requirements_unchanged`) via a rolling
        // hash of the compact cache's `prev_font_hashes`; the pagination
        // entry points did not — and worse, called the plain
        // `set_font_chain_cache`, which CLEARS the recorded signature, so
        // even a caller reusing one FontManager re-resolved a 160-family
        // chain on EVERY pagination (measured 8 ms/call, ~8% of a warm one).
        let font_stacks_sig = new_dom
            .css_property_cache
            .ptr
            .compact_cache
            .as_ref()
            .map(|cc| {
                let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                for &fh in &cc.prev_font_hashes {
                    h = h.rotate_left(13) ^ fh;
                    h = h.wrapping_mul(0x0100_0000_01b3);
                }
                h
            });
        let font_requirements_unchanged = font_stacks_sig.is_some()
            && font_stacks_sig == font_manager.last_resolved_font_stacks_sig
            && !font_manager.font_chain_cache.is_empty();

        if !font_requirements_unchanged {
            let _p = crate::probe::Probe::span("font_chain_resolve");
            let trace = std::env::var_os("AZ_PAGINATE_TRACE").is_some();
            // Clock reads are GATED ON `trace`, and use azul_core's clock rather
            // than std's, for two independent reasons:
            //
            //   * `std::time::Instant::now()` PANICS on wasm32-unknown-unknown,
            //     and azul-layout is built for wasm with `text_layout` (which
            //     turns std on, so a `feature = "std"` gate would not save it).
            //   * `azul_core::task::Instant` is FFI-shaped: it owns a
            //     `ManuallyDrop<Box<StdInstant>>`, so every `now()` is a heap
            //     allocation. Taking one unconditionally on this path made
            //     `regenerate_layout` grow 1112 B/iter under resize stress and
            //     tripped the leak regression test — which is exactly what that
            //     test is for.
            //
            // Tracing is off in every normal run, so this costs nothing there.
            let t0 = trace.then(azul_core::task::Instant::now);
            let platform = azul_css::system::Platform::current();

            let chains = collect_and_resolve_font_chains_with_registration(
                new_dom,
                &font_manager.fc_cache,
                font_manager,
                &platform,
            );
            let t_resolve = t0.map(|t0| azul_core::task::Instant::now().duration_since(&t0));

            let required_fonts = collect_font_ids_from_chains(&chains);
            let already_loaded = font_manager.get_loaded_font_ids();
            let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);
            if trace {
                eprintln!(
                    "[paginate] font_chain_resolve {t_resolve:?}: {} chain(s), {} font(s) \
                     required, {} already loaded, {} to load",
                    chains.chains.len(),
                    required_fonts.len(),
                    already_loaded.len(),
                    fonts_to_load.len(),
                );
            }

            if !fonts_to_load.is_empty() {
                let t1 = trace.then(azul_core::task::Instant::now);
                let load_result =
                    load_fonts_from_disk(&fonts_to_load, &font_manager.fc_cache, &font_loader);
                if trace {
                    eprintln!(
                        "[paginate] load_fonts_from_disk {:?}: {} loaded, {} failed",
                        t1.map(|t1| azul_core::task::Instant::now().duration_since(&t1)),
                        load_result.loaded.len(),
                        load_result.failed.len(),
                    );
                }

                font_manager.insert_fonts(load_result.loaded);
                for (font_id, error) in &load_result.failed {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "[FontLoading] Failed to load font {font_id:?}: {error}"
                        )));
                    }
                }
            }
            font_manager
                .set_font_chain_cache_with_sig(chains.into_fontconfig_chains(), font_stacks_sig);
        }
    }

    // Get page dimensions from fragmentation context
    let page_content_height = fragmentation_context.page_content_height();

    // Handle continuous media (no pagination)
    if !fragmentation_context.is_paged() {
        let _p = crate::probe::Probe::span("paged_layout_pass");
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
            reflowed_ifcs: std::collections::BTreeSet::new(),
            style_cache: Default::default(),
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
            cache_map: std::mem::take(&mut cache.cache_map),
            image_cache,
            content_overlay: None,
            system_style: None,
            get_system_time_fn,
        };

        let _p = crate::probe::Probe::span("paged_display_list");
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
        let total_content_height = calculate_display_list_height(&display_list);
        return Ok(PagedLayoutResult {
            pages: vec![display_list],
            breaks: Vec::new(),
            total_content_height,
        });
    }

    // Paged Layout

    // Perform layout with fragmentation context (layout only, no display list)
    let p_layout = crate::probe::Probe::span("paged_layout_pass");
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
        reflowed_ifcs: std::collections::BTreeSet::new(),
        style_cache: Default::default(),
        scrollbar_style_cache: core::cell::RefCell::new(std::collections::HashMap::new()),
        styled_dom: new_dom,
        font_manager: &*font_manager,
        text_selections: &empty_text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(&mut fragmentation_context),
        cursor_is_visible: true,      // Paged layout: cursor always visible
        cursor_locations: Vec::new(), // Paged layout: no cursor
        preedit_text: None,
        cache_map: std::mem::take(&mut cache.cache_map),
        image_cache,
        content_overlay: None,
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
    drop(p_layout);
    let _p_dl = crate::probe::Probe::span("paged_display_list");
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

    // B3c: with repeat_table_headers on, capture every table's thead from
    // the master display list — the registration side the tracker lacked.
    let table_headers = if page_config.break_policy.repeat_table_headers {
        crate::solver3::pagination::collect_table_headers(&full_display_list, new_dom)
    } else {
        crate::solver3::pagination::TableHeaderTracker::default()
    };

    let slicer_config = SlicerConfig {
        page_content_height,
        page_gap: 0.0,
        allow_clipping: true,
        header_footer,
        page_width,
        table_headers,
        break_policy: page_config.break_policy,
        page_sequence: page_config.page_sequence,
    };

    // Step 3: Analyze the breaks, THEN paginate against them — the analysis
    // is part of the result (document editors consume it without the pages).
    // Break-awareness runs per `slicer_config.break_policy` (all-off default
    // = the plain interval algorithm).
    let break_input = page_breaks::PageBreakInput {
        display_list: &full_display_list,
        layout_tree: cache.tree.as_ref(),
        styled_dom: new_dom,
        table_headers: Some(&slicer_config.table_headers),
    };
    let breaks = if let Some(sequence) = &slicer_config.page_sequence {
        // classic office suites model: every page's height from ITS setup.
        page_breaks::compute_page_breaks_with_sequence(
            &break_input,
            sequence,
            &slicer_config.break_policy,
        )
    } else {
        let constraints = page_breaks::PageConstraints::from_slicer_config(&slicer_config);
        page_breaks::compute_page_breaks(&break_input, &constraints, &slicer_config.break_policy)
    };
    let total_content_height = calculate_display_list_height(&full_display_list);

    let pages = if materialize_pages {
        paginate_display_list_with_breaks(
            full_display_list,
            &slicer_config,
            &breaks,
            renderer_resources,
        )?
    } else {
        // Precalculation-only: the analysis IS the result; no page is sliced.
        Vec::new()
    };

    if let Some(msgs) = ctx.debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PagedLayout] Paginated into {} pages with CSS break support",
            pages.len()
        )));
    }

    cache.cache_map = std::mem::take(&mut ctx.cache_map);

    Ok(PagedLayoutResult {
        pages,
        breaks,
        total_content_height,
    })
}

/// The PRECALCULATION-ONLY path (the document-editor requirement).
///
/// Full layout + display-list generation + break analysis, but NO per-page
/// display list is ever materialized. Pair with
/// [`crate::solver3::display_list::paginate_single_page`] to materialize
/// only visible pages, and [`crate::solver3::page_breaks::page_of_y`] to map
/// a node's Y to its page.
#[cfg(feature = "text_layout")]
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
/// # Errors
///
/// Returns a `LayoutError` if layout fails.
pub fn compute_document_pagination<T, F>(
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
    page_config: FakePageConfig,
    image_cache: &azul_core::resources::ImageCache,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
) -> Result<crate::solver3::page_breaks::PaginationInfo>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(
        std::sync::Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> std::result::Result<T, crate::text3::cache::LayoutError>,
{
    use crate::solver3::page_breaks;

    let result = layout_document_paged_impl(
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
        false, // NO page is materialized — the acceptance criterion of this entry
    )?;
    let page_count = page_breaks::page_spans(&result.breaks, result.total_content_height)
        .len()
        .max(1);
    Ok(page_breaks::PaginationInfo {
        page_count,
        breaks: result.breaks,
        total_content_height: result.total_content_height,
    })
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
        reflowed_ifcs: std::collections::BTreeSet::new(),
        style_cache: Default::default(),
        scrollbar_style_cache: core::cell::RefCell::new(std::collections::HashMap::new()),
        styled_dom: new_dom,
        font_manager,
        text_selections: &empty_text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(fragmentation_context),
        cursor_is_visible: true,      // Paged layout: cursor always visible
        cursor_locations: Vec::new(), // Paged layout: no cursor
        preedit_text: None,
        cache_map: cache::LayoutCacheMap::default(),
        image_cache,
        content_overlay: None,
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
        cache::reconcile_and_invalidate(&mut ctx_temp, cache, viewport, None)?
    };

    // Step 1.2: Clear Taffy Caches for Dirty Nodes
    for &node_idx in &recon_result.intrinsic_dirty {
        if let Some(warm) = new_tree.warm_mut(LayoutNodeId::new(node_idx)) {
            warm.taffy_cache.clear();
            warm.measured_content_sizes = (None, None);
        }
    }

    // Step 1.3: Compute CSS Counters
    {
        let _p = crate::probe::Probe::span("frag_compute_counters");
        cache::compute_counters(new_dom, &new_tree, &mut counter_values);
    }

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
        reflowed_ifcs: std::collections::BTreeSet::new(),
        style_cache: Default::default(),
        scrollbar_style_cache: core::cell::RefCell::new(std::collections::HashMap::new()),
        styled_dom: new_dom,
        font_manager,
        text_selections: &empty_text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: Some(fragmentation_context),
        cursor_is_visible: true,      // Paged layout: cursor always visible
        cursor_locations: Vec::new(), // Paged layout: no cursor
        preedit_text: None,
        cache_map,
        image_cache,
        content_overlay: None,
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
    let p_clone_pos = crate::probe::Probe::span("frag_clone_positions");
    let mut calculated_positions = cache.calculated_positions.clone();
    drop(p_clone_pos);
    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > 10 {
            break;
        }

        calculated_positions.clone_from(&cache.calculated_positions);
        let mut reflow_needed_for_scrollbars = false;

        let _p_intrinsic = crate::probe::Probe::span("frag_intrinsic_sizes");
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
            debug_log!(
                ctx,
                "Scrollbars changed container size, starting full reflow..."
            );
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
    cache.previous_positions =
        std::mem::replace(&mut cache.calculated_positions, calculated_positions);
    cache.viewport = Some(viewport);
    cache.scroll_ids = scroll_ids;
    cache.scroll_id_to_node_id = scroll_id_to_node_id;
    cache.counters = counter_values;
    cache.cache_map = cache_map_back;

    Ok(())
}

/// One width-section's pagination within a sectioned document.
#[derive(Debug, Clone)]
pub struct SectionPagination {
    /// 0-based GLOBAL index of the section's first page.
    pub first_page: usize,
    /// The width this section's content was laid out (re-wrapped) against.
    pub content_width: f32,
    /// Pagination of the section's OWN content (Y coordinates are local to
    /// the section's layout, page indices local to the section).
    pub info: crate::solver3::page_breaks::PaginationInfo,
}

/// Result of [`compute_sectioned_pagination`]: per-width-section pagination.
#[derive(Debug, Clone)]
pub struct SectionedPaginationInfo {
    pub sections: Vec<SectionPagination>,
}

impl SectionedPaginationInfo {
    /// Total page count across all sections.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.sections
            .iter()
            .map(|s| s.info.page_count)
            .sum::<usize>()
            .max(1)
    }
}

/// The child-index path (root → node) of the first block-level box whose top
/// edge sits at/after `y` — the SPINE the fragmentainer cut runs along.
///
/// `document_edit::split_dom_at_path` consumes this path to cut the
/// reconstructed document for the next section's re-wrap. Ties (equal Y)
/// resolve to the SHALLOWEST node so the cut spine stays as high as possible.
#[must_use]
pub fn spine_path_at_y(
    tree: &crate::solver3::layout_tree::LayoutTree,
    positions: &crate::solver3::PositionVec,
    styled_dom: &StyledDom,
    y: f32,
) -> Option<Vec<u32>> {
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let depth_of = |mut n: NodeId| -> u32 {
        let mut d = 0;
        while let Some(p) = hierarchy
            .get(n)
            .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
        {
            d += 1;
            n = p;
        }
        d
    };

    let mut best: Option<(f32, u32, NodeId)> = None;
    for idx in 0..tree.nodes.len() {
        let Some(node) = tree.get(LayoutNodeId::new(idx)) else {
            continue;
        };
        let Some(dom_id) = node.dom_node_id else {
            continue;
        };
        if !crate::solver3::layout_tree::is_block_level(styled_dom, dom_id) {
            continue;
        }
        let Some(pos) = crate::solver3::pos_get(positions, idx) else {
            continue;
        };
        if pos.y < y - 0.5 {
            continue;
        }
        let d = depth_of(dom_id);
        let better = match &best {
            None => true,
            Some((by, bd, _)) => pos.y < *by - 0.01 || ((pos.y - by).abs() <= 0.01 && d < *bd),
        };
        if better {
            best = Some((pos.y, d, dom_id));
        }
    }

    let (_, _, node) = best?;
    // Child-index path root → node.
    let mut path: Vec<u32> = Vec::new();
    let mut cur = node;
    while let Some(parent) = hierarchy
        .get(cur)
        .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
    {
        let mut i: u32 = 0;
        let mut c = hierarchy.get(parent).and_then(|h| h.first_child_id(parent));
        while let Some(cc) = c {
            if cc == cur {
                break;
            }
            i += 1;
            c = hierarchy
                .get(cc)
                .and_then(azul_core::styled_dom::NodeHierarchyItem::next_sibling_id);
        }
        path.push(i);
        cur = parent;
    }
    path.reverse();
    Some(path)
}

/// Materialize the tail of a [`PageSequence`] starting at `first_page` as a
/// standalone sequence (local page 0 = global `first_page`). Bounded
/// override copying; parity/first-page variation collapses into explicit
/// overrides so no offset arithmetic leaks into the break pass.
#[must_use]
fn materialize_sequence_tail(
    seq: &crate::solver3::pagination::PageSequence,
    first_page: usize,
    scan: usize,
) -> crate::solver3::pagination::PageSequence {
    let mut out = crate::solver3::pagination::PageSequence::uniform(seq.default.clone());
    for local in 0..scan {
        let setup = seq.setup_for_page(first_page + local);
        // Geometry decides pagination; decoration differences don't need an
        // override entry (the slicer reads decoration off the ORIGINAL
        // sequence by global page index).
        let differs = (setup.content_width() - out.default.content_width()).abs() >= 0.5
            || (setup.content_height() - out.default.content_height()).abs() >= 0.5;
        if differs {
            out.overrides.insert(local, setup.clone());
        }
    }
    out
}

/// Fragmentainer-flow pagination with PER-SECTION WIDTH RE-WRAP.
///
/// [`PageSequence::width_sections`] partitions pages into maximal
/// equal-width runs (the classic office suites model: page setup changes at section
/// breaks). Content lays out ONCE per section at that section's width; when
/// a section's page budget fills, the document is CUT along the spine of
/// the first block on the next page ([`spine_path_at_y`] +
/// [`crate::document_edit::split_dom_at_path`]), the tail re-styles against
/// the retained author css, and the flow continues in the next section at
/// its width. This replaces the `has_uniform_width()` degradation for the
/// document-pagination path.
///
/// Limits (staged): the cut is block-granular (a paragraph straddling a
/// section boundary moves wholly to the next section rather than splitting
/// mid-line — the classic office-suite behavior for section breaks); floats/positioned boxes
/// do not carry across sections.
///
/// # Errors
///
/// Returns a `LayoutError` if any section's layout fails.
// xml: `document_edit` (the spine-cut applier) lives behind text_layout+xml.
#[cfg(all(feature = "text_layout", feature = "xml"))]
#[allow(clippy::too_many_arguments)]
pub fn compute_sectioned_pagination<T, F>(
    styled_dom: &StyledDom,
    page_height: f32,
    font_manager: &mut crate::font_traits::FontManager<T>,
    renderer_resources: &RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
    font_loader: F,
    page_config: &FakePageConfig,
    sequence: &crate::solver3::pagination::PageSequence,
    image_cache: &azul_core::resources::ImageCache,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
) -> Result<SectionedPaginationInfo>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(
            std::sync::Arc<rust_fontconfig::FontBytes>,
            usize,
        ) -> std::result::Result<T, crate::text3::cache::LayoutError>
        + Copy,
{
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

    const SECTION_SCAN: usize = 512;
    let sections = sequence.width_sections(SECTION_SCAN);

    // Tails re-style against the ORIGINAL document's author css — a Dom
    // reconstructed from a StyledDom carries it in `.css`, but `create`'s
    // css parameter is the reliable non-lossy channel.
    let author_css = styled_dom
        .get_css_property_cache()
        .retained_author_css
        .clone();

    let mut out = SectionedPaginationInfo {
        sections: Vec::new(),
    };
    // The working document: exact styled_dom for section 0; reconstructed +
    // cut tails afterwards. The reconstruction happens lazily (only when a
    // second section actually receives content).
    let mut working: Option<azul_core::dom::Dom> = None;
    let mut working_styled: Option<StyledDom> = None;

    for (k, sec) in sections.iter().enumerate() {
        let is_last = k + 1 == sections.len();
        let section_styled: &StyledDom = working_styled.as_ref().map_or(styled_dom, |s| s);

        let content_size = LogicalSize::new(sec.content_width, page_height);
        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: content_size,
        };
        let mut cache = LayoutCache::default();
        let mut text_cache = TextLayoutCache::new();
        let frag = FragmentationContext::new_paged(content_size);
        let mut cfg = page_config.clone();
        cfg.page_sequence = Some(materialize_sequence_tail(
            sequence,
            sec.first_page,
            sec.page_count.unwrap_or(SECTION_SCAN).min(SECTION_SCAN),
        ));

        let info = compute_document_pagination(
            &mut cache,
            &mut text_cache,
            frag,
            section_styled,
            viewport,
            font_manager,
            &BTreeMap::new(),
            &mut None,
            None,
            renderer_resources,
            id_namespace,
            dom_id,
            font_loader,
            cfg,
            image_cache,
            get_system_time_fn,
        )?;

        let budget = sec.page_count.unwrap_or(usize::MAX);
        if is_last || info.page_count <= budget {
            // Content ends inside this section.
            out.sections.push(SectionPagination {
                first_page: sec.first_page,
                content_width: sec.content_width,
                info,
            });
            return Ok(out);
        }

        // Section overflows its page budget: cut at the end of its last page
        // and flow the tail into the next section at ITS width.
        let cut_y = info
            .breaks
            .get(budget - 1)
            .map_or(info.total_content_height, |b| b.y);
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;
        let spine = spine_path_at_y(tree, &cache.calculated_positions, section_styled, cut_y);

        let working_dom = working
            .take()
            .map_or_else(|| section_styled.reconstruct_dom_subtree(None), |d| d);
        // No block starts after the cut: everything fits after all.
        let Some(path) = &spine else {
            out.sections.push(SectionPagination {
                first_page: sec.first_page,
                content_width: sec.content_width,
                info,
            });
            return Ok(out);
        };
        // The head's pages come from `info` (trimmed below); only the tail
        // flows on.
        let (_head, tail) = crate::document_edit::split_dom_at_path(&working_dom, path);

        let mut trimmed = info;
        trimmed.page_count = budget;
        trimmed.breaks.truncate(budget.saturating_sub(1));
        trimmed.total_content_height = cut_y;
        out.sections.push(SectionPagination {
            first_page: sec.first_page,
            content_width: sec.content_width,
            info: trimmed,
        });

        // StyledDom::create consumes the Dom's node data — style a CLONE and
        // keep the pristine tail as the working document for the next cut.
        let mut style_me = tail.clone();
        working_styled = Some(StyledDom::create(&mut style_me, author_css.clone()));
        working = Some(tail);
    }

    Ok(out)
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
    fn page_count(
        fragmentation_context: FragmentationContext,
        dom: &StyledDom,
        vp: LogicalRect,
    ) -> usize {
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
        cache
            .tree
            .as_ref()
            .expect("layout must cache a tree")
            .nodes
            .len()
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

        assert!(
            !pages.is_empty(),
            "layout must always emit at least one page"
        );
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

        assert!(
            !pages.is_empty(),
            "layout must always emit at least one page"
        );
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

        assert_eq!(
            first.len(),
            second.len(),
            "page count must be deterministic"
        );
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
        assert_eq!(
            cache.viewport,
            Some(vp),
            "the layout viewport must be recorded"
        );
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

        assert_eq!(
            tree_node_count(&cache),
            nodes,
            "relayout changed the tree size"
        );
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

    // ==================================================================
    // Sectioned pagination — fragmentainer WIDTH re-wrap
    // ==================================================================

    fn setup(w: f32, h: f32) -> crate::solver3::pagination::PageSetup {
        crate::solver3::pagination::PageSetup {
            page_size: LogicalSize::new(w, h),
            margins: crate::solver3::pagination::PageMargins {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            },
            header_footer: Default::default(),
        }
    }

    #[test]
    fn width_sections_partition_by_content_width() {
        use crate::solver3::pagination::PageSequence;
        let mut seq = PageSequence::uniform(setup(600.0, 400.0));
        seq.overrides.insert(0, setup(300.0, 400.0));
        let sections = seq.width_sections(64);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].first_page, 0);
        assert_eq!(sections[0].page_count, Some(1));
        assert!((sections[0].content_width - 300.0).abs() < 0.5);
        assert_eq!(sections[1].first_page, 1);
        assert_eq!(sections[1].page_count, None, "tail is open-ended");
        assert!((sections[1].content_width - 600.0).abs() < 0.5);

        // Uniform sequence: one open-ended section.
        let uni = PageSequence::uniform(setup(600.0, 400.0)).width_sections(64);
        assert_eq!(uni.len(), 1);
        assert_eq!(uni[0].page_count, None);
    }

    /// The re-wrap acceptance: content whose height DEPENDS on the page
    /// width (aspect-ratio boxes — no fonts needed) paginates differently
    /// once the tail re-measures at the wider section's width.
    #[test]
    fn sectioned_pagination_rewraps_the_tail_at_the_new_width() {
        use crate::solver3::pagination::PageSequence;

        fn aspect_doc(n: usize) -> StyledDom {
            let children: Vec<Dom> = (0..n).map(|_| Dom::create_div()).collect();
            let mut dom = Dom::create_body().with_children(children.into());
            let css = azul_css::parser2::new_from_str(
                "div { width: 100%; aspect-ratio: 2; background-color: red; }",
            )
            .0;
            StyledDom::create(&mut dom, css)
        }

        let loader = PathLoader::new();
        let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
            loader.load_font_shared(bytes, index)
        };
        let mut font_manager: FontManager<TestFont> =
            FontManager::new(FcFontCache::default()).unwrap();
        let rr = RendererResources::default();
        let ic = ImageCache::default();

        // Page 0: 300 wide → divs 150 tall. Later pages: 600 wide → 300 tall.
        let mut seq = PageSequence::uniform(setup(600.0, 400.0));
        seq.overrides.insert(0, setup(300.0, 400.0));

        let sectioned = compute_sectioned_pagination(
            &aspect_doc(8),
            400.0,
            &mut font_manager,
            &rr,
            IdNamespace(0),
            DomId::ROOT_ID,
            font_loader,
            &FakePageConfig::new(),
            &seq,
            &ic,
            time_fn(),
        )
        .expect("sectioned pagination");

        assert_eq!(sectioned.sections.len(), 2, "narrow first page + wide tail");
        assert!((sectioned.sections[1].content_width - 600.0).abs() < 0.5);

        // The same document on a UNIFORM 300-wide sequence for comparison:
        // its divs stay 150 tall everywhere.
        let uniform = compute_sectioned_pagination(
            &aspect_doc(8),
            400.0,
            &mut font_manager,
            &rr,
            IdNamespace(0),
            DomId::ROOT_ID,
            font_loader,
            &FakePageConfig::new(),
            &PageSequence::uniform(setup(300.0, 400.0)),
            &ic,
            time_fn(),
        )
        .expect("uniform pagination");
        assert_eq!(uniform.sections.len(), 1);

        // 600-wide divs are 300 tall → ~1/page; 300-wide divs are 150 tall
        // → ~2/page. If the tail had NOT re-measured at 600, the totals
        // would match — more pages proves the re-wrap happened.
        assert!(
            sectioned.page_count() > uniform.page_count(),
            "sectioned={} uniform={}: the tail must RE-MEASURE at the wide width",
            sectioned.page_count(),
            uniform.page_count()
        );
    }
}

/// A page break mapped to a STRUCTURAL position in the DOM — the keystone of
/// the DOM-materialized-breaks editor architecture: the estimator computes
/// break Y coordinates, the application inserts its break nodes at DOM
/// positions. This type carries both.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralBreak {
    /// Document-space Y where the page ends (same value as the
    /// corresponding [`PageBreakPosition::y`](crate::solver3::page_breaks::PageBreakPosition)).
    pub y: f32,
    /// Why the break happened.
    pub kind: crate::solver3::page_breaks::BreakKind,
    /// For forced breaks: the node whose break property caused it.
    pub causing_node: Option<NodeId>,
    /// Root-to-node child-index path of the first block-level box at/after
    /// `y` — the position where a break node inserted BEFORE the addressed
    /// node reproduces this page boundary structurally
    /// (`azul_core::dom::Dom` child indices, consumable by
    /// `split_dom_at_path`). `None` when no block-level box sits at/after
    /// `y` (a break in trailing whitespace / past the last block).
    pub path: Option<Vec<u32>>,
    /// Ledger #2 (line-granular option): when the addressed block is an
    /// IFC whose LINE BOXES straddle `y`, the (run, byte) of the first
    /// line that moves to the next page — the app can split the paragraph
    /// text there instead of moving the whole block. `None` = block
    /// boundary (the entire addressed block moves), the v1 contract.
    pub line_start: Option<azul_core::selection::ContentIndex>,
}

/// Map every break of a [`PaginationInfo`](crate::solver3::page_breaks::PaginationInfo)
/// to its structural DOM position, using the layout tree and positions that
/// [`compute_document_pagination`] left in `cache`.
///
/// Call this immediately after [`compute_document_pagination`] with the SAME
/// `cache` and `styled_dom` — the mapping reads `cache.tree` and
/// `cache.calculated_positions`, which every further layout pass may
/// invalidate. Returns `None` when the cache holds no tree (pagination was
/// never computed, or the cache was cleared).
#[cfg(feature = "text_layout")]
#[must_use]
pub fn pagination_to_dom_breaks(
    cache: &LayoutCache,
    styled_dom: &StyledDom,
    pagination: &crate::solver3::page_breaks::PaginationInfo,
) -> Option<Vec<StructuralBreak>> {
    let tree = cache.tree.as_ref()?;
    let positions = &cache.calculated_positions;
    Some(
        pagination
            .breaks
            .iter()
            .map(|b| StructuralBreak {
                y: b.y,
                kind: b.kind,
                causing_node: b.causing_node,
                path: spine_path_at_y(tree, positions, styled_dom, b.y),
                line_start: spine_line_start_at_y(tree, positions, styled_dom, b.y),
            })
            .collect(),
    )
}

/// The DEEPEST block-level box whose border-box vertically CONTAINS `y`
/// (the spine path addresses the first block AT/AFTER `y`; a mid-block
/// break's line lookup needs the box the break lands IN).
fn spine_layout_hit_at_y(
    tree: &crate::solver3::layout_tree::LayoutTree,
    positions: &crate::solver3::PositionVec,
    styled_dom: &StyledDom,
    y: f32,
) -> Option<(usize, f32)> {
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let depth_of = |mut n: NodeId| -> u32 {
        let mut d = 0;
        while let Some(p) = hierarchy
            .get(n)
            .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
        {
            d += 1;
            n = p;
        }
        d
    };
    let mut best: Option<(u32, usize, f32)> = None;
    for idx in 0..tree.nodes.len() {
        let Some(node) = tree.get(LayoutNodeId::new(idx)) else {
            continue;
        };
        let Some(dom_id) = node.dom_node_id else {
            continue;
        };
        if !crate::solver3::layout_tree::is_block_level(styled_dom, dom_id) {
            continue;
        }
        let Some(pos) = crate::solver3::pos_get(positions, idx) else {
            continue;
        };
        let h = node.used_size.map_or(0.0, |sz| sz.height);
        if !(pos.y - 0.5 <= y && y < pos.y + h - 0.5) {
            continue;
        }
        let d = depth_of(dom_id);
        if best.as_ref().is_none_or(|(bd, ..)| d > *bd) {
            best = Some((d, idx, pos.y));
        }
    }
    best.map(|(_, idx, top)| (idx, top))
}

/// Ledger #2: the line-granular refinement of [`spine_path_at_y`]. When the
/// block the break lands IN is an IFC with line boxes on both sides of `y`,
/// returns the (run, byte) starting the first line at/after `y` — measured
/// in the block's content box. `None` when the break sits at a block
/// boundary, the block has no inline layout, or every line is below `y`
/// (then the whole block moves — the block-granular contract).
#[must_use]
pub fn spine_line_start_at_y(
    tree: &crate::solver3::layout_tree::LayoutTree,
    positions: &crate::solver3::PositionVec,
    styled_dom: &StyledDom,
    y: f32,
) -> Option<azul_core::selection::ContentIndex> {
    use crate::text3::cache::ShapedItem;
    let hierarchy = styled_dom.node_hierarchy.as_container();
    // Re-find the spine block the path addresses (same selection rule).
    let (layout_idx, node_top) = spine_layout_hit_at_y(tree, positions, styled_dom, y)?;
    let node = tree.get(LayoutNodeId::new(layout_idx))?;
    let bp = node.box_props.unpack();
    let content_top = node_top + bp.padding.top + bp.border.top;
    let rel_y = y - content_top;
    if rel_y <= 0.5 {
        return None; // block-boundary break: whole block moves
    }
    let layout = tree.get_inline_layout_for_node(layout_idx)?;
    // (d6h) Dense-first: the stored sparse may be the retirement
    // sentinel. LineRecords give the tops in O(lines); the sparse arm
    // remains as the flag-off path and the verify oracle.
    if let Some(dense) = tree.get_dense_for_node(layout_idx) {
        if !dense.clusters.is_empty() {
            let result = spine_line_start_dense(dense, rel_y);
            if crate::solver3::layout_tree::dense_text_mode() == 2 {
                let sparse = spine_line_start_sparse(layout, rel_y);
                assert_eq!(
                    result, sparse,
                    "d6h verify: spine_line_start dense vs sparse diverged at rel_y {rel_y}"
                );
            }
            return result;
        }
    }
    spine_line_start_sparse(layout, rel_y)
}

/// The sparse fold of [`spine_line_start_at_y`] — the pre-d6h body,
/// kept as the flag-off path and the verify oracle.
fn spine_line_start_sparse(
    layout: &crate::text3::cache::UnifiedLayout,
    rel_y: f32,
) -> Option<azul_core::selection::ContentIndex> {
    use crate::text3::cache::ShapedItem;
    // The line the break lands ON moves to the next page (a sliced line is
    // atomic; a break AT a line top moves that line). Identify it purely by
    // LINE TOPS — per-item heights are not trustworthy on this path — as
    // the line with the largest top not above the break.
    let mut line_tops: BTreeMap<usize, f32> = BTreeMap::new();
    for item in &layout.items {
        let entry = line_tops.entry(item.line_index).or_insert(f32::MAX);
        *entry = entry.min(item.position.y);
    }
    let straddler = line_tops
        .iter()
        .filter(|(_, top)| **top <= rel_y + 0.5)
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(line, _)| *line)?;
    // A first-line hit means the whole block moves: block-granular None.
    if straddler == 0 {
        return None;
    }
    let mut best: Option<azul_core::selection::ContentIndex> = None;
    for item in &layout.items {
        if item.line_index != straddler {
            continue;
        }
        // Clusters carry their identity in `source_cluster_id` (the same
        // GraphemeClusterId the cursor/editing pipeline keys on);
        // `source_content_index` is not populated on the paged shaping
        // path. Non-cluster items fall back to their ContentIndex.
        let src = match &item.item {
            ShapedItem::Cluster(c) => azul_core::selection::ContentIndex {
                run_index: c.source_cluster_id.source_run,
                item_index: c.source_cluster_id.start_byte_in_run,
            },
            ShapedItem::CombinedBlock { source, .. }
            | ShapedItem::Object { source, .. }
            | ShapedItem::Tab { source, .. }
            | ShapedItem::Break { source, .. } => *source,
        };
        if best
            .as_ref()
            .is_none_or(|s| (src.run_index, src.item_index) < (s.run_index, s.item_index))
        {
            best = Some(src);
        }
    }
    best
}

/// (d6h) The dense twin of [`spine_line_start_sparse`]: line tops from
/// `LineRecord` in O(lines). A line's sparse "top" is the MIN per-item y
/// — on mixed-size lines that is `shared_baseline - max ascent over the
/// line's runs`, reconstructed here exactly as the expander does.
fn spine_line_start_dense(
    dense: &crate::text3::dense::DenseText,
    rel_y: f32,
) -> Option<azul_core::selection::ContentIndex> {
    use crate::text3::dense::DenseText;
    let line_top = |l: &crate::text3::dense::LineRecord| -> f32 {
        let Some(first_run) = dense.run_of(l.clusters.0) else {
            return l.baseline_y;
        };
        let base = l.baseline_y + DenseText::resolved_run_ascent(first_run);
        let mut top = f32::MAX;
        let mut ci = l.clusters.0;
        while ci < l.clusters.1 {
            let Some(r) = dense.run_of(ci) else { break };
            top = top.min(base - DenseText::resolved_run_ascent(r));
            ci = r.clusters.end.max(ci + 1);
        }
        if top == f32::MAX {
            l.baseline_y
        } else {
            top
        }
    };
    let straddler = dense
        .lines
        .iter()
        .filter(|l| line_top(l) <= rel_y + 0.5)
        .max_by(|a, b| line_top(a).total_cmp(&line_top(b)))?;
    if straddler.source_index == 0 {
        return None;
    }
    let mut best: Option<azul_core::selection::ContentIndex> = None;
    for ci in straddler.clusters.0..straddler.clusters.1 {
        let c = &dense.clusters[ci as usize];
        let run = dense.run_of(ci)?;
        let src = azul_core::selection::ContentIndex {
            run_index: run.source_run,
            item_index: c.start_byte,
        };
        if best
            .as_ref()
            .is_none_or(|s| (src.run_index, src.item_index) < (s.run_index, s.item_index))
        {
            best = Some(src);
        }
    }
    best
}

/// One fragmentainer's outcome from [`layout_document_tokenized`].
#[derive(Debug, Clone)]
pub struct TokenizedPage {
    /// Block-size of the content laid INTO this fragmentainer (the root's
    /// fitted used height for this pass).
    pub content_block_size: f32,
    /// The outgoing resume token (`None` = the document finished here).
    pub outgoing: Option<crate::solver3::break_token::BreakToken>,
    /// This page's display list, GENERATED from the fragment pass (never
    /// sliced): only nodes laid on this page have assigned positions —
    /// everything else sits at the unassigned sentinel and is dropped by
    /// `push_item`. Page-local coordinates (the fragmentainer origin is 0).
    pub display_list: DisplayList,
}

/// K30b part 2 / K30c skeleton: the NG-style page loop. Lays the document
/// out one fragmentainer at a time — page N's outgoing token is page N+1's
/// incoming token; layout re-descends the tree each page, skipping finished
/// subtrees via the token (design doc §4.5). No display lists yet (that is
/// the rest of K30c); the output pins the token algebra: progress,
/// conservation, nested resume.
///
/// # Errors
/// Propagates layout errors; the internal no-progress guard turns the
/// NG infinite-loop class into loop termination instead.
#[allow(clippy::too_many_arguments)]
/// K34 — token convergence: what a previous tokenized run left behind, so
/// an incremental re-pagination can stop as soon as it re-synchronizes with
/// it.
///
/// Tokens are owned value types with reliable `Eq`, so the invariant is
/// exact: **if the token entering page N is unchanged, pages ≥ N are
/// unchanged.** An edit therefore only has to re-lay pages until an
/// outgoing token matches the cached one for that page; everything after
/// is reused verbatim. Typing converges in ≤ 2 pages, which is what makes
/// live repagination affordable on a long document.
#[derive(Debug, Clone, Default)]
pub struct TokenCache {
    /// Per-page outgoing token from the previous run (`None` = the document
    /// ended on that page).
    pub outgoing: Vec<Option<crate::solver3::break_token::BreakToken>>,
    /// The pages themselves, reused verbatim from the convergence point on.
    pub pages: Vec<TokenizedPage>,
}

/// Outcome of an incremental (convergence-aware) pagination.
#[derive(Debug)]
pub struct IncrementalPagination {
    /// The full page list — freshly laid pages followed by any reused tail.
    pub pages: Vec<TokenizedPage>,
    /// How many pages this run actually laid out. `pages.len() - laid_out`
    /// is what convergence saved.
    pub laid_out: usize,
    /// The page index at which the run re-synchronized with the cache, if it
    /// did (`None` = it ran to the end of the document).
    pub converged_at: Option<usize>,
}

/// What the shared page loop returns (see `layout_document_tokenized_from`).
struct PageLoopOutcome {
    pages: Vec<TokenizedPage>,
    laid_out: usize,
    converged_at: Option<usize>,
}

/// The public full-document entry point: lay every page from the start.
#[allow(clippy::too_many_arguments)]
pub fn layout_document_tokenized<T, F>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    new_dom: &StyledDom,
    viewport: LogicalRect,
    font_manager: &mut crate::font_traits::FontManager<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    image_cache: &azul_core::resources::ImageCache,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
    font_loader: F,
    renderer_resources: &RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
    page_height: f32,
    max_pages: usize,
) -> Result<Vec<TokenizedPage>>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(
            std::sync::Arc<rust_fontconfig::FontBytes>,
            usize,
        ) -> std::result::Result<T, crate::text3::cache::LayoutError>
        + Copy,
{
    Ok(layout_document_tokenized_from(
        cache,
        text_cache,
        new_dom,
        viewport,
        font_manager,
        debug_messages,
        image_cache,
        get_system_time_fn,
        font_loader,
        renderer_resources,
        id_namespace,
        dom_id,
        page_height,
        max_pages,
        None,
        None,
    )?
    .pages)
}

/// The shared page loop. `start_token` resumes mid-document (K34 incremental
/// re-pagination); `converge_against` lets it stop as soon as it
/// re-synchronizes with a previous run.
#[allow(clippy::too_many_arguments)]
fn layout_document_tokenized_from<T, F>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    new_dom: &StyledDom,
    viewport: LogicalRect,
    font_manager: &mut crate::font_traits::FontManager<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    image_cache: &azul_core::resources::ImageCache,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
    font_loader: F,
    renderer_resources: &RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
    page_content_height: f32,
    max_pages: usize,
    start_token: Option<crate::solver3::break_token::BreakToken>,
    converge_against: Option<(&TokenCache, usize)>,
) -> Result<PageLoopOutcome>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(
        std::sync::Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> std::result::Result<T, crate::text3::cache::LayoutError>,
{
    use crate::solver3::break_token::{token_fingerprint, BreakToken};
    let mut laid_out: usize = 0;
    let mut converged_at: Option<usize> = None;
    use crate::solver3::cache::{calculate_layout_for_subtree_fragment, ComputeMode};
    use crate::solver3::fc::FragmentainerSpace;

    // 1. Build the tree + shape text once via a CONTINUOUS pass (the page
    // loop re-descends this structure; fonts resolve the same way the
    // paged estimator does).
    let mut frag = FragmentationContext::new_continuous(viewport.size.width);
    {
        // Font resolution identical to the paged path.
        use crate::solver3::getters::{
            collect_and_resolve_font_chains_with_registration, collect_font_ids_from_chains,
            compute_fonts_to_load, load_fonts_from_disk,
        };
        let _p = crate::probe::Probe::span("font_chain_resolve");
        // SKIP THE RESOLVER when this DOM asks for the same font stacks the
        // manager already resolved. `LayoutWindow` has done this since the
        // beginning (window.rs, `font_requirements_unchanged`) via a rolling
        // hash of the compact cache's `prev_font_hashes`; the pagination
        // entry points did not — and worse, called the plain
        // `set_font_chain_cache`, which CLEARS the recorded signature, so
        // even a caller reusing one FontManager re-resolved a 160-family
        // chain on EVERY pagination (measured 8 ms/call, ~8% of a warm one).
        let font_stacks_sig = new_dom
            .css_property_cache
            .ptr
            .compact_cache
            .as_ref()
            .map(|cc| {
                let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                for &fh in &cc.prev_font_hashes {
                    h = h.rotate_left(13) ^ fh;
                    h = h.wrapping_mul(0x0100_0000_01b3);
                }
                h
            });
        let font_requirements_unchanged = font_stacks_sig.is_some()
            && font_stacks_sig == font_manager.last_resolved_font_stacks_sig
            && !font_manager.font_chain_cache.is_empty();

        if !font_requirements_unchanged {
            let _p = crate::probe::Probe::span("font_chain_resolve");
            let platform = azul_css::system::Platform::current();
            let chains = collect_and_resolve_font_chains_with_registration(
                new_dom,
                &font_manager.fc_cache,
                font_manager,
                &platform,
            );
            let required = collect_font_ids_from_chains(&chains);
            let loaded = font_manager.get_loaded_font_ids();
            let to_load = compute_fonts_to_load(&required, &loaded);
            if !to_load.is_empty() {
                let res = load_fonts_from_disk(&to_load, &font_manager.fc_cache, &font_loader);
                font_manager.insert_fonts(res.loaded);
            }
            font_manager
                .set_font_chain_cache_with_sig(chains.into_fontconfig_chains(), font_stacks_sig);
        }
    }
    compute_layout_with_fragmentation(
        cache,
        text_cache,
        &mut frag,
        new_dom,
        viewport,
        font_manager,
        debug_messages,
        image_cache,
        get_system_time_fn,
        false,
    )?;

    // 2. The page loop.
    let mut pages: Vec<TokenizedPage> = Vec::new();
    let mut incoming: Option<BreakToken> = start_token;
    for page_idx in 0..max_pages {
        let resume = match incoming.as_ref() {
            None => None,
            Some(BreakToken::Block(b)) => Some(b),
            // The ROOT is a block box; an inline token cannot reach here.
            Some(BreakToken::Inline(_)) => None,
        };
        let space = FragmentainerSpace {
            remaining_block_extent: page_content_height,
            next_fragmentainer_extent: page_content_height,
            is_first: page_idx == 0 && incoming.is_none(),
            resume,
        };

        let tree = cache.tree.as_mut().ok_or(LayoutError::InvalidTree)?;
        let mut counter_values = cache.counters.clone();
        let empty_text_selections: BTreeMap<DomId, TextSelection> = BTreeMap::new();
        let mut ctx = LayoutContext {
            style_cache: Default::default(),
            scrollbar_style_cache: core::cell::RefCell::new(std::collections::HashMap::new()),
            styled_dom: new_dom,
            font_manager: &*font_manager,
            text_selections: &empty_text_selections,
            debug_messages,
            counters: &mut counter_values,
            viewport_size: viewport.size,
            fragmentation_context: None,
            reflowed_ifcs: std::collections::BTreeSet::new(),
            cursor_is_visible: false,
            cursor_locations: Vec::new(),
            preedit_text: None,
            cache_map: std::mem::take(&mut cache.cache_map),
            image_cache,
            content_overlay: None,
            system_style: None,
            get_system_time_fn,
        };

        let mut outgoing: Option<BreakToken> = None;
        // Positions pre-filled with the UNASSIGNED sentinel: only nodes the
        // fragment pass actually lays on THIS page receive positions; the
        // display-list builder drops everything else (its existing
        // unassigned-position guard) — pages are generated, never sliced.
        let node_count = tree.nodes.len();
        let mut page_positions: crate::solver3::PositionVec =
            alloc::vec![crate::solver3::POSITION_UNSET; node_count];
        let mut tmp_scrollbars = false;
        let mut tmp_floats = std::collections::HashMap::new();
        let result = calculate_layout_for_subtree_fragment(
            &mut ctx,
            tree,
            text_cache,
            0, // the root layout node
            LogicalPosition::zero(),
            viewport.size,
            &mut page_positions,
            &mut tmp_scrollbars,
            &mut tmp_floats,
            ComputeMode::PerformLayout,
            Some(space),
            Some(&mut outgoing),
        );
        result?;

        // The ROOT box itself sits at the fragmentainer origin (its
        // children got positions from Pass 2; the root's own position is
        // the caller's job on the normal path).
        crate::solver3::pos_set(&mut page_positions, 0, LogicalPosition::zero());

        // Generate THIS page's display list from the fragment positions.
        let display_list = {
            let tree_ref: &_ = tree;
            crate::solver3::display_list::generate_display_list(
                &mut ctx,
                tree_ref,
                &page_positions,
                &BTreeMap::new(),
                &cache.scroll_ids,
                None,
                renderer_resources,
                id_namespace,
                dom_id,
            )?
        };
        cache.cache_map = std::mem::take(&mut ctx.cache_map);

        let content = cache
            .tree
            .as_ref()
            .and_then(|t| t.get(LayoutNodeId::new(0)))
            .and_then(|n| n.used_size)
            .map_or(0.0, |sz| sz.height);

        // PROGRESS GUARD (the NG infinite-loop class): an outgoing token
        // identical to the incoming one means the page consumed nothing.
        let stalled = match (&incoming, &outgoing) {
            (Some(a), Some(b)) => token_fingerprint(a) == token_fingerprint(b) && a == b,
            _ => false,
        };
        pages.push(TokenizedPage {
            content_block_size: content,
            outgoing: outgoing.clone(),
            display_list,
        });
        laid_out += 1;
        if stalled || outgoing.is_none() {
            break;
        }

        // K34 CONVERGENCE. Tokens are value types with reliable `Eq`: if the
        // token leaving this page equals the one that left the SAME page
        // last time, every later page receives an identical input and is
        // therefore identical. Splice the cached tail in and stop — this is
        // what turns "repaginate the document" into "repaginate two pages".
        if let Some((cached, base)) = converge_against {
            let abs_page = base + page_idx;
            if let Some(cached_outgoing) = cached.outgoing.get(abs_page) {
                let same = match (cached_outgoing, &outgoing) {
                    (Some(a), Some(b)) => token_fingerprint(a) == token_fingerprint(b) && a == b,
                    (None, None) => true,
                    _ => false,
                };
                if same && abs_page + 1 < cached.pages.len() {
                    pages.extend(cached.pages[abs_page + 1..].iter().cloned());
                    converged_at = Some(abs_page);
                    break;
                }
            }
        }

        incoming = outgoing;
    }
    Ok(PageLoopOutcome {
        pages,
        laid_out,
        converged_at,
    })
}

/// K34: re-paginate from `first_dirty_page`, stopping as soon as the run
/// re-synchronizes with `cache`.
///
/// The caller supplies the page the edit dirtied (`page_of_y` on the
/// chokepoint's dirty extent) and the previous run's [`TokenCache`]. Pages
/// before `first_dirty_page` are reused untouched — their incoming tokens
/// predate the edit and are therefore still valid — and the loop resumes
/// from that page's cached incoming token. After each freshly laid page the
/// outgoing token is compared against the cached one for the same index: on
/// equality every later page is spliced in verbatim and the run stops.
///
/// Falls back to a full run whenever the cache cannot be trusted (empty,
/// or `first_dirty_page` beyond it), so a caller can always call this.
///
/// # Errors
///
/// Propagates layout failures from the underlying page loop.
#[allow(clippy::too_many_arguments)]
pub fn layout_document_tokenized_incremental<T, F>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    new_dom: &StyledDom,
    viewport: LogicalRect,
    font_manager: &mut crate::font_traits::FontManager<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    image_cache: &azul_core::resources::ImageCache,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
    font_loader: F,
    renderer_resources: &RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
    page_height: f32,
    max_pages: usize,
    token_cache: &TokenCache,
    first_dirty_page: usize,
) -> Result<IncrementalPagination>
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(
            std::sync::Arc<rust_fontconfig::FontBytes>,
            usize,
        ) -> std::result::Result<T, crate::text3::cache::LayoutError>
        + Copy,
{
    // A full run whenever the cache cannot help.
    let usable = !token_cache.pages.is_empty()
        && token_cache.outgoing.len() == token_cache.pages.len()
        && first_dirty_page < token_cache.pages.len();
    if !usable {
        let pages = layout_document_tokenized(
            cache,
            text_cache,
            new_dom,
            viewport,
            font_manager,
            debug_messages,
            image_cache,
            get_system_time_fn,
            font_loader,
            renderer_resources,
            id_namespace,
            dom_id,
            page_height,
            max_pages,
        )?;
        let laid_out = pages.len();
        return Ok(IncrementalPagination {
            pages,
            laid_out,
            converged_at: None,
        });
    }

    // Pages before the dirty one are untouched by definition.
    let mut pages: Vec<TokenizedPage> = token_cache.pages[..first_dirty_page].to_vec();
    // Resume from the token that ENTERED the dirty page: the previous
    // page's outgoing, which predates the edit.
    let resume_token = if first_dirty_page == 0 {
        None
    } else {
        token_cache.outgoing[first_dirty_page - 1].clone()
    };

    let tail = layout_document_tokenized_from(
        cache,
        text_cache,
        new_dom,
        viewport,
        font_manager,
        debug_messages,
        image_cache,
        get_system_time_fn,
        font_loader,
        renderer_resources,
        id_namespace,
        dom_id,
        page_height,
        max_pages.saturating_sub(first_dirty_page),
        resume_token,
        Some((token_cache, first_dirty_page)),
    )?;

    let laid_out = tail.laid_out;
    let converged_at = tail.converged_at;
    pages.extend(tail.pages);
    Ok(IncrementalPagination {
        pages,
        laid_out,
        converged_at,
    })
}

/// The per-page delta of a re-estimation, for the editor's lazy re-break
/// loop: pages whose breaks are bit-for-bit unchanged keep their DOM
/// subtrees untouched; patching starts at `first_changed_page`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreaksDelta {
    /// Breaks (compared by exact `y` AND `kind`) identical to the previous
    /// estimate up to this index. On the first estimate this is 0.
    pub unchanged_prefix_len: usize,
    /// The first page whose boundary moved — equal to
    /// `unchanged_prefix_len` (page N ends at break N).
    pub first_changed_page: usize,
    /// Whether the total page count changed.
    pub page_count_changed: bool,
}

/// Owns the caches an incremental pagination loop needs
/// ([`crate::solver3::cache::LayoutCache`] + text cache + the previous
/// estimate) so an embedder holds ONE session object instead of wiring
/// solver internals (AZUL-STILL-TODO B7).
///
/// ```text
/// changeset -> app model -> session.re_estimate(new_dom, ...) -> BreaksDelta
///           -> patch own DOM only from first_changed_page on
///           -> session.dom_breaks(new_dom) for the structural positions
/// ```
// Holds the layout + text caches, neither of which is `Debug` (they are
// large, self-referential-ish caches whose contents are meaningless in a
// debug print). Deriving would force `Debug` onto both cache types.
#[allow(missing_debug_implementations)]
#[cfg(feature = "text_layout")]
pub struct PaginationSession {
    pub layout_cache: LayoutCache,
    pub text_cache: TextLayoutCache,
    pub previous: Option<crate::solver3::page_breaks::PaginationInfo>,
}

#[cfg(feature = "text_layout")]
impl Default for PaginationSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "text_layout")]
impl PaginationSession {
    #[must_use]
    pub fn new() -> Self {
        Self {
            layout_cache: LayoutCache::default(),
            text_cache: TextLayoutCache::new(),
            previous: None,
        }
    }

    /// Re-estimate pagination for (a new generation of) the document and
    /// report which pages kept their boundaries. Layout reuses this
    /// session's caches, so an unchanged prefix is cheap by construction.
    #[allow(clippy::too_many_arguments)] // mirrors compute_document_pagination's surface
    pub fn re_estimate<T, F>(
        &mut self,
        styled_dom: &StyledDom,
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
    ) -> Result<BreaksDelta>
    where
        T: ParsedFontTrait + Sync + 'static,
        F: Fn(
            std::sync::Arc<rust_fontconfig::FontBytes>,
            usize,
        ) -> std::result::Result<T, crate::text3::cache::LayoutError>,
    {
        let fragmentation_context = FragmentationContext::new_paged(viewport.size);
        let info = compute_document_pagination(
            &mut self.layout_cache,
            &mut self.text_cache,
            fragmentation_context,
            styled_dom,
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
        )?;

        let unchanged_prefix_len = match &self.previous {
            None => 0,
            Some(prev) => prev
                .breaks
                .iter()
                .zip(info.breaks.iter())
                // The reuse contract: unchanged breaks are bit-for-bit equal
                // (recompute_page_breaks_from), so exact comparison is right —
                // an epsilon would hide genuinely moved boundaries.
                .take_while(|(a, b)| a.y.to_bits() == b.y.to_bits() && a.kind == b.kind)
                .count(),
        };
        let page_count_changed = self
            .previous
            .as_ref()
            .is_none_or(|prev| prev.page_count != info.page_count);
        self.previous = Some(info);
        Ok(BreaksDelta {
            unchanged_prefix_len,
            first_changed_page: unchanged_prefix_len,
            page_count_changed,
        })
    }

    /// The latest estimate (after at least one [`Self::re_estimate`]).
    #[must_use]
    pub const fn info(&self) -> Option<&crate::solver3::page_breaks::PaginationInfo> {
        self.previous.as_ref()
    }

    /// Structural DOM positions for the latest estimate — see
    /// [`pagination_to_dom_breaks`]. Call with the SAME document that was
    /// last re-estimated.
    #[must_use]
    pub fn dom_breaks(&self, styled_dom: &StyledDom) -> Option<Vec<StructuralBreak>> {
        let info = self.previous.as_ref()?;
        pagination_to_dom_breaks(&self.layout_cache, styled_dom, info)
    }
}
