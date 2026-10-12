//! A collapsed navigation pane leaves no space behind.
//!
//! The PIM shell (AzMail, AzNotes, AzTasks) splits its row by shares: the navigation pane's
//! box takes a fifth of the width. Collapsed, Outlook's navigation pane is a strip of its
//! module icons and the list moves up beside it. In Chrome a flex item sized by its share stays
//! that wide whatever its content, so a pane box that keeps its share leaves the rest of it
//! blank (AzMail, the user's screenshot): the shell has to give a collapsed pane the strip's
//! width, outside the splits.
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
        PimShell, ShellNavigationModule, ShellNavigationPane,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn lay_out(collapsed: bool) -> LayoutWindow {
    let navigation = ShellNavigationPane::create()
        .with_module(ShellNavigationModule::create(
            AzString::from("Mail"),
            AzString::from("mail"),
        ))
        .with_module(ShellNavigationModule::create(
            AzString::from("Calendar"),
            AzString::from("calendar_month"),
        ))
        .with_collapsed(collapsed)
        .dom();
    let shell = PimShell::create(
        navigation,
        Dom::create_div().with_child(Dom::create_p_with_text("Inbox")),
        Dom::create_div().with_child(Dom::create_p_with_text("A message")),
    )
    .dom();
    let mut body = Dom::create_body().with_child(shell);
    body.root.set_css(
        "margin: 0px; width: 1000px; height: 600px; display: flex; flex-direction: column;",
    );
    let styled = StyledDom::create_from_dom(body);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(1000.0, 600.0);
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
