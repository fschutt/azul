//! A8, text.
//!
//! Layouts through a window (`LayoutWindow`, the product path) over an EMPTY font cache: the
//! only faces are the built-in mock fonts every font manager registers (`text3::mock_fonts`:
//! `Azul Mock Mono`, 0.5 em a glyph, ascent 0.8 em, descent 0.2 em, no line gap), so every
//! number below is arithmetic.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};

/// The used size of the box of DOM node `node` once `dom` is laid out in a new window of
/// 640 x 480 without system fonts.
fn size_of_node(dom: Dom, node: usize) -> LogicalSize {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let lr = &lw.layout_results[&DomId::ROOT_ID];
    let index = *lr
        .layout_tree
        .dom_to_layout
        .get(&NodeId::new(node))
        .and_then(|v| v.first())
        .expect("the node is laid out");
    lr.layout_tree
        .get(index)
        .and_then(|n| n.used_size)
        .expect("the node has a size")
}

/// `body(0) > div(1) > "Hi"(2)`: an absolutely positioned box (as wide as its text, as high as
/// its one line) in `Azul Mock Mono` at `font-size: <font_size>`.
fn a_box_of_two_glyphs(font_size: &str) -> Dom {
    Dom::create_body()
        .with_css("margin: 0px; font-family: \"Azul Mock Mono\";")
        .with_child(
            Dom::create_div_with_text("Hi").with_css(&format!(
                "position: absolute; left: 0px; top: 0px; font-size: {font_size};"
            )),
        )
}

/// A8-39. A font size in an absolute unit other than px is laid out at its px: the text is
/// shaped at that size and the line box is the one of the same size given in px.
///
/// `core/src/compact.rs` `resolve_font_size_to_px_against` kept the bare number of `in`, `cm`
/// and `mm` in the compact slot (`font-size: 1in` = 1 px), which is the font size of the text's
/// style: the glyphs were 1 px high. The cascade's computed value was 96 px, so the layout's
/// `em` lengths, the font collector and - when its font chain was not found under the 1 px
/// key - the strut used 96 px: one box, two font sizes.
///
/// Hand-computed (1in = 96 px; `Azul Mock Mono`: 0.5 em a glyph, 0.8 em above and 0.2 em
/// below the baseline, no line gap): 1in = 72pt = 2.54cm = 25.4mm = 6pc = 101.6Q = 96 px. "Hi"
/// is 2 x 0.5 x 96 = 96 px wide; its line is round(76.8) + round(19.2) = 77 + 19 = 96 px high.
/// Before: 2 x 0.5 x 1 = 1 px wide for `1in`, 2.54 px for `2.54cm`, 25.4 px for `25.4mm`; `6pc`
/// and `101.6Q` were dropped as no length (16 px text: 16 px wide).
///
/// Mutation: the absolute arm of `resolve_font_size_to_px_against` as `pv.number.get()` makes
/// the `1in` box 1 px wide.
#[test]
fn a_font_size_in_an_absolute_unit_is_laid_out_as_its_px() {
    let in_px = size_of_node(a_box_of_two_glyphs("96px"), 1);
    assert!(
        (in_px.width - 96.0).abs() < 0.5 && (in_px.height - 96.0).abs() < 0.5,
        "harness: two glyphs of 0.5 em at 96 px are 96 px wide on a line of 96 px, not {} x {}",
        in_px.width,
        in_px.height
    );
    for unit in ["1in", "72pt", "2.54cm", "25.4mm", "6pc", "101.6Q"] {
        let size = size_of_node(a_box_of_two_glyphs(unit), 1);
        assert!(
            (size.width - in_px.width).abs() < 0.01,
            "`font-size: {unit}` is 96 px: its two glyphs are {} px wide, those of `96px` {}",
            size.width,
            in_px.width
        );
        assert!(
            (size.height - in_px.height).abs() < 0.01,
            "`font-size: {unit}` is 96 px: its line box is {} px high, that of `96px` {}",
            size.height,
            in_px.height
        );
    }
}
