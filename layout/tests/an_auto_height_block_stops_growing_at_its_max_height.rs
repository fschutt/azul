//! An auto-height block stops growing at its `max-height`.
//!
//! Mail corpus (scripts/refci/mail_boxes.py, cerberus fluid / hybrid /
//! responsive): Cerberus hides its inbox preheader as
//! `<div style="max-height: 0; overflow: hidden">`. Chrome makes that div
//! 0 px tall; azul made it as tall as its text (97 px), and every box of the
//! mail below it sat 97 px too low.
//!
//! CSS 2.2 10.7: the used height of a box with `height: auto` is its content
//! height clamped by `max-height`, and then by `min-height` (min wins when
//! the two conflict). The sizing pass applied both to the pre-layout
//! placeholder height, but the content-based height that replaces the
//! placeholder after the children are laid out took the larger of the two
//! and never looked at `max-height` again.
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
<div style=\"width: 400px; font-size: 16px; line-height: 20px;\">\
<div id=\"preheader\" style=\"max-height: 0; overflow: hidden;\">This text appears in the inbox \
preview but not in the body of the mail. It is long enough to wrap onto several lines when \
nothing clips it, so an unclamped box is clearly taller than nothing.</div>\
<div id=\"after_preheader\">Hello</div>\
<div id=\"capped\" style=\"max-height: 30px; overflow: hidden;\">one<br/>two<br/>three<br/>four</div>\
<div id=\"after_capped\">After</div>\
<div id=\"visible\" style=\"max-height: 25px;\">one<br/>two<br/>three</div>\
<div id=\"after_visible\">After</div>\
<div id=\"padded\" style=\"max-height: 10px; padding: 5px; overflow: hidden;\">one<br/>two</div>\
<div id=\"min_wins\" style=\"min-height: 50px; max-height: 20px;\">x</div>\
<div id=\"short\" style=\"max-height: 100px;\">x</div>\
</div></body></html>";

fn laid_out() -> LayoutWindow {
    let parsed = azul_layout::xml::parse_xml(MAIL).expect("the mail parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 600.0);
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

/// `(top, height)` of the element with `id` (border box).
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

#[test]
fn a_preheader_with_max_height_zero_takes_no_space() {
    let lw = laid_out();
    let (top, height) = top_and_height(&lw, "preheader");
    assert!(
        close(height, 0.0),
        "`max-height: 0` makes the preheader 0px tall, got {height}"
    );
    let (next_top, _) = top_and_height(&lw, "after_preheader");
    assert!(
        close(next_top, top),
        "the text after the preheader starts where the preheader does: {next_top} vs {top}"
    );
}

#[test]
fn a_clipped_box_is_exactly_its_max_height_and_the_next_box_follows_it() {
    let lw = laid_out();
    let (top, height) = top_and_height(&lw, "capped");
    assert!(
        close(height, 30.0),
        "four lines under `max-height: 30px` are 30px tall, got {height}"
    );
    let (next_top, _) = top_and_height(&lw, "after_capped");
    assert!(
        close(next_top, top + 30.0),
        "the next box follows the 30px box: {next_top} vs {top} + 30"
    );
}

#[test]
fn an_overflowing_visible_box_is_its_max_height_too() {
    // `overflow: visible` lets the lines paint below the box, but the box
    // itself - and the flow after it - still stops at max-height.
    let lw = laid_out();
    let (top, height) = top_and_height(&lw, "visible");
    assert!(
        close(height, 25.0),
        "three lines under `max-height: 25px` make a 25px box, got {height}"
    );
    let (next_top, _) = top_and_height(&lw, "after_visible");
    assert!(
        close(next_top, top + 25.0),
        "the next box follows the 25px box, not the overflowing lines: {next_top} vs {top} + 25"
    );
}

#[test]
fn max_height_limits_the_content_box_and_the_padding_comes_on_top() {
    // box-sizing: content-box (the initial value): max-height is the CONTENT
    // height, the 5px padding above and below is added outside it.
    let lw = laid_out();
    let (_, height) = top_and_height(&lw, "padded");
    assert!(
        close(height, 20.0),
        "max-height 10px + 2 x 5px padding = a 20px border box, got {height}"
    );
}

#[test]
fn min_height_wins_over_a_smaller_max_height_and_a_short_box_keeps_its_content_height() {
    let lw = laid_out();
    let (_, min_wins) = top_and_height(&lw, "min_wins");
    assert!(
        close(min_wins, 50.0),
        "min-height 50px beats max-height 20px (CSS 2.2 10.7), got {min_wins}"
    );
    let (_, short) = top_and_height(&lw, "short");
    assert!(
        short > 10.0 && short < 30.0,
        "one line under a 100px max-height stays one line tall, got {short}"
    );
}
