//! A widget that embeds other widgets wears ONE look inside out.
//!
//! A pinned embedder passes its pin down (`ColorInput` to its hex field,
//! number fields and labels, `DateTimeLocalPicker` to its date and time
//! parts, `FileInput` to its button). A widget with NO theme option has one
//! look today - the ribbon, the status bar, the quick-access bar, the
//! backstage, the node graph - and the widgets it builds for itself (its
//! Buttons, the status bar's zoom Slider, the node graph's fields) are pinned
//! to that look, so the whole widget, the embedded widgets' classes and
//! styles included, is the same under every app theme. When such a widget
//! gains a theme option it passes its own pin down instead
//! (`UiTheme::SINGLE_LOOK` marks the places).

use azul_core::{app_theme::ThemeScope, dom::Dom};
use azul_css::{
    dynamic_selector::{DynamicSelector, ThemeCondition},
    AzString,
};
use azul_layout::widgets::themes::UiTheme;

/// `make()` built for the app theme `theme`, the way a window's layout pass
/// builds its DOM.
fn built_for(theme: UiTheme, make: &dyn Fn() -> Dom) -> Dom {
    let _scope = ThemeScope::enter(AzString::from_const_str(theme.name()));
    make()
}

/// Every node, depth first: its path, classes (the theme markers
/// included), inline style and component sheets.
fn outline(dom: &Dom) -> Vec<String> {
    fn walk(node: &Dom, path: String, out: &mut Vec<String>) {
        out.push(format!(
            "{path} {:?} {:?} {:?}",
            node.root.get_ids_and_classes(),
            node.root.style,
            node.css
        ));
        for (i, child) in node.children.as_ref().iter().enumerate() {
            walk(child, format!("{path}/{i}"), out);
        }
    }
    let mut out = Vec::new();
    walk(dom, String::from("root"), &mut out);
    out
}

/// How many rules of `dom` (inline or in a component sheet) sit in an app
/// theme's block.
fn theme_blocks(dom: &Dom) -> usize {
    let own = core::iter::once(&dom.root.style)
        .chain(dom.css.as_ref().iter())
        .flat_map(|css| css.rules.as_ref().iter())
        .filter(|rule| {
            rule.conditions
                .as_ref()
                .iter()
                .any(|c| matches!(c, DynamicSelector::Theme(ThemeCondition::Custom(_))))
        })
        .count();
    own + dom.children.as_ref().iter().map(theme_blocks).sum::<usize>()
}

fn assert_one_look(what: &str, make: &dyn Fn() -> Dom) {
    let flat = built_for(UiTheme::Flat, make);
    let flora = built_for(UiTheme::Flora, make);
    assert_eq!(
        theme_blocks(&flat),
        0,
        "{what} carries app-theme blocks: something it embeds follows the app theme"
    );
    let (flat, flora) = (outline(&flat), outline(&flora));
    assert_eq!(flat.len(), flora.len(), "{what}: the tree changes with the app theme");
    for (f, fl) in flat.iter().zip(flora.iter()) {
        assert_eq!(f, fl, "{what}: a node changes with the app theme");
    }
}

#[test]
fn a_ribbons_buttons_wear_the_ribbons_one_look() {
    use azul_layout::widgets::ribbon::{
        Ribbon, RibbonButton, RibbonGroup, RibbonItem, RibbonTab, RibbonTabVec,
    };
    assert_one_look("ribbon", &|| {
        Ribbon::new(RibbonTabVec::from_vec(vec![RibbonTab::new(AzString::from(
            "HOME",
        ))
        .with_group(
            RibbonGroup::new(AzString::from("Clipboard")).with_item(RibbonItem::LargeButton(
                RibbonButton::new(AzString::from("content_paste"), AzString::from("Paste")),
            )),
        )]))
        .dom()
    });
}

#[test]
fn a_status_bars_buttons_and_zoom_slider_wear_the_bars_one_look() {
    use azul_layout::widgets::statusbar::{
        StatusBar, StatusBarSegment, StatusBarSegmentVec, StatusBarViewSwitcher, StatusBarZoom,
    };
    assert_one_look("status bar", &|| {
        StatusBar::new(StatusBarSegmentVec::from_vec(vec![StatusBarSegment::new(
            AzString::from("Page 1 of 3"),
        )
        .with_icon(AzString::from("description"))]))
        .with_views(StatusBarViewSwitcher::office_2013())
        .with_zoom(StatusBarZoom::office_2013())
        .dom()
    });
}

#[test]
fn a_quick_access_bars_buttons_wear_the_bars_one_look() {
    use azul_layout::widgets::quick_access::QuickAccessBar;
    assert_one_look("quick-access bar", &|| {
        QuickAccessBar::office_2013(AzString::from("Document1")).dom()
    });
}

#[test]
fn a_backstages_back_button_wears_the_backstages_one_look() {
    use azul_layout::widgets::backstage::{Backstage, BackstageNavItem, BackstageNavItemVec};
    assert_one_look("backstage", &|| {
        Backstage::new(BackstageNavItemVec::from_vec(vec![
            BackstageNavItem::new(AzString::from("Info")),
            BackstageNavItem::new(AzString::from("Save")),
        ]))
        .dom()
    });
}
