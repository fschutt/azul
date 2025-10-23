//! Debug binary that solves layout and prints the display list
//!
//! This is useful for inspecting what items are being generated and submitted
//! to WebRender without actually rendering them.

use azul_core::{callbacks::LayoutCallbackInfo, dom::Dom, refany::RefAny, styled_dom::StyledDom};
use azul_css::{css::Css, parser2::CssApiWrapper};

struct AppData {
    content: &'static str,
}

extern "C" fn layout_callback(_data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    use std::io::Write;

    eprintln!("\n========================================");
    eprintln!("  DISPLAY LIST DEBUG TEST");
    eprintln!("========================================\n");

    // Create a simple DOM with text and styled rectangles
    let mut dom = Dom::body()
        .with_inline_style("width: 100%; height: 100%; padding: 20px;")
        .with_children(
            vec![
                // Header text - give it explicit size and styling
                Dom::div()
                    .with_inline_style(
                        "font-size: 24px; color: #FFFFFF; margin-bottom: 20px; width: 400px; \
                         height: 30px;",
                    )
                    .with_children(vec![Dom::text("Azul Display List Test")].into()),
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
                // Text with some styling - wrap in a div
                Dom::div()
                    .with_inline_style(
                        "font-size: 16px; color: #FFFFFF; margin-top: 20px; width: 600px; height: \
                         25px;",
                    )
                    .with_children(
                        vec![Dom::text("This is some sample text to test font rendering")].into(),
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

#[cfg(feature = "desktop")]
fn main() {
    use azul_dll::desktop::{app::App, resources::AppConfig};
    use azul_layout::window_state::WindowCreateOptions;

    eprintln!("\nStarting Azul Display List Debug Test...\n");

    let data = AppData { content: "test" };

    let config = AppConfig::new();
    let app = App::new(RefAny::new(data), config);
    let window = WindowCreateOptions::new(layout_callback);

    eprintln!("\nOpening window...\n");

    app.run(window);
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("This example requires the 'desktop' feature to be enabled.");
    eprintln!("Run with: cargo run --bin test_display_list --features desktop");
    std::process::exit(1);
}
