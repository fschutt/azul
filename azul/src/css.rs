//!
//! # Supported CSS properties
//!
//! | CSS Key                                            | Value syntax | Description | Example(s) | Parsing function |
//! |----------------------------------------------------|--------------|-------------|------------|------------------|
//! | `border-radius`                                    |              |             |            |                  |
//! | `background`                                       |              |             |            |                  |
//! | `background-color`                                 |              |             |            |                  |
//! | `background-size`                                  |              |             |            |                  |
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

#[cfg(debug_assertions)]
use std::time::Duration;
#[cfg(debug_assertions)]
use std::path::PathBuf;

pub use azul_css::*;
#[cfg(feature = "css_parser")]
pub mod css_parser {
    pub use azul_css_parser::*;
}

#[cfg(feature = "css_parser")]
pub use azul_css_parser::CssColor;

#[cfg(feature = "native_style")]
pub mod native_style {
    pub use azul_native_style::*;
}

#[cfg(feature = "css_parser")]
use azul_css_parser::{self, CssParseError};

/// Returns a style with the native appearance for the operating system. Convenience wrapper
/// for functionality from the the `azul-native-style` crate.
#[cfg(feature = "native_style")]
pub fn native() -> Css {
    azul_native_style::native()
}

/// Parses CSS stylesheet from a string. Convenience wrapper for `azul-css-parser::new_from_str`.
#[cfg(feature = "css_parser")]
pub fn from_str(input: &str) -> Result<Css, CssParseError> {
    azul_css_parser::new_from_str(input)
}

/// Appends a custom stylesheet to `css::native()`.
#[cfg(all(feature = "css_parser", feature = "native_style"))]
pub fn override_native(input: &str) -> Result<Css, CssParseError> {
    let mut css = native();
    css.append(from_str(input)?);
    Ok(css)
}

/// Allows dynamic reloading of a CSS file during an applications runtime, useful for
/// changing the look & feel while the application is running.
#[cfg(all(debug_assertions, feature = "css_parser"))]
pub fn hot_reload<P: Into<PathBuf>>(file_path: P, reload_interval: Duration) -> Box<dyn HotReloadHandler> {
    Box::new(azul_css_parser::HotReloader::new(file_path).with_reload_interval(reload_interval))
}

/// Same as `Self::hot_reload`, but appends the given file to the
/// `Self::native()` style before the hot-reloaded styles, similar to `override_native`.
#[cfg(all(debug_assertions, feature = "css_parser", feature = "native_style"))]
pub fn hot_reload_override_native<P: Into<PathBuf>>(file_path: P, reload_interval: Duration) -> Box<dyn HotReloadHandler> {
    Box::new(HotReloadOverrideHandler::new(native(), hot_reload(file_path, reload_interval)))
}

