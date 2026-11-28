//! The bottom tabs on a narrow phone (a window narrower than [`NARROW_MAX_PX`]): a tab that has an
//! icon is its icon (its label hides); a tab that has none IS its label, so the rule leaves it
//! alone (hiding it would leave an empty cell: AzClock's four places are words only).

use super::*;
use crate::widgets::{
    shells::fixtures::slot,
    themes::{theme_checks as tc, UiTheme},
};

/// Whether the label `dom` has the narrow-phone rule: `display: none` under the narrow width.
fn hides_when_narrow(label: &Dom) -> bool {
    label.root.style.iter_inline_properties().any(|(p, c)| {
        let rule: &[DynamicSelector] = c.as_ref();
        matches!(p, CssProperty::Display(_))
            && rule == [DynamicSelector::ViewportWidth(MinMaxRange::with_max(NARROW_MAX_PX))]
    })
}

fn bar(tabs: Vec<ShellBottomTab>, theme: UiTheme) -> Dom {
    MobileShell::create(AzString::from("Clock"))
        .with_page(slot())
        .with_tabs(ShellBottomTabVec::from_vec(tabs))
        .with_theme(theme)
        .dom()
}

#[test]
fn a_tab_with_an_icon_hides_its_label_on_a_narrow_phone_and_a_tab_without_one_keeps_it() {
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        let dom = bar(
            vec![
                ShellBottomTab::create(AzString::from("Alarm"), AzString::from("alarm")),
                ShellBottomTab::create(AzString::from("Stopwatch"), AzString::from("")),
            ],
            theme,
        );
        let labels = tc::find_all(&dom, TAB_LABEL_CLASS);
        assert_eq!(labels.len(), 2, "{theme:?}: one label a tab");
        assert!(hides_when_narrow(labels[0]), "{theme:?}: the tab with an icon hides its label");
        assert!(
            !hides_when_narrow(labels[1]),
            "{theme:?}: a tab of words only keeps them, or it is an empty cell"
        );
    }
}

#[test]
fn a_bar_of_words_only_shows_every_word_on_a_narrow_phone() {
    let words = ["Alarm", "Clock", "Timer", "Stopwatch"];
    let dom = bar(
        words
            .iter()
            .map(|w| ShellBottomTab::create(AzString::from(*w), AzString::from("")))
            .collect(),
        UiTheme::Flat,
    );
    let labels = tc::find_all(&dom, TAB_LABEL_CLASS);
    assert_eq!(labels.len(), words.len());
    assert!(labels.iter().all(|l| !hides_when_narrow(l)));
}
