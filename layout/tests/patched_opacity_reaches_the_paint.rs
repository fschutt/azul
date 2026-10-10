//! A node the app reveals by patching `opacity` is actually painted.
//!
//! The tooltip widget hides its tip with `opacity: 0` and reveals it from a
//! `MouseEnter` callback with `set_css_property`. That write lands in
//! `user_overridden_properties`, which the SLOW cascade walk consults first -
//! so every query answers "opacity: 1" - but the display-list builder reads
//! `get_opacity`, whose fast path is the COMPACT CACHE. `restyle_user_property`
//! rebuilt that cache only for properties that trigger relayout or inherit,
//! and `opacity` does neither, so the cache kept serving the pre-patch value
//! and the tip never drew. The same hole covers every property the compact
//! cache serves directly or by a presence bit: border colours and radii,
//! `background`, `box-shadow`, `transform`, `clip-path`, `text-decoration`.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    property::CssProperty,
    style::StyleOpacity,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
    overlay::ContentChange,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: f32 = 120.0;
const H: f32 = 80.0;

/// How many pixels of the patched node's red fill made it onto the frame.
fn red_pixels(lw: &LayoutWindow, rr: &RendererResources) -> usize {
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .unwrap()
        .display_list
        .clone();
    let mut glyph_cache = GlyphCache::new();
    let opts = RenderOptions {
        width: W,
        height: H,
        dpi_factor: 1.0,
    };
    let pixmap =
        cpurender::render_with_font_manager(&dl, rr, &lw.font_manager, opts, &mut glyph_cache)
            .unwrap();
    pixmap
        .data()
        .chunks_exact(4)
        .filter(|px| px[0] > 200 && px[1] < 80 && px[2] < 80)
        .count()
}

#[test]
fn a_node_revealed_by_patching_opacity_is_painted() {
    let dom = Dom::create_body()
        .with_css("margin: 0; background-color: #ffffff;")
        .with_children(
            vec![Dom::create_div()
                .with_css("width: 60px; height: 40px; background-color: #ff0000; opacity: 0;")]
            .into(),
        );
    let styled = StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .unwrap();

    assert_eq!(
        red_pixels(&lw, &rr),
        0,
        "premise: at `opacity: 0` the div contributes nothing"
    );

    // Exactly what `Tooltip`'s MouseEnter handler does, through the same
    // content chokepoint the shells and the E2E runner use.
    lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: NodeId::new(1),
        props: vec![CssProperty::const_opacity(StyleOpacity::const_new(100))],
        override_only: false,
    });

    assert_eq!(
        red_pixels(&lw, &rr),
        60 * 40,
        "the patched node must paint its whole 60x40 fill"
    );
}
