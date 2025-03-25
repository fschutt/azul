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
use azul_css::FloatValue;
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
        match generate_azul_rendering(test_file, &azul_png, dpi_factor) {
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

fn generate_azul_rendering(
    test_file: &Path,
    output_file: &Path,
    dpi_factor: f32,
) -> Result<(), Box<dyn Error>> {
    // Read XML content
    let xml_content = fs::read_to_string(test_file)?;

    // Parse XML to StyledDom
    let styled_dom = azul_layout::xml::domxml_from_str(
        &xml_content,
        &mut azul_layout::xml::XmlComponentMap::default(),
    );

    // Generate and save PNG
    styled_dom_to_png(
        &styled_dom.parsed_dom,
        output_file,
        WIDTH,
        HEIGHT,
        dpi_factor,
    )
}

fn styled_dom_to_png(
    styled_dom: &StyledDom,
    output_file: &Path,
    width: u32,
    height: u32,
    dpi_factor: f32,
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
    fake_window_state.size.dpi = (96.0 * dpi_factor) as u32;

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

    println!("{}", layout_result.print_layout_rects(false));

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
    let mut pixmap = Pixmap::new(
        (width as f32 * dpi_factor) as u32,
        (height as f32 * dpi_factor) as u32,
    )
    .ok_or_else(|| format!("cannot create pixmap"))?;
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
