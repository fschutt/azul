//! A ribbon of tab buttons (`TabStyle::Pills`) is one leather area, from
//! the top of the window through its content, closed by a golden line:
//!
//! - in the title bar the row is 28px tall, the first button 74px in (14px
//!   after macOS's last traffic light), every button 22px tall, centred
//!   14px from the top where the lights are centred, 6px from the next;
//! - the open content is 106px tall and the 2px golden line lies under it;
//!   closed (a reveal at rest) the content has no height, the line sits
//!   right under the row, and no tab is drawn selected;
//! - locked open, the shown tab is drawn selected and a chevron at the
//!   row's end closes it;
//! - on the leather an enabled command wears the gold edge and a disabled
//!   one a neutral edge, its quiet face opaque.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::props::{
    basic::color::ColorU,
    property::{CssProperty, CssPropertyType},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        ribbon::{
            Ribbon, RibbonAppButton, RibbonButton, RibbonGroup, RibbonItem, RibbonTab,
            RibbonTabVec,
        },
        tab_reveal::{RevealEvent, TabClickBehaviour, TabReveal},
        tabs::TabStyle,
        themes::UiTheme,
        titlebar::TabsInTitlebar,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const TAB: &str = "__azul-native-ribbon-tab";
const TAB_ACTIVE: &str = "__azul-native-ribbon-tab-active";
const APP_BUTTON: &str = "__azul-native-ribbon-appbutton";
const TAB_BAR: &str = "__azul-native-ribbon-tabbar";
const CONTENT: &str = "__azul-native-ribbon-content";
const RULE: &str = "__azul-native-ribbon-rule";
const CHEVRON: &str = "__azul-native-ribbon-chevron";
const DISABLED: &str = "__azul-native-ribbon-button-disabled";

fn home() -> RibbonTab {
    let group = RibbonGroup::new("Clipboard".into()).with_items(
        vec![
            RibbonItem::LargeButton(RibbonButton::new("content_copy".into(), "Copy".into())),
            RibbonItem::LargeButton(
                RibbonButton::new("content_paste".into(), "Paste".into())
                    .with_disabled("Nothing to paste".into()),
            ),
        ]
        .into(),
    );
    RibbonTab::new("HOME".into()).with_group(group)
}

fn ribbon() -> Ribbon {
    Ribbon::new(RibbonTabVec::from_vec(vec![
        home(),
        RibbonTab::new("SHARE".into()),
        RibbonTab::new("VIEW".into()),
    ]))
    .with_app_button(RibbonAppButton::new("FILE".into()))
    .with_theme(UiTheme::Flora)
    .with_tab_style(TabStyle::Pills)
    .with_tabs_in_titlebar(TabsInTitlebar::create(0.0, 74.0, 0.0))
    .with_available_width(1280.0)
}

fn laid_out(dom: Dom) -> LayoutWindow {
    let body = Dom::create_body().with_css("margin: 0px;").with_child(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(1280.0, 400.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(body),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the ribbon lays out");
    lw
}

fn nodes_with(lw: &LayoutWindow, class: &str) -> Vec<NodeId> {
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let nodes = result.styled_dom.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .filter(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .collect()
}

fn rects_of(lw: &LayoutWindow, class: &str) -> Vec<LogicalRect> {
    let mut rects: Vec<LogicalRect> = nodes_with(lw, class)
        .into_iter()
        .filter_map(|n| {
            lw.get_node_layout_rect(DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(n)),
            })
        })
        .collect();
    rects.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    rects
}

/// The application button and the tabs, left to right.
fn row(lw: &LayoutWindow) -> Vec<LogicalRect> {
    let mut v = rects_of(lw, APP_BUTTON);
    v.extend(rects_of(lw, TAB));
    v.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    v
}

#[test]
fn in_the_title_bar_the_buttons_start_74px_in_and_are_centred_on_the_lights() {
    let lw = laid_out(ribbon().dom_desktop());
    let bar = rects_of(&lw, TAB_BAR)[0];
    assert!(bar.origin.y.abs() < 0.5, "the row is the top of the window: {bar:?}");
    assert!((bar.size.height - 28.0).abs() < 0.5, "a 28px row: {bar:?}");
    let row = row(&lw);
    assert_eq!(row.len(), 4, "FILE and three tabs");
    assert!((row[0].origin.x - 74.0).abs() < 0.5, "the first button at 74: {:?}", row[0]);
    for b in &row {
        assert!((b.size.height - 22.0).abs() < 0.01, "22px: {b:?}");
        let centre = b.origin.y + b.size.height / 2.0;
        assert!((centre - 14.0).abs() < 0.5, "centred on the lights at 14: {b:?}");
    }
    for w in row.windows(2) {
        let gap = w[1].origin.x - (w[0].origin.x + w[0].size.width);
        assert!((gap - 6.0).abs() < 0.5, "6px apart, not {gap}");
    }
}

#[test]
fn the_golden_line_lies_under_the_open_content() {
    let lw = laid_out(ribbon().dom_desktop());
    let content = rects_of(&lw, CONTENT)[0];
    assert!((content.origin.y - 28.0).abs() < 0.5, "under the row: {content:?}");
    assert!((content.size.height - 106.0).abs() < 0.5, "106px open: {content:?}");
    let rule = rects_of(&lw, RULE);
    assert_eq!(rule.len(), 1, "one golden line");
    assert!((rule[0].size.height - 2.0).abs() < 0.01, "2px: {:?}", rule[0]);
    assert!(
        (rule[0].origin.y - (content.origin.y + content.size.height)).abs() < 0.5,
        "right under the content: {:?}",
        rule[0]
    );
    assert!((rule[0].size.width - 1280.0).abs() < 0.5, "the window's width");
}

#[test]
fn at_rest_a_revealing_ribbon_is_its_row_and_the_line_with_no_tab_selected() {
    let reveal = TabReveal::create(TabClickBehaviour::HoverRevealDoubleClickLock);
    let lw = laid_out(ribbon().with_reveal(reveal).dom_desktop());
    let content = rects_of(&lw, CONTENT)[0];
    assert!(content.size.height.abs() < 0.5, "closed, no height: {content:?}");
    let rule = rects_of(&lw, RULE)[0];
    assert!((rule.origin.y - 28.0).abs() < 0.5, "the line right under the row: {rule:?}");
    assert!(nodes_with(&lw, TAB_ACTIVE).is_empty(), "no tab is drawn selected");
    assert_eq!(nodes_with(&lw, CHEVRON).len(), 1, "a chevron to lock the content open");
}

#[test]
fn locked_open_the_shown_tab_is_selected_and_the_content_has_its_height() {
    let reveal = TabReveal::create(TabClickBehaviour::HoverRevealDoubleClickLock);
    let _ = reveal.apply(RevealEvent::Chevron, 0);
    assert!(reveal.is_locked(), "premise: locked open");
    let lw = laid_out(ribbon().with_reveal(reveal).dom_desktop());
    assert_eq!(nodes_with(&lw, TAB_ACTIVE).len(), 1, "the shown tab is drawn selected");
    let content = rects_of(&lw, CONTENT)[0];
    assert!((content.size.height - 106.0).abs() < 0.5, "open: {content:?}");
}

/// The resting value of `ty` on `node` after the cascade.
fn resting(sd: &StyledDom, node: NodeId, ty: CssPropertyType) -> Option<CssProperty> {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    cache.get_property(node_data, &node, state, &ty).cloned()
}

fn top_edge(sd: &StyledDom, node: NodeId) -> Option<ColorU> {
    match resting(sd, node, CssPropertyType::BorderTopColor) {
        Some(CssProperty::BorderTopColor(v)) => v.get_property().map(|c| c.inner),
        _ => None,
    }
}

#[test]
fn on_the_leather_an_enabled_command_is_gold_edged_and_a_disabled_one_quiet_and_opaque() {
    let mut dom = Dom::create_body().with_child(ribbon().dom_desktop());
    let sd = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let nodes = sd.node_data.as_container();
    let buttons: Vec<NodeId> = (0..nodes.len())
        .map(NodeId::new)
        .filter(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| c.as_class() == Some("__azul-native-button"))
        })
        .collect();
    let (disabled, enabled): (Vec<NodeId>, Vec<NodeId>) = buttons.into_iter().partition(|n| {
        nodes[*n]
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| c.as_class() == Some(DISABLED))
    });
    assert_eq!((enabled.len(), disabled.len()), (1, 1), "Copy and the disabled Paste");
    assert_eq!(
        top_edge(&sd, enabled[0]),
        Some(ColorU::rgb(0xDC, 0xC1, 0x7C)),
        "the gold edge, lightest on top"
    );
    assert_eq!(
        top_edge(&sd, disabled[0]),
        Some(ColorU::new(150, 144, 130, 115)),
        "a neutral edge, no gold"
    );
    let opacity = match resting(&sd, disabled[0], CssPropertyType::Opacity) {
        Some(CssProperty::Opacity(v)) => v.get_property().map(|o| o.inner.normalized()),
        _ => None,
    };
    assert!(
        opacity.is_none_or(|o| (o - 1.0).abs() < 0.001),
        "the quiet face is opaque, not dimmed: {opacity:?}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn on_macos_a_row_of_buttons_starts_14px_after_the_last_traffic_light() {
    let row = TabsInTitlebar::button_row();
    assert_eq!((row.top, row.left), (0.0, 74.0));
}
