//! The ribbon's touch pills: the tab row as a scrolling row of 32 px pills with a padded target, the
//! status bar's band around the cutouts, and the flip of the content band.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    display_features::{DisplayFeatures, DisplayPreset},
    dom::{Dom, IdOrClass::Class},
    geom::LogicalSize,
};
use azul_css::AzString;

use super::*;
use crate::widgets::{tab_flip::TabFlip, tab_strip::StatusBarBand, tabs::TabStyle};

const TAB_BAR: &str = "__azul-native-ribbon-tabbar";
const CONTENT: &str = "__azul-native-ribbon-content";
const TAB: &str = "__azul-native-ribbon-tab";

fn has_class(node: &Dom, name: &str) -> bool {
    node.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| matches!(c, Class(s) if s.as_str() == name))
}

fn tabs(n: usize) -> RibbonTabVec {
    RibbonTabVec::from_vec(
        (0..n)
            .map(|i| RibbonTab::new(AzString::from(format!("TAB {i}"))))
            .collect(),
    )
}

extern "C" fn on_tab(_: RefAny, _: CallbackInfo, _: usize) -> Update {
    Update::DoNothing
}

fn touch(n: usize) -> Ribbon {
    Ribbon::new(tabs(n))
        .with_tab_style(TabStyle::TouchPills)
        .with_theme(UiTheme::Flat)
}

fn pixel_8_band() -> StatusBarBand {
    let size = LogicalSize {
        width: 360.0,
        height: 720.0,
    };
    StatusBarBand::from_display(
        &DisplayFeatures::simulate(DisplayPreset::Pixel8, size),
        &DisplayPreset::Pixel8.safe_area(size),
        size,
    )
}

fn child_with<'a>(dom: &'a Dom, class: &str) -> &'a Dom {
    dom.children
        .as_ref()
        .iter()
        .find(|c| has_class(c, class))
        .unwrap_or_else(|| panic!("a child with the class {class}"))
}

/// The tabs of a row: its children with the tab class.
fn pills_of(bar: &Dom) -> Vec<&Dom> {
    bar.children
        .as_ref()
        .iter()
        .filter(|c| has_class(c, TAB))
        .collect()
}

#[test]
fn the_touch_pills_are_a_row_of_tabs_with_a_padded_target_and_the_selected_one_a_marker() {
    let dom = touch(4).with_active_tab(1).dom_desktop();
    let bar = child_with(&dom, TAB_BAR);
    let pills = pills_of(bar);
    assert_eq!(pills.len(), 4);
    for (i, pill) in pills.iter().enumerate() {
        // The label, the hit area, and for the selected tab the marker.
        assert_eq!(pill.children.as_ref().len(), if i == 1 { 3 } else { 2 }, "pill {i}");
    }
}

#[test]
fn the_other_looks_are_built_as_they_were() {
    for style in [TabStyle::Pills, TabStyle::Rectangular, TabStyle::Aurora] {
        let dom = Ribbon::new(tabs(3))
            .with_tab_style(style)
            .with_theme(UiTheme::Flat)
            .dom_desktop();
        let bar = child_with(&dom, TAB_BAR);
        for pill in pills_of(bar) {
            assert!(
                pill.children.as_ref().len() <= 1 + 2,
                "{style:?}: a label, at most the curves of a tab"
            );
        }
    }
}

#[test]
fn in_the_status_bars_band_the_row_sits_in_a_header_with_a_cover_over_the_camera() {
    let dom = touch(5).with_status_bar(pixel_8_band()).dom_desktop();
    // The first child of the ribbon is the header of the band: the row, and one cover for the one gap.
    let header = &dom.children.as_ref()[0];
    assert!(!has_class(header, TAB_BAR), "the row is inside the header");
    assert_eq!(header.children.as_ref().len(), 2);
    assert!(has_class(&header.children.as_ref()[0], TAB_BAR));
}

#[test]
fn without_a_band_the_row_is_the_first_child() {
    let dom = touch(5).dom_desktop();
    assert!(has_class(&dom.children.as_ref()[0], TAB_BAR));
}

#[test]
fn a_window_without_a_cutout_or_a_status_bar_has_no_band_to_go_into() {
    let size = LogicalSize {
        width: 900.0,
        height: 700.0,
    };
    let band = StatusBarBand::from_display(
        &DisplayFeatures::none(),
        &azul_css::system::SafeAreaInsets::default(),
        size,
    );
    let dom = touch(3).with_status_bar(band).dom_desktop();
    assert!(has_class(&dom.children.as_ref()[0], TAB_BAR));
}

#[test]
fn the_content_band_flips_with_a_vertical_swipe_when_somebody_is_told() {
    let flipping = touch(3)
        .with_active_tab(1)
        .with_on_tab_click(RefAny::new(1u8), on_tab as RibbonOnTabClickCallbackType)
        .dom_desktop();
    let band = child_with(&flipping, CONTENT);
    // Touch start, move, end, cancel, and the wheel.
    assert_eq!(band.root.get_callbacks().as_ref().len(), 5);
    assert!(band.root.get_dataset().is_some());
    // The chips of the neighbours: tab 0 before, tab 2 after.
    assert!(band.children.as_ref().len() >= 2);
}

#[test]
fn the_band_does_not_flip_without_somebody_to_tell_or_when_off() {
    let silent = touch(3).dom_desktop();
    assert!(child_with(&silent, CONTENT).root.get_callbacks().as_ref().is_empty());
    let off = touch(3)
        .with_on_tab_click(RefAny::new(1u8), on_tab as RibbonOnTabClickCallbackType)
        .with_flip(TabFlip::Off)
        .dom_desktop();
    assert!(child_with(&off, CONTENT).root.get_callbacks().as_ref().is_empty());
    // The desktop pills never flip: the flip is the touch strip's.
    let pills = Ribbon::new(tabs(3))
        .with_tab_style(TabStyle::Pills)
        .with_theme(UiTheme::Flat)
        .with_on_tab_click(RefAny::new(1u8), on_tab as RibbonOnTabClickCallbackType)
        .dom_desktop();
    assert!(child_with(&pills, CONTENT).root.get_callbacks().as_ref().is_empty());
}

#[test]
fn a_ribbon_asks_for_nothing_by_default() {
    let ribbon = Ribbon::new(tabs(2));
    assert!(ribbon.status_bar.is_none());
    assert_eq!(ribbon.flip, TabFlip::AtEdges);
}
