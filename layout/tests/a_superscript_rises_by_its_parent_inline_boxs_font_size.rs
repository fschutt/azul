//! A superscript rises, and a subscript drops, by the font size of the box
//! it sits in (Chrome, LayoutNG: the parent's font size / 3 + 1px up, / 5 +
//! 1px down): a `<sup>` in `<small>` text rises by the small text's size, not
//! the paragraph's (wave-9 N.2). A `<sup>` straight in the paragraph rises
//! by the paragraph's.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The baselines of the glyph runs of `<p>` holding `x` and then `inner`,
/// in paint order (the first is the paragraph's own `x`).
fn baselines(inner: Dom) -> Vec<f32> {
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_p()
            .with_css(
                "margin: 0px; font-family: \"EB Garamond\"; font-size: 30px; line-height: 60px;",
            )
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("x"))
            .with_child(inner),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the paragraph lays out");
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => glyphs.first().map(|g| g.point.y),
            _ => None,
        })
        .collect()
}

#[test]
fn a_superscript_in_small_text_rises_by_the_small_texts_font_size() {
    let runs = baselines(
        Dom::create_node(azul_core::dom::NodeType::Small)
            .with_css("font-size: 12px;")
            .with_child(
                Dom::create_node(azul_core::dom::NodeType::Sup)
                    .with_css("font-size: 12px;")
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("2")),
            ),
    );
    assert!(runs.len() >= 2, "the x and the superscript paint: {runs:?}");
    let raise = runs[0] - runs[runs.len() - 1];
    assert!(
        (raise - 5.0).abs() < 0.6,
        "12px / 3 + 1px = 5px above the line's baseline, not 30px / 3 + 1px = 11px: {raise}"
    );
}

#[test]
fn a_superscript_straight_in_the_paragraph_rises_by_the_paragraphs_font_size() {
    let runs = baselines(
        Dom::create_node(azul_core::dom::NodeType::Sup)
            .with_css("font-size: 12px;")
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("2")),
    );
    assert!(runs.len() >= 2, "the x and the superscript paint: {runs:?}");
    let raise = runs[0] - runs[runs.len() - 1];
    assert!(
        (raise - 11.0).abs() < 0.6,
        "30px / 3 + 1px = 11px above the line's baseline: {raise}"
    );
}
