//! A box after one that gains or loses a child keeps its own style when the
//! page is laid out again.
//!
//! A rebuilt DOM numbers its nodes anew: a node inserted early moves every
//! later one on by one. The layout's reconcile paired the new nodes with the
//! cached tree's by their raw ids, so each box after the insertion was paired
//! with its next sibling of the old tree. A `Dom::with_css` style is matched
//! by selector and leaves the node's own data empty, so the node fingerprint
//! saw no change and the box was cloned with its sibling's borders: a 1 px box
//! with 3 px borders was laid out 1 px square, its border painted around that.

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::{DisplayListItem, WindowLogicalRect},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Three boxes in a row of the body: a box that holds a child or not, a box
/// 1 px square with 3 px borders, and a box without any.
fn page(first_has_a_child: bool) -> Dom {
    let mut first = Dom::create_div().with_css(
        "position: absolute; left: 238px; top: 42px; width: 20px; height: 36px; \
         background: #757ba2; overflow: hidden;",
    );
    if first_has_a_child {
        first = first.with_child(Dom::create_div().with_css(
            "position: absolute; left: 0px; top: 12px; width: 20px; height: 36px; \
             background: #33cc66;",
        ));
    }
    Dom::create_html().with_child(
        Dom::create_body()
            .with_css("margin: 0; padding: 0;")
            .with_child(first)
            .with_child(Dom::create_div().with_css(
                "position: absolute; left: 122px; top: 30px; width: 1px; height: 1px; \
                 background: #f19242; border: 3px solid #224488;",
            ))
            .with_child(Dom::create_div().with_css(
                "position: absolute; left: 91px; top: 51px; width: 58px; height: 78px; \
                 background: #9cf00c;",
            )),
    )
}

fn lay_out(lw: &mut LayoutWindow, dom: Dom) {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(320.0, 240.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

/// The background painted in `(r, g, b)`.
fn painted(lw: &LayoutWindow, (r, g, b): (u8, u8, u8)) -> Vec<LogicalRect> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Rect {
                bounds: WindowLogicalRect(bounds),
                color,
                ..
            } if (color.r, color.g, color.b) == (r, g, b) => Some(*bounds),
            _ => None,
        })
        .collect()
}

#[test]
fn the_boxes_after_one_that_gains_or_loses_a_child_keep_their_own_style() {
    for (before, after, change) in [(false, true, "gains"), (true, false, "loses")] {
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        lay_out(&mut lw, page(before));
        lay_out(&mut lw, page(after));
        let bordered = painted(&lw, (0xf1, 0x92, 0x42));
        assert_eq!(
            bordered,
            vec![LogicalRect::new(LogicalPosition::new(122.0, 30.0), LogicalSize::new(7.0, 7.0))],
            "the first box {change} a child: the 1 px box with 3 px borders is 7 px square"
        );
        let plain = painted(&lw, (0x9c, 0xf0, 0x0c));
        assert_eq!(
            plain,
            vec![LogicalRect::new(LogicalPosition::new(91.0, 51.0), LogicalSize::new(58.0, 78.0))],
            "the first box {change} a child: the box without borders keeps its size"
        );
    }
}
