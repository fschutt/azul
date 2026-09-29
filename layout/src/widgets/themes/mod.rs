use azul_css::{impl_option, impl_option_inner};

pub mod flat;
pub mod flora;
pub mod system_palette;
/// Declaration builders the theme modules share (fills, inks, borders,
/// focus rings, each light value paired with its dark twin).
pub(crate) mod decl;

// ==== W3b: shared style builders + the theme marker, and their test helpers ====
pub mod style_kit;
/// Widgets that follow the app theme: every theme's `@theme(<name>)` block
/// in one DOM - the one merge (`follow_app_theme`, `follow_dom`,
/// `follow_props`).
pub(crate) mod theme_blocks;
#[cfg(test)]
pub(crate) mod theme_checks;

/// The visual theme for a widget.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum UiTheme {
    /// A flat, minimal theme (default for many legacy widgets, similar to Office 2013).
    #[default]
    Flat = 0,
    /// The Flora theme, a skeuomorphic rich theme with borders, depth, and shadows.
    Flora = 1,
}

impl UiTheme {
    /// The look of a widget that has no theme option yet - the ribbon, the
    /// status bar, the quick-access bar, the backstage, the node graph: one
    /// look, the flat one. The widgets such a widget builds for itself (its
    /// Buttons, the status bar's zoom Slider, a node's fields) are pinned to
    /// it, so the whole widget renders the same under every app theme. A
    /// widget that gains a theme option passes its own pin down in these
    /// places instead (`None`: they follow the app theme with it).
    pub(crate) const SINGLE_LOOK: Self = Self::Flat;

    /// The app-theme name this widget theme answers to (`@theme(<name>)`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Flora => "flora",
        }
    }

    /// The widget theme an app theme name selects; `None` for a name no
    /// widget theme knows.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "flat" => Some(Self::Flat),
            "flora" => Some(Self::Flora),
            _ => None,
        }
    }

    /// The theme the DOM being built is for - the STRUCTURAL theme of the
    /// window's app theme chain (`azul_css::dynamic_selector::structural_app_theme`:
    /// its first compiled-in theme, so `flora` for `flora:abc`, the one whose
    /// blocks are live) - and `Flat` for a chain no widget theme is in. What
    /// a widget that follows the app theme builds its STRUCTURE for.
    #[must_use]
    pub fn current() -> Self {
        let chain = azul_css::dynamic_selector::app_theme_chain(
            azul_core::app_theme::current_theme().as_str(),
        );
        azul_css::dynamic_selector::structural_app_theme(chain.as_slice())
            .and_then(Self::from_name)
            .unwrap_or_default()
    }
}

impl_option!(
    UiTheme,
    OptionUiTheme,
    copy = false,
    [Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(test)]
mod name_tests {
    use super::UiTheme;

    #[test]
    fn the_default_widget_theme_is_the_default_app_theme() {
        assert_eq!(
            UiTheme::default().name(),
            azul_css::dynamic_selector::DEFAULT_APP_THEME
        );
    }

    #[test]
    fn a_theme_name_round_trips_and_an_unknown_one_is_none() {
        for t in [UiTheme::Flat, UiTheme::Flora] {
            assert_eq!(UiTheme::from_name(t.name()), Some(t));
        }
        assert_eq!(UiTheme::from_name("monokai"), None);
    }

    /// Every widget theme is a complete look, so the theme chain must treat
    /// it as an exclusive floor.
    #[test]
    fn every_widget_theme_is_a_compiled_in_app_theme() {
        for t in [UiTheme::Flat, UiTheme::Flora] {
            assert!(
                azul_css::dynamic_selector::is_compiled_in_app_theme(t.name()),
                "{t:?} is missing from COMPILED_IN_APP_THEMES"
            );
        }
    }
}
