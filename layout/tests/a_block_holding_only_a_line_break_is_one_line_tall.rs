//! A block holding only a line break is one line tall.
//!
//! AzMail exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, 1.3
//! sample 02 and probe `br`, gap E-BR): Gmail writes every blank line of a
//! message as `<div><br></div>`. In every browser that div is one line box
//! tall - the line the `<br>` ends (CSS 2.2 9.4.2: a line box that ends with
//! a forced break is not a zero-height one). In azul it was 0 px, so a
//! Gmail reply lost all its paragraph spacing ("Hi Anna," ran straight into
//! the text).
//!
//! The line breaker did make that line (one band of line-height), but the
//! IFC's height was taken from the positioned ITEMS' bounds, and a break is
//! an item without geometry - so a line holding nothing else measured 0.
//! `<p>a<br><br>b</p>` was right only because the text after the blank line
//! reached below it.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::{
    dom::{DomId, DomNodeId, IdOrClass, NodeData, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const LINE: f32 = 20.0;

const MAIL: &str = "<html><head></head><body>\
<div style=\"font-size: 16px; line-height: 20px;\">\
<div id=\"a\">Hi Anna,</div>\
<div id=\"blank\"><br/></div>\
<div id=\"b\">row 14 is the restated figure.</div>\
<div id=\"span_blank\"><span><br/></span></div>\
<div id=\"c\">Robin</div>\
<p id=\"p\" style=\"margin: 0;\">one<br/><br/>two</p>\
</div></body></html>";

fn laid_out() -> LayoutWindow {
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
    lw
}

fn node(lw: &LayoutWindow, id: &str) -> DomNodeId {
    let sd = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    let n = sd
        .node_data
        .as_ref()
        .iter()
        .position(|nd: &NodeData| {
            nd.get_ids_and_classes()
                .iter()
                .any(|c| matches!(c, IdOrClass::Id(s) if s.as_str() == id))
        })
        .unwrap_or_else(|| panic!("no element with id {id}"));
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// `(top, height)` of the element with `id`.
fn top_and_height(lw: &LayoutWindow, id: &str) -> (f32, f32) {
    let n = node(lw, id);
    let pos = lw
        .get_node_position(n)
        .unwrap_or_else(|| panic!("#{id} has a position"));
    let size = lw
        .get_node_size(n)
        .unwrap_or_else(|| panic!("#{id} has a size"));
    (pos.y, size.height)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

/// "One line tall": at least the `line-height` (20px, what the blank line's
/// band is), and no taller than a line of TEXT measures - which is a little
/// more than the line-height in azul, where a line with glyphs is as tall as
/// its font's natural line box when that exceeds `line-height` (20.6px for
/// this font at 16px; browsers say 20). That quirk is not this test's
/// subject; a blank line between those two bounds is one line, not zero and
/// not two.
fn one_line(height: f32, text_line: f32) -> bool {
    height >= LINE - 0.5 && height <= text_line + 0.5
}

#[test]
fn a_div_holding_only_a_br_is_one_line_tall() {
    let lw = laid_out();
    let (_, text_line) = top_and_height(&lw, "a");
    assert!(
        text_line >= LINE - 0.5 && text_line < 1.5 * LINE,
        "a line of text is about {LINE}px: {text_line}"
    );
    let (blank_top, blank) = top_and_height(&lw, "blank");
    assert!(
        one_line(blank, text_line),
        "`<div><br></div>` is one line tall ({LINE}px, a text line is {text_line}px), got {blank}"
    );
    let (b_top, _) = top_and_height(&lw, "b");
    assert!(
        close(b_top, blank_top + blank),
        "the next line sits one blank line below: {b_top} vs {blank_top} + {blank}"
    );
}

#[test]
fn a_br_inside_a_span_is_one_line_tall_too() {
    let lw = laid_out();
    let (_, text_line) = top_and_height(&lw, "a");
    let (_, blank) = top_and_height(&lw, "span_blank");
    assert!(
        one_line(blank, text_line),
        "`<div><span><br></span></div>` is one line tall ({LINE}px, a text line is \
         {text_line}px), got {blank}"
    );
}

#[test]
fn two_brs_between_words_still_make_one_blank_line() {
    let lw = laid_out();
    let (_, text_line) = top_and_height(&lw, "a");
    let (_, p) = top_and_height(&lw, "p");
    assert!(
        p >= 3.0 * LINE - 0.5 && p <= 3.0 * text_line + 0.5,
        "`one<br><br>two` is three lines ({}px to {}px), got {p}",
        3.0 * LINE,
        3.0 * text_line
    );
}
