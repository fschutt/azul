use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Instant,
};

use azul_core::{
    app_resources::{DpiScaleFactor, Epoch, IdNamespace, ImageCache},
    callbacks::DocumentId,
    display_list::{
        CachedDisplayList, DisplayListFrame, DisplayListMsg, DisplayListScrollFrame,
        LayoutRectContent, RectBackground, RenderCallbacks, SolvedLayout, StyleBorderColors,
        StyleBorderStyles, StyleBorderWidths,
    },
    styled_dom::{DomId, StyledDom},
    ui_solver::LayoutResult,
    window::{FullWindowState, LogicalSize},
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
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/xhtml1");
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

    // Get Chrome path
    let chrome_path = get_chrome_path();

    // Verify Chrome installation
    match verify_chrome_installation(&chrome_path) {
        Ok(_) => println!("Chrome installation verified at: {}", chrome_path),
        Err(e) => {
            eprintln!("ERROR: Chrome verification failed: {}", e);
            eprintln!(
                "Please ensure Chrome is installed or set the CHROME environment variable to the \
                 correct path."
            );
            std::process::exit(1);
        }
    }

    // Process tests in parallel
    test_files.par_iter().for_each(|test_file| {
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

        // Generate Azul rendering
        match generate_azul_rendering(test_file, &azul_png) {
            Ok(_) => println!("  Azul rendering generated successfully"),
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

                // Store result
                let mut results_vec = results.lock().unwrap();
                results_vec.push(TestResult {
                    test_name: test_name.to_string(),
                    diff_count,
                    passed,
                });
            }
            Err(e) => {
                println!("  Failed to compare images: {}", e);
            }
        }
    });

    // Get the final results
    let final_results = results.lock().unwrap();
    let passed_tests = final_results.iter().filter(|r| r.passed).count();

    // Generate HTML report
    println!("Generating HTML report");
    generate_html_report(&output_dir, &final_results)?;

    // Generate JSON results
    println!("Generating JSON results");
    generate_json_results(&output_dir, &final_results, passed_tests)?;

    println!(
        "Testing complete. Results saved to {}",
        output_dir.display()
    );
    println!("Passed: {}/{}", passed_tests, final_results.len());

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

fn verify_chrome_installation(chrome_path: &str) -> Result<(), String> {
    // Try running chrome --version to verify it works
    let output = Command::new(chrome_path).arg("--version").output();

    match output {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("Found Chrome: {}", version.trim());
            Ok(())
        }
        Ok(_) => Err(format!("Chrome at '{}' returned error status", chrome_path)),
        Err(e) => Err(format!(
            "Failed to execute Chrome at '{}': {}",
            chrome_path, e
        )),
    }
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

fn generate_azul_rendering(test_file: &Path, output_file: &Path) -> Result<(), Box<dyn Error>> {
    // Read XML content
    let xml_content = fs::read_to_string(test_file)?;

    // Parse XML to StyledDom
    let styled_dom = azul_layout::xml::domxml_from_str(
        &xml_content,
        &mut azul_layout::xml::XmlComponentMap::default(),
    );

    // Generate and save PNG
    styled_dom_to_png(&styled_dom.parsed_dom, output_file, WIDTH, HEIGHT)
}

fn styled_dom_to_png(
    styled_dom: &StyledDom,
    output_file: &Path,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    println!("  Rendering display list to PNG");

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

    // Create resources for layout
    let mut renderer_resources = azul_core::app_resources::RendererResources::default();
    let fc_cache = azul_layout::font::loading::build_font_cache();
    let image_cache = ImageCache::default();

    // Define callbacks for layout
    let callbacks = RenderCallbacks {
        insert_into_active_gl_textures_fn: azul_core::gl::insert_into_active_gl_textures,
        layout_fn: azul_layout::solver2::do_the_layout,
        load_font_fn: azul_layout::font::loading::font_source_get_bytes,
        parse_font_fn: azul_layout::parse_font_fn,
    };

    // Solve layout
    println!("  Solving layout");
    let layout_result = solve_layout(
        styled_dom.clone(),
        document_id,
        epoch,
        &fake_window_state,
        &mut renderer_resources,
    )?;

    // Get the cached display list
    println!("  Getting cached display list");
    let dom_id = DomId { inner: 0 };
    let cached_display_list = LayoutResult::get_cached_display_list(
        &document_id,
        dom_id,
        epoch,
        &[layout_result],
        &fake_window_state,
        &azul_core::app_resources::GlTextureCache::default(),
        &renderer_resources,
        &image_cache,
    );

    // Create a pixmap with a white background
    let mut pixmap = Pixmap::new(width, height).ok_or_else(|| format!("cannot create pixmap"))?;
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    // Render the display list to the pixmap
    println!("  Rendering display list");
    render_display_list(&cached_display_list, &mut pixmap)?;

    // Save the pixmap to a PNG file
    pixmap.save_png(output_file)?;

    println!("  Rendering completed in {:?}", start.elapsed());

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
    let dpi_scale = DpiScaleFactor::new(1.0);

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
        dpi_scale,
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
) -> Result<(), Box<dyn Error>> {
    // Start with root position and identity transform
    let transform = Transform::identity();

    match &display_list.root {
        DisplayListMsg::Frame(frame) => {
            render_frame(frame, pixmap, transform, None)?;
        }
        DisplayListMsg::ScrollFrame(scroll_frame) => {
            render_scroll_frame(scroll_frame, pixmap, transform)?;
        }
        DisplayListMsg::IFrame(_, _, _, cached_dl) => {
            render_display_list(cached_dl, pixmap)?;
        }
    }

    Ok(())
}

fn render_frame(
    frame: &DisplayListFrame,
    pixmap: &mut Pixmap,
    transform: Transform,
    clip_rect: Option<Rect>,
) -> Result<(), Box<dyn Error>> {
    // Calculate the frame rectangle
    let frame_rect = match Rect::from_xywh(0.0, 0.0, frame.size.width, frame.size.height) {
        Some(rect) => rect,
        None => return Ok(()), // Invalid rect dimensions
    };

    // Render the frame content
    for content in &frame.content {
        render_content(content, pixmap, frame_rect, transform, clip_rect)?;
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
                render_frame(child_frame, pixmap, child_transform, clip_rect)?;
            }
            DisplayListMsg::ScrollFrame(scroll_frame) => {
                render_scroll_frame(scroll_frame, pixmap, child_transform)?;
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
                render_display_list(cached_dl, pixmap)?;
            }
        }
    }

    Ok(())
}

fn render_scroll_frame(
    scroll_frame: &DisplayListScrollFrame,
    pixmap: &mut Pixmap,
    transform: Transform,
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
    )?;

    Ok(())
}

fn render_content(
    content: &LayoutRectContent,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    clip_rect: Option<Rect>,
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

fn render_border(
    widths: StyleBorderWidths,
    colors: StyleBorderColors,
    styles: StyleBorderStyles,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    clip_rect: Option<Rect>,
) -> Result<(), Box<dyn Error>> {
    // Simplified border rendering - just draws rectangles for each border side
    let mut paint = Paint::default();

    // Top border
    if let Some(top_width) = widths.top.and_then(|w| w.get_property().cloned()) {
        if let Some(top_color) = colors.top.and_then(|c| c.get_property().cloned()) {
            let border_width = top_width.inner.to_pixels(rect.height());
            if border_width > 0.0 {
                paint.set_color_rgba8(
                    top_color.inner.r,
                    top_color.inner.g,
                    top_color.inner.b,
                    top_color.inner.a,
                );
                if let Some(top_rect) =
                    Rect::from_xywh(rect.x(), rect.y(), rect.width(), border_width)
                {
                    draw_rect_with_clip(pixmap, top_rect, &paint, transform, clip_rect)?;
                }
            }
        }
    }

    // Right border
    if let Some(right_width) = widths.right.and_then(|w| w.get_property().cloned()) {
        if let Some(right_color) = colors.right.and_then(|c| c.get_property().cloned()) {
            let border_width = right_width.inner.to_pixels(rect.width());
            if border_width > 0.0 {
                paint.set_color_rgba8(
                    right_color.inner.r,
                    right_color.inner.g,
                    right_color.inner.b,
                    right_color.inner.a,
                );
                if let Some(right_rect) = Rect::from_xywh(
                    rect.x() + rect.width() - border_width,
                    rect.y(),
                    border_width,
                    rect.height(),
                ) {
                    draw_rect_with_clip(pixmap, right_rect, &paint, transform, clip_rect)?;
                }
            }
        }
    }

    // Bottom border
    if let Some(bottom_width) = widths.bottom.and_then(|w| w.get_property().cloned()) {
        if let Some(bottom_color) = colors.bottom.and_then(|c| c.get_property().cloned()) {
            let border_width = bottom_width.inner.to_pixels(rect.height());
            if border_width > 0.0 {
                paint.set_color_rgba8(
                    bottom_color.inner.r,
                    bottom_color.inner.g,
                    bottom_color.inner.b,
                    bottom_color.inner.a,
                );
                if let Some(bottom_rect) = Rect::from_xywh(
                    rect.x(),
                    rect.y() + rect.height() - border_width,
                    rect.width(),
                    border_width,
                ) {
                    draw_rect_with_clip(pixmap, bottom_rect, &paint, transform, clip_rect)?;
                }
            }
        }
    }

    // Left border
    if let Some(left_width) = widths.left.and_then(|w| w.get_property().cloned()) {
        if let Some(left_color) = colors.left.and_then(|c| c.get_property().cloned()) {
            let border_width = left_width.inner.to_pixels(rect.width());
            if border_width > 0.0 {
                paint.set_color_rgba8(
                    left_color.inner.r,
                    left_color.inner.g,
                    left_color.inner.b,
                    left_color.inner.a,
                );
                if let Some(left_rect) =
                    Rect::from_xywh(rect.x(), rect.y(), border_width, rect.height())
                {
                    draw_rect_with_clip(pixmap, left_rect, &paint, transform, clip_rect)?;
                }
            }
        }
    }

    Ok(())
}

fn render_text(
    glyphs: &[azul_core::display_list::GlyphInstance],
    font_instance_key: azul_core::app_resources::FontInstanceKey,
    color: azul_css::ColorU,
    pixmap: &mut Pixmap,
    rect: Rect,
    transform: Transform,
    clip_rect: Option<Rect>,
) -> Result<(), Box<dyn Error>> {
    // Simplified text rendering - this is a placeholder that draws a colored rectangle
    // In a real implementation, you'd use a font rendering library to draw each glyph

    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);

    // Draw a thin line where the text baseline would be
    if let Some(text_rect) =
        Rect::from_xywh(rect.x(), rect.y() + rect.height() * 0.75, rect.width(), 1.0)
    {
        draw_rect_with_clip(pixmap, text_rect, &paint, transform, clip_rect)?;
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
    let chrome_img = image::open(chrome_png)?.to_rgba8();
    let azul_img = image::open(azul_png)?.to_rgba8();

    let width = chrome_img.width() as usize;
    let height = chrome_img.height() as usize;

    // Check image dimensions
    if width != azul_img.width() as usize || height != azul_img.height() as usize {
        return Err(format!(
            "Image dimensions don't match: Chrome: {}x{}, Azul: {}x{}",
            width,
            height,
            azul_img.width(),
            azul_img.height()
        )
        .into());
    }

    // Create diff image buffer
    let mut diff_img: RgbaImage = ImageBuffer::new(width as u32, height as u32);

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

    // Compare images
    let diff_count = pixelmatch::pixelmatch(
        &chrome_img.as_raw()[..],
        &azul_img.as_raw()[..],
        Some(&mut diff_img.as_mut()),
        Some(width as u32),
        Some(height as u32),
        Some(options),
    )?;

    // Save the diff image
    diff_img.save(diff_png)?;

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
