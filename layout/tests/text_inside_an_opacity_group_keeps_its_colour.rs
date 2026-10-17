//! Text inside an opacity group is drawn in its own colour at the group's
//! opacity - never darker, and without subpixel colour fringes.
//!
//! An opacity group (a dimmed, disabled button; a fading item) is rendered
//! into a layer that starts TRANSPARENT and is then composited through the
//! opacity. The default text path is RGB LCD subpixel AA, which blends each
//! colour stripe against the destination pixel and stamps it opaque - right
//! over an opaque backdrop, wrong over a transparent one: every glyph edge
//! was blended against transparent BLACK and made opaque, so dark text in a
//! dimmed control came out smeared and heavy (AzDrive's disabled ribbon
//! labels in light mode, 2026-10-03). Over a transparent backdrop the run
//! takes grayscale coverage, as browsers draw text in a layer without an
//! opaque background.
//!
//! Rendered through the layered compositor, the path the CPU backends and
//! the debug server's screenshot take.

use std::collections::HashMap;

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, cpurender, glyph_cache::GlyphCache, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: u32 = 220;
const H: u32 = 60;

/// `body(white) > div(style) > "Copy path"`, rendered through the layered
/// compositor: the RGBA pixels of the frame.
fn rendered(style: &str) -> Vec<u8> {
    let dom = Dom::create_body()
        .with_css("margin: 0; padding: 16px; background: #ffffff;")
        .with_child(
            Dom::create_div()
                .with_css(&format!("font-size: 16px; color: #333333; {style}"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Copy path")),
        );
    let styled = StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W as f32, H as f32);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the page lays out");
    let root = lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");

    let mut glyph_cache = GlyphCache::new();
    let render_state = cpurender::CpuRenderState::new(Default::default());
    let mut compositor = cpurender::CompositorState::new(W, H);
    compositor.allocate_layers_from_display_list(
        &root.display_list,
        1.0,
        &HashMap::new(),
        &HashMap::new(),
    );
    compositor
        .render_layers(
            &root.display_list,
            1.0,
            &rr,
            &lw.font_manager,
            &mut glyph_cache,
            &render_state,
        )
        .expect("the layers render");
    let mut out = cpurender::AzulPixmap::new(W, H).expect("a pixmap");
    out.fill(255, 255, 255, 255);
    compositor.composite_frame(&mut out, 1.0);
    out.data().to_vec()
}

/// How many pixels the text darkened (any channel under 240).
fn inked(pixels: &[u8]) -> usize {
    pixels
        .chunks_exact(4)
        .filter(|p| p[0] < 240 || p[1] < 240 || p[2] < 240)
        .count()
}

#[test]
fn text_inside_an_opacity_group_is_never_darker_than_its_colour_at_that_opacity() {
    let pixels = rendered("opacity: 0.5;");
    assert!(inked(&pixels) > 20, "the label is painted");
    // #333333 at 50% over white is #999999 (153): no pixel of the frame can be
    // darker than that, whatever the glyph coverage (2 for rounding).
    let darkest = pixels
        .chunks_exact(4)
        .map(|p| p[0].min(p[1]).min(p[2]))
        .min()
        .expect("the frame has pixels");
    assert!(
        darkest >= 151,
        "the darkest pixel is {darkest}, darker than #333333 at 50% over white (153): the glyph \
         edges were blended against transparent black"
    );
}

#[test]
fn text_inside_an_opacity_group_has_no_subpixel_colour_fringes() {
    let pixels = rendered("opacity: 0.5;");
    assert!(inked(&pixels) > 20, "the label is painted");
    // Grey text on white: every pixel stays grey. LCD stripes over a
    // transparent backdrop leave red and blue fringes.
    let fringe = pixels
        .chunks_exact(4)
        .map(|p| (i16::from(p[0]) - i16::from(p[2])).abs())
        .max()
        .expect("the frame has pixels");
    assert!(
        fringe <= 3,
        "a pixel's red and blue differ by {fringe}: subpixel text over a transparent layer"
    );
}
