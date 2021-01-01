//!
//! # Supported CSS properties
//!
//! | CSS Key                                            | Value syntax | Description | Example(s) | Parsing function |
//! |----------------------------------------------------|--------------|-------------|------------|------------------|
//! | `border-radius`                                    |              |             |            |                  |
//! | `background`                                       |              |             |            |                  |
//! | `background-color`                                 |              |             |            |                  |
//! | `background-size`                                  |              |             |            |                  |
//! | `background-image`                                 |              |             |            |                  |
//! | `background-position`                              |              |             |            |                  |
//! | `background-repeat`                                |              |             |            |                  |
//! | `color`                                            |              |             |            |                  |
//! | `font-size`                                        |              |             |            |                  |
//! | `font-family`                                      |              |             |            |                  |
//! | `text-align`                                       |              |             |            |                  |
//! | `letter-spacing`                                   |              |             |            |                  |
//! | `line-height`                                      |              |             |            |                  |
//! | `word-spacing`                                     |              |             |            |                  |
//! | `tab-width`                                        |              |             |            |                  |
//! | `cursor`                                           |              |             |            |                  |
//! | `width`, `min-width`, `max-width`                  |              |             |            |                  |
//! | `height`, `min-height`, `max-height`               |              |             |            |                  |
//! | `position`                                         |              |             |            |                  |
//! | `top`, `right`, `left`, `bottom`                   |              |             |            |                  |
//! | `flex-wrap`                                        |              |             |            |                  |
//! | `flex-direction`                                   |              |             |            |                  |
//! | `flex-grow`                                        |              |             |            |                  |
//! | `flex-shrink`                                      |              |             |            |                  |
//! | `justify-content`                                  |              |             |            |                  |
//! | `align-items`                                      |              |             |            |                  |
//! | `align-content`                                    |              |             |            |                  |
//! | `overflow`, `overflow-x`, `overflow-y`             |              |             |            |                  |
//! | `padding`, `-top`, `-left`, `-right`, `-bottom`    |              |             |            |                  |
//! | `margin`,  `-top`, `-left`, `-right`, `-bottom`    |              |             |            |                  |
//! | `border`,  `-top`, `-left`, `-right`, `-bottom`    |              |             |            |                  |
//! | `box-shadow`, `-top`, `-left`, `-right`, `-bottom` |              |             |            |                  |

#[cfg(feature = "css_parser")]
use azul_css_parser::{self, CssParseError};
pub use azul_css::*;
#[cfg(feature = "css_parser")]
pub mod css_parser {
    pub use azul_css_parser::*;
}

/// Returns an empty CSS style
pub fn empty() -> Css {
    Css::empty()
}

/// Parses CSS stylesheet from a string. Convenience wrapper for `azul-css-parser::new_from_str`.
#[cfg(feature = "css_parser")]
pub fn from_str(input: &str) -> Result<Css, CssParseError> {
    azul_css_parser::new_from_str(input)
}
