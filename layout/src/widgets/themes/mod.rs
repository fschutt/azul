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
/// in one DOM (T2/T3 migration).
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

    /// The theme the DOM being built is for - the window's app theme
    /// (`azul_core::app_theme::current_theme`) - and `Flat` for a name no
    /// widget theme knows. What a widget that follows the app theme builds
    /// its STRUCTURE for.
    #[must_use]
    pub fn current() -> Self {
        Self::from_name(azul_core::app_theme::current_theme().as_str()).unwrap_or_default()
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
}
