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
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::spec::executor::{
    self, AgentResult, WorktreeSlot, CODEBASE_CONTEXT, SHUTDOWN_REQUESTED,
};

use super::{
    generate_azul_rendering_sized, generate_chrome_screenshot_with_debug,
    get_chrome_path, find_test_files, pixels_similar, DebugData,
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

/// Discover tests that fail the pixel diff threshold.
pub fn discover_failing_tests(config: &AutodebugConfig) -> Result<Vec<FailingTestData>, String> {
    let test_dir = &config.test_dir;
    let screenshots = screenshots_dir(&config.project_root);
    fs::create_dir_all(&screenshots)
        .map_err(|e| format!("Failed to create screenshots dir: {}", e))?;

    let chrome_path = get_chrome_path();

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

    println!("Found {} test files", test_files.len());

    let mut failing = Vec::new();
    let threshold_pct = 0.5; // Fail if >0.5% pixels differ

    for (idx, test_file) in test_files.iter().enumerate() {
        let test_name = test_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        if (idx + 1) % 50 == 0 || idx == 0 {
            println!(
                "  [{}/{}] Processing {}...",
                idx + 1,
                test_files.len(),
                test_name
            );
        }

        let mut size_results = Vec::new();
        let mut any_failing = false;
        let mut debug_data = None;

        for size in &config.sizes {
            let chrome_file = screenshots.join(format!("{}_{}_chrome.png", test_name, size.name));
            let azul_file = screenshots.join(format!("{}_{}_azul.webp", test_name, size.name));

            // Generate Chrome screenshot (skip if cached and --skip-chrome)
            if !config.skip_chrome || !chrome_file.exists() {
                let layout_file = screenshots.join(format!("{}_{}_chrome_layout.json", test_name, size.name));
                if let Err(e) = generate_chrome_screenshot_with_debug(
                    &chrome_path,
                    test_file,
                    &chrome_file,
                    &layout_file,
                    size.width,
                    size.height,
                ) {
                    eprintln!("  Warning: Chrome screenshot failed for {} at {}: {}", test_name, size.name, e);
                    continue;
                }
            }

            // Generate Azul rendering — use desktop size for debug data
            match generate_azul_rendering_at_size(test_file, &azul_file, size.width, size.height) {
                Ok(dd) => {
                    if size.name == "desktop" || debug_data.is_none() {
                        debug_data = Some(dd);
                    }
                }
                Err(e) => {
                    eprintln!("  Warning: Azul rendering failed for {} at {}: {}", test_name, size.name, e);
                    continue;
                }
            }

            // Analyze diff
            if !chrome_file.exists() || !azul_file.exists() {
                continue;
            }

            match analyze_pixel_diff(&chrome_file, &azul_file, size.name) {
                Ok(analysis) => {
                    if analysis.diff_percent > threshold_pct {
                        any_failing = true;
                        size_results.push(SizeResult {
                            size: *size,
                            chrome_screenshot: chrome_file,
                            azul_screenshot: azul_file,
                            diff_analysis: analysis,
                        });
                    }
                }
                Err(e) => {
                    eprintln!("  Warning: Diff analysis failed for {} at {}: {}", test_name, size.name, e);
                }
            }
        }

        if any_failing && !size_results.is_empty() {
            failing.push(FailingTestData {
                test_name,
                test_file: test_file.clone(),
                size_results,
                debug_data,
            });
        }
    }

    println!(
        "\nDiscovered {} failing tests (>{:.1}% pixel diff)",
        failing.len(),
        threshold_pct
    );

    Ok(failing)
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
            // Positional args ignored (e.g. "claude-exec")
        } else {
            return Err(format!("Unknown option: {}", arg));
        }
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

    // Preflight checks
    preflight_checks(&project_root)?;

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
fn preflight_checks(workspace_root: &Path) -> Result<(), String> {
    println!("Preflight checks");
    println!("================\n");

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
