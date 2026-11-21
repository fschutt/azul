//! Bold text set in a face that has no bold cut is drawn bolder, the way
//! Chrome draws it.
//!
//! Calmet UI ships a regular and an italic and nothing heavier, so a
//! `font-weight: bold` crumb, title or selected name matches the regular
//! face. Chrome then synthesizes the weight: Skia strokes every glyph
//! outline while it fills it (stroke width `size * 1/24` at 9px down to
//! `size * 1/32` at 36px), so the stems thicken and the advances stay as
//! they are. A face with a real bold cut (EB Garamond 700) is drawn from that
//! cut and never thickened again.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::{AppConfig, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
    solver3::display_list::{DisplayList, DisplayListItem},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CALMET_UI: &str = azul_fonts_calmet::FAMILY;
const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 80.0;

/// A window that took Calmet UI in, the way AzDrive registers it.
fn window() -> LayoutWindow {
    let mut config = AppConfig::create();
    for face in azul_fonts_calmet::faces() {
        config.add_font(CALMET_UI.into(), face.to_vec().into());
    }
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.font_manager.register_app_fonts(config.bundled_fonts.as_ref());
    lw
}

/// `text` in `family` at 24px black on white, with `weight` as its
/// `font-weight`: the window that laid it out and its display list.
fn laid_out(family: &str, weight: &str, text: &str) -> (LayoutWindow, DisplayList) {
    let css = format!(
        "margin: 0px; padding: 8px; font-family: \"{family}\"; font-size: 24px; \
         font-weight: {weight}; color: #000000;"
    );
    let dom = Dom::create_body()
        .with_css("margin: 0px; background: #ffffff;")
        .with_child(
            Dom::create_div()
                .with_css(&css)
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
        );
    let mut lw = window();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the word lays out");
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref()
        .clone();
    (lw, dl)
}

/// Every painted glyph: its face hash, its index and its pen x.
fn glyphs(dl: &DisplayList) -> Vec<(u64, u32, f32)> {
    dl.items
        .iter()
        .flat_map(|item| match item {
            DisplayListItem::Text {
                glyphs, font_hash, ..
            } => glyphs
                .iter()
                .map(|g| (font_hash.font_hash, g.index, g.point.x))
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// How much ink the word puts down on the CPU: the darkness of every pixel
/// summed (black text on white, so 0 for an empty page).
fn ink(family: &str, weight: &str, text: &str) -> u64 {
    let (lw, dl) = laid_out(family, weight, text);
    let mut cache = GlyphCache::new();
    let pixmap = cpurender::render_with_font_manager(
        &dl,
        &RendererResources::default(),
        &lw.font_manager,
        RenderOptions {
            width: WIDTH,
            height: HEIGHT,
            dpi_factor: 1.0,
        },
        &mut cache,
    )
    .expect("the word renders on the CPU");
    pixmap
        .data()
        .chunks_exact(4)
        .map(|px| {
            let lum = (u64::from(px[0]) + u64::from(px[1]) + u64::from(px[2])) / 3;
            255 - lum
        })
        .sum()
}

#[test]
fn a_bold_word_in_calmet_ui_puts_down_clearly_more_ink_than_the_regular_one() {
    let regular = ink(CALMET_UI, "normal", "Gardasee");
    let bold = ink(CALMET_UI, "bold", "Gardasee");
    assert!(regular > 0, "the regular word paints");
    assert!(
        bold * 100 >= regular * 115,
        "bold Calmet UI is synthesized like Chrome's fake bold, so its stems are thicker: \
         bold ink {bold}, regular ink {regular}"
    );
}

#[test]
fn a_semibold_word_in_calmet_ui_is_drawn_bold_as_well() {
    let regular = ink(CALMET_UI, "normal", "Gardasee");
    let semibold = ink(CALMET_UI, "600", "Gardasee");
    assert!(
        semibold * 100 >= regular * 115,
        "600 is a bold weight (CSS Fonts: synthesized from 600 up): semibold ink {semibold}, \
         regular ink {regular}"
    );
}

#[test]
fn a_medium_word_in_calmet_ui_stays_regular() {
    let regular = ink(CALMET_UI, "normal", "Gardasee");
    let medium = ink(CALMET_UI, "500", "Gardasee");
    assert_eq!(
        medium, regular,
        "500 is not a bold weight, so the regular face is drawn as it is"
    );
}

#[test]
fn the_synthesized_bold_word_keeps_the_advances_of_the_regular_face() {
    let (_, regular) = laid_out(CALMET_UI, "normal", "Gardasee");
    let (_, bold) = laid_out(CALMET_UI, "bold", "Gardasee");
    let regular = glyphs(&regular);
    let bold = glyphs(&bold);
    assert!(!regular.is_empty(), "the word paints glyphs");
    assert_eq!(regular.len(), bold.len(), "the same glyphs");
    for (r, b) in regular.iter().zip(&bold) {
        assert_eq!((r.0, r.1), (b.0, b.1), "the same face and glyph");
        assert!(
            (r.2 - b.2).abs() < 0.01,
            "Chrome thickens the outline only, the pen moves as for the regular face: \
             regular x {}, bold x {}",
            r.2,
            b.2
        );
    }
}

#[test]
fn a_face_with_a_bold_cut_is_not_thickened_again() {
    // EB Garamond is bundled at 400 and 700: bold text is drawn from the 700
    // cut, so it differs from the regular in its glyph shapes and face, and
    // its ink is the bold cut's own. Thickening it on top would draw it
    // heavier than any browser does.
    let (_, regular) = laid_out("EB Garamond", "normal", "Gardasee");
    let (_, bold) = laid_out("EB Garamond", "bold", "Gardasee");
    let regular_face = glyphs(&regular).first().map(|g| g.0);
    let bold_face = glyphs(&bold).first().map(|g| g.0);
    assert!(regular_face.is_some() && bold_face.is_some(), "both paint");
    assert_ne!(
        regular_face, bold_face,
        "bold EB Garamond comes from its own bold cut"
    );
    let synthesized_runs = bold
        .items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::Text { .. }))
        .filter(|item| format!("{item:?}").contains("synthetic_bold: true"))
        .count();
    assert_eq!(
        synthesized_runs, 0,
        "a run drawn from a real bold cut is never synthesized"
    );
}

#[test]
fn a_bold_run_in_calmet_ui_is_marked_for_synthesis_in_the_display_list() {
    // Both rasterizers read the mark from the run's face: the CPU one
    // thickens its outlines, the GPU one asks WebRender for the glyphs with
    // its synthetic-bold flag. The `Debug` form names it, so this reads it
    // without depending on the field.
    let (_, bold) = laid_out(CALMET_UI, "bold", "Gardasee");
    let (_, regular) = laid_out(CALMET_UI, "normal", "Gardasee");
    let marked = |dl: &DisplayList| {
        dl.items
            .iter()
            .filter(|item| matches!(item, DisplayListItem::Text { .. }))
            .filter(|item| format!("{item:?}").contains("synthetic_bold: true"))
            .count()
    };
    assert!(marked(&bold) > 0, "the bold run is marked for synthetic bold");
    assert_eq!(marked(&regular), 0, "the regular run is not");
}
