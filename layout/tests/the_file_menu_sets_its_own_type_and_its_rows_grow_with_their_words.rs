//! The ribbon's File menu sets its own type, and its sub-command rows grow with their words.
//!
//! The ribbon hangs the File menu on the File tab, a flora button set in capitals: uppercase,
//! tracked, semibold, on a 24px line. CSS inheritance follows the DOM, not the window a popup
//! shows in - a popover in Chrome inherits from its DOM parent the same way - so the menu's
//! root has to set the inherited text properties itself, or every row of the menu is set in
//! the tab's capitals. A sub-command is a label over a description that may wrap (Delete
//! history's "Forgets the places ..."), so its row height is a minimum, as in Windows 8's
//! menu; a fixed height lets the words spill over the panel's title and the next row.
//!
//! The menu is laid out as the parent window measures it for its popup: its panel under the
//! node it hangs on, the Delete history panel shown (what pointing at the command does).
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId, NodeType},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::{
    props::{layout::LayoutDisplay, property::CssProperty},
    AzString,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        ribbon_file_menu::{
            RibbonFileMenu, RibbonFileMenuCommand, RibbonFileMenuCommandVec, MENU_CLASS,
            PANEL_CLASS, SUB_CLASS, TITLE_CLASS,
        },
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// What the flora File tab passes down to everything inside it (the debug server's computed
/// style of the menu's panel in AzDrive): its capitals and its line.
const FILE_TAB_TYPE: &str = "font-size: 14px; font-weight: 600; letter-spacing: 0.96px; \
                             line-height: 24px; text-transform: uppercase; text-align: center;";

fn s(text: &str) -> AzString {
    AzString::from(text)
}

/// AzDrive's Delete history: three choices, the first with a description long enough to
/// wrap in the side column in any type.
fn file_menu() -> RibbonFileMenu {
    let delete_history = RibbonFileMenuCommand::create(s("history"), s("Delete history"))
        .with_child(
            RibbonFileMenuCommand::create(s(""), s("Recent places")).with_description(s(
                "Forgets the places that Recent places and Frequent places remember. The \
                 pinned places stay where they are, and Back and Forward keep their way \
                 through the folders you opened today.",
            )),
        )
        .with_child(
            RibbonFileMenuCommand::create(s(""), s("Back and Forward"))
                .with_description(s("Forgets where Back and Forward lead.")),
        )
        .with_child(
            RibbonFileMenuCommand::create(s(""), s("Everything"))
                .with_description(s("Both.")),
        );
    RibbonFileMenu::create(RibbonFileMenuCommandVec::from_vec(vec![
        RibbonFileMenuCommand::create(s("open_in_new"), s("Open new window")),
        delete_history,
        RibbonFileMenuCommand::create(s("close"), s("Close")),
    ]))
    .with_theme(UiTheme::Flora)
}

fn has_class(dom: &Dom, class: &str) -> bool {
    dom.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| matches!(c, IdOrClass::Class(name) if name.as_str() == class))
}

/// The menu's panel (the popup's content) with Delete history's panel shown and the places
/// hidden, as the popup shows them while the pointer is on Delete history.
fn panel_showing_delete_history() -> Dom {
    let window = file_menu().dom();
    assert!(
        matches!(window.root.get_node_type(), NodeType::TransientWindow(_)),
        "the menu is a transient window"
    );
    let mut panel = window.children.as_ref()[0].clone();
    assert!(has_class(&panel, MENU_CLASS));
    let columns: &mut [Dom] = panel.children.as_mut();
    let side = &mut columns[1];
    let panels: &mut [Dom] = side.children.as_mut();
    for (index, p) in panels.iter_mut().enumerate() {
        assert!(has_class(p, PANEL_CLASS));
        // Panel 0 holds the places, panel 1 Delete history (the only command with choices).
        let display = if index == 1 {
            LayoutDisplay::Flex
        } else {
            LayoutDisplay::None
        };
        p.root
            .upsert_inline_css_property(CssProperty::const_display(display));
    }
    panel
}

/// The menu's panel hung on a node set in `inherited` type, laid out.
fn lay_out(inherited: &str) -> LayoutWindow {
    // Inline styles on the two nodes themselves (`NodeData::set_css`): `Dom::with_css` would
    // match its declarations on every node of the subtree instead of letting them inherit.
    let mut hook = Dom::create_div().with_child(panel_showing_delete_history());
    hook.root.set_css(inherited);
    let mut dom = Dom::create_body().with_child(hook);
    dom.root.set_css("margin: 0px;");
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(900.0, 700.0);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .expect("the menu lays out");
    lw
}

fn styled(lw: &LayoutWindow) -> &StyledDom {
    &lw.get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .styled_dom
}

fn with_class(lw: &LayoutWindow, class: &str) -> Vec<NodeId> {
    styled(lw)
        .node_data
        .as_container()
        .internal
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            n.get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, IdOrClass::Class(name) if name.as_str() == class))
        })
        .map(|(i, _)| NodeId::new(i))
        .collect()
}

fn rect(lw: &LayoutWindow, node: NodeId) -> Option<LogicalRect> {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    })
    .filter(|r| r.size.width > 0.0 && r.size.height > 0.0)
}

fn is_inside(lw: &LayoutWindow, node: NodeId, ancestor: NodeId) -> bool {
    let hierarchy = styled(lw).node_hierarchy.as_container();
    let mut at = node;
    while let Some(up) = hierarchy[at].parent_id() {
        if up == ancestor {
            return true;
        }
        at = up;
    }
    false
}

/// The `<p>` boxes inside `row` (its label and its description).
fn words_of(lw: &LayoutWindow, row: NodeId) -> Vec<LogicalRect> {
    let data = styled(lw).node_data.as_container();
    (0..data.len())
        .map(NodeId::new)
        .filter(|n| matches!(data[*n].get_node_type(), NodeType::P) && is_inside(lw, *n, row))
        .filter_map(|n| rect(lw, n))
        .collect()
}

fn bottom(r: &LogicalRect) -> f32 {
    r.origin.y + r.size.height
}

#[test]
fn a_sub_command_row_is_as_tall_as_its_label_and_its_wrapping_description() {
    let lw = lay_out("");
    let title = with_class(&lw, TITLE_CLASS)
        .into_iter()
        .find_map(|n| rect(&lw, n))
        .expect("Delete history's title is shown");
    let rows: Vec<(NodeId, LogicalRect)> = with_class(&lw, SUB_CLASS)
        .into_iter()
        .filter_map(|n| rect(&lw, n).map(|r| (n, r)))
        .collect();
    assert_eq!(rows.len(), 3, "Delete history shows its three choices: {rows:?}");
    for (row, r) in &rows {
        let words = words_of(&lw, *row);
        assert_eq!(words.len(), 2, "a label over a description: {words:?}");
        for w in &words {
            assert!(
                w.origin.y >= r.origin.y - 0.5 && bottom(w) <= bottom(r) + 0.5,
                "the words {w:?} stay inside their row {r:?}"
            );
        }
    }
    assert!(
        bottom(&title) <= rows[0].1.origin.y + 0.5,
        "the first choice {:?} starts under the title {title:?}",
        rows[0].1
    );
    for pair in rows.windows(2) {
        assert!(
            bottom(&pair[0].1) <= pair[1].1.origin.y + 0.5,
            "the choices do not overlap: {:?} then {:?}",
            pair[0].1,
            pair[1].1
        );
    }
}

#[test]
fn the_file_menu_lays_out_the_same_whatever_button_it_hangs_on() {
    let plain = lay_out("");
    let in_capitals = lay_out(FILE_TAB_TYPE);
    let menu = with_class(&plain, MENU_CLASS)[0];
    assert_eq!(menu, with_class(&in_capitals, MENU_CLASS)[0], "the same tree");
    let origin_plain = rect(&plain, menu).expect("the menu is laid out").origin;
    let origin_caps = rect(&in_capitals, menu).expect("the menu is laid out").origin;
    let count = styled(&plain).node_data.as_container().len();
    for index in menu.index()..count {
        let node = NodeId::new(index);
        let (Some(a), Some(b)) = (rect(&plain, node), rect(&in_capitals, node)) else {
            assert_eq!(
                rect(&plain, node).is_some(),
                rect(&in_capitals, node).is_some(),
                "node {index} has a box under both buttons or under neither"
            );
            continue;
        };
        let same = (a.origin.x - origin_plain.x - (b.origin.x - origin_caps.x)).abs() < 0.01
            && (a.origin.y - origin_plain.y - (b.origin.y - origin_caps.y)).abs() < 0.01
            && (a.size.width - b.size.width).abs() < 0.01
            && (a.size.height - b.size.height).abs() < 0.01;
        assert!(
            same,
            "node {index} is laid out the same under a plain node ({a:?}) and under the File \
             tab's capitals ({b:?})"
        );
    }
}
