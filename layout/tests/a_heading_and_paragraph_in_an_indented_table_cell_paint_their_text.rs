//! A heading and a paragraph in an indented table cell paint their text.
//!
//! The AzMail exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md,
//! section 1.3, sample 01) rendered a newsletter whose body - an `<h1>` and a
//! `<p>` inside a padded `<td>` - was missing: header, images, button and
//! footer painted, the greeting and every paragraph did not.
//!
//! Mail HTML is indented, so that cell's DOM children are whitespace text,
//! `<h1>`, whitespace, `<p>`, whitespace. The table's row-height pass sent
//! every cell with ANY text child (whitespace included) down its inline
//! (IFC) branch: that laid the cell out as one inline formatting context -
//! which a cell holding blocks does not establish (CSS 2.2 section 9.4.2) -
//! and then cleared the `inline_layout_result` of every child, i.e. the
//! heading's and the paragraph's own text. The exploration's reduction
//! (`nl.html`) had lost the indentation, so it did not reproduce this; its
//! smeared first line at a 700 px window is the headless harness caveat of
//! section 1.3, which the first test below pins as a guard.
//!
//! Not compiled by the author (house rule); expected RED (the second test).

use azul_core::{
    dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 400.0;
const PADDING: f32 = 24.0;

/// `nl.html` byte for byte: no whitespace between the tags.
const NEWSLETTER_FLAT: &str = "<html><head></head><body>\
<div style=\"background:#ffffff;color:#000000\"><table width=\"600\"><tr>\
<td style=\"padding:24px; font-family:Helvetica, Arial, sans-serif; font-size:16px; \
line-height:24px; color:#333333;\">\
<h1 style=\"margin:0 0 12px 0; font-size:22px;\">Hello Robin,</h1>\
<p style=\"margin:0 0 16px 0;\">Here is what happened.</p>\
</td></tr></table></div></body></html>";

/// The same cell, indented the way mail HTML is (sample 01).
const NEWSLETTER_INDENTED: &str = "<html><head></head><body>\
<div style=\"background:#ffffff;color:#000000\"><table width=\"600\"><tr>\
<td style=\"padding:24px; font-family:Helvetica, Arial, sans-serif; font-size:16px; \
line-height:24px; color:#333333;\">
            <h1 style=\"margin:0 0 12px 0; font-size:22px;\">Hello Robin,</h1>
            <p style=\"margin:0 0 16px 0;\">Here is what happened.</p>
          </td></tr></table></div></body></html>";

/// Every glyph the markup paints, as `(x, y)` pen positions.
fn painted_glyphs(markup: &str) -> Vec<(f32, f32)> {
    let parsed = azul_layout::xml::parse_xml(markup).expect("the newsletter parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled = StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the newsletter lays out");
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    result
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => Some(glyphs),
            _ => None,
        })
        .flatten()
        .map(|g| (g.point.x, g.point.y))
        .collect()
}

/// "Hello Robin," and "Here is what happened." paint, on two lines, inside
/// the padded cell and the window.
fn assert_both_lines_paint_inside_the_cell(glyphs: &[(f32, f32)], which: &str) {
    // 34 characters, 4 of them spaces: allow for spaces without a glyph.
    assert!(
        glyphs.len() >= 28,
        "{which}: the heading and the paragraph paint their glyphs, got {}",
        glyphs.len()
    );
    for &(x, y) in glyphs {
        assert!(
            x >= PADDING && x < WIDTH && y > 0.0 && y < HEIGHT,
            "{which}: glyph at ({x}, {y}) lies outside the padded cell or the window"
        );
    }
    let mut baselines: Vec<i32> = glyphs.iter().map(|&(_, y)| y.round() as i32).collect();
    baselines.sort_unstable();
    baselines.dedup();
    assert!(
        baselines.len() >= 2,
        "{which}: the heading and the paragraph are two lines, got baselines {baselines:?}"
    );
}

#[test]
fn a_flat_cells_heading_and_paragraph_paint_inside_the_cell() {
    assert_both_lines_paint_inside_the_cell(&painted_glyphs(NEWSLETTER_FLAT), "flat");
}

#[test]
fn an_indented_cells_heading_and_paragraph_paint_inside_the_cell() {
    assert_both_lines_paint_inside_the_cell(&painted_glyphs(NEWSLETTER_INDENTED), "indented");
}
