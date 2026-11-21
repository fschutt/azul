//! The OpenType features a stylesheet asks for reach the shaper and change
//! the glyphs, measured on Calmet UI (registered as an app font), which has them: `tnum`
//! (every figure on a 469-unit advance, the median digit's), `liga` (fi, fl,
//! ff), `dlig` (the ct ligature) and `ss01` (the long s before a letter).
//!
//! AzDrive's size and date columns are tabular (`font-variant-numeric:
//! tabular-nums`), and the design toggles `dlig` and `ss01` through
//! `font-feature-settings`. The numbers are Chrome's for the same CSS and the
//! same font at 20px: proportional "1111" is 36.41px wide, tabular 37.53px.

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

/// Calmet UI's units per em: an advance in px at 20px is units / 50.
const UPEM: f32 = 1000.0;
const SIZE: f32 = 20.0;

/// Every glyph the display list paints for `text` in Calmet UI 20px under
/// the declarations `css`: (glyph index, pen x).
fn glyphs(css: &str, text: &str) -> Vec<(u32, f32)> {
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css(&format!(
                "margin: 0px; font-family: \"Calmet UI\"; font-size: {SIZE}px; {css}"
            ))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
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
    .expect("the line lays out");
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .items
        .iter()
        .flat_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => {
                glyphs.iter().map(|g| (g.index, g.point.x)).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect()
}

/// The distance from each glyph's pen to the next one's.
fn steps(css: &str, text: &str) -> Vec<f32> {
    glyphs(css, text).windows(2).map(|w| w[1].1 - w[0].1).collect()
}

fn assert_every_step_is(css: &str, text: &str, units: f32) {
    let want = units / UPEM * SIZE;
    let got = steps(css, text);
    assert_eq!(got.len(), text.chars().count() - 1, "{css} {text:?}: one glyph per figure");
    assert!(
        got.iter().all(|s| (s - want).abs() < 0.01),
        "{css} {text:?}: every figure {want:.3}px apart, got {got:?}"
    );
}

#[test]
fn without_tnum_the_figures_keep_their_own_widths() {
    assert_every_step_is("", "1111", 455.0);
    assert_every_step_is("", "0000", 492.0);
}

#[test]
fn font_feature_settings_tnum_sets_every_figure_on_one_advance() {
    for text in ["1111", "0000", "2026"] {
        assert_every_step_is("font-feature-settings: \"tnum\";", text, 469.0);
    }
}

#[test]
fn font_variant_numeric_tabular_nums_sets_every_figure_on_one_advance() {
    for text in ["1111", "0000", "2026"] {
        assert_every_step_is("font-variant-numeric: tabular-nums;", text, 469.0);
    }
}

#[test]
fn a_feature_turned_off_is_off_even_where_the_shaper_turns_it_on() {
    assert_eq!(glyphs("", "fi").len(), 1, "premise: liga is on by default and joins fi");
    assert_eq!(
        glyphs("font-feature-settings: \"liga\" 0;", "fi").len(),
        2,
        "\"liga\" 0 sets f and i apart"
    );
    assert_eq!(
        steps("font-feature-settings: \"tnum\" 0;", "1111"),
        steps("", "1111"),
        "\"tnum\" 0 asks for nothing"
    );
}

#[test]
fn font_feature_settings_dlig_joins_c_and_t() {
    assert_eq!(glyphs("", "ct").len(), 2, "premise: dlig is off by default");
    assert_eq!(glyphs("font-feature-settings: \"dlig\";", "ct").len(), 1, "dlig joins ct");
}

#[test]
fn font_feature_settings_ss01_draws_the_long_s_before_a_letter() {
    let long_s = glyphs("", "\u{17F}a")[0].0;
    let round_s = glyphs("", "sa")[0].0;
    assert_ne!(long_s, round_s, "premise: the long and the round s are two glyphs");
    assert_eq!(glyphs("font-feature-settings: \"ss01\";", "sa")[0].0, long_s, "ss01: s before a");
    let word_end = glyphs("font-feature-settings: \"ss01\";", "as");
    assert_eq!(word_end[1].0, round_s, "ss01 keeps the round s at the end of a word");
}

/// Chrome does not join letters that are tracked apart (CSS Text 3 s8.2: a
/// non-zero letter-spacing should not apply optional ligatures): "fi" at
/// `letter-spacing: 1px` is the f and the i, 16.27px wide in Calmet UI 20px
/// (the unligated pair, 14.27px, and two trackings). Settings that ask for
/// `liga` still get it.
#[test]
fn a_tracked_word_is_not_ligated_unless_the_settings_ask_for_it() {
    assert_eq!(glyphs("letter-spacing: 1px;", "fi").len(), 2, "tracked: f and i");
    assert_eq!(
        glyphs("letter-spacing: 1px; font-feature-settings: \"liga\";", "fi").len(),
        1,
        "\"liga\" asked for: one ligature"
    );
}
