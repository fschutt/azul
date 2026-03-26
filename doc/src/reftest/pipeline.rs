//! Unified reftest pipeline: Chrome screenshot → Azul render → pixel diff.
//!
//! Uses FcFontRegistry (background scout + builders) instead of
//! FcFontCache::build() so fonts are discovered and parsed in background
//! threads. Parsed font data is shared across all tests via Arc.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use super::autodebug::cdp::{ChromeCdp, ChromePerformanceTiming};
use super::{
    compare_images, generate_chrome_screenshot_with_debug,
    DebugData, TestMetadata, PASS_THRESHOLD_PIXELS,
};

/// Extract test metadata (title, author, etc.) from raw XML string
/// without building a DOM tree — just scans for <title> and <meta> tags.
fn extract_metadata_from_string(xml: &str) -> TestMetadata {
    let mut m = TestMetadata::default();

    // Extract <title>...</title>
    if let Some(start) = xml.find("<title>").or_else(|| xml.find("<title ")) {
        let after = &xml[start..];
        if let Some(close_start) = after.find('>') {
            let content_start = start + close_start + 1;
            if let Some(end) = xml[content_start..].find("</title>") {
                m.title = xml[content_start..content_start + end].trim().to_string();
            }
        }
    }

    // Extract <meta name="assert" content="...">
    for tag in ["assert", "flags"] {
        let needle = format!("name=\"{}\"", tag);
        if let Some(pos) = xml.find(&needle) {
            // Find content="..." in the same tag
            let region = &xml[pos.saturating_sub(100)..xml.len().min(pos + 200)];
            if let Some(c_start) = region.find("content=\"") {
                let after = &region[c_start + 9..];
                if let Some(c_end) = after.find('"') {
                    let val = after[..c_end].to_string();
                    match tag {
                        "assert" => m.assert_content = val,
                        "flags" => m.flags = val,
                        _ => {}
                    }
                }
            }
        }
    }

    m
}

/// Per-test timing breakdown for Azul rendering.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AzulTiming {
    pub xml_ms: f64,
    pub layout_ms: f64,
    pub render_ms: f64,
    pub total_ms: f64,
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
            Ok(cdp) => {
                println!("  Chrome CDP connected (persistent instance)");
                ChromeBackend::Cdp(cdp)
            }
            Err(e) => {
                println!("  Chrome CDP failed ({}), using per-test process", e);
                ChromeBackend::Process(chrome_path.to_string())
            }
        }
    }

    fn screenshot(
        &mut self,
        test_file: &Path,
        chrome_img: &Path,
        chrome_layout_json: &Path,
        width: u32,
        height: u32,
    ) -> Result<(String, Option<ChromePerformanceTiming>), String> {
        match self {
            ChromeBackend::Cdp(cdp) => {
                // CDP saves PNG; if output path is .webp, save as temp PNG then convert
                let needs_convert = chrome_img.extension().map_or(false, |e| e == "webp");
                let save_path = if needs_convert {
                    chrome_img.with_extension("png")
                } else {
                    chrome_img.to_path_buf()
                };
                cdp.screenshot_and_layout(
                    test_file, &save_path, Some(chrome_layout_json), width, height,
                ).map_err(|e| format!("CDP screenshot: {}", e))?;
                if needs_convert {
                    // Convert PNG → WebP for consistent format
                    let img = image::open(&save_path)
                        .map_err(|e| format!("open png: {}", e))?
                        .to_rgba8();
                    let encoder = image::codecs::webp::WebPEncoder::new_lossless(
                        std::io::BufWriter::new(std::fs::File::create(chrome_img)
                            .map_err(|e| format!("create webp: {}", e))?)
                    );
                    use image::ImageEncoder;
                    encoder.write_image(
                        img.as_raw(), img.width(), img.height(),
                        image::ExtendedColorType::Rgba8,
                    ).map_err(|e| format!("encode webp: {}", e))?;
                    let _ = std::fs::remove_file(&save_path);
                }
                let timing = cdp.get_performance_metrics().ok();
                let layout_data = std::fs::read_to_string(chrome_layout_json)
                    .unwrap_or_default();
                Ok((layout_data, timing))
            }
            ChromeBackend::Process(chrome_path) => {
                let layout_data = generate_chrome_screenshot_with_debug(
                    chrome_path, test_file, chrome_img, chrome_layout_json, width, height,
                ).map_err(|e| format!("{}", e))?;
                Ok((layout_data, None))
            }
        }
    }
}

/// Unified reftest pipeline with shared resources.
///
/// Font loading uses FcFontRegistry (background scout + builder threads).
/// Parsed font data is shared across all tests via Arc<Mutex<HashMap>>.
pub struct ReftestPipeline {
    pub chrome: ChromeBackend,
    /// Shared LayoutWindow — fonts parsed on first use, then shared.
    /// Uses `from_arc_shared()` internally so parsed font bytes persist.
    pub layout_window: azul_layout::LayoutWindow,
}

impl ReftestPipeline {
    /// Create pipeline with background font loading (FcFontRegistry).
    ///
    /// Spawns scout + builder threads immediately. Chrome screenshot work
    /// can happen while fonts are being discovered in the background.
    pub fn new(chrome_path: &str) -> Result<Self, String> {
        // Use FcFontRegistry: background threads discover + parse fonts
        let t0 = Instant::now();
        let registry = azul_layout::FcFontRegistry::new();
        let had_cache = registry.load_from_disk_cache();
        registry.spawn_scout_and_builders();

        // Request common system fonts (blocks until builder threads parse them)
        let os = rust_fontconfig::OperatingSystem::current();
        let common_stacks = rust_fontconfig::config::tokenize_common_families(os);
        registry.request_fonts(&common_stacks);
        let fc_cache = registry.into_fc_font_cache();
        println!("  Font registry: {:.0}ms ({} fonts, disk_cache={})",
            t0.elapsed().as_secs_f64() * 1000.0,
            fc_cache.len(),
            had_cache.is_some());

        let layout_window = azul_layout::LayoutWindow::new(fc_cache)
            .map_err(|e| format!("LayoutWindow: {:?}", e))?;

        Ok(Self {
            chrome: ChromeBackend::new(chrome_path),
            layout_window,
        })
    }

    /// Run a single test: Chrome screenshot → Azul render (2 passes) → pixel diff.
    ///
    /// Pass 1 (debug): collects layout diagnostics + warms font cache.
    /// Pass 2 (timing): accurate measurement without debug overhead.
    pub fn run_test(
        &mut self,
        test_file: &Path,
        chrome_img: &Path,
        azul_img: &Path,
        chrome_layout_json: &Path,
        width: u32,
        height: u32,
    ) -> Result<TestRunResult, String> {
        let test_name = test_file.file_stem()
            .unwrap().to_string_lossy().to_string();

        // 1. Chrome screenshot
        let (chrome_layout_data, chrome_timing) = if !chrome_img.exists() {
            self.chrome.screenshot(
                test_file, chrome_img, chrome_layout_json, width, height,
            )?
        } else {
            let data = std::fs::read_to_string(chrome_layout_json).unwrap_or_default();
            (data, None)
        };

        // 2a. Debug pass — warms font cache + collects debug_messages
        let _ = self.render_azul(test_file, azul_img, width, height, true)?;

        // 2b. Timing pass — accurate measurement, no debug overhead
        let t_azul = Instant::now();
        let (debug_data, _) = self.render_azul(test_file, azul_img, width, height, false)?;
        let azul_total_ms = t_azul.elapsed().as_secs_f64() * 1000.0;

        let mut debug_data = debug_data;
        debug_data.chrome_layout = chrome_layout_data.clone();

        let azul_timing = AzulTiming {
            xml_ms: debug_data.xml_formatting_time_ms as f64,
            layout_ms: debug_data.layout_time_ms as f64,
            render_ms: debug_data.render_time_ms as f64,
            total_ms: azul_total_ms,
        };

        // 3. Pixel comparison
        let diff_pixels = compare_images(chrome_img, azul_img)
            .map_err(|e| format!("compare: {}", e))?;
        let passed = diff_pixels <= PASS_THRESHOLD_PIXELS;

        // 4. Print timing comparison
        if let Some(ref ct) = chrome_timing {
            println!("  Chrome: {}", ct);
        }
        println!("  Azul:   parse={:.1}ms layout={:.1}ms render={:.1}ms save={:.1}ms total={:.1}ms",
            azul_timing.xml_ms, azul_timing.layout_ms, azul_timing.render_ms,
            azul_timing.total_ms - azul_timing.xml_ms - azul_timing.layout_ms - azul_timing.render_ms,
            azul_timing.total_ms);
        println!("  Diff:   {} pixels ({})",
            diff_pixels, if passed { "PASS" } else { "FAIL" });

        Ok(TestRunResult {
            test_name,
            debug_data,
            chrome_timing,
            azul_timing,
            diff_pixels,
            passed,
            chrome_layout_data,
        })
    }

    /// Render a test file with Azul using the shared LayoutWindow.
    /// `collect_debug`: if true, enables layout debug messages (slower but collects diagnostics).
    fn render_azul(
        &mut self,
        test_file: &Path,
        output_file: &Path,
        width: u32,
        height: u32,
        collect_debug: bool,
    ) -> Result<(DebugData, AzulTiming), String> {
        use azul_core::dom::DomId;
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_layout::callbacks::ExternalSystemCallbacks;
        use azul_layout::window_state::FullWindowState;

        // Parse XHTML → StyledDom via fast path (tokenizer → FastDom → cascade)
        let t_parse = Instant::now();
        let xml_content = std::fs::read_to_string(test_file)
            .map_err(|e| format!("read: {}", e))?;
        let styled_dom = azul_layout::xml::parse_xml_to_styled_dom(&xml_content)
            .map_err(|e| format!("parse: {}", e))?;
        let parse_ms = t_parse.elapsed().as_secs_f64() * 1000.0;

        // Layout
        let t_layout = Instant::now();
        let mut fake_window_state = FullWindowState::default();
        fake_window_state.size.dimensions = LogicalSize {
            width: width as f32,
            height: height as f32,
        };
        fake_window_state.size.dpi = 96;
        let mut renderer_resources = azul_core::resources::RendererResources::default();
        let external = ExternalSystemCallbacks::rust_internal();

        // Use shared layout_window (fonts already loaded from first test)
        let mut debug_messages = if collect_debug { Some(Vec::new()) } else { None };
        self.layout_window.layout_and_generate_display_list(
            styled_dom,
            &fake_window_state,
            &mut renderer_resources,
            &external,
            &mut debug_messages,
        ).map_err(|e| format!("layout: {}", e))?;

        let display_list = self.layout_window
            .layout_results
            .remove(&DomId::ROOT_ID)
            .ok_or("No layout result")?
            .display_list;
        let layout_ms = t_layout.elapsed().as_secs_f64() * 1000.0;

        // CPU render
        let t_render = Instant::now();
        let dpi_factor = 1.0_f32;
        let mut glyph_cache = azul_layout::glyph_cache::GlyphCache::new();
        let pixmap = azul_layout::cpurender::render_with_font_manager(
            &display_list,
            &renderer_resources,
            &self.layout_window.font_manager,
            azul_layout::cpurender::RenderOptions {
                width: width as f32,
                height: height as f32,
                dpi_factor,
            },
            &mut glyph_cache,
        ).map_err(|e| format!("render: {}", e))?;
        let render_ms = t_render.elapsed().as_secs_f64() * 1000.0;

        // Save image (WebP encoding + disk write — not counted in layout/render timing)
        let t_save = Instant::now();
        let pixmap_data = pixmap.data();
        let img = image::RgbaImage::from_raw(
            (width as f32 * dpi_factor) as u32,
            (height as f32 * dpi_factor) as u32,
            pixmap_data.to_vec(),
        ).ok_or("Failed to create image")?;
        let encoder = image::codecs::webp::WebPEncoder::new_lossless(
            std::io::BufWriter::new(std::fs::File::create(output_file)
                .map_err(|e| format!("create: {}", e))?)
        );
        use image::ImageEncoder;
        encoder.write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        ).map_err(|e| format!("encode: {}", e))?;
        let save_ms = t_save.elapsed().as_secs_f64() * 1000.0;

        let metadata = extract_metadata_from_string(&xml_content);
        let mut debug_data = DebugData::new(xml_content);
        debug_data.title = metadata.title;
        debug_data.assert_content = metadata.assert_content;
        debug_data.flags = metadata.flags;
        debug_data.author = metadata.author;
        debug_data.xml_formatting_time_ms = parse_ms as u64;
        debug_data.layout_time_ms = layout_ms as u64;
        debug_data.render_time_ms = render_ms as u64;

        let timing = AzulTiming {
            xml_ms: parse_ms,
            layout_ms,
            render_ms,
            total_ms: t_parse.elapsed().as_secs_f64() * 1000.0,
        };

        Ok((debug_data, timing))
    }
}
