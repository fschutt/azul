//! Re-exports CSS property types (`azul_css::*`), the `Css` stylesheet type,
//! and the CSS parser (`css_parser` submodule) for the desktop module.
pub use azul_css::*;
pub mod css_parser {
    pub use azul_css::parser2::*;
}

// Re-export the actual Css type
pub use azul_css::css::Css;
