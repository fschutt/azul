//! Automated reftest bug-finding pipeline.
//!
//! Discovers failing reftests across multiple screen sizes, generates rich
//! diagnostic prompts, and dispatches Claude agents in parallel to analyze
//! and fix each bug.

use std::collections::VecDeque;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::spec::executor::{
    self, AgentResult, WorktreeSlot, CODEBASE_CONTEXT, SHUTDOWN_REQUESTED,
};

use super::{
    generate_azul_rendering_sized, generate_azul_rendering_sized_cached,
    generate_chrome_screenshot, get_chrome_path, find_test_files,
    pixels_similar, DebugData,
};

// ── Configuration ──────────────────────────────────────────────────────

/// Screen size preset for multi-resolution testing.
#[derive(Debug, Clone, Copy)]
pub struct ScreenSize {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
}

pub const SIZE_MOBILE: ScreenSize = ScreenSize { name: "mobile", width: 375, height: 667 };
pub const SIZE_TABLET: ScreenSize = ScreenSize { name: "tablet", width: 768, height: 1024 };
pub const SIZE_DESKTOP: ScreenSize = ScreenSize { name: "desktop", width: 1920, height: 1080 };

pub const ALL_SIZES: &[ScreenSize] = &[SIZE_MOBILE, SIZE_TABLET, SIZE_DESKTOP];

/// Configuration for the autodebug pipeline.
pub struct AutodebugConfig {
    pub project_root: PathBuf,
    pub test_dir: PathBuf,
    pub agents: usize,
    pub timeout: Duration,
    pub model: Option<String>,
    pub sizes: Vec<ScreenSize>,
    pub test_filter: Option<String>,
    pub skip_chrome: bool,
    pub retry_failed: bool,
    pub dry_run: bool,
    pub status_only: bool,
    pub collect_only: bool,
    pub cleanup: bool,
}

/// Output directory layout.
fn output_dir(project_root: &Path) -> PathBuf {
    project_root.join("doc/target/autodebug")
}

fn screenshots_dir(project_root: &Path) -> PathBuf {
    output_dir(project_root).join("screenshots")
}

fn prompts_dir(project_root: &Path) -> PathBuf {
    output_dir(project_root).join("prompts")
}

fn patches_dir(project_root: &Path) -> PathBuf {
    output_dir(project_root).join("patches")
}

fn reports_dir(project_root: &Path) -> PathBuf {
    output_dir(project_root).join("reports")
}

// ── Chrome CDP (DevTools Protocol) ─────────────────────────────────────
//
// Launches a single Chrome instance and communicates via WebSocket to
// capture screenshots and extract layout information without respawning
// Chrome for every test file.

mod cdp {
    use std::io::{BufRead, BufReader};
    use std::net::TcpStream;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use tungstenite::{connect, Message, WebSocket, stream::MaybeTlsStream};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    fn next_id() -> u64 {
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    }

    /// A persistent Chrome instance communicating via CDP over WebSocket.
    pub struct ChromeCdp {
        ws: WebSocket<MaybeTlsStream<TcpStream>>,
        child: Child,
        page_enabled: bool,
    }

    impl ChromeCdp {
        /// Launch Chrome headless with remote debugging and connect via WebSocket.
        pub fn launch(chrome_path: &str) -> Result<Self, String> {
            let mut child = Command::new(chrome_path)
                .arg("--headless")
                .arg("--disable-gpu")
                .arg("--no-sandbox")
                .arg("--remote-debugging-port=0")
                .arg("--disable-extensions")
                .arg("--disable-background-networking")
                .arg("--disable-sync")
                .arg("--no-first-run")
                .arg("about:blank")
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("Failed to spawn Chrome: {}", e))?;

            // Parse the debugging port from stderr
            // Chrome prints: "DevTools listening on ws://127.0.0.1:PORT/devtools/browser/UUID"
            let stderr = child.stderr.take()
                .ok_or("No stderr from Chrome")?;
            let reader = BufReader::new(stderr);

            let mut debug_port = None;

            let start = Instant::now();
            let timeout = Duration::from_secs(15);

            for line in reader.lines() {
                if start.elapsed() > timeout { break; }
                let line = line.map_err(|e| format!("Read stderr: {}", e))?;
                // Extract port from ws://127.0.0.1:PORT/...
                if let Some(idx) = line.find("ws://127.0.0.1:") {
                    let after = &line[idx + "ws://127.0.0.1:".len()..];
                    if let Some(slash) = after.find('/') {
                        debug_port = after[..slash].parse::<u16>().ok();
                    }
                    break;
                }
            }

            let port = debug_port.ok_or("Chrome didn't print debug port within 15s")?;

            // Get the page WebSocket URL from the JSON API
            // We need to connect to a page target, not the browser target
            let json_url = format!("http://127.0.0.1:{}/json/list", port);
            let resp = ureq::get(&json_url).call()
                .map_err(|e| format!("GET {}: {}", json_url, e))?;
            let targets: Vec<serde_json::Value> = resp.into_json()
                .map_err(|e| format!("Parse JSON from {}: {}", json_url, e))?;

            // Find the first page target
            let page_ws_url = targets.iter()
                .find(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"))
                .and_then(|t| t.get("webSocketDebuggerUrl").and_then(|v| v.as_str()))
                .ok_or("No page target found in Chrome debug JSON")?
                .to_string();

            // Connect WebSocket to the page target
            let (ws, _response) = connect(&page_ws_url)
                .map_err(|e| format!("WebSocket connect to {}: {}", page_ws_url, e))?;

            Ok(ChromeCdp { ws, child, page_enabled: false })
        }

        /// Send a CDP command and return the result JSON.
        fn send_command(&mut self, method: &str, params: &serde_json::Value) -> Result<serde_json::Value, String> {
            let id = next_id();
            let msg = serde_json::json!({
                "id": id,
                "method": method,
                "params": params,
            });

            self.ws.send(Message::Text(msg.to_string()))
                .map_err(|e| format!("WS send '{}': {}", method, e))?;

            // Read messages until we get our response
            let start = Instant::now();
            let timeout = Duration::from_secs(30);

            loop {
                if start.elapsed() > timeout {
                    return Err(format!("Timeout waiting for CDP response to '{}'", method));
                }

                let msg = self.ws.read()
                    .map_err(|e| format!("WS read: {}", e))?;

                if let Message::Text(text) = msg {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if json.get("id").and_then(|v| v.as_u64()) == Some(id) {
                            if let Some(err) = json.get("error") {
                                return Err(format!("CDP error for '{}': {}", method, err));
                            }
                            return Ok(json.get("result").cloned().unwrap_or(serde_json::Value::Null));
                        }
                    }
                }
            }
        }

        /// Navigate to a file URL and wait for load to finish.
        fn navigate_and_wait(&mut self, url: &str) -> Result<(), String> {
            self.send_command("Page.navigate", &serde_json::json!({ "url": url }))?;

            // Wait for Page.loadEventFired
            let start = Instant::now();
            let timeout = Duration::from_secs(15);

            loop {
                if start.elapsed() > timeout {
                    return Ok(()); // Proceed anyway
                }

                let msg = self.ws.read()
                    .map_err(|e| format!("WS read during navigate: {}", e))?;

                if let Message::Text(text) = msg {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if json.get("method").and_then(|v| v.as_str()) == Some("Page.loadEventFired") {
                            return Ok(());
                        }
                    }
                }
            }
        }

        /// Capture a screenshot at the given viewport size, saving as PNG.
        pub fn screenshot(
            &mut self,
            test_file: &Path,
            output_file: &Path,
            width: u32,
            height: u32,
        ) -> Result<(), String> {
            // Enable Page events once (needed for loadEventFired)
            if !self.page_enabled {
                self.send_command("Page.enable", &serde_json::json!({}))?;
                self.page_enabled = true;
            }

            // Set viewport size
            self.send_command("Emulation.setDeviceMetricsOverride", &serde_json::json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": 1,
                "mobile": false,
            }))?;

            // Navigate to test file
            let canonical = test_file.canonicalize()
                .map_err(|e| format!("canonicalize: {}", e))?;
            let url = format!("file://{}", canonical.display());
            self.navigate_and_wait(&url)?;

            // Small delay for rendering to settle
            std::thread::sleep(Duration::from_millis(50));

            // Capture screenshot
            let result = self.send_command("Page.captureScreenshot", &serde_json::json!({
                "format": "png",
                "clip": {
                    "x": 0,
                    "y": 0,
                    "width": width,
                    "height": height,
                    "scale": 1,
                },
            }))?;

            let data_b64 = result.get("data")
                .and_then(|v| v.as_str())
                .ok_or("No screenshot data in CDP response")?;

            let png_bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                data_b64,
            ).map_err(|e| format!("base64 decode: {}", e))?;

            std::fs::write(output_file, &png_bytes)
                .map_err(|e| format!("write screenshot: {}", e))?;

            Ok(())
        }

        /// Extract layout JSON by evaluating JS in the current page.
        pub fn extract_layout(&mut self) -> Result<String, String> {
            let js = r#"(function() {
                var result = { timestamp: new Date().toISOString(), viewport: { width: window.innerWidth, height: window.innerHeight }, elements: [] };
                var els = document.querySelectorAll('body, body *');
                for (var i = 0; i < Math.min(els.length, 500); i++) {
                    var el = els[i];
                    if (el.tagName === 'SCRIPT' || el.tagName === 'STYLE') continue;
                    var rect = el.getBoundingClientRect();
                    var cs = window.getComputedStyle(el);
                    result.elements.push({
                        i: i, tag: el.tagName.toLowerCase(), id: el.id || null, cls: el.className || null,
                        bounds: { x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.width), h: Math.round(rect.height) },
                        margin: { t: parseFloat(cs.marginTop)||0, r: parseFloat(cs.marginRight)||0, b: parseFloat(cs.marginBottom)||0, l: parseFloat(cs.marginLeft)||0 },
                        padding: { t: parseFloat(cs.paddingTop)||0, r: parseFloat(cs.paddingRight)||0, b: parseFloat(cs.paddingBottom)||0, l: parseFloat(cs.paddingLeft)||0 },
                        display: cs.display, position: cs.position
                    });
                }
                return JSON.stringify(result, null, 2);
            })()"#;

            let result = self.send_command("Runtime.evaluate", &serde_json::json!({
                "expression": js,
                "returnByValue": true,
            }))?;

            let value = result
                .get("result")
                .and_then(|r| r.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("{}");

            Ok(value.to_string())
        }

        /// Take a screenshot and optionally extract layout for a test at a given size.
        pub fn screenshot_and_layout(
            &mut self,
            test_file: &Path,
            screenshot_file: &Path,
            layout_file: Option<&Path>,
            width: u32,
            height: u32,
        ) -> Result<(), String> {
            self.screenshot(test_file, screenshot_file, width, height)?;
            if let Some(lf) = layout_file {
                let json = self.extract_layout()?;
                std::fs::write(lf, &json)
                    .map_err(|e| format!("write layout json: {}", e))?;
            }
            Ok(())
        }
    }

    impl Drop for ChromeCdp {
        fn drop(&mut self) {
            let _ = self.send_command("Browser.close", &serde_json::json!({}));
            let _ = self.child.wait();
        }
    }
}

// ── Pixel Diff Analysis ────────────────────────────────────────────────

/// Result of analyzing pixel differences between Chrome and Azul renders.
#[derive(Debug, Clone)]
pub struct DiffAnalysis {
    pub total_pixels: usize,
    pub diff_pixels: usize,
    pub diff_percent: f64,
    pub regions: Vec<DiffRegion>,
    pub summary: String,
}

/// A contiguous region of differing pixels.
#[derive(Debug, Clone)]
pub struct DiffRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub pixel_count: usize,
    pub avg_delta: f64,
    pub max_delta: f64,
    pub position_desc: String,
    pub cause_hint: String,
}

/// Block-level diff grid cell.
struct BlockGrid {
    cols: usize,
    rows: usize,
    cells: Vec<bool>, // true = different
}

impl BlockGrid {
    fn get(&self, col: usize, row: usize) -> bool {
        if col < self.cols && row < self.rows {
            self.cells[row * self.cols + col]
        } else {
            false
        }
    }

    fn set(&mut self, col: usize, row: usize, val: bool) {
        if col < self.cols && row < self.rows {
            self.cells[row * self.cols + col] = val;
        }
    }
}

/// Analyze pixel differences between Chrome and Azul renders.
///
/// Returns a structured analysis with connected-component regions,
/// position descriptions, and heuristic cause classification.
pub fn analyze_pixel_diff(
    chrome_path: &Path,
    azul_path: &Path,
    size_name: &str,
) -> Result<DiffAnalysis, String> {
    let chrome_img = image::open(chrome_path)
        .map_err(|e| format!("Failed to open chrome image: {}", e))?;
    let azul_img = image::open(azul_path)
        .map_err(|e| format!("Failed to open azul image: {}", e))?;

    let chrome_rgba = chrome_img.to_rgba8();
    let azul_rgba = azul_img.to_rgba8();

    let (cw, ch) = (chrome_rgba.width(), chrome_rgba.height());
    let (aw, ah) = (azul_rgba.width(), azul_rgba.height());

    // Use the smaller dimensions if they differ
    let width = cw.min(aw);
    let height = ch.min(ah);
    let total_pixels = (width as usize) * (height as usize);

    if total_pixels == 0 {
        return Ok(DiffAnalysis {
            total_pixels: 0,
            diff_pixels: 0,
            diff_percent: 0.0,
            regions: Vec::new(),
            summary: "Empty image".to_string(),
        });
    }

    // Build per-pixel diff data: delta values
    const BLOCK_SIZE: u32 = 16;
    let block_cols = ((width + BLOCK_SIZE - 1) / BLOCK_SIZE) as usize;
    let block_rows = ((height + BLOCK_SIZE - 1) / BLOCK_SIZE) as usize;

    // For each block: count differing pixels and accumulate deltas
    let mut block_diff_counts = vec![0u32; block_cols * block_rows];
    let mut block_total_counts = vec![0u32; block_cols * block_rows];
    let mut block_delta_sums = vec![0.0f64; block_cols * block_rows];
    let mut block_max_deltas = vec![0.0f64; block_cols * block_rows];

    let mut total_diff_pixels = 0usize;

    for y in 0..height {
        for x in 0..width {
            let cp = chrome_rgba.get_pixel(x, y);
            let ap = azul_rgba.get_pixel(x, y);

            let bc = (x / BLOCK_SIZE) as usize;
            let br = (y / BLOCK_SIZE) as usize;
            let bi = br * block_cols + bc;
            block_total_counts[bi] += 1;

            if !pixels_similar(cp, ap, 0.1) {
                total_diff_pixels += 1;
                block_diff_counts[bi] += 1;

                // Compute per-channel delta for analysis
                let delta = pixel_delta(cp, ap);
                block_delta_sums[bi] += delta;
                if delta > block_max_deltas[bi] {
                    block_max_deltas[bi] = delta;
                }
            }
        }
    }

    let diff_percent = (total_diff_pixels as f64) / (total_pixels as f64) * 100.0;

    // Build block grid: a block is "different" if >10% of its pixels differ
    let mut grid = BlockGrid {
        cols: block_cols,
        rows: block_rows,
        cells: vec![false; block_cols * block_rows],
    };
    for i in 0..(block_cols * block_rows) {
        if block_total_counts[i] > 0 {
            let pct = block_diff_counts[i] as f64 / block_total_counts[i] as f64;
            grid.cells[i] = pct > 0.10;
        }
    }

    // Connected-component labeling via flood-fill
    let mut labels = vec![0u32; block_cols * block_rows];
    let mut next_label = 1u32;
    for r in 0..block_rows {
        for c in 0..block_cols {
            if grid.get(c, r) && labels[r * block_cols + c] == 0 {
                flood_fill(&grid, &mut labels, c, r, next_label);
                next_label += 1;
            }
        }
    }

    // Gather regions from labels
    let num_regions = (next_label - 1) as usize;
    let mut region_bounds: Vec<(u32, u32, u32, u32)> = vec![(u32::MAX, u32::MAX, 0, 0); num_regions];
    let mut region_pixel_counts = vec![0usize; num_regions];
    let mut region_delta_sums = vec![0.0f64; num_regions];
    let mut region_max_deltas = vec![0.0f64; num_regions];

    for r in 0..block_rows {
        for c in 0..block_cols {
            let label = labels[r * block_cols + c];
            if label == 0 {
                continue;
            }
            let ri = (label - 1) as usize;
            let bi = r * block_cols + c;

            let px = (c as u32) * BLOCK_SIZE;
            let py = (r as u32) * BLOCK_SIZE;
            let px_end = ((c as u32 + 1) * BLOCK_SIZE).min(width);
            let py_end = ((r as u32 + 1) * BLOCK_SIZE).min(height);

            region_bounds[ri].0 = region_bounds[ri].0.min(px);
            region_bounds[ri].1 = region_bounds[ri].1.min(py);
            region_bounds[ri].2 = region_bounds[ri].2.max(px_end);
            region_bounds[ri].3 = region_bounds[ri].3.max(py_end);

            region_pixel_counts[ri] += block_diff_counts[bi] as usize;
            region_delta_sums[ri] += block_delta_sums[bi];
            if block_max_deltas[bi] > region_max_deltas[ri] {
                region_max_deltas[ri] = block_max_deltas[bi];
            }
        }
    }

    // Build DiffRegion objects
    let mut regions = Vec::with_capacity(num_regions);
    for i in 0..num_regions {
        let (x0, y0, x1, y1) = region_bounds[i];
        if x0 == u32::MAX {
            continue; // empty region
        }
        let rw = x1 - x0;
        let rh = y1 - y0;
        let pixel_count = region_pixel_counts[i];
        let avg_delta = if pixel_count > 0 {
            region_delta_sums[i] / pixel_count as f64
        } else {
            0.0
        };
        let max_delta = region_max_deltas[i];

        let position_desc = describe_position(x0, y0, rw, rh, width, height);
        let cause_hint = classify_cause(rw, rh, x0, y0, width, height, avg_delta, max_delta, pixel_count);

        regions.push(DiffRegion {
            x: x0,
            y: y0,
            width: rw,
            height: rh,
            pixel_count,
            avg_delta,
            max_delta,
            position_desc,
            cause_hint,
        });
    }

    // Sort regions by pixel count descending
    regions.sort_by(|a, b| b.pixel_count.cmp(&a.pixel_count));

    // Build summary text
    let mut summary = String::new();
    writeln!(
        summary,
        "## Pixel Diff Analysis ({}, {}x{})",
        size_name, width, height
    ).unwrap();
    writeln!(
        summary,
        "Total pixels: {}, differing: {} ({:.2}%)\n",
        total_pixels, total_diff_pixels, diff_percent
    ).unwrap();

    if regions.is_empty() {
        writeln!(summary, "No significant diff regions found.").unwrap();
    } else {
        for (i, region) in regions.iter().enumerate() {
            writeln!(
                summary,
                "Region {}: {}x{}px at ({}, {}) — {}",
                i + 1, region.width, region.height, region.x, region.y, region.position_desc
            ).unwrap();
            writeln!(
                summary,
                "  {} differing pixels, avg delta: {:.2}, max: {:.2}",
                region.pixel_count, region.avg_delta, region.max_delta
            ).unwrap();
            writeln!(summary, "  Likely cause: {}\n", region.cause_hint).unwrap();
        }
    }

    Ok(DiffAnalysis {
        total_pixels,
        diff_pixels: total_diff_pixels,
        diff_percent,
        regions,
        summary,
    })
}

/// Compute normalized color delta between two pixels (0.0 to 1.0).
fn pixel_delta(p1: &image::Rgba<u8>, p2: &image::Rgba<u8>) -> f64 {
    let dr = (p1[0] as f64 - p2[0] as f64) / 255.0;
    let dg = (p1[1] as f64 - p2[1] as f64) / 255.0;
    let db = (p1[2] as f64 - p2[2] as f64) / 255.0;
    let da = (p1[3] as f64 - p2[3] as f64) / 255.0;
    ((dr * dr + dg * dg + db * db + da * da) / 4.0).sqrt()
}

/// Flood-fill connected component labeling on the block grid.
fn flood_fill(grid: &BlockGrid, labels: &mut [u32], start_c: usize, start_r: usize, label: u32) {
    let mut queue = VecDeque::new();
    queue.push_back((start_c, start_r));
    labels[start_r * grid.cols + start_c] = label;

    while let Some((c, r)) = queue.pop_front() {
        // 4-connected neighbors
        let neighbors = [
            (c.wrapping_sub(1), r),
            (c + 1, r),
            (c, r.wrapping_sub(1)),
            (c, r + 1),
        ];
        for (nc, nr) in neighbors {
            if nc < grid.cols && nr < grid.rows {
                let ni = nr * grid.cols + nc;
                if grid.cells[ni] && labels[ni] == 0 {
                    labels[ni] = label;
                    queue.push_back((nc, nr));
                }
            }
        }
    }
}

/// Describe position of a region in human-readable terms.
fn describe_position(x: u32, y: u32, w: u32, h: u32, img_w: u32, img_h: u32) -> String {
    let cx = x + w / 2;
    let cy = y + h / 2;

    // Check if it spans full width/height
    let full_width = w as f64 > img_w as f64 * 0.8;
    let full_height = h as f64 > img_h as f64 * 0.8;

    if full_width && full_height {
        return "full-page difference".to_string();
    }

    let col = if full_width {
        "full-width"
    } else if cx < img_w / 3 {
        "left"
    } else if cx > img_w * 2 / 3 {
        "right"
    } else {
        "center"
    };

    let row = if full_height {
        "full-height"
    } else if cy < img_h / 3 {
        "top"
    } else if cy > img_h * 2 / 3 {
        "bottom"
    } else {
        "middle"
    };

    if full_width {
        format!("{} band at y={}", row, y)
    } else if full_height {
        format!("{} column at x={}", col, x)
    } else {
        format!("{}-{}", row, col)
    }
}

/// Classify the likely cause of a diff region.
fn classify_cause(
    w: u32, h: u32,
    x: u32, y: u32,
    img_w: u32, img_h: u32,
    avg_delta: f64,
    max_delta: f64,
    pixel_count: usize,
) -> String {
    // Thin strip: likely border or 1px offset
    if w < 8 || h < 8 {
        return "border or 1px offset".to_string();
    }

    // Edge-adjacent: margin collapse or overflow
    let edge_margin = 8;
    if x < edge_margin || y < edge_margin
        || x + w > img_w - edge_margin
        || y + h > img_h - edge_margin
    {
        return "margin collapse or overflow".to_string();
    }

    // High delta: wrong color or missing content
    if avg_delta > 0.5 {
        return "content missing or wrong color".to_string();
    }

    // Scattered small diffs with low delta
    if pixel_count < 200 && avg_delta < 0.3 {
        return "anti-aliasing or text rendering".to_string();
    }

    // Large block: layout shift
    let area = w as usize * h as usize;
    if area > 5000 {
        return "layout shift (wrong position/size)".to_string();
    }

    // Default
    if max_delta > 0.5 {
        "color mismatch".to_string()
    } else {
        "minor rendering difference".to_string()
    }
}

// ── Test Discovery ─────────────────────────────────────────────────────

/// Data for a single failing test across resolutions.
pub struct FailingTestData {
    pub test_name: String,
    pub test_file: PathBuf,
    pub size_results: Vec<SizeResult>,
    pub debug_data: Option<DebugData>,
}

/// Per-resolution result for a test.
pub struct SizeResult {
    pub size: ScreenSize,
    pub chrome_screenshot: PathBuf,
    pub azul_screenshot: PathBuf,
    pub diff_analysis: DiffAnalysis,
}

/// A (test_name, test_file, size) work item for screenshot generation.
struct ScreenshotJob {
    test_name: String,
    test_file: PathBuf,
    size: ScreenSize,
    chrome_file: PathBuf,
    azul_file: PathBuf,
}

/// Discover tests that fail the pixel diff threshold.
///
/// Runs in 3 parallel phases:
///   Phase 1a: Chrome screenshots (parallel Chrome processes)
///   Phase 1b: Azul CPU rendering (parallel via rayon)
///   Phase 1c: Pixel diff analysis (parallel via rayon)
pub fn discover_failing_tests(config: &AutodebugConfig) -> Result<Vec<FailingTestData>, String> {
    let test_dir = &config.test_dir;
    let screenshots = screenshots_dir(&config.project_root);
    fs::create_dir_all(&screenshots)
        .map_err(|e| format!("Failed to create screenshots dir: {}", e))?;

    let mut test_files = find_test_files(test_dir)
        .map_err(|e| format!("Failed to find test files: {}", e))?;
    test_files.sort();

    // Apply filter if specified
    if let Some(ref filter) = config.test_filter {
        test_files.retain(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.contains(filter.as_str()))
                .unwrap_or(false)
        });
    }

    let total_tests = test_files.len();
    let total_jobs = total_tests * config.sizes.len();
    println!("Found {} test files, {} screenshot jobs ({}x {} sizes)",
        total_tests, total_jobs, total_tests, config.sizes.len());

    // Build job list: (test_name, test_file, size, chrome_path, azul_path)
    let mut jobs: Vec<ScreenshotJob> = Vec::with_capacity(total_jobs);
    for test_file in &test_files {
        let test_name = test_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        for size in &config.sizes {
            let chrome_file = screenshots.join(format!("{}_{}_chrome.png", test_name, size.name));
            let azul_file = screenshots.join(format!("{}_{}_azul.webp", test_name, size.name));
            jobs.push(ScreenshotJob {
                test_name: test_name.clone(),
                test_file: test_file.clone(),
                size: *size,
                chrome_file,
                azul_file,
            });
        }
    }

    // Start font discovery in the background — scout enumerates system font
    // directories while builder threads parse font files concurrently.
    // This runs during Phase 1a (Chrome screenshots) so that by the time
    // Phase 1b (Azul rendering) starts, the font cache is mostly populated.
    let font_registry = rust_fontconfig::registry::FcFontRegistry::new();
    font_registry.spawn_scout_and_builders();
    println!("  Font registry: scout + builders started in background");

    // ── Phase 1a: Chrome screenshots (persistent CDP instance) ──────────

    if !config.skip_chrome {
        let chrome_path = get_chrome_path();
        let mut chrome_errors = 0usize;

        // Filter to only jobs that need Chrome screenshots
        let pending_jobs: Vec<&ScreenshotJob> = jobs.iter()
            .filter(|j| !j.chrome_file.exists())
            .collect();
        let skipped = jobs.len() - pending_jobs.len();

        println!("\n  Phase 1a: Chrome screenshots ({} pending, {} cached)...",
            pending_jobs.len(), skipped);
        let start = Instant::now();

        if !pending_jobs.is_empty() {
            // Launch a single persistent Chrome instance via CDP
            match cdp::ChromeCdp::launch(&chrome_path) {
                Ok(mut chrome) => {
                    for (i, job) in pending_jobs.iter().enumerate() {
                        // For desktop, also extract layout JSON
                        let layout_file = if job.size.name == "desktop" {
                            let lf = screenshots.join(
                                format!("{}_desktop_chrome_layout.json", job.test_name)
                            );
                            if lf.exists() { None } else { Some(lf) }
                        } else {
                            None
                        };

                        if let Err(e) = chrome.screenshot_and_layout(
                            &job.test_file,
                            &job.chrome_file,
                            layout_file.as_deref(),
                            job.size.width,
                            job.size.height,
                        ) {
                            eprintln!("  Warning: Chrome CDP failed for {} at {}: {}",
                                job.test_name, job.size.name, e);
                            chrome_errors += 1;
                        }

                        let done = i + 1 + skipped;
                        if done % 20 == 0 || i + 1 == pending_jobs.len() {
                            println!("    Chrome: {}/{}", done, jobs.len());
                        }
                    }
                    // Chrome is dropped here, closing the browser
                }
                Err(e) => {
                    eprintln!("  Warning: CDP launch failed ({}), falling back to per-process mode", e);
                    // Fallback: spawn one Chrome per screenshot (original behavior)
                    for (i, job) in pending_jobs.iter().enumerate() {
                        if let Err(e) = generate_chrome_screenshot(
                            &chrome_path, &job.test_file, &job.chrome_file,
                            job.size.width, job.size.height,
                        ) {
                            eprintln!("  Warning: Chrome failed for {} at {}: {}",
                                job.test_name, job.size.name, e);
                            chrome_errors += 1;
                        }
                        let done = i + 1 + skipped;
                        if done % 20 == 0 || i + 1 == pending_jobs.len() {
                            println!("    Chrome: {}/{}", done, jobs.len());
                        }
                    }

                    // Extract Chrome layout JSON for desktop tests with timeout fallback
                    let desktop_tests: Vec<&&ScreenshotJob> = pending_jobs.iter()
                        .filter(|j| j.size.name == "desktop")
                        .collect();
                    for job in &desktop_tests {
                        let layout_file = screenshots.join(
                            format!("{}_desktop_chrome_layout.json", job.test_name)
                        );
                        if layout_file.exists() { continue; }
                        if let Err(e) = generate_chrome_layout_with_timeout(
                            &chrome_path, &job.test_file, &layout_file,
                            job.size.width, job.size.height,
                        ) {
                            eprintln!("    Warning: Layout extraction failed for {}: {}",
                                job.test_name, e);
                        }
                    }
                }
            }
        }

        println!("  Phase 1a done in {:.1}s ({} errors)",
            start.elapsed().as_secs_f64(), chrome_errors);
    } else {
        println!("\n  Phase 1a: Chrome screenshots — skipped (--skip-chrome)");
    }

    // ── Phase 1b: Azul CPU rendering (parallel via rayon) ───────────────

    println!("  Phase 1b: Azul rendering ({} jobs)...", jobs.len());

    // Wait for font registry to finish scanning (typically < 100ms if it
    // was started during Phase 1a). Then snapshot the cache for renders.
    let fc_cache_start = Instant::now();
    // Wait for scout+builders to complete (they've been running since before Phase 1a)
    let deadline = Instant::now() + Duration::from_secs(30);
    while !font_registry.build_complete.load(std::sync::atomic::Ordering::Acquire) {
        if Instant::now() > deadline {
            eprintln!("    Warning: font registry build timed out after 30s, proceeding with partial cache");
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // Snapshot the cache — clone once, reuse for all renders
    let fc_cache = font_registry.cache.read()
        .map(|c| c.clone())
        .map_err(|e| format!("Failed to read font cache: {}", e))?;
    // Signal shutdown so background threads exit cleanly
    font_registry.shutdown.store(true, std::sync::atomic::Ordering::Release);
    font_registry.queue_condvar.notify_all();
    println!("    Font cache ready in {:.1}s ({} fonts)",
        fc_cache_start.elapsed().as_secs_f64(),
        fc_cache.list().len());

    let start = Instant::now();
    let azul_done = AtomicUsize::new(0);
    let azul_errors = AtomicUsize::new(0);

    // Collect debug data per test (keyed by test_name, prefer desktop)
    let debug_data_map: Arc<Mutex<std::collections::HashMap<String, DebugData>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    jobs.par_iter().for_each(|job| {
        if job.azul_file.exists() {
            azul_done.fetch_add(1, Ordering::Relaxed);
            return;
        }

        match generate_azul_rendering_at_size_cached(
            &job.test_file,
            &job.azul_file,
            job.size.width,
            job.size.height,
            &fc_cache,
        ) {
            Ok(dd) => {
                let mut map = debug_data_map.lock().unwrap();
                // Prefer desktop debug data, but store any if none yet
                if job.size.name == "desktop" || !map.contains_key(&job.test_name) {
                    map.insert(job.test_name.clone(), dd);
                }
            }
            Err(e) => {
                eprintln!("  Warning: Azul failed for {} at {}: {}", job.test_name, job.size.name, e);
                azul_errors.fetch_add(1, Ordering::Relaxed);
            }
        }

        let done = azul_done.fetch_add(1, Ordering::Relaxed) + 1;
        if done % 20 == 0 || done == jobs.len() {
            println!("    Azul: {}/{}", done, jobs.len());
        }
    });

    let errors = azul_errors.load(Ordering::Relaxed);
    println!("  Phase 1b done in {:.1}s ({} errors)",
        start.elapsed().as_secs_f64(), errors);

    // ── Phase 1c: Pixel diff analysis (parallel via rayon) ──────────────

    println!("  Phase 1c: Pixel diff analysis ({} jobs)...", jobs.len());
    let start = Instant::now();
    let threshold_pct = 0.5; // Fail if >0.5% pixels differ

    // Collect per-job results in parallel
    let diff_results: Vec<Option<(String, PathBuf, SizeResult)>> = jobs.par_iter().map(|job| {
        if !job.chrome_file.exists() || !job.azul_file.exists() {
            return None;
        }
        match analyze_pixel_diff(&job.chrome_file, &job.azul_file, job.size.name) {
            Ok(analysis) if analysis.diff_percent > threshold_pct => {
                Some((job.test_name.clone(), job.test_file.clone(), SizeResult {
                    size: job.size,
                    chrome_screenshot: job.chrome_file.clone(),
                    azul_screenshot: job.azul_file.clone(),
                    diff_analysis: analysis,
                }))
            }
            Ok(_) => None, // Below threshold
            Err(e) => {
                eprintln!("  Warning: Diff failed for {} at {}: {}", job.test_name, job.size.name, e);
                None
            }
        }
    }).collect();

    println!("  Phase 1c done in {:.1}s", start.elapsed().as_secs_f64());

    // ── Assemble results per test ───────────────────────────────────────

    let mut debug_data_map = match Arc::try_unwrap(debug_data_map) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(arc) => arc.lock().unwrap().clone(),
    };

    // Group failing size results by test name
    let mut test_map: std::collections::HashMap<String, (PathBuf, Vec<SizeResult>)> =
        std::collections::HashMap::new();

    for item in diff_results.into_iter().flatten() {
        let (test_name, test_file, size_result) = item;
        test_map.entry(test_name)
            .or_insert_with(|| (test_file, Vec::new()))
            .1.push(size_result);
    }

    let mut failing: Vec<FailingTestData> = test_map.into_iter().map(|(test_name, (test_file, size_results))| {
        // Attach Chrome layout JSON to debug data (only available for desktop)
        let mut debug_data = debug_data_map.remove(&test_name);
        if let Some(ref mut dd) = debug_data {
            let layout_file = screenshots_dir(&config.project_root)
                .join(format!("{}_desktop_chrome_layout.json", test_name));
            if let Ok(data) = fs::read_to_string(&layout_file) {
                dd.chrome_layout = data;
            }
        }
        FailingTestData { test_name, test_file, size_results, debug_data }
    }).collect();

    // Sort by name for deterministic output
    failing.sort_by(|a, b| a.test_name.cmp(&b.test_name));

    println!(
        "\nDiscovered {} failing tests (>{:.1}% pixel diff)",
        failing.len(),
        threshold_pct
    );

    Ok(failing)
}

/// Extract Chrome layout JSON with a timeout to avoid hangs from dump-dom.
fn generate_chrome_layout_with_timeout(
    chrome_path: &str,
    test_file: &Path,
    output_file: &Path,
    width: u32,
    height: u32,
) -> Result<(), String> {
    use std::process::{Command, Stdio};

    let canonical_path = test_file.canonicalize()
        .map_err(|e| format!("canonicalize failed: {}", e))?;

    // Build temp HTML with inline layout extraction script
    let original_content = fs::read_to_string(test_file)
        .map_err(|e| format!("read test file: {}", e))?;

    let simple_script = r#"(function() {
    var result = { timestamp: new Date().toISOString(), viewport: { width: window.innerWidth, height: window.innerHeight }, elements: [] };
    var els = document.querySelectorAll('body, body *');
    for (var i = 0; i < els.length; i++) {
        var el = els[i];
        if (el.tagName === 'SCRIPT' || el.tagName === 'STYLE') continue;
        var rect = el.getBoundingClientRect();
        var cs = window.getComputedStyle(el);
        result.elements.push({
            i: i, tag: el.tagName.toLowerCase(), id: el.id || null, cls: el.className || null,
            bounds: { x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.width), h: Math.round(rect.height) },
            margin: { t: parseFloat(cs.marginTop)||0, r: parseFloat(cs.marginRight)||0, b: parseFloat(cs.marginBottom)||0, l: parseFloat(cs.marginLeft)||0 },
            padding: { t: parseFloat(cs.paddingTop)||0, r: parseFloat(cs.paddingRight)||0, b: parseFloat(cs.paddingBottom)||0, l: parseFloat(cs.paddingLeft)||0 },
            display: cs.display, position: cs.position
        });
    }
    return JSON.stringify(result, null, 2);
})()"#;

    let extraction_html = if original_content.contains("</body>") {
        original_content.replace(
            "</body>",
            &format!(
                r#"<pre id="azul-layout-data" style="display:none"></pre>
<script>document.getElementById('azul-layout-data').textContent = {};</script>
</body>"#,
                simple_script
            ),
        )
    } else {
        original_content.clone()
    };

    let temp_dir = std::env::temp_dir();
    let temp_html = temp_dir.join(format!("chrome_layout_{}.html", std::process::id()));
    fs::write(&temp_html, &extraction_html)
        .map_err(|e| format!("write temp html: {}", e))?;

    // Spawn with timeout
    let mut child = Command::new(chrome_path)
        .arg("--headless")
        .arg("--disable-gpu")
        .arg("--dump-dom")
        .arg(format!("--window-size={},{}", width, height))
        .arg(format!("file://{}", temp_html.display()))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn chrome: {}", e))?;

    let timeout = Duration::from_secs(10);
    let start = Instant::now();
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = fs::remove_file(&temp_html);
                    break Err("timeout after 10s".to_string());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                let _ = fs::remove_file(&temp_html);
                break Err(format!("wait error: {}", e));
            }
        }
    };
    let _ = fs::remove_file(&temp_html);

    let status = exit?;
    if !status.success() {
        return Err("chrome dump-dom failed".to_string());
    }

    // Parse stdout for layout data
    let stdout = child.wait_with_output()
        .map_err(|e| format!("read output: {}", e))?;
    let dom_output = String::from_utf8_lossy(&stdout.stdout);

    if let Some(start) = dom_output.find("<pre id=\"azul-layout-data\"") {
        if let Some(content_start) = dom_output[start..].find('>') {
            let after_tag = &dom_output[start + content_start + 1..];
            if let Some(end) = after_tag.find("</pre>") {
                let json_content = &after_tag[..end];
                let decoded = json_content
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&")
                    .replace("&quot;", "\"");
                if decoded.starts_with('{') {
                    fs::write(output_file, &decoded)
                        .map_err(|e| format!("write layout json: {}", e))?;
                    return Ok(());
                }
            }
        }
    }

    // No layout data found — not an error, just skip
    Ok(())
}

/// Generate Azul rendering at a specific size.
fn generate_azul_rendering_at_size(
    test_file: &Path,
    output_file: &Path,
    width: u32,
    height: u32,
) -> Result<DebugData, String> {
    generate_azul_rendering_sized(test_file, output_file, width, height, 1.0)
        .map_err(|e| format!("{}", e))
}

fn generate_azul_rendering_at_size_cached(
    test_file: &Path,
    output_file: &Path,
    width: u32,
    height: u32,
    fc_cache: &rust_fontconfig::FcFontCache,
) -> Result<DebugData, String> {
    generate_azul_rendering_sized_cached(test_file, output_file, width, height, 1.0, fc_cache)
        .map_err(|e| format!("{}", e))
}

// ── Prompt Builder ─────────────────────────────────────────────────────

/// Build the autodebug prompt for a failing test.
pub fn build_autodebug_prompt(test: &FailingTestData) -> String {
    let mut prompt = String::with_capacity(16384);

    // Codebase orientation
    prompt.push_str(CODEBASE_CONTEXT);
    prompt.push('\n');

    // Working directory
    writeln!(
        prompt,
        "## Working Directory\n\n\
         You are in a git worktree. ALL file paths are relative to your current working directory.\n\
         Do NOT `cd` anywhere else — your commits will be lost if you do.\n",
    ).unwrap();

    // Test information
    writeln!(prompt, "## Test Under Analysis\n").unwrap();
    writeln!(prompt, "**Test name:** `{}`", test.test_name).unwrap();
    writeln!(prompt, "**Test file:** `{}`\n", test.test_file.display()).unwrap();

    // XHTML source
    if let Some(ref dd) = test.debug_data {
        if !dd.xhtml_source.is_empty() {
            writeln!(prompt, "### XHTML Source\n```xml\n{}\n```\n", dd.xhtml_source).unwrap();
        }

        // CSS warnings
        if !dd.css_warnings.is_empty() {
            writeln!(prompt, "### CSS Warnings\n```\n{}\n```\n", dd.css_warnings).unwrap();
        }

        // Layout debug trace
        if !dd.render_warnings.is_empty() {
            writeln!(prompt, "### Layout Debug Trace\n```").unwrap();
            for (i, w) in dd.render_warnings.iter().enumerate() {
                writeln!(prompt, "{}. {}", i + 1, w).unwrap();
            }
            writeln!(prompt, "```\n").unwrap();
        }
    }

    // Screenshots — referenced as file paths for Claude's Read tool
    writeln!(prompt, "## Screenshots\n").unwrap();
    writeln!(
        prompt,
        "Use the `Read` tool to view these image files. Claude Code can view images natively.\n"
    ).unwrap();

    for sr in &test.size_results {
        writeln!(
            prompt,
            "### {} ({}x{})\n",
            sr.size.name, sr.size.width, sr.size.height
        ).unwrap();
        writeln!(prompt, "- Chrome (reference): `{}`", sr.chrome_screenshot.display()).unwrap();
        writeln!(prompt, "- Azul (current): `{}`\n", sr.azul_screenshot.display()).unwrap();
    }

    // Pixel diff analysis for each resolution
    writeln!(prompt, "## Pixel Diff Analysis\n").unwrap();
    for sr in &test.size_results {
        prompt.push_str(&sr.diff_analysis.summary);
        prompt.push('\n');
    }

    // Chrome layout vs Azul display list
    if let Some(ref dd) = test.debug_data {
        if !dd.chrome_layout.is_empty() && dd.chrome_layout != "{}" {
            writeln!(prompt, "## Chrome Layout (reference)\n```json\n{}\n```\n", dd.chrome_layout).unwrap();
        }
        if !dd.display_list.is_empty() {
            writeln!(prompt, "## Azul Display List\n```\n{}\n```\n", dd.display_list).unwrap();
        }
        if !dd.solved_layout.is_empty() {
            writeln!(prompt, "## Azul Solved Layout\n```\n{}\n```\n", dd.solved_layout).unwrap();
        }
    }

    // Instructions for the agent
    writeln!(prompt, "## Your Task\n").unwrap();
    writeln!(prompt, "You are debugging a CSS layout rendering bug in the Azul layout engine.").unwrap();
    writeln!(prompt, "The screenshots above show how Chrome renders this test (reference) vs how Azul renders it.").unwrap();
    writeln!(prompt, "The pixel diff analysis highlights which regions differ and why.\n").unwrap();
    writeln!(prompt, "### Instructions\n").unwrap();
    writeln!(prompt, "1. **Analyze the bug**: Compare Chrome vs Azul screenshots using the Read tool.").unwrap();
    writeln!(prompt, "   Study the pixel diff analysis to understand what's wrong.").unwrap();
    writeln!(prompt, "2. **Find the root cause**: The layout solver is in `layout/src/solver3/`.").unwrap();
    writeln!(prompt, "   Key files: `mod.rs` (entry point), `fc.rs` (formatting contexts),").unwrap();
    writeln!(prompt, "   `sizing.rs` (width/height), `positioning.rs` (positioning).").unwrap();
    writeln!(prompt, "   Text layout is in `layout/src/text3/`.").unwrap();
    writeln!(prompt, "   CPU rendering is in `layout/src/cpurender.rs`.").unwrap();
    writeln!(prompt, "3. **Fix the bug**: Make minimal, targeted changes. Do NOT refactor").unwrap();
    writeln!(prompt, "   surrounding code or add features beyond what's needed.").unwrap();
    writeln!(prompt, "4. **Commit your fix**: Create one commit with a clear message describing").unwrap();
    writeln!(prompt, "   what was wrong and how you fixed it.\n").unwrap();
    writeln!(prompt, "### Important Rules\n").unwrap();
    writeln!(prompt, "- Keep changes minimal — fix only this bug, nothing else.").unwrap();
    writeln!(prompt, "- Do NOT modify taffy_bridge.rs (Flex/Grid is handled by Taffy).").unwrap();
    writeln!(prompt, "- Do NOT run `cargo build` or any compilation commands.").unwrap();
    writeln!(prompt, "- Your changes must compile — check types carefully.").unwrap();
    writeln!(prompt, "- If you cannot determine the fix with confidence, write a").unwrap();
    writeln!(prompt, "  detailed analysis in a file `doc/target/autodebug/reports/{}_report.md`", test.test_name).unwrap();
    writeln!(prompt, "  explaining what you found, then commit that file instead.").unwrap();

    prompt
}

// ── Main Entry Point ───────────────────────────────────────────────────

/// Parse CLI arguments for the autodebug command.
pub fn parse_autodebug_args(args: &[&str], project_root: &Path) -> Result<AutodebugConfig, String> {
    let manifest_dir = project_root.join("doc");
    let test_dir = manifest_dir.join("working");

    let mut config = AutodebugConfig {
        project_root: project_root.to_path_buf(),
        test_dir,
        agents: 4,
        timeout: Duration::from_secs(600),
        model: None,
        sizes: ALL_SIZES.to_vec(),
        test_filter: None,
        skip_chrome: false,
        retry_failed: false,
        dry_run: false,
        status_only: false,
        collect_only: false,
        cleanup: false,
    };

    for arg in args {
        if let Some(n) = arg.strip_prefix("--agents=") {
            config.agents = n.parse().map_err(|_| format!("Invalid --agents: {}", arg))?;
        } else if let Some(s) = arg.strip_prefix("--timeout=") {
            let secs: u64 = s.parse().map_err(|_| format!("Invalid --timeout: {}", arg))?;
            config.timeout = Duration::from_secs(secs);
        } else if let Some(m) = arg.strip_prefix("--model=") {
            config.model = Some(m.to_string());
        } else if let Some(sizes_str) = arg.strip_prefix("--sizes=") {
            let mut sizes = Vec::new();
            for s in sizes_str.split(',') {
                match s.trim() {
                    "mobile" => sizes.push(SIZE_MOBILE),
                    "tablet" => sizes.push(SIZE_TABLET),
                    "desktop" => sizes.push(SIZE_DESKTOP),
                    other => return Err(format!("Unknown size: {}", other)),
                }
            }
            config.sizes = sizes;
        } else if let Some(name) = arg.strip_prefix("--test=") {
            config.test_filter = Some(name.to_string());
        } else if *arg == "--skip-chrome" {
            config.skip_chrome = true;
        } else if *arg == "--retry-failed" {
            config.retry_failed = true;
        } else if *arg == "--dry-run" {
            config.dry_run = true;
        } else if *arg == "--status" {
            config.status_only = true;
        } else if *arg == "--collect" {
            config.collect_only = true;
        } else if *arg == "--cleanup" {
            config.cleanup = true;
        } else if !arg.starts_with('-') {
            // Last positional arg that is a directory → test dir override
            let candidate = PathBuf::from(arg);
            if candidate.is_dir() {
                config.test_dir = candidate;
            } else {
                // Try relative to project root
                let resolved = project_root.join(arg);
                if resolved.is_dir() {
                    config.test_dir = resolved;
                }
                // Otherwise ignore (e.g. "claude-exec")
            }
        } else {
            return Err(format!("Unknown option: {}", arg));
        }
    }

    // Ensure test_dir is absolute
    if config.test_dir.is_relative() {
        config.test_dir = project_root.join(&config.test_dir);
    }

    Ok(config)
}

/// Main entry point for the autodebug pipeline.
pub fn run_autodebug(config: AutodebugConfig) -> Result<(), String> {
    let project_root = config.project_root.clone();

    // Handle cleanup
    if config.cleanup {
        println!("Cleaning up autodebug worktrees...");
        executor::cleanup_worktrees_autodebug(&project_root)?;
        return Ok(());
    }

    // Handle status
    if config.status_only {
        return show_status(&project_root, config.retry_failed);
    }

    // Handle collect
    if config.collect_only {
        return collect_autodebug_patches(&project_root);
    }

    // Preflight checks (skip agent-specific checks for dry-run)
    preflight_checks(&project_root, config.dry_run)?;

    // Ensure output directories exist
    for dir in &[
        output_dir(&project_root),
        screenshots_dir(&project_root),
        prompts_dir(&project_root),
        patches_dir(&project_root),
        reports_dir(&project_root),
    ] {
        fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    }

    // Phase 1: Discover failing tests
    println!("\n=== Phase 1: Discovering failing tests ===\n");
    let failing_tests = discover_failing_tests(&config)?;

    if failing_tests.is_empty() {
        println!("No failing tests found. All tests pass the pixel diff threshold.");
        return Ok(());
    }

    // Phase 2: Generate prompts
    println!("\n=== Phase 2: Generating prompts ===\n");
    let prompts = prompts_dir(&project_root);
    let mut prompt_count = 0;

    for test in &failing_tests {
        let prompt_path = prompts.join(format!("{}.md", test.test_name));

        // Skip if already has a result (unless --retry-failed)
        let status = executor::classify_prompt(&prompt_path, config.retry_failed);
        match status {
            executor::PromptStatus::Done => {
                continue;
            }
            executor::PromptStatus::Taken { .. } => {
                continue;
            }
            _ => {}
        }

        let prompt_text = build_autodebug_prompt(test);
        fs::write(&prompt_path, &prompt_text)
            .map_err(|e| format!("Failed to write prompt: {}", e))?;
        prompt_count += 1;
    }

    println!("Generated {} prompts in {}", prompt_count, prompts.display());

    if config.dry_run {
        println!("\n--dry-run: stopping after prompt generation.");
        println!("Prompts are in: {}", prompts.display());
        return Ok(());
    }

    // Phase 3: Dispatch agents
    println!("\n=== Phase 3: Dispatching agents ===\n");
    dispatch_agents(&config)?;

    // Phase 4: Summary
    println!("\n=== Phase 4: Summary ===\n");
    show_status(&project_root, false)?;

    Ok(())
}

/// Preflight checks before running the pipeline.
/// `dry_run` skips agent-specific checks (CLAUDECODE env, API key, claude CLI).
fn preflight_checks(workspace_root: &Path, dry_run: bool) -> Result<(), String> {
    println!("Preflight checks");
    println!("================\n");

    if !dry_run {
        // Refuse to run inside an existing Claude Code session
        if std::env::var("CLAUDECODE").is_ok() {
            return Err(
                "Cannot run inside a Claude Code session.\n\
                 The executor spawns claude CLI subprocesses which would conflict.\n\
                 Run this command from a regular terminal:\n\
                 \n\
                 ./target/release/azul-doc autodebug claude-exec"
                    .to_string(),
            );
        }
        println!("  [OK] Not running inside Claude Code");

        // Check that ANTHROPIC_API_KEY is NOT set
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            return Err(
                "ANTHROPIC_API_KEY is set in environment.\n\
                 This would route claude CLI through the paid API.\n\
                 Unset it first: unset ANTHROPIC_API_KEY"
                    .to_string(),
            );
        }
        println!("  [OK] No ANTHROPIC_API_KEY set (using subscription plan)");

        // Check claude CLI is available
        let claude_check = std::process::Command::new("claude")
            .arg("--version")
            .output();
        match claude_check {
            Ok(o) if o.status.success() => {
                let version = String::from_utf8_lossy(&o.stdout);
                println!("  [OK] claude CLI: {}", version.trim());
            }
            _ => {
                return Err("claude CLI not found. Install it first.".to_string());
            }
        }
    } else {
        println!("  [SKIP] Agent checks skipped (--dry-run)");
    }

    // Verify working directory
    let solver_dir = workspace_root.join("layout/src/solver3");
    if !solver_dir.is_dir() {
        return Err(format!(
            "Working directory does not look like the azul repo.\n\
             Expected layout/src/solver3/ in: {}",
            workspace_root.display()
        ));
    }
    println!("  [OK] Working directory: {}", workspace_root.display());

    println!();
    Ok(())
}

/// Dispatch agents to work on pending prompts.
fn dispatch_agents(config: &AutodebugConfig) -> Result<(), String> {
    let project_root = &config.project_root;
    let prompts = prompts_dir(project_root);

    // Scan for pending prompts
    let (pending, done, failed, taken) =
        executor::scan_prompts_dir(&prompts, config.retry_failed);

    println!(
        "Prompt status: {} pending, {} done, {} failed, {} in-progress",
        pending.len(), done, failed, taken
    );

    if pending.is_empty() {
        println!("No pending prompts to process.");
        return Ok(());
    }

    // Create worktree pool
    let agent_count = config.agents.min(pending.len());
    println!("\nCreating {} worktrees...", agent_count);
    let slots = executor::create_worktree_pool_autodebug(project_root, agent_count)?;

    let base_sha = executor::get_head_sha(project_root)?;
    println!("Base SHA: {}", &base_sha[..12]);

    // Install signal handler
    executor::install_sigint_handler();

    // Set up work queue
    let work_queue: Arc<Mutex<VecDeque<PathBuf>>> =
        Arc::new(Mutex::new(pending.into_iter().collect()));

    // Set up spinner display
    let spinner = nanospinner::MultiSpinner::new().start();
    let slot_spinners: Vec<_> = (0..agent_count)
        .map(|i| spinner.add(format!("[SLOT {:03}] idle", i)))
        .collect();

    // Worker threads
    let results: Arc<Mutex<Vec<(String, AgentResult)>>> = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::with_capacity(agent_count);

    for (slot_idx, (slot, line)) in slots.into_iter().zip(slot_spinners).enumerate() {
        let work_queue = Arc::clone(&work_queue);
        let results = Arc::clone(&results);
        let base_sha = base_sha.clone();
        let timeout = config.timeout;
        let model = config.model.clone();

        let handle = std::thread::spawn(move || {
            loop {
                if SHUTDOWN_REQUESTED.load(Ordering::Relaxed) {
                    break;
                }

                let prompt_path = {
                    let mut queue = work_queue.lock().unwrap();
                    queue.pop_front()
                };

                let prompt_path = match prompt_path {
                    Some(p) => p,
                    None => break, // No more work
                };

                let test_name = prompt_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                line.update(format!("[SLOT {:03}] {}", slot_idx, test_name));

                let model_ref = model.as_deref();
                let result = executor::run_agent_in_slot_autodebug(
                    &slot,
                    slot_idx,
                    &prompt_path,
                    timeout,
                    &base_sha,
                    model_ref,
                    &|status| {
                        line.update(format!(
                            "[SLOT {:03}] {} | {}",
                            slot_idx, test_name, status
                        ));
                    },
                );

                let status_msg = if result.success {
                    format!("{}: {} patches", test_name, result.patches)
                } else {
                    format!(
                        "{}: FAILED ({})",
                        test_name,
                        result.error.as_deref().unwrap_or("unknown")
                    )
                };
                line.update(format!("[SLOT {:03}] done — {}", slot_idx, status_msg));

                results.lock().unwrap().push((test_name, result));
            }

            line.update(format!("[SLOT {:03}] finished", slot_idx));
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        let _ = handle.join();
    }

    // Print results summary
    let results = results.lock().unwrap();
    let total = results.len();
    let success = results.iter().filter(|(_, r)| r.success).count();
    let total_patches: usize = results.iter().map(|(_, r)| r.patches).sum();

    println!("\n\nAgent execution complete:");
    println!("  Total: {}", total);
    println!("  Success: {}", success);
    println!("  Failed: {}", total - success);
    println!("  Total patches: {}", total_patches);

    Ok(())
}

/// Show status of the autodebug pipeline.
fn show_status(project_root: &Path, retry_failed: bool) -> Result<(), String> {
    let prompts = prompts_dir(project_root);
    if !prompts.exists() {
        println!("No autodebug prompts directory found. Run autodebug claude-exec first.");
        return Ok(());
    }

    let (pending, done, failed, taken) =
        executor::scan_prompts_dir(&prompts, retry_failed);

    let total = pending.len() + done + failed + taken;
    println!("Autodebug Status");
    println!("================\n");
    println!("  Total prompts:  {}", total);
    println!("  Done:           {} ({:.0}%)", done, if total > 0 { done as f64 / total as f64 * 100.0 } else { 0.0 });
    println!("  Failed:         {}", failed);
    println!("  In-progress:    {}", taken);
    println!("  Pending:        {}", pending.len());

    // Show details for in-progress and failed
    if taken > 0 || failed > 0 {
        println!();
        let entries: Vec<_> = fs::read_dir(&prompts)
            .map_err(|e| format!("Failed to read prompts dir: {}", e))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();

        for entry in &entries {
            let ext = entry.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "taken" {
                let content = fs::read_to_string(entry).unwrap_or_default();
                let name = entry.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                println!("  IN-PROGRESS: {} ({})", name, content.trim());
            } else if ext == "failed" {
                let name = entry.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                // Read first line of failure
                let content = fs::read_to_string(entry).unwrap_or_default();
                let first_line = content.lines().next().unwrap_or("unknown error");
                println!("  FAILED: {} — {}", name, first_line);
            }
        }
    }

    Ok(())
}

/// Collect patches from completed agent runs.
fn collect_autodebug_patches(project_root: &Path) -> Result<(), String> {
    let prompts = prompts_dir(project_root);
    let patches = patches_dir(project_root);
    fs::create_dir_all(&patches)
        .map_err(|e| format!("Failed to create patches dir: {}", e))?;

    if !prompts.exists() {
        return Err("No prompts directory found. Run autodebug claude-exec first.".to_string());
    }

    let mut collected = 0;
    let entries: Vec<_> = fs::read_dir(&prompts)
        .map_err(|e| format!("Failed to read prompts dir: {}", e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.to_string_lossy().contains(".md.done.")
                && p.extension().map(|e| e == "patch").unwrap_or(false)
        })
        .collect();

    for patch_file in &entries {
        let dest_name = patch_file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let dest = patches.join(&dest_name);
        if !dest.exists() {
            fs::copy(patch_file, &dest)
                .map_err(|e| format!("Failed to copy patch: {}", e))?;
            collected += 1;
        }
    }

    println!("Collected {} patches to {}", collected, patches.display());
    Ok(())
}

// ── Summary JSON ───────────────────────────────────────────────────────

/// Write a summary JSON file with overall results.
fn write_summary_json(
    project_root: &Path,
    results: &[(String, AgentResult)],
) -> Result<(), String> {
    let summary_path = output_dir(project_root).join("summary.json");

    let entries: Vec<serde_json::Value> = results
        .iter()
        .map(|(name, result)| {
            serde_json::json!({
                "test": name,
                "success": result.success,
                "patches": result.patches,
                "error": result.error,
            })
        })
        .collect();

    let summary = serde_json::json!({
        "total": results.len(),
        "success": results.iter().filter(|(_, r)| r.success).count(),
        "failed": results.iter().filter(|(_, r)| !r.success).count(),
        "total_patches": results.iter().map(|(_, r)| r.patches).sum::<usize>(),
        "results": entries,
    });

    let json = serde_json::to_string_pretty(&summary)
        .map_err(|e| format!("Failed to serialize summary: {}", e))?;
    fs::write(&summary_path, json)
        .map_err(|e| format!("Failed to write summary: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_describe_position() {
        // Center of a 1920x1080 image
        assert_eq!(describe_position(800, 400, 100, 100, 1920, 1080), "middle-center");

        // Top-left
        assert_eq!(describe_position(10, 10, 50, 50, 1920, 1080), "top-left");

        // Full-width band
        assert_eq!(describe_position(0, 500, 1600, 50, 1920, 1080), "middle band at y=500");

        // Bottom-right
        assert_eq!(describe_position(1500, 900, 100, 100, 1920, 1080), "bottom-right");
    }

    #[test]
    fn test_classify_cause() {
        // Thin strip
        assert!(classify_cause(4, 200, 10, 10, 1920, 1080, 0.5, 0.8, 100).contains("border"));

        // Edge-adjacent
        assert!(classify_cause(100, 100, 0, 0, 1920, 1080, 0.3, 0.5, 500).contains("margin"));

        // High delta
        assert!(classify_cause(200, 200, 100, 100, 1920, 1080, 0.7, 1.0, 5000).contains("missing"));

        // Small scattered
        assert!(classify_cause(50, 50, 500, 500, 1920, 1080, 0.1, 0.2, 50).contains("anti-aliasing"));

        // Large block
        assert!(classify_cause(200, 200, 100, 100, 1920, 1080, 0.3, 0.5, 5000).contains("layout shift"));
    }

    #[test]
    fn test_pixel_delta() {
        let white = image::Rgba([255, 255, 255, 255]);
        let black = image::Rgba([0, 0, 0, 255]);
        let delta = pixel_delta(&white, &black);
        assert!(delta > 0.8, "White vs black delta should be high: {}", delta);

        let same = pixel_delta(&white, &white);
        assert!(same < 0.01, "Same pixel delta should be ~0: {}", same);
    }

    #[test]
    fn test_flood_fill_single_block() {
        let grid = BlockGrid {
            cols: 3,
            rows: 3,
            cells: vec![
                false, true, false,
                false, true, false,
                false, false, false,
            ],
        };
        let mut labels = vec![0u32; 9];
        flood_fill(&grid, &mut labels, 1, 0, 1);
        assert_eq!(labels[1], 1); // (1,0)
        assert_eq!(labels[4], 1); // (1,1)
        assert_eq!(labels[0], 0); // (0,0) not connected
    }
}
