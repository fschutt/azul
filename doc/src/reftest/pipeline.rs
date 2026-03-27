//! Unified reftest pipeline: Chrome screenshot → Azul render → pixel diff.
//!
//! Uses `FontContext` to share font data across tests without sharing
//! layout state. Each test gets a fresh `LayoutWindow`.

use std::path::Path;
use std::time::Instant;

use super::autodebug::cdp::{ChromeCdp, ChromePerformanceTiming};
use super::{
    compare_images, generate_chrome_screenshot_with_debug,
    DebugData, TestMetadata, PASS_THRESHOLD_PIXELS,
};

/// Per-test timing breakdown (microseconds).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AzulTiming {
    pub parse_us: f64,
    pub layout_us: f64,
    pub render_us: f64,
    pub save_us: f64,
    pub total_us: f64,
}

/// Result of running a single reftest.
#[derive(Debug)]
pub struct TestRunResult {
    pub test_name: String,
    pub debug_data: DebugData,
    pub chrome_timing: Option<ChromePerformanceTiming>,
    pub azul_timing: AzulTiming,
    pub diff_pixels: usize,
    pub passed: bool,
    pub chrome_layout_data: String,
}

/// Chrome backend: persistent CDP or per-test process.
pub enum ChromeBackend {
    Cdp(ChromeCdp),
    Process(String),
}

impl ChromeBackend {
    pub fn new(chrome_path: &str) -> Self {
        match ChromeCdp::launch(chrome_path) {
            Ok(cdp) => { println!("  Chrome CDP connected"); ChromeBackend::Cdp(cdp) }
            Err(e) => { println!("  Chrome CDP failed ({}), using process", e); ChromeBackend::Process(chrome_path.to_string()) }
        }
    }

    fn screenshot(&mut self, test_file: &Path, chrome_img: &Path, chrome_layout_json: &Path, width: u32, height: u32,
    ) -> Result<(String, Option<ChromePerformanceTiming>), String> {
        match self {
            ChromeBackend::Cdp(cdp) => {
                let needs_convert = chrome_img.extension().map_or(false, |e| e == "webp");
                let save_path = if needs_convert { chrome_img.with_extension("png") } else { chrome_img.to_path_buf() };
                cdp.screenshot_and_layout(test_file, &save_path, Some(chrome_layout_json), width, height)
                    .map_err(|e| format!("CDP: {}", e))?;
                if needs_convert {
                    let img = image::open(&save_path).map_err(|e| format!("open: {}", e))?.to_rgba8();
                    let enc = image::codecs::webp::WebPEncoder::new_lossless(
                        std::io::BufWriter::new(std::fs::File::create(chrome_img).map_err(|e| format!("{}", e))?));
                    use image::ImageEncoder;
                    enc.write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
                        .map_err(|e| format!("{}", e))?;
                    let _ = std::fs::remove_file(&save_path);
                }
                let timing = cdp.get_performance_metrics().ok();
                let layout_data = std::fs::read_to_string(chrome_layout_json).unwrap_or_default();
                Ok((layout_data, timing))
            }
            ChromeBackend::Process(p) => {
                let data = generate_chrome_screenshot_with_debug(p, test_file, chrome_img, chrome_layout_json, width, height)
                    .map_err(|e| format!("{}", e))?;
                Ok((data, None))
            }
        }
    }
}

/// Extract metadata from raw XML without building a DOM tree.
fn extract_metadata_from_string(xml: &str) -> TestMetadata {
    let mut m = TestMetadata::default();
    if let Some(start) = xml.find("<title>").or_else(|| xml.find("<title ")) {
        let after = &xml[start..];
        if let Some(close_start) = after.find('>') {
            let content_start = start + close_start + 1;
            if let Some(end) = xml[content_start..].find("</title>") {
                m.title = xml[content_start..content_start + end].trim().to_string();
            }
        }
    }
    for tag in ["assert", "flags"] {
        let needle = format!("name=\"{}\"", tag);
        if let Some(pos) = xml.find(&needle) {
            let region = &xml[pos.saturating_sub(100)..xml.len().min(pos + 200)];
            if let Some(c_start) = region.find("content=\"") {
                let after = &region[c_start + 9..];
                if let Some(c_end) = after.find('"') {
                    match tag {
                        "assert" => m.assert_content = after[..c_end].to_string(),
                        "flags" => m.flags = after[..c_end].to_string(),
                        _ => {}
                    }
                }
            }
        }
    }
    m
}

/// Unified reftest pipeline.
pub struct ReftestPipeline {
    pub chrome: ChromeBackend,
    pub font_context: azul_layout::FontContext,
}

impl ReftestPipeline {
    pub fn new(chrome_path: &str) -> Result<Self, String> {
        let t0 = Instant::now();
        let registry = azul_layout::FcFontRegistry::new();
        let _had_cache = registry.load_from_disk_cache();
        registry.spawn_scout_and_builders();
        let os = rust_fontconfig::OperatingSystem::current();
        let common_stacks = rust_fontconfig::config::tokenize_common_families(os);
        registry.request_fonts(&common_stacks);
        let fc_cache = registry.into_fc_font_cache();

        let mut font_context = azul_layout::FontContext::from_fc_cache(fc_cache);

        // Pre-resolve font chains by doing a small warmup parse.
        // This populates font_chain_cache with system font stacks.
        let warmup = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>x</p></body></html>";
        if let Ok(dom) = azul_layout::xml::parse_xml_to_styled_dom(warmup) {
            font_context.pre_resolve_chains_for_dom(&dom, &azul_css::system::Platform::current());
        }
        font_context.load_fonts_for_chains();

        println!("  Font context: {:.0}ms ({} chains, {} parsed fonts)",
            t0.elapsed().as_secs_f64() * 1000.0,
            font_context.font_chain_cache.len(),
            font_context.parsed_fonts.lock().map(|m| m.len()).unwrap_or(0));

        Ok(Self {
            chrome: ChromeBackend::new(chrome_path),
            font_context,
        })
    }

    pub fn run_test(&mut self, test_file: &Path, chrome_img: &Path, azul_img: &Path,
        chrome_layout_json: &Path, width: u32, height: u32,
    ) -> Result<TestRunResult, String> {
        let test_name = test_file.file_stem().unwrap().to_string_lossy().to_string();

        // Chrome
        let (chrome_layout_data, chrome_timing) = if !chrome_img.exists() {
            self.chrome.screenshot(test_file, chrome_img, chrome_layout_json, width, height)?
        } else {
            (std::fs::read_to_string(chrome_layout_json).unwrap_or_default(), None)
        };

        // Debug pass (collects layout diagnostics)
        let (debug_pass_data, _) = render_xhtml_to_webp(&self.font_context, test_file, azul_img, width, height, true)?;
        // Timing pass (accurate measurement)
        let (mut debug_data, azul_timing) = render_xhtml_to_webp(&self.font_context, test_file, azul_img, width, height, false)?;

        debug_data.chrome_layout = chrome_layout_data.clone();
        debug_data.render_warnings = debug_pass_data.render_warnings;

        // Diff
        let diff_pixels = compare_images(chrome_img, azul_img).map_err(|e| format!("{}", e))?;
        let passed = diff_pixels <= PASS_THRESHOLD_PIXELS;

        if let Some(ref ct) = chrome_timing { println!("  Chrome: {}", ct); }
        println!("  Azul:   parse={:.0}us layout={:.0}us render={:.0}us save={:.0}us total={:.0}us ({:.2}ms)",
            azul_timing.parse_us, azul_timing.layout_us, azul_timing.render_us,
            azul_timing.save_us, azul_timing.total_us, azul_timing.total_us / 1000.0);
        println!("  Diff:   {} pixels ({})", diff_pixels, if passed { "PASS" } else { "FAIL" });

        Ok(TestRunResult { test_name, debug_data, chrome_timing, azul_timing, diff_pixels, passed, chrome_layout_data })
    }
}

/// Core render function: XHTML → parse → layout → CPU render → WebP.
/// Creates a fresh LayoutWindow from FontContext per call.
pub fn render_xhtml_to_webp(
    font_context: &azul_layout::FontContext,
    test_file: &Path,
    output_file: &Path,
    width: u32,
    height: u32,
    collect_debug: bool,
) -> Result<(DebugData, AzulTiming), String> {
    use azul_core::dom::DomId;
    use azul_core::geom::LogicalSize;
    use azul_layout::callbacks::ExternalSystemCallbacks;
    use azul_layout::window_state::FullWindowState;

    let t_parse = Instant::now();
    let xml_content = std::fs::read_to_string(test_file).map_err(|e| format!("read: {}", e))?;
    let styled_dom = azul_layout::xml::parse_xml_to_styled_dom(&xml_content).map_err(|e| format!("parse: {}", e))?;
    let parse_us = t_parse.elapsed().as_secs_f64() * 1_000_000.0;

    // Fresh LayoutWindow from shared FontContext — no stale cache
    let mut layout_window = azul_layout::LayoutWindow::from_font_context(font_context)
        .map_err(|e| format!("LayoutWindow: {:?}", e))?;

    let t_layout = Instant::now();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize { width: width as f32, height: height as f32 };
    ws.size.dpi = 96;
    let mut rr = azul_core::resources::RendererResources::default();
    let ext = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = if collect_debug { Some(Vec::new()) } else { None };

    layout_window.layout_and_generate_display_list(styled_dom, &ws, &mut rr, &ext, &mut debug_messages)
        .map_err(|e| format!("layout: {}", e))?;

    let dl = layout_window.layout_results.remove(&DomId::ROOT_ID).ok_or("No layout result")?.display_list;
    let layout_us = t_layout.elapsed().as_secs_f64() * 1_000_000.0;

    let t_render = Instant::now();
    let dpi = 1.0_f32;
    let mut gc = azul_layout::glyph_cache::GlyphCache::new();
    let pixmap = azul_layout::cpurender::render_with_font_manager(
        &dl, &rr, &layout_window.font_manager,
        azul_layout::cpurender::RenderOptions { width: width as f32, height: height as f32, dpi_factor: dpi },
        &mut gc,
    ).map_err(|e| format!("render: {}", e))?;
    let render_us = t_render.elapsed().as_secs_f64() * 1_000_000.0;

    let t_save = Instant::now();
    let img = image::RgbaImage::from_raw(
        (width as f32 * dpi) as u32, (height as f32 * dpi) as u32, pixmap.data().to_vec(),
    ).ok_or("image")?;
    let enc = image::codecs::webp::WebPEncoder::new_lossless(
        std::io::BufWriter::new(std::fs::File::create(output_file).map_err(|e| format!("{}", e))?));
    use image::ImageEncoder;
    enc.write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("{}", e))?;
    let save_us = t_save.elapsed().as_secs_f64() * 1_000_000.0;
    let total_us = parse_us + layout_us + render_us;

    let metadata = extract_metadata_from_string(&xml_content);
    let mut dd = DebugData::new(xml_content);
    dd.title = metadata.title;
    dd.assert_content = metadata.assert_content;
    dd.flags = metadata.flags;
    dd.author = metadata.author;
    dd.xml_formatting_time_us = parse_us.round() as u64;
    dd.layout_time_us = layout_us.round() as u64;
    dd.render_time_us = render_us.round() as u64;
    if let Some(msgs) = debug_messages {
        dd.render_warnings = msgs.into_iter()
            .map(|m| format!("[{:?}] {}", m.message_type, m.message.as_str()))
            .collect();
    }

    Ok((dd, AzulTiming { parse_us, layout_us, render_us, save_us, total_us }))
}
