//! Graphics Stress Test
//!
//! This example tests various graphical features:
//! - Linear, Radial, and Conic gradients with rounded corners and box shadows
//! - Bordered boxes with text
//! - CSS filters, backdrop blur, and opacity
//!
//! Run with:
//!   cargo run --bin graphics_stress_test --package azul-dll --features "desktop"

use azul_core::{
    callbacks::{LayoutCallbackInfo, LayoutCallbackType},
    dom::Dom,
    refany::RefAny,
    resources::AppConfig,
    styled_dom::StyledDom,
};
use azul_css::{css::Css, parser2::CssApiWrapper};
use azul_dll::desktop::app::App;
use azul_layout::window_state::WindowCreateOptions;

#[derive(Debug, Clone)]
pub struct StressTestData {
    pub frame_count: u32,
}

pub extern "C" fn stress_test_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    eprintln!("[stress_test_layout] Called!");

    if let Some(model) = data.downcast_ref::<StressTestData>() {
        eprintln!("[stress_test_layout] frame_count = {}", model.frame_count);
    }

    // Build the DOM structure using inline styles
    let mut dom = Dom::create_div()
        .with_inline_style("
            display: flex;
            flex-direction: column;
            width: 100%;
            height: 100%;
            padding: 20px;
            background-color: #1a1a2e;
        ")
        // row 1: gradients with rounded corners and box shadows
        .with_child(
            Dom::create_div()
                .with_inline_style("display: flex; flex-direction: row; margin-bottom: 20px; gap: 20px;")
                // Linear Gradient
                .with_child(
                    Dom::create_div()
                        .with_inline_style("
                            width: 200px;
                            height: 120px;
                            border-radius: 15px;
                            box-shadow: 0px 8px 25px rgba(0, 0, 0, 0.5);
                            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                        ")
                )
                // Radial Gradient
                .with_child(
                    Dom::create_div()
                        .with_inline_style("
                            width: 200px;
                            height: 120px;
                            border-radius: 15px;
                            box-shadow: 0px 8px 25px rgba(0, 0, 0, 0.5);
                            background: radial-gradient(circle at center, #f093fb 0%, #f5576c 100%);
                        ")
                )
                // Conic Gradient
                .with_child(
                    Dom::create_div()
                        .with_inline_style("
                            width: 200px;
                            height: 120px;
                            border-radius: 15px;
                            box-shadow: 0px 8px 25px rgba(0, 0, 0, 0.5);
                            background: conic-gradient(from 0deg, #ff0000, #ff7f00, #ffff00, #00ff00, #0000ff, #9400d3, #ff0000);
                        ")
                )
        )
        // row 2: filter effect boxes
        .with_child(
            Dom::create_div()
                .with_inline_style("display: flex; flex-direction: row; margin-bottom: 20px; gap: 20px;")
                // Grayscale filter
                .with_child(
                    Dom::create_div()
                        .with_inline_style("
                            width: 180px;
                            height: 100px;
                            border-radius: 10px;
                            background-color: #4a90d9;
                            filter: grayscale(100%);
                        ")
                )
                // Backdrop blur (semi-transparent)
                .with_child(
                    Dom::create_div()
                        .with_inline_style("
                            width: 180px;
                            height: 100px;
                            border-radius: 10px;
                            background-color: rgba(255, 255, 255, 0.2);
                            backdrop-filter: blur(10px);
                            border: 1px solid rgba(255, 255, 255, 0.3);
                        ")
                )
                // Opacity
                .with_child(
                    Dom::create_div()
                        .with_inline_style("
                            width: 180px;
                            height: 100px;
                            border-radius: 10px;
                            background-color: #e91e63;
                            opacity: 0.6;
                        ")
                )
        )
        // row 3: bordered boxes with text
        .with_child(
            Dom::create_div()
                .with_inline_style("display: flex; flex-direction: row; margin-bottom: 20px; gap: 20px;")
                // Red bordered box
                .with_child(
                    Dom::create_div()
                        .with_inline_style("
                            width: 180px;
                            height: 100px;
                            border: 3px solid #f44336;
                            border-radius: 10px;
                            background-color: #ffebee;
                            color: #c62828;
                            font-size: 14px;
                            font-weight: bold;
                            display: flex;
                            justify-content: center;
                            align-items: center;
                        ")
                )
                // Green bordered box
                .with_child(
                    Dom::create_div()
                        .with_inline_style("
                            width: 180px;
                            height: 100px;
                            border: 3px solid #4caf50;
                            border-radius: 10px;
                            background-color: #e8f5e9;
                            color: #2e7d32;
                            font-size: 14px;
                            font-weight: bold;
                            display: flex;
                            justify-content: center;
                            align-items: center;
                        ")
                )
                // Blue bordered box
                .with_child(
                    Dom::create_div()
                        .with_inline_style("
                            width: 180px;
                            height: 100px;
                            border: 3px solid #2196f3;
                            border-radius: 10px;
                            background-color: #e3f2fd;
                            color: #1565c0;
                            font-size: 14px;
                            font-weight: bold;
                            display: flex;
                            justify-content: center;
                            align-items: center;
                        ")
                )
        )
        // row 4: shadow cascade
        .with_child(
            Dom::create_div()
                .with_inline_style("display: flex; flex-direction: row; gap: 20px;")
                .with_child(
                    Dom::create_div()
                        .with_inline_style("
                            width: 150px;
                            height: 150px;
                            background: linear-gradient(180deg, #4facfe 0%, #00f2fe 100%);
                            border-radius: 20px;
                            box-shadow: 0px 20px 40px rgba(0, 0, 0, 0.3);
                        ")
                )
        );

    eprintln!("[stress_test_layout] DOM created");

    let styled = dom.style(CssApiWrapper { css: Css::empty() });
    eprintln!(
        "[stress_test_layout] StyledDom has {} nodes",
        styled.styled_nodes.len()
    );
    styled
}

fn main() {
    eprintln!("===========================================");
    eprintln!("    Graphics Stress Test                   ");
    eprintln!("===========================================");
    eprintln!("");
    eprintln!("Testing:");
    eprintln!("  - Linear, Radial, Conic gradients");
    eprintln!("  - Rounded corners (border-radius)");
    eprintln!("  - Box shadows");
    eprintln!("  - Bordered boxes");
    eprintln!("  - CSS filters (grayscale)");
    eprintln!("  - Backdrop blur");
    eprintln!("  - Opacity");
    eprintln!("");

    let model = StressTestData { frame_count: 0 };
    let data = RefAny::new(model);
    let config = AppConfig::create();
    let app = App::create(data, config);

    let mut window = WindowCreateOptions::create(stress_test_layout as LayoutCallbackType);
    window.window_state.title = "Graphics Stress Test".to_string().into();
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 600.0;

    app.run(window);
}
