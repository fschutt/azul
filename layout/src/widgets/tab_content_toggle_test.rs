//! The chevron of the tab strip: one per window, up while the content shows, down while it is folded;
//! a panel with the toggle shows nothing while it is folded.

use azul_core::{
    a11y::AccessibilityState,
    callbacks::WindowSizeScope,
    dom::{Dom, IdOrClass, NodeType},
    geom::LogicalSize,
};
use azul_css::{
    props::{
        layout::LayoutDisplay,
        property::{CssProperty, LayoutDisplayValue},
    },
    AzString, StringVec,
};

use super::{chevron, CONTENT_TOGGLE_CLASS};
use crate::widgets::{
    tab_strip::StatusBarBand,
    tabs::{TabContent, TabHeader, TabStyle},
    themes::UiTheme,
};

fn labels(n: usize) -> StringVec {
    StringVec::from_vec((0..n).map(|i| AzString::from(format!("Tab {i}"))).collect())
}

fn has_class(dom: &Dom, class: &str) -> bool {
    dom.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| matches!(c, IdOrClass::Class(name) if name.as_str() == class))
}

/// How many nodes of `dom` are the chevron.
fn chevrons_in(dom: &Dom) -> usize {
    usize::from(has_class(dom, CONTENT_TOGGLE_CLASS)) + dom.children.as_ref().iter().map(chevrons_in).sum::<usize>()
}

fn window() -> LogicalSize {
    LogicalSize { width: 411.0, height: 914.0 }
}

fn bar(style: TabStyle) -> TabHeader {
    TabHeader::create(labels(4)).with_style(style).with_theme(UiTheme::Flora).with_content_toggle(true)
}

#[test]
fn a_bar_that_asks_for_the_chevron_draws_it_in_every_style_and_one_that_does_not_draws_none() {
    for style in [TabStyle::Rectangular, TabStyle::Pills, TabStyle::TouchPills] {
        assert_eq!(chevrons_in(&bar(style).dom()), 1, "{style:?}");
        let plain = TabHeader::create(labels(4)).with_style(style).with_theme(UiTheme::Flora);
        assert_eq!(chevrons_in(&plain.dom()), 0, "{style:?}: not asked for");
    }
}

#[test]
fn a_window_has_exactly_one_chevron_whatever_its_page_holds() {
    let _scope = WindowSizeScope::enter(window());
    let first = bar(TabStyle::Pills).dom();
    let second = bar(TabStyle::TouchPills).dom();
    let third = bar(TabStyle::Rectangular).dom();
    assert_eq!(
        chevrons_in(&first) + chevrons_in(&second) + chevrons_in(&third),
        1,
        "the first bar of the build has it"
    );
    assert_eq!(chevrons_in(&first), 1, "and it is the first");
    // A build of its own starts again.
    drop(_scope);
    let _scope = WindowSizeScope::enter(window());
    assert_eq!(chevrons_in(&bar(TabStyle::Pills).dom()), 1, "the next build has its own chevron");
}

#[test]
fn the_chevron_points_up_while_the_content_shows_and_down_while_it_is_folded() {
    for (hidden, icon, state, name) in [
        (false, "expand_less", AccessibilityState::Expanded, "Hide the tab content"),
        (true, "expand_more", AccessibilityState::Collapsed, "Show the tab content"),
    ] {
        let node = chevron(hidden, 28.0);
        let glyph = &node.children.as_ref()[0];
        assert!(
            matches!(glyph.root.get_node_type(), NodeType::Icon(name) if name.as_str() == icon),
            "hidden={hidden}: the {icon} glyph"
        );
        let info = node.root.get_accessibility_info().expect("the chevron is named");
        assert_eq!(info.accessibility_name.as_ref().map(AzString::as_str), Some(name));
        assert!(info.states.as_ref().contains(&state), "hidden={hidden}: {state:?}");
        assert!(node.root.get_tab_index().is_some(), "it takes the keyboard");
        assert_eq!(node.root.get_callbacks().as_ref().len(), 2, "a click, and Enter or Space");
    }
}

#[test]
fn the_chevron_of_the_build_is_drawn_as_the_windows_flag_says() {
    let hidden_icon = |hidden: bool| {
        let _scope = WindowSizeScope::enter_with(window(), hidden);
        let dom = bar(TabStyle::Pills).dom();
        let found = dom
            .children
            .as_ref()
            .iter()
            .find(|c| has_class(c, CONTENT_TOGGLE_CLASS))
            .map(|c| matches!(c.children.as_ref()[0].root.get_node_type(), NodeType::Icon(n) if n.as_str() == "expand_more"))
            .expect("the chevron is a child of the row");
        found
    };
    assert!(!hidden_icon(false), "shown: up");
    assert!(hidden_icon(true), "folded: down");
}

#[test]
fn in_a_row_the_chevron_is_the_first_node_and_on_a_phone_it_makes_room_for_itself() {
    let dom = bar(TabStyle::Pills).dom();
    assert!(has_class(&dom.children.as_ref()[0], CONTENT_TOGGLE_CLASS), "first in the row");
    // The strip in the band: the first lane begins at the window's edge, the chevron stands there, and
    // the pills start after it.
    let band = StatusBarBand::from_display(
        &azul_core::display_features::DisplayFeatures::simulate(
            azul_core::display_features::DisplayPreset::Pixel8,
            window(),
        ),
        &azul_core::display_features::DisplayPreset::Pixel8.safe_area(window()),
        window(),
    );
    let with = bar(TabStyle::TouchPills).with_status_bar(band.clone()).dom();
    let without = TabHeader::create(labels(4))
        .with_style(TabStyle::TouchPills)
        .with_theme(UiTheme::Flora)
        .with_status_bar(band)
        .dom();
    assert_eq!(chevrons_in(&with), 1);
    assert_eq!(
        with.children.as_ref().len(),
        without.children.as_ref().len() + 2,
        "two nodes more in the header: a stretch of leather under the chevron, and the chevron"
    );
}

fn displays_nothing(panel: &Dom) -> bool {
    panel
        .root
        .style
        .iter_inline_properties()
        .any(|(prop, _)| matches!(prop, CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::None))))
}

#[test]
fn a_panel_with_the_toggle_shows_nothing_while_the_content_is_folded() {
    let panel = |toggle: bool| TabContent::new(Dom::create_p_with_text("Home panel")).with_content_toggle(toggle).dom();
    {
        let _scope = WindowSizeScope::enter_with(window(), true);
        assert!(displays_nothing(&panel(true)), "folded: nothing");
        assert!(!displays_nothing(&panel(false)), "a panel that did not ask keeps showing");
    }
    {
        let _scope = WindowSizeScope::enter_with(window(), false);
        assert!(!displays_nothing(&panel(true)), "shown: it shows");
    }
    // Outside a build there is no window to fold for.
    assert!(!displays_nothing(&panel(true)));
}

#[test]
fn a_panel_leaves_room_at_its_left_for_a_rail() {
    use azul_css::props::{layout::LayoutMarginLeft, property::CssProperty as P};
    let margin_left = |inset: f32| {
        TabContent::new(Dom::create_p_with_text("x")).with_content_inset(inset).dom().root.style.iter_inline_properties().find_map(
            |(prop, _)| match prop {
                P::MarginLeft(value) => value.get_property().map(|m: &LayoutMarginLeft| m.inner.to_pixels_internal(0.0, 16.0, 16.0)),
                _ => None,
            },
        )
    };
    assert_eq!(margin_left(0.0), None, "no inset, no margin");
    assert_eq!(margin_left(18.0), Some(18.0), "the rail's width");
}
