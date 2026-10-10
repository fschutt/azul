//! A list marker is painted inside its text clip.
//!
//! AzMail exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, 1.3
//! samples 02 and 08, gap E-OL): `<ol>` drew only the marker "2." and
//! `<ul>` no bullet at all. Reproduced with `<ol><li>One</li><li>Two</li>
//! <li>Three</li><li>Four</li><li>Five</li></ol>`: "2." and "4." painted,
//! "1.", "3.", "5." and every bullet did not; with `AZ_NO_LCD_PRETILE=1`
//! all of them painted.
//!
//! Root cause: an outside marker hangs in the padding gutter, at a NEGATIVE
//! inline offset from the list item's content box (`position_one_line`'s
//! `marker_pen`). Every text item of an IFC carries the IFC's clip rect,
//! the content box - grown toward the END edges when a visible axis
//! overflows, never toward the start. So the marker's glyphs lay outside
//! their own item's clip:
//!
//! - WebRender clips a text item to its clip rect: no marker on the GPU;
//! - the CPU's pre-blended LCD tile path clips to it too, while the sweep
//!   path it falls back to for overlapping glyph tiles does not - so a
//!   marker whose digit and dot tiles overlapped ("2.", "4.") survived and
//!   the others vanished.
//!
//! `overflow: visible` does not clip; the clip must hold the ink.
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

const MAIL: &str = "<html><head></head><body><div>\
<p>Agenda:</p>\
<ol><li>Welcome</li><li>Budget</li><li>Any other business</li></ol>\
<ul><li>is it the restated number?</li><li>or the original?</li></ul>\
</div></body></html>";

/// Every text item: `(clip left, clip right, glyph pen xs)`.
fn text_items() -> Vec<(f32, f32, Vec<f32>)> {
    let parsed = azul_layout::xml::parse_xml(MAIL).expect("the mail parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 400.0);
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
            DisplayListItem::Text {
                glyphs, clip_rect, ..
            } if !glyphs.is_empty() => {
                let c = clip_rect.inner();
                Some((
                    c.origin.x,
                    c.origin.x + c.size.width,
                    glyphs.iter().map(|g| g.point.x).collect(),
                ))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn every_list_item_paints_a_marker_in_the_gutter() {
    let items = text_items();
    // The items' text starts at the list's 40px padding; the markers hang
    // left of it. The paragraph "Agenda:" starts further left than both.
    let content_left = items
        .iter()
        .filter_map(|(_, _, xs)| xs.first().copied())
        .fold(f32::MIN, f32::max);
    let markers = items
        .iter()
        .filter(|(_, _, xs)| xs.iter().all(|x| *x < content_left - 1.0))
        .count();
    assert!(
        markers >= 3,
        "the three <ol> items each paint a marker (and the <ul> items a bullet): {items:?}"
    );
}

#[test]
fn every_glyph_of_a_list_lies_inside_its_text_items_clip() {
    for (left, right, xs) in text_items() {
        for x in &xs {
            assert!(
                *x >= left - 0.5 && *x <= right + 0.5,
                "a glyph at x={x} lies outside its text item's clip {left}..{right}: an \
                 overflow-visible box must not clip its ink (the marker vanishes under \
                 WebRender and on the CPU's tile path) - glyphs {xs:?}"
            );
        }
    }
}
