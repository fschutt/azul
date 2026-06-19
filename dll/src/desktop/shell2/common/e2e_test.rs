//! `AZ_E2E_TEST` — deterministic resize/tick scenario runner.
//!
//! Reads a JSON scenario file from the `AZ_E2E_TEST` env var, constructs a
//! [`HeadlessWindow`], scripts resize/tick/sleep steps against it, probes
//! RSS via [`azul_layout::probe::current_rss_bytes`], and exits with code 0
//! (RSS stayed under the configured ceiling) or 1 (growth or absolute
//! breach). Designed to reproduce the calc.c 18 MiB → 100+ MiB resize leak.
//!
//! This is separate from `AZ_E2E=` (debug-server-dispatched assertion
//! scenarios) — this harness takes over `main()` instead of running
//! alongside a normal window.
//!
//! Gated behind the `e2e-test` cargo feature.

#![cfg(feature = "e2e-test")]

use std::{
    cell::RefCell,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use azul_core::{
    icon::SharedIconProvider,
    refany::RefAny,
    resources::AppConfig,
};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use super::super::headless::HeadlessWindow;
use super::WindowError;

// ---------------------------------------------------------------------------
// JSON schema
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize, Debug, Clone, Copy)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Step {
    /// Fast resize: update dimensions and call `incremental_relayout`,
    /// matching the real macOS/X11/Win32 resize path (no DOM rebuild).
    Resize { width: f32, height: f32 },
    /// Full rebuild — calls `regenerate_layout` (user layout callback
    /// fires, StyledDom is recreated from scratch).
    Tick,
    /// Cold path resize: updates dimensions AND calls regenerate_layout.
    /// Useful for reproducing the prior baseline numbers.
    ResizeFull { width: f32, height: f32 },
    SleepMs { ms: u64 },
}

#[derive(serde::Deserialize, Debug, Clone, Copy)]
pub struct LoopSpec {
    pub iterations: u32,
    /// Half-open slice `[a, b)` of `steps` to replay each iteration.
    /// Defaults to replay the whole list.
    #[serde(default)]
    pub steps_range: Option<[usize; 2]>,
}

#[derive(serde::Deserialize, Debug, Clone, Copy)]
pub struct RssProbes {
    #[serde(default = "default_every_n")]
    pub every_n_iterations: u32,
    #[serde(default)]
    pub warmup_skip: u32,
    #[serde(default)]
    pub assert_growth_mib_max: Option<f64>,
    #[serde(default)]
    pub assert_absolute_mib_max: Option<f64>,
    /// Emit per-probe `memory_report()` dumps for StyledDom,
    /// Solver3LayoutCache, TextLayoutCache, and selected managers as a
    /// structured `mem` JSONL event alongside the `probe` event.  Used to
    /// attribute RSS growth to a specific field.
    #[serde(default)]
    pub memory_breakdown: bool,
}

fn default_every_n() -> u32 { 100 }

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Output {
    #[serde(default = "dash")]
    pub jsonl_path: String,
    #[serde(default = "dash")]
    pub summary_path: String,
}

impl Default for Output {
    fn default() -> Self {
        Output { jsonl_path: dash(), summary_path: dash() }
    }
}

fn dash() -> String { "-".into() }

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Scenario {
    pub name: String,
    #[serde(default)]
    pub warmup_ticks: u32,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub r#loop: Option<LoopSpec>,
    #[serde(default)]
    pub rss_probes: Option<RssProbes>,
    #[serde(default)]
    pub output: Output,
}

// ---------------------------------------------------------------------------
// Env var reader
// ---------------------------------------------------------------------------

/// Returns the scenario file path if `AZ_E2E_TEST` is set.
pub fn scenario_path() -> Option<String> {
    static P: OnceLock<Option<String>> = OnceLock::new();
    P.get_or_init(|| {
        std::env::var("AZ_E2E_TEST").ok().filter(|s| !s.is_empty())
    }).clone()
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Run the scenario, print JSONL events to stderr, exit process.
///
/// Called from `run.rs` startup dispatch when `AZ_E2E_TEST` is set.
/// This bypasses `NSApplication` entirely and takes over the thread.
pub fn run_e2e_scenario(
    path: &str,
    app_data: RefAny,
    mut config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    font_registry: Option<Arc<FcFontRegistry>>,
    root_window: WindowCreateOptions,
) -> Result<(), WindowError> {
    let scenario_json = std::fs::read_to_string(path).map_err(|e| {
        WindowError::PlatformError(format!("cannot read AZ_E2E_TEST file '{}': {}", path, e))
    })?;
    let scenario: Scenario = serde_json::from_str(&scenario_json).map_err(|e| {
        WindowError::PlatformError(format!("invalid AZ_E2E_TEST JSON in '{}': {}", path, e))
    })?;

    eprintln!("[E2E] scenario: {}", scenario.name);
    eprintln!("[E2E] warmup_ticks={} steps={} loop_iter={:?}",
        scenario.warmup_ticks, scenario.steps.len(),
        scenario.r#loop.as_ref().map(|l| l.iterations));

    // Construct HeadlessWindow (same path the AZ_BACKEND=headless flow uses).
    let icon_provider_handle = core::mem::take(&mut config.icon_provider);
    let shared_icon_provider = SharedIconProvider::from_handle(icon_provider_handle);
    let app_data_arc = Arc::new(RefCell::new(app_data));
    eprintln!("[E2E-TRACE] before HeadlessWindow::new");
    let mut window = HeadlessWindow::new(
        root_window,
        app_data_arc,
        config,
        shared_icon_provider,
        fc_cache,
        font_registry,
    )?;
    eprintln!("[E2E-TRACE] after HeadlessWindow::new");

    // --- Warmup ticks ---
    for w in 0..scenario.warmup_ticks {
        eprintln!("[E2E-TRACE] before warmup tick {}", w);
        if let Err(e) = window.regenerate_layout() {
            eprintln!("[E2E] warmup regenerate_layout error: {}", e);
        }
        eprintln!("[E2E-TRACE] after warmup tick {}", w);
    }

    eprintln!("[E2E-TRACE] before baseline RSS");
    // --- Baseline RSS after warmup ---
    let baseline_rss = current_rss_mib();
    emit_jsonl(
        &scenario.output.jsonl_path,
        &format!(
            r#"{{"ev":"baseline","iter":0,"rss_mib":{:.2}}}"#,
            baseline_rss
        ),
    );

    let iterations = scenario.r#loop.as_ref().map(|l| l.iterations).unwrap_or(1);
    let range = resolve_range(&scenario);

    let probe_cfg = scenario.rss_probes.clone().unwrap_or(RssProbes {
        every_n_iterations: default_every_n(),
        warmup_skip: 0,
        assert_growth_mib_max: None,
        assert_absolute_mib_max: None,
        memory_breakdown: false,
    });

    let mut peak_rss = baseline_rss;
    let mut final_rss = baseline_rss;
    let mut absolute_breach_at: Option<u32> = None;
    let mut probe_count: u32 = 0;

    let t_start = Instant::now();

    for iter in 1..=iterations {
        if iter <= 3 || iter % 50 == 0 {
            eprintln!("[E2E-TRACE] iter {} start", iter);
        }
        // Drain the per-thread Probe buffer so its growth doesn't pollute
        // our RSS measurements. The buffer is only meaningful when
        // `AZ_PROFILE=cpu` is actually draining it, which this harness
        // is not. Without this the buffer grows unboundedly at ~dozens of
        // events per layout and masquerades as a resize leak.
        azul_layout::probe::Probe::drop_events();

        for step in &scenario.steps[range.0..range.1] {
            match step {
                Step::Resize { width, height } => {
                    set_size(&mut window, *width, *height);
                    if let Some(layout_window) = window.common.layout_window.as_mut() {
                        let mut debug_messages = None;
                        if let Err(e) = super::layout::incremental_relayout(
                            layout_window,
                            &window.common.current_window_state,
                            &mut window.common.renderer_resources,
                            &mut debug_messages,
                        ) {
                            eprintln!("[E2E] iter {} incremental_relayout error: {}", iter, e);
                        }
                    }
                }
                Step::ResizeFull { width, height } => {
                    set_size(&mut window, *width, *height);
                    window.common.frame_needs_regeneration = true;
                    if let Err(e) = window.regenerate_layout() {
                        eprintln!("[E2E] iter {} resize_full regenerate_layout error: {}", iter, e);
                    }
                }
                Step::Tick => {
                    if let Err(e) = window.regenerate_layout() {
                        eprintln!("[E2E] iter {} tick regenerate_layout error: {}", iter, e);
                    }
                }
                Step::SleepMs { ms } => {
                    std::thread::sleep(Duration::from_millis(*ms));
                }
            }
        }

        // Probe RSS at configured cadence.
        if iter % probe_cfg.every_n_iterations == 0 {
            probe_count += 1;
            let rss = current_rss_mib();
            final_rss = rss;
            if rss > peak_rss { peak_rss = rss; }
            let delta = rss - baseline_rss;

            if probe_count > probe_cfg.warmup_skip {
                let heap_bytes = azul_layout::probe::malloc_heap_bytes();
                emit_jsonl(
                    &scenario.output.jsonl_path,
                    &format!(
                        r#"{{"ev":"probe","iter":{},"rss_mib":{:.2},"delta_mib":{:.2},"heap_mib":{:.2}}}"#,
                        iter, rss, delta, (heap_bytes as f64) / 1_048_576.0
                    ),
                );

                if probe_cfg.memory_breakdown {
                    if let Some(line) = breakdown_line(iter, &window) {
                        emit_jsonl(&scenario.output.jsonl_path, &line);
                    }
                }

                if let Some(cap) = probe_cfg.assert_absolute_mib_max {
                    if rss > cap && absolute_breach_at.is_none() {
                        absolute_breach_at = Some(iter);
                    }
                }
            }
        }
    }

    // Final RSS sample.
    let final_rss_last = current_rss_mib();
    if final_rss_last > final_rss { final_rss = final_rss_last; }
    if final_rss_last > peak_rss { peak_rss = final_rss_last; }
    let growth = final_rss - baseline_rss;

    let duration_s = t_start.elapsed().as_secs_f64();

    // Summary line.
    let mut status = "pass";
    let mut reason = String::new();
    if let Some(cap) = probe_cfg.assert_growth_mib_max {
        if growth > cap {
            status = "fail";
            reason = format!("growth {:.2} > {:.2}", growth, cap);
        }
    }
    if let Some(iter) = absolute_breach_at {
        status = "fail";
        if !reason.is_empty() { reason.push_str("; "); }
        if let Some(cap) = probe_cfg.assert_absolute_mib_max {
            reason.push_str(&format!("absolute breach at iter {} (>{:.2})", iter, cap));
        }
    }

    emit_jsonl(
        &scenario.output.summary_path,
        &format!(
            r#"{{"ev":"summary","name":{:?},"iterations":{},"baseline_mib":{:.2},"final_mib":{:.2},"peak_mib":{:.2},"growth_mib":{:.2},"duration_s":{:.2},"status":{:?},"reason":{:?}}}"#,
            scenario.name, iterations, baseline_rss, final_rss, peak_rss, growth, duration_s, status, reason
        ),
    );

    let exit = if status == "pass" { 0 } else { 1 };
    std::process::exit(exit);
}

fn resolve_range(scenario: &Scenario) -> (usize, usize) {
    if let Some(l) = scenario.r#loop.as_ref() {
        if let Some(r) = l.steps_range {
            let a = r[0].min(scenario.steps.len());
            let b = r[1].min(scenario.steps.len()).max(a);
            return (a, b);
        }
    }
    (0, scenario.steps.len())
}

fn emit_jsonl(path: &str, line: &str) {
    if path == "-" {
        eprintln!("{}", line);
        return;
    }
    use std::io::Write;
    match std::fs::OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => {
            let _ = writeln!(f, "{}", line);
        }
        Err(_) => eprintln!("{}", line),
    }
}

fn current_rss_mib() -> f64 {
    let (rss, _virt) = azul_layout::probe::current_rss_bytes();
    rss as f64 / 1_048_576.0
}

/// Push a new size into both `current_window_state` (the scenario's source
/// of truth) and the `layout_window`'s mirror (which the layout pass reads).
fn set_size(window: &mut HeadlessWindow, width: f32, height: f32) {
    let dim = azul_core::geom::LogicalSize { width, height };
    if let Some(lw) = window.common.layout_window.as_mut() {
        lw.current_window_state.size.dimensions = dim;
    }
    window.common.current_window_state.size.dimensions = dim;
}

/// Emit one flat `mem` event that attributes every byte we can measure.
///
/// The flat-field shape (no nesting) is what the Python analyzer in
/// `research/calc-regression-triage/leak-deep-dive/scripts/` expects:
/// it fits a linear model `field_bytes = a * iter + b` to every key and
/// ranks by slope. Anything with a positive slope is a leak suspect.
fn breakdown_line(iter: u32, window: &HeadlessWindow) -> Option<String> {
    let lw = window.common.layout_window.as_ref()?;

    // StyledDom report — pick the root DOM; headless usually has exactly one.
    let (sd_total, sd_fields) = lw
        .layout_results
        .get(&azul_core::dom::DomId::ROOT_ID)
        .map(|r| {
            let sr = r.styled_dom.memory_report();
            let bd = &sr.css_property_cache;
            let total = sr.total_bytes();
            let fields = format!(
                r#""sd_node_hierarchy":{},"sd_node_data":{},"sd_styled_nodes":{},"sd_cascade_info":{},"sd_tag_ids":{},"sd_non_leaf_nodes":{},"sd_callback_vecs":{},"sd_prop_cascaded":{},"sd_prop_css_props":{},"sd_prop_computed":{},"sd_prop_user_overridden":{},"sd_prop_global":{},"sd_prop_compact":{},"sd_prop_font_sizes":{},"sd_node_count":{}"#,
                sr.node_hierarchy_bytes,
                sr.node_data_bytes,
                sr.styled_nodes_bytes,
                sr.cascade_info_bytes,
                sr.tag_ids_bytes,
                sr.non_leaf_nodes_bytes,
                sr.callback_vecs_bytes,
                bd.cascaded_props_bytes,
                bd.css_props_bytes,
                bd.computed_values_bytes,
                bd.user_overridden_bytes,
                bd.global_css_props_bytes,
                bd.compact_cache_bytes,
                bd.resolved_font_sizes_bytes,
                sr.node_count,
            );
            (total, fields)
        })
        .unwrap_or((0, String::new()));

    let sc = lw.layout_cache.memory_report();
    let sc_tree = sc.tree_report.as_ref();
    let sc_fields = format!(
        r#""sc_tree":{},"sc_tree_hot":{},"sc_tree_warm":{},"sc_tree_warm_inline":{},"sc_tree_warm_taffy":{},"sc_tree_cold":{},"sc_tree_children_arena":{},"sc_tree_dom_to_layout":{},"sc_cache_map":{},"sc_calculated_pos":{},"sc_previous_pos":{},"sc_float_cache":{},"sc_counters":{},"sc_scroll_ids":{},"sc_scroll_id_to_node":{},"sc_cached_display":{}"#,
        sc.tree_bytes,
        sc_tree.map(|t| t.hot_bytes).unwrap_or(0),
        sc_tree.map(|t| t.warm_bytes).unwrap_or(0),
        sc_tree.map(|t| t.warm_inline_layout_bytes).unwrap_or(0),
        sc_tree.map(|t| t.warm_taffy_cache_bytes).unwrap_or(0),
        sc_tree.map(|t| t.cold_bytes).unwrap_or(0),
        sc_tree.map(|t| t.children_arena_bytes).unwrap_or(0),
        sc_tree.map(|t| t.dom_to_layout_bytes).unwrap_or(0),
        sc.cache_map_bytes,
        sc.calculated_positions_bytes,
        sc.previous_positions_bytes,
        sc.float_cache_bytes,
        sc.counters_bytes,
        sc.scroll_ids_bytes,
        sc.scroll_id_to_node_id_bytes,
        sc.cached_display_list_bytes,
    );

    let tc = lw.text_cache.memory_report();
    let tc_fields = format!(
        r#""tc_logical_items":{},"tc_visual_items":{},"tc_shaped_items":{},"tc_shaped_glyphs":{},"tc_shaped_clusters":{},"tc_per_item":{},"tc_logical_entries":{},"tc_visual_entries":{},"tc_shaped_entries":{},"tc_per_item_entries":{}"#,
        tc.logical_items_bytes,
        tc.visual_items_bytes,
        tc.shaped_items_bytes,
        tc.shaped_glyph_bytes,
        tc.shaped_cluster_text_bytes,
        tc.per_item_shaped_bytes,
        tc.logical_items_entries,
        tc.visual_items_entries,
        tc.shaped_items_entries,
        tc.per_item_shaped_entries,
    );

    // Managers — no memory_report APIs, so sum HashMap/BTreeMap lengths.
    // Byte-accuracy isn't needed: we just want a "this count grows" signal.
    let mut gpu_transform_keys = 0usize;
    let mut gpu_opacity_keys = 0usize;
    let mut gpu_css_transform_keys = 0usize;
    let mut gpu_scrollbar_opacity_keys = 0usize;
    for (_dom_id, cache) in &lw.gpu_state_manager.caches {
        gpu_transform_keys += cache.transform_keys.len() + cache.h_transform_keys.len();
        gpu_opacity_keys += cache.opacity_keys.len();
        gpu_css_transform_keys += cache.css_transform_keys.len();
        gpu_scrollbar_opacity_keys +=
            cache.scrollbar_v_opacity_keys.len() + cache.scrollbar_h_opacity_keys.len();
    }
    let fade_states_len = lw.gpu_state_manager.fade_states.len();
    let parsed_fonts_len = lw
        .font_manager
        .parsed_fonts
        .lock()
        .map(|m| m.len())
        .unwrap_or(0);
    let font_chain_len = lw.font_manager.font_chain_cache.len();
    let font_hash_families = lw.font_manager.font_hash_to_families.len();
    let renderer_fonts = lw.renderer_resources.currently_registered_fonts.len();
    let renderer_images = lw.renderer_resources.currently_registered_images.len();

    let (scroll_states, scrollbar_states) = lw.scroll_manager.debug_counts();
    let (hover_points, hover_total) = lw.hover_manager.debug_counts();
    let (vv_states, vv_pipelines) = lw.virtual_view_manager.debug_counts();
    let (gesture_sessions, gesture_long_press) =
        lw.gesture_drag_manager.debug_counts();
    let rr = &lw.renderer_resources;

    // --- CpuBackend / window-level probes ---
    #[cfg(feature = "cpurender")]
    let (cpu_layers, cpu_next_layer_id, cpu_last_frame_px, cpu_prev_dl_items,
         cpu_glyph_paths, cpu_glyph_cells) = {
        let cb = &window.cpu_backend;
        let (layers, next_id) = cb.compositor.as_ref()
            .map(|c| (c.layers.len(), c.next_layer_id_peek()))
            .unwrap_or((0, 0));
        let last_bytes = cb.last_frame.as_ref()
            .map(|p| (p.width() * p.height()) as usize * 4)
            .unwrap_or(0);
        let prev_dl = cb.previous_display_list.as_ref()
            .map(|dl| dl.items.len()).unwrap_or(0);
        (layers, next_id, last_bytes, prev_dl,
         cb.glyph_cache.paths_len(), cb.glyph_cache.cells_len())
    };
    #[cfg(not(feature = "cpurender"))]
    let (cpu_layers, cpu_next_layer_id, cpu_last_frame_px, cpu_prev_dl_items,
         cpu_glyph_paths, cpu_glyph_cells) = (0usize, 0u64, 0usize, 0usize, 0usize, 0usize);

    let prev_ws = window.common.previous_window_state.is_some() as usize;
    let hit_test_entries: usize = window.cpu_backend.hit_tester.node_rects_total();

    let mgr_fields = format!(
        r#""mgr_layout_results":{},"mgr_gpu_caches":{},"mgr_gpu_transform_keys":{},"mgr_gpu_opacity_keys":{},"mgr_gpu_css_transform_keys":{},"mgr_gpu_scrollbar_opacity_keys":{},"mgr_gpu_fade_states":{},"mgr_font_parsed":{},"mgr_font_chain":{},"mgr_font_hash_families":{},"mgr_renderer_fonts":{},"mgr_renderer_images":{},"mgr_renderer_image_key_map":{},"mgr_renderer_last_frame_fonts":{},"mgr_renderer_font_families":{},"mgr_renderer_font_id_map":{},"mgr_renderer_font_hash_map":{},"mgr_scroll_states":{},"mgr_scrollbar_states":{},"mgr_hover_points":{},"mgr_hover_history_total":{},"mgr_vv_states":{},"mgr_vv_pipelines":{},"mgr_gesture_sessions":{},"mgr_gesture_long_press":{},"mgr_dirty_text_nodes":{},"mgr_timers":{},"mgr_threads":{},"probe_events":{},"cpu_layers":{},"cpu_next_layer_id":{},"cpu_last_frame_bytes":{},"cpu_prev_dl_items":{},"cpu_glyph_paths":{},"cpu_glyph_cells":{},"cpu_hit_rects":{},"prev_window_state":{}"#,
        lw.layout_results.len(),
        lw.gpu_state_manager.caches.len(),
        gpu_transform_keys,
        gpu_opacity_keys,
        gpu_css_transform_keys,
        gpu_scrollbar_opacity_keys,
        fade_states_len,
        parsed_fonts_len,
        font_chain_len,
        font_hash_families,
        renderer_fonts,
        renderer_images,
        rr.image_key_map.len(),
        rr.last_frame_registered_fonts.len(),
        rr.font_families_map.len(),
        rr.font_id_map.len(),
        rr.font_hash_map.len(),
        scroll_states,
        scrollbar_states,
        hover_points,
        hover_total,
        vv_states,
        vv_pipelines,
        gesture_sessions,
        gesture_long_press,
        lw.dirty_text_nodes.len(),
        lw.timers.len(),
        lw.threads.len(),
        azul_layout::probe::Probe::peek_len(),
        cpu_layers,
        cpu_next_layer_id,
        cpu_last_frame_px,
        cpu_prev_dl_items,
        cpu_glyph_paths,
        cpu_glyph_cells,
        hit_test_entries,
        prev_ws,
    );

    Some(format!(
        r#"{{"ev":"mem","iter":{},"sd_total":{},"sc_total":{},"tc_total":{},{},{},{},{}}}"#,
        iter,
        sd_total,
        sc.total_bytes(),
        tc.total_bytes(),
        sd_fields,
        sc_fields,
        tc_fields,
        mgr_fields,
    ))
}
