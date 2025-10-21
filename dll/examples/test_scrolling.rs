//! Test binary that loads and displays showcase-flexbox-complex-001.xht
//!
//! This demonstrates:
//! - Loading XHTML content with include_bytes!()
//! - Opening a window with the high-level App API
//! - Testing the wr_translate2.rs implementation

use azul_core::{callbacks::LayoutCallback, dom::Dom, refany::RefAny, styled_dom::StyledDom};
use azul_css::{css::Css, parser2::CssApiWrapper};
use azul_dll::desktop::{app::App, resources::AppConfig};
use azul_layout::window_state::WindowCreateOptions;

const XHTML_BYTES: &str = include_str!("../../doc/working/showcase-flexbox-complex-001.xht");

struct XhtmlData {
    xhtml_content: &'static str,
}

extern "C" fn layout_xhtml(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    let xhtml_data = match data.downcast_ref::<XhtmlData>() {
        Some(d) => d,
        None => return StyledDom::default(),
    };

    Dom::from_xml(xhtml_data.xhtml_content).style(CssApiWrapper { css: Css::empty() })
}

#[cfg(feature = "desktop")]
fn main() {
    // Load XHTML content using include_bytes!
    let data = XhtmlData {
        xhtml_content: XHTML_BYTES,
    };
    let config = AppConfig::new();
    let app = App::new(RefAny::new(data), config);
    let window = WindowCreateOptions::new(layout_xhtml);
    app.run(window);
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("This example requires the 'desktop' feature to be enabled.");
    eprintln!("Run with: cargo run --bin test_scrolling --features desktop");
    std::process::exit(1);
}
