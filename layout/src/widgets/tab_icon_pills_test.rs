//! The icon pills of a tab bar: on a phone every tab header is ONE ICON, every pill the same
//! square, standing lane by lane around the camera hole where they fit; a desktop keeps the text.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    a11y::{AccessibilityRole, AccessibilityState},
    display_features::{DisplayFeatures, DisplayPreset},
    dom::{ComponentEventFilter, Dom, EventFilter, IdOrClass, IdOrClass::Class, NodeType},
    geom::LogicalSize,
};
use azul_css::{
    props::{layout::LayoutHeight, layout::LayoutWidth, property::CssProperty},
    AzString, StringVec,
};

use super::*;
use crate::widgets::{
    tab_strip::{StatusBarBand, ICON_PILL_MAX, ICON_PILL_MIN},
    themes::UiTheme,
};

fn names(n: usize) -> StringVec {
    StringVec::from_vec((0..n).map(|i| AzString::from(format!("tab {i}"))).collect())
}

fn icons(n: usize) -> StringVec {
    StringVec::from_vec(
        ["home", "folder", "edit", "view_sidebar", "inbox", "search", "settings", "star"]
            .iter()
            .cycle()
            .take(n)
            .map(|name| AzString::from(*name))
            .collect(),
    )
}

fn classes_of(node: &Dom) -> Vec<String> {
    node.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .filter_map(|c| match c {
            Class(s) => Some(s.as_str().to_string()),
            IdOrClass::Id(_) => None,
        })
        .collect()
}

fn band_of(preset: DisplayPreset, width: f32, height: f32) -> StatusBarBand {
    let size = LogicalSize { width, height };
    StatusBarBand::from_display(
        &DisplayFeatures::simulate(preset, size),
        &preset.safe_area(size),
        size,
    )
}

/// A phone's tab bar of `n` tabs with an icon each, in the Pixel 8's band.
fn phone(n: usize) -> TabHeader {
    TabHeader::create(names(n))
        .with_icons(icons(n))
        .with_style(TabStyle::TouchPills)
        .with_theme(UiTheme::Flat)
        .with_status_bar(band_of(DisplayPreset::Pixel8, 411.0, 914.0))
}

fn strip_of(header: &Dom) -> &Dom {
    header
        .children
        .as_ref()
        .iter()
        .find(|c| classes_of(c).iter().any(|c| c == "__azul-native-tabs-touch-strip"))
        .expect("a touch header holds its strip")
}

fn pills_of(strip: &Dom) -> Vec<&Dom> {
    strip
        .children
        .as_ref()
        .iter()
        .filter(|n| {
            let c = classes_of(n);
            c.iter().any(|c| c == TAB_ACTIVE_CLASS_NAME || c == TAB_NOT_ACTIVE_CLASS_NAME)
        })
        .collect()
}

/// The last unconditional `width` / `height` a node declares inline, in px.
fn side_of(node: &Dom, height: bool) -> Option<f32> {
    crate::widgets::theme_probe::unconditional(node)
        .iter()
        .rev()
        .find_map(|p| match (p, height) {
            (CssProperty::Width(v), false) => v.get_property().and_then(|w| match w {
                LayoutWidth::Px(px) => Some(px.number.get()),
                _ => None,
            }),
            (CssProperty::Height(v), true) => v.get_property().and_then(|h| match h {
                LayoutHeight::Px(px) => Some(px.number.get()),
                _ => None,
            }),
            _ => None,
        })
}

fn is_icon(node: &Dom, name: &str) -> bool {
    match node.root.get_node_type() {
        NodeType::Icon(icon) => icon.as_str() == name,
        _ => false,
    }
}

#[test]
fn a_tab_bar_asks_for_the_rule_by_default_and_names_no_icons() {
    let header = TabHeader::create(names(3));
    assert_eq!(header.labels, TabLabels::Auto);
    assert!(header.icons.as_ref().is_empty());
    assert_eq!(TabLabels::default(), TabLabels::Auto);
}

#[test]
fn on_a_phone_every_pill_is_one_icon_and_a_square_of_one_size() {
    let dom = phone(6).with_active_tab(1).dom();
    let pills = pills_of(strip_of(&dom));
    assert_eq!(pills.len(), 6);
    let sides: Vec<f32> = pills.iter().map(|p| side_of(p, false).expect("a width")).collect();
    for (i, pill) in pills.iter().enumerate() {
        assert!(classes_of(pill).contains(&ICON_PILL_CLASS.to_string()), "pill {i}");
        // The pill is the <p> a label would stand in, holding the icon of the tab in its place: no
        // text.
        assert!(matches!(pill.root.get_node_type(), NodeType::P), "pill {i}");
        assert!(is_icon(&pill.children.as_ref()[0], icons(6).as_ref()[i].as_str()), "pill {i}");
        // A square, the same as every other.
        assert_eq!(side_of(pill, true), side_of(pill, false), "pill {i} is a square");
        assert_eq!(sides[i], sides[0], "every pill is the same size");
        assert!(sides[i] >= ICON_PILL_MIN && sides[i] <= ICON_PILL_MAX, "{}", sides[i]);
    }
}

#[test]
fn an_icon_pill_keeps_the_name_of_its_tab_for_a_screen_reader() {
    let dom = phone(4).with_active_tab(2).dom();
    for (i, pill) in pills_of(strip_of(&dom)).iter().enumerate() {
        let info = pill.root.get_accessibility_info().expect("a tab has accessibility info");
        assert_eq!(info.role, AccessibilityRole::PageTab);
        assert_eq!(
            info.accessibility_name.as_ref().map(|n| n.as_str().to_string()),
            Some(format!("tab {i}")),
            "the label is the name"
        );
        assert_eq!(info.states.as_ref().contains(&AccessibilityState::Selected), i == 2);
    }
}

#[test]
fn six_pills_in_the_pixel_8_band_stand_lane_by_lane_and_nothing_scrolls() {
    let dom = phone(6).with_active_tab(4).dom();
    let strip = strip_of(&dom);
    let children = strip.children.as_ref();
    // The room before the first, 6 pills, the room the camera hole takes between the third and
    // the fourth: no spacer after the last.
    assert_eq!(children.len(), 1 + 6 + 1, "{:?}", children.iter().map(classes_of).collect::<Vec<_>>());
    assert_eq!(pills_of(strip).len(), 6);
    let spacers: Vec<usize> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| classes_of(c).is_empty())
        .map(|(i, _)| i)
        .collect();
    assert_eq!(spacers, vec![0, 4], "before the first pill and at the camera hole");
    // Nothing to scroll into view: no marker, no resize listener.
    for pill in pills_of(strip) {
        assert_eq!(pill.children.as_ref().len(), 2, "the icon and the hit area, no marker");
    }
    let heard = strip
        .root
        .get_callbacks()
        .as_ref()
        .iter()
        .filter(|cb| matches!(cb.event, EventFilter::Component(ComponentEventFilter::NodeResized)))
        .count();
    assert_eq!(heard, 0);
}

#[test]
fn twelve_pills_do_not_fit_the_lanes_and_scroll_as_a_row_of_the_smallest_squares() {
    let dom = phone(12).with_active_tab(3).dom();
    let strip = strip_of(&dom);
    let pills = pills_of(strip);
    assert_eq!(pills.len(), 12);
    for pill in &pills {
        assert_eq!(side_of(pill, false), Some(ICON_PILL_MIN));
        assert_eq!(side_of(pill, true), Some(ICON_PILL_MIN));
    }
    // The plain row: a spacer at both ends, the marker in the selected pill, the resize listener.
    assert_eq!(strip.children.as_ref().len(), 12 + 2);
    assert_eq!(pills[3].children.as_ref().len(), 3, "icon, hit area, marker");
    let heard = strip
        .root
        .get_callbacks()
        .as_ref()
        .iter()
        .filter(|cb| matches!(cb.event, EventFilter::Component(ComponentEventFilter::NodeResized)))
        .count();
    assert_eq!(heard, 1);
}

#[test]
fn without_a_band_the_icon_pills_are_a_plain_row_of_equal_squares() {
    let dom = TabHeader::create(names(5))
        .with_icons(icons(5))
        .with_style(TabStyle::TouchPills)
        .with_theme(UiTheme::Flat)
        .with_active_tab(0)
        .dom();
    let strip = strip_of(&dom);
    let pills = pills_of(strip);
    assert_eq!(pills.len(), 5);
    for pill in &pills {
        // Outside a window's build nothing says narrow: the widest square.
        assert_eq!(side_of(pill, false), Some(ICON_PILL_MAX));
        assert!(classes_of(pill).contains(&ICON_PILL_CLASS.to_string()));
    }
    assert_eq!(strip.children.as_ref().len(), 5 + 2);
}

#[test]
fn a_strip_with_a_tab_that_names_no_icon_stays_on_text_unless_icons_are_asked_for() {
    let mut named = icons(4).into_library_owned_vec();
    named[2] = AzString::from("");
    let named = StringVec::from_vec(named);
    let text = phone(4).with_icons(named.clone()).dom();
    for pill in pills_of(strip_of(&text)) {
        assert!(!classes_of(pill).contains(&ICON_PILL_CLASS.to_string()));
        assert!(matches!(pill.children.as_ref()[0].root.get_node_type(), NodeType::Text(_)));
    }
    // `Icons`: every pill an icon, the placeholder where the name is missing.
    let squares = phone(4).with_icons(named).with_labels(TabLabels::Icons).dom();
    let pills = pills_of(strip_of(&squares));
    assert!(pills.iter().all(|p| classes_of(p).contains(&ICON_PILL_CLASS.to_string())));
    assert!(is_icon(&pills[2].children.as_ref()[0], FALLBACK_TAB_ICON));
    assert!(is_icon(&pills[1].children.as_ref()[0], "folder"));
}

#[test]
fn a_tab_bar_with_no_icons_at_all_is_the_text_it_always_was() {
    let dom = TabHeader::create(names(3))
        .with_style(TabStyle::TouchPills)
        .with_theme(UiTheme::Flat)
        .with_status_bar(band_of(DisplayPreset::Pixel8, 411.0, 914.0))
        .dom();
    for pill in pills_of(strip_of(&dom)) {
        assert!(matches!(pill.children.as_ref()[0].root.get_node_type(), NodeType::Text(_)));
    }
}

#[test]
fn text_is_the_text_even_where_every_tab_has_an_icon() {
    let dom = phone(4).with_labels(TabLabels::Text).dom();
    let pills = pills_of(strip_of(&dom));
    assert_eq!(pills.len(), 4);
    for pill in pills {
        assert!(!classes_of(pill).contains(&ICON_PILL_CLASS.to_string()));
        assert!(matches!(pill.children.as_ref()[0].root.get_node_type(), NodeType::Text(_)));
    }
}

#[test]
fn a_desktop_strip_always_shows_the_text() {
    for labels in [TabLabels::Auto, TabLabels::Icons, TabLabels::Text] {
        let dom = TabHeader::create(names(4))
            .with_icons(icons(4))
            .with_labels(labels)
            .with_style(TabStyle::Pills)
            .with_theme(UiTheme::Flat)
            .dom();
        let mut stack = vec![&dom];
        while let Some(node) = stack.pop() {
            assert!(
                !classes_of(node).contains(&ICON_PILL_CLASS.to_string()),
                "{labels:?}: a desktop pill is never an icon"
            );
            assert!(!matches!(node.root.get_node_type(), NodeType::Icon(_)), "{labels:?}");
            stack.extend(node.children.as_ref().iter());
        }
    }
}

#[test]
fn a_crowd_of_tabs_stays_on_text() {
    let n = crate::widgets::tab_strip::MAX_STRIP_PILLS + 1;
    let dom = phone(n).dom();
    let first = &pills_of(strip_of(&dom))[0];
    assert!(!classes_of(first).contains(&ICON_PILL_CLASS.to_string()));
}

#[test]
fn the_target_of_an_icon_pill_is_the_whole_row_tall_and_at_least_44_wide() {
    for side in [32.0_f32, 36.0, 40.0] {
        let (across, along) = icon_target_pad(side, crate::widgets::tab_strip::TOUCH_ROW);
        assert!(side + 2.0 * across >= crate::widgets::tab_strip::ICON_TARGET_MIN - 0.001, "side {side}");
        assert!(
            (side + 2.0 * along - crate::widgets::tab_strip::TOUCH_ROW).abs() < 0.001,
            "side {side}: the target is the row's height"
        );
    }
}

#[test]
fn the_selected_icon_pill_is_the_primary_button_and_the_others_the_plain_one() {
    // The same two looks a text pill has: the selected tab is the submit button.
    let dom = phone(3).with_active_tab(0).dom();
    let pills = pills_of(strip_of(&dom));
    assert!(classes_of(pills[0]).contains(&TAB_ACTIVE_CLASS_NAME.to_string()));
    assert!(classes_of(pills[1]).contains(&TAB_NOT_ACTIVE_CLASS_NAME.to_string()));
}
