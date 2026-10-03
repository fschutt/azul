//! A blank line is as tall as a line of text in the same font.
//!
//! Mail corpus (scripts/refci/mail_boxes.py, exploration/02_gmail_reply and
//! 05_apple_mail_reply): Gmail and Apple Mail write every blank line as
//! `<div><br></div>`. With `line-height: normal`, Chrome makes it exactly as
//! tall as a line of text (18px for 16px Arial); azul made it 16px - so
//! every blank line of a reply was 2px short and the quote below drifted up.
//!
//! The blank line is as tall as its STRUT (CSS 2.2 10.8.1): a zero-width
//! inline box with the ascent and descent of the element's first available
//! font - and its `normal` line height, A + D + the font's line gap, is the
//! line height of text in that font. Azul approximated the strut as 0.8 /
//! 0.2 em, without the line gap and without the font's own proportions.
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

// No `line-height` anywhere: every line is `normal`.
const MAIL: &str = "<html><head></head><body style=\"margin: 0;\">\
<div style=\"width: 600px; font-size: 16px;\">\
<div id=\"text\">Hi Anna,</div>\
<div id=\"blank\"><br/></div>\
<div id=\"after\">row 14 is the restated figure.</div>\
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

#[test]
fn a_div_holding_only_a_br_is_as_tall_as_a_line_of_text_in_its_font() {
    let lw = laid_out();
    let (_, text) = top_and_height(&lw, "text");
    let (blank_top, blank) = top_and_height(&lw, "blank");
    assert!(
        text > 16.5,
        "a 16px line of text with line-height: normal is taller than 1em: {text}"
    );
    assert!(
        (blank - text).abs() < 0.5,
        "`<div><br></div>` is one line of text tall ({text}px), got {blank}px"
    );
    let (after_top, _) = top_and_height(&lw, "after");
    assert!(
        (after_top - (blank_top + blank)).abs() < 0.5,
        "the next line follows the blank one: {after_top} vs {blank_top} + {blank}"
    );
}
