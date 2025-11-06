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

extern "C" fn layout_xhtml(_data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    use std::io::Write;
    eprintln!("[layout_xhtml] CALLED!");
    let _ = std::io::stderr().flush();

    // Create a simple DOM with text and a colored rectangle
    // Body should automatically grow to fit its children (no explicit size needed)
    let mut dom = Dom::body()
        .with_inline_style("padding: 20px; width: 640px; height: 480px;")
        .with_children(
            vec![
                // Text node - wrapped in a div with explicit size
                Dom::div()
                    .with_inline_style(
                        "font-size: 24px; color: #000000; margin-bottom: 20px; width: 400px; \
                         height: 30px;",
                    )
                    .with_children(vec![Dom::text("Azul Display List Test")].into()),
                // Red rectangle
                Dom::div().with_inline_style(
                    "width: 200px; height: 100px; background: #FF0000; margin: 10px;",
                ),
                // Text with inline style
                Dom::div()
                    .with_inline_style(
                        "font-size: 16px; color: #000000; width: 600px; height: 25px;",
                    )
                    .with_children(
                        vec![Dom::text("This is some sample text to test font rendering")].into(),
                    ),
            ]
            .into(),
        );

    eprintln!(
        "[layout_xhtml] Dom created with {} total children",
        dom.estimated_total_children
    );
    eprintln!("[layout_xhtml] Dom node_count: {}", dom.node_count());
    let _ = std::io::stderr().flush();

    let styled = dom.style(CssApiWrapper { css: Css::empty() });
    eprintln!(
        "[layout_xhtml] StyledDom created with {} nodes",
        styled.styled_nodes.len()
    );
    let _ = std::io::stderr().flush();
    styled
}

extern "C" fn on_window_close(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    // Show a native dialog asking if the user really wants to close
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        use azul_core::window::WindowFlags;

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

#[cfg(feature = "desktop")]
fn main() {
    // Load XHTML content using include_bytes!
    let data = XhtmlData {
        xhtml_content: XHTML_BYTES,
    };
    let config = AppConfig::new();
    let app = App::new(RefAny::new(data), config);
    let mut window = WindowCreateOptions::new(layout_xhtml);

    // Set the close callback
    window.state.close_callback =
        azul_layout::callbacks::OptionCallback::Some(azul_layout::callbacks::Callback {
            cb: on_window_close,
        });

    app.run(window);
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("This example requires the 'desktop' feature to be enabled.");
    eprintln!("Run with: cargo run --bin test_scrolling --features desktop");
    std::process::exit(1);
}
