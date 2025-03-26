use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Instant,
};

use azul_core::{
    app_resources::{DpiScaleFactor, Epoch, IdNamespace, ImageCache, RendererResources},
    callbacks::DocumentId,
    display_list::{
        CachedDisplayList, DisplayListFrame, DisplayListMsg, DisplayListScrollFrame,
        LayoutRectContent, RectBackground, RenderCallbacks, SolvedLayout, StyleBorderColors,
        StyleBorderStyles, StyleBorderWidths,
    },
    styled_dom::{DomId, StyledDom},
    ui_solver::LayoutResult,
    window::{FullWindowState, LogicalSize, StringPairVec},
    xml::{get_html_node, DomXml, XmlComponentMap, XmlNode},
};
use azul_css::{
    parser::{CssApiWrapper, CssParseWarnMsgOwned},
    BorderStyle, ColorU, Css, CssDeclaration, CssProperty, FloatValue,
};
use azul_layout::{
    text2::shaping::{GlyphOutlineOperation, ParsedFont},
    xml::{domxml_from_str, parse_xml_string},
};
use image::{ImageBuffer, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TestResult {
    test_name: String,
    diff_count: usize,
    passed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestResults {
    tests: Vec<TestResult>,
    total_tests: usize,
    passed_tests: usize,
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/working");
    let test_dir = PathBuf::from(path);
    let output_dir = PathBuf::from("reftest_output");

    // Create output directory if it doesn't exist
    fs::create_dir_all(&output_dir)?;

    println!("Looking for test files in {}", test_dir.display());

    // Find all XHTML files in the test directory
    let test_files = find_test_files(&test_dir)?;
    println!("Found {} test files", test_files.len());

    // Results to be collected for JSON
    let results = Arc::new(Mutex::new(Vec::new()));
    let enhanced_results = Arc::new(Mutex::new(Vec::new()));

    // Get Chrome path
    let chrome_path = get_chrome_path();

    // Get Chrome version
    let chrome_version = get_chrome_version(&chrome_path);
    let is_chrome_installed = !chrome_version.contains("Unknown");

    // Current time
    let current_time = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Git hash
    let git_hash = get_git_hash();

    if !is_chrome_installed {
        eprintln!("ERROR: Chrome not found. Tests will not be run.");
        eprintln!(
            "Please ensure Chrome is installed or set the CHROME environment variable to the \
             correct path."
        );

        // Generate empty report with just header information
        generate_enhanced_html_report(
            &output_dir,
            &Vec::new(),
            &chrome_version,
            &current_time,
            &git_hash,
            is_chrome_installed,
        )?;

        // Open the report unless we're in CI
        if env::var("GITHUB_CI").is_err() {
            let report_path = output_dir.join("report.html").to_string_lossy().to_string();
            let _ = open::that(&report_path);
        }

        return Ok(());
    }

    // Process tests
    test_files.iter().for_each(|test_file| {
        let test_name = test_file.file_stem().unwrap().to_string_lossy().to_string();
        println!("Processing test: {}", test_name);

        let chrome_png = output_dir.join(format!("{}_chrome.png", test_name));
        let azul_png = output_dir.join(format!("{}_azul.png", test_name));
        let diff_png = output_dir.join(format!("{}_diff.png", test_name));

        // Generate Chrome reference if it doesn't exist
        if !chrome_png.exists() {
            println!("  Generating Chrome reference for {}", test_name);
            match generate_chrome_screenshot(&chrome_path, test_file, &chrome_png, WIDTH, HEIGHT) {
                Ok(_) => println!("  Chrome screenshot generated successfully"),
                Err(e) => {
                    println!("  Failed to generate Chrome screenshot: {}", e);
                    return;
                }
            }
        } else {
            println!("  Using existing Chrome reference for {}", test_name);
        }

        let (chrome_w, chrome_h) = match image::image_dimensions(&chrome_png) {
            Ok(o) => o,
            Err(e) => {
                println!("  Failed to generate Chrome screenshot: {}", e);
                return;
            }
        };

        let dpi_factor = (chrome_w as f32 / WIDTH as f32).max((chrome_h as f32 / HEIGHT as f32));

        // Generate Azul rendering
        let mut debug_data = None;
        match generate_azul_rendering(test_file, &azul_png, dpi_factor) {
            Ok(data) => {
                println!("  Azul rendering generated successfully");
                debug_data = Some(data);
            }
            Err(e) => {
                println!("  Failed to generate Azul rendering: {}", e);
                return;
            }
        }

        // Compare images and generate diff
        match compare_images(&chrome_png, &azul_png, &diff_png) {
            Ok(diff_count) => {
                let passed = diff_count < 1000; // Threshold for passing
                println!(
                    "  Comparison complete: {} differing pixels, test {}",
                    diff_count,
                    if passed { "PASSED" } else { "FAILED" }
                );

                // Store basic result
                let mut results_vec = results.lock().unwrap();
                results_vec.push(TestResult {
                    test_name: test_name.to_string(),
                    diff_count,
                    passed,
                });

                // Store enhanced result with debug data
                if let Some(data) = debug_data {
                    let mut enhanced_results_vec = enhanced_results.lock().unwrap();
                    enhanced_results_vec.push(EnhancedTestResult::from_debug_data(
                        test_name.to_string(),
                        diff_count,
                        passed,
                        data,
                    ));
                }
            }
            Err(e) => {
                println!("  Failed to compare images: {}", e);
            }
        }
    });

    // Get the final results
    let final_results = results.lock().unwrap();
    let passed_tests = final_results.iter().filter(|r| r.passed).count();
    let total_tests = final_results.len();

    // Get enhanced results
    let final_enhanced_results = enhanced_results.lock().unwrap();

    // Generate enhanced HTML report with header information
    println!("Generating HTML report");
    generate_enhanced_html_report(
        &output_dir,
        &final_enhanced_results,
        &chrome_version,
        &current_time,
        &git_hash,
        is_chrome_installed,
    )?;

    // Generate JSON results
    println!("Generating JSON results");
    generate_json_results(&output_dir, &final_results, passed_tests)?;

    println!(
        "Testing complete. Results saved to {}",
        output_dir.display()
    );
    println!("Passed: {}/{}", passed_tests, final_results.len());

    // Open the report unless we're in CI
    if env::var("GITHUB_CI").is_err() {
        let report_path = output_dir.join("report.html").to_string_lossy().to_string();
        let _ = open::that(&report_path);
    }

    Ok(())
}

fn find_test_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut test_files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "xht") {
            test_files.push(path);
        }
    }

    Ok(test_files)
}

fn get_chrome_path() -> String {
    // Check for environment variable override first
    if let Ok(chrome_path) = env::var("CHROME") {
        if !chrome_path.is_empty() {
            return chrome_path;
        }
    }

    // Check platform-specific default locations
    #[cfg(target_os = "macos")]
    {
        let default_path = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        if Path::new(default_path).exists() {
            return default_path.to_string();
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Common Linux Chrome paths
        for path in &[
            "/usr/bin/google-chrome",
            "/usr/bin/chromium-browser",
            "/usr/bin/chromium",
        ] {
            if Path::new(path).exists() {
                return path.to_string();
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Check Program Files locations
        let program_files =
            env::var("PROGRAMFILES").unwrap_or_else(|_| "C:\\Program Files".to_string());
        let x86_program_files =
            env::var("PROGRAMFILES(X86)").unwrap_or_else(|_| "C:\\Program Files (x86)".to_string());

        let chrome_paths = [
            format!("{}\\Google\\Chrome\\Application\\chrome.exe", program_files),
            format!(
                "{}\\Google\\Chrome\\Application\\chrome.exe",
                x86_program_files
            ),
        ];

        for path in &chrome_paths {
            if Path::new(path).exists() {
                return path.to_string();
            }
        }
    }

    // Default to just "chrome" and let the system resolve it
    "chrome".to_string()
}

fn get_chrome_version(chrome_path: &str) -> String {
    match Command::new(chrome_path).arg("--version").output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "Unknown".to_string(),
    }
}

fn get_git_hash() -> String {
    // Try using git command first
    let git_result = Command::new("git").args(["rev-parse", "HEAD"]).output();
    if let Ok(output) = git_result {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string()
                .chars()
                .take(8)
                .collect();
        }
    }

    // Fall back to reading .git/HEAD
    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        if head.starts_with("ref: ") {
            let ref_path = head.trim_start_matches("ref: ").trim();
            if let Ok(hash) = std::fs::read_to_string(format!(".git/{}", ref_path)) {
                return hash.trim().to_string().chars().take(8).collect();
            }
        }
    }

    "Unknown".to_string()
}

fn generate_chrome_screenshot(
    chrome_path: &str,
    test_file: &Path,
    output_file: &Path,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let canonical_path = test_file.canonicalize()?;

    let status = Command::new(chrome_path)
        .arg("--headless")
        .arg(format!("--screenshot={}", output_file.display()))
        .arg(format!("--window-size={},{}", width, height))
        .arg(format!("file://{}", canonical_path.display()))
        .status()?;

    if !status.success() {
        return Err(format!("Chrome exited with status {}", status).into());
    }

    Ok(())
}

fn generate_azul_rendering(
    test_file: &Path,
    output_file: &Path,
    dpi_factor: f32,
) -> Result<DebugData, Box<dyn Error>> {
    let start = Instant::now();

    // Read XML content
    let xml_content = fs::read_to_string(test_file)?;

    // Initialize debug data collector
    let mut debug_collector = DebugDataCollector::new();

    // Parse XML to DomXml
    let (dom_xml, metadata, xml) =
        EnhancedXmlParser::parse_test_file(test_file).map_err(|e| Box::<dyn Error>::from(e))?;

    // Extract styling and metadata
    debug_collector.set_metadata(
        metadata.title,
        metadata.assert_content,
        metadata.help_link,
        metadata.flags,
        metadata.author,
    );

    // Format XML structure for debugging
    let mut xml_formatted = String::new();
    for node in xml {
        xml_formatted.push_str(&EnhancedXmlParser::format_xml_for_display(&node, 0));
    }

    // Extract and analyze CSS
    let mut css_collector = CssWarningCollector::new();
    if let Ok(css_text) = extract_css_from_xml(&xml_content) {
        let parsed_css = css_collector.parse_css(&css_text);
        let css_stats = CssStats::analyze(&parsed_css);
        debug_collector.set_css_debug_info(css_collector.format_warnings(), css_stats.format());
    }

    // Store DOM information
    debug_collector.set_dom_debug_info(
        xml_formatted,
        dom_xml.parsed_dom.get_html_string("", "", true),
    );

    // Generate and save PNG
    let result = styled_dom_to_png_with_debug(
        &dom_xml.parsed_dom,
        output_file,
        WIDTH,
        HEIGHT,
        dpi_factor,
        &mut debug_collector,
    )?;

    // Record rendering time
    let elapsed = start.elapsed();
    debug_collector.set_render_info(
        elapsed.as_millis() as u64,
        Vec::new(), // No warnings for now
    );

    // Save debug data to JSON
    let debug_data = debug_collector.get_data();
    let debug_json_path = output_file.with_file_name(
        output_file
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string()
            + "_debug.json",
    );

    let json = serde_json::to_string_pretty(&debug_data)?;
    std::fs::write(debug_json_path, json)?;

    Ok(debug_data)
}

fn styled_dom_to_png_with_debug(
    styled_dom: &StyledDom,
    output_file: &Path,
    width: u32,
    height: u32,
    dpi_factor: f32,
    debug_collector: &mut DebugDataCollector,
) -> Result<(), Box<dyn Error>> {
    // Create document ID and epoch for layout
    let document_id = DocumentId {
        namespace_id: IdNamespace(0),
        id: 0,
    };
    let epoch = Epoch::new();

    // Create window state for layout
    let mut fake_window_state = FullWindowState::default();
    fake_window_state.size.dimensions = LogicalSize {
        width: width as f32,
        height: height as f32,
    };
    fake_window_state.size.dpi = (96.0 * dpi_factor) as u32;

    // Create resources for layout
    let mut renderer_resources = azul_core::app_resources::RendererResources::default();
    let image_cache = ImageCache::default();

    // Solve layout with debug information
    let layout_result = solve_layout_with_debug(
        styled_dom.clone(),
        document_id,
        epoch,
        &fake_window_state,
        &mut renderer_resources,
        debug_collector,
    )?;

    // Get the cached display list
    let dom_id = DomId { inner: 0 };
    let mut cached_display_list = LayoutResult::get_cached_display_list(
        &document_id,
        dom_id,
        epoch,
        &[layout_result],
        &fake_window_state,
        &azul_core::app_resources::GlTextureCache::default(),
        &renderer_resources,
        &image_cache,
    );

    cached_display_list.scale_for_dpi(dpi_factor);

    // Capture display list for debugging
    let display_list_debug = format_display_list_for_debug(&cached_display_list);
    debug_collector.set_display_list_debug_info(display_list_debug);

    // Create a pixmap with a white background
    let mut pixmap = Pixmap::new(
        (width as f32 * dpi_factor) as u32,
        (height as f32 * dpi_factor) as u32,
    )
    .ok_or_else(|| format!("cannot create pixmap"))?;

    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    // Render the display list to the pixmap
    render_display_list(&cached_display_list, &mut pixmap, &renderer_resources)?;

    // Save the pixmap to a PNG file
    pixmap.save_png(output_file)?;

    Ok(())
}

fn solve_layout(
    styled_dom: StyledDom,
    document_id: DocumentId,
    epoch: Epoch,
    fake_window_state: &FullWindowState,
    renderer_resources: &mut azul_core::app_resources::RendererResources,
) -> Result<LayoutResult, Box<dyn Error>> {
    let fc_cache = azul_layout::font::loading::build_font_cache();
    let image_cache = ImageCache::default();
    let callbacks = RenderCallbacks {
        insert_into_active_gl_textures_fn: azul_core::gl::insert_into_active_gl_textures,
        layout_fn: azul_layout::solver2::do_the_layout,
        load_font_fn: azul_layout::font::loading::font_source_get_bytes,
        parse_font_fn: azul_layout::parse_font_fn,
    };

    // Solve the layout
    let mut resource_updates = Vec::new();
    let mut debug = Some(Vec::new());
    let id_namespace = IdNamespace(0);

    let mut solved_layout = SolvedLayout::new(
        styled_dom,
        epoch,
        &document_id,
        fake_window_state,
        &mut resource_updates,
        id_namespace,
        &image_cache,
        &fc_cache,
        &callbacks,
        renderer_resources,
        DpiScaleFactor {
            inner: FloatValue::new(fake_window_state.size.get_hidpi_factor()),
        },
        &mut debug,
    );

    if solved_layout.layout_results.is_empty() {
        // Handle error case with a default empty layout result
        Err(format!("    Warning: Failed to solve layout, using empty layout result").into())
    } else {
        Ok(solved_layout.layout_results.remove(0))
    }
}

fn render_display_list(
    display_list: &CachedDisplayList,
    pixmap: &mut Pixmap,
    renderer_resources: &RendererResources,
) -> Result<(), Box<dyn Error>> {
    // Start with root position and identity transform
    let transform = Transform::identity();

    match &display_list.root {
        DisplayListMsg::Frame(frame) => {
            render_frame(frame, pixmap, transform, None, renderer_resources)?;
        }
        DisplayListMsg::ScrollFrame(scroll_frame) => {
            render_scroll_frame(scroll_frame, pixmap, transform, renderer_resources)?;
        }
        DisplayListMsg::IFrame(_, _, _, cached_dl) => {
            render_display_list(cached_dl, pixmap, renderer_resources)?;
        }
    }

    Ok(())
}

fn render_frame(
    frame: &DisplayListFrame,
    pixmap: &mut Pixmap,
    transform: Transform,
    clip_rect: Option<Rect>,
    renderer_resources: &RendererResources,
) -> Result<(), Box<dyn Error>> {
    // Calculate the frame rectangle
    let frame_rect = match Rect::from_xywh(0.0, 0.0, frame.size.width, frame.size.height) {
        Some(rect) => rect,
        None => return Ok(()), // Invalid rect dimensions
    };

    // Render the frame content
    for content in &frame.content {
        render_content(
            content,
            pixmap,
            frame_rect,
            transform,
            clip_rect,
            renderer_resources,
        )?;
    }

    // Handle box shadow if any
    if let Some(box_shadow) = &frame.box_shadow {
        // Box shadow rendering would go here in a full implementation
    }

    // Render children
    for child in &frame.children {
        let child_pos = child.get_position();
        let rel_offset = child_pos.get_relative_offset();
        let offset_x = rel_offset.x;
        let offset_y = rel_offset.y;

        // Apply transform based on child position
        let child_transform = transform.pre_translate(offset_x, offset_y);

        match child {
            DisplayListMsg::Frame(child_frame) => {
                render_frame(
                    child_frame,
                    pixmap,
                    child_transform,
                    clip_rect,
                    renderer_resources,
                )?;
            }
            DisplayListMsg::ScrollFrame(scroll_frame) => {
                render_scroll_frame(scroll_frame, pixmap, child_transform, renderer_resources)?;
            }
            DisplayListMsg::IFrame(_, iframe_size, _, cached_dl) => {
                // Create a clip rect for the iframe
                let iframe_rect = match Rect::from_xywh(
                    offset_x,
                    offset_y,
                    iframe_size.width,
                    iframe_size.height,
                ) {
                    Some(rect) => rect,
                    None => continue,
                };

                // Recursively render the iframe with clipping
                render_display_list(cached_dl, pixmap, renderer_resources)?;
            }
        }
    }

    Ok(())
}

fn render_scroll_frame(
    scroll_frame: &DisplayListScrollFrame,
    pixmap: &mut Pixmap,
    transform: Transform,
    renderer_resources: &RendererResources,
) -> Result<(), Box<dyn Error>> {
    // Calculate scroll frame clip rectangle
    let clip_rect = match Rect::from_xywh(
        0.0,
        0.0,
        scroll_frame.parent_rect.size.width,
        scroll_frame.parent_rect.size.height,
    ) {
        Some(rect) => rect,
        None => return Ok(()), // Invalid rect dimensions
    };

    // Apply scroll offset
    let scroll_transform = transform.pre_translate(
        scroll_frame.content_rect.origin.x - scroll_frame.parent_rect.origin.x,
        scroll_frame.content_rect.origin.y - scroll_frame.parent_rect.origin.y,
    );

    // Render the frame with clipping
    render_frame(
        &scroll_frame.frame,
        pixmap,
        scroll_transform,
        Some(clip_rect),
        renderer_resources,
    )?;

    Ok(())
}

fn render_content(
    content: &LayoutRectContent,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    clip_rect: Option<Rect>,
    renderer_resources: &RendererResources,
) -> Result<(), Box<dyn Error>> {
    match content {
        LayoutRectContent::Background {
            content,
            size,
            offset,
            repeat,
        } => {
            render_background(
                content, *size, *offset, *repeat, pixmap, rect, transform, clip_rect,
            )?;
        }
        LayoutRectContent::Border {
            widths,
            colors,
            styles,
        } => {
            render_border(
                *widths, *colors, *styles, pixmap, rect, transform, clip_rect,
            )?;
        }
        LayoutRectContent::Text {
            glyphs,
            font_instance_key,
            color,
            glyph_options,
            overflow,
            text_shadow,
        } => {
            render_text(
                glyphs,
                *font_instance_key,
                *color,
                pixmap,
                rect,
                transform,
                clip_rect,
                renderer_resources,
            )?;
        }
        LayoutRectContent::Image {
            size,
            offset,
            image_rendering,
            alpha_type,
            image_key,
            background_color,
        } => {
            render_image(
                *size,
                *offset,
                *image_key,
                *background_color,
                pixmap,
                rect,
                transform,
                clip_rect,
            )?;
        }
    }

    Ok(())
}

fn render_background(
    content: &RectBackground,
    size: Option<azul_css::StyleBackgroundSize>,
    offset: Option<azul_css::StyleBackgroundPosition>,
    repeat: Option<azul_css::StyleBackgroundRepeat>,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    clip_rect: Option<Rect>,
) -> Result<(), Box<dyn Error>> {
    let mut paint = Paint::default();

    match content {
        RectBackground::Color(color) => {
            paint.set_color_rgba8(color.r, color.g, color.b, color.a);

            // Calculate background rectangle based on size and offset
            let bg_rect = calculate_background_rect(rect, size, offset);

            if let Some(bg_rect) = bg_rect {
                // Apply transforms and draw
                draw_rect_with_clip(pixmap, bg_rect, &paint, transform, clip_rect)?;
            }
        }
        RectBackground::LinearGradient(gradient) => {
            // Basic linear gradient rendering (simplified)
            if gradient.stops.as_slice().len() >= 2 {
                paint.set_color_rgba8(
                    gradient.stops.as_slice()[0].color.r,
                    gradient.stops.as_slice()[0].color.g,
                    gradient.stops.as_slice()[0].color.b,
                    gradient.stops.as_slice()[0].color.a,
                );

                let bg_rect = calculate_background_rect(rect, size, offset);
                if let Some(bg_rect) = bg_rect {
                    draw_rect_with_clip(pixmap, bg_rect, &paint, transform, clip_rect)?;
                }
            }
        }
        // For other background types, implement similar rendering logic
        _ => {
            // Default: draw a semi-transparent gray background as placeholder
            paint.set_color_rgba8(200, 200, 200, 100);
            draw_rect_with_clip(pixmap, rect, &paint, transform, clip_rect)?;
        }
    }

    Ok(())
}

fn calculate_background_rect(
    rect: Rect,
    size: Option<azul_css::StyleBackgroundSize>,
    offset: Option<azul_css::StyleBackgroundPosition>,
) -> Option<Rect> {
    // Default: use the entire rect
    let (width, height) = (rect.width(), rect.height());

    // Calculate size if specified
    let (bg_width, bg_height) = match size {
        Some(azul_css::StyleBackgroundSize::ExactSize([w, h])) => {
            let width_px = w.to_pixels(width) as f32;
            let height_px = h.to_pixels(height) as f32;
            (width_px, height_px)
        }
        Some(azul_css::StyleBackgroundSize::Contain) => {
            // Simplified contain logic - not fully implemented
            (width, height)
        }
        Some(azul_css::StyleBackgroundSize::Cover) => {
            // Simplified cover logic - not fully implemented
            (width, height)
        }
        None => (width, height),
    };

    // Calculate position if specified
    let (x_offset, y_offset) = match offset {
        Some(pos) => {
            // Simple horizontal position
            let x = match pos.horizontal {
                azul_css::BackgroundPositionHorizontal::Left => 0.0,
                azul_css::BackgroundPositionHorizontal::Center => (width - bg_width) / 2.0,
                azul_css::BackgroundPositionHorizontal::Right => width - bg_width,
                azul_css::BackgroundPositionHorizontal::Exact(val) => val.to_pixels(width) as f32,
            };

            // Simple vertical position
            let y = match pos.vertical {
                azul_css::BackgroundPositionVertical::Top => 0.0,
                azul_css::BackgroundPositionVertical::Center => (height - bg_height) / 2.0,
                azul_css::BackgroundPositionVertical::Bottom => height - bg_height,
                azul_css::BackgroundPositionVertical::Exact(val) => val.to_pixels(height) as f32,
            };

            (x, y)
        }
        None => (0.0, 0.0),
    };

    Rect::from_xywh(
        rect.x() + x_offset,
        rect.y() + y_offset,
        bg_width,
        bg_height,
    )
}

/// Translates a CSS border style to a StrokeDash pattern
fn translate_dash(style: &BorderStyle) -> Option<Vec<f32>> {
    match style {
        BorderStyle::None | BorderStyle::Hidden => None,
        BorderStyle::Solid => None, // No dash pattern for solid lines
        BorderStyle::Dotted => {
            // Dotted pattern: small on, small off
            Some(vec![1.0, 1.0])
        }
        BorderStyle::Dashed => {
            // Dashed pattern: longer on, small off
            Some(vec![3.0, 3.0])
        }
        // For these complex styles, we'll use solid lines as a fallback
        BorderStyle::Double
        | BorderStyle::Groove
        | BorderStyle::Ridge
        | BorderStyle::Inset
        | BorderStyle::Outset => None,
    }
}

fn render_border(
    widths: StyleBorderWidths,
    colors: StyleBorderColors,
    styles: StyleBorderStyles,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    _clip_rect: Option<Rect>,
) -> Result<(), Box<dyn Error>> {
    // Helper function to create a rounded corner path
    fn add_rounded_corner(
        pb: &mut PathBuilder,
        cx: f32,
        cy: f32,
        radius: f32,
        start_angle: f32,
        sweep_angle: f32,
    ) {
        if radius <= 0.0 {
            pb.line_to(cx, cy);
            return;
        }

        // Convert angles to radians
        let start_rad = start_angle * std::f32::consts::PI / 180.0;
        let end_rad = (start_angle + sweep_angle) * std::f32::consts::PI / 180.0;

        // Approximate a quarter circle with a cubic Bezier curve
        let kappa = 0.5522847498; // Magic constant for approximating a circle with cubics
        let control_dist = radius * kappa;

        let start_x = cx + radius * start_rad.cos();
        let start_y = cy + radius * start_rad.sin();

        let end_x = cx + radius * end_rad.cos();
        let end_y = cy + radius * end_rad.sin();

        // Calculate control points
        let ctrl1_x = start_x - control_dist * start_rad.sin();
        let ctrl1_y = start_y + control_dist * start_rad.cos();

        let ctrl2_x = end_x + control_dist * end_rad.sin();
        let ctrl2_y = end_y - control_dist * end_rad.cos();

        pb.line_to(start_x, start_y);
        pb.cubic_to(ctrl1_x, ctrl1_y, ctrl2_x, ctrl2_y, end_x, end_y);
    }

    // Helper function to render a border segment
    fn render_border_segment(
        width: f32,
        color: ColorU,
        style: BorderStyle,
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
        pixmap: &mut Pixmap,
        transform: Transform,
    ) -> Result<(), Box<dyn Error>> {
        if width <= 0.0 {
            return Ok(());
        }

        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);

        let mut pb = PathBuilder::new();
        pb.move_to(start_x, start_y);
        pb.line_to(end_x, end_y);

        if let Some(path) = pb.finish() {
            let transformed_path = path
                .transform(transform)
                .ok_or_else(|| "Failed to transform path".to_string())?;

            // Create stroke options with or without dash pattern
            let dash = translate_dash(&style);

            let stroke = tiny_skia::Stroke {
                width,
                miter_limit: 4.0,
                line_cap: tiny_skia::LineCap::Butt,
                line_join: tiny_skia::LineJoin::Miter,
                dash: dash.and_then(|sd| tiny_skia::StrokeDash::new(sd, 0.0)),
            };

            pixmap.stroke_path(
                &transformed_path,
                &paint,
                &stroke,
                Transform::identity(),
                None,
            );
        }

        Ok(())
    }

    // Helper to get border radius for a corner (top-left, top-right, etc.)
    // We should extract this from CSS properties, but for this example we'll use a simple approach
    let border_radius = 0.0; // Default to no radius

    // Get border widths
    let top_width = widths
        .top
        .and_then(|w| w.get_property().cloned())
        .map(|w| w.inner.to_pixels(rect.height()))
        .unwrap_or(0.0);

    let right_width = widths
        .right
        .and_then(|w| w.get_property().cloned())
        .map(|w| w.inner.to_pixels(rect.width()))
        .unwrap_or(0.0);

    let bottom_width = widths
        .bottom
        .and_then(|w| w.get_property().cloned())
        .map(|w| w.inner.to_pixels(rect.height()))
        .unwrap_or(0.0);

    let left_width = widths
        .left
        .and_then(|w| w.get_property().cloned())
        .map(|w| w.inner.to_pixels(rect.width()))
        .unwrap_or(0.0);

    // Get border styles
    let top_style = styles
        .top
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::BorderStyle::Solid);

    let right_style = styles
        .right
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::BorderStyle::Solid);

    let bottom_style = styles
        .bottom
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::BorderStyle::Solid);

    let left_style = styles
        .left
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::BorderStyle::Solid);

    let top_color = colors
        .top
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::ColorU::BLACK);

    let left_color = colors
        .left
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::ColorU::BLACK);

    let right_color = colors
        .right
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::ColorU::BLACK);

    let bottom_color = colors
        .bottom
        .and_then(|s| s.get_property().cloned())
        .map(|s| s.inner)
        .unwrap_or_else(|| azul_css::ColorU::BLACK);

    // Render all four borders using our helper function
    // Top border
    render_border_segment(
        top_width,
        top_color,
        top_style,
        rect.x() + border_radius,
        rect.y() + top_width / 2.0,
        rect.x() + rect.width() - border_radius,
        rect.y() + top_width / 2.0,
        pixmap,
        transform,
    )?;

    // Right border
    render_border_segment(
        right_width,
        right_color,
        right_style,
        rect.x() + rect.width() - right_width / 2.0,
        rect.y() + border_radius,
        rect.x() + rect.width() - right_width / 2.0,
        rect.y() + rect.height() - border_radius,
        pixmap,
        transform,
    )?;

    // Bottom border
    render_border_segment(
        bottom_width,
        bottom_color,
        bottom_style,
        rect.x() + rect.width() - border_radius,
        rect.y() + rect.height() - bottom_width / 2.0,
        rect.x() + border_radius,
        rect.y() + rect.height() - bottom_width / 2.0,
        pixmap,
        transform,
    )?;

    // Left border
    render_border_segment(
        left_width,
        left_color,
        left_style,
        rect.x() + left_width / 2.0,
        rect.y() + rect.height() - border_radius,
        rect.x() + left_width / 2.0,
        rect.y() + border_radius,
        pixmap,
        transform,
    )?;

    Ok(())
}

fn render_text(
    glyphs: &[azul_core::display_list::GlyphInstance],
    font_instance_key: azul_core::app_resources::FontInstanceKey,
    color: azul_css::ColorU,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    _clip_rect: Option<Rect>,
    renderer_resources: &RendererResources,
) -> Result<(), Box<dyn Error>> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);

    // Find the font and font size from the font_instance_key
    let font_instance = renderer_resources.get_renderable_font_data(&font_instance_key);

    if let Some((font_ref, au, dpi)) = font_instance {
        // Get the parsed font data
        let font_data = font_ref.get_data();
        let parsed_font = unsafe { &*(font_data.parsed as *const ParsedFont) };
        let units_per_em = parsed_font.font_metrics.units_per_em as f32;

        // Calculate font scale factor
        let font_size_px = au.into_px() * dpi.inner.get();
        let scale_factor = font_size_px / units_per_em;

        // Calculate baseline position (normally this would come from the font metrics)
        let baseline_y = rect.y() + parsed_font.font_metrics.ascender as f32 * scale_factor;

        // Draw each glyph
        for glyph in glyphs {
            let glyph_index = glyph.index as u16;

            // Find the glyph outline in the parsed font
            if let Some(glyph_data) = parsed_font.glyph_records_decoded.get(&glyph_index) {
                if let Some(outline) = &glyph_data.outline {
                    // Create path from outline
                    let mut pb = PathBuilder::new();
                    let mut is_first = true;

                    for op in outline.operations.as_ref() {
                        match op {
                            GlyphOutlineOperation::MoveTo(pt) => {
                                // Scale and position the point
                                let x = rect.x() + glyph.point.x + pt.x * scale_factor;
                                let y = baseline_y - pt.y * scale_factor;

                                if is_first {
                                    pb.move_to(x, y);
                                    is_first = false;
                                } else {
                                    pb.move_to(x, y);
                                }
                            }
                            GlyphOutlineOperation::LineTo(pt) => {
                                let x = rect.x() + glyph.point.x + pt.x * scale_factor;
                                let y = baseline_y - pt.y * scale_factor;
                                pb.line_to(x, y);
                            }
                            GlyphOutlineOperation::QuadraticCurveTo(qt) => {
                                let ctrl_x = rect.x() + glyph.point.x + qt.ctrl_1_x * scale_factor;
                                let ctrl_y = baseline_y - qt.ctrl_1_y * scale_factor;
                                let end_x = rect.x() + glyph.point.x + qt.end_x * scale_factor;
                                let end_y = baseline_y - qt.end_y * scale_factor;
                                pb.quad_to(ctrl_x, ctrl_y, end_x, end_y);
                            }
                            GlyphOutlineOperation::CubicCurveTo(ct) => {
                                let ctrl1_x = rect.x() + glyph.point.x + ct.ctrl_1_x * scale_factor;
                                let ctrl1_y = baseline_y - ct.ctrl_1_y * scale_factor;
                                let ctrl2_x = rect.x() + glyph.point.x + ct.ctrl_2_x * scale_factor;
                                let ctrl2_y = baseline_y - ct.ctrl_2_y * scale_factor;
                                let end_x = rect.x() + glyph.point.x + ct.end_x * scale_factor;
                                let end_y = baseline_y - ct.end_y * scale_factor;
                                pb.cubic_to(ctrl1_x, ctrl1_y, ctrl2_x, ctrl2_y, end_x, end_y);
                            }
                            GlyphOutlineOperation::ClosePath => {
                                pb.close();
                            }
                        }
                    }

                    if let Some(path) = pb.finish() {
                        let transformed_path = path
                            .transform(transform)
                            .ok_or_else(|| "Failed to transform text path".to_string())?;
                        pixmap.fill_path(
                            &transformed_path,
                            &paint,
                            tiny_skia::FillRule::Winding,
                            Transform::identity(),
                            None,
                        );
                    }
                } else {
                    // No outline data, use a fallback rectangle for the glyph
                    let width = glyph_data.horz_advance as f32 * scale_factor;
                    let height = (parsed_font.font_metrics.ascender
                        - parsed_font.font_metrics.descender)
                        as f32
                        * scale_factor;

                    if let Some(glyph_rect) = Rect::from_xywh(
                        rect.x() + glyph.point.x,
                        baseline_y - parsed_font.font_metrics.ascender as f32 * scale_factor,
                        width,
                        height,
                    ) {
                        let mut pb = PathBuilder::new();
                        let rect = Rect::from_xywh(
                            glyph_rect.x(),
                            glyph_rect.y(),
                            glyph_rect.width(),
                            glyph_rect.height(),
                        );
                        if let Some(r) = rect {
                            pb.push_rect(r);
                        }
                        if let Some(path) = pb.finish() {
                            let transformed_path = path
                                .transform(transform)
                                .ok_or_else(|| "Failed to transform text path".to_string())?;
                            pixmap.fill_path(
                                &transformed_path,
                                &paint,
                                tiny_skia::FillRule::Winding,
                                Transform::identity(),
                                None,
                            );
                        }
                    }
                }
            }
        }
    } else {
        // Fallback: just draw a simple line for text baseline
        if let Some(text_rect) =
            Rect::from_xywh(rect.x(), rect.y() + rect.height() * 0.75, rect.width(), 1.0)
        {
            let mut pb = PathBuilder::new();
            if let Some(text_rect2) = Rect::from_xywh(
                text_rect.x(),
                text_rect.y(),
                text_rect.width(),
                text_rect.height(),
            ) {
                pb.push_rect(text_rect2);
            }

            if let Some(path) = pb.finish() {
                let transformed_path = path
                    .transform(transform)
                    .ok_or_else(|| "Failed to transform text path".to_string())?;
                pixmap.fill_path(
                    &transformed_path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    Ok(())
}

fn render_image(
    size: azul_core::window::LogicalSize,
    offset: azul_core::window::LogicalPosition,
    image_key: azul_core::app_resources::ImageKey,
    bg_color: azul_css::ColorU,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    clip_rect: Option<Rect>,
) -> Result<(), Box<dyn Error>> {
    // Simplified image rendering - just draws a colored rectangle with a border
    let img_rect = match Rect::from_xywh(
        rect.x() + offset.x,
        rect.y() + offset.y,
        size.width,
        size.height,
    ) {
        Some(r) => r,
        None => return Ok(()),
    };

    // Draw background color
    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(bg_color.r, bg_color.g, bg_color.b, bg_color.a);
    draw_rect_with_clip(pixmap, img_rect, &bg_paint, transform, clip_rect)?;

    // Draw border to indicate it's an image
    let mut border_paint = Paint::default();
    border_paint.set_color_rgba8(100, 100, 100, 200);

    // Create a path for the border
    let mut pb = PathBuilder::new();
    pb.move_to(img_rect.x(), img_rect.y());
    pb.line_to(img_rect.x() + img_rect.width(), img_rect.y());
    pb.line_to(
        img_rect.x() + img_rect.width(),
        img_rect.y() + img_rect.height(),
    );
    pb.line_to(img_rect.x(), img_rect.y() + img_rect.height());
    pb.close();

    if let Some(path) = pb.finish() {
        // Apply transform
        let transformed_path = path
            .transform(transform)
            .ok_or_else(|| format!("cannot transform path"))?;

        // Apply clipping
        if let Some(clip) = clip_rect {
            let mut mask = tiny_skia::Mask::new(pixmap.width(), pixmap.height())
                .ok_or_else(|| format!("cannot create clip maps {clip:?}"))?;

            // Create clip path
            let mut clip_pb = PathBuilder::new();
            clip_pb.move_to(clip.x(), clip.y());
            clip_pb.line_to(clip.x() + clip.width(), clip.y());
            clip_pb.line_to(clip.x() + clip.width(), clip.y() + clip.height());
            clip_pb.line_to(clip.x(), clip.y() + clip.height());
            clip_pb.close();

            if let Some(clip_path) = clip_pb.finish() {
                mask.fill_path(&clip_path, FillRule::Winding, true, Transform::identity());
                pixmap.stroke_path(
                    &transformed_path,
                    &border_paint,
                    &tiny_skia::Stroke::default(),
                    Transform::identity(),
                    Some(&mask),
                );
            }
        } else {
            pixmap.stroke_path(
                &transformed_path,
                &border_paint,
                &tiny_skia::Stroke::default(),
                Transform::identity(),
                None,
            );
        }
    }

    Ok(())
}

fn draw_rect_with_clip(
    pixmap: &mut Pixmap,
    rect: Rect,
    paint: &Paint,
    transform: Transform,
    clip_rect: Option<Rect>,
) -> Result<(), Box<dyn Error>> {
    // Create a path for the rectangle
    let mut pb = PathBuilder::new();
    pb.move_to(rect.x(), rect.y());
    pb.line_to(rect.x() + rect.width(), rect.y());
    pb.line_to(rect.x() + rect.width(), rect.y() + rect.height());
    pb.line_to(rect.x(), rect.y() + rect.height());
    pb.close();

    if let Some(path) = pb.finish() {
        // Apply transform
        let transformed_path = path
            .transform(transform)
            .ok_or_else(|| format!("cannot draw rect with transformed clip"))?;

        // Apply clipping
        if let Some(clip) = clip_rect {
            let mut mask = tiny_skia::Mask::new(pixmap.width(), pixmap.height())
                .ok_or_else(|| format!("cannot draw rect with transformed clip {clip:?}"))?;

            // Create clip path
            let mut clip_pb = PathBuilder::new();
            clip_pb.move_to(clip.x(), clip.y());
            clip_pb.line_to(clip.x() + clip.width(), clip.y());
            clip_pb.line_to(clip.x() + clip.width(), clip.y() + clip.height());
            clip_pb.line_to(clip.x(), clip.y() + clip.height());
            clip_pb.close();

            if let Some(clip_path) = clip_pb.finish() {
                mask.fill_path(&clip_path, FillRule::Winding, true, Transform::identity());
                pixmap.fill_path(
                    &transformed_path,
                    paint,
                    FillRule::Winding,
                    Transform::identity(),
                    Some(&mask),
                );
            }
        } else {
            pixmap.fill_path(
                &transformed_path,
                paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    Ok(())
}

#[derive(Debug, Copy, Clone)]
pub struct Options {
    /// matching threshold (0 to 1); smaller is more sensitive
    pub threshold: f64,
    /// whether to skip anti-aliasing detection
    pub include_aa: bool,
    /// opacity of original image in diff output
    pub alpha: f64,
    /// color of anti-aliased pixels in diff output
    pub aa_color: [u8; 4],
    /// color of different pixels in diff output
    pub diff_color: [u8; 4],
    /// whether to detect dark on light differences between img1 and img2 and set an alternative
    /// color to differentiate between the two
    pub diff_color_alt: Option<[u8; 4]>,
    /// draw the diff over a transparent background (a mask)
    pub diff_mask: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            threshold: 0.1,
            include_aa: false,
            alpha: 0.1,
            aa_color: [255, 255, 0, 255],
            diff_color: [255, 0, 0, 255],
            diff_color_alt: None,
            diff_mask: false,
        }
    }
}

fn compare_images(
    chrome_png: &Path,
    azul_png: &Path,
    diff_png: &Path,
) -> Result<usize, Box<dyn Error>> {
    println!(
        "  Comparing images: {} vs {}",
        chrome_png.display(),
        azul_png.display()
    );

    // Load images
    let chrome_img = std::fs::read(chrome_png)?;
    let azul_img = std::fs::read(chrome_png)?;

    // Use pixelmatch to compare the images
    let options = pixelmatch::Options {
        threshold: 0.1,
        include_aa: false,
        alpha: 0.1,
        aa_color: [255, 255, 0, 255],
        diff_color: [255, 0, 0, 255],
        diff_color_alt: Some([0, 255, 0, 255]),
        diff_mask: false,
    };

    let mut diff_img = Cursor::new(Vec::new());

    // Compare images
    let diff_count = pixelmatch::pixelmatch(&chrome_img, &azul_img, &mut diff_img, Some(options))?;

    // Save the diff image
    std::fs::write(diff_png, diff_img.into_inner())?;

    Ok(diff_count)
}

fn generate_html_report(output_dir: &Path, results: &[TestResult]) -> Result<(), Box<dyn Error>> {
    let report_path = output_dir.join("report.html");
    let mut file = File::create(&report_path)?;

    // HTML header
    writeln!(file, "<!DOCTYPE html>")?;
    writeln!(file, "<html lang=\"en\">")?;
    writeln!(file, "<head>")?;
    writeln!(file, "  <meta charset=\"UTF-8\">")?;
    writeln!(
        file,
        "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">"
    )?;
    writeln!(file, "  <title>Azul CSS Reftest Results</title>")?;
    writeln!(file, "  <style>")?;
    writeln!(
        file,
        "    body {{ font-family: sans-serif; margin: 20px; }}"
    )?;
    writeln!(file, "    h1 {{ color: #333; }}")?;
    writeln!(
        file,
        "    table {{ border-collapse: collapse; width: 100%; }}"
    )?;
    writeln!(
        file,
        "    th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}"
    )?;
    writeln!(file, "    th {{ background-color: #f2f2f2; }}")?;
    writeln!(
        file,
        "    tr:nth-child(even) {{ background-color: #f9f9f9; }}"
    )?;
    writeln!(file, "    .passed {{ background-color: #dff0d8; }}")?;
    writeln!(file, "    .failed {{ background-color: #f2dede; }}")?;
    writeln!(file, "    img {{ max-width: 100%; height: auto; }}")?;
    writeln!(
        file,
        "    .summary {{ margin: 20px 0; padding: 10px; background-color: #f5f5f5; border-radius: \
         5px; }}"
    )?;
    writeln!(file, "  </style>")?;
    writeln!(file, "</head>")?;
    writeln!(file, "<body>")?;

    // Report header
    writeln!(file, "  <h1>Azul CSS Reftest Results</h1>")?;

    // Summary section
    let total_tests = results.len();
    let passed_tests = results.iter().filter(|r| r.passed).count();

    writeln!(file, "  <div class=\"summary\">")?;
    writeln!(file, "    <h2>Summary</h2>")?;
    writeln!(file, "    <p>Total tests: {}</p>", total_tests)?;
    writeln!(
        file,
        "    <p>Passed tests: {} ({}%)</p>",
        passed_tests,
        if total_tests > 0 {
            passed_tests * 100 / total_tests
        } else {
            0
        }
    )?;
    writeln!(
        file,
        "    <p>Failed tests: {} ({}%)</p>",
        total_tests - passed_tests,
        if total_tests > 0 {
            (total_tests - passed_tests) * 100 / total_tests
        } else {
            0
        }
    )?;
    writeln!(file, "  </div>")?;

    // Results table
    writeln!(file, "  <table>")?;
    writeln!(file, "    <tr>")?;
    writeln!(file, "      <th>Test</th>")?;
    writeln!(file, "      <th>Chrome Reference</th>")?;
    writeln!(file, "      <th>Azul Rendering</th>")?;
    writeln!(file, "      <th>Difference</th>")?;
    writeln!(file, "      <th>Diff Count</th>")?;
    writeln!(file, "      <th>Result</th>")?;
    writeln!(file, "    </tr>")?;

    // Sort results by test name
    let mut sorted_results = results.to_vec();
    sorted_results.sort_by(|a, b| a.test_name.cmp(&b.test_name));

    for result in sorted_results {
        let row_class = if result.passed { "passed" } else { "failed" };

        writeln!(file, "    <tr class=\"{}\">", row_class)?;
        writeln!(file, "      <td>{}</td>", result.test_name)?;
        writeln!(
            file,
            "      <td><img src=\"{}_chrome.png\" alt=\"Chrome\"></td>",
            result.test_name
        )?;
        writeln!(
            file,
            "      <td><img src=\"{}_azul.png\" alt=\"Azul\"></td>",
            result.test_name
        )?;
        writeln!(
            file,
            "      <td><img src=\"{}_diff.png\" alt=\"Difference\"></td>",
            result.test_name
        )?;
        writeln!(file, "      <td>{}</td>", result.diff_count)?;
        writeln!(
            file,
            "      <td>{}</td>",
            if result.passed { "PASS" } else { "FAIL" }
        )?;
        writeln!(file, "    </tr>")?;
    }

    writeln!(file, "  </table>")?;
    writeln!(file, "</body>")?;
    writeln!(file, "</html>")?;

    println!("HTML report generated at {}", report_path.display());

    Ok(())
}

fn generate_json_results(
    output_dir: &Path,
    results: &[TestResult],
    passed_tests: usize,
) -> Result<(), Box<dyn Error>> {
    let json_path = output_dir.join("results.json");
    let mut file = File::create(&json_path)?;

    let test_results = TestResults {
        tests: results.to_vec(),
        total_tests: results.len(),
        passed_tests,
    };

    let json = serde_json::to_string_pretty(&test_results)?;
    file.write_all(json.as_bytes())?;

    println!("JSON results saved to {}", json_path.display());

    Ok(())
}

mod pixelmatch {

    // modified from https://github.com/dfrankland/pixelmatch-rs
    //
    // MIT License
    //
    // Copyright (c) 2021 Dylan Frankland
    //
    // Permission is hereby granted, free of charge, to any person obtaining a copy
    // of this software and associated documentation files (the "Software"), to deal
    // in the Software without restriction, including without limitation the rights
    // to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    // copies of the Software, and to permit persons to whom the Software is
    // furnished to do so, subject to the following conditions:
    //
    // The above copyright notice and this permission notice shall be included in all
    // copies or substantial portions of the Software.
    //
    // THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    // IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    // FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    // AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    // LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    // OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
    // SOFTWARE.

    use core::f64;
    use std::io::{BufRead, Cursor, Read, Seek, Write};

    use image::{
        codecs::png::PngDecoder, save_buffer, DynamicImage, GenericImage, GenericImageView,
        ImageFormat, Rgba,
    };

    pub struct Options {
        /// matching threshold (0 to 1); smaller is more sensitive
        pub threshold: f64,
        /// whether to skip anti-aliasing detection
        pub include_aa: bool,
        /// opacity of original image in diff output
        pub alpha: f64,
        /// color of anti-aliased pixels in diff output
        pub aa_color: [u8; 4],
        /// color of different pixels in diff output
        pub diff_color: [u8; 4],
        /// whether to detect dark on light differences between img1 and img2 and set an
        /// alternative color to differentiate between the two
        pub diff_color_alt: Option<[u8; 4]>,
        /// draw the diff over a transparent background (a mask)
        pub diff_mask: bool,
    }

    impl Default for Options {
        fn default() -> Self {
            Options {
                threshold: 0.1,
                include_aa: false,
                alpha: 0.1,
                aa_color: [255, 255, 0, 255],
                diff_color: [255, 0, 0, 255],
                diff_color_alt: None,
                diff_mask: false,
            }
        }
    }

    pub fn pixelmatch(
        img1: &[u8],
        img2: &[u8],
        output: &mut Cursor<Vec<u8>>,
        options: Option<Options>,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let img1 = image::load_from_memory(img1)?;
        let img2 = image::load_from_memory(img2)?;

        let img1_dimensions = img1.dimensions();
        if img1.dimensions() != img2.dimensions() {
            return Err(<Box<dyn std::error::Error>>::from(
                "Image sizes do not match.",
            ));
        }

        let options = options.unwrap_or_default();
        let mut img_out = DynamicImage::new_rgba8(img1_dimensions.0, img1_dimensions.1);

        // check if images are identical
        let mut identical = true;
        for (pixel1, pixel2) in img1.pixels().zip(img2.pixels()) {
            if pixel1 != pixel2 {
                identical = false;
                break;
            }
        }

        // fast path if identical
        if identical {
            if !options.diff_mask {
                for pixel in img1.pixels() {
                    draw_gray_pixel(&pixel, options.alpha, &mut img_out)?;
                }
            }

            img_out.write_to(output, ImageFormat::Png)?;
            return Ok(0);
        }

        // maximum acceptable square distance between two colors;
        // 35215 is the maximum possible value for the YIQ difference metric
        let max_delta = 35215_f64 * options.threshold * options.threshold;
        let mut diff: usize = 0;

        for (pixel1, pixel2) in img1.pixels().zip(img2.pixels()) {
            let delta = color_delta(&pixel1.2, &pixel2.2, false);

            if delta.abs() > max_delta {
                // check it's a real rendering difference or just anti-aliasing
                if !options.include_aa
                    && (antialiased(
                        &img1,
                        pixel1.0,
                        pixel1.1,
                        img1_dimensions.0,
                        img1_dimensions.1,
                        &img2,
                    ) || antialiased(
                        &img2,
                        pixel1.0,
                        pixel1.1,
                        img1_dimensions.0,
                        img1_dimensions.1,
                        &img1,
                    ))
                {
                    // one of the pixels is anti-aliasing; draw as yellow and do not count as
                    // difference note that we do not include such pixels in a
                    // mask
                    if let (img_out, false) = (&mut img_out, options.diff_mask) {
                        img_out.put_pixel(pixel1.0, pixel1.1, Rgba(options.aa_color));
                    }
                } else {
                    // found substantial difference not caused by anti-aliasing; draw it as such
                    let color = if delta < 0.0 {
                        options.diff_color_alt.unwrap_or(options.diff_color)
                    } else {
                        options.diff_color
                    };
                    img_out.put_pixel(pixel1.0, pixel1.1, Rgba(color));
                    diff += 1;
                }
            } else if let (img_out, false) = (&mut img_out, options.diff_mask) {
                // pixels are similar; draw background as grayscale image blended with white
                draw_gray_pixel(&pixel1, options.alpha, img_out)?;
            }
        }

        img_out.write_to(output, ImageFormat::Png)?;

        Ok(diff)
    }

    // check if a pixel is likely a part of anti-aliasing;
    // based on "Anti-aliased Pixel and Intensity Slope Detector" paper by V. Vysniauskas, 2009
    fn antialiased(
        img1: &DynamicImage,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        img2: &DynamicImage,
    ) -> bool {
        let mut zeroes: u8 = if x == 0 || y == 0 || x == width - 1 || y == height - 1 {
            1
        } else {
            0
        };

        let mut min = 0.0;
        let mut max = 0.0;

        let mut min_x = 0;
        let mut min_y = 0;
        let mut max_x = 0;
        let mut max_y = 0;

        let center_rgba = img1.get_pixel(x, y);

        for adjacent_x in (if x > 0 { x - 1 } else { x })..=(if x < width - 1 { x + 1 } else { x })
        {
            for adjacent_y in
                (if y > 0 { y - 1 } else { y })..=(if y < height - 1 { y + 1 } else { y })
            {
                if adjacent_x == x && adjacent_y == y {
                    continue;
                }

                // brightness delta between the center pixel and adjacent one
                let rgba = img1.get_pixel(adjacent_x, adjacent_y);
                let delta = color_delta(&center_rgba, &rgba, true);

                // count the number of equal, darker and brighter adjacent pixels
                if delta == 0.0 {
                    zeroes += 1;

                    // if found more than 2 equal siblings, it's definitely not anti-aliasing
                    if zeroes > 2 {
                        return false;
                    }

                    continue;
                }

                // remember the darkest pixel
                if delta < min {
                    min = delta;
                    min_x = adjacent_x;
                    min_y = adjacent_y;

                    continue;
                }

                // remember the brightest pixel
                if delta > max {
                    max = delta;
                    max_x = adjacent_x;
                    max_y = adjacent_y;
                }
            }
        }

        // if there are no both darker and brighter pixels among siblings, it's not anti-aliasing
        if min == 0.0 || max == 0.0 {
            return false;
        }

        // if either the darkest or the brightest pixel has 3+ equal siblings in both images
        // (definitely not anti-aliased), this pixel is anti-aliased
        (has_many_siblings(img1, min_x, min_y, width, height)
            && has_many_siblings(img2, min_x, min_y, width, height))
            || (has_many_siblings(img1, max_x, max_y, width, height)
                && has_many_siblings(img2, max_x, max_y, width, height))
    }

    // check if a pixel has 3+ adjacent pixels of the same color.
    fn has_many_siblings(img: &DynamicImage, x: u32, y: u32, width: u32, height: u32) -> bool {
        let mut zeroes: u8 = if x == 0 || y == 0 || x == width - 1 || y == height - 1 {
            1
        } else {
            0
        };

        let center_rgba = img.get_pixel(x, y);

        for adjacent_x in (if x > 0 { x - 1 } else { x })..=(if x < width - 1 { x + 1 } else { x })
        {
            for adjacent_y in
                (if y > 0 { y - 1 } else { y })..=(if y < height - 1 { y + 1 } else { y })
            {
                if adjacent_x == x && adjacent_y == y {
                    continue;
                }

                let rgba = img.get_pixel(adjacent_x, adjacent_y);

                if center_rgba == rgba {
                    zeroes += 1;
                }

                if zeroes > 2 {
                    return true;
                }
            }
        }

        false
    }

    // calculate color difference according to the paper "Measuring perceived color difference
    // using YIQ NTSC transmission color space in mobile applications" by Y. Kotsarenko and F. Ramos
    fn color_delta(rgba1: &Rgba<u8>, rgba2: &Rgba<u8>, y_only: bool) -> f64 {
        let mut r1 = rgba1[0] as f64;
        let mut g1 = rgba1[1] as f64;
        let mut b1 = rgba1[2] as f64;
        let mut a1 = rgba1[3] as f64;

        let mut r2 = rgba2[0] as f64;
        let mut g2 = rgba2[1] as f64;
        let mut b2 = rgba2[2] as f64;
        let mut a2 = rgba2[3] as f64;

        if (a1 - a2).abs() < f64::EPSILON
            && (r1 - r2).abs() < f64::EPSILON
            && (g1 - g2).abs() < f64::EPSILON
            && (b1 - b2).abs() < f64::EPSILON
        {
            return 0.0;
        }

        if a1 < 255.0 {
            a1 /= 255.0;
            r1 = blend(r1, a1);
            g1 = blend(g1, a1);
            b1 = blend(b1, a1);
        }

        if a2 < 255.0 {
            a2 /= 255.0;
            r2 = blend(r2, a2);
            g2 = blend(g2, a2);
            b2 = blend(b2, a2);
        }

        let y1 = rgb2y(r1, g1, b1);
        let y2 = rgb2y(r2, g2, b2);
        let y = y1 - y2;

        // brightness difference only
        if y_only {
            return y;
        }

        let i = rgb2i(r1, g1, b1) - rgb2i(r2, g2, b2);
        let q = rgb2q(r1, g1, b1) - rgb2q(r2, g2, b2);

        let delta = 0.5053 * y * y + 0.299 * i * i + 0.1957 * q * q;

        // encode whether the pixel lightens or darkens in the sign
        if y1 > y2 {
            -delta
        } else {
            delta
        }
    }

    fn draw_gray_pixel(
        (x, y, rgba): &(u32, u32, Rgba<u8>),
        alpha: f64,
        output: &mut DynamicImage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !output.in_bounds(*x, *y) {
            return Err(<Box<dyn std::error::Error>>::from(
                "Pixel is not in bounds of output.",
            ));
        }

        let val = blend(
            rgb2y(rgba[0], rgba[1], rgba[2]),
            (alpha * rgba[3] as f64) / 255.0,
        ) as u8;
        let gray_rgba = Rgba([val, val, val, val]);
        output.put_pixel(*x, *y, gray_rgba);

        Ok(())
    }

    // blend semi-transparent color with white
    fn blend<T: Into<f64>>(c: T, a: T) -> f64 {
        255.0 + (c.into() - 255.0) * a.into()
    }

    fn rgb2y<T: Into<f64>>(r: T, g: T, b: T) -> f64 {
        r.into() * 0.29889531 + g.into() * 0.58662247 + b.into() * 0.11448223
    }
    fn rgb2i<T: Into<f64>>(r: T, g: T, b: T) -> f64 {
        r.into() * 0.59597799 - g.into() * 0.27417610 - b.into() * 0.32180189
    }
    fn rgb2q<T: Into<f64>>(r: T, g: T, b: T) -> f64 {
        r.into() * 0.21147017 - g.into() * 0.52261711 + b.into() * 0.31114694
    }
}

/// Metadata extracted from a test file
#[derive(Debug, Clone, Default)]
pub struct TestMetadata {
    pub title: String,
    pub assert_content: String,
    pub help_link: String,
    pub flags: String,
    pub author: String,
}

/// Enhanced XML parser that extracts metadata from test files
pub struct EnhancedXmlParser;

impl EnhancedXmlParser {
    /// Parse an XHTML file and extract both the DOM and metadata
    pub fn parse_test_file(
        file_path: &Path,
    ) -> Result<(DomXml, TestMetadata, Vec<XmlNode>), String> {
        // Read file content
        let xml_content = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(e) => return Err(format!("Error reading file: {}", e)),
        };

        // Parse XML
        let parsed_xml = match parse_xml_string(&xml_content) {
            Ok(nodes) => nodes,
            Err(e) => return Err(format!("XML parse error: {}", e)),
        };

        // Extract metadata
        let metadata = Self::extract_metadata(&parsed_xml);

        // Parse to DOM
        let dom = domxml_from_str(&xml_content, &mut XmlComponentMap::default());

        Ok((dom, metadata, parsed_xml))
    }

    /// Extract metadata from parsed XML nodes
    pub fn extract_metadata(nodes: &[XmlNode]) -> TestMetadata {
        let mut metadata = TestMetadata::default();

        // Find the <html> node
        if let Ok(html_node) = get_html_node(nodes) {
            // Look for <head> node
            for child in html_node.children.as_ref() {
                if child.node_type.as_str().to_lowercase() == "head" {
                    Self::extract_head_metadata(child, &mut metadata);
                }
            }
        }

        metadata
    }

    /// Extract metadata from the <head> node
    fn extract_head_metadata(head_node: &XmlNode, metadata: &mut TestMetadata) {
        for child in head_node.children.as_ref() {
            match child.node_type.as_str().to_lowercase().as_str() {
                "title" => {
                    if let Some(text) = &child.text.into_option() {
                        metadata.title = text.as_str().to_string();
                    }
                }
                "meta" => {
                    // Handle meta tags
                    let name = Self::get_attribute_value(&child.attributes, "name");
                    let content = Self::get_attribute_value(&child.attributes, "content");

                    if let (Some(name), Some(content)) = (name, content) {
                        match name.as_str() {
                            "assert" => metadata.assert_content = content,
                            "flags" => metadata.flags = content,
                            _ => {} // Ignore other meta tags
                        }
                    }
                }
                "link" => {
                    // Handle link tags
                    let rel = Self::get_attribute_value(&child.attributes, "rel");

                    if let Some(rel) = rel {
                        match rel.as_str() {
                            "help" => {
                                if let Some(href) =
                                    Self::get_attribute_value(&child.attributes, "href")
                                {
                                    metadata.help_link = href;
                                }
                            }
                            "author" => {
                                if let Some(title) =
                                    Self::get_attribute_value(&child.attributes, "title")
                                {
                                    metadata.author = title;
                                }
                            }
                            _ => {} // Ignore other link types
                        }
                    }
                }
                _ => {} // Ignore other head elements
            }
        }
    }

    /// Get attribute value by name from attributes list
    fn get_attribute_value(attributes: &StringPairVec, name: &str) -> Option<String> {
        for attr in attributes.as_ref() {
            if attr.key.as_str() == name {
                return Some(attr.value.as_str().to_string());
            }
        }
        None
    }

    /// Format XML node for debugging display
    pub fn format_xml_for_display(node: &XmlNode, indent: usize) -> String {
        let indent_str = " ".repeat(indent);
        let mut output = format!("{}{}:\n", indent_str, node.node_type.as_str());

        // Add attributes
        if !node.attributes.is_empty() {
            output.push_str(&format!("{}  Attributes:\n", indent_str));
            for attr in node.attributes.as_ref() {
                output.push_str(&format!(
                    "{}    {} = \"{}\"\n",
                    indent_str,
                    attr.key.as_str(),
                    attr.value.as_str()
                ));
            }
        }

        // Add text content
        if let Some(text) = &node.text.into_option() {
            if !text.as_str().trim().is_empty() {
                output.push_str(&format!(
                    "{}  Text: \"{}\"\n",
                    indent_str,
                    text.as_str().trim()
                ));
            }
        }

        // Add children
        if !node.children.is_empty() {
            output.push_str(&format!("{}  Children:\n", indent_str));
            for child in node.children.as_ref() {
                output.push_str(&Self::format_xml_for_display(child, indent + 4));
            }
        }

        output
    }
}

/// CSS Warning types
#[derive(Debug, Clone)]
pub enum CssWarningType {
    /// Parse error
    ParseError(CssParseWarnMsgOwned),
    /// Property not supported
    UnsupportedProperty(String),
    /// Value out of range
    ValueOutOfRange(String, String),
    /// Unknown selector
    UnknownSelector(String),
    /// Potentially invalid rule
    InvalidRule(String),
}

impl std::fmt::Display for CssWarningType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CssWarningType::ParseError(err) => write!(
                f,
                "Parse error at {:?}..{:?}: {}",
                err.location.0,
                err.location.1,
                err.warning.to_shared()
            ),
            CssWarningType::UnsupportedProperty(prop) => {
                write!(f, "Unsupported property: {}", prop)
            }
            CssWarningType::ValueOutOfRange(prop, val) => {
                write!(f, "Value out of range for {}: {}", prop, val)
            }
            CssWarningType::UnknownSelector(sel) => write!(f, "Unknown selector: {}", sel),
            CssWarningType::InvalidRule(rule) => write!(f, "Potentially invalid rule: {}", rule),
        }
    }
}

/// Collects CSS warnings during parsing and validation
pub struct CssWarningCollector {
    pub warnings: Vec<CssWarningType>,
}

impl CssWarningCollector {
    /// Create a new CSS warning collector
    pub fn new() -> Self {
        Self {
            warnings: Vec::new(),
        }
    }

    /// Parse CSS and collect warnings
    pub fn parse_css(&mut self, css_text: &str) -> Css {
        // Parse CSS using the wrapper
        let (api_wrapper, warnings) =
            CssApiWrapper::from_string_with_warnings(css_text.to_string().into());

        // Check for parse errors
        for w in warnings {
            self.warnings.push(CssWarningType::ParseError(w));
        }

        // Validate the CSS properties
        self.validate_css(&api_wrapper.css);

        // Get the parsed CSS
        api_wrapper.css
    }

    /// Validate CSS properties and collect warnings
    fn validate_css(&mut self, css: &Css) {
        for stylesheet in css.stylesheets.as_ref() {
            for rule in stylesheet.rules.as_ref() {
                // Check selector validity
                self.validate_selector(&rule.path.to_string());

                // Check property validity
                for decl in rule.declarations.as_ref() {
                    match decl {
                        CssDeclaration::Static(prop) => {
                            self.validate_property(prop);
                        }
                        CssDeclaration::Dynamic(dynamic) => {
                            self.validate_property(&dynamic.default_value);
                        }
                    }
                }
            }
        }
    }

    /// Validate a CSS selector
    fn validate_selector(&mut self, selector: &str) {
        // Check for potential selector issues
        if selector.contains(">>") {
            self.warnings.push(CssWarningType::UnknownSelector(format!(
                "Non-standard selector syntax: {}",
                selector
            )));
        }

        // Check for potentially unsupported pseudo-selectors
        let problematic_pseudos = [":has(", ":is(", ":where(", "::part(", "::slotted("];
        for pseudo in problematic_pseudos {
            if selector.contains(pseudo) {
                self.warnings.push(CssWarningType::UnknownSelector(format!(
                    "Potentially unsupported pseudo-selector: {}",
                    pseudo
                )));
            }
        }
    }

    /// Validate a CSS property
    fn validate_property(&mut self, property: &CssProperty) {
        // Example validations - add more as needed
        match property {
            CssProperty::Display(val) => {
                // Check for display values
                if val.is_none() {
                    self.warnings.push(CssWarningType::ValueOutOfRange(
                        "display".to_string(),
                        format!("{:?}", val),
                    ));
                }
            }
            CssProperty::MarginLeft(val) => {
                // Check for negative margins that might cause issues
                if let Some(margin) = val.get_property() {
                    if margin.inner.number.get().is_sign_negative() {
                        self.warnings.push(CssWarningType::ValueOutOfRange(
                            format!("{:?}", property),
                            format!("{:?}", margin),
                        ));
                    }
                }
            }
            CssProperty::MarginRight(val) => {
                // Check for negative margins that might cause issues
                if let Some(margin) = val.get_property() {
                    if margin.inner.number.get().is_sign_negative() {
                        self.warnings.push(CssWarningType::ValueOutOfRange(
                            format!("{:?}", property),
                            format!("{:?}", margin),
                        ));
                    }
                }
            }
            CssProperty::MarginTop(val) => {
                // Check for negative margins that might cause issues
                if let Some(margin) = val.get_property() {
                    if margin.inner.number.get().is_sign_negative() {
                        self.warnings.push(CssWarningType::ValueOutOfRange(
                            format!("{:?}", property),
                            format!("{:?}", margin),
                        ));
                    }
                }
            }
            CssProperty::MarginBottom(val) => {
                // Check for negative margins that might cause issues
                if let Some(margin) = val.get_property() {
                    if margin.inner.number.get().is_sign_negative() {
                        self.warnings.push(CssWarningType::ValueOutOfRange(
                            format!("{:?}", property),
                            format!("{:?}", margin),
                        ));
                    }
                }
            }
            // Add more property validations as needed
            _ => {}
        }
    }

    /// Format the warnings as a string for display
    pub fn format_warnings(&self) -> String {
        use std::fmt::Write;
        if self.warnings.is_empty() {
            return "No CSS warnings detected.".to_string();
        }

        let mut output = String::new();
        writeln!(output, "CSS Warnings ({})", self.warnings.len()).unwrap();
        writeln!(output, "===================").unwrap();

        for (i, warning) in self.warnings.iter().enumerate() {
            writeln!(output, "{}. {}", i + 1, warning).unwrap();
        }

        output
    }

    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Return the number of warnings
    pub fn warning_count(&self) -> usize {
        self.warnings.len()
    }
}

/// Simple struct for capturing CSS statistics
pub struct CssStats {
    pub rule_count: usize,
    pub declaration_count: usize,
    pub selectors: Vec<String>,
    pub properties: Vec<String>,
}

impl CssStats {
    /// Analyze CSS and return statistics
    pub fn analyze(css: &Css) -> Self {
        let mut stats = Self {
            rule_count: 0,
            declaration_count: 0,
            selectors: Vec::new(),
            properties: Vec::new(),
        };

        for stylesheet in css.stylesheets.as_ref() {
            for rule in stylesheet.rules.as_ref() {
                stats.rule_count += 1;
                stats.selectors.push(rule.path.to_string());

                for decl in rule.declarations.as_ref() {
                    stats.declaration_count += 1;
                    match decl {
                        CssDeclaration::Static(prop) => {
                            stats.properties.push(format!("{:?}", prop));
                        }
                        CssDeclaration::Dynamic(dynamic) => {
                            stats
                                .properties
                                .push(format!("{:?}", dynamic.default_value));
                        }
                    }
                }
            }
        }

        stats
    }

    /// Format CSS statistics as a string
    pub fn format(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        writeln!(output, "CSS Statistics").unwrap();
        writeln!(output, "==============").unwrap();
        writeln!(output, "Rules: {}", self.rule_count).unwrap();
        writeln!(output, "Declarations: {}", self.declaration_count).unwrap();

        if !self.selectors.is_empty() {
            writeln!(output, "\nSelectors:").unwrap();
            for (i, sel) in self.selectors.iter().enumerate() {
                if i < 10 {
                    // Limit number of selectors shown
                    writeln!(output, "- {}", sel).unwrap();
                }
            }

            if self.selectors.len() > 10 {
                writeln!(output, "... ({} more)", self.selectors.len() - 10).unwrap();
            }
        }

        // Count property types
        let mut property_types = std::collections::HashMap::new();
        for prop in &self.properties {
            let property_type = if let Some(idx) = prop.find('(') {
                &prop[0..idx]
            } else {
                prop
            };

            *property_types.entry(property_type.to_string()).or_insert(0) += 1;
        }

        writeln!(output, "\nProperty Types:").unwrap();
        let mut sorted_types: Vec<_> = property_types.iter().collect();
        sorted_types.sort_by(|a, b| b.1.cmp(a.1));

        for (prop_type, count) in sorted_types {
            writeln!(output, "- {}: {}", prop_type, count).unwrap();
        }

        output
    }
}

/// Contains all debug information for a test
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DebugData {
    // Metadata extracted from the test file
    pub title: String,
    pub assert_content: String,
    pub help_link: String,
    pub flags: String,
    pub author: String,

    // CSS parsing
    pub css_warnings: String,
    pub css_stats: String,

    // XML and DOM structure
    pub parsed_xml: String,
    pub styled_dom: String,

    // Layout information
    pub solved_layout: String,
    pub layout_stats: String,

    // Display list information
    pub display_list: String,

    // Rendering information
    pub render_time_ms: u64,
    pub render_warnings: Vec<String>,
}

impl DebugData {
    /// Create a new debug data collector
    pub fn new() -> Self {
        Default::default()
    }

    /// Format the entire debug data as a string
    pub fn format(&self) -> String {
        use std::fmt::Write;

        let mut output = String::new();

        // Metadata section
        writeln!(output, "# Test Metadata").unwrap();
        writeln!(output, "Title: {}", self.title).unwrap();
        if !self.assert_content.is_empty() {
            writeln!(output, "Assert: {}", self.assert_content).unwrap();
        }
        if !self.help_link.is_empty() {
            writeln!(output, "Spec Link: {}", self.help_link).unwrap();
        }
        if !self.author.is_empty() {
            writeln!(output, "Author: {}", self.author).unwrap();
        }
        if !self.flags.is_empty() {
            writeln!(output, "Flags: {}", self.flags).unwrap();
        }

        // Add all other sections with appropriate headers
        self.add_section(&mut output, "CSS Warnings", &self.css_warnings);
        self.add_section(&mut output, "CSS Statistics", &self.css_stats);
        self.add_section(&mut output, "Parsed XML", &self.parsed_xml);
        self.add_section(&mut output, "Styled DOM", &self.styled_dom);
        self.add_section(&mut output, "Solved Layout", &self.solved_layout);
        self.add_section(&mut output, "Layout Statistics", &self.layout_stats);
        self.add_section(&mut output, "Display List", &self.display_list);

        // Rendering information
        writeln!(output, "\n# Rendering Information").unwrap();
        writeln!(output, "Render time: {} ms", self.render_time_ms).unwrap();

        if !self.render_warnings.is_empty() {
            writeln!(output, "\n## Render Warnings").unwrap();
            for (i, warning) in self.render_warnings.iter().enumerate() {
                writeln!(output, "{}. {}", i + 1, warning).unwrap();
            }
        }

        output
    }

    /// Add a section to the output if it's not empty
    fn add_section(&self, output: &mut String, title: &str, content: &str) {
        use std::fmt::Write;
        if !content.is_empty() {
            writeln!(output, "\n# {}", title).unwrap();
            writeln!(output, "{}", content).unwrap();
        }
    }
}

/// Debug data collector for the reftest runner
pub struct DebugDataCollector {
    pub data: DebugData,
}

impl DebugDataCollector {
    /// Create a new debug data collector
    pub fn new() -> Self {
        Self {
            data: DebugData::new(),
        }
    }

    /// Set metadata for the test
    pub fn set_metadata(
        &mut self,
        title: String,
        assert_content: String,
        help_link: String,
        flags: String,
        author: String,
    ) {
        self.data.title = title;
        self.data.assert_content = assert_content;
        self.data.help_link = help_link;
        self.data.flags = flags;
        self.data.author = author;
    }

    /// Set CSS warnings and stats
    pub fn set_css_debug_info(&mut self, warnings: String, stats: String) {
        self.data.css_warnings = warnings;
        self.data.css_stats = stats;
    }

    /// Set XML and DOM structure
    pub fn set_dom_debug_info(&mut self, parsed_xml: String, styled_dom: String) {
        self.data.parsed_xml = parsed_xml;
        self.data.styled_dom = styled_dom;
    }

    /// Set layout information
    pub fn set_layout_debug_info(&mut self, solved_layout: String, layout_stats: String) {
        self.data.solved_layout = solved_layout;
        self.data.layout_stats = layout_stats;
    }

    /// Set display list information
    pub fn set_display_list_debug_info(&mut self, display_list: String) {
        self.data.display_list = display_list;
    }

    /// Add rendering information
    pub fn set_render_info(&mut self, time_ms: u64, warnings: Vec<String>) {
        self.data.render_time_ms = time_ms;
        self.data.render_warnings = warnings;
    }

    /// Get the formatted debug data
    pub fn get_formatted_data(&self) -> String {
        self.data.format()
    }

    /// Get the debug data
    pub fn get_data(&self) -> DebugData {
        self.data.clone()
    }
}

/// Wrapper around layout solving that captures debug information
pub fn solve_layout_with_debug(
    styled_dom: StyledDom,
    document_id: DocumentId,
    epoch: Epoch,
    fake_window_state: &FullWindowState,
    renderer_resources: &mut RendererResources,
    debug_collector: &mut DebugDataCollector,
) -> Result<LayoutResult, Box<dyn std::error::Error>> {
    use std::fmt::Write;
    // Create resources for layout
    let fc_cache = azul_layout::font::loading::build_font_cache();
    let image_cache = ImageCache::default();
    let callbacks = RenderCallbacks {
        insert_into_active_gl_textures_fn: azul_core::gl::insert_into_active_gl_textures,
        layout_fn: azul_layout::solver2::do_the_layout,
        load_font_fn: azul_layout::font::loading::font_source_get_bytes,
        parse_font_fn: azul_layout::parse_font_fn,
    };

    // Solve the layout
    let mut resource_updates = Vec::new();
    let mut debug = Some(Vec::new());
    let id_namespace = IdNamespace(0);

    // Start timer
    let start = std::time::Instant::now();

    let mut solved_layout = azul_core::display_list::SolvedLayout::new(
        styled_dom,
        epoch,
        &document_id,
        fake_window_state,
        &mut resource_updates,
        id_namespace,
        &image_cache,
        &fc_cache,
        &callbacks,
        renderer_resources,
        DpiScaleFactor {
            inner: FloatValue::new(fake_window_state.size.get_hidpi_factor()),
        },
        &mut debug,
    );

    // End timer
    let elapsed = start.elapsed();

    // Collect layout warnings
    let mut layout_warnings = Vec::new();
    if solved_layout.layout_results.is_empty() {
        layout_warnings.push("Failed to solve layout, using empty layout result".to_string());
    }

    // Capture layout statistics
    let mut layout_stats = String::new();
    writeln!(layout_stats, "Layout Statistics").unwrap();
    writeln!(layout_stats, "=================").unwrap();
    writeln!(layout_stats, "Layout time: {:?}", elapsed).unwrap();
    writeln!(layout_stats, "Resource updates: {}", resource_updates.len()).unwrap();

    if let Some(debug_vec) = &debug {
        writeln!(layout_stats, "Debug events: {}", debug_vec.len()).unwrap();
    }

    // Collect layout information
    if !solved_layout.layout_results.is_empty() {
        let layout_result = &solved_layout.layout_results[0];
        debug_collector.set_layout_debug_info(layout_result.print_layout_rects(true), layout_stats);
    } else {
        debug_collector
            .set_layout_debug_info("No layout results available".to_string(), layout_stats);
    }

    if solved_layout.layout_results.is_empty() {
        Err("No layout results available".into())
    } else {
        Ok(solved_layout.layout_results.remove(0))
    }
}

/// Format the display list for debugging
pub fn format_display_list_for_debug(display_list: &CachedDisplayList) -> String {
    use std::fmt::Write;
    let mut output = String::new();

    writeln!(output, "Display List").unwrap();
    writeln!(output, "=============").unwrap();

    match &display_list.root {
        DisplayListMsg::Frame(frame) => {
            format_frame(&mut output, frame, 0);
        }
        DisplayListMsg::ScrollFrame(scroll_frame) => {
            writeln!(output, "Root: ScrollFrame").unwrap();
            format_frame(&mut output, &scroll_frame.frame, 1);
        }
        DisplayListMsg::IFrame(id, size, origin, cached) => {
            writeln!(
                output,
                "Root: IFrame (id: {:?}, size: {:?}, origin: {:?})",
                id, size, origin
            )
            .unwrap();
            format_display_list_for_debug_internal(&mut output, cached, 1);
        }
    }

    output
}

/// Format the display list for debugging (internal recursive function)
fn format_display_list_for_debug_internal(
    output: &mut String,
    display_list: &CachedDisplayList,
    indent: usize,
) {
    use std::fmt::Write;
    match &display_list.root {
        DisplayListMsg::Frame(frame) => {
            format_frame(output, frame, indent);
        }
        DisplayListMsg::ScrollFrame(scroll_frame) => {
            writeln!(output, "{}ScrollFrame", "  ".repeat(indent)).unwrap();
            format_frame(output, &scroll_frame.frame, indent + 1);
        }
        DisplayListMsg::IFrame(id, size, origin, cached) => {
            writeln!(
                output,
                "{}IFrame (id: {:?}, size: {:?}, origin: {:?})",
                "  ".repeat(indent),
                id,
                size,
                origin
            )
            .unwrap();
            format_display_list_for_debug_internal(output, cached, indent + 1);
        }
    }
}

/// Format a display list frame for debugging
fn format_frame(output: &mut String, frame: &DisplayListFrame, indent: usize) {
    use std::fmt::Write;
    let indent_str = "  ".repeat(indent);

    writeln!(output, "{}Frame (size: {:?})", indent_str, frame.size).unwrap();

    // Print content
    if !frame.content.is_empty() {
        writeln!(output, "{}Content ({})", indent_str, frame.content.len()).unwrap();
        for (i, content) in frame.content.iter().enumerate() {
            if i < 10 {
                // Limit displayed items
                writeln!(output, "{}  {:?}", indent_str, content).unwrap();
            } else {
                writeln!(
                    output,
                    "{}  ... ({} more items)",
                    indent_str,
                    frame.content.len() - 10
                )
                .unwrap();
                break;
            }
        }
    }

    // Print children
    if !frame.children.is_empty() {
        writeln!(output, "{}Children ({})", indent_str, frame.children.len()).unwrap();
        for (i, child) in frame.children.iter().enumerate() {
            if i < 5 {
                // Limit displayed children
                match child {
                    DisplayListMsg::Frame(child_frame) => {
                        format_frame(output, child_frame, indent + 1);
                    }
                    DisplayListMsg::ScrollFrame(scroll_frame) => {
                        writeln!(output, "{}  ScrollFrame", indent_str).unwrap();
                        format_frame(output, &scroll_frame.frame, indent + 2);
                    }
                    DisplayListMsg::IFrame(id, size, origin, cached) => {
                        writeln!(
                            output,
                            "{}  IFrame (id: {:?}, size: {:?}, origin: {:?})",
                            indent_str, id, size, origin
                        )
                        .unwrap();
                    }
                }
            } else {
                writeln!(
                    output,
                    "{}  ... ({} more children)",
                    indent_str,
                    frame.children.len() - 5
                )
                .unwrap();
                break;
            }
        }
    }
}

/// Enhanced test result with debug data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedTestResult {
    // Basic test info
    pub test_name: String,
    pub diff_count: usize,
    pub passed: bool,

    // Metadata from test file
    pub title: String,
    pub assert_content: String,
    pub help_link: String,
    pub flags: String,
    pub author: String,

    // Debug info
    pub css_warnings: String,
    pub parsed_xml: String,
    pub styled_dom: String,
    pub solved_layout: String,
    pub display_list: String,

    // Additional stats
    pub render_time_ms: u64,
}

impl EnhancedTestResult {
    /// Create a new enhanced test result from test name
    pub fn new(test_name: String) -> Self {
        Self {
            test_name,
            diff_count: 0,
            passed: false,
            title: String::new(),
            assert_content: String::new(),
            help_link: String::new(),
            flags: String::new(),
            author: String::new(),
            css_warnings: String::new(),
            parsed_xml: String::new(),
            styled_dom: String::new(),
            solved_layout: String::new(),
            display_list: String::new(),
            render_time_ms: 0,
        }
    }

    /// Create an enhanced test result from debug data
    pub fn from_debug_data(
        test_name: String,
        diff_count: usize,
        passed: bool,
        debug_data: DebugData,
    ) -> Self {
        Self {
            test_name,
            diff_count,
            passed,
            title: debug_data.title,
            assert_content: debug_data.assert_content,
            help_link: debug_data.help_link,
            flags: debug_data.flags,
            author: debug_data.author,
            css_warnings: debug_data.css_warnings,
            parsed_xml: debug_data.parsed_xml,
            styled_dom: debug_data.styled_dom,
            solved_layout: debug_data.solved_layout,
            display_list: debug_data.display_list,
            render_time_ms: debug_data.render_time_ms,
        }
    }
}

// Generate enhanced HTML report
fn generate_enhanced_html_report(
    output_dir: &Path,
    results: &[EnhancedTestResult],
    chrome_version: &str,
    current_time: &str,
    git_hash: &str,
    is_chrome_installed: bool,
) -> Result<(), Box<dyn Error>> {
    let report_path = output_dir.join("report.html");
    let mut file = File::create(&report_path)?;

    // Serialize test data to JSON for JavaScript
    let test_data_json = serde_json::to_string(results)?;

    // Read the HTML template
    let html_template = include_str!("./report_template.html");

    // Add header information
    let chrome_class = if is_chrome_installed {
        "success"
    } else {
        "error"
    };

    // Replace placeholders with actual data
    let html_content = html_template
        .replace("CURRENT_TIME", current_time)
        .replace("CHROME_CLASS", chrome_class)
        .replace("CHROME_VERSION", &chrome_version.replace("Google ", ""))
        .replace("GIT_HASH", git_hash)
        .replace("[]{{test_data_json}}", &test_data_json);

    // Write HTML to file
    file.write_all(html_content.as_bytes())?;

    println!(
        "Enhanced HTML report generated at {}",
        report_path.display()
    );

    Ok(())
}
fn enhance_test_results(results: &[TestResult], test_dir: &Path) -> Vec<EnhancedTestResult> {
    let mut enhanced_results = Vec::new();

    for result in results {
        let test_file = test_dir.join(format!("{}.xht", result.test_name));
        let debug_json =
            PathBuf::from("reftest_output").join(format!("{}_debug.json", result.test_name));

        // Create a new enhanced result with basic test information
        let mut enhanced_result = EnhancedTestResult::new(result.test_name.clone());
        enhanced_result.diff_count = result.diff_count;
        enhanced_result.passed = result.passed;

        // First try to load debug data from the JSON file if it exists
        // This would contain data captured during test execution
        if debug_json.exists() {
            if let Ok(json_content) = std::fs::read_to_string(&debug_json) {
                if let Ok(debug_data) = serde_json::from_str::<DebugData>(&json_content) {
                    // Create enhanced result from the debug data
                    enhanced_result = EnhancedTestResult::from_debug_data(
                        result.test_name.clone(),
                        result.diff_count,
                        result.passed,
                        debug_data,
                    );
                }
            }
        } else {
            // Fall back to extracting what we can from the test file
            if test_file.exists() {
                // Try to parse the test file to extract metadata
                if let Ok((_, metadata, _)) = EnhancedXmlParser::parse_test_file(&test_file) {
                    enhanced_result.title = metadata.title;
                    enhanced_result.assert_content = metadata.assert_content;
                    enhanced_result.help_link = metadata.help_link;
                    enhanced_result.flags = metadata.flags;
                    enhanced_result.author = metadata.author;

                    // Try to get styling information and warnings
                    if let Ok(file_content) = std::fs::read_to_string(&test_file) {
                        // Extract CSS
                        if let Ok(css_text) = extract_css_from_xml(&file_content) {
                            let mut css_collector = CssWarningCollector::new();
                            let parsed_css = css_collector.parse_css(&css_text);
                            let css_stats = CssStats::analyze(&parsed_css);

                            enhanced_result.css_warnings = css_collector.format_warnings();

                            // Parse XML and format for display
                            if let Ok(parsed_nodes) = parse_xml_string(&file_content) {
                                let mut xml_formatted = String::new();
                                for node in &parsed_nodes {
                                    xml_formatted.push_str(
                                        &EnhancedXmlParser::format_xml_for_display(node, 0),
                                    );
                                }
                                enhanced_result.parsed_xml = xml_formatted;
                            }
                        }
                    }
                }
            }
        }

        // Add the enhanced result to our collection
        enhanced_results.push(enhanced_result);
    }

    enhanced_results
}

/// Helper function to extract CSS from an XHTML file
fn extract_css_from_xml(xml_content: &str) -> Result<String, Box<dyn Error>> {
    let mut css = String::new();

    // Simple string-based extraction for efficiency
    if let Some(style_start) = xml_content.find("<style type=\"text/css\">") {
        if let Some(style_end) = xml_content[style_start..].find("</style>") {
            css = xml_content[style_start + 23..style_start + style_end]
                .trim()
                .to_string();
        }
    }

    Ok(css)
}
