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

use std::time::Instant;
#[cfg(debug_assertions)]
use std::time::Duration;
#[cfg(debug_assertions)]
use std::path::PathBuf;
#[cfg(feature = "css_parser")]
use azul_css_parser::{self, CssParseError};
pub use azul_css::*;
#[cfg(feature = "css_parser")]
pub mod css_parser {
    pub use azul_css_parser::*;
}
#[cfg(feature = "native_style")]
pub mod native_style {
    pub use azul_native_style::*;
}

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

/// Reload the CSS if enough time has passed since the last reload
#[cfg(debug_assertions)]
pub(crate) fn hot_reload_css(
    css: &mut Css,
    hot_reload_handler: Option<&Box<dyn HotReloadHandler>>,
    last_style_reload: &mut Instant,
    force_reload: bool,
) -> Result<bool, String> {

    let mut has_reloaded = false;
    let now = Instant::now();

    let hot_reload_handler = match hot_reload_handler {
        Some(s) => s,
        None => return Ok(has_reloaded),
    };

    let reload_interval = hot_reload_handler.get_reload_interval();
    let should_reload = force_reload || now - *last_style_reload > reload_interval;

    if should_reload {
        let new_css = hot_reload_handler.reload_style()?;
        *css = new_css;
        has_reloaded = true;
        *last_style_reload = now;
    }

    Ok(has_reloaded)
}