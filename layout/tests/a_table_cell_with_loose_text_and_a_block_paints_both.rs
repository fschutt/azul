//! A table cell with loose text and a block paints both.
//!
//! R1_MAIL_RENDER's open item (scripts/R1_MAIL_RENDER_2026_09_30.md, "What is
//! left"): `<td>Label<div>..</div></td>` - Outlook writes cells like that
//! (`<td>text<p class=MsoNormal>..</p></td>`) - took the table's INLINE
//! branch (`layout_cell_for_height`), which lays the cell out as one inline
//! formatting context and then clears its children's own inline layouts: the
//! block's text vanished.
//!
//! Such a cell is a block container with mixed content (CSS 2.2 9.2.1.1):
//! its loose text goes into an anonymous block box beside the block. The
//! fresh layout tree did not build that box for a table cell (only the
//! reconciled one did), so the fix is two-part: a cell's children are built
//! like any block container's, and only a cell whose children are all
//! inline-level takes the inline branch.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::{
    dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Every text run: its glyph pens `(x, y)`.
fn runs(markup: &str) -> Vec<Vec<(f32, f32)>> {
    let parsed = azul_layout::xml::parse_xml(markup).expect("the mail parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the mail lays out");
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    result
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } if !glyphs.is_empty() => {
                Some(glyphs.iter().map(|g| (g.point.x, g.point.y)).collect())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_cells_label_and_the_block_after_it_both_paint_on_two_lines() {
    let runs = runs(
        "<html><head></head><body><table><tr>\
         <td>Label<div>block text</div></td><td>other</td>\
         </tr></table></body></html>",
    );
    let label = runs
        .iter()
        .find(|r| r.len() == "Label".len())
        .unwrap_or_else(|| panic!("\"Label\" paints: {runs:?}"));
    let block = runs
        .iter()
        .find(|r| r.len() == "block text".len())
        .unwrap_or_else(|| panic!("the block's \"block text\" paints: {runs:?}"));
    assert!(
        block[0].1 > label[0].1,
        "the block sits below the cell's loose text: {} vs {}",
        block[0].1,
        label[0].1
    );
    assert!(
        runs.iter().any(|r| r.len() == "other".len() && r != label),
        "the next cell paints too: {runs:?}"
    );
}

#[test]
fn outlooks_text_then_paragraph_cell_paints_both() {
    let runs = runs(
        "<html><head></head><body><table><tr>\
         <td>Agenda<p style=\"margin: 0\">budget review</p></td>\
         </tr></table></body></html>",
    );
    assert!(
        runs.iter().any(|r| r.len() == "Agenda".len()),
        "\"Agenda\" paints: {runs:?}"
    );
    assert!(
        runs.iter().any(|r| r.len() == "budget review".len()),
        "the paragraph paints: {runs:?}"
    );
}
