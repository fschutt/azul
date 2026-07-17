//! Native discriminator for the 2026-06-11 web-lift trap: the web-events.c
//! DOM (body > 9 divs each containing a text node, no stylesheet) traps with
//! `unreachable` inside `collect_font_stacks_from_styled_dom` when run LIFTED.
//! If this native run of the same shape panics too, it's a real azul bug;
//! if it passes, the lifted run corrupts data upstream of the bounds check
//! (mis-lift) and the hunt moves to the wasm side.

use azul_core::{
    dom::Dom,
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

#[test]
fn web_events_dom_layouts_natively() {
    let labels = [
        "click me",
        "mousedown me",
        "move over me",
        "wheel over me",
        "enter me",
        "leave me",
        "right-click me",
        "resize watcher",
        "keydown watcher",
    ];
    let mut body = Dom::create_body();
    for label in labels {
        let mut div = Dom::create_div();
        div.add_child(Dom::create_text(label));
        body.add_child(div);
    }

    let (css, _) = azul_css::parser2::new_from_str("");
    let styled_dom = StyledDom::create(&mut body, css);
    assert_eq!(styled_dom.node_data.as_ref().len(), 19, "body + 9 divs + 9 texts");

    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
}
