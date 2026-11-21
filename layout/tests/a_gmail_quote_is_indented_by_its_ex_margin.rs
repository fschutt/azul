//! A Gmail quote is indented by its `ex` margin, not by the UA's 40px.
//!
//! Mail corpus (scripts/refci/mail_boxes.py, exploration/02_gmail_reply):
//! Gmail writes every quote level as
//! `<blockquote style="margin: 0px 0px 0px 0.8ex; border-left: 1px solid
//! rgb(204,204,204); padding-left: 1ex">`. Chrome puts the quote 7px in and
//! its text 9px further; azul refused the `ex` unit, so the whole `margin`
//! declaration was dropped, the blockquote took the UA's `1em 40px` and the
//! reply's quoted text sat 33px too far right and 12-16px too low.
//!
//! CSS Values 4 (6.1.1): `1ex` is the font's x-height, 0.5em "where it is
//! impossible or impractical to determine"; azul resolves lengths without
//! the font at parse time, so `ex` and `ch` take that fallback (Arial's
//! x-height is 0.519em, so Chrome and azul differ by a fraction of a pixel).
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

const MAIL: &str = "<html><head></head><body style=\"margin: 0;\">\
<div id=\"reply\" style=\"width: 600px; font-size: 16px; line-height: 20px;\">\
<div id=\"attribution\">On Mon, Anna wrote:</div>\
<blockquote id=\"quote\" style=\"margin: 0px 0px 0px 0.8ex; border-left: 1px solid \
rgb(204,204,204); padding-left: 1ex\"><div id=\"quoted\">Thanks, the sheet looks good.</div>\
</blockquote></div></body></html>";

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

/// `(x, y, width, height)` of the element's border box.
fn rect(lw: &LayoutWindow, id: &str) -> (f32, f32, f32, f32) {
    let n = node(lw, id);
    let pos = lw
        .get_node_position(n)
        .unwrap_or_else(|| panic!("#{id} has a position"));
    let size = lw
        .get_node_size(n)
        .unwrap_or_else(|| panic!("#{id} has a size"));
    (pos.x, pos.y, size.width, size.height)
}

#[test]
fn the_quote_sits_point_eight_ex_in_and_right_below_the_attribution() {
    let lw = laid_out();
    let (rx, _, rw, _) = rect(&lw, "reply");
    let (_, ay, _, ah) = rect(&lw, "attribution");
    let (qx, qy, qw, _) = rect(&lw, "quote");
    // 0.8ex = 0.4em = 6.4px (Chrome: 0.8 x Arial's x-height = 6.6px).
    assert!(
        (qx - rx - 6.4).abs() < 1.0,
        "the quote's margin-left is 0.8ex (about 6.4px), not the UA 40px: {}",
        qx - rx
    );
    assert!(
        (qw - (rw - 6.4)).abs() < 1.0,
        "the quote fills the rest of the reply's width: {qw} of {rw}"
    );
    assert!(
        (qy - (ay + ah)).abs() < 0.5,
        "`margin: 0px ...` leaves no UA 1em above the quote: {qy} vs {}",
        ay + ah
    );
}

#[test]
fn the_quoted_text_starts_after_the_bar_and_one_ex_of_padding() {
    let lw = laid_out();
    let (qx, _, _, _) = rect(&lw, "quote");
    let (tx, _, _, _) = rect(&lw, "quoted");
    // 1px border + 1ex (8px) padding.
    assert!(
        (tx - qx - 9.0).abs() < 1.0,
        "the quoted block starts 1px border + 1ex padding into the quote: {}",
        tx - qx
    );
}
