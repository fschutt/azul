//! Small capitals are synthesized the way Chrome synthesizes them when the
//! font has none.
//!
//! Calmet UI and EB Garamond have no `smcp`, and AzDrive sets its capitals
//! (tab buttons, group titles, caps headers) in `font-variant-caps:
//! all-small-caps`. Chrome then draws a capital at 0.7 of the font size for
//! the letter: under `small-caps` for every lowercase letter (capitals,
//! figures and marks keep the full size), under `all-small-caps` for every
//! character. The tracking stays the element's (an `em` of the full size).
//!
//! The widths are Chrome's for the same CSS and the same font (headless
//! Chrome 155, Calmet UI at 20px, an inline-block's width).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Calmet UI (the `azul-fonts-calmet` crate AzDrive links), registered the
/// way an app registers its fonts at startup: `AppConfig::add_font` per
/// face, then the window's font manager takes them in.
fn register_calmet(lw: &mut LayoutWindow) {
    let mut config = azul_core::resources::AppConfig::create();
    for face in azul_fonts_calmet::faces() {
        config.add_font(azul_fonts_calmet::FAMILY.into(), face.to_vec().into());
    }
    lw.font_manager.register_app_fonts(config.bundled_fonts.as_ref());
}

/// What a laid-out span shows: every glyph (index, the font size it is
/// painted at) and the span's width.
struct Shown {
    glyphs: Vec<(u32, f32)>,
    width: f32,
}

/// `text` in an inline-block span in Calmet UI 20px under `css`.
fn shown(css: &str, text: &str) -> Shown {
    // body (0) > div (1) > span (2) > text (3)
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css("margin: 0px; font-family: \"Calmet UI\"; font-size: 20px;")
            .with_child(
                Dom::create_div()
                    .with_css(&format!("display: inline-block; white-space: nowrap; {css}"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
            ),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    register_calmet(&mut lw);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the span lays out");
    let width = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
        })
        .expect("the span is laid out")
        .size
        .width;
    let glyphs = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .items
        .iter()
        .flat_map(|item| match item {
            DisplayListItem::Text {
                glyphs,
                font_size_px,
                ..
            } => glyphs.iter().map(|g| (g.index, *font_size_px)).collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect();
    Shown { glyphs, width }
}

fn ids(s: &Shown) -> Vec<u32> {
    s.glyphs.iter().map(|g| g.0).collect()
}

fn sizes(s: &Shown) -> Vec<f32> {
    s.glyphs.iter().map(|g| g.1).collect()
}

fn assert_width(css: &str, text: &str, chrome: f32) {
    let got = shown(css, text).width;
    assert!(
        (got - chrome).abs() < 0.05,
        "{css} {text:?}: Chrome lays it out {chrome:.3}px wide, azul {got:.3}px"
    );
}

#[test]
fn premise_plain_capitals_and_letters_are_as_wide_as_in_chrome() {
    assert_width("", "ABCxyz", 67.328);
    assert_width("", "AB12", 45.469);
}

#[test]
fn small_caps_draws_a_lowercase_letter_as_a_capital_at_seven_tenths_of_the_size() {
    let caps = shown("font-variant-caps: small-caps;", "ABCxyz");
    assert_eq!(ids(&caps), ids(&shown("", "ABCXYZ")), "x y z drawn as the capitals X Y Z");
    assert_eq!(sizes(&caps), [20.0, 20.0, 20.0, 14.0, 14.0, 14.0]);
    assert_width("font-variant-caps: small-caps;", "ABCxyz", 70.016);
}

#[test]
fn small_caps_keeps_capitals_figures_and_marks_at_the_full_size() {
    let caps = shown("font-variant-caps: small-caps;", "Ab.");
    assert_eq!(sizes(&caps), [20.0, 14.0, 20.0]);
    assert_width("font-variant-caps: small-caps;", "Ab.", 27.359);
}

#[test]
fn all_small_caps_draws_every_character_at_seven_tenths_of_the_size() {
    let caps = shown("font-variant-caps: all-small-caps;", "ABCxyz");
    assert_eq!(ids(&caps), ids(&shown("", "ABCXYZ")));
    assert_eq!(sizes(&caps), [14.0; 6]);
    assert_width("font-variant-caps: all-small-caps;", "ABCxyz", 58.141);
    assert_width("font-variant-caps: all-small-caps;", "AB12", 31.828);
}

#[test]
fn a_small_capital_is_tracked_by_the_elements_letter_spacing() {
    assert_width("font-variant-caps: small-caps; letter-spacing: 2px;", "Ab", 27.406);
    // 0.1em of the element's 20px, not of the 14px capital.
    assert_width("font-variant-caps: small-caps; letter-spacing: 0.1em;", "ab", 22.953);
}
