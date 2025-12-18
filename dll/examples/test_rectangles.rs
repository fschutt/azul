//! Test binary that loads and displays showcase-flexbox-complex-001.xht
//!
//! This demonstrates:
//! - Loading XHTML content with include_bytes!()
//! - Opening a window with the high-level App API
//! - Testing the wr_translate2.rs implementation
//! - Handling window close events with a confirmation dialog

use azul_core::{
    callbacks::{LayoutCallbackInfo, Update},
    dom::Dom,
    refany::RefAny,
    styled_dom::StyledDom,
};
use azul_css::{css::Css, parser2::CssApiWrapper};
use azul_dll::desktop::{app::App, resources::AppConfig};
use azul_layout::{callbacks::CallbackInfo, window_state::WindowCreateOptions};

const XHTML_BYTES: &str = include_str!("../../doc/working/showcase-flexbox-complex-001.xht");

struct XhtmlData {
    xhtml_content: &'static str,
}

extern "C" fn layout_xhtml(_data: RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    use std::io::Write;
    eprintln!("[layout_rectangles] CALLED - NO TEXT VERSION!");
    let _ = std::io::stderr().flush();

    // Create a simple DOM with ONLY colored rectangles - NO TEXT!
    let mut dom = Dom::body()
        .with_inline_style("padding: 20px; width: 640px; height: 480px; background: #f0f0f0;")
        .with_children(
            vec![
                // Red rectangle with border
                Dom::div().with_inline_style(
                    "width: 200px; height: 100px; background: #FF0000; border: 2px solid #990000; \
                     margin-bottom: 20px;",
                ),
                // Green rectangle with border
                Dom::div().with_inline_style(
                    "width: 300px; height: 80px; background: #00FF00; border: 2px solid #009900; \
                     margin-bottom: 20px;",
                ),
                // Blue rectangle with border
                Dom::div().with_inline_style(
                    "width: 250px; height: 120px; background: #0000FF; border: 2px solid #000099;",
                ),
            ]
            .into(),
        );

    eprintln!(
        "[layout_rectangles] Dom created with {} total children (NO TEXT)",
        dom.estimated_total_children
    );
    eprintln!("[layout_rectangles] Dom node_count: {}", dom.node_count());
    let _ = std::io::stderr().flush();

    let styled = dom.style(CssApiWrapper { css: Css::empty() });
    eprintln!(
        "[layout_rectangles] StyledDom created with {} nodes",
        styled.styled_nodes.len()
    );
    let _ = std::io::stderr().flush();
    styled
}

extern "C" fn on_window_close(_data: RefAny, mut info: CallbackInfo) -> Update {
    // Show a native dialog asking if the user really wants to close
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        // Use osascript to show a native dialog
        let output = Command::new("osascript")
            .arg("-e")
            .arg(r#"display dialog "Do you really want to close the window?" buttons {"No", "Yes"} default button "No""#)
            .output();

        match output {
            Ok(result) => {
                if result.status.success() {
                    // User clicked "Yes" - allow closing
                    eprintln!("[Close Callback] User confirmed close");
                    let mut state = info.get_current_window_state().clone();
                    state.flags.close_requested = true;
                    info.modify_window_state(state);
                } else {
                    // User clicked "No" or closed dialog - prevent closing
                    eprintln!("[Close Callback] User cancelled close");
                    let mut state = info.get_current_window_state().clone();
                    state.flags.close_requested = false;
                    info.modify_window_state(state);
                }
            }
            Err(e) => {
                eprintln!("[Close Callback] Failed to show dialog: {}", e);
                // On error, allow closing
                let mut state = info.get_current_window_state().clone();
                state.flags.close_requested = true;
                info.modify_window_state(state);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("[Close Callback] Close confirmation not implemented for this platform");
        // Allow closing on other platforms
        let mut state = info.get_current_window_state().clone();
        state.flags.close_requested = true;
        info.modify_window_state(state);
    }

    Update::DoNothing
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("Rectangle Rendering Test (NO TEXT)\n");

    // Load XHTML content using include_bytes!
    let data = XhtmlData {
        xhtml_content: "", // Not used
    };
    let config = AppConfig::new();
    let app = App::new(RefAny::new(data), config);
    let mut window = WindowCreateOptions::new(layout_xhtml as azul_core::callbacks::LayoutCallbackType);

    // Set the close callback
    window.state.close_callback =
        azul_layout::callbacks::OptionCallback::Some(azul_layout::callbacks::Callback {
            cb: on_window_close,
            callable: azul_core::refany::OptionRefAny::None,
        });

    app.run(window);
}

#[cfg(target_arch = "wasm32")]
fn main() {
    eprintln!("This example is not supported on wasm32.");
}
