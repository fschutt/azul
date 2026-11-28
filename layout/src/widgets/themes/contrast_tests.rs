//! Increased contrast over the widgets of BOTH themes, in the light and the dark mode: every text of a page of
//! built-in widgets is read at 7:1 on the surface behind it, and every visible border is seen at 4.5:1 on what lies
//! behind the box - through the same getters the painters read, so a widget whose colour does not reach a painter by them
//! (a hard-coded paint) fails here by name.
//!
//! The numbers are `azul_css::presentation`; this is what proves that the five getters really are the only way a
//! colour reaches the screen.

use azul_core::{
    dom::{Dom, NodeType},
    id::NodeId,
    styled_dom::StyledDom,
};
use azul_css::{
    accessibility::{AccessibilityPreferences, ContrastPreference},
    css::CssPropertyValue,
    dynamic_selector::DynamicSelectorContext,
    presentation::{MORE_CONTRAST_EDGE, MORE_CONTRAST_TEXT},
    props::basic::color::ColorU,
};

use crate::{
    solver3::{
        getters::{get_border_info, get_used_text_color},
        presentation::{surface_at, surface_under},
    },
    widgets::{
        alert::Alert,
        button::Button,
        check_box::CheckBox,
        chip::Chip,
        info_bar::InfoBar,
        text_input::TextInput,
        themes::{
            theme_blocks::checks::{under, BOTH},
            UiTheme,
        },
    },
};

/// A page of widgets as `theme` builds them.
fn page(theme: UiTheme) -> Dom {
    under(theme, || {
        Dom::create_body().with_children(
            vec![
                Button::create("Save".into()).dom(),
                CheckBox::create(true).dom(),
                TextInput::create().with_placeholder("Name".into()).dom(),
                Chip::create("Tag".into()).dom(),
                Alert::create("Something happened".into()).dom(),
                InfoBar::create("A note".into()).dom(),
            ]
            .into(),
        )
    })
}

/// The page, styled in a window that asked for increased contrast (or for nothing).
fn styled(theme: UiTheme, dark: bool, more_contrast: bool) -> StyledDom {
    let mut preferences = AccessibilityPreferences::default();
    if more_contrast {
        preferences.contrast = ContrastPreference::More;
    }
    let mut context = DynamicSelectorContext {
        preferences,
        ..DynamicSelectorContext::default()
    };
    if dark {
        context.mode = azul_css::system::DarkLightMode::Dark;
    }
    StyledDom::create_from_dom_with_context(page(theme), Some(context.with_app_theme(theme.name())))
}

/// The best contrast an extreme ink reaches on `surface`: what a mid-tone surface is limited to.
fn best_possible(surface: &ColorU) -> f32 {
    surface
        .contrast_ratio(&ColorU::WHITE)
        .max(surface.contrast_ratio(&ColorU::BLACK))
}

#[test]
fn every_text_of_the_widgets_is_read_at_seven_to_one_under_increased_contrast() {
    let mut texts = 0;
    for theme in BOTH {
        for dark in [false, true] {
            let sd = styled(theme, dark, true);
            for index in 0..sd.node_data.len() {
                let node = NodeId::new(index);
                if !matches!(sd.node_data.as_container()[node].node_type, NodeType::Text(_)) {
                    continue;
                }
                let state = sd.get_styled_node_state(&node);
                let ink = get_used_text_color(&sd, node, &state);
                if ink.a == 0 {
                    continue; // hidden on purpose
                }
                texts += 1;
                let surface = surface_at(&sd, node);
                let ratio = ink.contrast_ratio(&surface);
                let wanted = MORE_CONTRAST_TEXT.min(best_possible(&surface));
                assert!(
                    ratio >= wanted - 0.01,
                    "{} {}: text node {index} is {ink:?} on {surface:?}: {ratio:.2}:1 (wanted {wanted:.2})",
                    theme.name(),
                    if dark { "dark" } else { "light" },
                );
            }
        }
    }
    assert!(texts >= 12, "the page has text to look at: {texts}");
}

#[test]
fn every_visible_border_of_the_widgets_is_seen_at_four_and_a_half_to_one_under_increased_contrast() {
    let mut borders = 0;
    for theme in BOTH {
        for dark in [false, true] {
            let sd = styled(theme, dark, true);
            for index in 0..sd.node_data.len() {
                let node = NodeId::new(index);
                let state = sd.get_styled_node_state(&node);
                let info = get_border_info(&sd, node, &state);
                for (side, width, colour) in [
                    ("top", info.widths.top, info.colors.top),
                    ("left", info.widths.left, info.colors.left),
                ] {
                    let width = width
                        .and_then(|w| w.get_property().map(|w| w.inner.to_pixels_internal(0.0, 16.0, 16.0)))
                        .unwrap_or(0.0);
                    let Some(CssPropertyValue::Exact(colour)) = colour else {
                        continue;
                    };
                    if width <= 0.0 || colour.inner.a == 0 {
                        continue;
                    }
                    borders += 1;
                    let behind = surface_under(&sd, node);
                    let ratio = colour.inner.contrast_ratio(&behind);
                    let wanted = MORE_CONTRAST_EDGE.min(best_possible(&behind));
                    assert!(
                        ratio >= wanted - 0.01,
                        "{} {}: the {side} border of node {index} is {:?} on {behind:?}: {ratio:.2}:1 (wanted {wanted:.2})",
                        theme.name(),
                        if dark { "dark" } else { "light" },
                        colour.inner,
                    );
                }
            }
        }
    }
    assert!(borders >= 8, "the page has borders to look at: {borders}");
}

/// Without the setting the same page is what the themes wrote: nothing is moved by the presentation layer.
#[test]
fn without_the_setting_the_widgets_are_painted_as_their_themes_wrote_them() {
    for theme in BOTH {
        for dark in [false, true] {
            let plain = styled(theme, dark, false);
            assert!(
                !crate::solver3::presentation::Presentation::of(&plain).is_active(),
                "{} {}",
                theme.name(),
                if dark { "dark" } else { "light" }
            );
        }
    }
}
