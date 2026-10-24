//! A tab bar built with a reveal (`TabHeader::with_reveal`) draws no tab
//! selected while its panel is closed and the shown tab while it is open;
//! its panel (`TabContent::with_reveal`) closes to no height, and both are
//! one hover area: each carries the reveal's enter and leave callbacks.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::{AzString, StringVec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        tab_reveal::{RevealEvent, TabClickBehaviour, TabReveal},
        tabs::{TabContent, TabHeader},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const ACTIVE: &str = "__azul-native-tabs-tab-active";

fn labels() -> StringVec {
    StringVec::from_vec(["Home", "Share", "View"].iter().map(|l| AzString::from(*l)).collect())
}

fn has_class(dom: &Dom, class: &str) -> bool {
    dom.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| c.as_class() == Some(class))
}

fn listens(dom: &Dom, filter: HoverEventFilter) -> bool {
    dom.root
        .get_callbacks()
        .as_ref()
        .iter()
        .any(|c| c.event == EventFilter::Hover(filter))
}

fn bar(reveal: &TabReveal) -> Dom {
    TabHeader::create(labels())
        .with_active_tab(1)
        .with_theme(UiTheme::Flora)
        .with_reveal(reveal.clone())
        .dom()
}

#[test]
fn closed_no_tab_is_selected_and_open_the_shown_one_is() {
    let reveal = TabReveal::create(TabClickBehaviour::HoverRevealDoubleClickLock);
    let closed = bar(&reveal);
    assert!(
        !closed.children.as_ref().iter().any(|c| has_class(c, ACTIVE)),
        "at rest the panel is closed: no tab is drawn selected"
    );
    let _ = reveal.apply(RevealEvent::Chevron, 0);
    let open = bar(&reveal);
    let selected: Vec<usize> = open
        .children
        .as_ref()
        .iter()
        .filter(|c| has_class(c, "__azul-native-tabs-tab-active") || has_class(c, "__azul-native-tabs-tab-not-active"))
        .enumerate()
        .filter(|(_, c)| has_class(c, ACTIVE))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(selected, vec![1], "open on the app's active tab");
}

#[test]
fn the_bar_and_its_panel_are_one_hover_area() {
    let reveal = TabReveal::create(TabClickBehaviour::HoverRevealDoubleClickLock);
    let header = bar(&reveal);
    let panel = TabContent::new(Dom::create_div()).with_reveal(reveal).dom();
    for (name, dom) in [("the bar", &header), ("the panel", &panel)] {
        assert!(listens(dom, HoverEventFilter::MouseEnter), "{name} hears the pointer come");
        assert!(listens(dom, HoverEventFilter::MouseLeave), "{name} hears it go");
    }
}

#[test]
fn a_closed_panel_has_no_height() {
    let reveal = TabReveal::create(TabClickBehaviour::HoverRevealDoubleClickLock);
    let panel = TabContent::new(
        Dom::create_div().with_css("height: 80px;").with_ids_and_classes(
            azul_core::dom::IdOrClassVec::from_vec(vec![azul_core::dom::IdOrClass::Class(
                "inside".into(),
            )]),
        ),
    )
    .with_reveal(reveal.clone())
    .with_theme(UiTheme::Flora)
    .dom()
    .with_ids_and_classes(azul_core::dom::IdOrClassVec::from_vec(vec![
        azul_core::dom::IdOrClass::Class("panel".into()),
    ]));
    let body = Dom::create_body().with_css("margin: 0px;").with_child(panel);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(body),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the panel lays out");
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let nodes = result.styled_dom.node_data.as_container();
    let panel = (0..nodes.len())
        .map(NodeId::new)
        .find(|n| nodes[*n].get_ids_and_classes().as_ref().iter().any(|c| c.as_class() == Some("panel")))
        .expect("the panel");
    let rect = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(panel)),
        })
        .expect("laid out");
    assert!(rect.size.height.abs() < 0.5, "closed, the panel has no height: {rect:?}");
}
