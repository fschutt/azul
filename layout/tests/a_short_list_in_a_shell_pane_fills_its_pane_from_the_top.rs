//! A short list in an Office-shell pane fills the pane and starts at its top.
//!
//! Found by PIMDRIVE7 (2026-10-03) on AzContacts with ONE contact: its list column (`flex-grow:
//! 1` in a `flex-grow: 1; align-items: stretch` row beside the A-Z bar) laid out 53 px high at
//! the BOTTOM of a row that was only as tall as the A-Z bar (y 423..476 in a row 148..476),
//! instead of filling the pane down to the status bar and starting under the heading. With 40
//! contacts the list overflows and the bug hides. The computed styles were right (the row
//! `display: flex; flex-direction: row; flex-grow: 1; min-height: 0; align-items: stretch`), so
//! the layout is wrong, not the app's CSS.
//!
//! The chain below is the one `get_html_string` printed (the theme scope, the OfficeShell, its
//! split panes - `display: block` halves with a `flex-grow` share holding a `height: 100%` pane).
//! Owner: LAYOUT7 (solver3 / the flex bridge). A stretched flex item's size is definite (CSS
//! Flexbox 9.8), so the `height: 100%` inside the split halves resolves, and the column under
//! it has the whole pane to grow its last row into.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: f32 = 1100.0;
const H: f32 = 720.0;

fn div(css: &str) -> Dom {
    Dom::create_div().with_css(css)
}

/// The list pane: the search line, the heading, the row of the list and the A-Z bar.
fn list_pane() -> Dom {
    let mut list = div(
        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto;",
    )
    .with_id("list".into());
    list.add_child(
        div("display: block; height: 53px;")
            .with_id("only-row".into())
            .with_child(Dom::create_span_with_text(AzString::from("Paula"))),
    );
    let mut jump = div("display: flex; flex-direction: column; width: 18px; flex-shrink: 0;")
        .with_id("jump".into());
    for _ in 0..27 {
        jump.add_child(div("height: 12px;"));
    }
    div("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(div("height: 36px; flex-shrink: 0;"))
        .with_child(div("height: 23px; flex-shrink: 0;"))
        .with_child(
            div(
                "display: flex; flex-direction: row; align-items: center; flex-grow: 1; \
                 min-height: 0px; align-items: stretch;",
            )
            .with_id("body-row".into())
            .with_children(vec![list, jump].into()),
        )
}

/// A split pane of `first` and `second`, each a `display: block` half with its share.
fn split(first: Dom, first_share: f32, second: Dom, second_share: f32) -> Dom {
    let half = |share: f32| {
        div(&format!(
            "display: block; min-width: 0px; min-height: 0px; overflow: hidden; flex-grow: {share};"
        ))
    };
    div("display: flex; flex-direction: row; width: 100%; height: 100%; overflow: hidden; flex-grow: 1;")
        .with_child(half(first_share).with_child(first))
        .with_child(half(second_share).with_child(second))
}

fn window() -> Dom {
    let pane = |content: Dom| {
        Dom::create_section()
            .with_css(
                "display: flex; flex-direction: column; width: 100%; height: 100%; min-width: \
                 0px; min-height: 0px; overflow: hidden; flex-grow: 1;",
            )
            .with_child(content)
    };
    let reading = pane(div("display: block;"));
    let navigation = pane(div("display: block;"));
    let shell = div(
        "display: flex; flex-direction: column; width: 100%; height: 100%; min-width: 0px; \
         min-height: 0px; overflow: hidden; flex-grow: 1;",
    )
    .with_child(
        div("display: flex; flex-direction: row; flex-grow: 1; min-width: 0px; min-height: 0px;")
            .with_child(
                div("display: flex; flex-direction: row; flex-grow: 1; min-width: 0px; min-height: 0px;")
                    .with_child(split(
                        navigation,
                        0.2,
                        split(pane(list_pane()), 0.4, reading, 0.6),
                        0.8,
                    )),
            ),
    );
    Dom::create_html().with_child(
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; height: 100%; margin: 0px;")
            .with_child(
                div(
                    "display: flex; flex-direction: column; width: 100%; height: 100%; \
                     min-width: 0px; min-height: 0px; overflow: hidden; flex-grow: 1;",
                )
                .with_child(
                    div("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
                        .with_child(shell),
                ),
            ),
    )
}

fn rect_of(lw: &LayoutWindow, id: &str) -> (f32, f32, f32, f32) {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|node| node.has_id(id))
        .unwrap_or_else(|| panic!("no node #{id}"));
    let r = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        })
        .unwrap_or_else(|| panic!("#{id} has no layout rect"));
    (r.origin.x, r.origin.y, r.size.width, r.size.height)
}

#[test]
fn a_short_list_in_a_shell_pane_fills_its_pane_from_the_top() {
    let mut dom = window();
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();

    let (_, row_y, _, row_h) = rect_of(&lw, "body-row");
    let (_, list_y, _, list_h) = rect_of(&lw, "list");
    let (_, item_y, _, _) = rect_of(&lw, "only-row");
    assert!(
        (row_y - 59.0).abs() < 0.5,
        "the row starts under the search line and the heading: y {row_y}"
    );
    assert!(
        (row_y + row_h - H).abs() < 0.5,
        "the row grows to the pane's bottom ({H}): it ends at {}",
        row_y + row_h
    );
    assert!(
        (list_y - row_y).abs() < 0.5 && (list_h - row_h).abs() < 0.5,
        "the list is stretched over the row ({row_y}, {row_h}): it is at {list_y}, {list_h} high"
    );
    assert!((item_y - list_y).abs() < 0.5, "the only row is at the list's top: {item_y} vs {list_y}");
}
