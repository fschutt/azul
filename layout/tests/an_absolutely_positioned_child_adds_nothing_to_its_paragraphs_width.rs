//! An absolutely positioned child adds nothing to its paragraph's width.
//!
//! The flora tab probe (`a_flora_tab_in_a_narrow_column_holds_its_capitals`), run by the lead
//! on 2026-10-10: the unselected "Memory" tab was 95.8 px, as wide as its capitals and its two
//! 18 px paddings, but the SELECTED "History" tab was 199.86 px - its seven capitals took 60 px
//! of it, centred. The selected tab is the only one that hangs children off its box: its two
//! Australis curves (`tabs::australis_curves`: `position: absolute`, 18 px wide, at `left:
//! -18px` / `right: -18px`) and their two run-outs (34 px wide, at -52 px). 18 + 18 + 34 + 34
//! = 104 px = 199.86 - 95.86: every one of them went into the tab's max-content width.
//!
//! CSS 2.2 section 10.3.7 / CSS Positioned Layout 3 section 3: an absolutely positioned box is
//! out of flow; it takes no room in its parent's lines and contributes nothing to its parent's
//! min-content or max-content width (CSS Sizing 3 section 5: the contributions are those of
//! the in-flow content). Chrome sizes a shrink-to-fit paragraph - here a flex item with
//! `flex-basis: auto`, whose base size is its max-content width - by its text alone, whatever
//! absolutely positioned children it holds and however wide they are, whether they are
//! blocks or (blockified) spans with text of their own.
//!
//! The paragraphs below are a tab's shape in plain CSS: "History" between 18 px paddings, as a
//! flex item; one bare, one with the tab's four hung boxes, one with an absolutely positioned
//! span of text wider than the word. All three are as wide as the bare one.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A tab's paragraph: the word between 18 px paddings, the containing block of what hangs off
/// it.
fn tab(id: &str, hung: Vec<Dom>) -> Dom {
    let mut p = Dom::create_p_with_text("History")
        .with_id(AzString::from(id))
        .with_css(
            "margin: 0px; padding: 0px 18px; font-size: 16px; line-height: 24px; \
             position: relative; text-align: center;",
        );
    for h in hung {
        p.add_child(h);
    }
    p
}

/// A box hung off the tab: `position: absolute` at `side: offset`, `width` x `height`.
fn hung_box(side: &str, offset: f32, width: f32, height: f32) -> Dom {
    Dom::create_div().with_css(&format!(
        "position: absolute; {side}: {offset}px; bottom: 0px; width: {width}px; \
         height: {height}px; background: #C6B279;"
    ))
}

fn lay_out(dom: Dom) -> LayoutWindow {
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 200.0);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .expect("the row lays out");
    lw
}

fn rect_of_id(lw: &LayoutWindow, id: &str) -> LogicalRect {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|n| n.has_id(id))
        .unwrap_or_else(|| panic!("no node #{id}"));
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .unwrap_or_else(|| panic!("#{id} has no layout rect"))
}

#[test]
fn an_absolutely_positioned_child_adds_nothing_to_its_paragraphs_width() {
    let curves_and_run_outs = vec![
        hung_box("left", -18.0, 18.0, 28.0),
        hung_box("right", -18.0, 18.0, 28.0),
        hung_box("left", -52.0, 34.0, 2.0),
        hung_box("right", -52.0, 34.0, 2.0),
    ];
    let note = vec![
        Dom::create_span_with_text("an absolutely positioned note, longer than the word")
            .with_css("position: absolute; left: 0px; top: 30px; white-space: nowrap;"),
    ];
    let row = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: flex-start; gap: 60px; \
             width: 760px; padding: 0px 20px;",
        )
        .with_child(tab("bare", Vec::new()))
        .with_child(tab("hung", curves_and_run_outs))
        .with_child(tab("noted", note));
    let lw = lay_out(Dom::create_body().with_css("margin: 0px;").with_child(row));

    let bare = rect_of_id(&lw, "bare");
    let hung = rect_of_id(&lw, "hung");
    let noted = rect_of_id(&lw, "noted");
    println!("bare {bare:?}\nhung {hung:?}\nnoted {noted:?}");
    assert!(
        bare.size.width > 36.0 + 30.0,
        "the bare tab holds its word between its paddings: {bare:?}"
    );
    assert!(
        (hung.size.width - bare.size.width).abs() < 0.5,
        "the tab with its two curves and two run-outs hung off it (18 + 18 + 34 + 34 px, all \
         position: absolute) is as wide as the bare one: hung {hung:?}, bare {bare:?} - {:.2} px \
         more",
        hung.size.width - bare.size.width
    );
    assert!(
        (noted.size.width - bare.size.width).abs() < 0.5,
        "the tab with an absolutely positioned span of text is as wide as the bare one: noted \
         {noted:?}, bare {bare:?} - {:.2} px more",
        noted.size.width - bare.size.width
    );
}
