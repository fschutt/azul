//! Traits and datatypes associated with reloading styles at runtime.

use crate::css::Css;
use std::time::Duration;

/// Interface that can be used to reload a stylesheet while an application is running.
/// Initialize the `Window::new` with a `Box<HotReloadHandle>` - this allows the hot-reloading
/// to be independent of the source format, making CSS only one frontend format.
///
/// You can, for example, parse and load styles directly from a SASS, LESS or JSON parser.
/// The default parser is `azul-css-parser`.
pub trait HotReloadHandler {
    /// Reloads the style from the source format. Should return Ok() when the CSS has be correctly
    /// reloaded, and an human-readable error string otherwise (since the error needs to be printed
    /// to stdout when hot-reloading).
    fn reload_style(&mut self) -> Result<Css, String>;
    /// Returns how quickly the hot-reloader should reload the source format.
    fn get_reload_interval(&self) -> Duration;
}

/// Custom hot-reloader combinator that can be used to merge hot-reloaded styles onto a base style.
/// Can be useful when working from a base configuration, such as the OS-native styles.
pub struct HotReloadOverrideHandler {
    /// The base style, usually provided by `azul-native-style`.
    pub base_style: Css,
    /// The style that will be added on top of the `base_style`.
    pub hot_reloader: Box<dyn HotReloadHandler>,
}

impl HotReloadOverrideHandler {
    /// Creates a new `HotReloadHandler` that merges styles onto the given base style
    /// (usually the system-native style, in order to let the user override properties).
    pub fn new(base_style: Css, hot_reloader: Box<dyn HotReloadHandler>) -> Self {
        Self {
            base_style,
            hot_reloader,
        }
    }
}

impl HotReloadHandler for HotReloadOverrideHandler {
    fn reload_style(&mut self) -> Result<Css, String> {
        let mut css = Css::new();
        for stylesheet in self.base_style.clone().stylesheets {
            css.append_stylesheet(stylesheet);
        }
        for stylesheet in self.hot_reloader.reload_style()?.stylesheets {
            css.append_stylesheet(stylesheet);
        }
        Ok(css)
    }

    fn get_reload_interval(&self) -> Duration {
        self.hot_reloader.get_reload_interval()
    }
}
