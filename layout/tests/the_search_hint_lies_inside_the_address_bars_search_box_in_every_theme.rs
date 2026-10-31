//! The address bar's search shortcut hint ("⌘F") lies over the end of the search box, on its
//! midline, in every theme.
//!
//! The hint is positioned absolutely (top, bottom and right of its containing block), so the
//! search box has to be its containing block: `position: relative` on the box. Without it the
//! hint is placed against the window, as Chrome does with the same CSS: it ran from the
//! window's top to its bottom and its cap was drawn halfway down the window's right edge
//! (AzDrive in the flat theme).
//!
//! It goes while the box holds the focus (`:focus-within`). Its rule only wins if the hint's own
//! declarations leave `display` to the sheet: an inline `display: flex` outranks any rule, in
//! Chrome as in azul, so the cap stayed over the typed text.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{FocusChange, NodeHierarchyItemId, StyledDom},
};
use azul_css::{
    props::{
        layout::LayoutDisplay,
        property::{CssProperty, CssPropertyType},
    },
    AzString, StringVec,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        address_bar::{AddressBar, SEARCH_HINT_CLASS},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SEARCH_CLASS: &str = "__azul-native-address-bar-search";

/// The bar with a search shortcut and an empty search, in `theme`.
fn bar(theme: UiTheme) -> Dom {
    AddressBar::create(StringVec::from_vec(vec![
        AzString::from("Home"),
        AzString::from("Documents"),
    ]))
    .with_search_shortcut(AzString::from("\u{2318}F"))
    .with_search_placeholder(AzString::from("Search Documents"))
    .with_theme(theme)
    .dom()
}

/// The bar laid out across the top of an 800 x 600 window.
fn lay_out(theme: UiTheme) -> LayoutWindow {
    let mut body = Dom::create_body().with_child(bar(theme));
    body.root.set_css(
        "margin: 0px; width: 800px; height: 600px; display: flex; flex-direction: column;",
    );
    let styled = StyledDom::create_from_dom(body);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .expect("the bar lays out");
    lw
}

fn rect_of_class(lw: &LayoutWindow, class: &str) -> LogicalRect {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|n| {
            n.get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, IdOrClass::Class(name) if name.as_str() == class))
        })
        .unwrap_or_else(|| panic!("no .{class}"));
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .unwrap_or_else(|| panic!(".{class} is laid out"))
}

#[test]
fn the_search_hint_lies_inside_the_address_bars_search_box_in_every_theme() {
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        let lw = lay_out(theme);
        let search = rect_of_class(&lw, SEARCH_CLASS);
        let hint = rect_of_class(&lw, SEARCH_HINT_CLASS);
        let inside = hint.origin.x >= search.origin.x - 0.5
            && hint.origin.y >= search.origin.y - 0.5
            && hint.origin.x + hint.size.width <= search.origin.x + search.size.width + 0.5
            && hint.origin.y + hint.size.height <= search.origin.y + search.size.height + 0.5;
        assert!(
            inside,
            "{theme:?}: the hint {hint:?} lies outside the search box {search:?}"
        );
        assert!(
            (hint.size.height - search.size.height).abs() < 0.5,
            "{theme:?}: the hint spans the search box's height ({hint:?} in {search:?})"
        );
    }
}

fn classed(sd: &StyledDom, class: &str) -> NodeId {
    let nodes = sd.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .find(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .unwrap_or_else(|| panic!("no .{class}"))
}

fn display(sd: &StyledDom, node: NodeId) -> Option<LayoutDisplay> {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    match cache.get_property(node_data, &node, state, &CssPropertyType::Display) {
        Some(CssProperty::Display(v)) => v.get_property().copied(),
        _ => None,
    }
}

#[test]
fn the_search_hint_hides_while_the_search_box_holds_the_focus() {
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        let mut dom = Dom::create_body().with_child(bar(theme));
        let mut sd = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let hint = classed(&sd, SEARCH_HINT_CLASS);
        let field = classed(&sd, "__azul-native-text-input-container");
        assert_ne!(
            display(&sd, hint),
            Some(LayoutDisplay::None),
            "{theme:?}: the hint shows on an empty box at rest"
        );
        let _ = sd.restyle_on_state_change(
            Some(FocusChange {
                lost_focus: None,
                gained_focus: Some(field),
            }),
            None,
            None,
        );
        assert_eq!(
            display(&sd, hint),
            Some(LayoutDisplay::None),
            "{theme:?}: the hint is gone while the field has the focus"
        );
        let _ = sd.restyle_on_state_change(
            Some(FocusChange {
                lost_focus: Some(field),
                gained_focus: None,
            }),
            None,
            None,
        );
        assert_ne!(
            display(&sd, hint),
            Some(LayoutDisplay::None),
            "{theme:?}: the hint is back once the focus left"
        );
    }
}
