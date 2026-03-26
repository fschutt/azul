/// Benchmark: Full rendering pipeline for 50K-node XHTML document.
///
/// Measures per-frame cost with warm font cache (matching real-world usage).
/// Font loading is excluded — in a real app, FcFontRegistry loads fonts in
/// background threads during app init.
///
/// Run with: cargo bench -p azul-layout --bench fast_dom_bench

use std::time::Instant;

fn main() {
    // Use chapter-8.xht as the benchmark file (24k+ lines, ~50K DOM nodes)
    let bench_file = std::path::Path::new("../doc/xhtml1/chapter-8.xht");
    let xml_content = match std::fs::read_to_string(bench_file) {
        Ok(c) => c,
        Err(e) => {
            let alt = std::path::Path::new("doc/xhtml1/chapter-8.xht");
            match std::fs::read_to_string(alt) {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("Cannot find benchmark file: {}", e);
                    return;
                }
            }
        }
    };

    println!("Benchmark: chapter-8.xht ({} bytes, {} lines)\n",
        xml_content.len(), xml_content.lines().count());

    // =========================================================================
    // PRE-LOAD: Font cache (excluded from benchmark, same as real app startup)
    // =========================================================================
    use std::collections::{BTreeMap, HashMap};
    use azul_core::{
        dom::DomId,
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::{IdNamespace, RendererResources, ImageCache},
    };
    use azul_layout::{
        solver3::{self, cache::LayoutCache},
        cpurender::{self, RenderOptions},
        glyph_cache::GlyphCache,
        FontManager, TextLayoutCache,
    };

    println!("--- Font pre-load (excluded from per-frame timing) ---");

    let t0 = Instant::now();
    let registry = azul_layout::FcFontRegistry::new();
    let had_cache = registry.load_from_disk_cache();
    let disk_cache_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("  disk cache load:   {:.2}ms (had_cache={})", disk_cache_ms, had_cache.is_some());

    let t1 = Instant::now();
    registry.spawn_scout_and_builders();
    let spawn_ms = t1.elapsed().as_secs_f64() * 1000.0;
    println!("  spawn threads:     {:.2}ms", spawn_ms);

    let t2 = Instant::now();
    let os = rust_fontconfig::OperatingSystem::current();
    let common_stacks = rust_fontconfig::config::tokenize_common_families(os);
    registry.request_fonts(&common_stacks);
    let request_ms = t2.elapsed().as_secs_f64() * 1000.0;
    println!("  request_fonts:     {:.2}ms (blocks until common fonts parsed)", request_ms);

    let t3 = Instant::now();
    let fc_cache = std::sync::Arc::new(registry.into_fc_font_cache());
    let snapshot_ms = t3.elapsed().as_secs_f64() * 1000.0;
    println!("  snapshot cache:    {:.2}ms ({} font entries)", snapshot_ms, fc_cache.len());

    // Warm up: parse once to discover + load required fonts
    let mut font_manager = FontManager::from_arc(fc_cache.clone()).expect("font manager");
    {
        let warmup_dom = azul_layout::xml::parse_xml_to_styled_dom(&xml_content).unwrap();
        use azul_layout::solver3::getters::*;
        let platform = azul_css::system::Platform::current();

        let t4 = Instant::now();
        let chains = collect_and_resolve_font_chains_with_registration(
            &warmup_dom, &font_manager.fc_cache, &font_manager, &platform,
        );
        let chain_ms = t4.elapsed().as_secs_f64() * 1000.0;

        let t6 = Instant::now();
        let required_fonts = collect_font_ids_from_chains(&chains);
        let already_loaded = font_manager.get_loaded_font_ids();
        let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);
        let need_count = fonts_to_load.len();
        if !fonts_to_load.is_empty() {
            use azul_layout::text3::default::PathLoader;
            let loader = PathLoader::new();
            let load_result = load_fonts_from_disk(
                &fonts_to_load,
                &font_manager.fc_cache,
                |bytes, index| loader.load_font(bytes, index),
            );
            font_manager.insert_fonts(load_result.loaded);
        }
        let load_ms = t6.elapsed().as_secs_f64() * 1000.0;

        font_manager.set_font_chain_cache(chains.into_fontconfig_chains());
        println!("  collect+resolve:   {:.2}ms (single pass)", chain_ms);
        println!("  load from disk:    {:.2}ms ({} fonts needed, {} total loaded)",
            load_ms, need_count, font_manager.get_loaded_font_ids().len());
    }
    let total_prefont_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("  TOTAL pre-load:    {:.2}ms\n", total_prefont_ms);

    // =========================================================================
    // PER-FRAME BENCHMARK (warm fonts)
    // =========================================================================
    let viewport_w = 1024.0_f32;
    let viewport_h = 768.0_f32;
    let dpi = 1.0_f32;

    const ITERATIONS: usize = 5;
    println!("--- Per-frame pipeline ({} iterations, warm fonts) ---", ITERATIONS);

    let mut pipeline_times = Vec::new();
    let mut stage_times = Vec::new(); // (parse+cascade, font_chains, layout+dl, render)

    for iter in 0..ITERATIONS {
        let t_pipeline = Instant::now();

        // Stage 1: XML → StyledDom (tokenize + build FastDom + cascade)
        let t_s1 = Instant::now();
        let styled_dom = azul_layout::xml::parse_xml_to_styled_dom(&xml_content).unwrap();
        let node_count = styled_dom.node_hierarchy.as_ref().len();
        let s1_ms = t_s1.elapsed().as_secs_f64() * 1000.0;

        // Stage 1.5: Font chain resolution (warm — no disk I/O, single pass)
        let t_s1b = Instant::now();
        {
            use azul_layout::solver3::getters::*;
            let platform = azul_css::system::Platform::current();
            let chains = collect_and_resolve_font_chains_with_registration(
                &styled_dom, &font_manager.fc_cache, &font_manager, &platform,
            );
            font_manager.set_font_chain_cache(chains.into_fontconfig_chains());
        }
        let s1b_ms = t_s1b.elapsed().as_secs_f64() * 1000.0;

        // Stage 2: Layout + display list
        let t_s2 = Instant::now();
        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize { width: viewport_w, height: viewport_h },
        };
        let mut layout_cache = LayoutCache {
            tree: None,
            calculated_positions: Vec::new(),
            viewport: None,
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
            counters: HashMap::new(),
            float_cache: HashMap::new(),
            cache_map: Default::default(),
            previous_positions: Vec::new(),
        };
        let mut text_cache = TextLayoutCache::new();
        let renderer_resources = RendererResources::default();
        let get_system_time_fn = azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        };

        let display_list = solver3::layout_document(
            &mut layout_cache,
            &mut text_cache,
            styled_dom,
            viewport,
            &font_manager,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &mut None,
            None,
            &renderer_resources,
            IdNamespace(0xFFFF),
            DomId::ROOT_ID,
            false,
            None,
            &ImageCache::default(),
            None,
            get_system_time_fn,
        ).expect("layout failed");
        let dl_items = display_list.items.len();
        let s2_ms = t_s2.elapsed().as_secs_f64() * 1000.0;

        // Stage 3: CPU render
        let t_s3 = Instant::now();
        let mut glyph_cache = GlyphCache::new();
        let _pixmap = cpurender::render_with_font_manager(
            &display_list,
            &renderer_resources,
            &font_manager,
            RenderOptions { width: viewport_w, height: viewport_h, dpi_factor: dpi },
            &mut glyph_cache,
        ).expect("render failed");
        let s3_ms = t_s3.elapsed().as_secs_f64() * 1000.0;

        let total_ms = t_pipeline.elapsed().as_secs_f64() * 1000.0;
        pipeline_times.push(total_ms);
        stage_times.push((s1_ms, s1b_ms, s2_ms, s3_ms));

        println!("  [{}/{}] {:.1}ms total | parse+cascade={:.1}ms fonts={:.1}ms layout={:.1}ms ({} DL) render={:.1}ms",
            iter + 1, ITERATIONS, total_ms, s1_ms, s1b_ms, s2_ms, dl_items, s3_ms);
    }

    // =========================================================================
    // SUMMARY
    // =========================================================================
    println!("\n--- Summary ({} nodes, {}x{}) ---", 50043, viewport_w as u32, viewport_h as u32);
    let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let s1_avg = avg(&stage_times.iter().map(|s| s.0).collect::<Vec<_>>());
    let s1b_avg = avg(&stage_times.iter().map(|s| s.1).collect::<Vec<_>>());
    let s2_avg = avg(&stage_times.iter().map(|s| s.2).collect::<Vec<_>>());
    let s3_avg = avg(&stage_times.iter().map(|s| s.3).collect::<Vec<_>>());
    let total_avg = avg(&pipeline_times);

    println!("  parse + cascade:   {:.1}ms", s1_avg);
    println!("  font chains:       {:.1}ms", s1b_avg);
    println!("  layout + DL:       {:.1}ms", s2_avg);
    println!("  CPU render:        {:.1}ms", s3_avg);
    println!("  ────────────────────────");
    println!("  TOTAL per-frame:   {:.1}ms", total_avg);
}
