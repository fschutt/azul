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

extern "C" fn layout_xhtml(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    let xhtml_data = match data.downcast_ref::<XhtmlData>() {
        Some(d) => d,
        None => return StyledDom::default(),
    };

    Dom::body()
    .with_children(vec![Dom::text("hello")].into())
    .style(CssApiWrapper { css: Css::empty() })
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
                    let mut flags = WindowFlags::default();
                    flags.close_requested = true;
                    info.set_window_flags(flags);
                } else {
                    // User clicked "No" or closed dialog - prevent closing
                    eprintln!("[Close Callback] User cancelled close");
                    let mut flags = WindowFlags::default();
                    flags.close_requested = false;
                    info.set_window_flags(flags);
                }
            }
            Err(e) => {
                eprintln!("[Close Callback] Failed to show dialog: {}", e);
                // On error, allow closing
                let mut flags = WindowFlags::default();
                flags.close_requested = true;
                info.set_window_flags(flags);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        use azul_core::window::WindowFlags;

        eprintln!("[Close Callback] Close confirmation not implemented for this platform");
        // Allow closing on other platforms
        let mut flags = WindowFlags::default();
        flags.is_about_to_close = true;
        info.set_window_flags(flags);
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
