use azul_css::{impl_option, impl_option_inner};

pub mod flat;
pub mod flora;

/// The visual theme for a widget.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UiTheme {
    /// A flat, minimal theme (default for many legacy widgets, similar to Office 2013).
    Flat = 0,
    /// The Flora theme, a skeuomorphic rich theme with borders, depth, and shadows.
    Flora = 1,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self::Flat
    }
}

impl_option!(
    UiTheme,
    OptionUiTheme,
    copy = false,
    [Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash]
);
