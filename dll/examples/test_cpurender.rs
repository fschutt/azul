//! CPU rendering test - renders the display list directly to a PNG file
//!
//! This example uses the same layout as test_display_list but renders
//! using the CPU renderer instead of WebRender. This is useful for:
//! - Debugging layout positioning without GPU complexity
//! - Testing font loading independently
//! - Verifying display list generation

use azul_core::{callbacks::LayoutCallbackInfo, dom::Dom, refany::RefAny, styled_dom::StyledDom};
use azul_css::{css::Css, parser2::CssApiWrapper, system::SystemStyle};

struct AppData {
    content: &'static str,
}

extern "C" fn layout_callback(_data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    use std::io::Write;

    eprintln!("\n========================================");
    eprintln!("  CPU RENDER TEST");
    eprintln!("========================================\n");

    // Create the same DOM as test_display_list
    let mut dom = Dom::body()
        .with_inline_style("width: 100%; height: 100%; padding: 20px;")
        .with_children(
            vec![
                // Header text
                Dom::div()
                    .with_inline_style(
                        "font-size: 24px; color: #000000; margin-bottom: 20px; width: 400px; \
                         height: 30px;",
                    )
                    .with_children(vec![Dom::text("CPU Render Test")].into()),
                // Red rectangle
                Dom::div().with_inline_style(
                    "width: 200px; height: 100px; background: #FF0000; margin: 10px; \
                     border-radius: 10px;",
                ),
                // Blue rectangle with border
                Dom::div().with_inline_style(
                    "width: 200px; height: 100px; background: #0000FF; margin: 10px; border: 5px \
                     solid #FFFFFF; border-radius: 5px;",
                ),
                // Green rectangle
                Dom::div().with_inline_style(
                    "width: 200px; height: 100px; background: #00FF00; margin: 10px;",
                ),
                // Sample text
                Dom::div()
                    .with_inline_style(
                        "font-size: 16px; color: #000000; margin-top: 20px; width: 600px; height: \
                         25px;",
                    )
                    .with_children(
                        vec![Dom::text("This is sample text to test positioning")].into(),
                    ),
            ]
            .into(),
        );

    eprintln!(
        "[layout_callback] Dom created with {} nodes",
        dom.node_count()
    );
    let _ = std::io::stderr().flush();

    let styled = dom.style(CssApiWrapper { css: Css::empty() });
    eprintln!(
        "[layout_callback] StyledDom created with {} nodes",
        styled.styled_nodes.len()
    );
    let _ = std::io::stderr().flush();

    styled
}

fn main() {
    use std::sync::Arc;

    use azul_core::{resources::RendererResources, window::WindowSize};
    use azul_layout::{
        cpurender::{render, RenderOptions},
        window::LayoutWindow,
    };
    use rust_fontconfig::FcFontCache;

    eprintln!("\n=== CPU Render Test ===\n");

    // Build font cache
    eprintln!("[main] Building font cache...");
    let fc_cache = Arc::new(FcFontCache::build());
    eprintln!("[main] Font cache built");

    // Create LayoutWindow
    eprintln!("[main] Creating LayoutWindow...");
    let mut layout_window =
        LayoutWindow::new((*fc_cache).clone()).expect("Failed to create LayoutWindow");

    // Set up window size (640x480 like in test_display_list)
    let size = WindowSize {
        dimensions: azul_core::geom::LogicalSize::new(640.0, 480.0),
        dpi: 96,
        min_dimensions: azul_core::geom::OptionLogicalSize::None,
        max_dimensions: azul_core::geom::OptionLogicalSize::None,
    };

    layout_window.current_window_state.size = size.clone();

    // Call layout callback to get styled DOM
    eprintln!("[main] Calling layout callback...");
    let mut app_data = RefAny::new(AppData { content: "test" });
    let mut callback_info = LayoutCallbackInfo::new(
        size,
        azul_core::window::WindowTheme::LightMode,
        &azul_core::resources::ImageCache::default(),
        &azul_core::gl::OptionGlContextPtr::None,
        &*fc_cache,
        std::sync::Arc::new(SystemStyle::default()),
    );

    let styled_dom = layout_callback(&mut app_data, &mut callback_info);

    eprintln!(
        "[main] StyledDom has {} nodes",
        styled_dom.styled_nodes.len()
    );

    // Perform layout
    eprintln!("[main] Performing layout...");
    if let Err(e) = layout_window.layout_and_generate_display_list(
        styled_dom,
        &layout_window.current_window_state.clone(),
        &RendererResources::default(),
        &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
        &mut None,
    ) {
        eprintln!("[main] ERROR: Layout failed: {:?}", e);
        std::process::exit(1);
    }

    eprintln!("[main] Layout completed successfully");

    // Get the display list for DOM 0
    if let Some(layout_result) = layout_window
        .layout_results
        .get(&azul_core::dom::DomId::ROOT_ID)
    {
        let display_list = &layout_result.display_list;
        eprintln!("[main] Display list has {} items", display_list.items.len());

        // Print display list for debugging
        eprintln!("\n=== Display List Items ===");
        for (idx, item) in display_list.items.iter().enumerate() {
            eprintln!("  Item {}: {:?}", idx + 1, item);
        }
        eprintln!("===========================\n");

        // Render to pixmap
        eprintln!("[main] Rendering to pixmap...");
        let pixmap = render(
            display_list,
            &RendererResources::default(),
            RenderOptions {
                width: 640.0,
                height: 480.0,
                dpi_factor: 1.0,
            },
        )
        .expect("Failed to render");

        // Save to PNG
        let output_path = "test_cpurender_output.png";
        eprintln!("[main] Saving to {}...", output_path);
        pixmap.save_png(output_path).expect("Failed to save PNG");

        eprintln!("[main] ✓ Success! Output saved to {}", output_path);
    } else {
        eprintln!("[main] ERROR: No layout result for DOM 0");
        std::process::exit(1);
    }
}
