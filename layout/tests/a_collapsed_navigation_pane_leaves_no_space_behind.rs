//! A collapsed navigation pane leaves no space behind.
//!
//! The PIM shell (AzMail, AzNotes, AzTasks) splits its row by shares: the navigation pane's
//! box takes a fifth of the width. Collapsed, Outlook's navigation pane is a strip of its
//! module icons and the list moves up beside it. In Chrome a flex item sized by its share stays
//! that wide whatever its content, so a pane box that keeps its share leaves the rest of it
//! blank (AzMail, the user's screenshot): the shell has to give a collapsed pane the strip's
//! width, outside the splits.
//!
//! Open, the pane fills whatever slot holds it, across and down: the slot's share (or a rail's
//! width) decides how wide the pane is, never the pane itself. A flex column's item with a
//! `width` of its own is not stretched across, and one that does not grow is only as tall as
//! its content, in Chrome as in azul (the plain-CSS probe below), so a pane box of fixed width
//! that does not grow left blank space beside it and under it (AzNotes, AzMail).
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::shells::{
        navigation_pane::NAV_CLASS,
        pim_shell::{LIST_ID, NAVIGATION_ID},
        OfficeShell, PimShell, ShellNavigationModule, ShellNavigationPane, ShellPane,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn navigation_pane(collapsed: bool) -> Dom {
    ShellNavigationPane::create()
        .with_module(ShellNavigationModule::create(
            AzString::from("Mail"),
            AzString::from("mail"),
        ))
        .with_module(ShellNavigationModule::create(
            AzString::from("Calendar"),
            AzString::from("calendar_month"),
        ))
        .with_collapsed(collapsed)
        .dom()
}

fn lay_out(collapsed: bool) -> LayoutWindow {
    lay_out_at(collapsed, 1000.0)
}

/// The PIM shell in a window `width` wide, its navigation pane open or collapsed.
fn lay_out_at(collapsed: bool, width: f32) -> LayoutWindow {
    let shell = PimShell::create(
        navigation_pane(collapsed),
        Dom::create_div().with_child(Dom::create_p_with_text("Inbox")),
        Dom::create_div().with_child(Dom::create_p_with_text("A message")),
    )
    .dom();
    lay_out_dom(shell, width)
}

/// `content` as the only child of a column body `width` x 600 px, laid out in a window that size.
fn lay_out_dom(content: Dom, width: f32) -> LayoutWindow {
    let mut body = Dom::create_body().with_child(content);
    body.root.set_css(&format!(
        "margin: 0px; width: {width}px; height: 600px; display: flex; flex-direction: column;"
    ));
    let styled = StyledDom::create_from_dom(body);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, 600.0);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .expect("the shell lays out");
    lw
}

fn rect_where(lw: &LayoutWindow, what: &str, matches: impl Fn(&IdOrClass) -> bool) -> LogicalRect {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|n| n.get_ids_and_classes().as_ref().iter().any(&matches))
        .unwrap_or_else(|| panic!("no {what}"));
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .unwrap_or_else(|| panic!("the {what} is laid out"))
}

fn nav(lw: &LayoutWindow) -> LogicalRect {
    rect_where(lw, "navigation pane", |c| {
        matches!(c, IdOrClass::Class(name) if name.as_str() == NAV_CLASS)
    })
}

fn pane(lw: &LayoutWindow, id: &str) -> LogicalRect {
    rect_where(lw, id, |c| matches!(c, IdOrClass::Id(name) if name.as_str() == id))
}

#[test]
fn a_collapsed_navigation_pane_leaves_no_space_behind() {
    let open = lay_out(false);
    assert!(
        nav(&open).size.width > 150.0,
        "open, the navigation pane takes its share: {:?}",
        nav(&open)
    );

    let collapsed = lay_out(true);
    let strip = nav(&collapsed);
    let slot = pane(&collapsed, NAVIGATION_ID);
    let list = pane(&collapsed, LIST_ID);
    assert!(
        strip.size.width < 80.0,
        "collapsed, the navigation pane is a strip of icons: {strip:?}"
    );
    assert!(
        slot.size.width - strip.size.width < 2.0,
        "the navigation pane's slot {slot:?} is as wide as the strip {strip:?} (and its rule)"
    );
    assert!(
        list.origin.x - (strip.origin.x + strip.size.width) < 2.0,
        "the list {list:?} starts beside the strip {strip:?}, nothing blank between them"
    );
}

/// The navigation pane covers `slot` but for a pixel of rounding on each side.
fn assert_fills(nav: LogicalRect, slot: LogicalRect, what: &str) {
    assert!(
        (nav.size.width - slot.size.width).abs() < 2.0 && (nav.origin.x - slot.origin.x).abs() < 2.0,
        "{what}: the navigation pane {nav:?} is as wide as its slot {slot:?}, nothing blank beside it"
    );
    assert!(
        (nav.size.height - slot.size.height).abs() < 2.0 && (nav.origin.y - slot.origin.y).abs() < 2.0,
        "{what}: the navigation pane {nav:?} is as tall as its slot {slot:?}, nothing blank under it"
    );
}

#[test]
fn an_open_navigation_pane_fills_its_share_of_the_pim_shell() {
    // A fifth of 1400 px is wider than the themes' old fixed 230 px, a fifth of 1000 px is
    // narrower: the pane follows the share both ways.
    for width in [1000.0, 1400.0] {
        let lw = lay_out_at(false, width);
        assert_fills(nav(&lw), pane(&lw, NAVIGATION_ID), &format!("{width} px wide"));
    }
}

#[test]
fn an_open_navigation_pane_fills_a_rail_of_the_office_shell() {
    // AzCalendar's layout: the pane in a rail of the app's width, the main pane beside it.
    let shell = OfficeShell::create()
        .with_pane(
            ShellPane::create(AzString::from("calendar-navigation"), navigation_pane(false))
                .with_width(252.0),
        )
        .with_pane(ShellPane::create(
            AzString::from("calendar-main"),
            Dom::create_div().with_child(Dom::create_p_with_text("October")),
        ))
        .dom();
    let lw = lay_out_dom(shell, 1000.0);
    let slot = pane(&lw, "calendar-navigation");
    assert!(
        (slot.size.width - 252.0).abs() < 2.0,
        "the rail keeps the width the app gave it: {slot:?}"
    );
    assert_fills(nav(&lw), slot, "a 252 px rail");
}

#[test]
fn a_flex_column_neither_stretches_an_item_with_a_width_nor_grows_one_that_does_not_grow() {
    // The plain-CSS probe for the two tests above, which Chrome lays out the same way: a
    // column's item is stretched across only while its width is auto (CSS Flexbox 8.3,
    // align-self: stretch), and an item without flex-grow is as tall as its content.
    let item = |id: &str, css: &str| {
        let mut d = Dom::create_div()
            .with_id(AzString::from(id))
            .with_child(Dom::create_div().with_css("height: 50px;"));
        d.root.set_css(css);
        d
    };
    let mut column = Dom::create_div()
        .with_id(AzString::from("column"))
        .with_child(item("fixed", "width: 230px; flex-grow: 0; flex-shrink: 0;"))
        .with_child(item("grown", "flex-grow: 1; min-height: 0px;"));
    column
        .root
        .set_css("display: flex; flex-direction: column; width: 300px; height: 400px;");
    let lw = lay_out_dom(column, 1000.0);
    let fixed = pane(&lw, "fixed");
    let grown = pane(&lw, "grown");
    assert!(
        (fixed.size.width - 230.0).abs() < 0.5 && (fixed.size.height - 50.0).abs() < 0.5,
        "the item with a width keeps it and is as tall as its content: {fixed:?}"
    );
    assert!(
        (grown.size.width - 300.0).abs() < 0.5 && (grown.size.height - 350.0).abs() < 0.5,
        "the auto-width item that grows is stretched across and takes the rest: {grown:?}"
    );
}
