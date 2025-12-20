//! Re-exports for CSS properties

use azul_css::parser2::{self, CssParseError};
pub use azul_css::*;
pub mod css_parser {
    pub use azul_css::parser2::*;
}

// Re-export the actual Css type
pub use azul_css::css::Css;
