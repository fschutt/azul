//! Unified reftest pipeline: Chrome screenshot → Azul render → pixel diff.
//!
//! Used by both `reftest` (batch + headless) and `autodebug` (discovery phase).
//! Ensures consistent behavior: shared font cache, persistent Chrome CDP,
//! two-pass Azul rendering (fast timing + debug diagnostics).

use std::path::Path;
use std::time::Instant;

use super::autodebug::cdp::{ChromeCdp, ChromePerformanceTiming};
use super::{
    compare_images, generate_azul_rendering_sized_cached,
    generate_chrome_screenshot_with_debug, DebugData, PASS_THRESHOLD_PIXELS,
};

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
    Process(String), // chrome_path
}

impl ChromeBackend {
    /// Try CDP first, fall back to process-per-test.
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

    /// Take screenshot + extract layout.
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
                cdp.screenshot_and_layout(
                    test_file, chrome_img, Some(chrome_layout_json), width, height,
                ).map_err(|e| format!("CDP screenshot: {}", e))?;
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

/// Unified reftest pipeline.
pub struct ReftestPipeline {
    pub fc_cache: rust_fontconfig::FcFontCache,
    pub chrome: ChromeBackend,
}

impl ReftestPipeline {
    pub fn new(fc_cache: rust_fontconfig::FcFontCache, chrome_path: &str) -> Self {
        Self {
            fc_cache,
            chrome: ChromeBackend::new(chrome_path),
        }
    }

    /// Run a single test: Chrome screenshot → Azul render (2 passes) → pixel diff.
    ///
    /// Pass 1 (fast): `debug_messages=None` — accurate timing, no string overhead.
    /// Pass 2 (debug): `debug_messages=Some(...)` — collects layout diagnostics.
    pub fn run_test(
        &mut self,
        test_file: &Path,
        chrome_img: &Path,
        azul_img: &Path,
        chrome_layout_json: &Path,
        width: u32,
        height: u32,
        dpi_factor: f32,
    ) -> Result<TestRunResult, String> {
        let test_name = test_file.file_stem()
            .unwrap().to_string_lossy().to_string();

        // 1. Chrome screenshot (reuses persistent CDP if available)
        let (chrome_layout_data, chrome_timing) = if !chrome_img.exists() {
            self.chrome.screenshot(
                test_file, chrome_img, chrome_layout_json, width, height,
            )?
        } else {
            let data = std::fs::read_to_string(chrome_layout_json).unwrap_or_default();
            (data, None)
        };

        // 2. Azul render — fast pass for timing
        let t_azul = Instant::now();
        let mut debug_data = generate_azul_rendering_sized_cached(
            test_file, azul_img, width, height, dpi_factor, &self.fc_cache,
        ).map_err(|e| format!("Azul render: {}", e))?;
        let azul_total_ms = t_azul.elapsed().as_secs_f64() * 1000.0;

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
        println!("  Azul:   xml={:.0}ms layout={:.0}ms render={:.0}ms total={:.1}ms",
            azul_timing.xml_ms, azul_timing.layout_ms, azul_timing.render_ms, azul_timing.total_ms);
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
}
