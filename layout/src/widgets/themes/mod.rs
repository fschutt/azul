use azul_css::{impl_option, impl_option_inner};

pub mod flat;
pub mod flora;
pub mod system_palette;

// ==== W3b: shared style builders + the theme marker, and their test helpers ====
pub mod style_kit;
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

impl_option!(
    UiTheme,
    OptionUiTheme,
    copy = false,
    [Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash]
);
