//! Re-exports for CSS properties

#[cfg(feature = "css_parser")]
use azul_css::parser2::{self, CssParseError};
pub use azul_css::*;
#[cfg(feature = "css_parser")]
pub mod css_parser {
    pub use azul_css::parser2::*;
}

// azul_css::Css and azul_css::parser2::CssApiWrapper
// have the exact same binary layout. However, we
// don't want the azul_css crate to depend on a CSS parser
// which requires this workaround for static linking.
pub use azul_css::parser2::CssApiWrapper as Css;
