//! Flora's `system:` palette: what the `system:` colour keywords stand for
//! while a window's app theme is flora (or one of its spins).
//!
//! The keywords name ROLES - the text, a panel, a field, a rule, the accent,
//! a selection - and the desktop fills them in (`SystemStyle::colors`). Under
//! flora the room is flora's (`doc/templates/flora.css`), so the roles take
//! its tokens instead: an app that paints its own panes and rules with
//! `system:` colours then sits in the same room as the flora widgets around
//! it, in both modes, and recolours with a spin. `flat` keeps the desktop's
//! palette (`LayoutWindow::dynamic_selector_context` asks here only for a
//! flora chain).
//!
//! | keyword                                 | day (flora.css)        | night                  |
//! |-----------------------------------------|------------------------|------------------------|
//! | `text`, `button-text`                   | `--fl-ink`             | `--fl-ink`             |
//! | `secondary-text`                        | `--fl-soft1`           | `--fl-soft1`           |
//! | `tertiary-text`, `placeholder-text`     | `--fl-soft2`           | `--fl-soft2`           |
//! | `disabled-text`                         | `--fl-disTx`           | `--fl-disTx`           |
//! | `window-background`, `sidebar-background` | `--fl-sur` (a leaf)  | `--fl-sur`             |
//! | `background`, `control-background`      | `--fl-fld` (field paper) | `--fl-fld`           |
//! | `under-page-background`                 | `--fl-desk`            | `--fl-desk`            |
//! | `button-face`                           | `--fl-rT` (the lit face) | `--fl-rT`            |
//! | `separator`                             | `--fl-bd`              | `--fl-bd`              |
//! | `grid`                                  | `--fl-sep`             | `--fl-sep`             |
//! | `accent`                                | `--fl-acc` (the stone) | `--fl-glow`            |
//! | `accent-text`                           | `--fl-on-acc`          | `--fl-deep`            |
//! | `selection-background`, `sidebar-selection`, `text-selection-background` | `--fl-soft` | `--fl-acc` |
//! | `selection-text`                        | `--fl-deep`            | `--fl-on-acc`          |
//! | `selection-background-inactive`         | `--fl-sep`             | `--fl-bd`              |
//! | `selection-text-inactive`               | `--fl-ink`             | `--fl-ink`             |
//! | `link`                                  | `--fl-qt` (brass ink)  | `--fl-qt`              |
//! | `find-highlight`                        | `--fl-gla`             | `--fl-gla`             |
//!
//! Two choices the CSS does not spell out for a keyword:
//!
//! * `accent` at night. flora.css keeps the STONE on its commands at night and
//!   lifts only the accent INK - the focus ring - to `--fl-glow`, "so it
//!   survives on a dark ground". A `system:accent` is used as both (an app's
//!   accent-coloured label or icon, a badge's fill), and the stone as ink
//!   reads 1.8:1 on the night leaf. So at night the keyword is the glow (5.1:1
//!   on `--fl-sur`), and `accent-text` the deep tone on it (4:1) - the stone
//!   lit, not a different colour. Flora's own stones (buttons, the checked
//!   box, a selected tab) are the widgets', drawn from the ramp, unchanged.
//! * `find-highlight`: flora has no highlighter, and brass is never a fill
//!   ("metal lives on borders only"); the stone's halo (`--fl-gla`, the glow
//!   at 55 %) marks a match without becoming a selection.

use azul_css::{
    props::basic::color::{ColorU, OptionColorU},
    system::SystemColors,
};

use super::{flora as f, spin::FloraAccentRamp};

/// `--fl-gla`: `rgba(122, 147, 198, 0.55)`, the glow of `ramp` at 55 %.
const fn halo(ramp: FloraAccentRamp) -> ColorU {
    ColorU {
        a: 140,
        ..ramp.glow
    }
}

/// The `system:` palette of the app theme flora in the stone `ramp` (the base
/// blue, or a spin's - `spin::FloraSpin::ramp`), by day or at night (`dark`).
/// Every slot is set: nothing falls back to a keyword's neutral default.
#[must_use]
pub const fn system_colors(dark: bool, ramp: FloraAccentRamp) -> SystemColors {
    const fn some(c: ColorU) -> OptionColorU {
        OptionColorU::Some(c)
    }
    // The tokens that differ by night; the ramp is the same by day and by
    // night ("the accent keeps its stone").
    let (ink, soft1, soft2, dis_tx, sur, fld, desk, face, bd, sep, qt) = if dark {
        (
            f::DARK_INK,
            f::DARK_SOFT1,
            f::DARK_SOFT2,
            f::DARK_DISTX,
            f::DARK_SUR,
            f::DARK_FLD,
            f::DARK_DESK,
            f::DARK_RT,
            f::DARK_BD,
            f::DARK_SEP,
            f::DARK_QT,
        )
    } else {
        (
            f::LIGHT_INK,
            f::LIGHT_SOFT1,
            f::LIGHT_SOFT2,
            f::LIGHT_DISTX,
            f::LIGHT_SUR,
            f::LIGHT_FLD,
            f::LIGHT_DESK,
            f::LIGHT_RT,
            f::LIGHT_BD,
            f::LIGHT_SEP,
            f::LIGHT_QT,
        )
    };
    let (accent, accent_text) = if dark {
        (ramp.glow, ramp.deep)
    } else {
        (ramp.acc, f::LIGHT_ON_ACC)
    };
    let (selection, selection_text, inactive) = if dark {
        (ramp.acc, f::LIGHT_ON_ACC, f::DARK_BD)
    } else {
        (ramp.soft, ramp.deep, f::LIGHT_SEP)
    };
    SystemColors {
        text: some(ink),
        secondary_text: some(soft1),
        tertiary_text: some(soft2),
        background: some(fld),
        accent: some(accent),
        accent_text: some(accent_text),
        button_face: some(face),
        button_text: some(ink),
        disabled_text: some(dis_tx),
        window_background: some(sur),
        under_page_background: some(desk),
        selection_background: some(selection),
        selection_text: some(selection_text),
        selection_background_inactive: some(inactive),
        selection_text_inactive: some(ink),
        link: some(qt),
        separator: some(bd),
        grid: some(sep),
        find_highlight: some(halo(ramp)),
        sidebar_background: some(sur),
        sidebar_selection: some(selection),
        control_background: some(fld),
        placeholder_text: some(soft2),
        text_selection_background: some(selection),
    }
}

/// The `system:` palette a window whose app theme expands to `chain` paints
/// with, by day or at night: flora's in the chain's stone for a flora chain
/// (`spin::FloraSpin::of_chain`), `None` for every other theme - the
/// desktop's palette stands.
#[must_use]
pub fn for_chain<S: AsRef<str>>(chain: &[S], dark: bool) -> Option<SystemColors> {
    super::spin::FloraSpin::of_chain(chain).map(|spin| system_colors(dark, spin.ramp()))
}

#[cfg(test)]
mod tests {
    use azul_css::props::basic::color::SystemColorRef as R;

    use super::*;
    use crate::widgets::themes::spin::FloraSpin;

    /// Every `system:` keyword under flora, by day and at night.
    const ALL: [R; 24] = [
        R::Text,
        R::Background,
        R::Accent,
        R::AccentText,
        R::ButtonFace,
        R::ButtonText,
        R::WindowBackground,
        R::SelectionBackground,
        R::SelectionText,
        R::SecondaryText,
        R::TertiaryText,
        R::DisabledText,
        R::UnderPageBackground,
        R::SelectionBackgroundInactive,
        R::SelectionTextInactive,
        R::Link,
        R::Separator,
        R::Grid,
        R::FindHighlight,
        R::SidebarBackground,
        R::SidebarSelection,
        R::ControlBackground,
        R::PlaceholderText,
        R::TextSelectionBackground,
    ];

    #[test]
    fn every_keyword_has_a_flora_colour_in_both_modes() {
        for dark in [false, true] {
            let palette = system_colors(dark, FloraSpin::Blue.ramp());
            for slot in ALL {
                assert!(
                    slot.get(&palette).into_option().is_some(),
                    "system:{} has no flora colour (dark {dark})",
                    slot.as_css_str()
                );
            }
        }
    }

    #[test]
    fn the_text_keywords_read_on_the_surfaces_they_sit_on() {
        for dark in [false, true] {
            let p = system_colors(dark, FloraSpin::Blue.ramp());
            let get = |slot: R| slot.resolve_for_theme(&p, dark);
            for (ink, ground, floor) in [
                (R::Text, R::WindowBackground, 7.0),
                (R::Text, R::ControlBackground, 7.0),
                (R::SecondaryText, R::WindowBackground, 4.5),
                (R::Link, R::WindowBackground, 4.5),
                (R::SelectionText, R::SelectionBackground, 4.5),
                (R::AccentText, R::Accent, 3.0),
                // The accent as ink (an icon, an accent label).
                (R::Accent, R::WindowBackground, 4.5),
                (R::ButtonText, R::ButtonFace, 7.0),
            ] {
                let ratio = get(ink).contrast_ratio(&get(ground));
                assert!(
                    ratio >= floor,
                    "dark {dark}: system:{} on system:{} reads {ratio:.2}:1, under {floor}:1",
                    ink.as_css_str(),
                    ground.as_css_str()
                );
            }
        }
    }

    #[test]
    fn a_spin_recuts_the_accent_and_the_selection_but_not_the_ground() {
        let blue = system_colors(false, FloraSpin::Blue.ramp());
        for spin in FloraSpin::ALL {
            let ramp = spin.ramp();
            for dark in [false, true] {
                let p = system_colors(dark, ramp);
                let want_accent = if dark { ramp.glow } else { ramp.acc };
                assert_eq!(p.accent.into_option(), Some(want_accent), "{spin:?} dark {dark}");
                let want_selection = if dark { ramp.acc } else { ramp.soft };
                assert_eq!(p.selection_background.into_option(), Some(want_selection));
            }
            let p = system_colors(false, ramp);
            assert_eq!(p.window_background, blue.window_background, "{spin:?}: the ground stays");
            assert_eq!(p.text, blue.text);
            assert_eq!(p.link, blue.link, "{spin:?}: the brass is not the stone");
        }
    }

    #[test]
    fn only_a_flora_chain_has_a_flora_palette() {
        assert_eq!(
            for_chain(&["flora", "flat"], false),
            Some(system_colors(false, FloraSpin::Blue.ramp()))
        );
        assert_eq!(
            for_chain(&["flora:red", "flora", "flat"], true),
            Some(system_colors(true, FloraSpin::Red.ramp()))
        );
        assert_eq!(for_chain(&["flat"], false), None);
        assert_eq!(for_chain::<&str>(&[], true), None);
    }

    #[test]
    fn the_night_ground_is_neutral_grey() {
        // flora.css: "untinted gray in dark", never the warm brown.
        let p = system_colors(true, FloraSpin::Blue.ramp());
        for slot in [R::WindowBackground, R::ControlBackground, R::UnderPageBackground] {
            let c = slot.get(&p).into_option().expect("set");
            assert!(c.r == c.g && c.g == c.b, "system:{} at night is {c:?}", slot.as_css_str());
        }
    }
}
