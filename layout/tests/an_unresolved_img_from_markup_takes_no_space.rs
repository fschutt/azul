//! An unresolved `<img>` from markup takes no space.
//!
//! DEDUP_EDITORS A3.8 (scripts/DEDUP_EDITORS_2026_10_02.md): AzMail emits
//! `<img src>` for every picture of a mail once "download pictures" is
//! pressed, before the bytes arrive. The XML loader makes such an image a
//! `NullImage` tagged with its `src`, which has no size until the app caches
//! the picture under that src - and the sizing pass gave a replaced element
//! without a natural size the CSS 2.2 10.3.2 fallback of 300x150. So every
//! pending picture opened a 300x150 hole, and a failed download stayed one.
//!
//! A browser lays a not-yet-available image without dimension attributes out
//! with no size (HTML rendering 15.4.3: an image that represents nothing is
//! an empty inline element), and keeps the box its `width` / `height`
//! attributes give it. The 300x150 fallback stays for replaced elements that
//! are sized from the outside (a render-callback image, a VirtualView).
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
<div style=\"width: 600px; font-size: 16px; line-height: 20px;\">\
<div id=\"before\">Hello Robin,</div>\
<div id=\"holder\"><img id=\"pending\" src=\"https://example.org/logo.png\"/></div>\
<div id=\"sized_holder\"><img id=\"sized\" src=\"https://example.org/hero.png\" width=\"200\" \
height=\"80\"/></div>\
<div id=\"after\">Robin</div>\
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

fn size(lw: &LayoutWindow, id: &str) -> LogicalSize {
    lw.get_node_size(node(lw, id))
        .unwrap_or_else(|| panic!("#{id} has a size"))
}

#[test]
fn a_pending_picture_without_dimensions_is_not_a_300_by_150_hole() {
    let lw = laid_out();
    let img = size(&lw, "pending");
    assert!(
        img.width < 0.5 && img.height < 0.5,
        "an <img src> whose picture is not there yet has no size, got {}x{}",
        img.width,
        img.height
    );
    let holder = size(&lw, "holder");
    assert!(
        holder.height < 30.0,
        "the block around it is at most one line tall, not 150px: {}",
        holder.height
    );
}

#[test]
fn a_pending_picture_with_width_and_height_keeps_its_box() {
    let lw = laid_out();
    let img = size(&lw, "sized");
    assert!(
        (img.width - 200.0).abs() < 0.5 && (img.height - 80.0).abs() < 0.5,
        "width=200 height=80 keep their 200x80 box while the picture loads, got {}x{}",
        img.width,
        img.height
    );
}
