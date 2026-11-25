//! Two runs that differ in any coordinate of the font - the stretch, a `font-variation-settings`
//! value - never share a font chain or a shaping.
//!
//! The request is part of the chain key, and the style that keys the shaping cache and the
//! logical-item cache hashes the font stack, so the request: two paragraphs of one document, set
//! in one family at `wdth` 75 and 125, are two chains and are laid out at two widths.
//!
//! `StyleProperties::font_variations` was dead: nothing filled it, so no style differed by it.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    text3::cache::FontChainKeyOrRef, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

use crate::variable_fonts::{
    cache_with_file, chain_of, mock_variable_path, page, resolve,
};

/// One block of "DDDD" per entry of `css`, all in the family.
fn paragraphs(family: &str, css: &[&str]) -> Dom {
    let mut body = Dom::create_body().with_css("margin: 0px;");
    for style in css {
        body = body.with_child(
            Dom::create_div()
                .with_css(&format!("font-family: \"{family}\"; font-size: 16px; {style}"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("DDDD")),
        );
    }
    body
}

/// The distance between the first and the last glyph of each text item of `dom`, in px, laid out
/// in a window that holds the fixture as a face registered at runtime.
fn spans(dom: Dom) -> Vec<f32> {
    let bytes = std::fs::read(crate::variable_fonts::mock_variable_path()).expect("the fixture");
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    assert!(
        lw.font_manager
            .registered_fonts
            .register_font("Azul Mock Variable", &bytes),
        "the fixture parses and is registered"
    );
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
    .expect("the page lays out");
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } if glyphs.len() > 1 => {
                Some(glyphs.last()?.point.x - glyphs.first()?.point.x)
            }
            _ => None,
        })
        .collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

/// "DDDD" is three advances from the first glyph to the last: D = 500 + 4 * (wdth - 100) units
/// of 1000 per em, so at 16px it is 3 * 0.4 * 16 = 19.2px at `wdth` 75 and 28.8px at 125.
#[test]
fn two_stretches_in_one_document_are_laid_out_at_two_widths() {
    let spans = spans(paragraphs(
        "Azul Mock Variable",
        &["font-stretch: 75%", "font-stretch: 125%", "font-stretch: 100%"],
    ));
    assert_eq!(spans.len(), 3, "three paragraphs, three text items: {spans:?}");
    assert!(close(spans[0], 19.2), "wdth 75: {spans:?}");
    assert!(close(spans[1], 28.8), "wdth 125: {spans:?}");
    assert!(close(spans[2], 24.0), "wdth 100: {spans:?}");
}

#[test]
fn two_variation_settings_in_one_document_are_laid_out_at_two_widths() {
    let spans = spans(paragraphs(
        "Azul Mock Variable",
        &[
            "font-variation-settings: \"wdth\" 75",
            "font-variation-settings: \"wdth\" 125",
        ],
    ));
    assert_eq!(spans.len(), 2, "two paragraphs, two text items: {spans:?}");
    assert!(close(spans[0], 19.2), "wdth 75: {spans:?}");
    assert!(close(spans[1], 28.8), "wdth 125: {spans:?}");
}

/// The keys of the chains tell the two requests apart.
#[test]
fn requests_that_differ_only_in_one_coordinate_have_different_chain_keys() {
    let (cache, family, _source, _file) = cache_with_file(&mock_variable_path());
    let keys_of = |css: &[&str]| -> usize {
        let styled = StyledDom::create_from_dom(paragraphs(&family, css));
        resolve(&styled, &cache)
            .chains
            .keys()
            .filter(|key| match key {
                FontChainKeyOrRef::Chain(key) => key.font_families.first() == Some(&family),
                FontChainKeyOrRef::Ref(_) => false,
            })
            .count()
    };
    assert_eq!(keys_of(&["font-stretch: 100%", "font-stretch: 100%"]), 1, "equal requests: one chain");
    assert_eq!(keys_of(&["font-stretch: 75%", "font-stretch: 100%"]), 2, "two stretches");
    assert_eq!(
        keys_of(&[
            "font-variation-settings: \"wdth\" 75",
            "font-variation-settings: \"wdth\" 76"
        ]),
        2,
        "two settings"
    );
    assert_eq!(
        keys_of(&[
            "font-variation-settings: \"wdth\" 75, \"wght\" 500",
            "font-variation-settings: \"wght\" 500, \"wdth\" 75"
        ]),
        1,
        "the same settings in another order are one request"
    );
    let _ = (page, chain_of);
}
