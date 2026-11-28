//! The band under the tab row: one height for the band of every tab (a tab with a location
//! included), a band that scrolls sideways without a scrollbar where its groups do not fit - with a
//! fade at each clipped end - and a phone's pills as icons.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    a11y::{AccessibilityRole, AccessibilityState},
    display_features::{DisplayFeatures, DisplayPreset},
    dom::{ComponentEventFilter, Dom, EventFilter, IdOrClass::Class, NodeType},
    geom::LogicalSize,
};
use azul_css::{
    props::{
        layout::{LayoutHeight, LayoutOverflow},
        property::CssProperty,
        style::LayoutScrollbarWidth,
    },
    AzString,
};

use super::*;
use crate::widgets::{
    tab_flip::TabFlip,
    tab_strip::StatusBarBand,
    tabs::{TabLabels, TabStyle, ICON_PILL_CLASS, TAB_BAND_PX},
};

const CONTENT: &str = "__azul-native-ribbon-content";
const TAB_BAR: &str = "__azul-native-ribbon-tabbar";
const TAB: &str = "__azul-native-ribbon-tab";
const GROUP: &str = "__azul-native-ribbon-group";

fn s(text: &str) -> AzString {
    AzString::from(text)
}

fn has_class(node: &Dom, name: &str) -> bool {
    node.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| matches!(c, Class(c) if c.as_str() == name))
}

fn child_with<'a>(dom: &'a Dom, class: &str) -> &'a Dom {
    dom.children
        .as_ref()
        .iter()
        .find(|c| has_class(c, class))
        .unwrap_or_else(|| panic!("a child with the class {class}"))
}

extern "C" fn on_tab(_: RefAny, _: CallbackInfo, _: usize) -> Update {
    Update::DoNothing
}

/// A group of `n` large buttons: each a few dozen px wide.
fn group(label: &str, n: usize) -> RibbonGroup {
    let mut group = RibbonGroup::new(s(label));
    for i in 0..n {
        group.add_item(RibbonItem::LargeButton(RibbonButton::new(
            s("tune"),
            AzString::from(format!("Command {i}")),
        )));
    }
    group
}

/// File (a location), Home (groups), Empty (nothing): the band of each.
fn three_tabs() -> RibbonTabVec {
    RibbonTabVec::from_vec(vec![
        RibbonTab::new(s("File")).with_location(Dom::create_div().with_ids_and_classes(
            azul_core::dom::IdOrClassVec::from_vec(vec![Class(s("test-location"))]),
        )),
        RibbonTab::new(s("Home")).with_group(group("Clipboard", 2)).with_group(group("Font", 3)),
        RibbonTab::new(s("Empty")),
    ])
}

fn ribbon(style: TabStyle, theme: UiTheme, active: usize) -> Ribbon {
    Ribbon::new(three_tabs())
        .with_tab_style(style)
        .with_theme(theme)
        .with_active_tab(active)
}

/// The last unconditional declaration of `kind` a node has, as the node's own.
fn declared(node: &Dom) -> Vec<CssProperty> {
    crate::widgets::theme_probe::unconditional(node)
}

fn height_of(node: &Dom) -> Option<isize> {
    declared(node).iter().rev().find_map(|p| match p {
        CssProperty::Height(v) => v.get_property().and_then(|h| match h {
            LayoutHeight::Px(px) => Some(px.number.get() as isize),
            _ => None,
        }),
        _ => None,
    })
}

fn pixel_8_band() -> StatusBarBand {
    let size = LogicalSize {
        width: 411.0,
        height: 914.0,
    };
    StatusBarBand::from_display(
        &DisplayFeatures::simulate(DisplayPreset::Pixel8, size),
        &DisplayPreset::Pixel8.safe_area(size),
        size,
    )
}

// ---- one height ----

#[test]
fn the_band_of_every_tab_is_exactly_one_height_in_every_look() {
    for style in [
        TabStyle::Auto,
        TabStyle::Pills,
        TabStyle::Rectangular,
        TabStyle::TouchPills,
    ] {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            for active in 0..3 {
                let dom = ribbon(style, theme, active).dom_desktop();
                let band = child_with(&dom, CONTENT);
                assert_eq!(
                    height_of(band),
                    Some(TAB_BAND_PX),
                    "{style:?} / {theme:?} / tab {active}: a flip must not move what is below"
                );
            }
        }
    }
}

#[test]
fn the_one_height_is_the_same_in_the_adaptive_and_the_phone_chromes_too() {
    for active in 0..3 {
        let adaptive = ribbon(TabStyle::Pills, UiTheme::Flat, active).dom();
        assert_eq!(height_of(child_with(&adaptive, CONTENT)), Some(TAB_BAND_PX), "adaptive, tab {active}");
    }
}

#[test]
fn a_band_the_app_styled_itself_stays_the_apps() {
    // AzMail hides its band behind the backstage with a style of its own.
    let mut ribbon = ribbon(TabStyle::Rectangular, UiTheme::Flat, 1);
    ribbon.style.content_style = azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(
        azul_css::dynamic_selector::CssPropertyWithConditionsVec::from_vec(vec![
            azul_css::dynamic_selector::CssPropertyWithConditions::simple(CssProperty::const_height(
                LayoutHeight::const_px(0),
            )),
        ]),
    );
    let dom = ribbon.dom_desktop();
    assert_eq!(height_of(child_with(&dom, CONTENT)), Some(0));
}

// ---- the location ----

#[test]
fn a_tab_with_a_location_shows_it_in_the_band_instead_of_groups() {
    let dom = ribbon(TabStyle::Pills, UiTheme::Flat, 0).dom_desktop();
    let band = child_with(&dom, CONTENT);
    let kids = band.children.as_ref();
    assert_eq!(kids.len(), 1, "the location, nothing else");
    assert!(has_class(&kids[0], RIBBON_LOCATION_CLASS));
    assert!(has_class(&kids[0].children.as_ref()[0], "test-location"), "the app's block inside");
    assert!(kids.iter().all(|k| !has_class(k, GROUP)));
    // A tab of commands has its groups and no location.
    let home = ribbon(TabStyle::Pills, UiTheme::Flat, 1).dom_desktop();
    let groups = child_with(&home, CONTENT).children.as_ref();
    assert_eq!(groups.iter().filter(|k| has_class(k, GROUP)).count(), 2);
    assert!(groups.iter().all(|k| !has_class(k, RIBBON_LOCATION_CLASS)));
}

#[test]
fn a_tab_asks_for_no_location_and_no_icon_until_told() {
    let tab = RibbonTab::new(s("Home"));
    assert!(tab.location.is_none());
    assert!(tab.icon.as_str().is_empty());
    let located = RibbonTab::new(s("File")).with_location(Dom::create_div()).with_icon(s("folder"));
    assert!(located.location.is_some());
    assert_eq!(located.icon.as_str(), "folder");
    assert!(RibbonAppButton::new(s("FILE")).icon.as_str().is_empty());
    assert_eq!(RibbonAppButton::new(s("FILE")).with_icon(s("menu")).icon.as_str(), "menu");
}

#[test]
fn the_band_of_a_location_flips_like_the_band_of_commands() {
    let flipping = |active: usize| {
        ribbon(TabStyle::TouchPills, UiTheme::Flat, active)
            .with_on_tab_click(RefAny::new(1u8), on_tab as RibbonOnTabClickCallbackType)
            .dom_desktop()
    };
    for active in 0..2 {
        let dom = flipping(active);
        let band = child_with(&dom, CONTENT);
        // Touch start, move, end, cancel and the wheel: the same on the location as on the groups.
        assert_eq!(band.root.get_callbacks().as_ref().len(), 5, "tab {active}");
        assert!(band.root.get_dataset().is_some(), "tab {active}");
    }
    let off = ribbon(TabStyle::TouchPills, UiTheme::Flat, 0)
        .with_on_tab_click(RefAny::new(1u8), on_tab as RibbonOnTabClickCallbackType)
        .with_flip(TabFlip::Off)
        .dom_desktop();
    assert!(child_with(&off, CONTENT).root.get_callbacks().as_ref().is_empty());
}

// ---- the band scrolls sideways ----

#[test]
fn the_band_scrolls_sideways_without_a_scrollbar() {
    for style in [TabStyle::Pills, TabStyle::Rectangular, TabStyle::TouchPills] {
        let dom = ribbon(style, UiTheme::Flat, 1).dom_desktop();
        let band = child_with(&dom, CONTENT);
        let declared = declared(band);
        let last = |want: &dyn Fn(&CssProperty) -> bool| declared.iter().rev().find(|p| want(p)).cloned();
        assert!(
            matches!(
                last(&|p| matches!(p, CssProperty::OverflowX(_))),
                Some(CssProperty::OverflowX(v)) if v.get_property() == Some(&LayoutOverflow::Auto)
            ),
            "{style:?}: overflow-x"
        );
        assert!(
            matches!(
                last(&|p| matches!(p, CssProperty::OverflowY(_))),
                Some(CssProperty::OverflowY(v)) if v.get_property() == Some(&LayoutOverflow::Hidden)
            ),
            "{style:?}: overflow-y"
        );
        assert!(
            matches!(
                last(&|p| matches!(p, CssProperty::ScrollbarWidth(_))),
                Some(CssProperty::ScrollbarWidth(v)) if v.get_property() == Some(&LayoutScrollbarWidth::None)
            ),
            "{style:?}: no scrollbar"
        );
    }
}

#[test]
fn the_groups_of_the_band_never_shrink() {
    let dom = ribbon(TabStyle::Pills, UiTheme::Flat, 1).dom_desktop();
    let groups: Vec<&Dom> = child_with(&dom, CONTENT)
        .children
        .as_ref()
        .iter()
        .filter(|k| has_class(k, GROUP))
        .collect();
    assert_eq!(groups.len(), 2);
    for group in groups {
        assert!(
            declared(group).iter().any(|p| matches!(
                p,
                CssProperty::FlexShrink(v) if v.get_property().map(|shrink| shrink.inner.get()) == Some(0.0)
            )),
            "a group keeps its width and the band scrolls"
        );
    }
}

fn fades_of(band: &Dom) -> usize {
    band.children
        .as_ref()
        .iter()
        .filter(|k| has_class(k, RIBBON_BAND_FADE_CLASS))
        .count()
}

/// A ribbon whose one tab has `groups` groups of three large buttons.
fn crowded(groups: usize, style: TabStyle) -> Ribbon {
    let mut tab = RibbonTab::new(s("Home"));
    for i in 0..groups {
        tab.add_group(group(&format!("Group {i}"), 3));
    }
    Ribbon::new(RibbonTabVec::from_vec(vec![tab]))
        .with_tab_style(style)
        .with_theme(UiTheme::Flat)
}

#[test]
fn a_band_wider_than_its_room_has_a_fade_at_each_end_and_one_that_fits_has_none() {
    for style in [TabStyle::Pills, TabStyle::TouchPills] {
        // Eight groups in a 200 px window: even at the narrowest step of a desktop (every group one
        // button) they are wider than the room, and a phone's groups are not scaled at all.
        let narrow = crowded(8, style).with_available_width(200.0).dom_desktop();
        let band = child_with(&narrow, CONTENT);
        assert_eq!(fades_of(band), 2, "{style:?}");
        let kids = band.children.as_ref();
        assert!(has_class(&kids[0], "__azul-native-ribbon-band-fade-left"), "the first child is the left fade");
        assert!(
            has_class(kids.last().expect("a last child"), "__azul-native-ribbon-band-fade-right"),
            "the last child is the right fade"
        );
        // 5000 px: everything fits, nothing fades.
        let roomy = crowded(8, style).with_available_width(5000.0).dom_desktop();
        assert_eq!(fades_of(child_with(&roomy, CONTENT)), 0, "{style:?}");
    }
}

#[test]
fn a_desktop_band_that_fits_by_scaling_its_groups_down_does_not_fade() {
    // Two groups in 200 px: the groups give way (small buttons, icons, one button) and fit.
    let dom = crowded(2, TabStyle::Pills).with_available_width(200.0).dom_desktop();
    assert_eq!(fades_of(child_with(&dom, CONTENT)), 0);
}

#[test]
fn a_tab_with_a_location_or_no_groups_never_fades() {
    for active in [0, 2] {
        let dom = ribbon(TabStyle::Pills, UiTheme::Flat, active)
            .with_available_width(50.0)
            .dom_desktop();
        assert_eq!(fades_of(child_with(&dom, CONTENT)), 0, "tab {active}");
    }
}

#[test]
fn a_phone_never_collapses_a_group_its_band_scrolls_instead() {
    let collapsed = |dom: &Dom| {
        let mut count = 0;
        let mut stack = vec![dom];
        while let Some(node) = stack.pop() {
            if has_class(node, RIBBON_GROUP_COLLAPSED_CLASS) || has_class(node, RIBBON_GROUP_BUTTON_CLASS) {
                count += 1;
            }
            stack.extend(node.children.as_ref().iter());
        }
        count
    };
    let desktop = crowded(8, TabStyle::Pills).with_available_width(200.0).dom_desktop();
    assert!(collapsed(&desktop) > 0, "a desktop's groups give way at 200 px");
    let phone = crowded(8, TabStyle::TouchPills).with_available_width(200.0).dom_desktop();
    assert_eq!(collapsed(&phone), 0, "a phone's band scrolls: no group is collapsed");
    assert_eq!(fades_of(child_with(&phone, CONTENT)), 2);
}

// ---- icon pills in the ribbon ----

fn iconic(labels: TabLabels) -> Ribbon {
    Ribbon::new(RibbonTabVec::from_vec(vec![
        RibbonTab::new(s("File")).with_location(Dom::create_div()).with_icon(s("folder")),
        RibbonTab::new(s("Home")).with_group(group("Clipboard", 1)).with_icon(s("home")),
        RibbonTab::new(s("Share")).with_icon(s("share")),
        RibbonTab::new(s("View")).with_icon(s("view_sidebar")),
    ]))
    .with_tab_style(TabStyle::TouchPills)
    .with_theme(UiTheme::Flat)
    .with_labels(labels)
    .with_status_bar(pixel_8_band())
}

fn pills_of(bar: &Dom) -> Vec<&Dom> {
    bar.children
        .as_ref()
        .iter()
        .filter(|c| has_class(c, TAB))
        .collect()
}

fn tab_bar_of(dom: &Dom) -> &Dom {
    // In the status bar's band the row sits in a header with the leather and the covers.
    let header = &dom.children.as_ref()[0];
    if has_class(header, TAB_BAR) {
        header
    } else {
        child_with(header, TAB_BAR)
    }
}

#[test]
fn a_ribbon_asks_for_the_rule_by_default() {
    assert_eq!(Ribbon::new(three_tabs()).labels, TabLabels::Auto);
}

#[test]
fn on_a_phone_the_ribbons_tabs_are_icon_squares_with_their_names() {
    let dom = iconic(TabLabels::Auto).with_active_tab(1).dom_desktop();
    let pills = pills_of(tab_bar_of(&dom));
    assert_eq!(pills.len(), 4);
    for (i, pill) in pills.iter().enumerate() {
        assert!(has_class(pill, ICON_PILL_CLASS), "pill {i}");
        // The icon where the text was.
        let body = &pill.children.as_ref()[0];
        assert!(matches!(body.children.as_ref()[0].root.get_node_type(), NodeType::Icon(_)), "pill {i}");
        let info = pill.root.get_accessibility_info();
        // The tab's own role and name are the ribbon's: a screen reader says "Home".
        let info = info.expect("an icon pill is named");
        assert_eq!(info.role, AccessibilityRole::PageTab);
        assert_eq!(
            info.accessibility_name.as_ref().map(|n| n.as_str().to_string()),
            Some(["File", "Home", "Share", "View"][i].to_string())
        );
        let _ = AccessibilityState::Selected;
    }
}

#[test]
fn the_ribbons_icon_pills_stand_lane_by_lane_and_nothing_scrolls() {
    let dom = iconic(TabLabels::Auto).with_active_tab(1).dom_desktop();
    let bar = tab_bar_of(&dom);
    // No scroll-into-view marker in the selected pill, no resize listener on the row.
    for pill in pills_of(bar) {
        assert!(
            pill.children.as_ref().iter().all(|c| !has_class(c, "__azul-tabs-selected-marker")),
            "nothing to scroll into view"
        );
    }
    let heard = bar
        .root
        .get_callbacks()
        .as_ref()
        .iter()
        .filter(|cb| matches!(cb.event, EventFilter::Component(ComponentEventFilter::NodeResized)))
        .count();
    assert_eq!(heard, 0);
}

#[test]
fn a_ribbon_tab_without_an_icon_keeps_the_strip_on_text() {
    let mut ribbon = iconic(TabLabels::Auto);
    let mut tabs = ribbon.tabs.into_library_owned_vec();
    tabs[2].icon = s("");
    ribbon.tabs = RibbonTabVec::from_vec(tabs);
    let dom = ribbon.dom_desktop();
    for pill in pills_of(tab_bar_of(&dom)) {
        assert!(!has_class(pill, ICON_PILL_CLASS));
    }
}

#[test]
fn text_is_the_text_and_a_desktop_ignores_the_icons() {
    let text = iconic(TabLabels::Text).dom_desktop();
    assert!(pills_of(tab_bar_of(&text)).iter().all(|p| !has_class(p, ICON_PILL_CLASS)));
    for labels in [TabLabels::Auto, TabLabels::Icons] {
        let desktop = iconic(labels).with_tab_style(TabStyle::Pills).dom_desktop();
        let bar = child_with(&desktop, TAB_BAR);
        assert!(
            pills_of(bar).iter().all(|p| !has_class(p, ICON_PILL_CLASS)),
            "{labels:?}: a desktop's tabs show their text"
        );
    }
}

#[test]
fn the_application_button_is_the_first_icon_pill_where_it_names_an_icon() {
    let with_button = |icon: &str| {
        iconic(TabLabels::Auto)
            .with_app_button(RibbonAppButton::new(s("MENU")).with_icon(s(icon)))
            .dom_desktop()
    };
    let named = with_button("menu");
    let bar = tab_bar_of(&named);
    let button = bar
        .children
        .as_ref()
        .iter()
        .find(|c| has_class(c, "__azul-native-ribbon-appbutton"))
        .expect("the application button");
    assert!(has_class(button, ICON_PILL_CLASS));
    assert_eq!(pills_of(bar).len(), 4);
    // Without an icon the whole strip stays on text.
    let unnamed = with_button("");
    let bar = tab_bar_of(&unnamed);
    assert!(pills_of(bar).iter().all(|p| !has_class(p, ICON_PILL_CLASS)));
}
