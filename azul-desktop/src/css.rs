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
use azul_core::window::HotReloadOptions;
use std::time::Instant;
pub use azul_css::*;
#[cfg(feature = "css_parser")]
pub mod css_parser {
    pub use azul_css_parser::*;
}
#[cfg(feature = "native_style")]
pub mod native_style {
    pub use azul_native_style::*;
}

/// Returns an empty CSS style
pub fn empty() -> Css {
    Css::empty()
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

/// Reload the CSS if enough time has passed since the last reload
#[cfg(all(feature = "css_parser", feature = "native_style"))]
pub fn hot_reload_css(
    css: &mut Css,
    hot_reload_options: Option<&HotReloadOptions>,
    last_style_reload: &mut Instant,
    force_reload: bool,
) -> Result<bool, String> {

    use std::time::Duration;
    use crate::css;

    let mut has_reloaded = false;
    let now = Instant::now();

    let hot_reload_options = match hot_reload_options {
        Some(s) => s,
        None => return Ok(has_reloaded),
    };

    let reload_interval: Duration = hot_reload_options.reload_interval.into();
    let should_reload = force_reload || now - *last_style_reload > reload_interval;

    if should_reload {

        let mut new_css = Css::empty();

        if hot_reload_options.apply_native_css {
            let mut native_css = css::native();
            native_css.sort_by_specificity();
            new_css.append_css(native_css);
        }

        let loaded_css = std::fs::read_to_string(hot_reload_options.path.as_str())
            .map_err(|e| format!("error reading \"{}\": {}", hot_reload_options.path.as_str(), e))?;
        let mut parsed_css = css::from_str(&loaded_css)
            .map_err(|e| format!("error in CSS file \"{}\": {}", hot_reload_options.path.as_str(), e))?;
        parsed_css.sort_by_specificity();
        new_css.append_css(parsed_css);

        *css = new_css;
        has_reloaded = true;
        *last_style_reload = now;
    }

    Ok(has_reloaded)
}